//! Loading frames for long transitions, behind `loading_screen`.
//!
//! The loads stay synchronous between their existing stage boundaries. At a boundary the
//! async load functions call `between`, which presents one frame (the transition's Loading
//! text, the destination title and a step bar) while a load is active, and returns at once
//! otherwise. No frame is presented inside a stage, so a stage costs what it cost before;
//! each boundary adds one frame. Sync callers drive the same async functions through
//! `run_ready`, which never presents.
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::time::Instant;

/// Stage boundaries that present a frame, the same for every load kind: two in
/// `campaign::load_visit` (or two of `Restored::build`'s first stages), two more in the scene
/// and cast build, one after the level art. The sixth step is the destination's first frame,
/// so the bar never reads full while the screen still says Loading.
pub(crate) const STEPS: u32 = 6;

struct Active {
    kind: &'static str,
    destination: String,
    done: u32,
    presents: u32,
    present_ms: f64,
    started: Instant,
}

thread_local! {
    static ACTIVE: RefCell<Option<Active>> = const { RefCell::new(None) };
    static SUPPRESS: Cell<u32> = const { Cell::new(0) };
}

pub(crate) fn enabled() -> bool {
    #[cfg(test)]
    if let Some(on) = TEST_ENABLED.with(|v| v.get()) { return on; }
    #[cfg(feature = "vita_renderer")]
    { crate::render::vita_renderer::config().loading_screen }
    #[cfg(not(feature = "vita_renderer"))]
    { false }
}
#[cfg(test)]
thread_local! { static TEST_ENABLED: Cell<Option<bool>> = const { Cell::new(None) }; }
#[cfg(test)]
pub(crate) fn set_enabled_for_test(on: Option<bool>) { TEST_ENABLED.with(|v| v.set(on)); }

/// Ends the active load on drop, on success and on every error path alike.
pub(crate) struct Load(());
impl Drop for Load {
    fn drop(&mut self) {
        if let Some(a) = ACTIVE.with(|a| a.borrow_mut().take()) {
            eprintln!("[lg/loading] end kind={} destination={} steps={}/{STEPS} presents={} present_ms={:.3} wall_ms={:.3}",
                a.kind, a.destination, a.done, a.presents, a.present_ms, a.started.elapsed().as_secs_f64() * 1000.);
        }
    }
}

/// Start a load when the key is on. A second begin while one is active keeps the first.
pub(crate) fn begin(kind: &'static str, destination: &str) -> Option<Load> {
    if !enabled() || ACTIVE.with(|a| a.borrow().is_some()) { return None; }
    eprintln!("[lg/loading] begin kind={kind} destination={destination}");
    ACTIVE.with(|a| *a.borrow_mut() = Some(Active {
        kind, destination: destination.into(), done: 0, presents: 0, present_ms: 0., started: Instant::now(),
    }));
    Some(Load(()))
}

#[cfg(test)]
fn active() -> bool { ACTIVE.with(|a| a.borrow().is_some()) }

/// Steps done and frames presented by the active load.
#[cfg(all(feature = "vita_renderer", not(target_os = "vita")))]
pub(crate) fn counts() -> Option<(u32, u32)> {
    ACTIVE.with(|a| a.borrow().as_ref().map(|a| (a.done, a.presents)))
}

fn current() -> Option<(String, u32)> {
    ACTIVE.with(|a| a.borrow().as_ref().map(|a| (a.destination.clone(), a.done)))
}

/// A finished stage. Presents one frame when a load is active and nothing suppresses frames.
pub(crate) async fn between(stage: &'static str) {
    if SUPPRESS.with(|s| s.get()) > 0 { return; }
    let Some((destination, done)) = ACTIVE.with(|a| a.borrow_mut().as_mut().map(|a| {
        a.done = (a.done + 1).min(STEPS);
        (a.destination.clone(), a.done)
    })) else { return };
    let start = Instant::now();
    present(&destination, done, stage).await;
    let ms = start.elapsed().as_secs_f64() * 1000.;
    ACTIVE.with(|a| if let Some(a) = a.borrow_mut().as_mut() { a.presents += 1; a.present_ms += ms; });
    eprintln!("[lg/loading] step={stage} done={done}/{STEPS} present_ms={ms:.3}");
}

async fn present(destination: &str, done: u32, stage: &str) {
    crate::frame_profile::exclude_transition_frame();
    draw(destination, done);
    #[cfg(not(target_os = "vita"))]
    if let Some(dir) = std::env::var_os("LOOKING_GLASS_LOADING_SCREEN_CAPTURE") {
        let path = std::path::Path::new(&dir).join(format!("step{done}-{stage}.png"));
        crate::viewer::save_capture(&path).expect("loading screen capture");
    }
    #[cfg(target_os = "vita")]
    let _ = stage;
    macroquad::prelude::next_frame().await;
}

/// The release frames before the build: the Loading screen at step zero when a load is
/// active, black otherwise, exactly as before.
pub(crate) async fn released() {
    match current() {
        Some((destination, done)) => crate::transition_memory::released_with(|| draw(&destination, done)).await,
        None => crate::transition_memory::released().await,
    }
}

