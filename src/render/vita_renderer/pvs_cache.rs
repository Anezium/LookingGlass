//! Private, portable PVS cache. The key hashes every input used by list compilation.
//! No pointers, GPU IDs, textures or target-sized integers enter the file.
use super::{config, Group, Lists, Pvs};
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
const MAGIC: &[u8; 8] = b"LGPVS003";
const MAX_BYTES: usize = 16 * 1024 * 1024;
#[cfg(target_os = "vita")]
const ROOT: &str = "ux0:data/alice/private/renderer-cache";
#[cfg(not(target_os = "vita"))]
const ROOT: &str = "private/renderer-cache";
fn key(groups: &[Group], visibility: &crate::visibility::Visibility) -> String {
    let mut hash = Sha256::new();
    hash.update(MAGIC);
    visibility.retained_cache_key(&mut hash);
    hash.update((groups.len() as u64).to_le_bytes());
    for g in groups {
        hash.update([
            u8::from(g.recipe.opaque),
            u8::from(g.recipe.model.is_some()),
            u8::from(g.recipe.motion.is_some()),
            u8::from(g.recipe.deform.is_some()),
        ]);
        for n in [g.recipe.rank, g.recipe.blend, g.recipe.cull] {
            hash.update((n as u64).to_le_bytes());
        }
        for n in g.min.to_array().into_iter().chain(g.max.to_array()) {
            hash.update(n.to_bits().to_le_bytes());
        }
        hash.update((g.faces.len() as u64).to_le_bytes());
        for &face in &g.faces {
            hash.update((face as u64).to_le_bytes());
        }
    }
    format!("{:x}", hash.finalize())
}
fn encode(pvs: &Pvs, groups: usize) -> Result<Vec<u8>> {
    let mut pool = vec![&pvs.unknown];
    let mut ids = BTreeMap::from([(Arc::as_ptr(&pvs.unknown) as usize, 0u32)]);
    let mut clusters = Vec::new();
    for list in &pvs.clusters {
        let n = ids.entry(Arc::as_ptr(list) as usize).or_insert_with(|| {
            let n = pool.len() as u32;
            pool.push(list);
            n
        });
        clusters.push(*n);
    }
    let mut body = Vec::new();
    let mut put = |n: usize| -> Result<()> {
        body.extend(u32::try_from(n)?.to_le_bytes());
        Ok(())
    };
    put(groups)?;
    put(clusters.len())?;
    put(pool.len())?;
    for id in clusters {
        put(id as usize)?;
    }
    for list in pool {
        for items in [&list.opaque, &list.alpha] {
            put(items.len())?;
            for &id in items {
                put(id)?;
            }
        }
    }
    ensure!(body.len() + 40 <= MAX_BYTES, "PVS cache exceeds 16 MiB");
    let mut data = Vec::with_capacity(body.len() + 40);
    data.extend(MAGIC);
    data.extend(Sha256::digest(&body));
    data.extend(body);
    Ok(data)
}
fn decode(bytes: &[u8], groups: usize, clusters: usize) -> Result<Pvs> {
    ensure!(
        bytes.len() >= 40 && bytes.len() <= MAX_BYTES && &bytes[..8] == MAGIC,
        "invalid PVS cache header/size"
    );
    let body = &bytes[40..];
    ensure!(
        Sha256::digest(body).as_slice() == &bytes[8..40],
        "PVS cache checksum mismatch"
    );
    let mut cursor = 0;
    let mut get = || -> Result<usize> {
        let value = u32::from_le_bytes(
            body.get(cursor..cursor + 4)
                .context("truncated PVS cache")?
                .try_into()?,
        );
        cursor += 4;
        Ok(value as usize)
    };
    ensure!(
        get()? == groups && get()? == clusters,
        "PVS cache input counts changed"
    );
    let count = get()?;
    ensure!(
        count > 0 && count <= clusters + 1,
        "invalid PVS unique count"
    );
    let cluster_ids = (0..clusters)
        .map(|_| {
            let id = get()?;
            ensure!(id < count, "invalid PVS list ID");
            Ok(id)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut pool = Vec::with_capacity(count);
    for _ in 0..count {
        let mut seen = vec![false; groups];
        let mut list = || -> Result<Vec<usize>> {
            let len = get()?;
            ensure!(len <= groups, "invalid PVS group count");
            let mut result = Vec::with_capacity(len);
            for _ in 0..len {
                let id = get()?;
                ensure!(id < groups && !seen[id], "invalid/duplicate PVS group ID");
                seen[id] = true;
                result.push(id);
            }
            Ok(result)
        };
        let opaque = list()?;
        let alpha = list()?;
        pool.push(Arc::new(Lists { opaque, alpha }));
    }
    ensure!(cursor == body.len(), "trailing PVS cache data");
    Ok(Pvs {
        unknown: pool[0].clone(),
        clusters: cluster_ids.into_iter().map(|id| pool[id].clone()).collect(),
    })
}
pub(super) fn load_or_build(
    groups: &[Group],
    visibility: &crate::visibility::Visibility,
) -> (Pvs, &'static str) {
    if !config().pvs_cache {
        return (Pvs::new(groups, visibility), "disabled");
    }
    let path = PathBuf::from(ROOT).join(format!("pvs-{}.bin", key(groups, visibility)));
    let mut invalid = false;
    let read = (|| -> Result<Option<Pvs>> {
        let metadata = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        ensure!(
            metadata.len() <= MAX_BYTES as u64,
            "PVS cache file exceeds bound"
        );
        let bytes = std::fs::read(&path)?;
        Ok(Some(decode(
            &bytes,
            groups.len(),
            visibility.cluster_count(),
        )?))
    })();
    match read {
        Ok(Some(pvs)) => {
            println!("[lg/vita-renderer] PVS cache hit {}", path.display());
            return (pvs, "hit");
        }
        Ok(None) => {}
        Err(e) => {
            invalid = true;
            eprintln!(
                "[lg/vita-renderer] PVS cache ignored {}: {e:#}",
                path.display()
            );
        }
    }
    let pvs = Pvs::new(groups, visibility);
    let write = (|| -> Result<()> {
        let data = encode(&pvs, groups.len())?;
        std::fs::create_dir_all(ROOT)?;
        let tmp = path.with_extension("bin.tmp");
        std::fs::write(&tmp, data)?;
        std::fs::rename(tmp, &path)?;
        Ok(())
    })();
    match write {
        Ok(()) => {
            println!("[lg/vita-renderer] PVS cache stored {}", path.display());
            (
                pvs,
                if invalid {
                    "invalid-rebuilt"
                } else {
                    "miss-stored"
                },
            )
        }
        Err(e) => {
            eprintln!("[lg/vita-renderer] PVS cache not stored: {e:#}");
            (pvs, "miss-not-stored")
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_lists_preserve_unknown_shared_clusters_and_order() {
        let pvs = Pvs::compile(3, |c| Lists {
            opaque: if c == Some(1) { vec![1] } else { vec![0, 1] },
            alpha: vec![2],
        });
        let bytes = encode(&pvs, 3).unwrap();
        let decoded = decode(&bytes, 3, 3).unwrap();
        for cluster in [None, Some(0), Some(1), Some(2), Some(999)] {
            assert!(pvs.get(cluster) == decoded.get(cluster));
        }
        assert!(Arc::ptr_eq(&decoded.clusters[0], &decoded.clusters[2]));
        assert!(decode(&bytes, 4, 3).is_err());
        assert!(decode(&bytes, 3, 4).is_err());
    }
    #[test]
    fn damaged_truncated_or_invalid_cached_ranges_fail_closed() {
        let pvs = Pvs::compile(1, |_| Lists {
            opaque: vec![0],
            alpha: vec![],
        });
        let bytes = encode(&pvs, 1).unwrap();
        for size in [0, 8, 40, bytes.len() - 1] {
            assert!(decode(&bytes[..size], 1, 1).is_err());
        }
        let mut corrupt = bytes.clone();
        *corrupt.last_mut().unwrap() ^= 1;
        assert!(decode(&corrupt, 1, 1).is_err());
        // Last opaque ID is beyond the known renderer group count, with a valid checksum.
        let mut invalid = bytes.clone();
        invalid[60..64].copy_from_slice(&1u32.to_le_bytes());
        let checksum = Sha256::digest(&invalid[40..]);
        invalid[8..40].copy_from_slice(&checksum);
        assert!(decode(&invalid, 1, 1).is_err());
    }
}
