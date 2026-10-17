//! Fixed-capacity viewer timestamps. No per-draw records or exports within the window.
#![cfg_attr(not(target_os="vita"), allow(dead_code))]
use std::{cell::RefCell, time::Instant};
const CAPACITY: usize = 10000;
const COLUMNS: [&str; 15] = ["frame_start", "cpu_start", "cpu_end", "reduction_start", "reduction_end",
    "log_start", "log_end", "report_start", "report_end", "yield_start", "swap_start", "swap_end",
    "input_start", "input_end", "next_frame_start"];
#[derive(Clone, Default, serde::Serialize)]
struct Row { sequence: u64, measured_frame: Option<usize>, timestamps_us: [u64; 15] }
struct Trace { origin: Instant, sequence: u64, rows: Vec<Row>, current: Option<Row>, overflow: usize }
impl Default for Trace {
    fn default() -> Self { Self { origin: Instant::now(), sequence: 0,
        rows: Vec::with_capacity(CAPACITY), current: None, overflow: 0 } }
}
impl Trace {
    fn now(&self) -> u64 { self.origin.elapsed().as_micros() as u64 + 1 }
    fn begin(&mut self) {
        let now = self.now();
        if let Some(mut row) = self.current.take() {
            row.timestamps_us[14] = now;
            if self.rows.len() < CAPACITY { self.rows.push(row); } else { self.overflow += 1; }
        }
        self.sequence += 1;
        let mut row = Row { sequence: self.sequence, ..Default::default() };
        row.timestamps_us[0] = now;
        self.current = Some(row);
    }
}
thread_local! { static TRACE: RefCell<Trace> = RefCell::default(); }
fn enabled() -> bool {
    #[cfg(feature="vita_renderer")]
    { crate::render::vita_renderer::config().frame_phase_probe && crate::frame_profile::enabled() }
    #[cfg(not(feature="vita_renderer"))]
    { false }
}
pub(crate) fn initialize() { if enabled() { TRACE.with(|_| {}); } }
pub(crate) fn begin() { if enabled() { TRACE.with(|t|t.borrow_mut().begin()); } }
pub(crate) fn mark(column: usize) {
    if !enabled() { return; }
    TRACE.with(|t| { let mut t=t.borrow_mut(); let now=t.now();
        if let Some(row)=&mut t.current { row.timestamps_us[column]=now; }
    });
}
pub(crate) fn measured(frame: usize) {
    if enabled() { TRACE.with(|t| { if let Some(row)=&mut t.borrow_mut().current { row.measured_frame=Some(frame); } }); }
}
pub(crate) fn reset() {
    if enabled() { TRACE.with(|t| { let mut t=t.borrow_mut(); t.rows.clear(); t.overflow=0; }); }
}
pub(crate) fn report() -> serde_json::Value {
    if !enabled() { return serde_json::Value::Null; }
    TRACE.with(|t| { let t=t.borrow(); serde_json::json!({
        "columns":COLUMNS, "rows":t.rows, "capacity":CAPACITY, "overflow":t.overflow,
        "clock":"monotonic microseconds since trace init + 1; zero means absent",
        "attribution":"same viewer frame through swap; input before next viewer begin is charged to preceding wall interval; all warmup/rejected rows retained with measured_frame=null",
        "outside_cpu":"frame_start..cpu_start, reduction, log, report, yield..swap_start (framework tail), swap, swap_end..next_frame_start (backend log/input/update/scheduling)",
    }) })
}
#[cfg(target_os="vita")]
extern "C" {
    fn __real_vglSwapBuffers(dialog: u8);
    fn __real_sceCtrlPeekBufferPositive(port: i32, data: *mut core::ffi::c_void, count: i32) -> i32;
}
#[cfg(target_os="vita")]
#[no_mangle]
pub unsafe extern "C" fn __wrap_vglSwapBuffers(dialog: u8) {
    crate::gpu_wait::tail_end();
    mark(10); crate::light_profile::swap_begin();
    { let _timer=crate::gpu_wait::call(crate::gpu_wait::Api::Swap); __real_vglSwapBuffers(dialog); }
    crate::light_profile::swap_end(); mark(11);
}
#[cfg(target_os="vita")]
#[no_mangle]
pub unsafe extern "C" fn __wrap_sceCtrlPeekBufferPositive(port: i32, data: *mut core::ffi::c_void, count: i32) -> i32 {
    mark(12); let result=__real_sceCtrlPeekBufferPositive(port,data,count); mark(13); result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_storage_attributes_tail_and_next_begin_to_previous_frame() {
        let mut trace=Trace::default(); let capacity=trace.rows.capacity();
        trace.begin(); trace.current.as_mut().unwrap().measured_frame=Some(1);
        trace.current.as_mut().unwrap().timestamps_us[10]=trace.now();
        trace.begin();
        assert_eq!(trace.rows[0].measured_frame,Some(1));
        assert!(trace.rows[0].timestamps_us[14]>=trace.rows[0].timestamps_us[10]);
        assert_eq!(trace.current.as_ref().unwrap().timestamps_us[0],trace.rows[0].timestamps_us[14]);
        for _ in 0..CAPACITY+20 { trace.begin(); }
        assert_eq!(trace.rows.len(),CAPACITY); assert!(trace.overflow>0);
        assert_eq!(trace.rows.capacity(),capacity);
    }
}
