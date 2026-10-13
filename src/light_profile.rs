//! Constant-space gameplay timings; CSV export once per completed 60-frame window.
#![cfg_attr(not(target_os = "vita"), allow(dead_code))]
use crate::frame_profile::Mode;
use std::{
    cell::RefCell,
    io::Write,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Light,
    Full,
}
pub(crate) fn resolve(explicit: Option<Level>, mode: Mode) -> Level {
    explicit.unwrap_or(if mode == Mode::Full {
        Level::Light
    } else {
        Level::Full
    })
}
static FULL: AtomicBool = AtomicBool::new(true);
pub(crate) fn set_mode(mode: Mode) {
    #[cfg(feature = "vita_renderer")]
    FULL.store(
        resolve(crate::render::vita_renderer::config().profiler, mode) == Level::Full,
        Ordering::Relaxed,
    );
    #[cfg(not(feature = "vita_renderer"))]
    let _ = mode;
}
pub(crate) fn full() -> bool {
    FULL.load(Ordering::Relaxed)
}
fn active() -> bool {
    cfg!(target_os = "vita") && !full()
}
const WINDOW: usize = 60;
#[derive(Clone, Copy, Default, Debug)]
struct Row {
    frame: u64,
    wall_us: u64,
    cpu_us: u64,
    swap_us: u64,
    skin_collect_wait_us: u64,
    skin_gather_us: u64,
    skin_fallbacks: u64,
    skin_overflow: u64,
    fx: [u64; 8],
}
struct Ring {
    rows: [Row; WINDOW],
    count: usize,
    exported: usize,
    sequence: u64,
    start: Option<Instant>,
    cpu_start: Option<Instant>,
    swap_start: Option<Instant>,
    current: Row,
    pending: bool,
}
impl Default for Ring {
    fn default() -> Self {
        Self {
            rows: [Row::default(); WINDOW],
            count: 0,
            exported: 0,
            sequence: 0,
            start: None,
            cpu_start: None,
            swap_start: None,
            current: Row::default(),
            pending: false,
        }
    }
}
impl Ring {
    fn begin(&mut self, now: Instant) {
        if self.pending {
            self.current.wall_us = now.duration_since(self.start.unwrap()).as_micros() as u64;
            self.sequence += 1;
            self.current.frame = self.sequence;
            self.rows[self.count % WINDOW] = self.current;
            self.count += 1;
        }
        self.pending = false;
        self.start = Some(now);
        self.current = Row::default();
    }
    fn write(&self, out: &mut impl Write) -> std::io::Result<()> {
        writeln!(out, "frame,wall_us,frame_cpu_us,swap_us,skin_collect_wait_us,skin_gather_us,skin_fallbacks,skin_overflow,particle_producer_us,fx_merge_us,fx_upload_cpu_us,fx_upload_gl_us,fx_upload_bytes,fx_finish_prepare_us,fx_worker_wait_us,fx_finish_recycle_us")?;
        let first = self.count.saturating_sub(WINDOW);
        for i in first..self.count {
            let r = self.rows[i % WINDOW];
            writeln!(out, "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}", r.frame, r.wall_us, r.cpu_us, r.swap_us,
                r.skin_collect_wait_us, r.skin_gather_us, r.skin_fallbacks, r.skin_overflow,
                r.fx[0],r.fx[1],r.fx[2],r.fx[3],r.fx[4],r.fx[5],r.fx[6],r.fx[7])?;
        }
        Ok(())
    }
}
thread_local! { static RING: RefCell<Ring> = RefCell::default(); }
pub(crate) fn reset() {
    RING.with(|r| *r.borrow_mut() = Ring::default());
}
pub(crate) fn begin(map: &str, mode: Mode) {
    if !active() {
        return;
    }
    RING.with(|r| r.borrow_mut().begin(Instant::now()));
    if let Err(e) = export_window(map, mode) {
        eprintln!("[lg/light] export failed: {e}");
    }
}
pub(crate) fn skin_collect_wait(us: u64) {
    if active() { RING.with(|r| r.borrow_mut().current.skin_collect_wait_us += us); }
}
pub(crate) fn skin_overlap(gather_us: u64, fallbacks: u64, overflow: bool) {
    if active() {
        RING.with(|r| {
            let mut r = r.borrow_mut();
            r.current.skin_gather_us += gather_us;
            r.current.skin_fallbacks += fallbacks;
            r.current.skin_overflow |= u64::from(overflow);
        });
    }
}
#[derive(Clone, Copy)]
pub(crate) enum FxMetric { Producer, Merge, UploadCpu, UploadGl, UploadBytes, FinishPrepare, WorkerWait, FinishRecycle }
pub(crate) fn fx_add(metric: FxMetric, value: u64) {
    if active() { RING.with(|r| r.borrow_mut().current.fx[metric as usize] += value); }
}
pub(crate) struct FxTimer(FxMetric, Instant);
pub(crate) fn fx_timer(metric: FxMetric) -> FxTimer { FxTimer(metric, Instant::now()) }
impl Drop for FxTimer { fn drop(&mut self) { fx_add(self.0, self.1.elapsed().as_micros() as u64); } }

