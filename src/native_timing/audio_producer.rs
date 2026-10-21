//! A single bounded journal: main -> producer -> main, never shared mutably.
use super::{
    audio_types::{ThreadPolicy, Window},
    clock::{self, Checked, Source},
    transfer as queue,
};
use serde::{
    ser::{SerializeSeq, SerializeStruct},
    Serialize,
};
use std::{
    mem::size_of,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::*},
        Arc, OnceLock,
    },
    time::{Duration, Instant},
};
#[cfg(not(target_has_atomic = "64"))]
compile_error!("producer observation requests require native 64-bit timestamp atomics");
pub const BLOCK_CAP: usize = 8192;
pub const FENCE_CAP: usize = 128;
pub const LEGACY_CAP: usize = 128;
pub const SCRATCH_CAP: usize = 512;
pub const START: u32 = 1;
pub const SEAL: u32 = 2;
pub const PENDING: u32 = 0;
pub const DELIVERED: u32 = 1;
pub const DELIVERY_FAILED: u32 = 2;
pub const ABORTED: u32 = 3;
pub const CLOCK: u32 = 1;
pub const OVERFLOW: u32 = 2;
pub const TOPOLOGY: u32 = 4;
pub const LOSS: u32 = 8;
pub const RESOLUTION: u32 = 16;
pub const PROTOCOL: u32 = 32;
pub const FIRST: u32 = 1;
pub const FINAL: u32 = 2;
pub const EMPTY: u32 = 4;

