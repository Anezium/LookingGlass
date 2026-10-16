//! Exact simulation optimizations, independently selectable at startup.
//! The host probe is excluded from production packages.
macro_rules! switch {
    ($name:ident, $field:ident, $index:expr) => {
        pub fn $name() -> bool {
            #[cfg(test)]
            if let Some(switches) = TEST_SWITCHES.with(|s| s.get()) { return switches[$index]; }
            #[cfg(feature = "vita_renderer")]
            { crate::render::vita_renderer::config().$field }
            #[cfg(not(feature = "vita_renderer"))]
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
#[cfg(test)]
pub(crate) fn with_test_switches<T>(switches: [bool; 4], run: impl FnOnce() -> T) -> T {
    struct Reset(Option<[bool; 4]>);
    impl Drop for Reset { fn drop(&mut self) { TEST_SWITCHES.with(|s| s.set(self.0)); } }
    let _reset = Reset(TEST_SWITCHES.with(|s| s.replace(Some(switches))));
    run()
}
