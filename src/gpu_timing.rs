//! GPU timing and draw masks behind config keys, for console diagnostics. Off by default.
//!
//! vitaGL ends every scene with `sceGxmEndScene(gxm_context, NULL, &query_fence)` after bumping
//! `query_fence.value`, so `value - *address` is the count of ended scenes the GPU has not finished.
//! The Vita link wraps `sceGxmBeginScene`, `sceGxmEndScene` and `sceDisplayWaitVblankStartMulti`
//! (`build.rs`). Each wrapper tests one atomic and tail-calls the real function until a survey
//! window arms the log.
//!
//! `gpu_timing` modes, each one a superset of the one before in what it stamps:
//! - `backlog`: scene counts, in-flight scenes, time blocked in Begin and End, swap, display queue.
//!   One load of the notification word per call. No thread and no GXM argument changes.
//! - `poll`: adds a vertex notification in its own slot and one thread that polls both words every
//!   millisecond, on CPU2. A stamp is late by up to one period plus scheduling.
//! - `wait`: the same thread blocks in `sceGxmNotificationWait` instead. vitaGL has this call but
//!   nothing in this program runs it, so its behaviour on a value already passed is unknown here.
//!   If it never returns only the timing thread hangs, and the export reports `incomplete`.
//!
//! A tiled GPU shades at scene end, so this cannot see inside a scene. `gpu_mask` separates
//! passes instead: it skips the draw call of a class and keeps the CPU work around it.
//!
//! Nothing in this file allocates after `prepare`, takes a lock or touches thread-local storage on
//! the wrapper paths. The log has a single writer per field: the main thread owns everything up to
//! publishing a scene through `head`, the timing thread owns the two completion stamps.
#![allow(dead_code)]
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicUsize, Ordering::{Acquire, Relaxed, Release}};

/// 180 frames of up to 8 scenes, the most vitaGL accepts per render target.
pub(crate) const SCENE_CAPACITY: usize = 1440;
pub(crate) const FRAME_CAPACITY: usize = 180;
/// One display queue entry per frame, plus entries still draining when the window opens.
pub(crate) const DISPLAY_CAPACITY: usize = 200;
/// Slot 0 of the notification region is vitaGL's `query_fence` (`buffers.c`, `reset_queries`).
pub(crate) const VERTEX_SLOT: usize = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Mode { Off, Backlog, Poll, Wait }
impl Mode {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text { "off" => Some(Self::Off), "backlog" => Some(Self::Backlog), "poll" => Some(Self::Poll),
            "wait" => Some(Self::Wait), _ => None }
    }
    fn name(self) -> &'static str {
        match self { Self::Off => "off", Self::Backlog => "backlog", Self::Poll => "poll", Self::Wait => "wait" }
    }
    fn threaded(self) -> bool { matches!(self, Self::Poll | Self::Wait) }
}

/// Draw classes of `gpu_mask`.
pub(crate) const MASK_SKY: u32 = 1 << 0;
pub(crate) const MASK_WORLD_LOW: u32 = 1 << 1;
pub(crate) const MASK_WORLD_FULL: u32 = 1 << 2;
pub(crate) const MASK_WORLD_ALPHA: u32 = 1 << 3;
pub(crate) const MASK_ACTORS: u32 = 1 << 4;
pub(crate) const MASK_FX_ALPHA: u32 = 1 << 5;
pub(crate) const MASK_FX_ADD: u32 = 1 << 6;
pub(crate) const MASK_FX_OTHER: u32 = 1 << 7;
pub(crate) fn mask_bit(name: &str) -> Option<u32> {
    Some(match name {
        "sky" => MASK_SKY, "world_low" => MASK_WORLD_LOW, "world_full" => MASK_WORLD_FULL,
        "world_alpha" => MASK_WORLD_ALPHA, "actors" => MASK_ACTORS, "fx_alpha" => MASK_FX_ALPHA,
        "fx_add" => MASK_FX_ADD, "fx_other" => MASK_FX_OTHER, _ => return None,
    })
}
/// Class of an effect packet, from `render::blend`'s index: 1 is Alpha, 2 Add and 5 AlphaAdd.
pub(crate) fn fx_class(blend_index: usize) -> u32 {
    match blend_index { 1 => MASK_FX_ALPHA, 2 | 5 => MASK_FX_ADD, _ => MASK_FX_OTHER }
}
static MASK: AtomicU32 = AtomicU32::new(0);
pub(crate) fn set_mask(bits: u32) { MASK.store(bits, Relaxed); }
#[inline] pub(crate) fn masked(class: u32) -> bool { MASK.load(Relaxed) & class != 0 }

