//! Optional one-shot timings from game-thread entry through presentation.
use std::{sync::{Once, OnceLock}, time::Instant};
static START: OnceLock<Instant> = OnceLock::new();
static MAP_START: OnceLock<Instant> = OnceLock::new();
static MENU: Once = Once::new();
static MAP: Once = Once::new();
pub fn start() { START.get_or_init(Instant::now); }
pub fn menu() { mark(&MENU, "menu", ""); }
pub fn begin_map() { MAP_START.get_or_init(Instant::now); }
pub fn map(name: &str) { mark(&MAP, "first_map_frame", name); }
fn mark(once: &Once, stage: &str, map: &str) {
    once.call_once(|| {
        if let Some(start) = START.get() {
            let load_ms = MAP_START.get().map(|t| t.elapsed().as_secs_f64() * 1000.).unwrap_or(0.);
            eprintln!("[lg/load] stage={stage} map={map} launch_ms={:.3} load_ms={load_ms:.3}", start.elapsed().as_secs_f64() * 1000.);
        }
    });
}
