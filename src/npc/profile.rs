//! Full-profiler-only NPC attribution. Fixed scratch; one publication per update.
//! Controller phases are disjoint: nested work temporarily replaces its caller.
use std::{
    cell::{Cell, RefCell},
    sync::atomic::{AtomicBool, Ordering::Relaxed},
    time::Instant,
};

// The viewer owns NPC updates. This gate avoids TLS/clock reads in light mode;
// thread-local state also prevents shared helpers on skin workers being charged.
static ACTIVE: AtomicBool = AtomicBool::new(false);

macro_rules! kinds {
    ($($kind:ident => $name:literal),+ $(,)?) => {
        #[derive(Clone, Copy)]
        pub(crate) enum Kind { $($kind),+ }
        const LABELS: &[crate::frame_profile::NpcLabels] = &[$(
            (concat!("npc_", $name),
             [concat!("npc_", $name, "_think"), concat!("npc_", $name, "_movement"),
              concat!("npc_", $name, "_animation"), concat!("npc_", $name, "_combat")],
             [concat!("npc_", $name, "_updated"), concat!("npc_", $name, "_sweeps"),
              concat!("npc_", $name, "_static_hulls_tested")]),
        )+];
    }
}
kinds! {
    Prepare => "collision_prepare", Garden => "garden", Companions => "companions",
    Ambush => "ambush", Wildlife => "wildlife", Plant => "plant", Card => "card",
    Snark => "snark", Insect => "insect", Magma => "magma", Diamond => "diamond",
    Boojum => "boojum", Ladybug => "ladybug", Clockwork => "clockwork",
    FireImp => "fire_imp", Pawn => "chess_pawn", Knight => "chess_knight",
    Bishop => "chess_bishop", Rook => "chess_rook", Ant => "ant", Guard => "guard",
    Ambient => "ambient",
}
#[derive(Clone, Copy)]
pub(crate) enum Phase {
    Think,
    Movement,
    Animation,
    Combat,
}

/// World-local query counts, enabled only while that world serves an NPC update.
/// Static hull visits are accumulated by the existing traversal, not per-hull calls.
#[derive(Debug, Default)]
pub(crate) struct CollisionCounts {
    active: Cell<bool>,
    sweeps: Cell<u64>,
    hulls: Cell<u64>,
}
impl CollisionCounts {
    pub(crate) fn sweep(&self) {
        if self.active.get() {
            self.sweeps.set(self.sweeps.get() + 1);
        }
    }
    pub(crate) fn active(&self) -> bool {
        self.active.get()
    }
    pub(crate) fn hulls(&self, n: u64) {
        if self.active.get() {
            self.hulls.set(self.hulls.get() + n);
        }
    }
    fn snapshot(&self) -> [u64; 2] {
        [self.sweeps.get(), self.hulls.get()]
    }
}

#[derive(Default)]
struct State {
    active: bool,
    current: Option<(usize, usize, Instant)>,
    times: [[f64; 4]; LABELS.len()],
    children: [[f64; 4]; LABELS.len()],
    counts: [[u64; 3]; LABELS.len()],
}
impl State {
    fn switch(&mut self, next: Option<(usize, usize)>, now: Instant) {
        if let Some((kind, phase, since)) = self.current.take() {
            self.times[kind][phase] += now.duration_since(since).as_secs_f64() * 1000.;
        }
        self.current = next.map(|(kind, phase)| (kind, phase, now));
    }
}
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

pub(crate) struct Frame(bool);
pub(crate) fn frame(dt: f32) -> Frame {
    let active = dt.is_finite() && dt > 0. && crate::frame_profile::enabled();
    if active {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            assert!(!s.active, "nested NPC profile frame");
            *s = State {
                active: true,
                ..State::default()
            };
        });
        ACTIVE.store(true, Relaxed);
    }
    Frame(active)
}
impl Drop for Frame {
    fn drop(&mut self) {
        if !self.0 {
            return;
        }
        ACTIVE.store(false, Relaxed);
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            debug_assert!(s.current.is_none());
            s.active = false;
            crate::frame_profile::npc_frame(LABELS, &s.times, &s.children, &s.counts);
        });
    }
}

/// Existing scoped spans are already charged to `npcs` by the full profiler.
/// Retain their attribution inside the containing phase without charging twice.
#[cfg(any(target_os = "vita", test))]
pub(crate) fn scoped_child(ms: f64) {
    if !ACTIVE.load(Relaxed) {
        return;
    }
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if let Some((kind, phase, _)) = s.current {
            s.children[kind][phase] += ms;
        }
    });
}