#[inline] pub(crate) fn reached(current: u32, target: u32) -> bool { current.wrapping_sub(target) as i32 >= 0 }

#[derive(Default)]
pub(crate) struct Scene {
    frame: AtomicU32, display: AtomicU32,
    begin_enter: AtomicU32, begin_us: AtomicU32, inflight_begin: AtomicU32,
    end_enter: AtomicU32, end_us: AtomicU32, inflight_end: AtomicU32,
    fragment_value: AtomicU32, vertex_value: AtomicU32,
    vertex_done: AtomicU32, fragment_done: AtomicU32,
}
#[derive(Default)]
struct FrameRow { presented: AtomicU32, swap_enter: AtomicU32, swap_us: AtomicU32, inflight: AtomicU32 }
#[derive(Default)]
struct DisplayRow { callback: AtomicU32, vblank_done: AtomicU32 }

pub(crate) struct Log {
    mode: Mode,
    scenes: Box<[Scene]>, frames: Box<[FrameRow]>, display: Box<[DisplayRow]>,
    /// Scenes whose End returned. Everything below is complete for the timing thread to read.
    head: AtomicUsize,
    /// Scenes the timing thread has finished with.
    tail: AtomicUsize,
    /// A scene opened by Begin and not yet published, as index plus one.
    open: AtomicUsize,
    frame: AtomicU32, swap_enter: AtomicU32, swap_us: AtomicU32,
    display_next: AtomicUsize, vertex_value: AtomicU32,
    scene_overflow: AtomicU32, frame_overflow: AtomicU32, display_overflow: AtomicU32,
    stop: AtomicBool, stopped: AtomicBool,
}
impl Log {
    pub(crate) fn new(mode: Mode) -> Self {
        Self { mode,
            scenes: (0..SCENE_CAPACITY).map(|_| Scene::default()).collect(),
            frames: (0..FRAME_CAPACITY).map(|_| FrameRow::default()).collect(),
            display: (0..DISPLAY_CAPACITY).map(|_| DisplayRow::default()).collect(),
            head: AtomicUsize::new(0), tail: AtomicUsize::new(0), open: AtomicUsize::new(0),
            frame: AtomicU32::new(1), swap_enter: AtomicU32::new(0), swap_us: AtomicU32::new(0),
            display_next: AtomicUsize::new(0), vertex_value: AtomicU32::new(0),
            scene_overflow: AtomicU32::new(0), frame_overflow: AtomicU32::new(0), display_overflow: AtomicU32::new(0),
            stop: AtomicBool::new(false), stopped: AtomicBool::new(false) }
    }
    /// Main thread, Begin returned.
    pub(crate) fn begin(&self, display: bool, enter: u32, blocked_us: u32, inflight: u32) {
        let index = self.head.load(Relaxed);
        let Some(scene) = self.scenes.get(index) else { self.scene_overflow.fetch_add(1, Relaxed); return; };
        scene.frame.store(self.frame.load(Relaxed), Relaxed);
        scene.display.store(u32::from(display), Relaxed);
        scene.begin_enter.store(enter, Relaxed);
        scene.begin_us.store(blocked_us, Relaxed);
        scene.inflight_begin.store(inflight, Relaxed);
        self.open.store(index + 1, Relaxed);
    }
    pub(crate) fn next_vertex_value(&self) -> u32 { self.vertex_value.fetch_add(1, Relaxed).wrapping_add(1) }
    /// Main thread, End returned. A scene that began before the window opened has begin stamps of zero.
    pub(crate) fn end(&self, enter: u32, blocked_us: u32, inflight: u32, fragment_value: u32, vertex_value: u32) {
        let index = self.head.load(Relaxed);
        let Some(scene) = self.scenes.get(index) else { self.scene_overflow.fetch_add(1, Relaxed); return; };
        if self.open.swap(0, Relaxed) == 0 { scene.frame.store(self.frame.load(Relaxed), Relaxed); }
        scene.end_enter.store(enter, Relaxed);
        scene.end_us.store(blocked_us, Relaxed);
        scene.inflight_end.store(inflight, Relaxed);
        scene.fragment_value.store(fragment_value, Relaxed);
        scene.vertex_value.store(vertex_value, Relaxed);
        self.head.store(index + 1, Release);
    }
    pub(crate) fn swap(&self, enter: u32, us: u32) {
        self.swap_enter.store(enter, Relaxed);
        self.swap_us.store(us, Relaxed);
    }
    /// Main thread, once per presented frame, numbered as the survey CSV numbers it.
    pub(crate) fn presented(&self, frame: u32, now: u32, inflight: u32) {
        let Some(row) = (frame as usize).checked_sub(1).and_then(|i| self.frames.get(i)) else {
            self.frame_overflow.fetch_add(1, Relaxed); return; };
        row.presented.store(now, Relaxed);
        row.swap_enter.store(self.swap_enter.load(Relaxed), Relaxed);
        row.swap_us.store(self.swap_us.load(Relaxed), Relaxed);
        row.inflight.store(inflight, Relaxed);
        self.frame.store(frame.wrapping_add(1), Relaxed);
    }
    /// Display queue thread, before the flip is requested.
    pub(crate) fn display_callback(&self, now: u32) {
        match self.display.get(self.display_next.load(Relaxed)) {
            Some(row) => row.callback.store(now, Relaxed),
            None => { self.display_overflow.fetch_add(1, Relaxed); }
        }
    }
    /// Display queue thread, the vblank wait returned. Closes the entry.
    pub(crate) fn display_vblank(&self, now: u32) {
        let index = self.display_next.load(Relaxed);
        if let Some(row) = self.display.get(index) {
            row.vblank_done.store(now, Relaxed);
            self.display_next.store(index + 1, Relaxed);
        }
    }
    /// Timing thread, polling. Returns true when the oldest scene is finished and consumed.
    /// Stamps can coincide when both words moved inside one period: read them as an upper bound.
    pub(crate) fn poll_once(&self, now: u32, vertex_word: u32, fragment_word: u32) -> bool {
        let tail = self.tail.load(Relaxed);
        if tail >= self.head.load(Acquire) { return false; }
        let scene = &self.scenes[tail];
        let vertex = scene.vertex_value.load(Relaxed);
        if vertex != 0 && scene.vertex_done.load(Relaxed) == 0 && reached(vertex_word, vertex) {
            scene.vertex_done.store(now, Relaxed);
        }
        if !reached(fragment_word, scene.fragment_value.load(Relaxed)) { return false; }
        scene.fragment_done.store(now, Relaxed);
        self.tail.store(tail + 1, Release);
        true
    }
    pub(crate) fn drained(&self) -> bool { self.tail.load(Acquire) >= self.head.load(Acquire) }
    pub(crate) fn scene_count(&self) -> usize { self.head.load(Acquire) }
    /// The export record. Zero means no stamp. Times are process microseconds, low 32 bits.
    pub(crate) fn write_csv(&self, out: &mut impl std::io::Write) -> std::io::Result<()> {
        writeln!(out, "kind,frame,index,display,begin_enter_us,begin_us,inflight_begin,end_enter_us,end_us,inflight_end,fragment_value,vertex_value,vertex_done_us,fragment_done_us")?;
        for (index, s) in self.scenes[..self.scene_count().min(SCENE_CAPACITY)].iter().enumerate() {
            writeln!(out, "S,{},{index},{},{},{},{},{},{},{},{},{},{},{}", s.frame.load(Relaxed), s.display.load(Relaxed),
                s.begin_enter.load(Relaxed), s.begin_us.load(Relaxed), s.inflight_begin.load(Relaxed),
                s.end_enter.load(Relaxed), s.end_us.load(Relaxed), s.inflight_end.load(Relaxed),
                s.fragment_value.load(Relaxed), s.vertex_value.load(Relaxed),
                s.vertex_done.load(Relaxed), s.fragment_done.load(Relaxed))?;
        }
        for (index, f) in self.frames.iter().enumerate() {
            if f.presented.load(Relaxed) == 0 { continue; }
            // F rows: begin_enter is the presentation time, end_enter and end_us the swap.
            writeln!(out, "F,{},{index},,{},,{},{},{},,,,,", index + 1, f.presented.load(Relaxed), f.inflight.load(Relaxed),
                f.swap_enter.load(Relaxed), f.swap_us.load(Relaxed))?;
        }
        for (index, d) in self.display[..self.display_next.load(Relaxed).min(DISPLAY_CAPACITY)].iter().enumerate() {
            // D rows: begin_enter is the display callback, end_enter the vblank wait's return.
            writeln!(out, "D,,{index},,{},,,{},,,,,,", d.callback.load(Relaxed), d.vblank_done.load(Relaxed))?;
        }
        Ok(())
    }
    pub(crate) fn summary(&self, map: &str) -> String {
        format!("[lg/gpu-timing] map={map} mode={} scenes={} drained={} overflow={}/{}/{} vertex_notifications={}",
            self.mode.name(), self.scene_count(), self.drained(), self.scene_overflow.load(Relaxed),
            self.frame_overflow.load(Relaxed), self.display_overflow.load(Relaxed), self.vertex_value.load(Relaxed))
    }
}

