//! Host-only audio exactness trace.
//!
//! Enabled by `LOOKING_GLASS_AUDIO_PROOF=1` in a `sim_maps_probe` host build and
//! never compiled on Vita. `AUDIO_DECODE` records the decoded mono samples of every
//! clip that enters the cache; `AUDIO_MIX` records the actual mixed sample stream the
//! silent mixer produced for one frame. Two runs that agree on every frame's mixed
//! samples and on each path's decoded samples have identical audio output.
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::sync::OnceLock;

struct Mix {
    samples: u64,
    hash: Sha256,
}
thread_local! {
    static MIX: RefCell<Option<Mix>> = const { RefCell::new(None) };
}

pub(super) fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LOOKING_GLASS_AUDIO_PROOF").is_some())
}

/// One mixed sample from the deterministic host mixer pump.
pub(super) fn mix_sample(value: f32) {
    if !enabled() {
        return;
    }
    MIX.with(|mix| {
        let mut mix = mix.borrow_mut();
        let entry = mix.get_or_insert_with(|| Mix {
            samples: 0,
            hash: Sha256::new(),
        });
        entry.hash.update(value.to_bits().to_le_bytes());
        entry.samples += 1;
    });
}

/// Emit the samples accumulated for the current preparation pass.
pub(super) fn mix_end() {
    if !enabled() {
        return;
    }
    let frame = crate::sim_maps::probe::frame();
    MIX.with(|mix| {
        if let Some(entry) = mix.borrow_mut().take() {
            println!(
                "AUDIO_MIX frame={frame} samples={} sha256={:x}",
                entry.samples,
                entry.hash.finalize()
            );
        }
    });
}

/// One decoded clip entering the level cache. `origin` is preload, prefetch,
/// runtime or loop; the path and samples are what playback consumes.
pub(super) fn decode(origin: &str, path: &str, samples: &[f32]) {
    if !enabled() {
        return;
    }
    let mut hash = Sha256::new();
    for sample in samples {
        hash.update(sample.to_bits().to_le_bytes());
    }
    println!(
        "AUDIO_DECODE frame={} origin={origin} path={path} samples={} sha256={:x}",
        crate::sim_maps::probe::frame(),
        samples.len(),
        hash.finalize()
    );
}
