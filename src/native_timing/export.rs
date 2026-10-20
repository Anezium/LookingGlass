use super::*;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

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
        #[link(name = "SceLibRng_stub")]
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
    frames: &'a [Frame],
    worlds: &'a [World],
    events: &'a [Event],
    pending: &'a Option<Frame>,
}
struct HashWriter<W> {
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
        let mut writer = HashWriter {
            inner: exclusive(&dir.join("records.json"))?,
            hash: Sha256::new(),
            bytes: 0,
        };
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
                frames: &self.frames,
                worlds: &self.worlds,
                events: &self.events,
                pending: &self.pending,
            },
        )?;
        writer.flush()?;
        writer.inner.sync_all()?;
        let receipt_bytes = writer.bytes;
        let receipt_sha = hex(&writer.hash.finalize());
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
            "phase2_unimplemented":["audio-producer-fences","raw-counter-brackets","image-tokens","review-ack","native-age-automation","pair-equivalence","complete-qualification"],
            "campaign_qualified":false
        });
        if let serde_json::Value::Object(fields) = fields {
            manifest.as_object_mut().unwrap().extend(fields);
        }
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
