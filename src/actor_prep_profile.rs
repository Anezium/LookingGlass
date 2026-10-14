//! Opt-in exclusive game-thread accounting for the live actor preparation probe.
//! Numeric workers never touch this thread-local state or any render handle.
#![allow(dead_code)]
use std::{cell::RefCell, time::Instant};
#[derive(Clone, Copy, Debug)]
pub(crate) enum Phase { Input, Interactions, Movers, Movement, Story, Particles, Combat,
    Alice, Audio, Camera, Scene, Gather, Discovery, Consume, Simple, Modifiers,
    Publication, Collect, Finish, Upload, Submit, Other, Support }
pub(crate) const N: usize = 23;
pub(crate) const NAMES: [&str; N] = ["input", "interactions", "movers", "movement", "story", "particles",
    "combat", "alice", "audio", "camera", "scene", "gather", "actor_discovery_setup",
    "overlap_consume_validation_copy", "simple_layer_publication", "dynamic_modifiers",
    "packet_publication", "collect_wait", "finish_bookkeeping", "upload", "actual_submission", "other", "actor_shadow_atmosphere"];
#[derive(Clone, Copy)]
pub(crate) enum Count { CopiedBytes, ComparedBytes, LogicalDraws }
/// Fixed-size value copied once after all game-thread spans have closed.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Snapshot {
    pub ns: [u64; N],
    pub calls: [u64; N],
    pub counts: [[u64; 3]; N],
}
pub(crate) fn snapshot() -> Snapshot {
    STATE.with(|s| {
        let s = s.borrow();
        debug_assert!(s.current.is_none(), "snapshot requires closed spans");
        Snapshot { ns: s.ns, calls: s.calls, counts: s.counts }
    })
}
#[derive(Default)]
struct State { current: Option<Phase>, last: Option<Instant>, ns: [u64; N],
    calls: [u64; N], counts: [[u64; 3]; N] }
impl State {
    fn checkpoint(&mut self, now: Instant) {
        if let (Some(p), Some(start)) = (self.current, self.last) {
            self.ns[p as usize] += now.duration_since(start).as_nanos() as u64;
        }
        self.last = Some(now);
    }
}
thread_local! { static STATE: RefCell<State> = RefCell::default(); }
pub(crate) fn enabled() -> bool {
    #[cfg(feature="vita_renderer")]
    { crate::render::vita_renderer::config().actor_prep_profile }
    #[cfg(not(feature="vita_renderer"))]
    { false }
}
pub(crate) struct Span { previous: Option<Phase>, active: bool }
pub(crate) fn span(phase: Phase) -> Span {
    if !enabled() { return Span { previous: None, active: false }; }
    match STATE.try_with(|s| { let mut s=s.borrow_mut(); s.checkpoint(Instant::now());
        s.calls[phase as usize] += 1; s.current.replace(phase) }) {
        Ok(previous) => Span {previous, active:true},
        Err(_) => Span {previous:None, active:false},
    }
}
/// Restrict actor publication/modifier accounting to the measured actor traversal.
/// Effects and pickup art outside that traversal retain their enclosing category.
pub(crate) fn actor_span(phase: Phase) -> Span {
    if enabled() && STATE.try_with(|s| matches!(s.borrow().current,
        Some(Phase::Gather | Phase::Discovery | Phase::Consume | Phase::Simple |
            Phase::Modifiers | Phase::Publication))).unwrap_or(false) {span(phase)}
    else {Span {previous:None, active:false}}
}
impl Drop for Span {
    fn drop(&mut self) {
        if self.active { let _=STATE.try_with(|s| { let mut s=s.borrow_mut();
            s.checkpoint(Instant::now()); s.current=self.previous; }); }
    }
}
pub(crate) fn count(kind: Count, value: usize) {
    if enabled() { let _=STATE.try_with(|s| { let mut s=s.borrow_mut();
        if let Some(p)=s.current {s.counts[p as usize][kind as usize] += value as u64;} }); }
}
pub(crate) fn reset() { if enabled() { STATE.with(|s| *s.borrow_mut()=State::default()); } }
#[cfg(not(target_os="vita"))]
pub(crate) fn report() -> serde_json::Value {
    STATE.with(|s| { let s=s.borrow(); serde_json::Value::Object(NAMES.iter().enumerate().map(|(i,n)|
        ((*n).to_owned(), serde_json::json!({"exclusive_ns":s.ns[i], "calls":s.calls[i],
            "copied_bytes":s.counts[i][0], "compared_bytes":s.counts[i][1], "logical_draws":s.counts[i][2]}))).collect()) })
}

