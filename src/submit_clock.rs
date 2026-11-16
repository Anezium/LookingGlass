//! Default-off main-thread submission census, buffered by spike_log. Inclusive scopes
//! may nest: report joint_finish/sky/water as parents/subsets, never sum all columns.
use std::{cell::RefCell, fmt::Write, time::Instant};
pub(crate) const NAMES: &[&str] = &[
    "world_opaque", "world_visibility", "world_parameters", "alpha_prepare",
    "joint_signature", "joint_plan", "joint_copy", "joint_observe", "joint_recycle",
    "actor_gather", "actor_collect", "actor_publication", "world", "sky", "world_alpha", "water",
    "joint_finish", "joint_sort", "joint_prepare_merge", "present_prep",
    "actor_upload", "actor_uniform_build", "actor_state", "actor_bindings", "actor_uniform_apply", "actor_draw",
    "fx_upload", "fx_uniform_build", "fx_state", "fx_bindings", "fx_uniform_apply", "fx_draw",
    "world_state", "world_uniform_apply", "world_draw", "fx_gather", "render_setup", "first_person",
];
thread_local! { static NS: RefCell<[u64; NAMES.len()]> = const { RefCell::new([0; NAMES.len()]) }; }
#[inline] fn enabled() -> bool {
    #[cfg(feature="vita_renderer")]
    { let c=crate::render::vita_renderer::config(); c.diag_submit_timing && c.spike_log_ms > 0. }
    #[cfg(not(feature="vita_renderer"))]
    { false }
}
pub(crate) struct Scope { slot: usize, start: Option<Instant> }
#[inline] pub(crate) fn scope(name: &'static str) -> Scope {
    if !enabled() { return Scope { slot: 0, start: None }; }
    Scope { slot: NAMES.iter().position(|&s|s==name).expect("submission clock name"), start: Some(Instant::now()) }
}
impl Drop for Scope {
    #[inline] fn drop(&mut self) {
        if let Some(start)=self.start { let ns=start.elapsed().as_nanos() as u64; NS.with(|s|s.borrow_mut()[self.slot]+=ns); }
    }
}
pub(crate) fn reset() { if enabled() { NS.with(|s|s.borrow_mut().fill(0)); } }
pub(crate) fn write(out: &mut String) {
    if !enabled() { return; }
    NS.with(|s| {
        out.push_str(" submit_cpu=");
        for (i,(&name,&ns)) in NAMES.iter().zip(s.borrow().iter()).enumerate() {
            let _=write!(out,"{}{name}:{:.3}",if i==0 {""} else {","},ns as f64/1e6);
        }
    });
}
