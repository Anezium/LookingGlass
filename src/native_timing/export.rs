use super::*;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};
/// Bounded export buffer, inside AUX_RESERVE. serde_json issues one write per
/// token; without it every token becomes a separate file write on the device.
pub(super) const EXPORT_BUFFER: usize = 32 * 1024;

pub fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
pub fn parse_hash(text: Option<String>) -> anyhow::Result<Option<[u8; 32]>> {
    let Some(text) = text else { return Ok(None) };
    anyhow::ensure!(
        text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid timing metadata SHA256"
    );
    let mut out = [0; 32];
    for (i, value) in out.iter_mut().enumerate() {
        *value = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16)?;
    }
    Ok(Some(out))
}
pub fn random_id() -> Option<[u8; 16]> {
    let mut out = [0; 16];
    #[cfg(target_os = "vita")]
    {
        #[link(name = "SceLibKernel_stub")]
        extern "C" {
            fn sceKernelGetRandomNumber(out: *mut u8, size: u32) -> i32;
        }
        if unsafe { sceKernelGetRandomNumber(out.as_mut_ptr(), 16) } < 0 {
            return None;
        }
    }
    #[cfg(not(target_os = "vita"))]
    {
        use std::io::Read;
        File::open("/dev/urandom").ok()?.read_exact(&mut out).ok()?;
    }
    (out != [0; 16]).then_some(out)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
#[derive(Serialize)]
struct Snapshot<'a> {
    schema: &'static str,
    run_id: Option<[u8; 16]>,
    clock_id: &'static str,
    capture_ordinal: u64,
    #[serde(skip_serializing_if="Option::is_none")]
    schedule: &'a Option<schedule::Schedule>,
    frames: &'a [Frame],
    worlds: &'a [World],
    events: &'a [Event],
    pending: &'a Option<Frame>,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw_counters: &'a Option<audio_counters::Capture>,
    #[serde(skip_serializing_if = "Option::is_none")]
    audio_producer: &'a Option<Box<audio_producer::Owner>>,
}
/// The buffer is INSIDE this writer: the receipt counts exactly the prefix
/// accepted by Write, including short writes. A failed flush/sync has no receipt.
pub(super) struct HashWriter<W> {
    inner: W,
    hash: Sha256,
    bytes: u64,
}
impl<W: Write> Write for HashWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(bytes)?;
        self.hash.update(&bytes[..n]);
        self.bytes = self
            .bytes
            .checked_add(n as u64)
            .ok_or_else(|| io::Error::other("export length overflow"))?;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
