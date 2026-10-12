//! Opt-in failure isolation. A mask-bypassed build is not renderer validation.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
use std::sync::OnceLock;

/// Independent draw controls; absent fields preserve the requested workload.
/// This file is read only by explicit Vita diagnostic builds.
#[derive(Default, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub actors: Option<bool>,
    pub objects: Option<bool>,
    pub effects: Option<bool>,
    pub hud: Option<bool>,
    pub fx_queue: Option<bool>,
    pub depth_flush: Option<bool>,
    pub trace: bool,
    pub trace_gpu: bool,
    pub soak_seconds: u64,
}

pub fn config() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let path = "ux0:data/alice/private/vita3k_diagnostics.json";
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).expect("invalid Vita3K diagnostic config"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(e) => panic!("cannot read {path}: {e}"),
        }
    })
}

pub struct Trace(Option<(&'static str, usize)>);
pub fn trace(name: &'static str) -> Trace {
    if config().trace && FRAME.load(Relaxed) <= 4 {
        static CALL: AtomicUsize = AtomicUsize::new(0);
        let id = CALL.fetch_add(1, Relaxed);
        println!(
            "[lg/vita3k-diagnostic] enter {name} id={id} frame={}",
            FRAME.load(Relaxed)
        );
        Trace(Some((name, id)))
    } else {
        Trace(None)
    }
}
impl Drop for Trace {
    fn drop(&mut self) {
        if let Some((name, id)) = self.0 {
            println!("[lg/vita3k-diagnostic] exit {name} id={id}");
        }
    }
}

static LIVE: AtomicUsize = AtomicUsize::new(0);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static FREES: AtomicUsize = AtomicUsize::new(0);
static FRAME: AtomicUsize = AtomicUsize::new(0);
static SOAKING: AtomicBool = AtomicBool::new(false);
static BYPASS: AtomicBool = AtomicBool::new(false);
static IN_MASK_UPDATE: AtomicBool = AtomicBool::new(false);
static FINISH_CALLS: AtomicUsize = AtomicUsize::new(0);
static MASK_CALLS: AtomicUsize = AtomicUsize::new(0);
static COMPILES: AtomicUsize = AtomicUsize::new(0);
static LINKS: AtomicUsize = AtomicUsize::new(0);
static PROGRAMS: AtomicUsize = AtomicUsize::new(0);
static DELETES: AtomicUsize = AtomicUsize::new(0);
static MATERIALS: AtomicUsize = AtomicUsize::new(0);
static MATERIAL_SUCCESSES: AtomicUsize = AtomicUsize::new(0);
static NEW_SHADERS: AtomicUsize = AtomicUsize::new(0);
static NEW_PIPELINES: AtomicUsize = AtomicUsize::new(0);

pub fn start_soak() {
    SOAKING.store(true, Relaxed);
}
pub fn soaking() -> bool {
    SOAKING.load(Relaxed)
}

// Vita newlib malloc.h: ten size_t fields, returned by value.
#[repr(C)]
struct MallInfo {
    fields: [usize; 10],
}
extern "C" {
    fn mallinfo() -> MallInfo;
    fn sceKernelGetProcessTimeWide() -> u64;
    fn __real_vglGetProcAddress(name: *const core::ffi::c_char) -> *mut core::ffi::c_void;
    fn glCompileShader(shader: u32);
    fn glLinkProgram(program: u32);
    fn glCreateProgram() -> u32;
    fn glDeleteProgram(program: u32);
    fn glCreateShader(kind: u32) -> u32;
    fn glGetAttribLocation(program: u32, name: *const core::ffi::c_char) -> i32;
    fn __real_vglSwapBuffers(dialog: u8);
    fn __real_sceGxmDisplayQueueAddEntry(
        old: *mut core::ffi::c_void,
        new: *mut core::ffi::c_void,
        data: *const core::ffi::c_void,
    ) -> i32;
    fn __real_sceGxmEndScene(
        context: *mut core::ffi::c_void,
        vertex: *const core::ffi::c_void,
        fragment: *const core::ffi::c_void,
    ) -> i32;
    fn __real_sceGxmNotificationWait(notification: *const core::ffi::c_void) -> i32;
}
fn heap() -> usize {
    unsafe { mallinfo().fields[7] }
}
fn counters(reason: &str) {
    println!("[lg/vita3k-diagnostic] counters reason={reason} time_us={} frame={} rust_live={} newlib_uordblks={} material_attempts={} material_successes={} glCompileShader={} glLinkProgram={} glCreateProgram={} glDeleteProgram={} miniquad_new_shader={} miniquad_new_pipeline={}",
        unsafe { sceKernelGetProcessTimeWide() }, FRAME.load(Relaxed), LIVE.load(Relaxed), heap(),
        MATERIALS.load(Relaxed), MATERIAL_SUCCESSES.load(Relaxed), COMPILES.load(Relaxed), LINKS.load(Relaxed), PROGRAMS.load(Relaxed), DELETES.load(Relaxed), NEW_SHADERS.load(Relaxed), NEW_PIPELINES.load(Relaxed));
}
pub fn material_enter(site: &str, variant: usize) {
    let id = MATERIALS.fetch_add(1, Relaxed);
    println!("[lg/vita3k-diagnostic] material enter site={site} variant={variant} id={id} frame={} newlib_uordblks={}", FRAME.load(Relaxed), heap());
}
pub fn material_exit(site: &str, variant: usize) {
    MATERIAL_SUCCESSES.fetch_add(1, Relaxed);
    println!("[lg/vita3k-diagnostic] material exit site={site} variant={variant} frame={} newlib_uordblks={}", FRAME.load(Relaxed), heap());
}

unsafe extern "C" fn compile_shader(shader: u32) {
    let id = COMPILES.fetch_add(1, Relaxed);
    let before = heap();
    println!("[lg/vita3k-diagnostic] glCompileShader enter id={id} shader={shader} frame={} newlib_uordblks={before}", FRAME.load(Relaxed));
    glCompileShader(shader);
    println!(
        "[lg/vita3k-diagnostic] glCompileShader exit id={id} newlib_uordblks={} delta={}",
        heap(),
        heap() as isize - before as isize
    );
}
unsafe extern "C" fn link_program(program: u32) {
    let id = LINKS.fetch_add(1, Relaxed);
    let before = heap();
    println!("[lg/vita3k-diagnostic] glLinkProgram enter id={id} program={program} frame={} newlib_uordblks={before}", FRAME.load(Relaxed));
    glLinkProgram(program);
    println!(
        "[lg/vita3k-diagnostic] glLinkProgram exit id={id} newlib_uordblks={} delta={}",
        heap(),
        heap() as isize - before as isize
    );
}
unsafe extern "C" fn create_program() -> u32 {
    PROGRAMS.fetch_add(1, Relaxed);
    glCreateProgram()
}
unsafe extern "C" fn delete_program(program: u32) {
    DELETES.fetch_add(1, Relaxed);
    glDeleteProgram(program);
}
// These are call-boundary proxies for this pinned miniquad revision: each
// new_shader creates one vertex shader, and each new_pipeline queries position
// once. All LG layouts include position; no other backend path queries it.
unsafe extern "C" fn create_shader(kind: u32) -> u32 {
    if kind == 0x8B31 {
        NEW_SHADERS.fetch_add(1, Relaxed);
    }
    glCreateShader(kind)
}
unsafe extern "C" fn attribute_location(program: u32, name: *const core::ffi::c_char) -> i32 {
    if std::ffi::CStr::from_ptr(name).to_bytes() == b"position" {
        NEW_PIPELINES.fetch_add(1, Relaxed);
    }
    glGetAttribLocation(program, name)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wrap_vglGetProcAddress(
    name: *const core::ffi::c_char,
) -> *mut core::ffi::c_void {
    match std::ffi::CStr::from_ptr(name).to_bytes() {
        b"glCompileShader" => compile_shader as *const () as *mut _,
        b"glLinkProgram" => link_program as *const () as *mut _,
        b"glCreateProgram" => create_program as *const () as *mut _,
        b"glDeleteProgram" => delete_program as *const () as *mut _,
        b"glCreateShader" => create_shader as *const () as *mut _,
        b"glGetAttribLocation" => attribute_location as *const () as *mut _,
        _ => __real_vglGetProcAddress(name),
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wrap_vglSwapBuffers(dialog: u8) {
    let _call = trace("vglSwapBuffers");
    __real_vglSwapBuffers(dialog);
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wrap_sceGxmDisplayQueueAddEntry(
    old: *mut core::ffi::c_void,
    new: *mut core::ffi::c_void,
    data: *const core::ffi::c_void,
) -> i32 {
    let _call = trace("sceGxmDisplayQueueAddEntry");
    __real_sceGxmDisplayQueueAddEntry(old, new, data)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wrap_sceGxmEndScene(
    context: *mut core::ffi::c_void,
    vertex: *const core::ffi::c_void,
    fragment: *const core::ffi::c_void,
) -> i32 {
    let _call = trace("sceGxmEndScene");
    __real_sceGxmEndScene(context, vertex, fragment)
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wrap_sceGxmNotificationWait(
    notification: *const core::ffi::c_void,
) -> i32 {
    let _call = trace("sceGxmNotificationWait");
    __real_sceGxmNotificationWait(notification)
}

struct Tracked;
#[global_allocator]
static ALLOCATOR: Tracked = Tracked;
unsafe impl GlobalAlloc for Tracked {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            LIVE.fetch_add(layout.size(), Relaxed);
            ALLOCS.fetch_add(1, Relaxed);
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc_zeroed(layout);
        if !p.is_null() {
            LIVE.fetch_add(layout.size(), Relaxed);
            ALLOCS.fetch_add(1, Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        System.dealloc(p, layout);
        LIVE.fetch_sub(layout.size(), Relaxed);
        FREES.fetch_add(1, Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        let result = System.realloc(p, old, new_size);
        if !result.is_null() {
            LIVE.fetch_add(new_size, Relaxed);
            LIVE.fetch_sub(old.size(), Relaxed);
        }
        result
    }
}

pub fn initialize() {
    let bypass = std::path::Path::new("ux0:data/alice/private/vita3k_disable_mask").exists();
    BYPASS.store(bypass, Relaxed);
    println!(
        "[lg/vita3k-diagnostic] build enabled; diagnostic build, not the console binary; config={}",
        serde_json::to_string(config()).unwrap()
    );
    println!("[lg/vita3k-diagnostic] mask_bypass={bypass}; bypass is diagnostic only and disables scissor clipping");
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_secs(10));
        counters("tick");
    });
}
pub fn frame() {
    let n = FRAME.fetch_add(1, Relaxed);
    if n < 5 || n % 10 == 0 {
        counters("frame");
        println!(
            "[lg/vita3k-diagnostic] frame={n} rust_live={} allocations={} frees={} mask_calls={}",
            LIVE.load(Relaxed),
            ALLOCS.load(Relaxed),
            FREES.load(Relaxed),
            MASK_CALLS.load(Relaxed)
        );
    }
}
extern "C" {
    fn __real_update_scissor_test();
    fn __real_sceGxmFinish(context: *mut core::ffi::c_void);
    fn __real_sceGxmDrawPrecomputed(
        context: *mut core::ffi::c_void,
        draw: *const core::ffi::c_void,
    ) -> i32;
    fn __real_sceGxmDraw(
        context: *mut core::ffi::c_void,
        primitive: u32,
        index_format: u32,
        indices: *const core::ffi::c_void,
        count: u32,
    ) -> i32;
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wrap_update_scissor_test() {
    MASK_CALLS.fetch_add(1, Relaxed);
    IN_MASK_UPDATE.store(true, Relaxed);
    __real_update_scissor_test();
    IN_MASK_UPDATE.store(false, Relaxed);
}

// Keep vitaGL viewport/stencil/uniform restoration while avoiding mask triangles.
// The emulator does not implement the mask bit, so clipping remains diagnostic-only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wrap_sceGxmDraw(
    context: *mut core::ffi::c_void,
    primitive: u32,
    index_format: u32,
    indices: *const core::ffi::c_void,
    count: u32,
) -> i32 {
    if BYPASS.load(Relaxed) && IN_MASK_UPDATE.load(Relaxed) {
        0
    } else {
        let _call = config().trace_gpu.then(|| trace("sceGxmDraw"));
        __real_sceGxmDraw(context, primitive, index_format, indices, count)
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wrap_sceGxmDrawPrecomputed(
    context: *mut core::ffi::c_void,
    draw: *const core::ffi::c_void,
) -> i32 {
    if BYPASS.load(Relaxed) && IN_MASK_UPDATE.load(Relaxed) {
        0
    } else {
        let _call = config().trace_gpu.then(|| trace("sceGxmDrawPrecomputed"));
        __real_sceGxmDrawPrecomputed(context, draw)
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wrap_sceGxmFinish(context: *mut core::ffi::c_void) {
    let id = FINISH_CALLS.fetch_add(1, Relaxed);
    println!("[lg/vita3k-diagnostic] gxm_finish enter id={id} context={context:p}");
    __real_sceGxmFinish(context);
    println!("[lg/vita3k-diagnostic] gxm_finish exit id={id}");
}
