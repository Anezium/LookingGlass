//! Default-off census of the main-thread `actor_gather` region. A5, 2026-11-16.
//!
//! The console already brackets gather as a whole (`phase_census`) and exports a 180-frame
//! JSON table. This module answers a narrower question for one replay window: how the
//! main-thread gather time divides between story pose, `Frame::begin`, Alice, the NPC
//! traversal (by kind), level art and dispatch, and how many actors, surfaces, lane jobs,
//! serial jobs and bones each frame carried. It keeps only integer sums and emits one
//! `[lg/spike] gather ...` mean line for the window, next to the existing spike lines.
//!
//! `gather_census` (config) is false in every shipped configuration. Nothing below runs
//! then beyond one cached config read per span call site. No allocation, I/O, GL call or
//! lock happens inside a measured frame: the summary string is reserved on the first
//! counted frame. The elapsed values are wall-clock brackets like `phase_census`: they
//! include preemption and API waits, and are not GPU times.
#![allow(dead_code)]
use std::cell::RefCell;
use std::time::Instant;

/// Fixed leaves; the NPC kinds follow them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum Leaf {
    StoryPose = 0,
    FrameBegin,
    AliceTraversal,
    NpcTraversal,
    LevelArt,
    Dispatch,
    FrameBeginPublication,
    FrameBeginReset,
    FrameBeginPrepared,
    PoseEval,
    Palette,
    SkinPrepare,
    SerialSkin,
    SerialFallback,
    NpcOverlay,
    NpcAttachment,
    DispatchHold,
    DispatchLanes,
    NpcKindBase,
}
/// One row per NPC controller kind; `src/npc/profile.rs` owns the names.
pub(crate) const KINDS: usize = 22;
pub(crate) const LEAVES: usize = Leaf::NpcKindBase as usize + KINDS;
const FIXED_NAMES: [&str; Leaf::NpcKindBase as usize] = [
    "story_pose", "frame_begin", "alice", "npc", "level_art", "dispatch",
    "begin_publication", "begin_reset", "begin_prepared",
    "pose_eval", "palette", "skin_prepare", "serial_skin", "serial_fallback",
    "npc_overlay", "npc_attachment", "dispatch_hold", "dispatch_lanes",
];
const ROOT_STORY: u8 = 1 << 0;
const ROOT_BEGIN: u8 = 1 << 1;
const ROOT_ALICE: u8 = 1 << 2;
const ROOT_NPC: u8 = 1 << 3;
const ROOT_ART: u8 = 1 << 4;
const ROOT_DISPATCH: u8 = 1 << 5;
const ANY_TRAVERSAL: u8 = ROOT_ALICE | ROOT_NPC | ROOT_ART;
/// Bracket depth; the measured tree is a root and at most two child levels.
const DEPTH: usize = 8;
const COUNTERS: [&str; 16] = [
    "actors_visited", "actors_drawn", "actors_culled", "surfaces_skinned",
    "vertices_skinned", "bones_captured", "lane_jobs", "serial_surfaces",
    "pose_evaluations", "pose_hits", "bones_evaluated", "sample_evaluations",
    "sample_hits", "overlays", "attachments", "serial_fallbacks",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum Counter {
    ActorsVisited = 0,
    ActorsDrawn,
    ActorsCulled,
    SurfacesSkinned,
    VerticesSkinned,
    BonesCaptured,
    LaneJobs,
    SerialSurfaces,
    PoseEvaluations,
    PoseHits,
    BonesEvaluated,
    SampleEvaluations,
    SampleHits,
    Overlays,
    Attachments,
    SerialFallbacks,
}

#[inline]
fn is_root(leaf: usize) -> bool { leaf <= Leaf::Dispatch as usize }
#[inline]
fn root_bit(leaf: usize) -> u8 {
    match leaf {
        x if x == Leaf::StoryPose as usize => ROOT_STORY,
        x if x == Leaf::FrameBegin as usize => ROOT_BEGIN,
        x if x == Leaf::AliceTraversal as usize => ROOT_ALICE,
        x if x == Leaf::NpcTraversal as usize => ROOT_NPC,
        x if x == Leaf::LevelArt as usize => ROOT_ART,
        _ => ROOT_DISPATCH,
    }
}
#[inline]
fn allowed_roots(leaf: usize) -> u8 {
    if is_root(leaf) { return root_bit(leaf); }
    match leaf {
        x if x == Leaf::FrameBeginPublication as usize
            || x == Leaf::FrameBeginReset as usize
            || x == Leaf::FrameBeginPrepared as usize => ROOT_BEGIN,
        x if x == Leaf::DispatchHold as usize || x == Leaf::DispatchLanes as usize => ROOT_DISPATCH,
        x if x == Leaf::NpcOverlay as usize
            || x == Leaf::NpcAttachment as usize
            || x >= Leaf::NpcKindBase as usize => ROOT_NPC,
        _ => ANY_TRAVERSAL,
    }
}

struct State {
    ns: [u64; LEAVES],
    kids: [u64; LEAVES],
    calls: [u64; LEAVES],
    counters: [u64; COUNTERS.len()],
    kind_calls: [u64; KINDS],
    frames: u64,
    measured: u64,
    armed: bool,
    finished: bool,
    touched: bool,
    window_first: u64,
    window_last: u64,
    stack: [usize; DEPTH],
    depth: usize,
    root_mask: u8,
    errors: u64,
}
impl State {
    const ZERO: Self = Self {
        ns: [0; LEAVES], kids: [0; LEAVES], calls: [0; LEAVES],
        counters: [0; COUNTERS.len()], kind_calls: [0; KINDS],
        frames: 0, measured: 0, armed: false, finished: false, touched: false,
        window_first: 0, window_last: 0,
        stack: [0; DEPTH], depth: 0, root_mask: 0, errors: 0,
    };
    fn open(&mut self, first: u64, last: u64) {
        *self = Self::ZERO;
        self.armed = true;
        self.window_first = first;
        self.window_last = last;
    }
}
thread_local! { static STATE: RefCell<State> = const { RefCell::new(State::ZERO) }; }

#[inline]
pub(crate) fn on() -> bool {
    #[cfg(test)]
    if FORCE.with(|f| f.get()) { return true; }
    #[cfg(feature = "vita_renderer")]
    { crate::render::vita_renderer::config().gather_census }
    #[cfg(not(feature = "vita_renderer"))]
    { false }
}
#[cfg(test)]
thread_local! { static FORCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
#[cfg(test)]
pub(crate) fn test_force(value: bool) { FORCE.with(|f| f.set(value)); }
/// True while a counted window is open.
pub(crate) fn active() -> bool {
    if !on() { return false; }
    STATE.with(|s| { let s = s.borrow(); s.armed && !s.finished })
}
pub(crate) struct Scope { leaf: usize, start: Option<Instant>, level: usize }
impl Scope {
    const OFF: Self = Self { leaf: 0, start: None, level: 0 };
}
fn enter(leaf: usize, root_allowed: u8) -> Scope {
    let mut scope = Scope::OFF;
    if !on() { return scope; }
    scope.leaf = leaf;
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if !s.armed || s.finished || s.depth == DEPTH { return; }
        if is_root(leaf) {
            if s.root_mask != 0 { return; }
        } else if s.root_mask & root_allowed == 0 {
            return;
        }
        let d = s.depth;
        s.stack[d] = leaf;
        s.depth += 1;
        if is_root(leaf) { s.root_mask |= root_bit(leaf); }
        scope.level = d;
        scope.start = Some(Instant::now());
    });
    scope
}
fn leave(leaf: usize, level: usize, start: Option<Instant>) {
    let Some(start) = start else { return; };
    let elapsed = start.elapsed().as_nanos() as u64;
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.depth != level + 1 || s.stack[level] != leaf { s.errors += 1; return; }
        s.depth = level;
        if is_root(leaf) { s.root_mask &= !root_bit(leaf); s.touched = true; }
        s.ns[leaf] += elapsed;
        s.calls[leaf] += 1;
        for i in 0..level {
            let parent = s.stack[i];
            s.kids[parent] += elapsed;
        }
    });
}
impl Drop for Scope {
    fn drop(&mut self) { leave(self.leaf, self.level, self.start.take()); }
}
pub(crate) fn span(leaf: Leaf) -> Scope {
    let index = leaf as usize;
    enter(index, allowed_roots(index))
}
/// One actor traversal, attributed to its controller kind. `kind` indexes `profile::KINDS`.
pub(crate) struct KindScope { leaf: usize, start: Option<Instant>, level: usize }
impl Drop for KindScope {
    fn drop(&mut self) { leave(self.leaf, self.level, self.start.take()); }
}
pub(crate) fn kind(kind: usize) -> KindScope {
    if kind >= KINDS { return KindScope { leaf: 0, start: None, level: 0 }; }
    let leaf = Leaf::NpcKindBase as usize + kind;
    let scope = enter(leaf, ROOT_NPC);
    if scope.start.is_some() {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.kind_calls[kind] += 1;
            s.counters[Counter::ActorsVisited as usize] += 1;
        });
    }
    KindScope { leaf: scope.leaf, start: scope.start, level: scope.level }
}
pub(crate) fn count(counter: Counter) { count_n(counter, 1); }
pub(crate) fn count_n(counter: Counter, value: u64) {
    if value == 0 || !on() { return; }
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.armed && !s.finished && s.root_mask != 0 { s.counters[counter as usize] += value; }
    });
}
/// Names of the fixed leaves then the NPC kinds; `profile::name` supplies the kind names.
fn write_summary(s: &State, out: &mut String) {
    use std::fmt::Write as _;
    let _ = write!(out, "[lg/spike] gather window={}..{} frames={} measured={}",
        s.window_first, s.window_last, s.frames, s.measured);
    let leaves = FIXED_NAMES.len() + KINDS;
    let mean = |ns: u64| if s.frames == 0 { 0. } else { ns as f64 / s.frames as f64 / 1e6 };
    let mut first = true;
    for i in 0..leaves {
        if s.calls[i] == 0 && s.ns[i] == 0 { continue; }
        let name = if i < FIXED_NAMES.len() { FIXED_NAMES[i] } else { crate::npc::profile::name(i - FIXED_NAMES.len()) };
        let _ = write!(out, "{}{}={:.4}/{:.4}/{}", if first { " leaf=" } else { "," },
            name, mean(s.ns[i]), mean(s.ns[i].saturating_sub(s.kids[i])), s.calls[i]);
        first = false;
    }
    if first { out.push_str(" leaf=-"); }
    let mut first = true;
    for (i, name) in COUNTERS.iter().enumerate() {
        let _ = write!(out, "{}{}={:.3}", if first { " counter=" } else { "," },
            name, s.counters[i] as f64 / s.frames.max(1) as f64);
        first = false;
    }
    let _ = write!(out, " errors={}", s.errors);
}
/// Called from `spike_log::begin_frame` with the number of the frame that just closed.
/// Arms the window one frame before its first measured frame, and hands the single
/// summary line to `push` once the last frame closed.
pub(crate) fn frame_closed(number: u64, push: impl FnOnce(&str)) {
    if !on() { return; }
    let mut line = String::new();
    let mut emit = false;
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.finished { return; }
        if !s.armed {
            let (first, last) = {
                #[cfg(feature = "vita_renderer")]
                { let c = crate::render::vita_renderer::config();
                  (u64::from(c.spike_log_first_frame), u64::from(c.spike_log_last_frame)) }
                #[cfg(not(feature = "vita_renderer"))]
                { (0u64, 0u64) }
            };
            if last == 0 { return; }
            if number < first.saturating_sub(1) { return; }
            s.open(first, last);
        }
        if number < s.window_first || number > s.window_last { return; }
        s.frames += 1;
        if s.root_mask != 0 || s.depth != 0 { s.errors += 1; }
        if s.touched { s.measured += 1; s.touched = false; }
        if number == s.window_last {
            write_summary(&s, &mut line);
            s.finished = true;
            emit = true;
        }
    });
    if emit { push(&line); }
}

