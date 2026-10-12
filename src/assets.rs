use anyhow::{bail, Context, Result};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, BufReader, Cursor, Read, Seek, SeekFrom},
    sync::{atomic::{AtomicUsize, Ordering}, mpsc, Arc},
    thread::{self, JoinHandle},
    path::{Path, PathBuf},
};
use zip::ZipArchive;

const MAX_ENTRY: u64 = 128 * 1024 * 1024;

/// Bulk reads with an explicit logical cursor: zip asks for stream_position per
/// central record. BufReader's ordinary Seek would discard the buffer and query
/// the filesystem for these calls. Seek within this window stays in memory.
struct BufferedFile {
    reader: BufReader<File>,
    position: u64,
}
impl BufferedFile {
    fn open(path: &Path) -> io::Result<Self> {
        Ok(Self { reader: BufReader::with_capacity(1024 * 1024, File::open(path)?), position: 0 })
    }
}
impl Read for BufferedFile {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = self.reader.read(bytes)?;
        self.position += count as u64;
        Ok(count)
    }
}
impl Seek for BufferedFile {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let target = match from {
            SeekFrom::End(offset) => {
                let position = self.reader.seek(SeekFrom::End(offset))?;
                self.position = position;
                return Ok(position);
            }
            SeekFrom::Start(position) => i128::from(position),
            SeekFrom::Current(offset) => i128::from(self.position) + i128::from(offset),
        };
        if !(0..=i128::from(u64::MAX)).contains(&target) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid ZIP seek"));
        }
        let delta = target - i128::from(self.position);
        if let Ok(delta) = i64::try_from(delta) {
            self.reader.seek_relative(delta)?;
        } else {
            self.reader.seek(SeekFrom::Start(target as u64))?;
        }
        self.position = target as u64;
        Ok(self.position)
    }
    fn stream_position(&mut self) -> io::Result<u64> { Ok(self.position) }
}


/// Read-only PK3 mount. Later alphabetically sorted packs override earlier packs.
pub struct Assets {
    packs: Vec<ZipArchive<BufferedFile>>,
    paths: Vec<PathBuf>,
    entries: BTreeMap<String, (usize, usize)>,
    pub base: PathBuf,
    content_fingerprint: Option<String>,
}

