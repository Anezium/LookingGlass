//! Exact simulation optimizations, independently selectable at startup.
//! The host probe is excluded from production packages.

/// Backward-compatible packet tag for legacy classes, with a separate wide namespace.
#[cfg(any(test, all(feature = "sim_maps_probe", not(target_os = "vita"))))]
pub(crate) fn world_pipeline_tag(low: bool, class: usize) -> ([u8; 9], usize) {
    let mut bytes = [0; 9];
    if let Ok(class) = u8::try_from(class) {
        bytes[0] = u8::from(low); bytes[1] = class;
        (bytes, 2)
    } else {
        bytes[0] = 0x80 | u8::from(low);
        bytes[1..].copy_from_slice(&(class as u64).to_le_bytes());
        (bytes, 9)
    }
}
macro_rules! switch {
    ($name:ident, $field:ident, $index:expr) => {
        pub fn $name() -> bool {
            #[cfg(test)]
            { test_switches()[$index] }
            #[cfg(all(not(test), feature = "vita_renderer"))]
            { crate::render::vita_renderer::config().$field }
            #[cfg(all(not(test), not(feature = "vita_renderer")))]
            { false }
        }
    };
}
switch!(school_mesh, sim_school_mesh, 0);
switch!(tower_templates, sim_tower_templates, 1);
switch!(npc_queries, sim_npc_queries, 2);
switch!(mechanism_state, sim_mechanism_state, 3);

