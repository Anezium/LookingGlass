//! Probe existing driver calls only. Never flush, finish, wait or fence on behalf
//! of the profiler. Elapsed API time includes preemption; off-CPU is not GPU time.
#![cfg_attr(not(target_os="vita"), allow(dead_code))]
use std::{cell::RefCell, time::Instant};
pub(crate) const NAMES: [&str; 33] = [
    "gpu_submit_wall_us", "gpu_submit_cpu_us", "gpu_submit_nonwait_cpu_us",
    "gpu_wait_api_wall_us", "gpu_wait_api_cpu_us", "gpu_wait_off_cpu_us",
    "gpu_gl_flush_us", "gpu_gl_flush_calls", "gpu_gl_finish_us", "gpu_gl_finish_calls",
    "gpu_vgl_swap_us", "gpu_vgl_swap_calls", "gpu_gxm_finish_us", "gpu_gxm_finish_calls",
    "gpu_gxm_transfer_finish_us", "gpu_gxm_transfer_finish_calls",
    "gpu_gxm_notification_wait_us", "gpu_gxm_notification_wait_calls",
    "gpu_gxm_display_queue_add_us", "gpu_gxm_display_queue_add_calls",
    "gpu_gxm_display_queue_finish_us", "gpu_gxm_display_queue_finish_calls",
    "gpu_gxm_begin_scene_us", "gpu_gxm_begin_scene_calls",
    "gpu_gxm_end_scene_us", "gpu_gxm_end_scene_calls",
    "gpu_submit_scopes", "gpu_clock_reads", "gpu_clock_errors", "gpu_probe_frames",
    "gpu_gl_flush_cpu_us", "gpu_gl_finish_cpu_us", "gpu_vgl_swap_cpu_us",
];
pub(crate) const COUNT: usize = NAMES.len();
#[derive(Clone, Copy)]
pub(crate) enum Api { Flush, Finish, Swap, GxmFinish, TransferFinish, Notification,
    DisplayAdd, DisplayFinish, BeginScene, EndScene }