/// No registry, and no reset/reuse in production. Test universes are independent.
pub struct Universe {
    ordinal: AtomicU32,
    epoch: AtomicU32,
    active: AtomicU32,
    busy: AtomicU32,
    fault: AtomicBool,
    capture_claim: AtomicBool,
}
impl Universe {
    pub const fn new() -> Self {
        Self {
            ordinal: AtomicU32::new(0),
            epoch: AtomicU32::new(0),
            active: AtomicU32::new(0),
            busy: AtomicU32::new(0),
            fault: AtomicBool::new(false),
            capture_claim: AtomicBool::new(false),
        }
    }
    fn increment(&self, word: &AtomicU32) -> u32 {
        match word.fetch_update(SeqCst, SeqCst, |n| n.checked_add(1)) {
            Ok(n) => n + 1,
            Err(_) => {
                self.fault.store(true, SeqCst);
                0
            }
        }
    }
    fn identity(&self) -> u32 {
        self.increment(&self.ordinal)
    }
    fn change(&self, entering: bool) {
        self.increment(&self.busy);
        if entering {
            self.increment(&self.active);
        } else if self
            .active
            .fetch_update(SeqCst, SeqCst, |n| n.checked_sub(1))
            .is_err()
        {
            self.fault.store(true, SeqCst);
        }
        self.increment(&self.epoch);
        self.busy.fetch_sub(1, SeqCst);
    }
    pub fn topology(&self) -> Option<u32> {
        let busy = self.busy.load(SeqCst);
        let epoch = self.epoch.load(SeqCst);
        let active = self.active.load(SeqCst);
        let after = self.epoch.load(SeqCst);
        if busy == 0
            && self.busy.load(SeqCst) == 0
            && !self.fault.load(SeqCst)
            && active == 1
            && epoch == after
        {
            Some(epoch)
        } else {
            None
        }
    }
}
pub static UNIVERSE: Universe = Universe::new();
#[derive(Clone, Copy, Serialize)]
pub struct Stamp {
    pub pre_us: Option<u64>,
    pub post_us: Option<u64>,
    pub fault: Option<clock::Error>,
}
impl Stamp {
    pub fn read<C: Source>(clock: &mut Checked<C>) -> Self {
        let pre_us = clock.read();
        let post_us = clock.read();
        Self {
            pre_us,
            post_us,
            fault: clock.fault,
        }
    }
}
#[derive(Clone, Copy, Serialize)]
pub struct Birth {
    pub audio_id: u32,
    pub clock_id: &'static str,
    pub creation: Stamp,
}
impl Birth {
    pub fn new(universe: &Universe) -> Self {
        let mut c = Checked::new(clock::Native);
        let pre_us = c.read();
        let audio_id = universe.identity();
        universe.change(true);
        let post_us = c.read();
        Self {
            audio_id,
            clock_id: clock::DOMAIN,
            creation: Stamp {
                pre_us,
                post_us,
                fault: c.fault,
            },
        }
    }
}
/// Stored inline in the existing Metrics, never accessed in per-sample code.
pub struct AudioState {
    pub birth: Option<Birth>,
    pub universe: &'static Universe,
    pub worker_claim: AtomicBool,
    pub bootstrap: OnceLock<Arc<Bootstrap>>,
}
impl Default for AudioState {
    fn default() -> Self {
        Self {
            birth: None,
            universe: &UNIVERSE,
            worker_claim: AtomicBool::new(false),
            bootstrap: OnceLock::new(),
        }
    }
}
impl AudioState {
    pub fn mixer(universe: &'static Universe) -> Self {
        Self {
            birth: Some(Birth::new(universe)),
            universe,
            ..Self::default()
        }
    }
    pub fn mixer_dropped(&self) {
        if self.birth.is_some() {
            self.universe.change(false);
        }
    }
    pub fn claim_worker(&self) -> std::io::Result<()> {
        self.worker_claim
            .compare_exchange(false, true, SeqCst, SeqCst)
            .map(|_| ())
            .map_err(|_| std::io::Error::other("native observation: Metrics already has a worker"))
    }
}
#[derive(Clone, Copy, Serialize)]
pub struct WorkerBirth {
    pub entry: Stamp,
    pub policy: ThreadPolicy,
    pub applied: bool,
}
pub struct Bootstrap {
    pub worker_id: u32,
    pub audio: Option<Birth>,
    pub spawn: Stamp,
    pub entry: OnceLock<WorkerBirth>,
    pub control: OnceLock<Arc<Control>>,
    pub loop_closed: AtomicBool,
    pub exit: AtomicU32,
    pub joined: AtomicU32,
    pub reporter: AtomicU32,
    pub reporter_join: AtomicU32,
    pub bind_claim: AtomicBool,
    pub universe: &'static Universe,
}
impl Bootstrap {
    pub fn new(state: &AudioState) -> Self {
        let mut c = Checked::new(clock::Native);
        let pre_us = c.read();
        let worker_id = state.universe.identity();
        let post_us = c.read();
        Self {
            worker_id,
            audio: state.birth,
            spawn: Stamp {
                pre_us,
                post_us,
                fault: c.fault,
            },
            entry: OnceLock::new(),
            control: OnceLock::new(),
            loop_closed: AtomicBool::new(false),
            exit: AtomicU32::new(0),
            joined: AtomicU32::new(0),
            reporter: AtomicU32::new(0),
            reporter_join: AtomicU32::new(0),
            bind_claim: AtomicBool::new(false),
            universe: state.universe,
        }
    }
    pub fn close(&self, kind: u32) {
        self.exit.store(kind, Relaxed);
        if let Some(c) = self.control.get() {
            let _ = c
                .terminal
                .compare_exchange(PENDING, ABORTED, Release, Relaxed);
        }
        self.loop_closed.store(true, Release);
    }
}
/// Reporter closure status is distinct from the main-side join receipt.
pub struct ReporterExit {
    pub bootstrap: Arc<Bootstrap>,
    pub normal: bool,
}
impl Drop for ReporterExit {
    fn drop(&mut self) {
        self.bootstrap
            .reporter
            .store(if self.normal { 2 } else { 3 }, Release);
    }
}
/// Drop covers unwinding/early policy rejection; normal close is explicit.
pub struct ExitGuard(pub Arc<Bootstrap>);
impl Drop for ExitGuard {
    fn drop(&mut self) {
        if !self.0.loop_closed.load(Acquire) {
            self.0.close(3);
        }
    }
}
pub struct Control {
    pub run_id: Option<[u8; 16]>,
    pub clock_id: &'static str,
    pub audio_id: u32,
    pub worker_id: u32,
    pub topology: u32,
    pub request: AtomicU32,
    pub start_us: AtomicU64,
    pub start_fault: AtomicBool,
    pub terminal: AtomicU32,
    pub final_blocks: AtomicU32,
    pub final_fences: AtomicU32,
    pub final_legacy: AtomicU32,
}
impl Control {
    pub fn request_start(&self, now: Option<u64>) {
        if self.request.load(Acquire) & START == 0 {
            self.start_us.store(now.unwrap_or(0), Relaxed);
            self.start_fault.store(now.is_none(), Relaxed);
            self.request.fetch_or(START, Release);
        }
    }
}
#[derive(Clone, Copy)]
pub struct Block {
    pub begin_us: u64,
    pub end_us: u64,
    pub total_ms_bits: u64,
}
struct Blocks<'a>(&'a [Block]);
impl Serialize for Blocks<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(self.0.len()))?;
        for (i, b) in self.0.iter().enumerate() {
            #[derive(Serialize)]
            struct Row {
                seq: u32,
                begin_us: u64,
                end_us: u64,
                total_ms_bits: u64,
            }
            seq.serialize_element(&Row {
                seq: i as u32 + 1,
                begin_us: b.begin_us,
                end_us: b.end_us,
                total_ms_bits: b.total_ms_bits,
            })?;
        }
        seq.end()
    }
}
#[derive(Clone, Copy, Serialize)]
pub struct Fence {
    pub seq: u32,
    pub n: u32,
    pub begin_us: u64,
    pub end_us: u64,
    pub first_block: u32,
    pub last_block: u32,
    pub quantile_ms_bits: [u64; 3],
    pub legacy_id: u32,
    pub flags: u32,
}
#[derive(Clone, Copy, Serialize)]
pub struct Legacy {
    pub seq: u32,
    pub fence_id: u32,
    pub pre_us: u64,
    pub post_us: u64,
    pub emitted: bool,
    pub window: Window,
}
pub struct Buffers {
    pub blocks: Vec<Block>,
    pub fences: Vec<Fence>,
    pub legacy: Vec<Legacy>,
    scratch: Vec<f64>,
}
impl Buffers {
    fn new() -> Self {
        Self {
            blocks: Vec::with_capacity(BLOCK_CAP),
            fences: Vec::with_capacity(FENCE_CAP),
            legacy: Vec::with_capacity(LEGACY_CAP),
            scratch: Vec::with_capacity(SCRATCH_CAP),
        }
    }
    pub fn capacities(&self) -> [usize; 4] {
        [
            self.blocks.capacity(),
            self.fences.capacity(),
            self.legacy.capacity(),
            self.scratch.capacity(),
        ]
    }
    pub fn storage(&self) -> usize {
        self.blocks.capacity() * size_of::<Block>()
            + self.fences.capacity() * size_of::<Fence>()
            + self.legacy.capacity() * size_of::<Legacy>()
            + self.scratch.capacity() * size_of::<f64>()
    }
}
#[derive(Clone, Copy, Default, Serialize)]
pub struct Receipt {
    pub attached_us: Option<u64>,
    pub start_consumed_us: Option<u64>,
    pub final_us: Option<u64>,
    pub blocks_produced: u32,
    pub blocks_lost: u32,
    pub fences_produced: u32,
    pub fences_lost: u32,
    pub legacy_produced: u32,
    pub legacy_lost: u32,
    pub legacy_emitted: u32,
    pub legacy_queue_dropped: u32,
    pub flags: u32,
    pub clock_fault: Option<clock::Error>,
    pub final_partial: bool,
    pub no_residual_interval: bool,
    pub reason: &'static str,
}
#[derive(Clone, Copy, Serialize, PartialEq, Eq, Debug)]
pub struct Identity {
    pub run_id: Option<[u8; 16]>,
    pub clock_id: &'static str,
    pub audio_id: u32,
    pub worker_id: u32,
}
impl Control {
    fn identity(&self) -> Identity {
        Identity {
            run_id: self.run_id,
            clock_id: self.clock_id,
            audio_id: self.audio_id,
            worker_id: self.worker_id,
        }
    }
}
pub struct Completed {
    pub identity: Identity,
    pub buffers: Buffers,
    pub receipt: Receipt,
}
pub struct Start {
    pub buffers: Buffers,
    pub control: Arc<Control>,
    pub return_tx: queue::Sender<Completed>,
}
/// The endpoint lives on main until teardown, even after an exported Timeout.
#[derive(Clone, Copy, Serialize)]
pub struct Lifecycle {
    pub audio_birth: Option<Birth>,
    pub worker_spawn_request: Stamp,
    pub worker_birth: Option<WorkerBirth>,
    pub producer_loop_closed: bool,
    pub worker_exit: u32,
    pub thread_join: u32,
    pub reporter_status: u32,
    pub reporter_join: u32,
}
pub struct Owner {
    pub control: Option<Arc<Control>>,
    pub bootstrap: Option<Arc<Bootstrap>>,
    rx: Option<queue::Receiver<Completed>>,
    pub completed: Option<Completed>,
    pub status: &'static str,
    pub binding: Option<Stamp>,
    pub seal: Option<Stamp>,
    pub wait_us: u64,
    pub wait_limit_us: u64,
    pub terminal: u32,
    pub repeat_seal: bool,
    pub reserved: usize,
    pub lifecycle: Option<Lifecycle>,
    pub counter_lifetimes: [Option<[u8; 16]>; 2],
}
impl Owner {
    pub fn unavailable(reason: &'static str) -> Self {
        Self {
            control: None,
            bootstrap: None,
            rx: None,
            completed: None,
            status: reason,
            binding: None,
            seal: None,
            wait_us: 0,
            wait_limit_us: 0,
            terminal: 0,
            repeat_seal: false,
            lifecycle: None,
            counter_lifetimes: [None, None],
            reserved: size_of::<Self>(),
        }
    }
    pub fn prepare<C: Source>(
        bootstrap: Arc<Bootstrap>,
        tx: &queue::Sender<Start>,
        clock: &mut Checked<C>,
        run_id: Option<[u8; 16]>,
    ) -> Self {
        let u = bootstrap.universe;
        if bootstrap.loop_closed.load(Acquire) {
            let reason = match bootstrap.exit.load(Acquire) {
                1 => "WorkerStoppedBeforeAttachment",
                2 => "PolicyRejected",
                3 => "ProducerAborted",
                4 => "ProducerSpawnFailed",
                _ => "WorkerAlreadyClosed",
            };
            let mut owner = Self::unavailable(reason);
            owner.bootstrap = Some(bootstrap);
            owner.reserved += BOOTSTRAP_RESERVE;
            owner.freeze();
            return owner;
        }
        let Some(topology) = u.topology() else {
            return Self::unavailable("AmbiguousMixerTopology");
        };
        if bootstrap.audio.is_none() || bootstrap.worker_id == 0 {
            return Self::unavailable("UnknownAudioBirth");
        }

        if bootstrap.audio.unwrap().clock_id != clock.source.domain() {
            return Self::unavailable("ClockDomainMismatch");
        }
        if bootstrap
            .bind_claim
            .compare_exchange(false, true, SeqCst, SeqCst)
            .is_err()
        {
            return Self::unavailable("DuplicateSessionClaim");
        }
        // Irrevocable admission BEFORE any large buffer allocation, including on failure.
        if u.capture_claim
            .compare_exchange(false, true, SeqCst, SeqCst)
            .is_err()
        {
            return Self::unavailable("GlobalCaptureAlreadyClaimed");
        }
        let pre_us = clock.read();
        let control = Arc::new(Control {
            run_id,
            clock_id: clock.source.domain(),
            audio_id: bootstrap.audio.unwrap().audio_id,
            worker_id: bootstrap.worker_id,
            topology,
            request: AtomicU32::new(0),
            start_us: AtomicU64::new(0),
            start_fault: AtomicBool::new(false),
            terminal: AtomicU32::new(0),
            final_blocks: AtomicU32::new(0),
            final_fences: AtomicU32::new(0),
            final_legacy: AtomicU32::new(0),
        });
        let buffers = Buffers::new();
        let payload = buffers.storage();
        let (return_tx, rx) = queue::channel(1);
        let mut owner = Self {
            control: Some(control.clone()),
            bootstrap: Some(bootstrap.clone()),
            rx: Some(rx),
            completed: None,
            status: "Pending",
            binding: None,
            seal: None,
            wait_us: 0,
            wait_limit_us: 0,
            terminal: 0,
            repeat_seal: false,
            lifecycle: None,
            counter_lifetimes: [None, None],
            reserved: payload + FIXED_RESERVE,
        };
        let published = bootstrap.control.set(control.clone()).is_ok();
        let capacities = buffers.blocks.capacity() == BLOCK_CAP
            && buffers.fences.capacity() == FENCE_CAP
            && buffers.legacy.capacity() == LEGACY_CAP
            && buffers.scratch.capacity() == SCRATCH_CAP;
        let sent = published
            && capacities
            && tx
                .push(Start {
                    buffers,
                    control,
                    return_tx,
                })
                .is_ok();
        owner.binding = Some(Stamp {
            pre_us,
            post_us: clock.read(),
            fault: clock.fault,
        });
        if !sent {
            owner.status = "StartPublicationFailed";
        }
        owner
    }
    pub fn associate_counters(&mut self, raw: Option<&super::audio_counters::Capture>) {
        self.counter_lifetimes = raw.map_or([None, None], |r| {
            [
                r.pcm.and_then(|b| b.lifetime_id),
                r.music.and_then(|b| b.lifetime_id),
            ]
        });
    }
    pub fn request_start(&self, now: Option<u64>) {
        if let Some(c) = &self.control {
            c.request_start(now);
        }
    }
    pub fn request_seal<C: Source>(&mut self, clock: &mut Checked<C>) {
        if self.seal.is_some() {
            self.repeat_seal = true;
            return;
        }
        let pre_us = clock.read();
        if let Some(c) = &self.control {
            c.request.fetch_or(SEAL, Release);
        }
        self.seal = Some(Stamp {
            pre_us,
            post_us: clock.read(),
            fault: clock.fault,
        });
    }
    pub fn freeze(&mut self) {
        if self.lifecycle.is_none() {
            self.lifecycle = self.bootstrap.as_ref().map(|b| Lifecycle {
                audio_birth: b.audio,
                worker_spawn_request: b.spawn,
                worker_birth: b.entry.get().copied(),
                producer_loop_closed: b.loop_closed.load(Acquire),
                worker_exit: b.exit.load(Acquire),
                thread_join: b.joined.load(Acquire),
                reporter_status: b.reporter.load(Acquire),
                reporter_join: b.reporter_join.load(Acquire),
            });
        }
    }
    pub fn collect(&mut self, limit: Duration) {
        if self.status != "Pending" {
            return;
        }
        let began = Instant::now();
        let limit = limit.min(Duration::from_secs(2));
        self.wait_limit_us = limit.as_micros() as u64;
        loop {
            let c = self.control.as_ref().unwrap();
            let terminal = c.terminal.load(Acquire);
            if terminal != PENDING {
                self.terminal = terminal;
                if terminal == DELIVERED {
                    match self.rx.as_ref().unwrap().pop() {
                        Some(bundle) => {
                            let valid = bundle.identity == c.identity()
                                && bundle.receipt.blocks_produced as usize
                                    == bundle.buffers.blocks.len()
                                        + bundle.receipt.blocks_lost as usize
                                && bundle.receipt.fences_produced as usize
                                    == bundle.buffers.fences.len()
                                        + bundle.receipt.fences_lost as usize
                                && bundle.receipt.legacy_produced as usize
                                    == bundle.buffers.legacy.len()
                                        + bundle.receipt.legacy_lost as usize
                                && bundle.receipt.blocks_produced == c.final_blocks.load(Relaxed)
                                && bundle.receipt.fences_produced == c.final_fences.load(Relaxed)
                                && bundle.receipt.legacy_produced == c.final_legacy.load(Relaxed);
                            self.completed = Some(bundle);
                            self.status = if valid {
                                "Delivered"
                            } else {
                                "ProtocolFailure"
                            };
                        }
                        _ => self.status = "ProtocolFailure",
                    }
                } else {
                    self.status = if terminal == DELIVERY_FAILED {
                        "DeliveryFailed"
                    } else {
                        "Aborted"
                    };
                }
                break;
            }
            #[cfg(test)]
            if let Some(hook) = tests::COLLECT_GAP.with(|h| h.borrow_mut().take()) {
                hook();
            }
            if self.bootstrap.as_ref().unwrap().loop_closed.load(Acquire) {
                // Every producer exit publishes terminal before loop closure's
                // Release. A delivery/abort between these loads must retain its
                // actual terminal classification after this Acquire.
                if c.terminal.load(Acquire) != PENDING {
                    continue;
                }
                self.status = "NoAttachment";
                break;
            }
            if began.elapsed() >= limit {
                self.status = "Timeout";
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        self.wait_us = began.elapsed().as_micros().min(u64::MAX as u128) as u64;
    }
}
/// Charge actual payload plus all inline/control/channel/Arc/bootstrap/static
/// allocations, and a whole bounded 32KiB host worker test stack. Existing game
/// worker's 1MiB stack is preexisting, not a new allocation by this observer.
pub const DONE_ALLOCATION_BYTES: usize = (2 * size_of::<usize>() + size_of::<AtomicBool>())
    .div_ceil(std::mem::align_of::<usize>())
    * std::mem::align_of::<usize>();
pub const BOOTSTRAP_RESERVE: usize = size_of::<Capture<clock::Native>>()
    + size_of::<Bootstrap>()
    + 2 * size_of::<usize>()
    + size_of::<AudioState>()
    + size_of::<Universe>()
    + queue::allocation_bytes::<Start>(1)
    + 2 * size_of::<queue::Sender<Start>>()
    + 32 * 1024
    + DONE_ALLOCATION_BYTES
    + size_of::<ReporterExit>()
    + size_of::<Arc<Bootstrap>>()
    + size_of::<AtomicBool>()
    + size_of::<std::sync::OnceLock<std::time::Instant>>();
pub const FIXED_RESERVE: usize = BOOTSTRAP_RESERVE
    + size_of::<Owner>()
    + size_of::<Control>()
    + 2 * size_of::<usize>()
    + queue::allocation_bytes::<Completed>(1)
    + 2 * size_of::<queue::Receiver<Completed>>();
pub const MAX_STORAGE: usize = BLOCK_CAP * size_of::<Block>()
    + FENCE_CAP * size_of::<Fence>()
    + LEGACY_CAP * size_of::<Legacy>()
    + SCRATCH_CAP * size_of::<f64>()
    + FIXED_RESERVE;

pub struct Capture<C: Source> {
    pub clock: Checked<C>,
    bootstrap: Arc<Bootstrap>,
    inbox: queue::Receiver<Start>,
    active: Option<Start>,
    failed_return: Option<Completed>,
    receipt: Receipt,
    begin: Option<u64>,
    range: usize,
    closed: bool,
    #[cfg(test)]
    transfer_pause: Option<(std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>)>,
}
impl<C: Source> Capture<C> {
    pub fn new(clock: C, bootstrap: Arc<Bootstrap>, inbox: queue::Receiver<Start>) -> Self {
        Self {
            clock: Checked::new(clock),
            bootstrap,
            inbox,
            active: None,
            failed_return: None,
            receipt: Receipt::default(),
            begin: None,
            range: 0,
            closed: false,
            #[cfg(test)]
            transfer_pause: None,
        }
    }
    fn read(&mut self) -> Option<u64> {
        let v = self.clock.read();
        if v.is_none() {
            self.receipt.flags |= CLOCK;
            self.receipt.clock_fault = self.clock.fault;
        }
        v
    }
    fn inc(n: &mut u32, flags: &mut u32) {
        if let Some(v) = n.checked_add(1) {
            *n = v
        } else {
            *flags |= OVERFLOW
        }
    }
    pub fn poll(&mut self) {
        if self.closed {
            return;
        }
        if self.active.is_none() {
            if let Some(start) = self.inbox.pop() {
                self.active = Some(start);
                self.receipt.attached_us = self.read();
            }
        }
        let Some(start) = self.active.as_ref() else {
            return;
        };
        let request = start.control.request.load(Acquire);
        if request & SEAL != 0 {
            self.close("DiagnosticSeal");
            return;
        }
        if request & START != 0 && self.begin.is_none() {
            if start.control.start_fault.load(Relaxed)
                || start.control.clock_id != self.clock.source.domain()
            {
                self.receipt.flags |= CLOCK;
            }
            if self.bootstrap.universe.topology() != Some(start.control.topology) {
                self.receipt.flags |= TOPOLOGY;
            }
            let now = self.read();
            self.receipt.start_consumed_us = now;
            self.begin = now;
            // Faulted clock remains active for explicit invalid records, never restart.
            if now.is_none() {
                self.begin = Some(0);
            }
        }
    }
    pub fn block_begin(&mut self) -> Option<u64> {
        if self.begin.is_some() && !self.closed {
            self.read()
        } else {
            None
        }
    }
    pub fn block_end(&mut self, begin: Option<u64>, total: f64) {
        if self.begin.is_none() || self.closed {
            return;
        }
        let end = self.read();
        Self::inc(&mut self.receipt.blocks_produced, &mut self.receipt.flags);
        if begin.is_none() || end.is_none() {
            self.receipt.flags |= CLOCK;
        }
        let row = Block {
            begin_us: begin.unwrap_or(0),
            end_us: end.unwrap_or(0),
            total_ms_bits: total.to_bits(),
        };
        if row.begin_us > row.end_us || row.end_us <= self.begin.unwrap() {
            self.receipt.flags |= RESOLUTION;
        }
        if !total.is_finite() || total < 0. {
            self.receipt.flags |= PROTOCOL;
        }
        let b = &mut self.active.as_mut().unwrap().buffers;
        if b.blocks.len() < BLOCK_CAP {
            b.blocks.push(row);
        } else {
            Self::inc(&mut self.receipt.blocks_lost, &mut self.receipt.flags);
            self.receipt.flags |= LOSS;
        }
    }
    fn fence_at(&mut self, end: u64, legacy_id: u32, final_partial: bool) -> u32 {
        let Some(begin) = self.begin else {
            return 0;
        };
        Self::inc(&mut self.receipt.fences_produced, &mut self.receipt.flags);
        let seq = self.receipt.fences_produced;
        let b = &mut self.active.as_mut().unwrap().buffers;
        let rows = &b.blocks[self.range..];
        let n = rows.len();
        let mut flags = if seq == 1 { FIRST } else { 0 };
        if final_partial {
            flags |= FINAL;
        }
        if n == 0 {
            flags |= EMPTY;
        }
        if end <= begin {
            self.receipt.flags |= RESOLUTION;
        }
        if n > SCRATCH_CAP {
            self.receipt.flags |= LOSS;
        }
        b.scratch.clear();
        for row in rows {
            if !(begin < row.end_us && row.end_us <= end) {
                self.receipt.flags |= RESOLUTION;
            }
            if b.scratch.len() < SCRATCH_CAP {
                b.scratch.push(f64::from_bits(row.total_ms_bits));
            }
        }
        let q = if n == 0 || n > SCRATCH_CAP {
            [0; 3]
        } else {
            b.scratch.sort_unstable_by(f64::total_cmp);
            [
                b.scratch[n / 2].to_bits(),
                b.scratch[(n - 1) * 95 / 100].to_bits(),
                b.scratch[n - 1].to_bits(),
            ]
        };
        let f = Fence {
            seq,
            n: n as u32,
            begin_us: begin,
            end_us: end,
            first_block: if n == 0 { 0 } else { self.range as u32 + 1 },
            last_block: if n == 0 { 0 } else { b.blocks.len() as u32 },
            quantile_ms_bits: q,
            legacy_id,
            flags,
        };
        if b.fences.len() < FENCE_CAP {
            b.fences.push(f);
        } else {
            Self::inc(&mut self.receipt.fences_lost, &mut self.receipt.flags);
            self.receipt.flags |= LOSS;
        }
        self.range = b.blocks.len();
        self.begin = Some(end);
        seq
    }
    /// Called BEFORE the unchanged legacy swaps/loads and their emit/reset work.
    pub fn legacy_begin(&mut self) -> Option<(u32, u64)> {
        if self.begin.is_none() || self.closed {
            return None;
        }
        let end = self.read().unwrap_or(0);
        let id = self
            .receipt
            .legacy_produced
            .checked_add(1)
            .unwrap_or_else(|| {
                self.receipt.flags |= OVERFLOW;
                u32::MAX
            });
        let fence = self.fence_at(end, id, false);
        Some((fence, end))
    }
    pub fn legacy_end(&mut self, cut: Option<(u32, u64)>, window: Window, emitted: bool) {
        let Some((fence_id, pre_us)) = cut else {
            return;
        };
        let post_us = self.read().unwrap_or(0);
        Self::inc(&mut self.receipt.legacy_produced, &mut self.receipt.flags);
        if emitted {
            Self::inc(&mut self.receipt.legacy_emitted, &mut self.receipt.flags);
        } else {
            Self::inc(
                &mut self.receipt.legacy_queue_dropped,
                &mut self.receipt.flags,
            );
        }
        let b = &mut self.active.as_mut().unwrap().buffers;
        let row = Legacy {
            seq: self.receipt.legacy_produced,
            fence_id,
            pre_us,
            post_us,
            emitted,
            window,
        };
        if b.legacy.len() < LEGACY_CAP {
            b.legacy.push(row);
        } else {
            Self::inc(&mut self.receipt.legacy_lost, &mut self.receipt.flags);
            self.receipt.flags |= LOSS;
        }
    }
    pub fn close(&mut self, reason: &'static str) {
        if self.closed {
            return;
        }
        // Shutdown can discover a command queued just before stop.
        if self.active.is_none() {
            if let Some(s) = self.inbox.pop() {
                self.active = Some(s);
                self.receipt.attached_us = self.read();
            }
        }
        if self.active.is_none() {
            return;
        }
        let final_us = self.read();
        self.receipt.final_us = final_us;
        self.receipt.reason = if self.begin.is_none() {
            "SealedBeforeStart"
        } else {
            reason
        };
        if let Some(begin) = self.begin {
            let end = final_us.unwrap_or(0);
            let no_rows = self.range == self.active.as_ref().unwrap().buffers.blocks.len();
            if end == begin && no_rows {
                self.receipt.no_residual_interval = true;
            } else {
                self.fence_at(end, 0, true);
                self.receipt.final_partial = true;
            }
        }
        let start = self.active.take().unwrap();
        if self.bootstrap.universe.topology() != Some(start.control.topology) {
            self.receipt.flags |= TOPOLOGY;
        }
        if start.control.run_id.is_none()
            || start.control.audio_id == 0
            || start.control.worker_id == 0
        {
            self.receipt.flags |= PROTOCOL;
        }
        self.receipt.clock_fault = self.clock.fault;
        let bundle = Completed {
            identity: start.control.identity(),
            buffers: start.buffers,
            receipt: self.receipt,
        };
        self.closed = true;
        #[cfg(test)]
        if let Some((entered, release)) = self.transfer_pause.take() {
            entered.send(()).unwrap();
            release.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        match start.return_tx.push(bundle) {
            Ok(()) => {
                start
                    .control
                    .final_blocks
                    .store(self.receipt.blocks_produced, Relaxed);
                start
                    .control
                    .final_fences
                    .store(self.receipt.fences_produced, Relaxed);
                start
                    .control
                    .final_legacy
                    .store(self.receipt.legacy_produced, Relaxed);
                start.control.terminal.store(DELIVERED, Release);
            }
            Err(bundle) => {
                self.failed_return = Some(bundle);
                start.control.terminal.store(DELIVERY_FAILED, Release);
            }
        }
    }
}
#[derive(Serialize)]
struct Storage {
    block_capacity: usize,
    fence_capacity: usize,
    legacy_capacity: usize,
    scratch_capacity: usize,
    block_size: usize,
    fence_size: usize,
    legacy_size: usize,
    fixed_bytes: usize,
    reserved_bytes: usize,
}
impl Serialize for Owner {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = s.serialize_struct("AudioProducer", 23)?;
        out.serialize_field("schema", "alice-producer-v1")?;
        out.serialize_field("status", self.status)?;
        out.serialize_field("terminal", &self.terminal)?;
        let c = self.control.as_ref();
        out.serialize_field("run_id", &c.and_then(|v| v.run_id))?;
        out.serialize_field("clock_id", &c.map(|v| v.clock_id))?;
        out.serialize_field("audio_id", &c.map(|v| v.audio_id))?;
        out.serialize_field("worker_id", &c.map(|v| v.worker_id))?;
        out.serialize_field("topology", &c.map(|v| v.topology))?;
        out.serialize_field("lifecycle", &self.lifecycle)?;
        out.serialize_field("counter_lifetimes", &self.counter_lifetimes)?;
        out.serialize_field("binding", &self.binding)?;
        out.serialize_field(
            "start_request_us",
            &c.filter(|v| v.request.load(Acquire) & START != 0 && !v.start_fault.load(Relaxed))
                .map(|v| v.start_us.load(Relaxed)),
        )?;
        out.serialize_field("seal_request", &self.seal)?;
        out.serialize_field("wait_us", &self.wait_us)?;
        out.serialize_field("wait_limit_us", &self.wait_limit_us)?;
        out.serialize_field("repeat_seal", &self.repeat_seal)?;
        let d = self.completed.as_ref();
        out.serialize_field("completed_identity", &d.map(|v| v.identity))?;
        out.serialize_field("receipt", &d.map(|v| v.receipt))?;
        out.serialize_field(
            "blocks",
            &Blocks(d.map_or(&[], |v| v.buffers.blocks.as_slice())),
        )?;
        out.serialize_field(
            "fences",
            &d.map_or(&[][..], |v| v.buffers.fences.as_slice()),
        )?;
        out.serialize_field(
            "legacy",
            &d.map_or(&[][..], |v| v.buffers.legacy.as_slice()),
        )?;
        let storage = Storage {
            block_capacity: d.map_or(BLOCK_CAP, |v| v.buffers.blocks.capacity()),
            fence_capacity: d.map_or(FENCE_CAP, |v| v.buffers.fences.capacity()),
            legacy_capacity: d.map_or(LEGACY_CAP, |v| v.buffers.legacy.capacity()),
            scratch_capacity: d.map_or(SCRATCH_CAP, |v| v.buffers.scratch.capacity()),
            block_size: size_of::<Block>(),
            fence_size: size_of::<Fence>(),
            legacy_size: size_of::<Legacy>(),
            fixed_bytes: FIXED_RESERVE,
            reserved_bytes: self.reserved,
        };
        out.serialize_field("storage", &storage)?;
        out.serialize_field(
            "estimator",
            "sorted[n/2],sorted[(n-1)*95/100],sorted[n-1]; exact legacy total_ms f64 bits",
        )?;
        out.end()
    }
}
#[cfg(test)]
mod tests;
