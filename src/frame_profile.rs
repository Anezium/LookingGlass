//! Optional local CPU profiling; inactive during ordinary play.
use std::{cell::RefCell, collections::BTreeMap, time::Instant};

thread_local! { static SAMPLES: RefCell<BTreeMap<&'static str, Vec<f64>>> = RefCell::default(); }

#[cfg(target_os = "vita")]
const VITA_PROFILE_DIR: &str = "ux0:data/alice/private";
#[cfg(target_os = "vita")]
#[derive(Default)]
struct VitaFrames {
    map: String,
    frames: usize,
    pending: Vec<(&'static str, f64)>,
}
#[cfg(target_os = "vita")]
thread_local! { static VITA_FRAMES: RefCell<VitaFrames> = RefCell::default(); }

fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    #[cfg(target_os = "vita")]
    let check = || std::path::Path::new(VITA_PROFILE_DIR).join("frame_profile").exists();
    #[cfg(not(target_os = "vita"))]
    let check = || std::env::var_os("LOOKING_GLASS_FRAME_PROFILE").is_some();
    *ON.get_or_init(check)
}

pub struct Span {
    label: &'static str,
    start: Option<Instant>,
}
pub fn span(label: &'static str) -> Span {
    Span { label, start: enabled().then(Instant::now) }
}
impl Span {
    pub fn cancel(&mut self) { self.start = None; }
}
impl Drop for Span {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            let ms = start.elapsed().as_secs_f64() * 1000.;
            #[cfg(target_os = "vita")]
            VITA_FRAMES.with(|f| f.borrow_mut().pending.push((self.label, ms)));
            #[cfg(not(target_os = "vita"))]
            SAMPLES.with(|s| {
                let mut s = s.borrow_mut();
                let v = s.entry(self.label).or_default();
                if v.len() < 10000 { v.push(ms); }
            });
        }
    }
}

/// Stage all spans until the viewer knows whether this was a gameplay frame.
#[cfg(target_os = "vita")]
pub fn begin_frame(map: &str) {
    if !enabled() { return; }
    VITA_FRAMES.with(|f| {
        let mut f = f.borrow_mut();
        f.pending.clear();
        if f.map != map {
            f.map = map.to_owned();
            f.frames = 0;
            SAMPLES.with(|s| s.borrow_mut().clear());
        }
    });
}

/// Discard every span from menus, loading and cinematics; report every 60
/// accepted frames. No per-label warmup: rare preview captures must survive.
#[cfg(target_os = "vita")]
pub fn end_frame(map: &str, gameplay: bool) -> bool {
    if !enabled() { return false; }
    VITA_FRAMES.with(|f| {
        let mut f = f.borrow_mut();
        if f.map != map || !gameplay {
            f.pending.clear();
            return false;
        }
        SAMPLES.with(|s| {
            let mut s = s.borrow_mut();
            for (label, ms) in f.pending.drain(..) {
                let v = s.entry(label).or_default();
                if v.len() < 10000 { v.push(ms); }
            }
        });
        f.frames += 1;
        f.frames % 60 == 0
    })
}

pub fn report(map: &str) -> anyhow::Result<()> {
    if !enabled() { return Ok(()); }
    let report = SAMPLES.with(|s| s.borrow().iter().map(|(name, v)| {
        #[cfg(target_os = "vita")]
        let mut samples = v.clone();
        #[cfg(not(target_os = "vita"))]
        let mut samples = v.iter().skip(30).copied().collect::<Vec<_>>();
        samples.sort_by(f64::total_cmp);
        let n = samples.len();
        (*name, serde_json::json!({"samples":n,"median_ms":samples.get(n/2),"p95_ms":samples.get(n*95/100)}))
    }).collect::<BTreeMap<_, _>>());
    #[cfg(target_os = "vita")]
    let dir = std::path::PathBuf::from(VITA_PROFILE_DIR);
    #[cfg(not(target_os = "vita"))]
    let dir = std::path::PathBuf::from(std::env::var_os("LOOKING_GLASS_FRAME_PROFILE").unwrap());
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(format!("{map}.json")), serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
