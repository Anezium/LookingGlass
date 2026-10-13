//! Versioned, bounded binary envelopes shared by Vita startup caches.
//! GLSL bytes include all defines. Program pairs retain translator semantics.
#![cfg_attr(not(any(target_os = "vita", test)), allow(dead_code))]
use sha2::{Digest, Sha256};
use std::{cell::RefCell, fs, io::{Read, Write}, path::Path};
#[cfg(any(target_os = "vita", test))]
#[path="shader_cache/native.rs"]
mod native;

pub(crate) const MAX_BINARY: usize = 4 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"LGCACHE1";
const HEADER: usize = 8 + 32 + 32 + 4;
thread_local! { static NAME: RefCell<&'static str> = const { RefCell::new("macroquad/miniquad") }; }
pub(crate) struct Name(&'static str);
impl Name {
    pub fn new(name: &'static str) -> Self { Self(NAME.with(|n| n.replace(name))) }
}
impl Drop for Name { fn drop(&mut self) { NAME.with(|n| n.replace(self.0)); } }
pub(crate) fn name() -> &'static str { NAME.with(|n| *n.borrow()) }

pub(crate) fn key(identity: &str, domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for part in std::iter::once(identity.as_bytes()).chain(std::iter::once(domain)).chain(parts.iter().copied()) {
        h.update((part.len() as u64).to_le_bytes()); h.update(part);
    }
    h.finalize().into()
}
pub(crate) fn hex(key: &[u8; 32]) -> String { key.iter().map(|b| format!("{b:02x}")).collect() }
fn encode(key: &[u8; 32], payload: &[u8]) -> Option<Vec<u8>> {
    if payload.is_empty() || payload.len() > MAX_BINARY { return None; }
    let mut b = Vec::with_capacity(HEADER + payload.len());
    b.extend_from_slice(MAGIC); b.extend_from_slice(key);
    b.extend_from_slice(&Sha256::digest(payload));
    b.extend_from_slice(&(payload.len() as u32).to_le_bytes()); b.extend_from_slice(payload); Some(b)
}
fn decode<'a>(bytes: &'a [u8], key: &[u8; 32]) -> Option<&'a [u8]> {
    if bytes.len() < HEADER || &bytes[..8] != MAGIC || &bytes[8..40] != key { return None; }
    let size = word(bytes, 72)? as usize;
    if size == 0 || size > MAX_BINARY || size.checked_add(HEADER)? != bytes.len() { return None; }
    let data = &bytes[HEADER..];
    (Sha256::digest(data).as_slice() == &bytes[40..72]).then_some(data)
}
pub(crate) fn read(path: &Path, key: &[u8; 32]) -> Option<Vec<u8>> {
    let mut f = fs::File::open(path).ok()?;
    if f.metadata().ok()?.len() > (HEADER + MAX_BINARY) as u64 { return None; }
    let mut b = Vec::new(); Read::by_ref(&mut f).take((HEADER + MAX_BINARY + 1) as u64).read_to_end(&mut b).ok()?;
    decode(&b, key).map(<[u8]>::to_vec)
}
pub(crate) fn write(path: &Path, key: &[u8; 32], payload: &[u8]) -> std::io::Result<()> {
    let data = encode(key, payload).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "cache payload size"))?;
    if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
    let temporary = path.with_extension("tmp");
    let result = (|| { let mut f = fs::File::create(&temporary)?; f.write_all(&data)?; f.sync_all()?; fs::rename(&temporary, path) })();
    if result.is_err() { let _ = fs::remove_file(temporary); } result
}
pub(crate) fn word(b: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(offset..offset.checked_add(4)?)?.try_into().ok()?))
}
/// vitaGL 2bdbe89: 16 eight-byte attributes, then two length-prefixed serialized stages.
/// Do not feed an unchecked disk buffer to vitaGL's unbounded deserializer.
pub(crate) fn stages(b: &[u8]) -> Option<[&[u8]; 2]> {
    if word(b, 0)? > 16 { return None; }
    let vlen = word(b, 132)? as usize;
    let vend = 136usize.checked_add(vlen)?;
    let flen = word(b, vend)? as usize;
    let fend = vend.checked_add(4)?.checked_add(flen)?;
    if fend != b.len() { return None; }
    let v = b.get(136..vend)?; let f = b.get(vend + 4..fend)?;
    stage_gxp(v)?; stage_gxp(f)?; Some([v, f])
}
/// Locate the GXP after the matrix index table and optional textureSize metadata.
/// The SDK archive identity pins the serializer ABI; both build variants are recognized.
pub(crate) fn stage_gxp(b: &[u8]) -> Option<(&[u8], &[u8])> {
    let count = word(b, 0)? as usize;
    let end = count.checked_add(1)?.checked_mul(4)?;
    let indices = b.get(4..end)?;
    for offset in [Some(end), b.get(end).and_then(|n| end.checked_add(1 + *n as usize * 64))].into_iter().flatten() {
        let gxp = b.get(offset..)?;
        if gxp.len() >= 156 && gxp.get(..4) == Some(b"GXP\0") && (156..=gxp.len()).contains(&(word(gxp, 8)? as usize)) && gxp.len() - (word(gxp, 8)? as usize) < 16 {
            return Some((gxp, indices));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn fake_stage() -> Vec<u8> {
        let mut s = vec![0; 4 + 156]; s[4..8].copy_from_slice(b"GXP\0"); s[12..16].copy_from_slice(&156u32.to_le_bytes()); s
    }
    pub(super) fn fake_program() -> Vec<u8> {
        let s = fake_stage(); let mut b = vec![0;132];
        for kind in 0..2 { let mut s=s.clone(); s[20]=kind; b.extend_from_slice(&(s.len() as u32).to_le_bytes()); b.extend_from_slice(&s); } b
    }
    #[test] fn identities_defines_and_stage_boundaries_change_keys() {
        let k = key("build-a", b"program", &[b"ab", b"c"]);
        assert_ne!(k, key("build-a", b"program", &[b"a",b"bc"]));
        assert_ne!(k, key("build-b", b"program", &[b"ab",b"c"]));
        assert_ne!(k, key("build-a", b"program", &[b"ab",b"#define X\nc"]));
    }
    #[test] fn envelope_rejects_corruption_truncation_wrong_key_and_trailing_bytes() {
        let k = key("sdk", b"shader", &[b"source"]); let b = encode(&k, b"binary").unwrap();
        assert_eq!(decode(&b,&k), Some(b"binary".as_slice()));
        for i in 0..b.len() { let mut bad=b.clone(); bad[i]^=1; assert!(decode(&bad,&k).is_none()); }
        for n in 0..b.len() { assert!(decode(&b[..n],&k).is_none()); }
        assert!(decode(&b,&[0;32]).is_none());
        let mut bad=b; bad.push(0); assert!(decode(&bad,&k).is_none());
        assert!(encode(&k,&[]).is_none()); assert!(encode(&k,&vec![0;MAX_BINARY+1]).is_none());
    }
    #[test] fn serializer_bounds_are_checked_before_native_load() {
        let b=fake_program(); assert!(stages(&b).is_some());
        for n in 0..b.len() { assert!(stages(&b[..n]).is_none()); }
        for offset in [0,132,136,148,296] { let mut bad=b.clone(); bad[offset..offset+4].copy_from_slice(&u32::MAX.to_le_bytes()); assert!(stages(&bad).is_none()); }
        let mut sized = fake_stage(); sized.insert(4,0); assert!(stage_gxp(&sized).is_some());
    }
    #[test] fn atomic_file_roundtrip_replaces_invalid_entry() {
        let dir=std::env::temp_dir().join(format!("lg-j7-cache-{}",std::process::id())); let p=dir.join("entry.bin");
        let k=[7;32]; write(&p,&k,b"one").unwrap(); assert_eq!(read(&p,&k).unwrap(),b"one");
        fs::write(&p,b"broken").unwrap(); assert!(read(&p,&k).is_none());
        write(&p,&k,b"two").unwrap(); assert_eq!(read(&p,&k).unwrap(),b"two"); fs::remove_dir_all(dir).unwrap();
    }
}