impl Assets {
    /// Stable identity of the mounted, override-resolved corpus, independent of its folder.
    /// Central-directory CRC/size metadata avoids decoding every asset at startup.
    pub fn fingerprint(&mut self) -> Result<String> {
        if let Some(value)=&self.content_fingerprint { return Ok(value.clone()); }
        let start=std::time::Instant::now();
        eprintln!("[lg/startup] corpus_identity begin entries={}",self.entries.len());
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        for (name, &(pack, index)) in &self.entries {
            let entry = self.packs[pack].by_index(index)?;
            hash.update((name.len() as u64).to_le_bytes());
            hash.update(name.as_bytes());
            hash.update(entry.size().to_le_bytes());
            hash.update(entry.crc32().to_le_bytes());
        }
        let value=format!("{:x}", hash.finalize());
        self.content_fingerprint=Some(value.clone());
        eprintln!("[lg/startup] corpus_identity done elapsed_ms={:.3}",start.elapsed().as_secs_f64()*1000.);
        Ok(value)
    }
    pub fn open(base: &Path) -> Result<Self> {
        let mount_start=std::time::Instant::now();
        eprintln!("[lg/startup] pk3_mount begin data={}",base.display());
        let mut paths = fs::read_dir(base)
            .with_context(|| {
                format!(
                    "Cannot read game data at {}. Pass --data <base folder>.",
                    base.display()
                )
            })?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|s| s.eq_ignore_ascii_case("pk3")))
            .collect::<Vec<_>>();
        paths.sort_by_key(|p| p.file_name().unwrap_or_default().to_ascii_lowercase());
        eprintln!("[lg/startup] pk3_directory done packs={} elapsed_ms={:.3}",paths.len(),mount_start.elapsed().as_secs_f64()*1000.);
        if paths.is_empty() {
            bail!("No PK3 archives found in {}", base.display());
        }
        let mut entries = BTreeMap::new();
        let mut packs = Vec::new();
        for path in &paths {
            let start=std::time::Instant::now();
            eprintln!("[lg/startup] pk3_open begin file={}",path.display());
            let file=BufferedFile::open(path)?;
            eprintln!("[lg/startup] pk3_open done elapsed_ms={:.3}; central_directory begin",start.elapsed().as_secs_f64()*1000.);
            let mut pack = ZipArchive::new(file)
                .with_context(|| format!("Invalid PK3: {}", path.display()))?;
            eprintln!("[lg/startup] pk3_central_directory done entries={} elapsed_ms={:.3}; index begin",pack.len(),start.elapsed().as_secs_f64()*1000.);
            let p = packs.len();
            let total=pack.len();
            for i in 0..total {
                let entry = pack.by_index(i)?;
                if !entry.is_dir() {
                    entries.insert(normalize(entry.name()), (p, i));
                }
                if (i+1)%1024==0 || i+1==total {
                    eprintln!("[lg/startup] pk3_index progress={}/{} elapsed_ms={:.3}",i+1,total,start.elapsed().as_secs_f64()*1000.);
                }
            }
            packs.push(pack);
        }
        eprintln!("[lg/startup] pk3_mount done packs={} names={} elapsed_ms={:.3}",packs.len(),entries.len(),mount_start.elapsed().as_secs_f64()*1000.);
        Ok(Self {
            packs,
            paths,
            entries,
            base: base.to_path_buf(),
            content_fingerprint: None,
        })
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(&normalize(name))
    }
    pub fn pack_count(&self) -> usize {
        self.packs.len()
    }
    /// Stream an override-resolved entry through a bounded ZIP reader.
    pub fn open_stream(&mut self, name: &str) -> Result<EntryReader> {
        let &(p, i) = self
            .entries
            .get(&normalize(name))
            .with_context(|| format!("Asset not found: {name}"))?;
        // The lookup is normalized, but by_name needs the archive's original spelling.
        let original = self.packs[p].by_index(i)?.name().to_owned();
        let entry = self.packs[p].by_name(&original)?;
        if entry.size() > MAX_ENTRY {
            bail!("Asset exceeds 128 MiB limit: {name}");
        }
        Ok(EntryReader {
            pack: self.paths[p].clone(),
            name: original,
            stream: None,
            usage: Arc::new(StreamUse {
                live: AtomicUsize::new(0),
                peak: Arc::new(AtomicUsize::new(0)),
            }),
        })
    }
    pub fn read(&mut self, name: &str) -> Result<Vec<u8>> {
        let &(p, i) = self
            .entries
            .get(&normalize(name))
            .with_context(|| format!("Asset not found: {name}"))?;
        let entry = self.packs[p].by_index(i)?;
        if entry.size() > MAX_ENTRY {
            bail!("Asset exceeds 128 MiB limit: {name}");
        }
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry
            .take(MAX_ENTRY + 1)
            .read_to_end(&mut data)
            .with_context(|| format!("Reading {name}"))?;
        if data.len() as u64 > MAX_ENTRY {
            bail!("Expanded asset exceeds limit: {name}");
        }
        Ok(data)
    }
    pub fn maps(&self) -> Vec<String> {
        self.names()
            .filter(|n| n.starts_with("maps/") && n.ends_with(".bsp"))
            .map(|n| {
                n.trim_start_matches("maps/")
                    .trim_end_matches(".bsp")
                    .to_owned()
            })
            .collect()
    }
}
// zip 0.6.6's ZipFile erases Send on its borrowed reader. Keep it entirely on
// one worker thread; only owned chunks cross the bounded channel. This also lets
// a stream can outlive the mount without borrowing Assets.
const STREAM_CHUNK: usize = 32 * 1024;
struct StreamUse {
    live: AtomicUsize,
    peak: Arc<AtomicUsize>,
}
impl StreamUse {
    fn reserve(&self, bytes: usize) {
        if bytes == 0 {
            return;
        }
        let now = self.live.fetch_add(bytes, Ordering::Relaxed) + bytes;
        let mut seen = self.peak.load(Ordering::Relaxed);
        while now > seen {
            match self.peak.compare_exchange_weak(
                seen,
                now,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => seen = actual,
            }
        }
    }
    fn release(&self, bytes: usize) {
        if bytes > 0 {
            self.live.fetch_sub(bytes, Ordering::Relaxed);
        }
    }
}
struct EntryStream {
    receiver: Option<mpsc::Receiver<io::Result<Vec<u8>>>>,
    buffer: Cursor<Vec<u8>>,
    worker: Option<JoinHandle<()>>,
    finished: bool,
    usage: Arc<StreamUse>,
}
impl Drop for EntryStream {
    fn drop(&mut self) {
        let leftover = std::mem::replace(&mut self.buffer, Cursor::new(Vec::new()));
        self.usage.release(leftover.into_inner().capacity());
        // Disconnect before joining so a producer blocked on send can exit.
        if let Some(receiver) = self.receiver.take() {
            while let Ok(message) = receiver.try_recv() {
                if let Ok(bytes) = message {
                    self.usage.release(bytes.capacity());
                }
            }
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
pub(crate) struct EntryReader {
    pack: PathBuf,
    name: String,
    stream: Option<EntryStream>,
    usage: Arc<StreamUse>,
}
impl EntryReader {
    #[cfg(test)]
    fn peak_bytes(&self) -> usize {
        self.usage.peak.load(Ordering::Relaxed)
    }
    #[cfg(target_os = "vita")]
    pub(crate) fn peak_counter(&self) -> Arc<AtomicUsize> {
        Arc::clone(&self.usage.peak)
    }
    fn start(&mut self) -> io::Result<()> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let pack = self.pack.clone();
        let name = self.name.clone();
        let usage = Arc::clone(&self.usage);
        // std's default thread stack on Vita is not a settled size: one port
        // reports 64 KiB, the std we read reserves 2 MiB. Pin 256 KiB so ZIP
        // inflate on this worker does not depend on that default.
        let worker = thread::Builder::new()
            .stack_size(256 * 1024)
            .name("asset-stream".into())
            .spawn(move || {
                let run = || -> Result<()> {
                    let mut archive = ZipArchive::new(BufferedFile::open(&pack)?)?;
                    let mut entry = archive.by_name(&name)?;
                    anyhow::ensure!(entry.size() <= MAX_ENTRY, "Asset exceeds 128 MiB limit");
                    loop {
                        let mut bytes = vec![0; STREAM_CHUNK];
                        let count = entry.read(&mut bytes)?;
                        if count == 0 {
                            let _ = sender.send(Ok(Vec::new()));
                            return Ok(());
                        }
                        bytes.truncate(count);
                        let reserved = bytes.capacity();
                        usage.reserve(reserved);
                        if sender.send(Ok(bytes)).is_err() {
                            usage.release(reserved);
                            return Ok(());
                        }
                    }
                };
                if let Err(error) = run() {
                    let _ = sender.send(Err(io::Error::new(io::ErrorKind::InvalidData, error)));
                }
            })?;
        self.stream = Some(EntryStream {
            receiver: Some(receiver),
            buffer: Cursor::new(Vec::new()),
            worker: Some(worker),
            finished: false,
            usage: Arc::clone(&self.usage),
        });
        Ok(())
    }
}
impl Read for EntryReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.stream.is_none() {
            self.start()?;
        }
        let stream = self.stream.as_mut().unwrap();
        loop {
            let count = stream.buffer.read(output)?;
            if count > 0 {
                return Ok(count);
            }
            if stream.finished {
                return Ok(0);
            }
            let bytes = stream
                .receiver
                .as_ref()
                .unwrap()
                .recv()
                .map_err(|_| io::Error::other("Asset reader worker stopped"))??;
            stream.finished = bytes.is_empty();
            let previous = std::mem::replace(&mut stream.buffer, Cursor::new(bytes));
            stream.usage.release(previous.into_inner().capacity());
        }
    }
}
fn normalize(s: &str) -> String {
    s.replace('\\', "/").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn real_pk3_directory_metadata_matches_unbuffered() -> Result<()> {
        let Some(base) = std::env::var_os("ALICE_TEST_DATA") else { return Ok(()); };
        for path in fs::read_dir(base)?.map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pk3"))) {
            let mut old = ZipArchive::new(File::open(&path)?)?;
            let mut new = ZipArchive::new(BufferedFile::open(&path)?)?;
            assert_eq!(old.len(), new.len());
            assert_eq!(old.offset(), new.offset());
            for i in 0..old.len() {
                let a = old.by_index(i)?;
                let b = new.by_index(i)?;
                assert_eq!((a.name(), a.size(), a.compressed_size(), a.crc32(), a.header_start(), a.data_start(), a.central_header_start()),
                    (b.name(), b.size(), b.compressed_size(), b.crc32(), b.header_start(), b.data_start(), b.central_header_start()), "{} index={i}", path.display());
            }
        }
        Ok(())
    }
    #[test]
    fn streaming_reports_crc_errors_and_can_be_dropped_early() -> Result<()> {
        use std::io::{Seek, SeekFrom};
        let dir = std::env::temp_dir().join(format!("looking-glass-stream-{}", std::process::id()));
        fs::create_dir_all(&dir)?;
        let path = dir.join("pak0.pk3");
        let mut archive = zip::ZipWriter::new(File::create(&path)?);
        archive.start_file("large.bin", zip::write::FileOptions::default())?;
        archive.write_all(&vec![7; STREAM_CHUNK * 8])?;
        archive.finish()?;
        let mut assets = Assets::open(&dir)?;
        // Producer may already be blocked on the bounded queue when its reader drops.
        let mut reader = assets.open_stream("large.bin")?;
        reader.read_exact(&mut [0; 1])?;
        drop(reader);
        let offset = assets.packs[0].by_name("large.bin")?.data_start();
        let mut file = fs::OpenOptions::new().write(true).open(&path)?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&[8])?;
        drop(file);
        let mut reader = assets.open_stream("large.bin")?;
        assert!(io::copy(&mut reader, &mut io::sink()).is_err());
        drop(reader);
        drop(assets);
        fs::remove_file(path)?;
        fs::remove_dir(dir)?;
        Ok(())
    }
    #[test]
    fn later_pack_wins_and_paths_are_case_insensitive() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("looking-glass-vfs-{}", std::process::id()));
        fs::create_dir_all(&dir)?;
        for (name, value) in [("pak0.pk3", b"old"), ("pak5.pk3", b"new")] {
            let mut z = zip::ZipWriter::new(File::create(dir.join(name))?);
            z.start_file("Textures/Test.ftx", zip::write::FileOptions::default())?;
            z.write_all(value)?;
            z.finish()?;
        }
        let mut a = Assets::open(&dir)?;
        assert_eq!(a.read("TEXTURES\\test.ftx")?, b"new");
        assert!(a.read("missing").is_err());
        assert!(a.open_stream("missing").is_err());
        let mut reader = a.open_stream("TEXTURES\\test.ftx")?;
        let mut prefix = [0; 2]; reader.read_exact(&mut prefix)?;
        assert_eq!(&prefix, b"ne");
        let mut tail = Vec::new(); reader.read_to_end(&mut tail)?;
        assert_eq!(tail, b"w");
        assert_eq!(reader.peak_bytes(), STREAM_CHUNK);
        let mut independent = a.open_stream("textures/test.ftx")?;
        drop(reader);
        let identity=a.fingerprint()?;
        assert_eq!(identity,a.fingerprint()?);
        drop(a);
        let mut replay = Vec::new(); independent.read_to_end(&mut replay)?;
        assert_eq!(replay, b"new"); drop(independent);
        let mut z=zip::ZipWriter::new(File::create(dir.join("pak5.pk3"))?);
        z.start_file("Textures/Test.ftx",zip::write::FileOptions::default())?;
        z.write_all(b"changed")?;z.finish()?;
        let mut updated=Assets::open(&dir)?;
        assert_ne!(identity,updated.fingerprint()?);
        drop(updated);
        for name in ["pak0.pk3", "pak5.pk3"] {
            fs::remove_file(dir.join(name))?;
        }
        fs::remove_dir(dir)?;
        Ok(())
    }
}