pub(super) trait DurableWrite: Write {
    fn sync_all(&self) -> io::Result<()>;
}
impl DurableWrite for File {
    fn sync_all(&self) -> io::Result<()> {
        File::sync_all(self)
    }
}
impl<W: DurableWrite> HashWriter<io::BufWriter<W>> {
    pub(super) fn buffered(inner: W) -> Self {
        Self {
            inner: io::BufWriter::with_capacity(EXPORT_BUFFER, inner),
            hash: Sha256::new(),
            bytes: 0,
        }
    }
    pub(super) fn finish(mut self) -> io::Result<(u64, String)> {
        // Flush every accepted byte, including a short final buffer, before
        // syncing the underlying file. Drop the buffer before building metadata.
        self.flush()?;
        self.inner.get_ref().sync_all()?;
        Ok((self.bytes, hex(&self.hash.finalize())))
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Stage {
    Data,
    BeforeManifest,
    Manifest,
}
fn exclusive(path: &Path) -> io::Result<File> {
    OpenOptions::new().write(true).create_new(true).open(path)
}
impl<C: Source> Recorder<C> {
    pub fn export(&mut self, root: &Path) -> anyhow::Result<PathBuf> {
        self.export_inner(root, None)
    }
    /// Test injection executes the production transaction up to the requested
    /// write failure. Even a complete payload without its manifest is unqualified.
    pub(super) fn export_inner(
        &mut self,
        root: &Path,
        fail: Option<Stage>,
    ) -> anyhow::Result<PathBuf> {
        self.export_with(root, fail, exclusive)
    }
    // Same production transaction with an injectable underlying durable writer.
    // Tests exercise real buffering, short writes and write/flush/sync failures.
    pub(super) fn export_with<W: DurableWrite>(
        &mut self,
        root: &Path,
        fail: Option<Stage>,
        open: impl FnOnce(&Path) -> io::Result<W>,
    ) -> anyhow::Result<PathBuf> {
        Self::inc(&mut self.export_attempts, &mut self.arithmetic_overflow);
        fs::create_dir_all(root)?;
        let run = self
            .run_id
            .map(|v| hex(&v))
            .unwrap_or_else(|| "UNKNOWN".into());
        let capture = format!("{run}-{:016x}", self.export_attempts);
        let dir = root.join(&capture);
        // Never overwrite a previous/partial capture. Ordinal increments BEFORE IO.
        fs::create_dir(&dir)?;
        let mut writer = HashWriter::buffered(open(&dir.join("records.json"))?);
        if fail == Some(Stage::Data) {
            writer.write_all(b"{\"schema\":")?;
            anyhow::bail!("injected payload write failure");
        }
        serde_json::to_writer(
            &mut writer,
            &Snapshot {
                schema: "alice-native-phase1-v1",
                run_id: self.run_id,
                clock_id: self.clock.source.domain(),
                capture_ordinal: self.export_attempts,
                schedule: &self.schedule,
                frames: &self.frames,
                worlds: &self.worlds,
                events: &self.events,
                pending: &self.pending,
                raw_counters: &self.raw_counters,
                audio_producer: &self.audio_producer,
            },
        )?;
        let (receipt_bytes, receipt_sha) = writer.finish()?;
        if fail == Some(Stage::BeforeManifest) {
            anyhow::bail!("injected failure before manifest");
        }
        // Bounded manifest (<16KiB); datasets above stream without clones/buffers.
        let mut manifest = serde_json::json!({
            "schema":"alice-native-phase1-v1", "complete":true,
            "run_id":self.run_id, "capture_id":capture, "capture_ordinal":self.export_attempts,
            "clock_id":self.clock.source.domain(), "clock_unit":"us", "clock_resolution":"Vita source microseconds; host quantized to us",
            "clock_lifetime":"Vita process lifetime OR host shared Instant session; NEVER join launches or UTC",
            "records_file":"records.json", "records_sha256":receipt_sha, "records_bytes":receipt_bytes,
            "frame_count":self.frames.len(), "world_count":self.worlds.len(), "event_count":self.events.len(),
            "viewer_seq":self.viewer_seq, "completed":self.completed, "event_total":self.event_count,
            "generation_total":self.generation, "operation_total":self.operation,
        });
        let fields = serde_json::json!({
            "first_viewer_seq":self.frames.first().map(|r|r.viewer_seq),
            "last_viewer_seq":self.frames.last().map(|r|r.viewer_seq),
            "pending_count":usize::from(self.pending.is_some()), "stopped_at_actual_begin":self.stopped,
            "capture_stop_us":self.capture_stop_us, "capture_stop_viewer_seq":self.capture_stop_viewer_seq,
            "frame_capacity":self.frames.capacity(), "world_capacity":self.worlds.capacity(), "event_capacity":self.events.capacity(),
            "frame_size":std::mem::size_of::<Frame>(), "world_size":std::mem::size_of::<World>(), "event_size":std::mem::size_of::<Event>(),
            "recorder_size":std::mem::size_of::<Self>(), "allocated_storage_bytes":self.actual_storage_bytes(),
            "aux_reserve_bytes":AUX_RESERVE, "budget_bytes":BUDGET,
        });
        if let serde_json::Value::Object(fields) = fields {
            manifest.as_object_mut().unwrap().extend(fields);
        }
        let fields = serde_json::json!({
            "lost_frames":self.lost_frames, "lost_worlds":self.lost_worlds, "lost_events":self.lost_events,
            "clock_fault":self.clock.fault, "arithmetic_overflow":self.arithmetic_overflow,
            "invalid_identity":self.invalid_identity, "metadata":self.metadata,
            "metadata_assurance":"deployment supplied source/build/game-config/start-save associations UNVERIFIED; null UNKNOWN",
            "repeated_cumulative_export":self.export_attempts != 1,
            "swap_observations":"UNAVAILABLE; never estimated from wall minus CPU",
            "door_observations":"UNIMPLEMENTED per-frame; legacy door chunks remain separate/unjoined",
            "phase2_unimplemented":([
                self.audio_producer.is_none().then_some("audio-producer-fences"),
                self.raw_counters.is_none().then_some("raw-counter-brackets"),
                self.schedule.is_none().then_some("image-tokens"), self.schedule.is_none().then_some("review-ack"), self.schedule.is_none().then_some("native-age-automation"),
                Some("pair-equivalence"), Some("complete-qualification"),
            ].into_iter().flatten().collect::<Vec<_>>()),
            "campaign_qualified":false
        });
        if let serde_json::Value::Object(fields) = fields {
            manifest.as_object_mut().unwrap().extend(fields);
        }
        if let Some(plan)=&self.schedule {
            manifest.as_object_mut().unwrap().insert("schedule_storage_bytes".into(),plan.storage_bytes().into());
            manifest.as_object_mut().unwrap().insert("schedule_token_size".into(),std::mem::size_of::<schedule::Token>().into());
        }
        if let Some(raw) = &self.raw_counters {
            manifest.as_object_mut().unwrap().insert(
                "raw_counter_storage_bytes".into(),
                raw.storage_bytes().into(),
            );
        }
        if let Some(audio) = &self.audio_producer {
            manifest
                .as_object_mut()
                .unwrap()
                .insert("audio_producer_storage_bytes".into(), audio.reserved.into());
        }
        manifest.as_object_mut().unwrap().insert(
            "producer_bootstrap_reserve_bytes".into(),
            if self.audio_producer.is_none() {
                audio_producer::BOOTSTRAP_RESERVE
            } else {
                0
            }
            .into(),
        );
        let mut out = exclusive(&dir.join("manifest.json"))?;
        if fail == Some(Stage::Manifest) {
            out.write_all(b"{\"complete\":")?;
            anyhow::bail!("injected manifest failure");
        }
        serde_json::to_writer(&mut out, &manifest)?;
        out.flush()?;
        out.sync_all()?;
        // Directory sync is platform dependent; this is completion, NOT a claim
        // of power-loss durability or an on-device/readback qualification.
        Ok(dir)
    }
}