#[inline]
pub fn count(name: &'static str, n: u64) {
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    probe::count(name, n);
    #[cfg(not(all(feature = "sim_maps_probe", not(target_os = "vita"))))]
    let _ = (name, n);
}
#[inline]
pub fn uncount(name: &'static str, n: u64) {
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    probe::uncount(name, n);
    #[cfg(not(all(feature = "sim_maps_probe", not(target_os = "vita"))))]
    let _ = (name, n);
}
/// Host-only elapsed time as a counter whose name ends in `_ns`. Nothing in a production build.
#[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
pub struct Timer(&'static str, std::time::Instant);
#[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
impl Drop for Timer { fn drop(&mut self) { probe::count(self.0, self.1.elapsed().as_nanos() as u64); } }
#[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
#[inline]
pub fn timer(name: &'static str) -> Timer { Timer(name, std::time::Instant::now()) }
#[cfg(not(all(feature = "sim_maps_probe", not(target_os = "vita"))))]
#[inline(always)]
pub fn timer(_: &'static str) {}
#[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
pub mod probe;

#[cfg(test)]
thread_local! { static TEST_SWITCHES: std::cell::Cell<Option<[bool; 4]>> = const { std::cell::Cell::new(None) }; }
/// Unit tests never consult a working-directory renderer config. A whole-suite
/// arm is selected before process startup; individual tests can override it.
#[cfg(test)]
fn test_switches() -> [bool; 4] {
    TEST_SWITCHES.with(|s| s.get()).unwrap_or_else(|| {
        static DEFAULTS: std::sync::OnceLock<[bool; 4]> = std::sync::OnceLock::new();
        *DEFAULTS.get_or_init(|| match std::env::var("LOOKING_GLASS_TEST_SIM_SWITCHES").as_deref() {
            Err(std::env::VarError::NotPresent) | Ok("off") => [false; 4],
            Ok("on") => [true; 4],
            _ => panic!("LOOKING_GLASS_TEST_SIM_SWITCHES must be off or on"),
        })
    })
}
#[cfg(test)]
pub(crate) fn with_test_switches<T>(switches: [bool; 4], run: impl FnOnce() -> T) -> T {
    struct Reset(Option<[bool; 4]>);
    impl Drop for Reset { fn drop(&mut self) { TEST_SWITCHES.with(|s| s.set(self.0)); } }
    let _reset = Reset(TEST_SWITCHES.with(|s| s.replace(Some(switches))));
    run()
}

#[test]
fn test_arm_and_nested_overrides_are_independent_of_renderer_config() {
    let current = || [school_mesh(), tower_templates(), npc_queries(), mechanism_state()];
    let expected = [std::env::var("LOOKING_GLASS_TEST_SIM_SWITCHES").as_deref() == Ok("on"); 4];
    assert_eq!(current(), expected);
    with_test_switches([true, false, true, false], || {
        assert_eq!(current(), [true, false, true, false]);
        with_test_switches([false; 4], || assert_eq!(current(), [false; 4]));
        assert_eq!(current(), [true, false, true, false]);
    });
    assert_eq!(current(), expected);
}

/// Small exact dependency keys for mechanism publication. No allocation, hash,
/// epsilon, or serialized save layout is involved.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct MechanismKey { words: [u32; 16], len: usize }
impl MechanismKey {
    pub fn new() -> Self { Self { words: [0; 16], len: 0 } }
    pub fn u(&mut self, value: u32) { self.words[self.len] = value; self.len += 1; }
    pub fn f(&mut self, value: f32) { self.u(value.to_bits()); }
    pub fn option(&mut self, value: Option<f32>) { self.u(value.map_or(u32::MAX, f32::to_bits)); }
}

// Round-two switches are independent of the accepted first-round defaults.
macro_rules! round_two_switch {
    ($name:ident, $field:ident, $index:expr) => {
        pub fn $name() -> bool {
            #[cfg(test)] { r2_test_switches()[$index] }
            #[cfg(all(not(test), feature="vita_renderer"))]
            { crate::render::vita_renderer::config().$field }
            #[cfg(all(not(test), not(feature="vita_renderer")))] { false }
        }
    };
}
round_two_switch!(triangle_candidates, sim_triangle_candidates, 0);
round_two_switch!(fan_candidates, sim_fan_candidates, 1);
round_two_switch!(merge_local_reuse, fx_merge_local_reuse, 2);
#[cfg(test)]
thread_local! { static R2_TEST: std::cell::Cell<Option<[bool; 3]>> = const { std::cell::Cell::new(None) }; }
#[cfg(test)]
fn r2_test_switches() -> [bool; 3] {
    R2_TEST.with(|s| s.get()).unwrap_or_else(|| match std::env::var("LOOKING_GLASS_TEST_SIM_R2").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("off") => [false; 3],
        Ok("on") => [true; 3],
        _ => panic!("LOOKING_GLASS_TEST_SIM_R2 must be off or on"),
    })
}
#[cfg(test)]
pub(crate) fn with_r2_switches<T>(switches: [bool; 3], run: impl FnOnce() -> T) -> T {
    struct Reset(Option<[bool; 3]>);
    impl Drop for Reset { fn drop(&mut self) { R2_TEST.with(|s| s.set(self.0)); } }
    let _reset = Reset(R2_TEST.with(|s| s.replace(Some(switches))));
    run()
}

// Round three: default on since r22 (hatter1 console ABBA). In tests an unset
// LOOKING_GLASS_TEST_SIM_R3 means the shipped default; `off` runs the suite with it off.
pub fn hatter_door_candidates() -> bool {
    #[cfg(test)]
    { match std::env::var("LOOKING_GLASS_TEST_SIM_R3").as_deref() {
        Ok("off") => false,
        Err(std::env::VarError::NotPresent) | Ok("on") => true,
        _ => panic!("LOOKING_GLASS_TEST_SIM_R3 must be off or on"),
    } }
    #[cfg(all(not(test), feature="vita_renderer"))]
    { crate::render::vita_renderer::config().sim_hatter_door_candidates }
    #[cfg(all(not(test), not(feature="vita_renderer")))]
    { false }
}

#[cfg(test)]
thread_local! { static TRIGGER_FACTS_TEST: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) }; }
#[cfg(test)]
pub(crate) fn with_trigger_facts_reuse<T>(on: bool, run: impl FnOnce() -> T) -> T {
    struct Reset(Option<bool>);
    impl Drop for Reset { fn drop(&mut self) { TRIGGER_FACTS_TEST.with(|s| s.set(self.0)); } }
    let _reset = Reset(TRIGGER_FACTS_TEST.with(|s| s.replace(Some(on))));
    run()
}
#[cfg(test)]
thread_local! { static FX_OBSERVE_MOVE_TEST: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) }; }
#[cfg(test)]
pub(crate) fn with_fx_observe_move<T>(on: bool, run: impl FnOnce() -> T) -> T {
    struct Reset(Option<bool>);
    impl Drop for Reset { fn drop(&mut self) { FX_OBSERVE_MOVE_TEST.with(|s| s.set(self.0)); } }
    let _reset = Reset(FX_OBSERVE_MOVE_TEST.with(|s| s.replace(Some(on))));
    run()
}
/// r26 CPU levers. Keep in step with Config and with tools/perf_contracts/host_flag_exact_cpu.py.
/// Tests: a scoped override, else off; LOOKING_GLASS_TEST_*=on runs the whole suite with one on.
#[cfg(test)]
pub(crate) const R26_LEVERS: [&str; 2] = ["fx_attached_persist", "mover_advance_reuse"];
#[cfg(test)]
thread_local! { static MOVER_ADVANCE_REUSE_TEST: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) }; }
#[cfg(test)]
pub(crate) fn with_mover_advance_reuse<T>(on: bool, run: impl FnOnce() -> T) -> T {
    struct Reset(Option<bool>);
    impl Drop for Reset { fn drop(&mut self) { MOVER_ADVANCE_REUSE_TEST.with(|s| s.set(self.0)); } }
    let _reset = Reset(MOVER_ADVANCE_REUSE_TEST.with(|s| s.replace(Some(on))));
    run()
}
/// The mover replay proof also checks each far step against the colliders it skipped.
#[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
pub fn mover_replay_checks() -> bool { probe::mover_replay_enabled() }
#[cfg(test)]
#[cfg(not(feature="sim_maps_probe"))]
pub fn mover_replay_checks() -> bool { true }
pub fn mover_advance_reuse() -> bool {
    #[cfg(test)]
    { MOVER_ADVANCE_REUSE_TEST.with(|s| s.get()).unwrap_or_else(|| test_lever_env("LOOKING_GLASS_TEST_MOVER_ADVANCE_REUSE")) }
    #[cfg(all(not(test), feature="vita_renderer"))]
    { crate::render::vita_renderer::config().mover_advance_reuse }
    #[cfg(all(not(test), not(feature="vita_renderer")))]
    { false }
}
/// Off unless the variable says on. Read once per lever: allocation tests run with a lever on,
/// and reading the environment allocates.
#[cfg(test)]
fn test_lever_env(name: &'static str) -> bool {
    static SEEN: std::sync::Mutex<Vec<(&'static str, bool)>> = std::sync::Mutex::new(Vec::new());
    thread_local! { static LOCAL: std::cell::Cell<[Option<bool>; 2]> = const { std::cell::Cell::new([None; 2]) }; }
    let slot = usize::from(name.ends_with("MOVER_ADVANCE_REUSE"));
    if let Some(on) = LOCAL.with(|l| l.get()[slot]) { return on; }
    let mut seen = SEEN.lock().unwrap();
    let on = seen.iter().find(|s| s.0 == name).map(|s| s.1).unwrap_or_else(|| {
        let on = match std::env::var(name).as_deref() {
            Err(std::env::VarError::NotPresent) | Ok("off") => false,
            Ok("on") => true,
            _ => panic!("{name} must be off or on"),
        };
        seen.push((name, on));
        on
    });
    LOCAL.with(|l| { let mut v = l.get(); v[slot] = Some(on); l.set(v); });
    on
}
#[cfg(test)]
thread_local! { static FX_ATTACHED_PERSIST_TEST: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) }; }
#[cfg(test)]
pub(crate) fn with_fx_attached_persist<T>(on: bool, run: impl FnOnce() -> T) -> T {
    struct Reset(Option<bool>);
    impl Drop for Reset { fn drop(&mut self) { FX_ATTACHED_PERSIST_TEST.with(|s| s.set(self.0)); } }
    let _reset = Reset(FX_ATTACHED_PERSIST_TEST.with(|s| s.replace(Some(on))));
    run()
}
pub fn fx_attached_persist() -> bool {
    #[cfg(test)]
    { FX_ATTACHED_PERSIST_TEST.with(|s| s.get()).unwrap_or_else(|| test_lever_env("LOOKING_GLASS_TEST_FX_ATTACHED_PERSIST")) }
    #[cfg(all(not(test), feature="vita_renderer"))]
    { crate::render::vita_renderer::config().fx_attached_persist }
    #[cfg(all(not(test), not(feature="vita_renderer")))]
    { false }
}
/// Attached emitters honour `sphere`. Changes pixels. Default off.
pub fn fx_attached_sphere() -> bool {
    #[cfg(feature="vita_renderer")]
    { crate::render::vita_renderer::config().fx_attached_sphere }
    #[cfg(not(feature="vita_renderer"))]
    { false }
}
/// The bubble sprite takes its image's width and a compounding scalerate. Changes pixels. Default off.
pub fn fx_bubble_true_size() -> bool {
    #[cfg(feature="vita_renderer")]
    { crate::render::vita_renderer::config().fx_bubble_true_size }
    #[cfg(not(feature="vita_renderer"))]
    { false }
}
// Default on since r25, like Config. Tests: a scoped override, else the shipped default;
// LOOKING_GLASS_TEST_FX_OBSERVE_MOVE=off runs the whole suite with the lever off.
pub fn fx_observe_move() -> bool {
    #[cfg(test)]
    { FX_OBSERVE_MOVE_TEST.with(|s| s.get()).unwrap_or_else(|| match std::env::var("LOOKING_GLASS_TEST_FX_OBSERVE_MOVE").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("on") => true,
        Ok("off") => false,
        _ => panic!("LOOKING_GLASS_TEST_FX_OBSERVE_MOVE must be off or on"),
    }) }
    #[cfg(all(not(test), feature="vita_renderer"))]
    { crate::render::vita_renderer::config().fx_observe_move }
    #[cfg(all(not(test), not(feature="vita_renderer")))]
    { false }
}
// Default on since r21, like Config. Tests: a scoped override, else
// LOOKING_GLASS_TEST_TRIGGER_FACTS=off runs the whole suite with reuse off.
pub fn trigger_facts_reuse() -> bool {
    #[cfg(test)]
    { TRIGGER_FACTS_TEST.with(|s| s.get()).unwrap_or_else(|| match std::env::var("LOOKING_GLASS_TEST_TRIGGER_FACTS").as_deref() {
        Ok("off") => false,
        Err(std::env::VarError::NotPresent) | Ok("on") => true,
        _ => panic!("LOOKING_GLASS_TEST_TRIGGER_FACTS must be off or on"),
    }) }
    #[cfg(all(not(test), feature="vita_renderer"))]
    { crate::render::vita_renderer::config().sim_trigger_facts_reuse }
    #[cfg(all(not(test), not(feature="vita_renderer")))]
    { false }
}
// Default on since r21, like Config. A whole-suite arm is chosen before startup;
// tests override it per thread, so off/on comparisons share one process.
pub fn sight_query_reuse() -> bool {
    #[cfg(test)]
    { SIGHT_TEST.with(|s| s.get()).unwrap_or_else(|| match std::env::var("LOOKING_GLASS_TEST_SIGHT_REUSE").as_deref() {
        Ok("off") => false,
        Err(std::env::VarError::NotPresent) | Ok("on") => true,
        _ => panic!("LOOKING_GLASS_TEST_SIGHT_REUSE must be off or on"),
    }) }
    #[cfg(all(not(test), feature="vita_renderer"))]
    { crate::render::vita_renderer::config().sim_sight_query_reuse }
    #[cfg(all(not(test), not(feature="vita_renderer")))]
    { false }
}
#[cfg(test)]
thread_local! { static SIGHT_TEST: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) }; }
#[cfg(test)]
pub(crate) fn with_sight_query_reuse<T>(enabled: bool, run: impl FnOnce() -> T) -> T {
    struct Reset(Option<bool>);
    impl Drop for Reset { fn drop(&mut self) { SIGHT_TEST.with(|s| s.set(self.0)); } }
    let _reset = Reset(SIGHT_TEST.with(|s| s.replace(Some(enabled))));
    run()
}

