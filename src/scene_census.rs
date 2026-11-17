//! Default-off census of the main-thread `scene` spike bucket. WCHESS1, 2026-11-17.
//!
//! The spike line brackets `scene` from `sim(Sim::Scene)` to `sim(Sim::Particles2)`: the level
//! camera overrides, the 3D pass handoff and the transform/rope gather before render. On the
//! Vita that bucket also contains the one `sceGxmEndScene` call of the frame that closes the
//! previous pass, so it mixes CPU work with a GPU completion wait. This module splits the bucket
//! into three roots and their leaves, counts what each frame carried, and prints one mean line
//! per spike window plus the per-frame worst leaf. It keeps only integer sums.
//!
//! `scene_census` (config) is false in every shipped configuration. Nothing below runs then
//! beyond one cached config read per span call site. No allocation, I/O, GL call or lock happens
//! inside a measured frame: the summary strings are built when the window closes. The values are
//! wall-clock brackets like `phase_census`: they include preemption and API waits, and are not
//! GPU times. `pass_begin` is the leaf that closes the previous render pass; on the Vita its
//! inclusive value carries the EndScene wait the spike line reports as `gxm_end_in=scene`.
#![allow(dead_code)]
use std::cell::RefCell;
use std::time::Instant;

/// Fixed leaves, grouped by root. Three roots are inclusive brackets and are never nested.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum Leaf {
    Presentation = 0,
    Prepare,
    Transforms,
    Liquid,
    SkyApply,
    LevelCamera,
    DisplayClear,
    Prepare3d,
    CameraBuild,
    CameraSet,
    RenderTail,
    FrameLog,
    PassBegin,
    PassEnd,
    InteractionTransforms,
    TraversalTransforms,
    RopeHand,
    RopeUpdate,
}
const NAMES: [&str; Leaf::RopeUpdate as usize + 1] = [
    "presentation", "prepare", "transforms",
    "liquid", "sky_apply", "level_camera",
    "display_clear", "prepare_3d", "camera_build", "camera_set",
    "render_tail", "frame_log", "pass_begin", "pass_end",
    "interaction_transforms", "traversal_transforms", "rope_hand", "rope_update",
];
const LEAVES: usize = NAMES.len();
const ROOT_PRESENTATION: u8 = 1 << 0;
const ROOT_PREPARE: u8 = 1 << 1;
const ROOT_TRANSFORMS: u8 = 1 << 2;
/// Bracket depth; the measured tree is a root and at most two child levels.
const DEPTH: usize = 8;
const COUNTERS: [&str; 6] = [
    "levels", "transforms", "mirror_extra", "mirror_hide", "traversal", "rope",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum Counter {
    Levels = 0,
    Transforms,
    MirrorExtra,
    MirrorHide,
    Traversal,
    Rope,
}

#[inline]
fn is_root(leaf: usize) -> bool { leaf <= Leaf::Transforms as usize }
#[inline]
fn root_bit(leaf: usize) -> u8 {
    match leaf {
        x if x == Leaf::Presentation as usize => ROOT_PRESENTATION,
        x if x == Leaf::Prepare as usize => ROOT_PREPARE,
        _ => ROOT_TRANSFORMS,
    }
}
#[inline]
fn allowed_roots(leaf: usize) -> u8 {
    if is_root(leaf) { return root_bit(leaf); }
    match leaf {
        x if x == Leaf::Liquid as usize
            || x == Leaf::SkyApply as usize
            || x == Leaf::LevelCamera as usize => ROOT_PRESENTATION,
        x if x == Leaf::InteractionTransforms as usize
            || x == Leaf::TraversalTransforms as usize
            || x == Leaf::RopeHand as usize
            || x == Leaf::RopeUpdate as usize => ROOT_TRANSFORMS,
        _ => ROOT_PREPARE,
    }
}

struct State {
    ns: [u64; LEAVES],
    kids: [u64; LEAVES],
    calls: [u64; LEAVES],
    frame_ns: [u64; LEAVES],
    frame_kids: [u64; LEAVES],
    counters: [u64; COUNTERS.len()],
    worst: [u64; LEAVES],
    frames: u64,
    measured: u64,
    armed: bool,
    finished: bool,
    touched: bool,
    window_first: u64,
    window_last: u64,
    worst_frame: u64,
    worst_leaf: usize,
    worst_ns: u64,
    stack: [usize; DEPTH],
    depth: usize,
    root_mask: u8,
    errors: u64,
}
impl State {
    const ZERO: Self = Self {
        ns: [0; LEAVES], kids: [0; LEAVES], calls: [0; LEAVES],
        frame_ns: [0; LEAVES], frame_kids: [0; LEAVES],
        counters: [0; COUNTERS.len()], worst: [0; LEAVES],
        frames: 0, measured: 0, armed: false, finished: false, touched: false,
        window_first: 0, window_last: 0, worst_frame: 0, worst_leaf: 0, worst_ns: 0,
        stack: [0; DEPTH], depth: 0, root_mask: 0, errors: 0,
    };
    fn open(&mut self, first: u64, last: u64) {
        *self = Self::ZERO;
        self.armed = true;
        self.window_first = first;
        self.window_last = last;
    }
    fn reset_frame(&mut self) {
        self.frame_ns = [0; LEAVES];
        self.frame_kids = [0; LEAVES];
        self.touched = false;
    }
}
thread_local! { static STATE: RefCell<State> = const { RefCell::new(State::ZERO) }; }
#[cfg(test)]
thread_local! { static TEST_NS: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) }; }
/// Tests inject exact durations; production always reads the bracket clock.
#[cfg(test)]
pub(crate) fn test_elapsed(ns: Option<u64>) { TEST_NS.with(|n| n.set(ns)); }