pub(crate) struct Controller<'a> {
    counts: &'a CollisionCounts,
    kind: Option<usize>,
    before: [u64; 2],
}
pub(crate) fn controller(kind: Kind, counts: &CollisionCounts, updated: bool) -> Controller<'_> {
    let kind = if ACTIVE.load(Relaxed) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            if !s.active {
                return None;
            }
            debug_assert!(s.current.is_none());
            s.switch(Some((kind as usize, Phase::Think as usize)), Instant::now());
            s.counts[kind as usize][0] += u64::from(updated);
            Some(kind as usize)
        })
    } else {
        None
    };
    if kind.is_some() {
        counts.active.set(true);
    }
    Controller {
        counts,
        kind,
        before: counts.snapshot(),
    }
}
impl Drop for Controller<'_> {
    fn drop(&mut self) {
        if let Some(kind) = self.kind {
            let after = self.counts.snapshot();
            self.counts.active.set(false);
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                s.switch(None, Instant::now());
                s.counts[kind][1] += after[0] - self.before[0];
                s.counts[kind][2] += after[1] - self.before[1];
            });
        }
    }
}

pub(crate) struct Work(Option<(usize, usize)>);
pub(crate) fn work(phase: Phase) -> Work {
    if !ACTIVE.load(Relaxed) {
        return Work(None);
    }
    Work(STATE.with(|s| {
        let mut s = s.borrow_mut();
        let (kind, previous, _) = s.current?;
        s.switch(Some((kind, phase as usize)), Instant::now());
        Some((kind, previous))
    }))
}
impl Drop for Work {
    fn drop(&mut self) {
        if let Some(previous) = self.0 {
            STATE.with(|s| s.borrow_mut().switch(Some(previous), Instant::now()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phases_partition_nested_work_and_collision_counts_do_not_escape_scope() {
        let mut s = State::default();
        let at = Instant::now();
        let t = |ms| at + std::time::Duration::from_millis(ms);
        s.switch(Some((0, 0)), t(0));
        s.switch(Some((0, 1)), t(2));
        s.switch(Some((0, 2)), t(5));
        s.switch(Some((0, 1)), t(9));
        s.switch(Some((0, 0)), t(12));
        s.switch(None, t(15));
        assert_eq!(s.times[0], [5., 6., 4., 0.]);
        s.children[0][1] = 2.;
        // The existing child has already charged two milliseconds to npcs.
        // The aggregate adds thirteen, so the fifteen millisecond controller
        // is charged once, and movement exclusive retains four milliseconds.
        assert_eq!(
            2. + crate::frame_profile::npc_incremental_charge(&s.times, &s.children),
            15.
        );
        assert_eq!(s.times[0][1] - s.children[0][1], 4.);
        let c = CollisionCounts::default();
        c.sweep();
        c.hulls(10);
        assert_eq!(c.snapshot(), [0, 0]);
        c.active.set(true);
        c.sweep();
        c.hulls(10);
        c.active.set(false);
        c.sweep();
        c.hulls(99);
        assert_eq!(c.snapshot(), [1, 10]);
        assert!(!frame(0.).0);
        assert!(work(Phase::Combat).0.is_none());

        // Exercise real scoped ownership, nested phases and a frame reset, with
        // no sleeps or dependence on the process-wide profiler environment.
        STATE.with(|s| {
            *s.borrow_mut() = State {
                active: true,
                ..State::default()
            }
        });
        ACTIVE.store(true, Relaxed);
        {
            let _actor = controller(Kind::Ant, &c, true);
            c.sweep();
            let _move = work(Phase::Movement);
            c.hulls(7);
            scoped_child(1e-9);
            {
                let _pose = work(Phase::Animation);
                c.sweep();
            }
        }
        c.sweep();
        STATE.with(|s| {
            let s = s.borrow();
            assert_eq!(s.counts[Kind::Ant as usize], [1, 2, 7]);
            assert_eq!(s.children[Kind::Ant as usize], [0., 1e-9, 0., 0.]);
            assert!(s.current.is_none());
            assert!(s
                .times
                .iter()
                .flatten()
                .all(|ms| ms.is_finite() && *ms >= 0.));
        });
        drop(Frame(true));
        assert!(!ACTIVE.load(Relaxed));
        assert!(work(Phase::Combat).0.is_none());
        assert!(!c.active());
    }
}
