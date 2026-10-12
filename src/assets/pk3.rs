//! Cache the ZIP directory, not the pack's contents. zip remains the authority
//! for validation and decompression; ordinary ZIP32 metadata needs no local I/O.
use anyhow::{ensure, Result};
use std::{fs::File, io::{self, Read, Seek, SeekFrom}};
use zip::ZipArchive;

const TAIL: u64 = 22 + u16::MAX as u64;
const MAX_DIRECTORY: u64 = 16 * 1024 * 1024;

pub(super) struct PackReader {
    file: File,
    len: u64,
    pos: u64,
    file_pos: u64,
    tail_start: u64,
    tail: Vec<u8>,
    directory_start: u64,
    directory: Vec<u8>,
    #[cfg(test)]
    pub reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
impl PackReader {
    pub fn new(file: File) -> io::Result<Self> {
        let len = file.metadata()?.len();
        let mut reader = Self {
            file, len, pos: 0, file_pos: u64::MAX, tail_start: len.saturating_sub(TAIL),
            tail: Vec::new(), directory_start: 0, directory: Vec::new(),
            #[cfg(test)]
            reads: Default::default(),
        };
        reader.seek_file(reader.tail_start)?;
        reader.tail = reader.read_region((len - reader.tail_start) as usize)?;
        // Unusual/ZIP64 archives still go through zip's original parser. Only
        // cache a bounded ZIP32 directory with an unambiguous terminal footer.
        if let Some(end) = reader.tail.windows(4).rposition(|b| b == b"PK\x05\x06") {
            let footer = &reader.tail[end..];
            if footer.len() >= 22 && footer.len() == 22 + u16_at(footer, 20) as usize {
                let size = u32_at(footer, 12) as u64;
                let offset = u32_at(footer, 16);
                let end_pos = reader.tail_start + end as u64;
                let zip64 = u16_at(footer, 10) == u16::MAX || offset == u32::MAX
                    || size == u32::MAX as u64
                    || (end >= 20 && &reader.tail[end - 20..end - 16] == b"PK\x06\x07");
                if !zip64 && size <= MAX_DIRECTORY && size <= end_pos
                    && offset as u64 <= end_pos - size {
                    reader.directory_start = end_pos - size;
                    reader.seek_file(reader.directory_start)?;
                    reader.directory = reader.read_region(size as usize)?;
                }
            }
        }
        Ok(reader)
    }
    fn file_read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        #[cfg(test)]
        self.reads.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let count = self.file.read(bytes)?;
        self.file_pos += count as u64;
        Ok(count)
    }
    fn seek_file(&mut self, pos: u64) -> io::Result<()> {
        if self.file_pos != pos {
            self.file.seek(SeekFrom::Start(pos))?;
            self.file_pos = pos;
        }
        Ok(())
    }
    fn read_region(&mut self, len: usize) -> io::Result<Vec<u8>> {
        let mut bytes = vec![0; len];
        let mut done = 0;
        while done < len {
            match self.file_read(&mut bytes[done..]) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(n) => done += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => (),
                Err(e) => return Err(e),
            }
        }
        Ok(bytes)
    }
}
impl Read for PackReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() { return Ok(0); }
        let cached = if self.pos >= self.directory_start
            && self.pos - self.directory_start < self.directory.len() as u64 {
            Some((&self.directory, self.directory_start))
        } else if self.pos >= self.tail_start && self.pos < self.len {
            Some((&self.tail, self.tail_start))
        } else { None };
        let count = if let Some((bytes, start)) = cached {
            let offset = (self.pos - start) as usize;
            let count = output.len().min(bytes.len() - offset);
            output[..count].copy_from_slice(&bytes[offset..offset + count]);
            count
        } else {
            self.seek_file(self.pos)?;
            self.file_read(output)?
        };
        self.pos += count as u64;
        Ok(count)
    }
}
impl Seek for PackReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let pos = match from {
            SeekFrom::Start(pos) => pos as i128,
            SeekFrom::Current(delta) => self.pos as i128 + delta as i128,
            SeekFrom::End(delta) => self.len as i128 + delta as i128,
        };
        if !(0..=u64::MAX as i128).contains(&pos) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.pos = pos as u64;
        Ok(self.pos)
    }
}
fn u16_at(b: &[u8], i: usize) -> u16 { u16::from_le_bytes(b[i..i + 2].try_into().unwrap()) }
fn u32_at(b: &[u8], i: usize) -> u32 { u32::from_le_bytes(b[i..i + 4].try_into().unwrap()) }