#[inline]
pub(crate) fn on() -> bool {
    #[cfg(test)]
    if FORCE.with(|f| f.get()) { return true; }
    #[cfg(feature = "vita_renderer")]
    { crate::render::vita_renderer::config().scene_census }
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
    #[cfg(test)]
    let elapsed = TEST_NS.with(|n| n.get()).unwrap_or_else(|| start.elapsed().as_nanos() as u64);
    #[cfg(not(test))]
    let elapsed = start.elapsed().as_nanos() as u64;
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.depth != level + 1 || s.stack[level] != leaf { s.errors += 1; return; }
        s.depth = level;
        if is_root(leaf) { s.root_mask &= !root_bit(leaf); s.touched = true; }
        s.ns[leaf] += elapsed;
        s.calls[leaf] += 1;
        s.frame_ns[leaf] += elapsed;
        // Charge the immediate parent only. Each level subtracts its direct
        // children once; charging every ancestor would double-count a nested
        // child in the root and be hidden by saturating_sub.
        if level > 0 {
            let parent = s.stack[level - 1];
            s.kids[parent] += elapsed;
            s.frame_kids[parent] += elapsed;
        }
    });
}
/// Inclusive minus direct children, with saturation surfaced by `validate_charges`.
fn exclusive_ns(s: &State, leaf: usize) -> u64 {
    s.ns[leaf].saturating_sub(s.kids[leaf])
}
fn frame_exclusive_ns(s: &State, leaf: usize) -> u64 {
    s.frame_ns[leaf].saturating_sub(s.frame_kids[leaf])
}
/// A child can never outlast its parent, and the exclusive leaves must add up to
/// the root inclusive. Either failure is an error, never a silent zero.
fn validate_charges(s: &mut State) {
    let mut exclusive = 0u64;
    let mut roots = 0u64;
    for leaf in 0..LEAVES {
        if s.kids[leaf] > s.ns[leaf] {
            s.errors += 1;
        }
        exclusive = exclusive.saturating_add(exclusive_ns(s, leaf));
        if is_root(leaf) {
            roots = roots.saturating_add(s.ns[leaf]);
        }
    }
    if exclusive != roots {
        s.errors += 1;
    }
}
impl Drop for Scope {
    fn drop(&mut self) { leave(self.leaf, self.level, self.start.take()); }
}
pub(crate) fn span(leaf: Leaf) -> Scope {
    let index = leaf as usize;
    enter(index, allowed_roots(index))
}
pub(crate) fn count(counter: Counter) { count_n(counter, 1); }
pub(crate) fn count_n(counter: Counter, value: u64) {
    if value == 0 || !on() { return; }
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.armed && !s.finished && s.root_mask != 0 { s.counters[counter as usize] += value; }
    });
}
/// Names of the fixed leaves, with each leaf's mean inclusive, mean exclusive and call count.
fn write_summary(s: &State, out: &mut String) {
    use std::fmt::Write as _;
    let _ = write!(out, "[lg/spike] scene window={}..{} frames={} measured={}",
        s.window_first, s.window_last, s.frames, s.measured);
    let mean = |ns: u64| if s.frames == 0 { 0. } else { ns as f64 / s.frames as f64 / 1e6 };
    let mut first = true;
    for i in 0..LEAVES {
        if s.calls[i] == 0 && s.ns[i] == 0 { continue; }
        let _ = write!(out, "{}{}:{:.4}/{:.4}/{}", if first { " leaf=" } else { "," },
            NAMES[i], mean(s.ns[i]), mean(exclusive_ns(s, i)), s.calls[i]);
        first = false;
    }
    if first { out.push_str(" leaf=-"); }
    let mut first = true;
    for (i, name) in COUNTERS.iter().enumerate() {
        let _ = write!(out, "{}{}:{:.3}", if first { " counter=" } else { "," },
            name, s.counters[i] as f64 / s.frames.max(1) as f64);
        first = false;
    }
    let mut first = true;
    for i in 0..LEAVES {
        if s.worst[i] == 0 { continue; }
        let _ = write!(out, "{}{}:{}", if first { " worst=" } else { "," }, NAMES[i], s.worst[i]);
        first = false;
    }
    if s.worst_ns > 0 {
        let _ = write!(out, " worst_frame={}:{}:{:.2}", s.worst_frame, NAMES[s.worst_leaf],
            s.worst_ns as f64 / 1e6);
    }
    let _ = write!(out, " errors={}", s.errors);
}
/// One frame's worst leaf, pushed next to the spike line of that frame.
fn write_frame(s: &State, number: u64, out: &mut String) {
    use std::fmt::Write as _;
    let mut leaf = usize::MAX;
    let mut ns = 0u64;
    for i in 0..LEAVES {
        if s.frame_ns[i] == 0 { continue; }
        let exclusive = frame_exclusive_ns(s, i);
        if exclusive > ns || leaf == usize::MAX {
            leaf = i;
            ns = exclusive;
        }
    }
    if leaf == usize::MAX {
        let _ = write!(out, "[lg/spike] scene-frame frame={number} leaf=-");
        return;
    }
    let _ = write!(out, "[lg/spike] scene-frame frame={number} leaf={}:{:.2}", NAMES[leaf],
        ns as f64 / 1e6);
}
/// Called from `spike_log::begin_frame` with the number of the frame that just closed.
/// Arms the window one frame before its first measured frame, pushes one frame line per
/// measured frame and hands the single summary line to `push` once the last frame closed.
pub(crate) fn frame_closed(number: u64, push: impl FnOnce(&str), push_frame: impl FnOnce(&str)) {
    if !on() { return; }
    let mut line = String::new();
    let mut frame_line = String::new();
    let mut emit = false;
    let mut emit_frame = false;
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
        if s.touched {
            s.measured += 1;
            let mut leaf = usize::MAX;
            let mut ns = 0u64;
            for i in 0..LEAVES {
                if s.frame_ns[i] == 0 { continue; }
                let exclusive = frame_exclusive_ns(&s, i);
                if exclusive > ns || leaf == usize::MAX { leaf = i; ns = exclusive; }
            }
            if leaf != usize::MAX {
                s.worst[leaf] += 1;
                if ns > s.worst_ns { s.worst_ns = ns; s.worst_frame = number; s.worst_leaf = leaf; }
            }
            write_frame(&s, number, &mut frame_line);
            emit_frame = true;
        }
        s.reset_frame();
        if number == s.window_last {
            validate_charges(&mut s);
            write_summary(&s, &mut line);
            s.finished = true;
            emit = true;
        }
    });
    if emit_frame { push_frame(&frame_line); }
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
pub(crate) fn test_validate() { STATE.with(|s| validate_charges(&mut s.borrow_mut())); }
#[cfg(test)]
pub(crate) fn test_ns(leaf: usize) -> u64 { STATE.with(|s| s.borrow().ns[leaf]) }
#[cfg(test)]
pub(crate) fn test_exclusive(leaf: usize) -> u64 {
    STATE.with(|s| exclusive_ns(&s.borrow(), leaf))
}
#[cfg(test)]
pub(crate) fn test_reset() { STATE.with(|s| *s.borrow_mut() = State::ZERO); }