/// Direct SAH traversal for yes/no sweeps; exact legacy admission, no ordered candidate buffer.
pub fn sweep_any() -> bool {
    #[cfg(test)]
    { SWEEP_ANY_TEST.with(|s| s.get()) }
    #[cfg(all(not(test), feature="vita_renderer"))]
    { crate::render::vita_renderer::config().sim_sweep_any }
    #[cfg(all(not(test), not(feature="vita_renderer")))]
    { false }
}
#[cfg(test)]
thread_local! { static SWEEP_ANY_TEST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
#[cfg(test)]
pub(crate) fn with_sweep_any<T>(enabled: bool, run: impl FnOnce() -> T) -> T {
    struct Reset(bool);
    impl Drop for Reset { fn drop(&mut self) { SWEEP_ANY_TEST.with(|s| s.set(self.0)); } }
    let _reset = Reset(SWEEP_ANY_TEST.with(|s| s.replace(enabled)));
    run()
}

/// `sim_sweep_blocked`: yes/no collision questions stop at the first blocking hull. Off by default.
pub fn sweep_blocked() -> bool {
    #[cfg(test)]
    { SWEEP_BLOCKED_TEST.with(|s| s.get()) }
    #[cfg(all(not(test), feature="vita_renderer"))]
    { crate::render::vita_renderer::config().sim_sweep_blocked }
    #[cfg(all(not(test), not(feature="vita_renderer")))]
    { false }
}
#[cfg(test)]
thread_local! { static SWEEP_BLOCKED_TEST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
#[cfg(test)]
pub(crate) fn with_sweep_blocked<T>(enabled: bool, run: impl FnOnce() -> T) -> T {
    struct Reset(bool);
    impl Drop for Reset { fn drop(&mut self) { SWEEP_BLOCKED_TEST.with(|s| s.set(self.0)); } }
    let _reset = Reset(SWEEP_BLOCKED_TEST.with(|s| s.replace(enabled)));
    run()
}

// Off by default. Same arrangement as sight_query_reuse, for the off/on comparison in one process.
pub fn sweep_memo() -> bool {
    #[cfg(test)]
    { SWEEP_MEMO_TEST.with(|s| s.get()).unwrap_or_else(|| match std::env::var("LOOKING_GLASS_TEST_SWEEP_MEMO").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("off") => false,
        Ok("on") => true,
        _ => panic!("LOOKING_GLASS_TEST_SWEEP_MEMO must be off or on"),
    }) }
    #[cfg(all(not(test), feature="vita_renderer"))]
    { crate::render::vita_renderer::config().sim_sweep_memo }
    #[cfg(all(not(test), not(feature="vita_renderer")))]
    { false }
}
#[cfg(test)]
thread_local! { static SWEEP_MEMO_TEST: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) }; }
#[cfg(test)]
pub(crate) fn with_sweep_memo<T>(enabled: bool, run: impl FnOnce() -> T) -> T {
    struct Reset(Option<bool>);
    impl Drop for Reset { fn drop(&mut self) { SWEEP_MEMO_TEST.with(|s| s.set(self.0)); } }
    let _reset = Reset(SWEEP_MEMO_TEST.with(|s| s.replace(Some(enabled))));
    run()
}

