//! Exact simulation optimizations, independently selectable at startup.
//! The host probe is excluded from production packages.
macro_rules! switch {
    ($name:ident, $field:ident) => {
        pub fn $name() -> bool {
            #[cfg(feature = "vita_renderer")]
            { crate::render::vita_renderer::config().$field }
            #[cfg(not(feature = "vita_renderer"))]
            { false }
        }
    };
}
switch!(school_mesh, sim_school_mesh);
switch!(tower_templates, sim_tower_templates);
switch!(npc_queries, sim_npc_queries);
switch!(mechanism_state, sim_mechanism_state);

#[inline]
pub fn count(name: &'static str, n: u64) {
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    probe::count(name, n);
    #[cfg(not(all(feature = "sim_maps_probe", not(target_os = "vita"))))]
    let _ = (name, n);
}
#[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
pub mod probe;
