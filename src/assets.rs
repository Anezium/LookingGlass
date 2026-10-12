use anyhow::{bail, Context, Result};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Cursor, Read},
    sync::{atomic::{AtomicUsize, Ordering}, mpsc, Arc},
    thread::{self, JoinHandle},
    path::{Path, PathBuf},
};
use zip::ZipArchive;
#[path = "assets/pk3.rs"]
mod pk3;

const MAX_ENTRY: u64 = 128 * 1024 * 1024;

/// Read-only PK3 mount. Later alphabetically sorted packs override earlier packs.
pub struct Assets {
    packs: Vec<ZipArchive<pk3::PackReader>>,
    paths: Vec<PathBuf>,
    entries: BTreeMap<String, (usize, usize, u64, u32)>,
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
        for (name, &(_, _, size, crc)) in &self.entries {
            hash.update((name.len() as u64).to_le_bytes());
            hash.update(name.as_bytes());
            hash.update(size.to_le_bytes());
            hash.update(crc.to_le_bytes());
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
            let file=File::open(&path)?;
            eprintln!("[lg/startup] pk3_open done elapsed_ms={:.3}; central_directory begin",start.elapsed().as_secs_f64()*1000.);
            let (pack, metadata) = pk3::open(file)
                .with_context(|| format!("Invalid PK3: {}", path.display()))?;
            eprintln!("[lg/startup] pk3_central_directory done entries={} elapsed_ms={:.3}; index begin",pack.len(),start.elapsed().as_secs_f64()*1000.);
            let p = packs.len();
            let total=pack.len();
            let index_start = std::time::Instant::now();
            for (i, entry) in metadata.into_iter().enumerate() {
                if !entry.name.ends_with(['/', '\\']) {
                    entries.insert(normalize(&entry.name), (p, i, entry.size, entry.crc));
                }
                if (i+1)%1024==0 || i+1==total {
                    eprintln!("[lg/startup] pk3_index progress={}/{} elapsed_ms={:.3}",i+1,total,start.elapsed().as_secs_f64()*1000.);
                }
            }
            eprintln!("[lg/startup] pk3_index done entries={total} elapsed_ms={:.3}", index_start.elapsed().as_secs_f64()*1000.);
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
        let &(p, i, _, _) = self
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
        let &(p, i, _, _) = self
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
                    let mut archive = ZipArchive::new(pk3::PackReader::new(File::open(pack)?)?)?;
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
    #[ignore = "requires user-supplied PK3s; set LG_PK3_TEST_DATA"]
    fn real_pk3_metadata_identity_and_phase_timings() -> Result<()> {
        use sha2::{Digest, Sha256};
        use std::time::Instant;
        let base = PathBuf::from(std::env::var("LG_PK3_TEST_DATA")?);
        let mut paths = fs::read_dir(&base)?.map(|e| e.map(|e| e.path()))
            .collect::<io::Result<Vec<_>>>()?;
        paths.retain(|p| p.extension().is_some_and(|s| s.eq_ignore_ascii_case("pk3")));
        paths.sort_by_key(|p| p.file_name().unwrap_or_default().to_ascii_lowercase());
        assert!(!paths.is_empty());
        let mut reference_identity = String::new();
        let mut reference_names = Vec::new();
        for run in 0..5 {
            let mut old_packs = Vec::new();
            let mut old_entries = BTreeMap::new();
            let mut new_entries = BTreeMap::new();
            let mut phases = [0.; 5];
            for (p, path) in paths.iter().enumerate() {
                let start = Instant::now();
                let mut old = ZipArchive::new(File::open(path)?)?;
                let old_cd = start.elapsed().as_secs_f64() * 1000.;
                let start = Instant::now();
                for i in 0..old.len() {
                    let e = old.by_index(i)?;
                    if !e.is_dir() { old_entries.insert(normalize(e.name()), (p, i)); }
                }
                let old_index = start.elapsed().as_secs_f64() * 1000.;
                let start = Instant::now();
                let (mut new, metadata, reads) = pk3::open_counted(File::open(path)?)?;
                let new_cd = start.elapsed().as_secs_f64() * 1000.;
                assert!(reads <= 2, "{} used {reads} reads during directory + metadata", path.display());
                let start = Instant::now();
                for (i, e) in metadata.iter().enumerate() {
                    if !e.name.ends_with(['/', '\\']) {
                        new_entries.insert(normalize(&e.name), (p, i, e.size, e.crc));
                    }
                }
                let new_index = start.elapsed().as_secs_f64() * 1000.;
                // Verify every entry, in directory order, including overridden
                // entries and directories. Local data offsets are checked too.
                if run == 0 {
                    assert_eq!(old.len(), metadata.len());
                    for (i, e) in metadata.iter().enumerate() {
                        let a = old.by_index(i)?;
                        let b = new.by_index(i)?;
                        assert_eq!(e.name, a.name());
                        assert_eq!((e.size, e.crc, e.compressed_size, e.header_start, e.central_start),
                            (a.size(), a.crc32(), a.compressed_size(), a.header_start(), a.central_header_start()));
                        assert_eq!((b.name(), b.size(), b.crc32(), b.compressed_size(), b.header_start(), b.data_start()),
                            (a.name(), a.size(), a.crc32(), a.compressed_size(), a.header_start(), a.data_start()));
                    }
                }
                eprintln!("[j6/bench] run={run} pack={} entries={} central_ms={old_cd:.3}->{new_cd:.3} index_ms={old_index:.3}->{new_index:.3}", path.file_name().unwrap().to_string_lossy(), old.len());
                for (sum, ms) in phases.iter_mut().zip([old_cd, new_cd, old_index, new_index]) { *sum += ms; }
                old_packs.push(old);
            }
            let start = Instant::now();
            let mut hash = Sha256::new();
            for (name, &(p, i)) in &old_entries {
                let e = old_packs[p].by_index(i)?;
                hash.update((name.len() as u64).to_le_bytes());
                hash.update(name.as_bytes());
                hash.update(e.size().to_le_bytes());
                hash.update(e.crc32().to_le_bytes());
            }
            phases[4] = start.elapsed().as_secs_f64() * 1000.;
            let expected = format!("{:x}", hash.finalize());
            assert_eq!(old_entries.iter().map(|(n, &(p, i))| (n, p, i)).collect::<Vec<_>>(),
                new_entries.iter().map(|(n, &(p, i, _, _))| (n, p, i)).collect::<Vec<_>>());
            let mut assets = Assets::open(&base)?;
            assert_eq!(assets.entries, new_entries);
            let start = Instant::now();
            assert_eq!(assets.fingerprint()?, expected);
            let identity_ms = start.elapsed().as_secs_f64() * 1000.;
            eprintln!("[j6/bench] totals run={run} names={} central_ms={:.3}->{:.3} index_ms={:.3}->{:.3} identity_ms={:.3}->{identity_ms:.3} identity={expected}", old_entries.len(), phases[0], phases[1], phases[2], phases[3], phases[4]);
            reference_identity = expected;
            reference_names = old_entries.keys().cloned().collect();
            // Verify real decompression through both readers, outside timing.
            if run == 0 {
                for name in ["maps/gvillage.bsp", "ui/main.urc", "ui/loadsave.urc", "ui/quit.urc"] {
                    let &(p, i) = old_entries.get(name).context("Missing test asset")?;
                    let mut bytes = Vec::new();
                    old_packs[p].by_index(i)?.read_to_end(&mut bytes)?;
                    assert_eq!(assets.read(name)?, bytes);
                }
            }
        }
        assert_eq!(reference_names.len(), 10_221);
        eprintln!("[j6/proof] identity={reference_identity} resolved_names={}", reference_names.len());
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