#[cfg(test)]
pub(crate) fn test_open(first: u64, last: u64) { STATE.with(|s| s.borrow_mut().open(first, last)); }
#[cfg(test)]
pub(crate) fn test_summary() -> String {
    STATE.with(|s| { let mut out = String::new(); write_summary(&s.borrow(), &mut out); out })
}
#[cfg(test)]
pub(crate) fn test_counters() -> [u64; COUNTERS.len()] { STATE.with(|s| s.borrow().counters) }
#[cfg(test)]
pub(crate) fn test_errors() -> u64 { STATE.with(|s| s.borrow().errors) }

#[cfg(test)]
mod tests {
    use super::*;
    fn field(text: &str, name: &str) -> f64 {
        text.split_whitespace().find(|f| f.starts_with(&format!("{name}=")))
            .unwrap_or_else(|| panic!("{name} in {text}"))
            .split('=').nth(1).unwrap().split('/').next().unwrap().parse().unwrap()
    }
    #[test]
    fn inactive_gather_keeps_no_time_or_counters() {
        test_force(false);
        STATE.with(|s| *s.borrow_mut() = State::ZERO);
        assert!(!active());
        { let _s = span(Leaf::StoryPose); }
        count(Counter::ActorsVisited);
        STATE.with(|s| { let s = s.borrow();
            assert!(s.ns.iter().all(|&n| n == 0) && s.counters.iter().all(|&n| n == 0));
            assert_eq!(s.errors, 0);
        });
    }
    #[test]
    fn children_are_excluded_from_the_enclosing_root() {
        test_force(true);
        test_open(10, 12);
        {
            let _root = span(Leaf::NpcTraversal);
            std::thread::sleep(std::time::Duration::from_millis(3));
            {
                let _k = kind(3);
                std::thread::sleep(std::time::Duration::from_millis(4));
            }
        }
        let text = test_summary();
        assert!(text.contains("npc="), "{text}");
        let npc = field(&text, "npc");
        let ant = field(&text, "ant");
        assert!(npc > ant && npc > 5.0, "{text}");
        assert_eq!(test_errors(), 0);
        assert_eq!(test_counters()[Counter::ActorsVisited as usize], 1);
    }
    #[test]
    fn kind_outside_the_npc_traversal_is_ignored() {
        test_force(true);
        test_open(10, 12);
        { let _k = kind(0); }
        STATE.with(|s| { let s = s.borrow(); assert!(s.ns.iter().all(|&n| n == 0)); });
    }
    #[test]
    fn generic_leaves_need_a_traversal_root() {
        test_force(true);
        test_open(10, 12);
        { let _p = span(Leaf::PoseEval); }
        STATE.with(|s| { let s = s.borrow(); assert_eq!(s.ns[Leaf::PoseEval as usize], 0); });
        {
            let _a = span(Leaf::AliceTraversal);
            let _p = span(Leaf::PoseEval);
        }
        STATE.with(|s| { let s = s.borrow(); assert!(s.ns[Leaf::PoseEval as usize] > 0); });
    }
    #[test]
    fn the_window_emits_one_line_and_then_stops() {
        test_force(true);
        test_open(10, 12);
        let mut lines = Vec::new();
        for number in 8..=12 {
            frame_closed(number, |line| lines.push(line.to_string()));
        }
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("[lg/spike] gather window=10..12 frames=3 measured=0 leaf=- counter=actors_visited=0.000"));
        assert!(!active());
    }
}
