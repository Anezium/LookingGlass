//! W0 dispatch diagnostics and worker preconditions.
use crate::sim_pipeline::Stamp;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Audit {
    rows: bool,
    pub(super) depth: bool,
    pub(super) snapshot_census: bool,
    outcomes: BTreeMap<&'static str, u64>,
}

impl Audit {
    pub(super) fn new() -> Self {
        Self {
            rows: cfg!(all(feature = "sim_maps_probe", not(target_os = "vita")))
                && std::env::var_os("LOOKING_GLASS_SIM_AUDIT").is_some(),
            depth: cfg!(all(feature = "sim_maps_probe", not(target_os = "vita")))
                && std::env::var_os("LOOKING_GLASS_SIM_DEPTH").is_some(),
            snapshot_census: cfg!(all(feature = "sim_maps_probe", not(target_os = "vita")))
                && std::env::var_os("LOOKING_GLASS_SNAPSHOT_FENCE_CENSUS").is_some(),
            outcomes: BTreeMap::new(),
        }
    }

    pub(super) fn diagnose(&self) -> bool {
        self.rows || self.snapshot_census
    }

    pub(super) fn event(&self, event: &str, stamp: Stamp) {
        if self.rows {
            self::event(event, stamp);
        }
    }

    pub(super) fn drain(&self, last: Option<u64>, frames: u64) {
        if self.rows {
            println!(
                "SIM_EVENT {}",
                serde_json::json!({
                    "event": "final_drain", "last_present": last, "frames": frames,
                })
            );
        }
    }

    pub(super) fn seal_consumed(&self, stamp: Stamp, check: bool) {
        if self.rows {
            println!(
                "SIM_W0_SEAL_CONSUMED frame={} tape_step={} check={}",
                stamp.frame.frame,
                stamp.frame.tape_step,
                u8::from(check)
            );
        }
    }

    pub(super) fn mid_step(&self, stamp: Stamp, reason: &'static str) {
        if self.rows {
            println!(
                "SIM_W0_DUE frame={} tape_step={} reason={reason}",
                stamp.frame.frame, stamp.frame.tape_step
            );
        }
    }