impl Api { fn wait(self) -> bool { self as usize >= Self::GxmFinish as usize } }
struct State {
    enabled: bool, values: [u64; COUNT], submit_depth: usize, wait_depth: usize,
    wait_cpu_in_submit: u64, swap_depth: usize,
}
impl Default for State {
    fn default() -> Self { Self { enabled:false,values:[0;COUNT],submit_depth:0,
        wait_depth:0,wait_cpu_in_submit:0,swap_depth:0 } }
}
impl State {
    fn api(&mut self, api: Api, m: Sample, outer_wait: bool) {
        let offset = 6 + api as usize * 2;
        self.values[offset] += m.wall_us; self.values[offset+1] += 1;
        if (api as usize) < 3 { self.values[30+api as usize] += m.cpu_us; }
        if outer_wait {
            self.values[3] += m.wall_us;
            self.values[4] += m.cpu_us;
            self.values[5] += m.wall_us.saturating_sub(m.cpu_us);
            if self.submit_depth > 0 { self.wait_cpu_in_submit += m.cpu_us; }
            else if self.swap_depth == 0 {
                // An existing readback/scene boundary can wait outside a retained
                // packet. Its scheduled CPU and call wall form a disjoint region.
                self.wait_cpu_in_submit = m.cpu_us; self.submit(m);
            }
        }
        self.values[28] += u64::from(!m.valid);
    }
    fn submit(&mut self, m: Sample) {
        self.values[0] += m.wall_us; self.values[1] += m.cpu_us;
        self.values[2] += m.cpu_us.saturating_sub(self.wait_cpu_in_submit);
        self.values[26] += 1;
        self.wait_cpu_in_submit = 0;
    }
}
thread_local! {
    static STATE: RefCell<State> = RefCell::default();
    static TAIL: RefCell<Option<Submission>> = const { RefCell::new(None) };
}
#[derive(Clone, Copy)]
struct Sample { wall_us: u64, cpu_us: u64, valid: bool }
struct Stamp { wall: Instant, cpu: Option<(i32, u64)> }
fn runtime() -> Option<(i32,u64)> {
    STATE.with(|s| s.borrow_mut().values[27] += 1);
    #[cfg(target_os="vita")]
    { crate::skin_worker::current_runtime().ok().map(|(id,cpu,_,_)| (id,cpu)) }
    #[cfg(not(target_os="vita"))]
    { None }
}
impl Stamp {
    fn new() -> Self { let wall = Instant::now(); Self { wall, cpu: runtime() } }
    fn finish(self) -> Sample {
        let end = runtime();
        let wall_us = self.wall.elapsed().as_micros() as u64;
        let cpu = self.cpu.zip(end).filter(|((a,x),(b,y))| a == b && y >= x).map(|((_,x),(_,y))| y-x);
        Sample { wall_us, cpu_us: cpu.unwrap_or(0), valid: cpu.is_some_and(|v| v <= wall_us.saturating_add(2)) }
    }
}
/// Non-overlapping coarse submission intervals; nested regions are a union.
/// Numeric world preparation and actor gathering are outside these intervals.
pub(crate) struct Submission { active: bool, stamp: Option<Stamp>, _game: crate::game_thread::GameThread }
pub(crate) fn submission() -> Submission {
    let (active, outer) = STATE.with(|s| { let mut s = s.borrow_mut();
        if !s.enabled { return (false,false); }
        let outer = s.submit_depth == 0; s.submit_depth += 1;
        if outer { s.wait_cpu_in_submit = 0; } (true,outer) });
    Submission { active, stamp: outer.then(Stamp::new), _game: crate::game_thread::GameThread::default() }
}
impl Drop for Submission {
    fn drop(&mut self) {
        if !self.active { return; }
        let sample = self.stamp.take().map(Stamp::finish);
        STATE.with(|s| { let mut s = s.borrow_mut();
            assert!(s.submit_depth > 0); s.submit_depth -= 1;
            if let Some(m) = sample { assert_eq!(s.submit_depth,0); s.submit(m); s.values[28] += u64::from(!m.valid); }
        });
    }
}
pub(crate) struct Call { api: Api, stamp: Option<Stamp>, outer_wait: bool, _game: crate::game_thread::GameThread }
pub(crate) fn call(api: Api) -> Call {
    let (active, outer_wait) = STATE.with(|s| { let mut s = s.borrow_mut();
        if !s.enabled { return (false,false); }
        let outer = api.wait() && s.wait_depth == 0;
        if api.wait() { s.wait_depth += 1; }
        if matches!(api,Api::Swap) { s.swap_depth += 1; } (true,outer) });
    Call { api, stamp: active.then(Stamp::new), outer_wait, _game: crate::game_thread::GameThread::default() }
}
impl Drop for Call {
    fn drop(&mut self) {
        let Some(stamp) = self.stamp.take() else { return; };
        let sample = stamp.finish();
        STATE.with(|s| { let mut s = s.borrow_mut();
            if self.api.wait() { assert!(s.wait_depth > 0); s.wait_depth -= 1; }
            s.api(self.api,sample,self.outer_wait);
            if matches!(self.api,Api::Swap) { assert!(s.swap_depth > 0); s.swap_depth -= 1; }
        });
    }
}
/// First real deferred GL event starts the framework tail. Repeated draw packets
/// share one scope through the actual draw; swap closes any residual tail BEFORE swapping.
pub(crate) fn tail_begin() {
    if STATE.with(|s| { let s=s.borrow(); !s.enabled || s.submit_depth > 0 }) { return; }
    TAIL.with(|t| { let mut t = t.borrow_mut(); if t.is_none() { *t = Some(submission()); } });
}
pub(crate) fn tail_end() { drop(TAIL.with(|t| t.borrow_mut().take())); }
pub(crate) fn reset() { tail_end(); STATE.with(|s| *s.borrow_mut() = State::default()); }
/// Called before Ring::begin saves the preceding frame, after its tail and swap.
pub(crate) fn finish_frame(enabled: bool) -> [u64;COUNT] {
    let unexpected_tail = TAIL.with(|t| t.borrow().is_some()); tail_end();
    STATE.with(|s| { let mut s = s.borrow_mut();
        let mut values = s.values;
        if s.enabled { values[29] = 1; }
        if unexpected_tail || s.submit_depth != 0 || s.wait_depth != 0 || s.swap_depth != 0 { values[28] += 1; }
        *s = State { enabled: enabled && cfg!(target_os="vita"), ..Default::default() };
        values
    })
}

