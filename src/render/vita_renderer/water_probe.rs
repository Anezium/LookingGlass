//! Host probe for the CPU render split of liquid batches.
//!
//! Active only in a host build with `sim_maps_probe`, and only when
//! `LOOKING_GLASS_WATER_PROBE` is set. It reads counters and clocks around work
//! that already happens; it changes no draw, uniform, order or state. Absent from
//! native builds: every entry point below is an empty inline function there.
#![allow(dead_code)]

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Frame {
    pub opaque_groups: u64,
    pub opaque_water: u64,
    pub opaque_water_triangles: u64,
    pub alpha_groups: u64,
    pub water_groups: u64,
    pub water_triangles: u64,
    pub water_parameters: u64,
    pub water_parameter_builds: u64,
    pub water_parameter_time: u64,
    pub vis_time: u64,
    pub water_vis_time: u64,
    pub parameter_time: u64,
    pub sort_time: u64,
    pub prepare_time: u64,
    pub opaque_submit_time: u64,
    pub alpha_draw_time: u64,
    pub finish_time: u64,
    pub world_submit_time: u64,
    pub water_submit_time: u64,
    pub water_pipeline_time: u64,
    pub water_binding_time: u64,
    pub water_uniform_time: u64,
    pub water_draw_time: u64,
    pub water_pipeline_applications: u64,
    pub water_binding_applications: u64,
    pub water_uniform_applications: u64,
    pub water_uniform_calls: u64,
    pub water_uniform_bytes: u64,
    pub water_draw_calls: u64,
    pub water_upload_time: u64,
    pub water_upload_bytes: u64,
    pub water_deform_groups: u64,
    pub water_turb_groups: u64,
    pub water_environment_groups: u64,
    pub water_dot_alpha_groups: u64,
    pub water_wave_normal_groups: u64,
    pub world_geometry_uploads: u64,
    pub world_retransformed: u64,
}

#[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
mod imp {
    use super::Frame;
    use std::{cell::RefCell, sync::OnceLock, time::Instant};

    fn bound(name: &str, default: u64) -> u64 {
        std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
    }
    pub(super) fn active() -> bool {
        static ON: OnceLock<bool> = OnceLock::new();
        *ON.get_or_init(|| std::env::var_os("LOOKING_GLASS_WATER_PROBE").is_some())
    }
    pub(super) fn in_window() -> bool {
        if !active() { return false; }
        static FIRST: OnceLock<u64> = OnceLock::new();
        static LAST: OnceLock<u64> = OnceLock::new();
        let first = *FIRST.get_or_init(|| bound("LOOKING_GLASS_WATER_PROBE_FIRST", 1690));
        let last = *LAST.get_or_init(|| bound("LOOKING_GLASS_WATER_PROBE_LAST", 1850));
        let frame = crate::sim_maps::probe::frame();
        frame >= first && frame <= last
    }
    thread_local! {
        static FRAME: RefCell<Frame> = RefCell::new(Frame::default());
        static START: RefCell<Option<Instant>> = const { RefCell::new(None) };
    }
    pub(super) fn begin(alpha: bool) {
        if !in_window() { return; }
        if !alpha {
            FRAME.with(|f| *f.borrow_mut() = Frame::default());
            START.with(|s| *s.borrow_mut() = Some(Instant::now()));
        }
    }
    pub(super) fn tick() -> Option<Instant> { in_window().then(Instant::now) }
    pub(super) fn add(slot: fn(&mut Frame) -> &mut u64, t: Option<Instant>) {
        if let Some(t) = t { FRAME.with(|f| *slot(&mut f.borrow_mut()) += t.elapsed().as_nanos() as u64); }
    }
    pub(super) fn add_n(slot: fn(&mut Frame) -> &mut u64, n: u64) {
        if n != 0 && in_window() { FRAME.with(|f| *slot(&mut f.borrow_mut()) += n); }
    }
    pub(super) fn finish(t: Option<Instant>) {
        if !in_window() { return; }
        let f = FRAME.with(|f| *f.borrow());
        let total = START.with(|s| s.borrow().map(|s| s.elapsed().as_nanos() as u64)).unwrap_or(0);
        let finish = t.map_or(f.finish_time, |t| t.elapsed().as_nanos() as u64);
        println!("[lg/water-probe] frame={} frame_ns={} opaque_submit_ns={} opaque_groups={} opaque_water={} \
opaque_water_triangles={} alpha_ns={} alpha_groups={} water_groups={} water_triangles={} water_parameters={} \
water_parameter_builds={} water_parameter_ns={} vis_ns={} water_vis_ns={} parameter_ns={} sort_ns={} prepare_ns={} \
finish_ns={} world_submit_ns={} water_submit_ns={} water_pipeline_ns={} water_binding_ns={} water_uniform_ns={} \
water_draw_ns={} water_pipeline_applications={} water_binding_applications={} water_uniform_applications={} \
water_uniform_calls={} water_uniform_bytes={} water_draw_calls={} water_upload_ns={} water_upload_bytes={} \
water_deform_groups={} water_turb_groups={} water_environment_groups={} water_dot_alpha_groups={} \
water_wave_normal_groups={} world_geometry_uploads={} world_retransformed={}",
            crate::sim_maps::probe::frame(), total, f.opaque_submit_time, f.opaque_groups, f.opaque_water,
            f.opaque_water_triangles, f.alpha_draw_time, f.alpha_groups, f.water_groups, f.water_triangles,
            f.water_parameters, f.water_parameter_builds, f.water_parameter_time, f.vis_time, f.water_vis_time,
            f.parameter_time, f.sort_time, f.prepare_time, finish, f.world_submit_time, f.water_submit_time,
            f.water_pipeline_time, f.water_binding_time, f.water_uniform_time, f.water_draw_time,
            f.water_pipeline_applications, f.water_binding_applications, f.water_uniform_applications,
            f.water_uniform_calls, f.water_uniform_bytes, f.water_draw_calls, f.water_upload_time,
            f.water_upload_bytes, f.water_deform_groups, f.water_turb_groups, f.water_environment_groups,
            f.water_dot_alpha_groups, f.water_wave_normal_groups, f.world_geometry_uploads,
            f.world_retransformed);
    }
}

#[cfg(not(all(feature = "sim_maps_probe", not(target_os = "vita"))))]
mod imp {
    use super::Frame;
    use std::time::Instant;
    pub(super) fn active() -> bool { false }
    pub(super) fn in_window() -> bool { false }
    pub(super) fn begin(_alpha: bool) {}
    pub(super) fn tick() -> Option<Instant> { None }
    pub(super) fn add(_slot: fn(&mut Frame) -> &mut u64, _t: Option<Instant>) {}
    pub(super) fn add_n(_slot: fn(&mut Frame) -> &mut u64, _n: u64) {}
    pub(super) fn finish(_t: Option<Instant>) {}
}

pub(super) fn active() -> bool { imp::active() }
pub(super) fn in_window() -> bool { imp::in_window() }
pub(super) fn begin(alpha: bool) { imp::begin(alpha) }
pub(super) fn tick() -> Option<std::time::Instant> { imp::tick() }
pub(super) fn add(slot: fn(&mut Frame) -> &mut u64, t: Option<std::time::Instant>) { imp::add(slot, t) }
pub(super) fn add_n(slot: fn(&mut Frame) -> &mut u64, n: u64) { imp::add_n(slot, n) }
pub(super) fn finish(t: Option<std::time::Instant>) { imp::finish(t) }