#[cfg(test)]
mod tests {
    use super::*;
    fn field(text: &str, name: &str) -> f64 {
        text.replace(',', " ")
            .split_whitespace()
            .map(|f| f.strip_prefix("leaf=").or_else(|| f.strip_prefix("counter=")).unwrap_or(f))
            .find(|f| f.starts_with(&format!("{name}:")))
            .unwrap_or_else(|| panic!("{name} in {text}"))
            .split(':').nth(1).unwrap().split('/').next().unwrap().parse().unwrap()
    }
    fn line_field(text: &str, name: &str) -> f64 {
        text.split_whitespace()
            .find_map(|f| f.strip_prefix(&format!("{name}=")))
            .unwrap_or_else(|| panic!("{name} in {text}"))
            .split(':').last().unwrap().parse().unwrap()
    }
    #[test]
    fn inactive_census_keeps_no_time_or_counters() {
        test_force(false);
        test_reset();
        assert!(!active());
        { let _s = span(Leaf::Liquid); }
        count(Counter::Levels);
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
            let _root = span(Leaf::Prepare);
            std::thread::sleep(std::time::Duration::from_millis(2));
            { let _p = span(Leaf::PassBegin); std::thread::sleep(std::time::Duration::from_millis(3)); }
        }
        STATE.with(|s| s.borrow_mut().frames = 1);
        let text = test_summary();
        assert!(text.contains("prepare:"), "{text}");
        assert!(text.contains("pass_begin:"), "{text}");
        assert!(field(&text, "prepare") > field(&text, "pass_begin"), "{text}");
        assert_eq!(test_errors(), 0);
    }
    #[test]
    fn nested_leaves_charge_only_their_immediate_parent() {
        test_force(true);
        test_open(10, 10);
        {
            let prepare = span(Leaf::Prepare);
            let prepare3d = { test_elapsed(Some(1_000_000)); span(Leaf::Prepare3d) };
            let pass = { test_elapsed(Some(400_000)); span(Leaf::PassBegin) };
            test_elapsed(Some(400_000));
            drop(pass);
            test_elapsed(Some(1_000_000));
            drop(prepare3d);
            test_elapsed(Some(1_500_000));
            drop(prepare);
        }
        test_elapsed(None);
        let mut line = String::new();
        frame_closed(10, |text| line.push_str(text), |_| {});
        assert_eq!(test_ns(Leaf::Prepare as usize), 1_500_000);
        assert_eq!(test_ns(Leaf::Prepare3d as usize), 1_000_000);
        assert_eq!(test_ns(Leaf::PassBegin as usize), 400_000);
        assert_eq!(test_exclusive(Leaf::PassBegin as usize), 400_000);
        assert_eq!(test_exclusive(Leaf::Prepare3d as usize), 600_000);
        assert_eq!(test_exclusive(Leaf::Prepare as usize), 500_000);
        let text = line;
        assert!(text.contains("prepare:1.5000/0.5000/1"), "{text}");
        assert!(text.contains("prepare_3d:1.0000/0.6000/1"), "{text}");
        assert!(text.contains("worst=prepare_3d:1"), "{text}");
        test_validate();
        assert_eq!(test_errors(), 0, "{text}");
        assert_eq!(STATE.with(|s| s.borrow().kids[Leaf::PassBegin as usize]), 0);
        assert_eq!(STATE.with(|s| s.borrow().kids[Leaf::Prepare3d as usize]), 400_000);
    }
    #[test]
    fn a_child_longer_than_its_parent_is_an_error_not_a_silent_zero() {
        test_force(true);
        test_open(10, 12);
        {
            let prepare = span(Leaf::Prepare);
            let prepare3d = { test_elapsed(Some(100_000)); span(Leaf::Prepare3d) };
            let pass = { test_elapsed(Some(400_000)); span(Leaf::PassBegin) };
            test_elapsed(Some(400_000));
            drop(pass);
            test_elapsed(Some(100_000));
            drop(prepare3d);
            test_elapsed(Some(100_000));
            drop(prepare);
        }
        test_elapsed(None);
        assert_eq!(test_ns(Leaf::Prepare3d as usize), 100_000);
        assert_eq!(test_exclusive(Leaf::Prepare3d as usize), 0);
        test_validate();
        assert!(test_errors() >= 1);
    }
    #[test]
    fn children_outside_their_root_are_ignored() {
        test_force(true);
        test_open(10, 12);
        { let _p = span(Leaf::PassBegin); }
        { let _l = span(Leaf::Liquid); }
        STATE.with(|s| { let s = s.borrow(); assert!(s.ns.iter().all(|&n| n == 0)); });
        {
            let _a = span(Leaf::Transforms);
            { let _t = span(Leaf::InteractionTransforms); std::thread::sleep(std::time::Duration::from_millis(2)); }
        }
        STATE.with(|s| { let s = s.borrow(); assert!(s.ns[Leaf::InteractionTransforms as usize] > 0); });
    }
    #[test]
    fn two_roots_cannot_overlap() {
        test_force(true);
        test_open(10, 12);
        let _a = span(Leaf::Presentation);
        {
            let b = span(Leaf::Prepare);
            assert!(b.start.is_none());
        }
        assert_eq!(test_errors(), 0);
        assert_eq!(test_ns(Leaf::Prepare as usize), 0);
    }
    #[test]
    fn counters_need_an_open_root() {
        test_force(true);
        test_open(10, 12);
        count_n(Counter::Levels, 2);
        assert_eq!(test_counters()[Counter::Levels as usize], 0);
        {
            let _p = span(Leaf::Prepare);
            count_n(Counter::Levels, 2);
        }
        assert_eq!(test_counters()[Counter::Levels as usize], 2);
    }
    #[test]
    fn the_window_emits_per_frame_lines_and_one_summary_then_stops() {
        test_force(true);
        test_open(10, 12);
        let mut lines = Vec::new();
        let mut frames = Vec::new();
        for number in 8..=12 {
            {
                let _root = span(Leaf::Prepare);
                let _leaf = span(Leaf::PassBegin);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            frame_closed(number, |line| lines.push(line.to_string()),
                |line| frames.push(line.to_string()));
        }
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("[lg/spike] scene window=10..12 frames=3 measured=3 leaf=prepare:"), "{}", lines[0]);
        assert!(lines[0].contains(" worst=pass_begin:3 worst_frame="), "{}", lines[0]);
        assert!(lines[0].contains(":pass_begin:"), "{}", lines[0]);
        assert_eq!(frames.len(), 3);
        assert!(frames[0].starts_with("[lg/spike] scene-frame frame=10 leaf=pass_begin:"), "{}", frames[0]);
        assert!(line_field(&frames[2], "leaf") > 0.5);
        assert!(!active());
    }
}
