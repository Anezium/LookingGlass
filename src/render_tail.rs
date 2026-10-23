//! Observe deferred framework submissions without flushing or changing GL state.
//! Program identity separates the final upscale from native-resolution HUD work.
use crate::light_profile::{RenderMetrics, RenderPhase};
use std::{cell::RefCell, time::Instant};
#[derive(Default)]
struct State {
    tail: bool,
    owner: Option<RenderPhase>,
    program: u32,
    present: u32,
    pending: RenderMetrics,
    start: Option<Instant>,
}
impl State {
    fn complete(&mut self, now:Instant, outside_cpu:bool) -> Option<(RenderPhase,RenderMetrics)> {
        if !self.tail {return None;}
        let elapsed=self.start.take().map_or(0,|v|now.duration_since(v).as_nanos() as u64);
        let mut m=std::mem::take(&mut self.pending);
        m.draws=1; m.pipelines=1; m.bindings=1;
        m.uniform_uploads=u64::from(m.uniform_gl_calls>0);
        m.cpu_ns=elapsed; m.deferred_cpu_ns=if outside_cpu {elapsed} else {0};
        Some((if self.present!=0 && self.program==self.present {RenderPhase::Present} else {RenderPhase::Hud},m))
    }
}
thread_local! { static SUBMIT: RefCell<crate::render::submit_gl::Cache> = RefCell::default(); }
pub(crate) struct Submission(bool);
pub(crate) fn submission(bindings:bool, raster:bool) -> Submission {
    let active = bindings || raster;
    if active { SUBMIT.with(|s|s.borrow_mut().begin(bindings,raster)); }
    Submission(active)
}
impl Drop for Submission { fn drop(&mut self) {if self.0 {SUBMIT.with(|s|s.borrow_mut().end());}} }
fn filter(kind:u8,key:u32,value:[u32;4])->bool {SUBMIT.with(|s|s.borrow_mut().raster(kind,key,value))}
fn raw_call(name: &'static str) {
    #[cfg(all(not(target_os="vita"),feature="sim_maps_probe"))]
    if crate::light_profile::full() { STATE.with(|s| {let s=s.borrow(); if !s.tail && matches!(s.owner,Some(RenderPhase::WorldOpaque)) {crate::sim_maps::count(name,1);} }); }
    let _=name;
}
thread_local! { static STATE: RefCell<State> = RefCell::default(); }
pub(crate) struct Owner(Option<RenderPhase>);
pub(crate) fn owner(phase: RenderPhase) -> Owner {
    Owner(STATE.with(|s| s.borrow_mut().owner.replace(phase)))
}
impl Drop for Owner { fn drop(&mut self) { STATE.with(|s| s.borrow_mut().owner = self.0); } }
pub(crate) fn begin() {
    STATE.with(|s| { let mut s=s.borrow_mut(); s.tail=true; s.pending=Default::default(); s.start=None; });
}
pub(crate) fn end() {
    STATE.with(|s| { let mut s=s.borrow_mut(); s.tail=false; s.pending=Default::default(); s.start=None; });
}
pub(crate) fn register(program:u32, name:&str) {
    SUBMIT.with(|s|s.borrow_mut().invalidate(program));
    if name.ends_with("render/vita_renderer/scale.rs") { STATE.with(|s| s.borrow_mut().present=program); }
}
pub(crate) fn unregister(program:u32) {
    SUBMIT.with(|s|s.borrow_mut().invalidate(program));
    STATE.with(|s| { let mut s=s.borrow_mut(); if s.present==program {s.present=0;} });
}
fn event(update: impl FnOnce(&mut RenderMetrics)) {
    if !crate::light_profile::render_active() { return; }
    STATE.with(|s| { let mut s=s.borrow_mut(); if s.tail {
        if s.start.is_none() {s.start=Some(Instant::now());}
        update(&mut s.pending);
    } });
}
fn raster() {
    if !crate::light_profile::render_active() {return;}
    let owner=STATE.with(|s| {let s=s.borrow(); (!s.tail).then_some(s.owner).flatten()});
    if let Some(phase)=owner {crate::light_profile::render_add(phase,RenderMetrics {state_gl_calls:1,..Default::default()});}
    event(|m|m.state_gl_calls+=1);
}
fn program(id:u32) {
    let (changed,owner,tail)=STATE.with(|s| {let mut s=s.borrow_mut();let changed=s.program!=id;s.program=id;(changed,s.owner,s.tail)});
    if changed && crate::light_profile::render_active() {
        if !tail { if let Some(p)=owner {crate::light_profile::render_add(p,RenderMetrics {program_gl_changes:1,..Default::default()});} }
        event(|m|m.program_gl_changes+=1);
    }
}
fn texture() {
    if !crate::light_profile::render_active() {return;}
    let owner=STATE.with(|s| {let s=s.borrow(); (!s.tail).then_some(s.owner).flatten()});
    if let Some(p)=owner {crate::light_profile::render_add(p,RenderMetrics {texture_gl_binds:1,..Default::default()});}
    event(|m|m.texture_gl_binds+=1);
}
fn uniform(count:i32, width:u64) { event(|m| {m.uniform_gl_calls+=1;m.uniform_bytes+=count.max(0) as u64*width*4;}); }
fn buffer(size:isize, data:*const std::ffi::c_void) {
    if !data.is_null() {event(|m|{m.buffer_updates+=1;m.buffer_bytes+=size.max(0) as u64;});}
}
fn draw() {
    if !crate::light_profile::render_active() { return; }
    let packet=STATE.with(|s| {
        let mut s=s.borrow_mut();
        // Direct world/FX draws have no deferred packet. Avoid a clock read for
        // each one unless explicitly measuring the legacy per-draw overhead.
        if !s.tail && !crate::render::vita_renderer::config().render_draw_timing {return None;}
        s.complete(Instant::now(),!crate::light_profile::cpu_running())
    });
    if let Some((phase,m))=packet {crate::light_profile::render_add(phase,m);crate::light_profile::deferred_cpu(m.cpu_ns);}
}

