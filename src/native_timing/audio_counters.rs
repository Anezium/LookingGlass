//! Partial audio observations. These counters are NOT producer fences or a
//! completed audio capture. Each atomic read has its own native bracket.
use super::clock::{Checked, Source};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering::SeqCst},
    Arc, OnceLock,
};

// Rust's target catalog reports native atomics for Vita; no emulated mutex.
// No explicit CAS/retry loop, clock, allocation or logging in event().
#[cfg(not(target_has_atomic = "32"))]
compile_error!("raw audio counters require native 32-bit atomics");
pub const SNAPSHOT_CAP: usize = 16;
pub const STATUS: &str = "PARTIAL_COUNTERS_ONLY_UNQUALIFIED";

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub enum Field {
    PcmStarvedBlocks,
    MusicUnderrunFrames,
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub enum Tag {
    Binding,
    Requested,
    AfterViewer,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Binding {
    pub lifetime_id: Option<[u8; 16]>,
    pub clock_id: &'static str,
    pub width_bits: u32,
    pub activation_pre_us: Option<u64>,
    pub activation_post_us: Option<u64>,
}
/// A lifetime starts at activation, which may be AFTER audio playback started.
/// Never reset, deactivate, replace, or reconstruct missed pre-activation events.
pub struct Raw {
    pub binding: Binding,
    value: AtomicU32,
    writers: AtomicU32,
    wrapped: AtomicBool,
}
impl Raw {
    pub fn activate<C: Source>(
        slot: &OnceLock<Arc<Self>>,
        clock: &mut Checked<C>,
        id: Option<[u8; 16]>,
    ) -> Arc<Self> {
        slot.get_or_init(|| {
            let pre = clock.read();
            let raw = Self {
                binding: Binding {
                    lifetime_id: id,
                    clock_id: clock.source.domain(),
                    width_bits: 32,
                    activation_pre_us: pre,
                    activation_post_us: None,
                },
                value: AtomicU32::new(0),
                writers: AtomicU32::new(0),
                wrapped: AtomicBool::new(false),
            };
            Arc::new(raw)
        })
        .clone()
    }
    #[inline]
    pub fn event(&self) {
        self.event_with(|| {});
    }
    #[inline]
    pub fn event_with(&self, legacy: impl FnOnce()) {
        // An in-flight wrap must not look like a zero total with an unset fault.
        // Busy readers fail closed. SeqCst orders the value, busy and wrap reads
        // with these three RMWs and (only on wrap) the sticky store. At most one
        // writer per actual callback/producer; concurrent callers remain safe.
        self.writers.fetch_add(1, SeqCst);
        legacy();
        if self.value.fetch_add(1, SeqCst) == u32::MAX {
            self.wrapped.store(true, SeqCst);
        }
        self.writers.fetch_sub(1, SeqCst);
    }
    fn read<C: Source>(
        &self,
        clock: &mut Checked<C>,
        ordinal: u64,
        field: Field,
        tag: Tag,
    ) -> Snapshot {
        self.read_with(clock, ordinal, field, tag, |_| {})
    }
    fn read_with<C: Source>(
        &self,
        clock: &mut Checked<C>,
        ordinal: u64,
        field: Field,
        tag: Tag,
        mut at: impl FnMut(ReadPoint),
    ) -> Snapshot {
        at(ReadPoint::BeforePre);
        let pre_us = clock.read();
        at(ReadPoint::AfterPre);
        let busy_before = self.writers.load(SeqCst) != 0;
        let value = self.value.load(SeqCst);
        at(ReadPoint::AfterValue);
        let busy_after = self.writers.load(SeqCst) != 0;
        // Read AFTER busy_after: any wrap which completed before the latter
        // read must already have published its fault. Otherwise busy rejects.
        let wrapped = self.wrapped.load(SeqCst);
        let post_us = clock.read();
        at(ReadPoint::AfterPost);
        Snapshot {
            ordinal,
            field,
            tag,
            lifetime_id: self.binding.lifetime_id,
            pre_us,
            value,
            post_us,
            busy: busy_before || busy_after,
            wrapped,
        }
    }
}
#[derive(Clone, Copy)]
enum ReadPoint {
    BeforePre,
    AfterPre,
    AfterValue,
    AfterPost,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Snapshot {
    pub ordinal: u64,
    pub field: Field,
    pub tag: Tag,
    pub lifetime_id: Option<[u8; 16]>,
    pub pre_us: Option<u64>,
    pub value: u32,
    pub post_us: Option<u64>,
    pub busy: bool,
    pub wrapped: bool,
}
#[derive(Serialize)]
pub struct Capture {
    pub status: &'static str,
    pub pcm: Option<Binding>,
    pub music: Option<Binding>,
    pub snapshots: Vec<Snapshot>,
    pub attempted: u64,
    pub lost: u64,
    pub overflow: bool,
    pub capacity: usize,
    pub snapshot_size: usize,
    pub shared_bytes: usize,
    pub flush_subset: Option<u64>,
    #[serde(skip)]
    raw: [Option<Arc<Raw>>; 2],
}
impl Capture {
    pub fn new(pcm: Option<Arc<Raw>>, music: Option<Arc<Raw>>) -> Self {
        let shared_bytes = (usize::from(pcm.is_some()) + usize::from(music.is_some()))
            * (std::mem::size_of::<Raw>() + 2 * std::mem::size_of::<usize>());
        let snapshots = Vec::with_capacity(SNAPSHOT_CAP);
        let capacity = snapshots.capacity();
        Self {
            status: STATUS,
            pcm: pcm.as_ref().map(|r| r.binding),
            music: music.as_ref().map(|r| r.binding),
            snapshots,
            attempted: 0,
            lost: 0,
            overflow: false,
            capacity,
            snapshot_size: std::mem::size_of::<Snapshot>(),
            shared_bytes,
            flush_subset: None,
            raw: [pcm, music],
        }
    }
    pub fn sample<C: Source>(&mut self, clock: &mut Checked<C>, tag: Tag) {
        // Separate brackets; NEVER a simultaneous PCM/music cut.
        for (raw, field) in self
            .raw
            .iter()
            .zip([Field::PcmStarvedBlocks, Field::MusicUnderrunFrames])
        {
            if let Some(raw) = raw {
                if tag == Tag::Binding {
                    // Confirm publication AFTER OnceLock::get_or_init returned.
                    // No timestamp from inside its initializer can prove that
                    // callbacks could already see the slot at that instant.
                    let binding = if field == Field::PcmStarvedBlocks {
                        &mut self.pcm
                    } else {
                        &mut self.music
                    };
                    binding.as_mut().unwrap().activation_post_us = clock.read();
                }
                let Some(next) = self.attempted.checked_add(1) else {
                    self.overflow = true;
                    return;
                };
                self.attempted = next;
                let snapshot = raw.read(clock, next, field, tag);
                if self.snapshots.len() < SNAPSHOT_CAP {
                    self.snapshots.push(snapshot);
                } else if let Some(n) = self.lost.checked_add(1) {
                    self.lost = n;
                } else {
                    self.overflow = true;
                }
            }
        }
    }
    pub fn storage_bytes(&self) -> usize {
        self.capacity * self.snapshot_size + self.shared_bytes
    }
}
#[cfg(test)]
mod tests;
