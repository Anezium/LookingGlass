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
    outside_start:Option<Instant>,
    outside_ns:u64,
}
impl State {
    fn complete(&mut self, now:Instant, outside_cpu:bool, repaired:bool) -> Option<(RenderPhase,RenderMetrics)> {
        if !self.tail {return None;}
        let elapsed=self.start.take().map_or(0,|v|now.duration_since(v).as_nanos() as u64);
        let mut m=std::mem::take(&mut self.pending);
        m.draws=1; m.pipelines=1; m.bindings=1;
        m.uniform_uploads=u64::from(m.uniform_gl_calls>0);
        m.cpu_ns=elapsed;
        m.deferred_cpu_ns=if repaired {
            let tail=self.outside_start.take().map_or(0,|t|now.duration_since(t).as_nanos() as u64);
            std::mem::take(&mut self.outside_ns)+tail
        } else if outside_cpu {elapsed} else {0};
        Some((if self.present!=0 && self.program==self.present {RenderPhase::Present} else {RenderPhase::Hud},m))
    }
}
pub(crate) fn cpu_boundary(running:bool) {
    if !crate::coverage::active() {return;}
    let now=Instant::now();
    STATE.with(|s|s.borrow_mut().boundary(now,running));
}
impl State {
    fn boundary(&mut self,now:Instant,running:bool) {
        if let Some(t)=self.outside_start.take(){self.outside_ns+=now.duration_since(t).as_nanos() as u64;}
        if self.start.is_some() && !running {self.outside_start=Some(now);}
    }
}
thread_local! { static STATE: RefCell<State> = RefCell::default(); }
pub(crate) struct Owner(Option<RenderPhase>);
pub(crate) fn owner(phase: RenderPhase) -> Owner {
    Owner(STATE.with(|s| s.borrow_mut().owner.replace(phase)))
}
impl Drop for Owner { fn drop(&mut self) { STATE.with(|s| s.borrow_mut().owner = self.0); } }
pub(crate) fn begin() {
    STATE.with(|s| { let mut s=s.borrow_mut(); s.tail=true; s.pending=Default::default(); s.start=None;s.outside_start=None;s.outside_ns=0; });
}
pub(crate) fn end() {
    STATE.with(|s| { let mut s=s.borrow_mut(); s.tail=false; s.pending=Default::default(); s.start=None;s.outside_start=None;s.outside_ns=0; });
}
pub(crate) fn register(program:u32, name:&str) {
    if name.ends_with("render/vita_renderer/scale.rs") { STATE.with(|s| s.borrow_mut().present=program); }
}
pub(crate) fn unregister(program:u32) {
    STATE.with(|s| { let mut s=s.borrow_mut(); if s.present==program {s.present=0;} });
}
fn event(update: impl FnOnce(&mut RenderMetrics)) {
    if !crate::light_profile::render_active() { return; }
    STATE.with(|s| { let mut s=s.borrow_mut(); if s.tail {
        if s.start.is_none() {
            let now=Instant::now();s.start=Some(now);
            if crate::coverage::active() && !crate::light_profile::cpu_running(){s.outside_start=Some(now);}
        }
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
        s.complete(Instant::now(),!crate::light_profile::cpu_running(),crate::coverage::active())
    });
    if let Some((phase,m))=packet {crate::light_profile::render_add(phase,m);crate::light_profile::deferred_cpu(if crate::coverage::active(){m.deferred_cpu_ns}else{m.cpu_ns});}
}

#[cfg(any(target_os="vita", target_os="macos"))]
mod hooks {
    use super::*;
    use std::ffi::{c_char,c_void};
    macro_rules! api {
        ($($name:ident($($arg:ident:$ty:ty),*));* $(;)?) => {
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
                $(#[allow(non_snake_case)] pub unsafe fn $name($($arg:$ty),*) {
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
        glLinkProgram(p:u32); glDeleteProgram(p:u32)
    }
    macro_rules! wrap {
        ($($fn:ident => $real:ident($($arg:ident:$ty:ty),*) {$($before:tt)*});* $(;)?) => {$ (
            unsafe extern "C" fn $fn($($arg:$ty),*) { $($before)* api::$real($($arg),*); }
        )*};
    }
    wrap! {
        use_program => glUseProgram(p:u32) {program(p);}; bind_texture => glBindTexture(t:u32,p:u32) {texture();};
        depth_mask => glDepthMask(v:u8) {raster();}; enable => glEnable(v:u32) {raster();}; disable => glDisable(v:u32) {raster();};
        depth_func => glDepthFunc(v:u32) {raster();}; cull_face => glCullFace(v:u32) {raster();}; front_face => glFrontFace(v:u32) {raster();};
        blend_func => glBlendFuncSeparate(a:u32,b:u32,c:u32,d:u32) {raster();}; blend_equation => glBlendEquationSeparate(a:u32,b:u32) {raster();};
        viewport => glViewport(x:i32,y:i32,w:i32,h:i32) {raster();}; scissor => glScissor(x:i32,y:i32,w:i32,h:i32) {raster();};
        uniform_i => glUniform1i(l:i32,v:i32) {event(|m|m.texture_bind_slots+=1);};
        uniform1 => glUniform1fv(l:i32,n:i32,p:*const f32) {uniform(n,1);}; uniform2 => glUniform2fv(l:i32,n:i32,p:*const f32) {uniform(n,2);};
        uniform3 => glUniform3fv(l:i32,n:i32,p:*const f32) {uniform(n,3);}; uniform4 => glUniform4fv(l:i32,n:i32,p:*const f32) {uniform(n,4);};
        uniform_matrix => glUniformMatrix4fv(l:i32,n:i32,t:u8,p:*const f32) {uniform(n,16);};
        buffer_data => glBufferData(t:u32,n:isize,p:*const c_void,u:u32) {buffer(n,p);};
        buffer_sub => glBufferSubData(t:u32,o:isize,n:isize,p:*const c_void) {buffer(n,p);}
    }
    unsafe extern "C" fn elements(t:u32,n:i32,k:u32,p:*const c_void,i:i32) {api::glDrawElementsInstanced(t,n,k,p,i);draw();}
    #[cfg(target_os="macos")]
    unsafe extern "C" fn link(p:u32) {api::glLinkProgram(p);register(p,crate::shader_cache::name());}
    #[cfg(target_os="macos")]
    unsafe extern "C" fn delete(p:u32) {unregister(p);api::glDeleteProgram(p);}
    pub(crate) fn lookup(name:&[u8])->Option<*mut c_void> {
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
            _=>return None,
        })
    }
    #[cfg(target_os="macos")]
    pub(crate) fn install() {
        static ONCE:std::sync::Once=std::sync::Once::new();
        ONCE.call_once(||macroquad::miniquad::gl::load_gl_funcs(|name|unsafe {
            let p=lookup(name.as_bytes()).unwrap_or_else(||api::raw(name));
            (!p.is_null()).then(||std::mem::transmute(p))
        }));
    }
}
#[cfg(target_os="vita")]
pub(crate) use hooks::lookup;
#[cfg(target_os="macos")]
pub(crate) use hooks::install;
#[cfg(not(target_os="macos"))]
pub(crate) fn install() {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deferred_span_crossing_cpu_end_adds_only_nonoverlapping_tail() {
        let start=Instant::now();
        let mut s=State::default();s.tail=true;s.start=Some(start);
        s.boundary(start+std::time::Duration::from_micros(40),false);
        let (_,m)=s.complete(start+std::time::Duration::from_micros(47),true,true).unwrap();
        assert_eq!(m.cpu_ns,47_000);assert_eq!(m.deferred_cpu_ns,7_000);
        s.start=Some(start);s.boundary(start,false);
        s.boundary(start+std::time::Duration::from_micros(3),true);
        let (_,m)=s.complete(start+std::time::Duration::from_micros(9),false,true).unwrap();
        assert_eq!(m.cpu_ns,9_000);assert_eq!(m.deferred_cpu_ns,3_000);
    }
    #[test]
    fn deferred_packets_drain_at_draw_and_separate_upscale_hud_and_running_cpu() {
        let start=Instant::now();
        let mut s=State::default();s.present=7;s.program=3;s.tail=true;s.start=Some(start);
        s.pending.buffer_updates=2;s.pending.buffer_bytes=172;s.pending.uniform_gl_calls=2;
        s.program=7; // Uploads precede apply_pipeline in the framework.
        let scope=crate::kernel_alloc::Scope::begin();
        let (phase,m)=s.complete(start+std::time::Duration::from_micros(9),true,false).unwrap();
        assert_eq!(scope.finish(),0);
        assert!(matches!(phase,RenderPhase::Present));
        assert_eq!((m.draws,m.buffer_updates,m.buffer_bytes,m.uniform_uploads),(1,2,172,1));
        assert_eq!((m.cpu_ns,m.deferred_cpu_ns),(9000,9000));
        assert_eq!(s.pending.buffer_bytes,0);assert!(s.start.is_none());
        s.program=3;s.start=Some(start);s.pending.buffer_bytes=24;
        let (phase,m)=s.complete(start+std::time::Duration::from_micros(7),false,false).unwrap();
        assert!(matches!(phase,RenderPhase::Hud));assert_eq!((m.cpu_ns,m.deferred_cpu_ns,m.buffer_bytes),(7000,0,24));
        s.tail=false;assert!(s.complete(start,true,false).is_none());
    }
}
