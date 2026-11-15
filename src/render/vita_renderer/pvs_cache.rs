//! Private, portable PVS cache. The key hashes every input used by list compilation.
//! No pointers, GPU IDs, textures or target-sized integers enter the file.
use super::{config, Group, Lists, Pvs};
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::{Path, PathBuf}, sync::Arc};
const MAGIC: &[u8; 8] = b"LGPVS005";
const MAX_BYTES: usize = 16 * 1024 * 1024;
/// Hard bound for the whole cache; least recently written maps are evicted past it.
/// All 36 shipping maps together measured 104.3 MiB, so this keeps about twenty of them.
const DIR_BUDGET: u64 = 64 * 1024 * 1024;
#[cfg(target_os = "vita")]
const ROOT: &str = "ux0:data/alice/private/renderer-cache";
#[cfg(not(target_os = "vita"))]
const ROOT: &str = "private/renderer-cache";
thread_local! {static WRITE_WARNING:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
fn warn_write(error: &anyhow::Error) {
    if WRITE_WARNING.with(|w| w.replace(true)) { return; }
    eprintln!("[lg/vita-renderer] PVS cache write failed: {error:#}; later failures stay silent");
}
fn key(groups: &[Group], visibility: &crate::visibility::Visibility, corpus: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(MAGIC);
    hash.update((corpus.len() as u64).to_le_bytes()); hash.update(corpus.as_bytes());
    // `pvs_method` selects a different compiler with a different answer.
    let method = config().pvs_method.as_bytes();
    hash.update((method.len() as u64).to_le_bytes()); hash.update(method);
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
    corpus: &str,
) -> (Pvs, &'static str) {
    if !config().pvs_cache {
        return (Pvs::new(groups, visibility), "disabled");
    }
    let root = PathBuf::from(ROOT);
    let path = root.join(format!("pvs-{}.bin", key(groups, visibility, corpus)));
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
        use std::io::Read;
        let mut bytes=Vec::with_capacity(metadata.len() as usize);
        std::fs::File::open(&path)?.take(MAX_BYTES as u64+1).read_to_end(&mut bytes)?;
        ensure!(bytes.len()<=MAX_BYTES,"PVS cache grew beyond bound");
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
    let stored = encode(&pvs, groups.len()).and_then(|data| store(&root, &path, &data));
    match stored {
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
            warn_write(&e);
            (pvs, "miss-not-stored")
        }
    }
}
/// Write one entry atomically under `root`, then enforce the directory bound. A failed
/// write removes its temporary file and leaves no partial entry behind.
fn store(root: &Path, path: &Path, data: &[u8]) -> Result<()> {
    std::fs::create_dir_all(root)?;
    let tmp = path.with_extension("bin.tmp");
    let result = (|| -> Result<()> {
        use std::io::Write;
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        match std::fs::rename(&tmp, path) {
            Ok(()) => Ok(()),
            // Some card filesystems refuse to replace an existing target. A lost cache
            // entry costs one rebuild, so remove it and retry once.
            Err(_) if path.exists() => {
                std::fs::remove_file(path)?;
                std::fs::rename(&tmp, path)?;
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return result;
    }
    if let Some((files, freed)) =
        crate::shader_cache::note_write(root, DIR_BUDGET, data.len() as u64, path)
    {
        println!("[lg/vita-renderer] PVS cache evicted files={files} bytes={freed} bound_bytes={DIR_BUDGET}");
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mounted_data_change_invalidates_pvs_cache() {
        let visibility=crate::visibility::Visibility::default();
        assert_ne!(key(&[],&visibility,"old-corpus"),key(&[],&visibility,"new-corpus"));
    }
    #[test]
    fn pvs_method_is_part_of_the_key() {
        let visibility=crate::visibility::Visibility::default();
        let sources=super::super::with_config(|c|c.pvs_method="sources".into(),||key(&[],&visibility,"corpus"));
        let legacy=super::super::with_config(|c|c.pvs_method="legacy".into(),||key(&[],&visibility,"corpus"));
        assert_ne!(sources,legacy,"a method change must miss, never return the other compiler's lists");
    }
    #[test]
    fn store_replaces_an_invalid_entry_and_leaves_no_temporary() {
        let root=std::env::temp_dir().join(format!("lg-pvs-store-{}",std::process::id()));
        let _=std::fs::remove_dir_all(&root);
        let path=root.join("pvs-entry.bin");
        assert!(store(&root,&path,b"first").is_ok());
        assert_eq!(std::fs::read(&path).unwrap(),b"first");
        std::fs::write(&path,b"truncated").unwrap();
        assert!(store(&root,&path,b"second").is_ok());
        assert_eq!(std::fs::read(&path).unwrap(),b"second");
        let names=std::fs::read_dir(&root).unwrap().map(|e|e.unwrap().file_name()).collect::<Vec<_>>();
        assert_eq!(names,vec![std::ffi::OsString::from("pvs-entry.bin")],"no temporary survives a write");
        std::fs::remove_dir_all(&root).unwrap();
    }
    #[test]
    fn unreadable_cache_directory_is_a_miss_not_a_failure() {
        let root=std::env::temp_dir().join(format!("lg-pvs-save-dir-{}",std::process::id()));
        let _=std::fs::remove_dir_all(&root);
        // A file where the cache directory should be: every write fails, nothing panics.
        std::fs::write(&root,b"not a directory").unwrap();
        let path=root.join("pvs-entry.bin");
        assert!(store(&root,&path,b"data").is_err());
        assert!(!path.is_file());
        std::fs::remove_file(&root).unwrap();
    }
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