#[cfg(target_os="vita")]
mod hooks {
    use super::*;
    use std::ffi::c_void;
    extern "C" {
        fn __real_glFlush(); fn __real_glFinish();
        fn __real_sceGxmFinish(context:*mut c_void);
        fn __real_sceGxmTransferFinish()->i32;
        fn __real_sceGxmNotificationWait(notification:*const c_void)->i32;
        fn __real_sceGxmDisplayQueueAddEntry(old:*mut c_void,new:*mut c_void,data:*const c_void)->i32;
        fn __real_sceGxmDisplayQueueFinish()->i32;
        fn __real_sceGxmBeginScene(context:*mut c_void,flags:u32,target:*const c_void,region:*const c_void,
            vertex:*mut c_void,fragment:*mut c_void,color:*const c_void,depth:*const c_void)->i32;
        fn __real_sceGxmEndScene(context:*mut c_void,vertex:*const c_void,fragment:*const c_void)->i32;
    }
    // Signatures match the pinned VitaSDK psp2/gxm.h. Each wrapper forwards its
    // original arguments exactly once and returns the original result unchanged.
    macro_rules! wrap {
        ($name:ident => $real:ident, $kind:ident, ($($arg:ident:$ty:ty),*) -> $ret:ty) => {
            #[no_mangle]
            pub unsafe extern "C" fn $name($($arg:$ty),*) -> $ret {
                let _submit = matches!(Api::$kind,Api::Flush | Api::Finish).then(submission);
                let _timer = call(Api::$kind);
                $real($($arg),*)
            }
        };
    }
    wrap!(__wrap_glFlush => __real_glFlush, Flush, () -> ());
    wrap!(__wrap_glFinish => __real_glFinish, Finish, () -> ());
    wrap!(__wrap_sceGxmFinish => __real_sceGxmFinish, GxmFinish, (context:*mut c_void) -> ());
    wrap!(__wrap_sceGxmTransferFinish => __real_sceGxmTransferFinish, TransferFinish, () -> i32);
    wrap!(__wrap_sceGxmNotificationWait => __real_sceGxmNotificationWait, Notification, (notification:*const c_void) -> i32);
    wrap!(__wrap_sceGxmDisplayQueueAddEntry => __real_sceGxmDisplayQueueAddEntry, DisplayAdd,
        (old:*mut c_void,new:*mut c_void,data:*const c_void) -> i32);
    wrap!(__wrap_sceGxmDisplayQueueFinish => __real_sceGxmDisplayQueueFinish, DisplayFinish, () -> i32);
    wrap!(__wrap_sceGxmBeginScene => __real_sceGxmBeginScene, BeginScene,
        (context:*mut c_void,flags:u32,target:*const c_void,region:*const c_void,vertex:*mut c_void,
            fragment:*mut c_void,color:*const c_void,depth:*const c_void) -> i32);
    wrap!(__wrap_sceGxmEndScene => __real_sceGxmEndScene, EndScene,
        (context:*mut c_void,vertex:*const c_void,fragment:*const c_void) -> i32);
    pub(crate) fn lookup(name:&[u8]) -> Option<*mut c_void> {
        match name {
            b"glFlush" => Some(__wrap_glFlush as *const () as _),
            b"glFinish" => Some(__wrap_glFinish as *const () as _),
            _ => None,
        }
    }
}
#[cfg(target_os="vita")]
pub(crate) use hooks::lookup;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wait_api_and_submission_cpu_are_distinct_and_nested_waits_count_once() {
        let mut s = State::default(); s.submit_depth = 1;
        s.api(Api::GxmFinish,Sample {wall_us:100,cpu_us:8,valid:true},true);
        s.api(Api::Notification,Sample {wall_us:40,cpu_us:3,valid:true},false);
        s.api(Api::Finish,Sample {wall_us:120,cpu_us:20,valid:true},false);
        s.submit(Sample {wall_us:300,cpu_us:150,valid:true});
        assert_eq!(&s.values[..6], &[300,150,142,100,8,92]);
        assert_eq!(s.values[8],120); assert_eq!(s.values[9],1); assert_eq!(s.values[31],20);
        assert_eq!(s.values[28],0);
    }
    #[test]
    fn swap_children_do_not_inflate_submission_before_swap() {
        let mut s=State::default(); s.swap_depth=1;
        s.api(Api::DisplayAdd,Sample {wall_us:100,cpu_us:5,valid:true},true);
        assert_eq!(s.values[0],0);
        s.swap_depth=0;
        s.api(Api::GxmFinish,Sample {wall_us:80,cpu_us:10,valid:true},true);
        assert_eq!(&s.values[..6], &[80,10,0,180,15,165]);
    }
    #[test]
    fn packet_tail_does_not_cross_or_nest_a_manual_submission_scope() {
        reset(); STATE.with(|s| s.borrow_mut().enabled = true);
        let outer = submission(); let inner = submission();
        tail_begin(); assert!(TAIL.with(|t| t.borrow().is_none()));
        drop(inner); drop(outer);
        tail_begin(); tail_begin(); assert_eq!(STATE.with(|s| s.borrow().submit_depth),1);
        tail_end(); assert_eq!(STATE.with(|s| s.borrow().submit_depth),0);
        let values = finish_frame(false); assert_eq!(values[26],2); assert_eq!(values[29],1);
        // Host has no Vita runtime clock: invalid CPU readings are explicit.
        assert_eq!(values[28],2);
    }
    #[test]
    fn missing_runtime_is_flagged_instead_of_accepted_as_zero_cpu() {
        let mut s = State::default();
        s.api(Api::BeginScene,Sample {wall_us:20,cpu_us:0,valid:false},true);
        assert_eq!(s.values[28],1);
    }
    #[test]
    fn disabled_probe_never_reads_a_clock_or_exports_nonzero_values() {
        reset(); drop(submission()); drop(call(Api::Swap));
        assert_eq!(finish_frame(false),[0;COUNT]);
        let mut names = std::collections::BTreeSet::new();
        assert!(NAMES.iter().all(|n| names.insert(*n)));
    }
}