#[cfg(any(target_os="vita", target_os="macos"))]
mod hooks {
    use super::*;
    use std::ffi::{c_char,c_void};
    // Miniquad owns one GL context and calls its table on the render thread only.
    // Select these targets at frame boundaries, never during a draw or worker job.
    macro_rules! api {
        ($($name:ident($($arg:ident:$ty:ty),*));* $(;)?) => {
            #[derive(Clone, Copy)]
            #[allow(non_snake_case)]
            struct Dispatch { $($name: unsafe extern "C" fn($($ty),*),)* }
            static mut DISPATCH: Dispatch = Dispatch { $($name: api::$name,)* };
            static DIRECT: std::sync::OnceLock<Dispatch> = std::sync::OnceLock::new();
            mod entry {
                use super::*;
                $(#[allow(non_snake_case)] pub unsafe extern "C" fn $name($($arg:$ty),*) {
                    // No TLS, profiling, cache flags, or RefCell on the raw path.
                    (DISPATCH.$name)($($arg),*);
                })*
            }
            fn dispatch(observed: bool) -> Dispatch {
                DIRECT.get_or_init(|| Dispatch {
                    $($name: {
                        #[cfg(target_os="vita")] { api::$name }
                        #[cfg(target_os="macos")] { unsafe {std::mem::transmute(api::raw(stringify!($name)))} }
                    },)*
                }).with_observers(observed)
            }
            impl Dispatch {
                fn with_observers(mut self, observed: bool) -> Self {
                    if observed { $(if let Some(p)=lookup_observed(stringify!($name).as_bytes()) {
                        self.$name=unsafe {std::mem::transmute(p)};
                    })* }
                    self
                }
            }
            pub(crate) fn lookup(name:&[u8])->Option<*mut c_void> {
                Some(match name { $(n if n==stringify!($name).as_bytes()=>entry::$name as *const () as _,)* _=>return None })
            }
            #[cfg(target_os="vita")]
            mod api { use super::*; extern "C" {$(pub fn $name($($arg:$ty),*);)*} }
            #[cfg(target_os="macos")]
            mod api {
                use super::*;
                extern "C" {fn dlopen(p:*const c_char,flags:i32)->*mut c_void;fn dlsym(h:*mut c_void,n:*const c_char)->*mut c_void;}
                pub unsafe fn raw(name:&str)->*mut c_void {
                    static LIB:std::sync::OnceLock<usize>=std::sync::OnceLock::new();
                    let h=*LIB.get_or_init(||dlopen(c"/System/Library/Frameworks/OpenGL.framework/Versions/Current/OpenGL".as_ptr(),1) as usize);
                    assert_ne!(h,0,"OpenGL library unavailable");
                    let n=std::ffi::CString::new(name).unwrap();dlsym(h as _,n.as_ptr())
                }
                $(#[allow(non_snake_case)] pub unsafe extern "C" fn $name($($arg:$ty),*) {
                    static FN:std::sync::OnceLock<unsafe extern "C" fn($($ty),*)>=std::sync::OnceLock::new();
                    let f=FN.get_or_init(|| {let p=raw(stringify!($name));assert!(!p.is_null(),"missing OpenGL entry");std::mem::transmute(p)});
                    f($($arg),*);
                })*
            }
        }
    }
    api! {
        glUseProgram(p:u32); glBindTexture(t:u32,p:u32); glDepthMask(v:u8);
        glEnable(v:u32); glDisable(v:u32); glDepthFunc(v:u32); glCullFace(v:u32); glFrontFace(v:u32);
        glBlendFuncSeparate(a:u32,b:u32,c:u32,d:u32); glBlendEquationSeparate(a:u32,b:u32);
        glViewport(x:i32,y:i32,w:i32,h:i32); glScissor(x:i32,y:i32,w:i32,h:i32);
        glUniform1i(l:i32,v:i32); glUniform1fv(l:i32,n:i32,p:*const f32); glUniform2fv(l:i32,n:i32,p:*const f32);
        glUniform3fv(l:i32,n:i32,p:*const f32); glUniform4fv(l:i32,n:i32,p:*const f32);
        glUniformMatrix4fv(l:i32,n:i32,t:u8,p:*const f32);
        glBufferData(t:u32,n:isize,p:*const c_void,u:u32); glBufferSubData(t:u32,o:isize,n:isize,p:*const c_void);
        glDrawElementsInstanced(t:u32,n:i32,k:u32,p:*const c_void,i:i32);
        glLinkProgram(p:u32); glDeleteProgram(p:u32);
        glBindBuffer(t:u32,b:u32); glActiveTexture(t:u32);
        glVertexAttribPointer(i:u32,n:i32,t:u32,z:u8,s:i32,p:*const c_void);
        glEnableVertexAttribArray(i:u32); glDisableVertexAttribArray(i:u32); glVertexAttribDivisor(i:u32,d:u32)
    }
    macro_rules! wrap {
        ($($fn:ident => $real:ident($($arg:ident:$ty:ty),*) {$($before:tt)*});* $(;)?) => {$ (
            unsafe extern "C" fn $fn($($arg:$ty),*) { $($before)* raw_call(concat!("world_gl_",stringify!($real))); api::$real($($arg),*); }
        )*};
    }
    wrap! {
        use_program => glUseProgram(p:u32) {program(p);if !SUBMIT.with(|s|s.borrow_mut().program(p)) {return;} }; bind_texture => glBindTexture(t:u32,p:u32) {texture();};
        depth_mask => glDepthMask(v:u8) {if !filter(1,0,[v as u32,0,0,0]) {return;} raster();}; enable => glEnable(v:u32) {if !filter(2,v,[1,0,0,0]) {return;} raster();}; disable => glDisable(v:u32) {if !filter(2,v,[0,0,0,0]) {return;} raster();};
        depth_func => glDepthFunc(v:u32) {if !filter(3,0,[v,0,0,0]) {return;} raster();}; cull_face => glCullFace(v:u32) {if !filter(4,0,[v,0,0,0]) {return;} raster();}; front_face => glFrontFace(v:u32) {if !filter(5,0,[v,0,0,0]) {return;} raster();};
        blend_func => glBlendFuncSeparate(a:u32,b:u32,c:u32,d:u32) {if !filter(6,0,[a,b,c,d]) {return;} raster();}; blend_equation => glBlendEquationSeparate(a:u32,b:u32) {if !filter(7,0,[a,b,0,0]) {return;} raster();};
        viewport => glViewport(x:i32,y:i32,w:i32,h:i32) {raster();}; scissor => glScissor(x:i32,y:i32,w:i32,h:i32) {raster();};
        uniform_i => glUniform1i(l:i32,v:i32) {if !SUBMIT.with(|s|s.borrow_mut().sampler(l,v)) {return;} event(|m|m.texture_bind_slots+=1);};
        uniform1 => glUniform1fv(l:i32,n:i32,p:*const f32) {uniform(n,1);}; uniform2 => glUniform2fv(l:i32,n:i32,p:*const f32) {uniform(n,2);};
        uniform3 => glUniform3fv(l:i32,n:i32,p:*const f32) {uniform(n,3);}; uniform4 => glUniform4fv(l:i32,n:i32,p:*const f32) {uniform(n,4);};
        uniform_matrix => glUniformMatrix4fv(l:i32,n:i32,t:u8,p:*const f32) {uniform(n,16);};
        bind_buffer => glBindBuffer(t:u32,b:u32) {};
        active_texture => glActiveTexture(t:u32) {};
        attrib_pointer => glVertexAttribPointer(i:u32,n:i32,t:u32,z:u8,s:i32,p:*const c_void) {};
        enable_attrib => glEnableVertexAttribArray(i:u32) {};
        disable_attrib => glDisableVertexAttribArray(i:u32) {};
        attrib_divisor => glVertexAttribDivisor(i:u32,d:u32) {};
        buffer_data => glBufferData(t:u32,n:isize,p:*const c_void,u:u32) {buffer(n,p);};
        buffer_sub => glBufferSubData(t:u32,o:isize,n:isize,p:*const c_void) {buffer(n,p);}
    }
    unsafe extern "C" fn elements(t:u32,n:i32,k:u32,p:*const c_void,i:i32) {raw_call("world_gl_glDrawElementsInstanced");api::glDrawElementsInstanced(t,n,k,p,i);draw();}
    #[cfg(target_os="macos")]
    unsafe extern "C" fn link(p:u32) {api::glLinkProgram(p);register(p,crate::shader_cache::name());}
    #[cfg(target_os="macos")]
    unsafe extern "C" fn delete(p:u32) {unregister(p);api::glDeleteProgram(p);}
    fn lookup_observed(name:&[u8])->Option<*mut c_void> {
        Some(match name {
            b"glUseProgram"=>use_program as *const () as _, b"glBindTexture"=>bind_texture as *const () as _,
            b"glDepthMask"=>depth_mask as *const () as _, b"glEnable"=>enable as *const () as _, b"glDisable"=>disable as *const () as _,
            b"glDepthFunc"=>depth_func as *const () as _, b"glCullFace"=>cull_face as *const () as _, b"glFrontFace"=>front_face as *const () as _,
            b"glBlendFuncSeparate"=>blend_func as *const () as _, b"glBlendEquationSeparate"=>blend_equation as *const () as _,
            b"glViewport"=>viewport as *const () as _, b"glScissor"=>scissor as *const () as _,
            b"glUniform1i"=>uniform_i as *const () as _, b"glUniform1fv"=>uniform1 as *const () as _, b"glUniform2fv"=>uniform2 as *const () as _,
            b"glUniform3fv"=>uniform3 as *const () as _, b"glUniform4fv"=>uniform4 as *const () as _, b"glUniformMatrix4fv"=>uniform_matrix as *const () as _,
            b"glBufferData"=>buffer_data as *const () as _, b"glBufferSubData"=>buffer_sub as *const () as _, b"glDrawElementsInstanced"=>elements as *const () as _,
            #[cfg(target_os="macos")] b"glLinkProgram"=>link as *const () as _,
            #[cfg(target_os="macos")] b"glDeleteProgram"=>delete as *const () as _,
            b"glBindBuffer"=>bind_buffer as *const () as _, b"glActiveTexture"=>active_texture as *const () as _,
            b"glVertexAttribPointer"=>attrib_pointer as *const () as _, b"glEnableVertexAttribArray"=>enable_attrib as *const () as _,
            b"glDisableVertexAttribArray"=>disable_attrib as *const () as _, b"glVertexAttribDivisor"=>attrib_divisor as *const () as _,
            _=>return None,
        })
    }
    #[cfg(test)]
    mod dispatch_tests {
        use super::*;
        #[test]
        fn raw_targets_bypass_hooks_and_observed_targets_retain_them() {
            let raw=dispatch(false);
            let observed=dispatch(true);
            macro_rules! check { ($($name:ident),*) => { $(
                assert_eq!(observed.$name as *const () as usize,
                    lookup_observed(stringify!($name).as_bytes()).unwrap() as usize);
                assert_ne!(raw.$name as *const () as usize, observed.$name as *const () as usize);
            )* }; }
            check!(glUseProgram,glUniform1i,glBindBuffer,glActiveTexture,
                glVertexAttribPointer,glEnableVertexAttribArray,glVertexAttribDivisor,
                glDepthMask,glBufferSubData,glDrawElementsInstanced);
            #[cfg(target_os="macos")]
            assert_eq!(raw.glBindBuffer as *const () as usize,unsafe {api::raw("glBindBuffer")} as usize);
            #[cfg(target_os="vita")]
            assert_eq!(raw.glBindBuffer as *const () as usize,api::glBindBuffer as *const () as usize);
        }
    }
    fn observed() -> bool {
        let c=crate::render::vita_renderer::config();
        if c.world_binding_cache || c.world_gl_state_cache {return true;}
        if c.light_gl_observers && c.profiler==Some(crate::light_profile::Level::Light) {return true;}
        // Otherwise retain the existing full-profiler observer policy.
        #[cfg(target_os="vita")]
        { crate::light_profile::full() }
        #[cfg(target_os="macos")]
        {
            static HOST:std::sync::OnceLock<bool>=std::sync::OnceLock::new();
            *HOST.get_or_init(||std::env::var_os("LOOKING_GLASS_FRAME_PROFILE").is_some()
                || (std::env::var_os("LOOKING_GLASS_SIM_MAPS_PROBE").is_some()
                    && std::env::var("LOOKING_GLASS_SIM_MAPS_COUNTERS").as_deref()!=Ok("0")))
                && crate::light_profile::full()
        }
    }
    pub(crate) fn configure() {
        let observed=observed();
        // One table choice per frame; the driver targets themselves have no guard.
        static LAST:std::sync::atomic::AtomicU8=std::sync::atomic::AtomicU8::new(2);
        if LAST.swap(u8::from(observed),std::sync::atomic::Ordering::Relaxed)!=u8::from(observed) {
            unsafe { DISPATCH=dispatch(observed); }
        }
    }
    #[cfg(target_os="vita")]
    pub(crate) fn loader_lookup(name:&[u8])->Option<*mut c_void> {
        let c=crate::render::vita_renderer::config();
        // Cache and explicit light policies are immutable after Vita startup.
        // Match the original observer table without a dispatch trampoline.
        if c.world_binding_cache || c.world_gl_state_cache
            || (c.light_gl_observers && c.profiler==Some(crate::light_profile::Level::Light)) {
            return lookup_observed(name);
        }
        // Explicit light/off policy cannot become full when profile_mode changes.
        // Resolve straight to vitaGL at startup in this common console case.
        if !c.world_binding_cache && !c.world_gl_state_cache
            && !(c.light_gl_observers && c.profiler==Some(crate::light_profile::Level::Light))
            && matches!(c.profiler,Some(crate::light_profile::Level::Light | crate::light_profile::Level::Off)) {
            return None;
        }
        lookup(name)
    }
    pub(crate) fn install() {
        configure();
        #[cfg(target_os="macos")]
        {
            static ONCE:std::sync::Once=std::sync::Once::new();
            ONCE.call_once(||macroquad::miniquad::gl::load_gl_funcs(|name|unsafe {
                let p=match name {
                    "glLinkProgram"=>link as *const () as _,
                    "glDeleteProgram"=>delete as *const () as _,
                    _=>lookup(name.as_bytes()).unwrap_or_else(||api::raw(name)),
                };
                (!p.is_null()).then(||std::mem::transmute(p))
            }));
        }
    }
}
#[cfg(target_os="vita")]
pub(crate) use hooks::loader_lookup as lookup;
#[cfg(any(target_os="vita",target_os="macos"))]
pub(crate) use hooks::{install,configure};
#[cfg(not(any(target_os="vita",target_os="macos")))]
pub(crate) fn install() {}
#[cfg(not(any(target_os="vita",target_os="macos")))]
pub(crate) fn configure() {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deferred_packets_drain_at_draw_and_separate_upscale_hud_and_running_cpu() {
        let start=Instant::now();
        let mut s=State::default();s.present=7;s.program=3;s.tail=true;s.start=Some(start);
        s.pending.buffer_updates=2;s.pending.buffer_bytes=172;s.pending.uniform_gl_calls=2;
        s.program=7; // Uploads precede apply_pipeline in the framework.
        let scope=crate::kernel_alloc::Scope::begin();
        let (phase,m)=s.complete(start+std::time::Duration::from_micros(9),true).unwrap();
        assert_eq!(scope.finish(),0);
        assert!(matches!(phase,RenderPhase::Present));
        assert_eq!((m.draws,m.buffer_updates,m.buffer_bytes,m.uniform_uploads),(1,2,172,1));
        assert_eq!((m.cpu_ns,m.deferred_cpu_ns),(9000,9000));
        assert_eq!(s.pending.buffer_bytes,0);assert!(s.start.is_none());
        s.program=3;s.start=Some(start);s.pending.buffer_bytes=24;
        let (phase,m)=s.complete(start+std::time::Duration::from_micros(7),false).unwrap();
        assert!(matches!(phase,RenderPhase::Hud));assert_eq!((m.cpu_ns,m.deferred_cpu_ns,m.buffer_bytes),(7000,0,24));
        s.tail=false;assert!(s.complete(start,true).is_none());
    }
}