static ARMED: AtomicBool = AtomicBool::new(false);
static LOG: AtomicPtr<Log> = AtomicPtr::new(std::ptr::null_mut());
#[inline] fn log() -> Option<&'static Log> {
    let pointer = LOG.load(Acquire);
    // Published once by `prepare`, leaked for the life of the process.
    if pointer.is_null() { None } else { Some(unsafe { &*pointer }) }
}
#[inline] pub(crate) fn armed() -> bool { ARMED.load(Relaxed) }

#[cfg(target_os = "vita")]
mod vita {
    use super::*;
    use core::ffi::c_void;
    #[repr(C)] pub(super) struct Notification { address: *mut u32, value: u32 }
    extern "C" {
        static query_fence: Notification;
        static gxm_render_target: *const c_void;
        fn __real_sceGxmBeginScene(context: *mut c_void, flags: u32, target: *const c_void, region: *const c_void,
            vertex_sync: *mut c_void, fragment_sync: *mut c_void, color: *const c_void, depth: *const c_void) -> i32;
        fn __real_sceGxmEndScene(context: *mut c_void, vertex: *const Notification, fragment: *const Notification) -> i32;
        fn __real_sceDisplayWaitVblankStartMulti(vcount: u32) -> i32;
        fn sceGxmGetNotificationRegion() -> *mut u32;
        fn sceGxmNotificationWait(notification: *const Notification) -> i32;
        fn sceKernelGetProcessTimeLow() -> u32;
        fn sceKernelDelayThread(microseconds: u32) -> i32;
        fn sceKernelCreateThread(name: *const u8, entry: extern "C" fn(u32, *mut c_void) -> i32, priority: i32,
            stack: i32, attributes: u32, affinity: i32, option: *const c_void) -> i32;
        fn sceKernelStartThread(id: i32, length: u32, argument: *const c_void) -> i32;
        fn vglSetDisplayCallback(callback: extern "C" fn(*mut c_void));
        fn vglSetupRenderTargetScenesNum(display: u8, fbo: u8);
        fn vglSetParamBufferSize(bytes: u32);
    }
    /// CPU0 is the game, CPU1 and CPU2 the workers, and CPU2 also the audio producer at priority 161.
    const THREAD_AFFINITY: i32 = 0x40000;
    const THREAD_PRIORITY: i32 = 120;
    const POLL_US: u32 = 1000;
    #[inline] fn now() -> u32 { unsafe { sceKernelGetProcessTimeLow() }.max(1) }
    #[inline] unsafe fn inflight() -> u32 {
        // Null until vitaGL has run reset_queries; the window only opens long after that.
        if query_fence.address.is_null() { return 0; }
        query_fence.value.wrapping_sub(core::ptr::read_volatile(query_fence.address))
    }
    static mut VERTEX: Notification = Notification { address: core::ptr::null_mut(), value: 0 };

    #[no_mangle]
    pub unsafe extern "C" fn __wrap_sceGxmBeginScene(context: *mut c_void, flags: u32, target: *const c_void, region: *const c_void,
        vertex_sync: *mut c_void, fragment_sync: *mut c_void, color: *const c_void, depth: *const c_void) -> i32 {
        if !armed() { return __real_sceGxmBeginScene(context, flags, target, region, vertex_sync, fragment_sync, color, depth); }
        let enter = now();
        let waiting = inflight();
        let result = __real_sceGxmBeginScene(context, flags, target, region, vertex_sync, fragment_sync, color, depth);
        if let Some(log) = log() { log.begin(target == gxm_render_target, enter, now().wrapping_sub(enter), waiting); }
        result
    }
    #[no_mangle]
    pub unsafe extern "C" fn __wrap_sceGxmEndScene(context: *mut c_void, vertex: *const Notification, fragment: *const Notification) -> i32 {
        let log_now = if armed() { log() } else { None };
        let Some(log) = log_now else { return __real_sceGxmEndScene(context, vertex, fragment); };
        let enter = now();
        let waiting = inflight();
        let threaded = log.mode.threaded() && vertex.is_null();
        let (vertex_argument, vertex_value) = if threaded {
            let value = log.next_vertex_value();
            let slot = core::ptr::addr_of_mut!(VERTEX);
            (*slot).address = sceGxmGetNotificationRegion().add(VERTEX_SLOT);
            (*slot).value = value;
            (slot as *const Notification, value)
        } else { (vertex, 0) };
        let result = __real_sceGxmEndScene(context, vertex_argument, fragment);
        let fragment_value = if fragment.is_null() { 0 } else { (*fragment).value };
        log.end(enter, now().wrapping_sub(enter), waiting, fragment_value, vertex_value);
        result
    }
    #[no_mangle]
    pub unsafe extern "C" fn __wrap_sceDisplayWaitVblankStartMulti(vcount: u32) -> i32 {
        let result = __real_sceDisplayWaitVblankStartMulti(vcount);
        if armed() { if let Some(log) = log() { log.display_vblank(now()); } }
        result
    }
    extern "C" fn display_callback(_framebuffer: *mut c_void) {
        if armed() { if let Some(log) = log() { log.display_callback(now()); } }
    }
    extern "C" fn timing_thread(_length: u32, _argument: *mut c_void) -> i32 {
        let Some(log) = log() else { return 0; };
        let region = unsafe { sceGxmGetNotificationRegion() };
        let (vertex_word, fragment_word) = unsafe { (region.add(VERTEX_SLOT), query_fence.address) };
        let read = |word: *mut u32| unsafe { core::ptr::read_volatile(word) };
        loop {
            if log.mode == Mode::Wait {
                let tail = log.tail.load(Relaxed);
                if tail < log.head.load(Acquire) {
                    let scene = &log.scenes[tail];
                    let (vertex, fragment) = (scene.vertex_value.load(Relaxed), scene.fragment_value.load(Relaxed));
                    if vertex != 0 && !reached(read(vertex_word), vertex) {
                        let target = Notification { address: vertex_word, value: vertex };
                        unsafe { sceGxmNotificationWait(&target); }
                    }
                    if vertex != 0 && scene.vertex_done.load(Relaxed) == 0 { scene.vertex_done.store(now(), Relaxed); }
                    if !reached(read(fragment_word), fragment) {
                        let target = Notification { address: fragment_word, value: fragment };
                        unsafe { sceGxmNotificationWait(&target); }
                    }
                }
            }
            if log.poll_once(now(), read(vertex_word), read(fragment_word)) { continue; }
            if log.stop.load(Relaxed) && log.drained() { break; }
            unsafe { sceKernelDelayThread(POLL_US); }
        }
        log.stopped.store(true, Release);
        0
    }
    /// Before vglInit. Replaces vitaGL's default of one scene per frame on each kind of target.
    pub(super) fn set_render_target_scenes(display: u8, fbo: u8) { unsafe { vglSetupRenderTargetScenesNum(display, fbo); } }
    pub(super) fn set_param_buffer(mib: u32) { unsafe { vglSetParamBufferSize(mib * 1024 * 1024); } }
    pub(super) fn start_thread(log: &'static Log) {
        unsafe {
            *sceGxmGetNotificationRegion().add(VERTEX_SLOT) = 0;
            vglSetDisplayCallback(display_callback);
            if !log.mode.threaded() { return; }
            let id = sceKernelCreateThread(b"lg-gpu-timing\0".as_ptr(), timing_thread, THREAD_PRIORITY, 0x4000, 0, THREAD_AFFINITY, core::ptr::null());
            let started = if id >= 0 { sceKernelStartThread(id, 0, core::ptr::null()) } else { id };
            println!("[lg/gpu-timing] thread id={id:#x} start={started:#x} priority={THREAD_PRIORITY} affinity={THREAD_AFFINITY:#x}");
        }
    }
    pub(super) fn stamp() -> u32 { now() }
    pub(super) fn inflight_now() -> u32 { unsafe { inflight() } }
    pub(super) fn delay(microseconds: u32) { unsafe { sceKernelDelayThread(microseconds); } }
}