// Independent light-cache and dynamic-bucket controls; runtime defaults live in Config.
macro_rules! hedge_switch {
    ($name:ident, $field:ident) => {
        pub fn $name() -> bool {
            #[cfg(test)] { std::env::var("LOOKING_GLASS_TEST_HEDGE3").as_deref() == Ok("on") }
            #[cfg(all(not(test), feature="vita_renderer"))]
            { crate::render::vita_renderer::config().$field }
            #[cfg(all(not(test), not(feature="vita_renderer")))] { false }
        }
    };
}
hedge_switch!(lights_static_cache, lights_static_cache);
hedge_switch!(dynamic_buckets, sim_dynamic_buckets);

macro_rules! think_switch {
    ($name:ident, $field:ident, $index:expr) => {
        pub fn $name() -> bool {
            #[cfg(test)] { think_test_switches()[$index] }
            #[cfg(all(not(test), feature="vita_renderer"))]
            { crate::render::vita_renderer::config().$field }
            #[cfg(all(not(test), not(feature="vita_renderer")))] { false }
        }
    };
}
think_switch!(npc_sight_candidates, sim_npc_sight_candidates, 0);
think_switch!(npc_sight_cache, sim_npc_sight_cache, 1);
think_switch!(npc_target_stack, sim_npc_target_stack, 2);
#[cfg(test)]
thread_local! { static THINK_TEST: std::cell::Cell<Option<[bool; 3]>> = const { std::cell::Cell::new(None) }; }
#[cfg(test)]
fn think_test_switches() -> [bool; 3] {
    THINK_TEST.with(|s| s.get()).unwrap_or_else(|| match std::env::var("LOOKING_GLASS_TEST_NPC_THINK").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("off") => [false; 3],
        Ok("on") => [true; 3],
        _ => panic!("LOOKING_GLASS_TEST_NPC_THINK must be off or on"),
    })
}
#[cfg(test)]
pub(crate) fn with_think_switches<T>(switches: [bool; 3], run: impl FnOnce() -> T) -> T {
    struct Reset(Option<[bool; 3]>);
    impl Drop for Reset { fn drop(&mut self) { THINK_TEST.with(|s| s.set(self.0)); } }
    let _reset = Reset(THINK_TEST.with(|s| s.replace(Some(switches))));
    run()
}
