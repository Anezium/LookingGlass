//! Cache the ZIP directory, not the pack's contents. zip remains the authority
//! for validation and decompression; ordinary ZIP32 metadata needs no local I/O.
//! An optional bounded read-ahead window coalesces the small local-header reads and
//! the decoder's input chunks of one entry into a few large pack reads.
use anyhow::{ensure, Result};
use std::{fs::File, io::{self, Read, Seek, SeekFrom}, sync::{Arc, atomic::{AtomicU64, Ordering}}};
use zip::ZipArchive;

const TAIL: u64 = 22 + u16::MAX as u64;
const MAX_DIRECTORY: u64 = 16 * 1024 * 1024;

// Per-thread pack read accounting. One bounded line per outermost load phase.
pub(crate) mod read_stats {
    use std::cell::Cell;
    #[derive(Clone, Copy, Default)]
    pub struct Snapshot { pub calls: u64, pub bytes: u64, pub micros: u64, pub seeks: u64 }
    impl Snapshot {
        fn delta(self, before: Snapshot) -> Snapshot {
            Snapshot {
                calls: self.calls.saturating_sub(before.calls),
                bytes: self.bytes.saturating_sub(before.bytes),
                micros: self.micros.saturating_sub(before.micros),
                seeks: self.seeks.saturating_sub(before.seeks),
            }
        }
    }
    thread_local! {
        static IO: Cell<Snapshot> = const { Cell::new(Snapshot { calls: 0, bytes: 0, micros: 0, seeks: 0 }) };
        static REPORTED: Cell<Snapshot> = const { Cell::new(Snapshot { calls: 0, bytes: 0, micros: 0, seeks: 0 }) };
    }
    pub fn note_read(bytes: usize, micros: u64) {
        let _ = IO.try_with(|cell| {
            let mut s = cell.get();
            s.calls += 1;
            s.bytes += bytes as u64;
            s.micros += micros;
            cell.set(s);
        });
    }
    pub fn note_seek() {
        let _ = IO.try_with(|cell| {
            let mut s = cell.get();
            s.seeks += 1;
            cell.set(s);
        });
    }
    fn snapshot() -> Snapshot { IO.try_with(Cell::get).unwrap_or_default() }
    /// One bounded line per outermost load phase: the slice since the previous report, then
    /// the running total. `elapsed_ms` is time inside the pack file reads only.
    pub fn report(scope: &str) {
        let now = snapshot();
        let before = REPORTED.try_with(|cell| cell.replace(now)).unwrap_or_default();
        let d = now.delta(before);
        if d.calls == 0 { return; }
        eprintln!("[lg/startup] load_read scope={scope} calls={} bytes={} seeks={} elapsed_ms={:.3} total_bytes={}",
            d.calls, d.bytes, d.seeks, d.micros as f64 / 1000., now.bytes);
    }
}