pub(crate) fn cpu_begin() {
    if active() {
        RING.with(|r| r.borrow_mut().cpu_start = Some(Instant::now()));
    }
}
pub(crate) fn cpu_end() {
    if active() {
        RING.with(|r| {
            let mut r = r.borrow_mut();
            r.current.cpu_us = r
                .cpu_start
                .take()
                .map_or(0, |t| t.elapsed().as_micros() as u64);
        });
    }
}
pub(crate) fn swap_begin() {
    if active() {
        RING.with(|r| r.borrow_mut().swap_start = Some(Instant::now()));
    }
}
pub(crate) fn swap_end() {
    if active() {
        RING.with(|r| {
            let mut r = r.borrow_mut();
            r.current.swap_us += r
                .swap_start
                .take()
                .map_or(0, |t| t.elapsed().as_micros() as u64);
        });
    }
}
pub(crate) fn end(measured: bool) {
    if active() {
        RING.with(|r| r.borrow_mut().pending = measured);
    }
}
pub(crate) fn export(map: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "vita")]
    return export_window(map, crate::frame_profile::mode());
    #[cfg(not(target_os = "vita"))]
    {
        let _ = map;
        Ok(())
    }
}
fn export_window(map: &str, mode: Mode) -> anyhow::Result<()> {
    RING.with(|r| {
        let mut r = r.borrow_mut();
        if r.count == r.exported || r.count % WINDOW != 0 {
            return Ok(());
        }
        #[cfg(target_os = "vita")]
        let dir = std::path::Path::new("ux0:data/alice/private/frame_profile");
        #[cfg(not(target_os = "vita"))]
        let dir = std::path::Path::new("private/frame_profile");
        std::fs::create_dir_all(dir)?;
        let name = format!("{map}-{}-light-{}.csv", mode_name(mode), r.count);
        let mut out = std::io::BufWriter::new(std::fs::File::create(dir.join(&name))?);
        r.write(&mut out)?;
        out.flush()?;
        r.exported = r.count;
        println!(
            "[lg/light] window mode={} frames_end={} rows={} file={name}",
            mode_name(mode),
            r.count,
            r.count.min(WINDOW)
        );
        Ok(())
    })
}
fn mode_name(mode: Mode) -> &'static str {
    #[cfg(any(target_os = "vita", test))]
    {
        mode.name()
    }
    #[cfg(not(any(target_os = "vita", test)))]
    {
        let _ = mode;
        "full"
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_explicit_levels_cover_every_mode() {
        for mode in [
            Mode::Full,
            Mode::NoPreview,
            Mode::NoPrep,
            Mode::NoWorld,
            Mode::Clk444,
            Mode::Bench,
            Mode::RenderBench,
            Mode::FullBench,
            Mode::Route,
            Mode::RouteContinue,
        ] {
            assert_eq!(
                resolve(None, mode),
                if mode == Mode::Full {
                    Level::Light
                } else {
                    Level::Full
                }
            );
            for level in [Level::Light, Level::Full] {
                assert_eq!(resolve(Some(level), mode), level);
            }
        }
    }
    #[test]
    fn ring_csv_is_chronological_bounded_and_attributes_previous_wall() {
        let mut r = Ring::default();
        let origin = Instant::now();
        r.begin(origin);
        for i in 1..=125 {
            r.current.cpu_us = 7;
            r.current.swap_us = 2;
            r.current.skin_collect_wait_us = i;
            r.current.skin_gather_us = i * 3;
            r.current.skin_fallbacks = i % 3;
            r.current.skin_overflow = i % 2;
            r.pending = true;
            r.begin(origin + std::time::Duration::from_micros(i * 10));
        }
        let mut out = Vec::new();
        r.write(&mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert_eq!(s.lines().count(), 61);
        assert_eq!(
            s.lines().next().unwrap(),
            "frame,wall_us,frame_cpu_us,swap_us,skin_collect_wait_us,skin_gather_us,skin_fallbacks,skin_overflow,particle_producer_us,fx_merge_us,fx_upload_cpu_us,fx_upload_gl_us,fx_upload_bytes,fx_finish_prepare_us,fx_worker_wait_us,fx_finish_recycle_us"
        );
        assert_eq!(s.lines().nth(1).unwrap(), "66,10,7,2,66,198,0,0,0,0,0,0,0,0,0,0");
        assert_eq!(s.lines().last().unwrap(), "125,10,7,2,125,375,2,1,0,0,0,0,0,0,0,0");
        assert_eq!(r.current.skin_gather_us, 0);
        assert_eq!(r.current.skin_fallbacks, 0);
        assert_eq!(r.current.skin_overflow, 0);
        r.begin(origin + std::time::Duration::from_micros(2000));
        assert_eq!(r.count, 125);
    }
}
