//! Vita-only loader hooks. No shared program handles or material uniform state.
#![allow(non_snake_case)]
use super::*;
use std::{collections::BTreeMap, ffi::{c_char, c_void, CStr}, time::Instant};
const VERTEX: u32 = 0x8b31;
const FRAGMENT: u32 = 0x8b30;
const LINK_STATUS: u32 = 0x8b82;
const COMPILE_STATUS: u32 = 0x8b81;
const BINARY_LENGTH: u32 = 0x8741;
#[cfg(target_os="vita")]
mod api {
    use super::*;
    extern "C" {
        pub fn glCreateShader(kind:u32)->u32;
        pub fn glShaderSource(shader:u32,count:i32,strings:*const *const c_char,lengths:*const i32);
        pub fn glCompileShader(shader:u32);
        pub fn glDeleteShader(shader:u32);
        pub fn glCreateProgram()->u32;
        pub fn glAttachShader(program:u32,shader:u32);
        pub fn glLinkProgram(program:u32);
        pub fn glDeleteProgram(program:u32);
        pub fn glGetProgramiv(program:u32,pname:u32,value:*mut i32);
        pub fn glGetShaderiv(shader:u32,pname:u32,value:*mut i32);
        pub fn glGetProgramBinary(program:u32,size:i32,length:*mut i32,format:*mut u32,data:*mut c_void);
        pub fn glShaderBinary(count:i32,shaders:*const u32,format:u32,data:*const c_void,len:i32);
        pub fn sceGxmProgramCheck(program:*const c_void)->i32;
        pub fn sceGxmProgramGetParameterCount(program:*const c_void)->u32;
        pub fn sceGxmProgramGetType(program:*const c_void)->u32;
        pub static glsl_sema_mode:u32;
        #[link_name="__real_vglGetProcAddress"]
        pub fn get_proc(name:*const c_char)->*mut c_void;
    }
}
#[cfg(test)]
#[path="mock.rs"]
mod api;
#[derive(Default)]
struct Shader { kind:u32, source:Vec<u8>, compile_ms:f64 }
struct Program {
    key:[u8;32], vertex:Vec<u8>, fragment:Vec<u8>, name:&'static str,
    hit:&'static str, compile_ms:f64, create_ms:f64,
}
#[derive(Default)]
struct State {
    shaders:BTreeMap<u32,Shader>, pending:Vec<u32>, programs:BTreeMap<u32,Program>,
    binaries:BTreeMap<[u8;32],Vec<u8>>, requests:u64, compiled:u64, avoided:u64,
}
thread_local! { static STATE: RefCell<State> = RefCell::default(); }
#[cfg(test)]
thread_local! { static TEST_ROOT: RefCell<Option<std::path::PathBuf>> = const { RefCell::new(None) }; }
fn identity()-> &'static str {
    #[cfg(target_os="vita")] { env!("LG_VITAGL_ID") }
    #[cfg(test)] { "mock-vitaGL" }
}
fn disk_enabled()->bool {
    #[cfg(test)] { TEST_ROOT.with(|r|r.borrow().is_some()) }
    #[cfg(all(target_os="vita",feature="vita_renderer"))] { crate::render::vita_renderer::config().shader_cache == "on" }
    #[cfg(all(target_os="vita",not(feature="vita_renderer")))] { true }
}
fn path(key:&[u8;32])->std::path::PathBuf {
    #[cfg(test)]
    if let Some(root)=TEST_ROOT.with(|r|r.borrow().clone()) {return root.join(format!("{}.bin",hex(key)));}
    Path::new("ux0:data/alice/private/cache/shaders").join(format!("{}.bin",hex(key)))
}
/// Copy to word-aligned storage before calling GXM. Validate matrix indices too.
unsafe fn valid_stage(stage:&[u8], kind:u32)->bool {
    let Some((gxp,indices)) = stage_gxp(stage) else { return false; };
    let mut aligned=vec![0u32;gxp.len().div_ceil(4)];
    std::ptr::copy_nonoverlapping(gxp.as_ptr(),aligned.as_mut_ptr().cast(),gxp.len());
    let p=aligned.as_ptr().cast();
    if api::sceGxmProgramCheck(p)!=0 || api::sceGxmProgramGetType(p)!=kind { return false; }
    let count=api::sceGxmProgramGetParameterCount(p);
    indices.chunks_exact(4).all(|i|u32::from_le_bytes(i.try_into().unwrap())<count)
}
/// Build a fresh candidate. Failure never mutates the caller's GLSL shaders.
unsafe fn load_pair(binary:&[u8])->Option<u32> {
    let pair=stages(binary)?;
    if !valid_stage(pair[0],0) || !valid_stage(pair[1],1) { return None; }
    let p=api::glCreateProgram(); if p==0 { return None; }
    let vs=api::glCreateShader(0x890e); let fs=api::glCreateShader(0x890f); // GL_CG_*_SHADER_EXT
    if vs==0 || fs==0 {
        if vs!=0 {api::glDeleteShader(vs);} if fs!=0 {api::glDeleteShader(fs);}
        api::glDeleteProgram(p); return None;
    }
    let mut ok=true;
    for (shader,stage) in [(vs,pair[0]),(fs,pair[1])] {
        api::glShaderBinary(1,&shader,0,stage.as_ptr().cast(),stage.len() as i32);
        let mut status=0; api::glGetShaderiv(shader,COMPILE_STATUS,&mut status); ok &= status!=0;
    }
    if ok { api::glAttachShader(p,vs); api::glAttachShader(p,fs); api::glLinkProgram(p); }
    let mut status=0; api::glGetProgramiv(p,LINK_STATUS,&mut status);
    api::glDeleteShader(vs); api::glDeleteShader(fs);
    if !ok || status==0 { api::glDeleteProgram(p); None } else { Some(p) }
}
unsafe fn dump(program:u32)->Option<Vec<u8>> {
    let mut size=0; api::glGetProgramiv(program,BINARY_LENGTH,&mut size);
    if size<=0 || size as usize>MAX_BINARY { return None; }
    let mut b=vec![0;size as usize]; let mut written=0; let mut format=0;
    api::glGetProgramBinary(program,size,&mut written,&mut format,b.as_mut_ptr().cast());
    if written!=size || stages(&b).is_none() { return None; }
    Some(b)
}
unsafe extern "C" fn create_shader(kind:u32)->u32 {
    let shader=api::glCreateShader(kind);
    if shader!=0 {STATE.with(|s|s.borrow_mut().shaders.insert(shader,Shader{kind,..Default::default()}));} shader
}
unsafe extern "C" fn source(shader:u32,count:i32,strings:*const *const c_char,lengths:*const i32) {
    api::glShaderSource(shader,count,strings,lengths);
    if count<0 || count>512 || strings.is_null() {return;}
    let mut bytes=Vec::new();
    for i in 0..count as usize {
        let p=*strings.add(i); if p.is_null() {return;}
        let n=if lengths.is_null() {-1} else {*lengths.add(i)};
        let part=if n<0 {CStr::from_ptr(p).to_bytes()} else {std::slice::from_raw_parts(p.cast(),n as usize)};
        bytes.extend_from_slice(part);
    }
    STATE.with(|s| {if let Some(shader)=s.borrow_mut().shaders.get_mut(&shader) {shader.source=bytes;}});
}
unsafe extern "C" fn compile(shader:u32) {
    let start=Instant::now(); api::glCompileShader(shader);
    STATE.with(|s| {let mut s=s.borrow_mut(); if let Some(v)=s.shaders.get_mut(&shader) {v.compile_ms=start.elapsed().as_secs_f64()*1000.;} s.pending.push(shader);});
}
unsafe extern "C" fn delete_shader(shader:u32) {
    STATE.with(|s| {let mut s=s.borrow_mut(); s.shaders.remove(&shader); s.pending.retain(|id|*id!=shader);});
    api::glDeleteShader(shader);
}
unsafe extern "C" fn create_program()->u32 {
    let start=Instant::now();
    let pair=STATE.with(|s| {
        let mut s=s.borrow_mut(); let ids=std::mem::take(&mut s.pending);
        if ids.len()!=2 || api::glsl_sema_mode!=2 {return None;}
        let v=s.shaders.get(&ids[0])?; let f=s.shaders.get(&ids[1])?;
        (v.kind==VERTEX && f.kind==FRAGMENT && !v.source.is_empty() && !f.source.is_empty())
            .then(||(v.source.clone(),f.source.clone(),v.compile_ms+f.compile_ms))
    });
    let Some((vertex,fragment,compile_ms))=pair else {return api::glCreateProgram();};
    let key=key(identity(),b"GLSL-pair-postponed-v1",&[&vertex,&fragment]);
    let mut hit="compile";
    let memory=STATE.with(|s|s.borrow().binaries.get(&key).cloned());
    let binary=if memory.is_some() {hit="memory";memory} else if disk_enabled() {hit="disk";read(&path(&key),&key)} else {None};
    let candidate=binary.as_ref().and_then(|b|load_pair(b));
    if binary.is_some() && candidate.is_none() {
        eprintln!("[lg/shader] rejected hash={} fallback=compile",hex(&key));
        STATE.with(|s|s.borrow_mut().binaries.remove(&key));
        if disk_enabled() {let _=fs::remove_file(path(&key));}
    }
    let program=if let Some(p)=candidate {
        STATE.with(|s|s.borrow_mut().binaries.insert(key,binary.unwrap())); p
    } else {hit="compile";api::glCreateProgram()};
    if program!=0 {STATE.with(|s|s.borrow_mut().programs.insert(program,Program{key,vertex,fragment,name:name(),hit,compile_ms,create_ms:start.elapsed().as_secs_f64()*1000.}));}
    program
}
unsafe extern "C" fn attach(program:u32,shader:u32) {
    let hit=STATE.with(|s|s.borrow().programs.get(&program).is_some_and(|p|p.hit!="compile"));
    if !hit {api::glAttachShader(program,shader);}
}
unsafe extern "C" fn link(program:u32) {
    let p=STATE.with(|s|s.borrow_mut().programs.remove(&program));
    let Some(p)=p else {api::glLinkProgram(program);return;};
    let start=Instant::now();
    if p.hit=="compile" {api::glLinkProgram(program);}
    let ms=start.elapsed().as_secs_f64()*1000.;
    let mut status=0; api::glGetProgramiv(program,LINK_STATUS,&mut status);
    if status!=0 && p.hit=="compile" {
        if let Some(b)=dump(program) {
            if disk_enabled() { if let Err(e)=write(&path(&p.key),&p.key,&b) {eprintln!("[lg/shader] cache_write_failed hash={} error={e}",hex(&p.key));} }
            STATE.with(|s|s.borrow_mut().binaries.insert(p.key,b));
        } else {eprintln!("[lg/shader] export_unavailable hash={}",hex(&p.key));}
    }
    #[cfg(all(target_os="vita",feature="vita_renderer"))]
    if status!=0 {crate::render_tail::register(program,p.name);}
    let (before,after,avoided)=STATE.with(|s| {let mut s=s.borrow_mut(); s.requests+=1; if p.hit=="compile" {s.compiled+=1;} else {s.avoided+=1;} (s.requests,s.compiled,s.avoided)});
    eprintln!("[lg/shader] name={} source_hash={} cache_key={} vertex_bytes={} fragment_bytes={} cache={} compile_ms={:.3} load_ms={:.3} linked={} before={} after={} avoided={}",p.name,hex(&key("GLSL",b"source",&[&p.vertex,&p.fragment])),hex(&p.key),p.vertex.len(),p.fragment.len(),p.hit,if p.hit=="compile" {p.compile_ms+ms} else {0.},p.create_ms,status,before,after,avoided);
}
unsafe extern "C" fn delete_program(program:u32) {
    #[cfg(all(target_os="vita",feature="vita_renderer"))]
    crate::render_tail::unregister(program);
    STATE.with(|s|s.borrow_mut().programs.remove(&program)); api::glDeleteProgram(program);
}
/// Miniquad resolves function pointers; wrapping glLinkProgram alone misses them.
#[cfg(any(target_os="vita",test))]
#[no_mangle]
pub unsafe extern "C" fn __wrap_vglGetProcAddress(n:*const c_char)->*mut c_void {
    match CStr::from_ptr(n).to_bytes() {
        b"glCreateShader"=>create_shader as *const () as _, b"glShaderSource"=>source as *const () as _,
        b"glCompileShader"=>compile as *const () as _, b"glDeleteShader"=>delete_shader as *const () as _,
        b"glCreateProgram"=>create_program as *const () as _, b"glAttachShader"=>attach as *const () as _,
        b"glLinkProgram"=>link as *const () as _, b"glDeleteProgram"=>delete_program as *const () as _,
        _=>{
            #[cfg(target_os="vita")]
            if let Some(p)=crate::gpu_wait::lookup(CStr::from_ptr(n).to_bytes()) {return p;}
            #[cfg(all(target_os="vita",feature="vita_renderer"))]
            if let Some(p)=crate::render_tail::lookup(CStr::from_ptr(n).to_bytes()) {return p;}
            api::get_proc(n)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe fn program(v:&str,f:&str)->u32 {
        for (kind,text) in [(VERTEX,v),(FRAGMENT,f)] {
            let id=create_shader(kind); let text=std::ffi::CString::new(text).unwrap(); let ptr=text.as_ptr(); source(id,1,&ptr,std::ptr::null()); compile(id);
        }
        let originals=STATE.with(|s|s.borrow().pending.clone());
        let p=create_program(); for id in &originals {attach(p,*id);} link(p);
        for id in originals {delete_shader(id);} p
    }
    #[test] fn duplicate_pairs_compile_once_and_keep_separate_programs() { unsafe {
        STATE.with(|s|*s.borrow_mut()=State::default()); api::reset();
        let mut programs=Vec::new();
        for _ in 0..10 {programs.push(program("vertex","fragment"));}
        assert_ne!(programs[0],programs[1]); assert_eq!(api::compiled(),1);
        assert_eq!(STATE.with(|s|(s.borrow().requests,s.borrow().avoided)),(10,9));
        programs.push(program("vertex","#define VARIANT\nfragment"));
        programs.push(program("sky-vertex","sky-fragment"));
        assert_eq!(api::compiled(),3);
        assert_eq!(STATE.with(|s|(s.borrow().requests,s.borrow().avoided)),(12,9));
        for p in programs {delete_program(p);} assert_eq!(api::live(),(0,0));
    }}
    #[test] fn binary_load_failure_falls_back_with_original_sources_and_rewrites() { unsafe {
        STATE.with(|s|*s.borrow_mut()=State::default()); api::reset();
        let a=program("vertex","fragment"); api::fail_next_load();
        let b=program("vertex","fragment"); assert_eq!(api::compiled(),2);
        let c=program("vertex","fragment"); assert_eq!(api::compiled(),2);
        for p in [a,b,c] {delete_program(p);} assert_eq!(api::live(),(0,0));
    }}
    #[test] fn second_process_uses_disk_and_corruption_recompiles() {unsafe {
        STATE.with(|s|*s.borrow_mut()=State::default()); api::reset();
        let root=std::env::temp_dir().join(format!("lg-j7-shader-disk-{}",std::process::id()));
        TEST_ROOT.with(|r|*r.borrow_mut()=Some(root.clone()));
        let a=program("vertex","fragment"); delete_program(a);
        let k=key(identity(),b"GLSL-pair-postponed-v1",&[b"vertex",b"fragment"]);
        assert!(read(&path(&k),&k).is_some());
        STATE.with(|s|*s.borrow_mut()=State::default());
        let b=program("vertex","fragment"); assert_eq!(api::compiled(),1); delete_program(b);
        fs::write(path(&k),b"truncated").unwrap(); STATE.with(|s|*s.borrow_mut()=State::default());
        let c=program("vertex","fragment"); assert_eq!(api::compiled(),2); delete_program(c);
        assert!(read(&path(&k),&k).is_some());
        TEST_ROOT.with(|r|*r.borrow_mut()=None); fs::remove_dir_all(root).unwrap(); assert_eq!(api::live(),(0,0));
    }}
    #[test] fn loader_resolves_hooks_and_preserves_other_functions() {unsafe {
        for n in ["glCreateShader","glShaderSource","glCompileShader","glDeleteShader","glCreateProgram","glAttachShader","glLinkProgram","glDeleteProgram"] {
            let n=std::ffi::CString::new(n).unwrap(); assert_ne!(__wrap_vglGetProcAddress(n.as_ptr()),std::ptr::null_mut());
        }
        let n=std::ffi::CString::new("glUniform4fv").unwrap(); assert_eq!(__wrap_vglGetProcAddress(n.as_ptr()),api::get_proc(n.as_ptr()));
    }}
    #[test] fn malformed_binary_is_rejected_before_deserializer() {unsafe {
        STATE.with(|s|*s.borrow_mut()=State::default()); api::reset();
        let a=program("vertex","fragment"); STATE.with(|s| {for b in s.borrow_mut().binaries.values_mut() {b[132..136].copy_from_slice(&u32::MAX.to_le_bytes());}});
        let b=program("vertex","fragment"); assert_eq!(api::compiled(),2); assert_eq!(api::binary_calls(),0);
        delete_program(a);delete_program(b); assert_eq!(api::live(),(0,0));
    }}
}