pub(super) struct PackReader {
    file: File,
    len: u64,
    pos: u64,
    file_pos: u64,
    tail_start: u64,
    tail: Vec<u8>,
    directory_start: u64,
    directory: Vec<u8>,
    // Bounded read-ahead for one entry payload. `window_size` 0 keeps one read per
    // request; otherwise each fill is at most `window_size` and never crosses the
    // entry window in `cap_end`. `window_fill` is the valid prefix of `window`,
    // which keeps its allocation.
    window_size: usize,
    window_start: u64,
    window_fill: usize,
    window: Vec<u8>,
    // First position beyond the announced entry (plus one KiB for its local header).
    // A fill never crosses it, so read-ahead cannot waste a card read on a far entry.
    cap_end: Arc<AtomicU64>,
    #[cfg(test)]
    pub reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
impl PackReader {
    pub fn new(file: File) -> io::Result<Self> { Self::with_window(file, 0) }
    pub fn with_window(file: File, window_size: usize) -> io::Result<Self> {
        let len = file.metadata()?.len();
        let mut reader = Self {
            file, len, pos: 0, file_pos: u64::MAX, tail_start: len.saturating_sub(TAIL),
            tail: Vec::new(), directory_start: 0, directory: Vec::new(),
            window_size, window_start: 0, window_fill: 0, window: Vec::new(),
            cap_end: Arc::new(AtomicU64::new(0)),
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
        let started = std::time::Instant::now();
        let count = self.file.read(bytes)?;
        let micros = started.elapsed().as_micros() as u64;
        self.file_pos += count as u64;
        read_stats::note_read(count, micros);
        Ok(count)
    }
    fn seek_file(&mut self, pos: u64) -> io::Result<()> {
        if self.file_pos != pos {
            read_stats::note_seek();
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
    /// Serve the current position from the read-ahead window, if it covers it.
    fn window_read(&self, output: &mut [u8]) -> Option<usize> {
        let offset = self.pos.checked_sub(self.window_start)?;
        if offset >= self.window_fill as u64 { return None; }
        let offset = offset as usize;
        let count = output.len().min(self.window_fill - offset);
        output[..count].copy_from_slice(&self.window[offset..offset + count]);
        Some(count)
    }
    pub fn cap_end(&self) -> Arc<AtomicU64> { Arc::clone(&self.cap_end) }
    /// One sequential read, at most `window_size` bytes and never past the entry window.
    fn fill_window(&mut self) -> io::Result<usize> {
        let want = (self.window_size as u64)
            .min(self.cap_end.load(Ordering::Relaxed).saturating_sub(self.pos))
            .min(self.len.saturating_sub(self.pos)) as usize;
        self.window_fill = 0;
        if want == 0 { return Ok(0); }
        // Grow once; later fills reuse the same allocation and overwrite its prefix.
        if self.window.len() < want { self.window.resize(want, 0); }
        self.window_start = self.pos;
        self.seek_file(self.pos)?;
        let mut window = std::mem::take(&mut self.window);
        let mut done = 0;
        let mut result = Ok(());
        while done < want {
            match self.file_read(&mut window[done..want]) {
                Ok(0) => break,
                Ok(n) => done += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => (),
                Err(e) => { result = Err(e); break; }
            }
        }
        self.window = window;
        result?;
        self.window_fill = done;
        Ok(done)
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
        } else if self.pos >= self.len {
            0
        } else if let Some(count) = self.window_read(output) {
            count
        } else if output.len() >= self.window_size {
            // No window, or a read as large as one: go straight into the caller's
            // buffer. That keeps a whole stored entry from being copied twice.
            self.seek_file(self.pos)?;
            self.window_start = self.pos;
            self.window_fill = 0;
            self.file_read(output)?
        } else {
            let filled = self.fill_window()?;
            if filled == 0 {
                // No window allowance at this position: read straight through.
                self.seek_file(self.pos)?;
                self.window_start = self.pos;
                self.window_fill = 0;
                self.file_read(output)?
            } else {
                self.window_read(output).unwrap_or(0)
            }
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
    pub compressed_size: u64,
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
            compressed_size: compressed_size as u64,
            header_start: offset as u64,
            #[cfg(test)] central_start: reader.directory_start + (reader.directory.len() - bytes.len()) as u64,
        });
        bytes = &bytes[len..];
    }
    Some(entries)
}
#[cfg(test)]
pub(super) fn open(file: File) -> Result<(ZipArchive<PackReader>, Vec<Metadata>)> {
    let (pack, entries, _) = open_windowed(file, 0)?;
    Ok((pack, entries))
}
pub(super) fn open_windowed(file: File, window: usize)
    -> Result<(ZipArchive<PackReader>, Vec<Metadata>, Arc<AtomicU64>)> {
    let reader = PackReader::with_window(file, window)?;
    let cap_end = reader.cap_end();
    let (pack, entries) = open_reader(reader)?;
    Ok((pack, entries, cap_end))
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
        // The central-directory offsets are relative to the archive, not the file.
        for entry in &mut entries { entry.header_start += pack.offset(); }
        entries
    } else {
        (0..pack.len()).map(|i| {
            let e = pack.by_index(i)?;
            Ok(Metadata {
                name: e.name().to_owned(), size: e.size(), crc: e.crc32(),
                compressed_size: e.compressed_size(),
                header_start: e.header_start(),
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

    // A window may change how many reads fetch the bytes, never which bytes an entry
    // produces. Sizes straddle one fill, several fills and the stored fast path.
    #[test]
    fn windowed_reads_return_the_same_bytes_and_cost_fewer_reads() -> Result<()> {
        use std::sync::atomic::Ordering;
        let path = std::env::temp_dir().join(format!("lg-pk3-window-{}.zip", std::process::id()));
        let sizes = [("one.bin", 1usize), ("small.bin", 4096), ("page.bin", 33_000),
                     ("four.bin", 300_000), ("stored.bin", 70_000)];
        let mut payloads = Vec::new();
        let mut writer = zip::ZipWriter::new(File::create(&path)?);
        for (name, size) in sizes {
            let method = if name == "stored.bin" {
                zip::CompressionMethod::Stored
            } else {
                zip::CompressionMethod::Deflated
            };
            let data = (0..size).map(|i| (i.wrapping_mul(31) ^ size) as u8).collect::<Vec<_>>();
            writer.start_file(name, zip::write::FileOptions::default().compression_method(method))?;
            writer.write_all(&data)?;
            payloads.push((name.to_string(), data));
        }
        writer.finish()?;
        let mut reads = Vec::new();
        for window in [0usize, 7, 4096, 256 * 1024] {
            let reader = PackReader::with_window(File::open(&path)?, window)?;
            let cap = reader.cap_end();
            let counter = reader.reads.clone();
            let (mut pack, entries) = open_reader(reader)?;
            for (i, (name, expected)) in payloads.iter().enumerate() {
                cap.store(entries[i].header_start + entries[i].compressed_size + 1024, Ordering::Relaxed);
                let mut entry = pack.by_index(i)?;
                let mut got = Vec::new();
                entry.read_to_end(&mut got)?;
                assert_eq!(&got, expected, "window {window} read {name} differently");
            }
            reads.push(counter.load(Ordering::Relaxed));
        }
        // 6 reads per entry without a window: three local-header fields, the data
        // chunks and the end probe. One 256 KiB fill covers every entry here.
        assert_eq!(reads[0], 30, "unwindowed reads: local headers and chunks: {reads:?}");
        assert_eq!(reads[2], 5, "a 4 KiB window reads each entry once: {reads:?}");
        assert_eq!(reads[3], 5, "a 256 KiB window reads each entry once: {reads:?}");
        assert!(reads[1] < reads[0], "even a 7-byte window saves header reads: {reads:?}");
        assert!(reads[1] > reads[3], "a 7-byte window cannot beat 256 KiB: {reads:?}");
        std::fs::remove_file(path)?;
        Ok(())
    }
}