/// The active load's frame, if any, else the caller's own frame.
pub(crate) fn draw_or(otherwise: impl FnOnce()) {
    match current() {
        Some((destination, done)) => draw(&destination, done),
        None => otherwise(),
    }
}

/// The natural exit's Loading text and title, unchanged, with the step bar under them.
pub(crate) fn draw(destination: &str, done: u32) {
    use macroquad::prelude::*;
    crate::viewer::draw_transition_loading(destination);
    for (rect, filled) in bar(screen_width(), screen_height(), done) {
        if filled {
            draw_rectangle(rect.x, rect.y, rect.w, rect.h, WHITE);
        } else {
            draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 2., GRAY);
        }
    }
}

/// STEPS equal segments centred under the title, the first `done` filled.
pub(crate) fn bar(width: f32, height: f32, done: u32) -> Vec<(macroquad::prelude::Rect, bool)> {
    let segment = (width * 0.04).round().max(8.);
    let gap = (segment * 0.25).round().max(2.);
    let tall = (segment * 0.35).round().max(4.);
    let total = STEPS as f32 * segment + (STEPS - 1) as f32 * gap;
    let x0 = ((width - total) * 0.5).round();
    let y = (height * 0.66).round();
    (0..STEPS).map(|i| (macroquad::prelude::Rect::new(x0 + i as f32 * (segment + gap), y, segment, tall), i < done)).collect()
}

/// Drive a load future that only awaits `between` to completion without presenting frames.
/// Sync callers (checks, headless routes, startup restores) keep their exact behaviour.
pub(crate) fn run_ready<F: Future>(future: F) -> F::Output {
    struct Release;
    impl Drop for Release { fn drop(&mut self) { SUPPRESS.with(|s| s.set(s.get() - 1)); } }
    SUPPRESS.with(|s| s.set(s.get() + 1));
    let _release = Release;
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut context) {
        std::task::Poll::Ready(value) => value,
        std::task::Poll::Pending => panic!("a load awaited something other than a suppressed stage boundary"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Reset;
    impl Drop for Reset { fn drop(&mut self) { set_enabled_for_test(None); } }

    async fn staged(log: &mut Vec<u32>) -> u32 {
        for stage in ["a", "b", "c"] {
            between(stage).await;
            log.push(current().map_or(0, |c| c.1));
        }
        7
    }

    #[test]
    fn off_by_default_and_begin_needs_the_key() {
        let _reset = Reset;
        set_enabled_for_test(None);
        assert!(!enabled(), "the code default keeps the black release frames");
        assert!(begin("chapter-or-console", "gvillage").is_none());
        assert!(!active());
        set_enabled_for_test(Some(true));
        let load = begin("death-retry", "hatter1").expect("key on starts a load");
        assert!(active());
        assert!(begin("native-load", "gvillage").is_none(), "nested begin keeps the outer load");
        drop(load);
        assert!(!active(), "the guard ends the load");
    }

    #[test]
    fn sync_callers_never_present_or_count_steps() {
        let _reset = Reset;
        set_enabled_for_test(Some(true));
        let mut log = Vec::new();
        assert_eq!(run_ready(staged(&mut log)), 7, "no active load: every boundary is ready");
        assert_eq!(log, [0, 0, 0]);
        let _load = begin("native-load", "gvillage").unwrap();
        log.clear();
        assert_eq!(run_ready(staged(&mut log)), 7, "active load: a suppressed boundary is ready too");
        assert_eq!(log, [0, 0, 0], "suppressed boundaries do not advance the bar");
        assert_eq!(SUPPRESS.with(|s| s.get()), 0, "suppression ends with run_ready");
    }

    #[test]
    fn suppression_survives_a_panicking_load() {
        let caught = std::panic::catch_unwind(|| run_ready(async { panic!("load failed") }));
        assert!(caught.is_err());
        assert_eq!(SUPPRESS.with(|s| s.get()), 0);
    }

    #[test]
    fn bar_has_steps_segments_centred_and_filled_in_order() {
        for (w, h) in [(960., 544.), (1280., 720.)] {
            for done in 0..=STEPS {
                let bar = bar(w, h, done);
                assert_eq!(bar.len(), STEPS as usize);
                assert_eq!(bar.iter().filter(|(_, f)| *f).count(), done as usize);
                assert!(bar.iter().take(done as usize).all(|(_, f)| *f), "filled from the left");
                let left = bar[0].0.x;
                let right = bar.last().unwrap().0.right();
                assert!((left - (w - right)).abs() <= 1., "centred: {left} vs {}", w - right);
                assert!(bar.windows(2).all(|p| p[0].0.right() < p[1].0.x), "segments do not overlap");
                assert!(bar.iter().all(|(r, _)| r.y > h * 0.58 + 20. && r.bottom() < h), "under the title, on screen");
            }
        }
    }
}