pub(super) struct Metadata {
    pub name: String,
    pub size: u64,
    pub crc: u32,
    #[cfg(test)]
    pub compressed_size: u64,
    #[cfg(test)]
    pub header_start: u64,
    #[cfg(test)]
    pub central_start: u64,
}
// Return None for encodings/ZIP64 that need zip's full metadata handling.
fn metadata(reader: &PackReader) -> Option<Vec<Metadata>> {
    let mut bytes = reader.directory.as_slice();
    let mut entries = Vec::new();
    while !bytes.is_empty() {
        if bytes.len() < 46 || &bytes[..4] != b"PK\x01\x02" { return None; }
        let flags = u16_at(bytes, 8);
        let size = u32_at(bytes, 24);
        let compressed_size = u32_at(bytes, 20);
        let offset = u32_at(bytes, 42);
        let name_len = u16_at(bytes, 28) as usize;
        let len = 46 + name_len + u16_at(bytes, 30) as usize + u16_at(bytes, 32) as usize;
        if len > bytes.len() || flags & 1 != 0
            || !matches!(u16_at(bytes, 10), 0 | 8)
            || [size, compressed_size, offset].contains(&u32::MAX) { return None; }
        let raw = &bytes[46..46 + name_len];
        let name = if flags & (1 << 11) != 0 {
            String::from_utf8_lossy(raw).into_owned()
        } else if raw.is_ascii() {
            String::from_utf8(raw.to_vec()).ok()?
        } else { return None; };
        // ZIP64 extra fields can override otherwise ordinary 32-bit fields.
        let mut extra = &bytes[46 + name_len..46 + name_len + u16_at(bytes, 30) as usize];
        while extra.len() >= 4 {
            let n = u16_at(extra, 2) as usize;
            if matches!(u16_at(extra, 0), 1 | 0x9901) || n + 4 > extra.len() { return None; }
            extra = &extra[n + 4..];
        }
        entries.push(Metadata {
            name, size: size as u64, crc: u32_at(bytes, 16),
            #[cfg(test)] compressed_size: compressed_size as u64,
            #[cfg(test)] header_start: offset as u64,
            #[cfg(test)] central_start: reader.directory_start + (reader.directory.len() - bytes.len()) as u64,
        });
        bytes = &bytes[len..];
    }
    Some(entries)
}
pub(super) fn open(file: File) -> Result<(ZipArchive<PackReader>, Vec<Metadata>)> {
    open_reader(PackReader::new(file)?)
}
#[cfg(test)]
pub(super) fn open_counted(file: File) -> Result<(ZipArchive<PackReader>, Vec<Metadata>, usize)> {
    let reader = PackReader::new(file)?;
    let reads = reader.reads.clone();
    let (pack, entries) = open_reader(reader)?;
    Ok((pack, entries, reads.load(std::sync::atomic::Ordering::Relaxed)))
}
fn open_reader(reader: PackReader) -> Result<(ZipArchive<PackReader>, Vec<Metadata>)> {
    let cached = metadata(&reader);
    let mut pack = ZipArchive::new(reader)?;
    let entries = if let Some(mut entries) = cached.filter(|e| e.len() == pack.len()) {
        #[cfg(test)]
        for entry in &mut entries { entry.header_start += pack.offset(); }
        #[cfg(not(test))]
        let _ = &mut entries;
        entries
    } else {
        (0..pack.len()).map(|i| {
            let e = pack.by_index(i)?;
            Ok(Metadata {
                name: e.name().to_owned(), size: e.size(), crc: e.crc32(),
                #[cfg(test)] compressed_size: e.compressed_size(),
                #[cfg(test)] header_start: e.header_start(),
                #[cfg(test)] central_start: e.central_header_start(),
            })
        }).collect::<Result<Vec<_>>>()?
    };
    ensure!(entries.len() == pack.len(), "PK3 metadata count mismatch");
    Ok((pack, entries))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn cp437_metadata_uses_zip_fallback_and_corrupt_directories_still_fail() -> Result<()> {
        let path = std::env::temp_dir().join(format!("lg-pk3-cp437-{}.zip", std::process::id()));
        let mut writer = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
        writer.start_file("cafe.tga", zip::write::FileOptions::default())?;
        writer.write_all(b"pixels")?;
        let mut bytes = writer.finish()?.into_inner();
        let central = bytes.windows(4).position(|b| b == b"PK\x01\x02").unwrap();
        bytes[central + 46 + 3] = 0x82; // CP437 é, with the UTF-8 flag unset.
        bytes[30 + 3] = 0x82;
        std::fs::write(&path, &bytes)?;
        let reader = PackReader::new(File::open(&path)?)?;
        assert!(metadata(&reader).is_none());
        let (mut pack, entries) = open_reader(reader)?;
        assert_eq!(entries[0].name, "café.tga");
        let mut payload = Vec::new();
        pack.by_index(0)?.read_to_end(&mut payload)?;
        assert_eq!(payload, b"pixels");
        drop(pack);
        bytes[central] = 0;
        std::fs::write(&path, bytes)?;
        assert!(ZipArchive::new(File::open(&path)?).is_err());
        assert!(open(File::open(&path)?).is_err());
        std::fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn buffered_directory_preserves_prepended_comments_duplicates_and_zip64() -> Result<()> {
        let path = std::env::temp_dir().join(format!("lg-pk3-{}.zip", std::process::id()));
        for zip64 in [false, true] {
            let mut writer = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
            writer.set_comment("footer with a comment");
            for name in ["dir/", "same", "same", "textures/é.tga"] {
                writer.start_file(name, zip::write::FileOptions::default().large_file(zip64))?;
                writer.write_all(b"payload")?;
            }
            let bytes = writer.finish()?.into_inner();
            std::fs::write(&path, [b"prepended".as_slice(), &bytes].concat())?;
            let mut original = ZipArchive::new(File::open(&path)?)?;
            let reader = PackReader::new(File::open(&path)?)?;
            let reads = reader.reads.clone();
            let (mut buffered, entries) = open_reader(reader)?;
            if !zip64 { assert!(reads.load(std::sync::atomic::Ordering::Relaxed) <= 2); }
            for (i, metadata) in entries.iter().enumerate() {
                let old = original.by_index(i)?;
                let mut new = buffered.by_index(i)?;
                assert_eq!(metadata.name, old.name());
                assert_eq!((metadata.size, metadata.crc, metadata.compressed_size, metadata.header_start, metadata.central_start),
                    (old.size(), old.crc32(), old.compressed_size(), old.header_start(), old.central_header_start()));
                let mut payload = Vec::new();
                new.read_to_end(&mut payload)?;
                assert_eq!(payload, b"payload");
            }
        }
        std::fs::write(&path, b"PK\x05\x06broken")?;
        assert!(open(File::open(&path)?).is_err());
        std::fs::remove_file(path)?;
        Ok(())
    }
}
