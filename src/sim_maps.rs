//! Exact simulation optimizations, independently selectable at startup.
//! The host probe is excluded from production packages.
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