/// Before vglInit, from the config alone. Off leaves vitaGL's defaults untouched.
pub(crate) fn before_gl_init() {
    #[cfg(feature = "vita_renderer")]
    {
        let config = crate::render::vita_renderer::config();
        set_mask(config.gpu_mask.iter().filter_map(|name| mask_bit(name)).fold(0, |all, bit| all | bit));
        #[cfg(target_os = "vita")]
        if let Some([display, fbo]) = config.rt_scenes { vita::set_render_target_scenes(display, fbo); }
        #[cfg(target_os = "vita")]
        if let Some(mib) = config.param_buffer_mib { vita::set_param_buffer(mib); }
    }
}
/// When the survey opens. All of the log's memory is taken here, never in a frame.
pub(crate) fn prepare() {
    #[cfg(feature = "vita_renderer")]
    {
        let mode = Mode::parse(&crate::render::vita_renderer::config().gpu_timing).unwrap_or(Mode::Off);
        if mode != Mode::Off && LOG.load(Relaxed).is_null() {
            LOG.store(Box::into_raw(Box::new(Log::new(mode))), Release);
        }
    }
}
/// The survey window opens. Runs before the window's first clock read.
pub(crate) fn arm() {
    let Some(_log) = log() else { return; };
    #[cfg(target_os = "vita")]
    vita::start_thread(_log);
    ARMED.store(true, Relaxed);
}
/// Swap brackets, from the existing `vglSwapBuffers` wrapper.
#[cfg(target_os = "vita")]
#[inline] pub(crate) fn swap_enter() -> u32 { if armed() { vita::stamp() } else { 0 } }
#[cfg(target_os = "vita")]
#[inline] pub(crate) fn swap_return(enter: u32) {
    if armed() { if let Some(log) = log() { log.swap(enter, vita::stamp().wrapping_sub(enter)); } }
}
#[cfg(target_os = "vita")]
#[inline] pub(crate) fn presented(frame: u32) {
    if armed() { if let Some(log) = log() { log.presented(frame, vita::stamp(), vita::inflight_now()); } }
}
/// After the window. Waits at most a second for scenes still in flight, then writes what it has.
#[cfg(target_os = "vita")]
pub(crate) fn export(map: &str) -> anyhow::Result<()> {
    let Some(log) = log() else { return Ok(()); };
    ARMED.store(false, Relaxed);
    log.stop.store(true, Relaxed);
    if log.mode.threaded() {
        for _ in 0..200 { if log.stopped.load(Acquire) { break; } vita::delay(5000); }
    }
    let path = format!("ux0:data/alice/private/frame_profile/{map}-r13b-gpu.csv");
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    log.write_csv(&mut out)?;
    use std::io::Write;
    out.flush()?;
    println!("{} incomplete={}", log.summary(map), log.mode.threaded() && !log.stopped.load(Acquire));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scene(log: &Log, display: bool, begin: u32, fragment: u32, vertex: u32) {
        log.begin(display, begin, 2, 1);
        log.end(begin + 10, 3, 2, fragment, vertex);
    }
    #[test]
    fn modes_parse_and_only_the_named_modes_exist() {
        for (text, mode) in [("off", Mode::Off), ("backlog", Mode::Backlog), ("poll", Mode::Poll), ("wait", Mode::Wait)] {
            assert_eq!(Mode::parse(text), Some(mode));
            assert_eq!(mode.name(), text);
        }
        assert_eq!(Mode::parse(""), None);
        assert_eq!(Mode::parse("on"), None);
        assert!(!Mode::Backlog.threaded() && Mode::Poll.threaded() && Mode::Wait.threaded());
    }
    #[test]
    fn notification_counters_compare_across_wrap() {
        assert!(reached(5, 5) && reached(6, 5) && !reached(4, 5));
        assert!(reached(0, u32::MAX) && reached(3, u32::MAX - 2) && !reached(u32::MAX, 0));
    }
    #[test]
    fn poll_stamps_in_scene_order_and_never_passes_an_unfinished_scene() {
        let log = Log::new(Mode::Poll);
        scene(&log, true, 100, 7, 1);
        scene(&log, false, 200, 8, 2);
        assert!(!log.poll_once(300, 0, 6), "nothing finished");
        assert_eq!(log.scenes[0].vertex_done.load(Relaxed), 0);
        assert!(!log.poll_once(310, 1, 6), "vertex done first");
        assert_eq!(log.scenes[0].vertex_done.load(Relaxed), 310);
        assert!(log.poll_once(320, 1, 7));
        assert_eq!(log.scenes[0].fragment_done.load(Relaxed), 320);
        assert_eq!(log.scenes[0].vertex_done.load(Relaxed), 310, "first stamp stays");
        assert_eq!(log.scenes[1].fragment_done.load(Relaxed), 0);
        assert!(log.poll_once(330, 2, 8));
        assert!(log.drained() && !log.poll_once(340, 2, 8));
    }
    #[test]
    fn a_skipped_poll_finishes_each_scene_it_passed() {
        let log = Log::new(Mode::Poll);
        scene(&log, true, 100, 7, 1);
        scene(&log, false, 200, 8, 2);
        assert!(log.poll_once(300, 2, 8));
        assert!(log.poll_once(300, 2, 8));
        assert_eq!(log.scenes[1].vertex_done.load(Relaxed), 300);
        assert!(log.drained());
    }
    #[test]
    fn backlog_scenes_have_no_vertex_stamp() {
        let log = Log::new(Mode::Backlog);
        scene(&log, true, 100, 7, 0);
        assert!(log.poll_once(300, 99, 7));
        assert_eq!(log.scenes[0].vertex_done.load(Relaxed), 0);
    }
    #[test]
    fn a_scene_begun_before_the_window_is_kept_with_empty_begin_stamps() {
        let log = Log::new(Mode::Backlog);
        log.end(50, 1, 1, 3, 0);
        assert_eq!(log.scene_count(), 1);
        assert_eq!(log.scenes[0].begin_enter.load(Relaxed), 0);
        assert_eq!(log.scenes[0].frame.load(Relaxed), 1);
    }
    #[test]
    fn scenes_carry_the_frame_the_survey_will_number_them() {
        let log = Log::new(Mode::Backlog);
        scene(&log, true, 100, 1, 0);
        log.presented(1, 150, 2);
        scene(&log, true, 200, 2, 0);
        log.presented(2, 250, 1);
        assert_eq!(log.scenes[0].frame.load(Relaxed), 1);
        assert_eq!(log.scenes[1].frame.load(Relaxed), 2);
    }
    #[test]
    fn overflow_is_counted_and_never_writes_past_the_log() {
        let log = Log::new(Mode::Backlog);
        let (scene_ptr, frame_ptr, display_ptr) = (log.scenes.as_ptr(), log.frames.as_ptr(), log.display.as_ptr());
        for i in 0..SCENE_CAPACITY as u32 + 5 { scene(&log, false, 10 + i, i + 1, 0); }
        for i in 0..FRAME_CAPACITY as u32 + 5 { log.presented(i + 1, 10 + i, 0); }
        for i in 0..DISPLAY_CAPACITY as u32 + 5 { log.display_callback(10 + i); log.display_vblank(20 + i); }
        assert_eq!(log.scene_count(), SCENE_CAPACITY);
        assert_eq!(log.scene_overflow.load(Relaxed), 10, "one each from begin and end");
        assert_eq!(log.frame_overflow.load(Relaxed), 5);
        assert_eq!(log.display_next.load(Relaxed), DISPLAY_CAPACITY);
        assert_eq!(log.display_overflow.load(Relaxed), 5);
        assert_eq!((log.scenes.as_ptr(), log.frames.as_ptr(), log.display.as_ptr()), (scene_ptr, frame_ptr, display_ptr));
        log.presented(0, 1, 0);
        assert_eq!(log.frame_overflow.load(Relaxed), 6);
    }
    #[test]
    fn export_has_one_row_per_record_and_equal_columns() {
        let log = Log::new(Mode::Poll);
        scene(&log, true, 100, 7, 1);
        assert!(log.poll_once(150, 1, 7));
        log.swap(160, 4);
        log.presented(1, 170, 0);
        log.display_callback(180);
        log.display_vblank(190);
        let mut out = Vec::new();
        log.write_csv(&mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4, "{text}");
        let columns = lines[0].split(',').count();
        for line in &lines { assert_eq!(line.split(',').count(), columns, "{line}"); }
        assert_eq!(lines[1], "S,1,0,1,100,2,1,110,3,2,7,1,150,150");
        assert_eq!(lines[2], "F,1,0,,170,,0,160,4,,,,,");
        assert_eq!(lines[3], "D,,0,,180,,,190,,,,,,");
        assert!(log.summary("utemple").starts_with("[lg/gpu-timing] map=utemple mode=poll scenes=1 drained=true overflow=0/0/0"));
    }
    #[test]
    fn masks_name_every_class_once() {
        let names = ["sky", "world_low", "world_full", "world_alpha", "actors", "fx_alpha", "fx_add", "fx_other"];
        let bits: Vec<u32> = names.iter().map(|name| mask_bit(name).unwrap()).collect();
        assert_eq!(bits.iter().sum::<u32>(), bits.iter().fold(0, |all, bit| all | bit));
        assert_eq!(mask_bit("bubbles"), None);
        assert_eq!(fx_class(1), MASK_FX_ALPHA);
        assert_eq!(fx_class(2), MASK_FX_ADD);
        assert_eq!(fx_class(5), MASK_FX_ADD);
        assert_eq!(fx_class(3), MASK_FX_OTHER);
    }
}
