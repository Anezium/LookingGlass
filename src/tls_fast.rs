//! `tls_fast_path`: skip thread-local reads whose result cannot change behaviour.
//! The latch is set once by the renderer config initializer from its local value;
//! before that it is false, which selects the original (slower, correct) path.
use std::sync::atomic::{AtomicBool, Ordering};

static FAST: AtomicBool = AtomicBool::new(false);

/// Called by the non-test `config()` initializer with its local `Config` value.
/// Reads no configuration itself, so it cannot re-enter `config()`'s `OnceLock`.
#[cfg_attr(any(test, not(feature = "vita_renderer")), allow(dead_code))]
pub(crate) fn init(enabled: bool) {
    FAST.store(enabled, Ordering::Relaxed);
}

#[cfg(test)]
thread_local! { static TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) }; }

/// Scoped test override; the previous value is restored on return and on unwind.
#[cfg(test)]
pub(crate) fn with_tls_fast_path<T>(on: bool, run: impl FnOnce() -> T) -> T {
    struct Reset(Option<bool>);
    impl Drop for Reset { fn drop(&mut self) { let _ = TEST_OVERRIDE.try_with(|s| s.set(self.0)); } }
    let _reset = Reset(TEST_OVERRIDE.with(|s| s.replace(Some(on))));
    run()
}

/// Whole-suite arm, read once so a later environment change cannot flip it mid-suite.
#[cfg(test)]
fn test_arm() -> bool {
    static ARM: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ARM.get_or_init(|| match std::env::var("LOOKING_GLASS_TEST_TLS_FAST_PATH").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("off") => false,
        Ok("on") => true,
        _ => panic!("LOOKING_GLASS_TEST_TLS_FAST_PATH must be off or on"),
    })
}

/// Default off pending console acceptance. Tests: a scoped override, else
/// LOOKING_GLASS_TEST_TLS_FAST_PATH=on runs the whole suite with the fast path.
#[inline]
pub(crate) fn on() -> bool {
    #[cfg(test)]
    { TEST_OVERRIDE.try_with(|s| s.get()).ok().flatten().unwrap_or_else(test_arm) }
    #[cfg(not(test))]
    { FAST.load(Ordering::Relaxed) }
}

/// The value stored by `init`, for tests of the latch itself.
#[cfg(test)]
pub(crate) fn latched() -> bool {
    FAST.load(Ordering::Relaxed)
}

/// Thread-local reads that the fast path can skip. Counted only in tests and in
/// host `sim_maps_probe` builds; production builds compile these calls away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lookup {
    /// `thread::current()` in `game_thread::assert_current`.
    ThreadCurrent,
    /// `frame_profile`'s transition-frame flag.
    TransitionFrame,
    /// `frame_profile`'s `OBJECT_TOTALS` cell in `object_timer`.
    ObjectTotals,
}
#[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
const NAMES: [[&str; 2]; 3] = [
    ["tls_fast_thread_lookups", "tls_fast_thread_lookups_skipped"],
    ["tls_fast_transition_reads", "tls_fast_transition_reads_skipped"],
    ["tls_fast_object_totals_reads", "tls_fast_object_totals_reads_skipped"],
];
#[cfg(test)]
thread_local! { static TEST_COUNTS: std::cell::Cell<[[u64; 2]; 3]> = const { std::cell::Cell::new([[0; 2]; 3]) }; }

#[inline(always)]
fn observe(kind: Lookup, skipped: bool) {
    #[cfg(test)]
    let _ = TEST_COUNTS.try_with(|c| { let mut v = c.get(); v[kind as usize][skipped as usize] += 1; c.set(v); });
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    crate::sim_maps::count(NAMES[kind as usize][skipped as usize], 1);
    let _ = (kind, skipped);
}
/// A lookup that was actually performed.
#[inline(always)]
pub(crate) fn performed(kind: Lookup) { observe(kind, false); }
/// A lookup that the fast path skipped.
#[inline(always)]
pub(crate) fn skipped(kind: Lookup) { observe(kind, true); }

/// This thread's `[performed, skipped]` counts per `Lookup`, then reset.
#[cfg(test)]
pub(crate) fn take_test_counts() -> [[u64; 2]; 3] {
    TEST_COUNTS.with(|c| c.replace([[0; 2]; 3]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessor_follows_scoped_override_and_the_suite_arm() {
        let arm = std::env::var("LOOKING_GLASS_TEST_TLS_FAST_PATH").as_deref() == Ok("on");
        assert_eq!(on(), arm);
        for requested in [false, true] {
            with_tls_fast_path(requested, || {
                assert_eq!(on(), requested);
                with_tls_fast_path(!requested, || assert_eq!(on(), !requested));
                assert_eq!(on(), requested);
            });
        }
        assert_eq!(on(), arm);
    }

    #[test]
    fn scoped_override_is_restored_on_unwind() {
        let arm = on();
        for requested in [false, true] {
            let r = std::panic::catch_unwind(|| with_tls_fast_path(requested, || -> () { assert_eq!(on(), requested); panic!("unwind through the override") }));
            assert!(r.is_err());
            assert_eq!(on(), arm);
        }
        with_tls_fast_path(!arm, || {
            let r = std::panic::catch_unwind(|| with_tls_fast_path(arm, || -> () { panic!("nested unwind") }));
            assert!(r.is_err());
            assert_eq!(on(), !arm, "inner unwind restores the outer override");
        });
    }

    #[test]
    fn override_is_thread_scoped() {
        with_tls_fast_path(true, || {
            let arm = std::env::var("LOOKING_GLASS_TEST_TLS_FAST_PATH").as_deref() == Ok("on");
            assert_eq!(std::thread::spawn(on).join().unwrap(), arm);
            assert!(on());
        });
    }

    // `init` must never reach `config()`: the initializer calls it from inside
    // config's own OnceLock. Check the source, then call it before and after config().
    #[test]
    fn init_reads_no_config_and_is_callable_before_and_after_config() {
        let source = include_str!("tls_fast.rs");
        let production = source.split("#[cfg(test)]\nmod tests").next().unwrap();
        let code: String = production.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        assert!(!code.contains("config(") && !code.contains("vita_renderer::") && !code.contains("Config"), "tls_fast must not read the renderer config");
        let renderer = include_str!("render/vita_renderer.rs");
        let initializer = renderer.split("#[cfg(not(test))]\npub fn config()").nth(1).expect("non-test config initializer");
        let initializer = initializer.split("\n}\n").next().unwrap();
        assert!(initializer.contains("crate::tls_fast::init(c.tls_fast_path);"), "config initializer latches its local value");
        let before = latched();
        init(true); assert!(latched());
        init(false); assert!(!latched());
        #[cfg(feature = "vita_renderer")]
        { let _ = crate::render::vita_renderer::config(); }
        init(true); assert!(latched());
        init(before);
    }
}