    pub(super) fn frame(
        &mut self,
        stamp: Stamp,
        overlap: bool,
        failures: &[(&'static str, bool)],
        mid_step: Option<&'static str>,
        snapshot_accepted: bool,
        free_tail: bool,
    ) {
        if !self.rows {
            return;
        }
        let failed: Vec<_> = failures
            .iter()
            .filter_map(|&(name, bad)| bad.then_some(name))
            .collect();
        let eligible = failed.is_empty();
        let dispatched = overlap && eligible;
        let reason = if !overlap {
            "disabled"
        } else {
            failed.first().copied().or(mid_step).unwrap_or("dispatched")
        };
        *self.outcomes.entry(reason).or_default() += 1;
        println!(
            "SIM_W0_FRAME {}",
            serde_json::json!({
                    "frame": stamp.frame.frame, "tape_step": stamp.frame.tape_step,
                    "visit": stamp.frame.visit_id, "generation": stamp.slot_generation,
                    "eligible": eligible, "dispatched": dispatched, "reason": reason, "failed": failed,
                    "snapshot_accepted": snapshot_accepted,
                    "worker_free_tail": dispatched && mid_step.is_none() && free_tail && snapshot_accepted,
            })
        );
    }

    pub(super) fn finish(&self, frames: u64) {
        if self.rows {
            assert_eq!(
                self.outcomes.values().sum::<u64>(),
                frames,
                "W0 frame census lost a frame"
            );
            println!(
                "SIM_W0_OUTCOMES {}",
                serde_json::json!({"frames": frames, "outcomes": self.outcomes})
            );
        }
    }
}

pub(super) fn event(event: &str, stamp: Stamp) {
    println!(
        "SIM_EVENT {}",
        serde_json::json!({
            "event": event, "frame": stamp.frame.frame, "tape_step": stamp.frame.tape_step,
            "visit": stamp.frame.visit_id, "generation": stamp.slot_generation,
            "thread": format!("{:?}", std::thread::current().id()),
        })
    );
}

/// Only control bits participate: exception status flags may differ after work.
#[cfg(target_os = "vita")]
pub(super) fn fp_control() -> u32 {
    let value: u32;
    // SAFETY: VMRS only reads the current thread's floating-point register.
    unsafe {
        core::arch::asm!("vmrs {}, fpscr", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value & 0x07ff_df00
}

#[cfg(all(not(target_os = "vita"), target_arch = "aarch64"))]
pub(super) fn fp_control() -> u32 {
    let value: u64;
    // SAFETY: MRS only reads the current thread's floating-point control register.
    unsafe {
        core::arch::asm!("mrs {}, fpcr", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value as u32
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) fn fp_control() -> u32 {
    assert!(
        std::arch::is_x86_feature_detected!("sse"),
        "W0 requires MXCSR"
    );
    let mut value = 0u32;
    // SAFETY: the SSE instruction writes one u32 to this live, aligned local.
    unsafe {
        core::arch::asm!("stmxcsr [{}]", in(reg) &mut value, options(nostack, preserves_flags));
    }
    value & !0x3f
}

#[cfg(not(any(
    target_os = "vita",
    target_arch = "aarch64",
    target_arch = "x86",
    target_arch = "x86_64"
)))]
pub(super) fn fp_control() -> u32 {
    panic!("W0 FP control readback is unavailable on this architecture");
}

#[cfg(not(target_os = "vita"))]
#[derive(Clone, Copy)]
pub(super) struct WrongStamp {
    frame: u64,
    path: &'static str,
}

#[cfg(not(target_os = "vita"))]
impl WrongStamp {
    pub(super) fn from_env() -> anyhow::Result<Option<Self>> {
        let raw = match std::env::var("LOOKING_GLASS_SIM_WRONG_STAMP") {
            Ok(raw) => raw,
            Err(std::env::VarError::NotPresent) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let (path, frame) = raw.split_once(':').unwrap_or(("output", &raw));
        let path = match path {
            "output" => "output",
            "state" => "state",
            "transport" => "transport",
            "present" => "present",
            "seal" => "seal",
            _ => anyhow::bail!("unknown W0 stamp control: {path}"),
        };
        Ok(Some(Self {
            frame: frame.parse()?,
            path,
        }))
    }

    pub(super) fn corrupt(control: Option<Self>, path: &str, mut stamp: Stamp) -> Stamp {
        if control.is_some_and(|c| c.path == path && c.frame == stamp.frame.frame) {
            stamp.slot_generation = stamp.slot_generation.wrapping_add(1);
        }
        stamp
    }
}

use std::cell::Cell;

#[derive(Clone, Copy)]
struct StackSample {
    low: usize,
    bytes: usize,
    minimum: usize,
    count: u64,
    site: &'static str,
}

thread_local! {
    static STACK: Cell<Option<StackSample>> = const { Cell::new(None) };
}

static STACK_ARMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(target_os = "vita")]
fn stack_bounds() -> Option<(usize, usize)> {
    crate::skin_worker::current_stack()
}

#[cfg(all(feature = "sim_maps_probe", target_os = "macos"))]
fn stack_bounds() -> Option<(usize, usize)> {
    // SAFETY: both pthread getters accept the current thread's live handle.
    unsafe {
        let thread = libc::pthread_self();
        let high = libc::pthread_get_stackaddr_np(thread) as usize;
        let bytes = libc::pthread_get_stacksize_np(thread);
        Some((high.checked_sub(bytes)?, bytes))
    }
}

#[cfg(all(feature = "sim_maps_probe", target_os = "linux"))]
fn stack_bounds() -> Option<(usize, usize)> {
    // SAFETY: initialize the opaque attributes before reading them and destroy
    // them on both successful and failed stack-address queries.
    unsafe {
        let mut attributes = std::mem::MaybeUninit::uninit();
        if libc::pthread_getattr_np(libc::pthread_self(), attributes.as_mut_ptr()) != 0 {
            return None;
        }
        let mut attributes = attributes.assume_init();
        let mut low = std::ptr::null_mut();
        let mut bytes = 0;
        let status = libc::pthread_attr_getstack(&attributes, &mut low, &mut bytes);
        libc::pthread_attr_destroy(&mut attributes);
        (status == 0).then_some((low as usize, bytes))
    }
}

#[cfg(not(any(
    target_os = "vita",
    all(
        feature = "sim_maps_probe",
        any(target_os = "linux", target_os = "macos")
    )
)))]
fn stack_bounds() -> Option<(usize, usize)> {
    None
}

pub(super) fn begin_stack_sampling() {
    let bounds = stack_bounds();
    if let Some((low, bytes)) = bounds {
        STACK_ARMED.store(true, std::sync::atomic::Ordering::Relaxed);
        STACK.set(Some(StackSample {
            low,
            bytes,
            minimum: bytes,
            count: 0,
            site: "",
        }));
    } else {
        eprintln!("[lg/sim-overlap] stack_bounds_unavailable");
    }
}

pub(crate) fn sample_stack(site: &'static str) {
    if !STACK_ARMED.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    if let Some(mut sample) = STACK.get() {
        let marker = 0u8;
        let here = std::hint::black_box(&marker) as *const u8 as usize;
        if let Some(margin) = here
            .checked_sub(sample.low)
            .filter(|&margin| margin < sample.bytes)
        {
            sample.count += 1;
            if margin < sample.minimum {
                sample.minimum = margin;
                sample.site = site;
            }
            STACK.set(Some(sample));
        }
    }
}

pub(super) fn stack_sample(stamp: Stamp) {
    if let Some(sample) = STACK.get() {
        println!(
            "[lg/sim-overlap] stack_sample frame={} bytes={} min_margin={} samples={} site={}",
            stamp.frame.frame, sample.bytes, sample.minimum, sample.count, sample.site
        );
    }
}