#[cfg(all(feature="vita_renderer", not(target_os="vita")))]
pub(crate) mod replay {
    use anyhow::{ensure, Result};
    use sha2::{Digest, Sha256};
    pub(crate) struct Session { pub frame: usize, pub rows: Vec<serde_json::Value> }
    impl Session {
        pub(crate) fn open(options: &mut crate::Options) -> Result<Option<Self>> {
            if std::env::var_os("LOOKING_GLASS_ACTOR_REPLAY").is_none() {return Ok(None);}
            ensure!(super::enabled(), "live replay requires actor_prep_profile=true");
            ensure!(options.load == Some(crate::save::Slot::Auto), "live replay requires --load auto");
            ensure!(options.frames.is_some_and(|n| n >= 180), "live replay requires >=180 frames");
            ensure!(options.no_audio, "live replay requires --no-audio");
            ensure!(std::env::var_os("LOOKING_GLASS_ROUTE").is_none(), "route and actor replay are exclusive");
            crate::light_profile::set_mode(crate::frame_profile::Mode::Full);
            Ok(Some(Self {frame:0, rows:Vec::with_capacity(options.frames.unwrap() as usize)}))
        }
        pub(crate) fn dt(&self) -> f32 { dt(self.frame) }
        pub(crate) fn sample(&mut self, game: &crate::save::Game, active: bool, dt: f32,
            cpu_ns: u64, allocations: u64) -> Result<()> {
            ensure!(active, "live replay left active gameplay at frame {}", self.frame);
            ensure!(dt.to_bits()==self.dt().to_bits(), "replay dt changed");
            // Snapshot serialization and report construction are outside the CPU span.
            let state=serde_json::to_vec(&serde_json::to_value(game)?)?;
            self.rows.push(serde_json::json!({"frame":self.frame+1,"dt":dt,
                "simulation_sha256":format!("{:x}",Sha256::digest(&state)),
                "environment_clock":game.level()?.environment_clock,
                "feet":game.player.feet.to_array(),"cpu_ns":cpu_ns,"allocations":allocations,
                "exclusive":super::report(),"render":crate::light_profile::render_host_report(),
                "counts":crate::render_fx::retained::host_report()}));
            self.frame+=1;
            Ok(())
        }
        pub(crate) fn finish(&self) -> Result<()> {
            std::fs::create_dir_all("private/actor-replay")?;
            std::fs::write("private/actor-replay/samples.json",serde_json::to_vec_pretty(&serde_json::json!({
                "simulation_frozen":false,"normal_viewer_update":true,"input":"neutral",
                "dt_sequence":[1_f32/30.,0.0366_f32],"warmup_frames":60,"samples":self.rows,
                "audio_disabled":true,"save_writes":false,"console_access":false}))?)?;
            println!("PASS live actor replay: {} frames, fixed dt, normal simulation",self.rows.len());
            Ok(())
        }
    }
    fn dt(frame: usize) -> f32 { if frame%2==0 {1./30.} else {0.0366} }
    #[cfg(test)]
    mod tests {
        #[test] fn prescribed_dt_is_repeatable_and_covers_both_frame_lengths() {
            for i in 0..360 {assert_eq!(super::dt(i).to_bits(),super::dt(i+360).to_bits());}
            assert_eq!(super::dt(0),1./30.); assert_eq!(super::dt(1),0.0366);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_is_allocation_free_and_survives_source_reset() {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            *s = State::default();
            for i in 0..N {
                s.ns[i] = 100 + i as u64;
                s.calls[i] = 200 + i as u64;
                s.counts[i] = [300 + i as u64, 400 + i as u64, 500 + i as u64];
            }
        });
        let scope = crate::kernel_alloc::Scope::begin();
        let saved = snapshot();
        STATE.with(|s| *s.borrow_mut() = State::default());
        assert_eq!(scope.finish(), 0);
        for i in 0..N {
            assert_eq!(saved.ns[i], 100 + i as u64);
            assert_eq!(saved.calls[i], 200 + i as u64);
            assert_eq!(saved.counts[i], [300 + i as u64, 400 + i as u64, 500 + i as u64]);
        }
    }
    #[test] fn nested_spans_partition_time_without_counting_children_twice() {
        let t=Instant::now();let mut s=State::default();
        s.current=Some(Phase::Discovery);s.last=Some(t);
        s.checkpoint(t+std::time::Duration::from_micros(3));
        s.current=Some(Phase::Consume);s.checkpoint(t+std::time::Duration::from_micros(8));
        s.current=Some(Phase::Discovery);s.checkpoint(t+std::time::Duration::from_micros(11));
        assert_eq!(s.ns[Phase::Discovery as usize],6000);
        assert_eq!(s.ns[Phase::Consume as usize],5000);
        assert_eq!(s.ns.iter().sum::<u64>(),11000);
    }
}
