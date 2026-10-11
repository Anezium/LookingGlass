use anyhow::{bail, Context, Result};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Cursor, Read},
    path::{Path, PathBuf},
    sync::mpsc,
    thread::{self, JoinHandle},
};
use zip::ZipArchive;

const MAX_ENTRY: u64 = 128 * 1024 * 1024;

/// Read-only PK3 mount. Later alphabetically sorted packs override earlier packs.
pub struct Assets {
    packs: Vec<ZipArchive<File>>,
    paths: Vec<PathBuf>,
    entries: BTreeMap<String, (usize, usize)>,
    pub base: PathBuf,
}

impl Assets {
    /// Stable identity of the mounted, override-resolved corpus, independent of its folder.
    /// Central-directory CRC/size metadata avoids decoding every asset at startup.
    pub fn fingerprint(&mut self) -> Result<String> {
        use sha2::{Digest, Sha256};
        let mut hash = Sha256::new();
        for (name, &(pack, index)) in &self.entries {
            let entry = self.packs[pack].by_index(index)?;
            hash.update((name.len() as u64).to_le_bytes());
            hash.update(name.as_bytes());
            hash.update(entry.size().to_le_bytes());
            hash.update(entry.crc32().to_le_bytes());
        }
        Ok(format!("{:x}", hash.finalize()))
    }
    pub fn mount(base: &Path) -> Result<Self> {
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
        if paths.is_empty() {
            bail!("No PK3 archives found in {}", base.display());
        }
        let mut entries = BTreeMap::new();
        let mut packs = Vec::new();
        for path in &paths {
            let mut pack = ZipArchive::new(File::open(&path)?)
                .with_context(|| format!("Invalid PK3: {}", path.display()))?;
            let p = packs.len();
            for i in 0..pack.len() {
                let entry = pack.by_index(i)?;
                if !entry.is_dir() {
                    entries.insert(normalize(entry.name()), (p, i));
                }
            }
            packs.push(pack);
        }
        Ok(Self {
            packs,
            paths,
            entries,
            base: base.to_path_buf(),
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
    pub fn open(&mut self, name: &str) -> Result<impl Read + Send> {
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
struct EntryStream {
    receiver: Option<mpsc::Receiver<io::Result<Vec<u8>>>>,
    buffer: Cursor<Vec<u8>>,
    worker: Option<JoinHandle<()>>,
    finished: bool,
}
impl Drop for EntryStream {
    fn drop(&mut self) {
        // Disconnect before joining so a producer blocked on send can exit.
        self.receiver.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
struct EntryReader {
    pack: PathBuf,
    name: String,
    stream: Option<EntryStream>,
}
impl EntryReader {
    fn start(&mut self) -> io::Result<()> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let pack = self.pack.clone();
        let name = self.name.clone();
        let worker = thread::Builder::new()
            .name("asset-stream".into())
            .spawn(move || {
                let run = || -> Result<()> {
                    let mut archive = ZipArchive::new(File::open(pack)?)?;
                    let mut entry = archive.by_name(&name)?;
                    anyhow::ensure!(entry.size() <= MAX_ENTRY, "Asset exceeds 128 MiB limit");
                    loop {
                        let mut bytes = vec![0; STREAM_CHUNK];
                        let count = entry.read(&mut bytes)?;
                        bytes.truncate(count);
                        if sender.send(Ok(bytes)).is_err() || count == 0 {
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
            stream.buffer = Cursor::new(bytes);
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
    fn streaming_reports_crc_errors_and_can_be_dropped_early() -> Result<()> {
        use std::io::{Seek, SeekFrom};
        let dir = std::env::temp_dir().join(format!("looking-glass-stream-{}", std::process::id()));
        fs::create_dir_all(&dir)?;
        let path = dir.join("pak0.pk3");
        let mut archive = zip::ZipWriter::new(File::create(&path)?);
        archive.start_file("large.bin", zip::write::FileOptions::default())?;
        archive.write_all(&vec![7; STREAM_CHUNK * 8])?;
        archive.finish()?;
        let mut assets = Assets::mount(&dir)?;
        // Producer may already be blocked on the bounded queue when its reader drops.
        let mut reader = assets.open("large.bin")?;
        reader.read_exact(&mut [0; 1])?;
        drop(reader);
        let offset = assets.packs[0].by_name("large.bin")?.data_start();
        let mut file = fs::OpenOptions::new().write(true).open(&path)?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&[8])?;
        drop(file);
        let mut reader = assets.open("large.bin")?;
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
            z.start_file(
                "Textures/Test.ftx",
                zip::write::FileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )?;
            z.write_all(value)?;
            z.finish()?;
        }
        let mut a = Assets::mount(&dir)?;
        assert_eq!(a.read("TEXTURES\\test.ftx")?, b"new");
        assert!(a.read("missing").is_err());
        assert!(a.open("missing").is_err());
        let mut reader = a.open("TEXTURES\\test.ftx")?;
        let mut prefix = [0; 2];
        reader.read_exact(&mut prefix)?;
        assert_eq!(&prefix, b"ne");
        let mut tail = Vec::new();
        reader.read_to_end(&mut tail)?;
        assert_eq!(tail, b"w");
        // Independent reopening preserves the normalized lookup and pack precedence.
        let mut replay_reader = a.open("textures/test.ftx")?;
        assert_eq!(a.read("textures/test.ftx")?, b"new");
        drop(a);
        let mut replay = Vec::new();
        replay_reader.read_to_end(&mut replay)?;
        assert_eq!(replay, b"new");
        drop(reader);
        drop(replay_reader);
        for name in ["pak0.pk3", "pak5.pk3"] {
            fs::remove_file(dir.join(name))?;
        }
        fs::remove_dir(dir)?;
        Ok(())
    }
}
