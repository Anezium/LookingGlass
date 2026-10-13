//! Optional persistence of vitaGL's exact swizzled DXT5 allocation, not a new compressor.
#![cfg(any(target_os="vita",test))]
#![allow(non_snake_case)]
use crate::shader_cache;
use std::{ffi::c_void, path::{Path,PathBuf}, time::Instant};
const TEXTURE_2D:u32=0x0de1;
const RGBA:u32=0x1908;
const UNSIGNED_BYTE:u32=0x1401;
const DXT5:i32=0x83f3;
#[cfg(target_os="vita")]
mod api {
    use super::*;
    extern "C" {
        #[link_name="__real_glTexImage2D"]
        pub fn upload(target:u32,level:i32,internal:i32,width:i32,height:i32,border:i32,format:u32,kind:u32,data:*const c_void);
        pub fn vglGetTexDataPointer(target:u32)->*mut c_void;
        pub static fast_texture_compression:u8;
    }
}
#[cfg(test)]
mod api {
    use super::*;
    use std::cell::RefCell;
    #[derive(Default)]
    struct Mock {pixels:Vec<u8>,compressions:usize,fail:bool}
    thread_local! {static MOCK:RefCell<Mock>=RefCell::default();}
    #[allow(non_upper_case_globals)]
    pub static fast_texture_compression:u8=0;
    pub fn reset(){MOCK.with(|m|*m.borrow_mut()=Mock::default());}
    pub fn fail_next_allocation(){MOCK.with(|m|m.borrow_mut().fail=true);}
    pub fn pixels()->Vec<u8>{MOCK.with(|m|m.borrow().pixels.clone())}
    pub fn compressions()->usize{MOCK.with(|m|m.borrow().compressions)}
    pub unsafe fn upload(_target:u32,_level:i32,internal:i32,width:i32,height:i32,_border:i32,_format:u32,_kind:u32,data:*const c_void) {
        MOCK.with(|m|{let mut m=m.borrow_mut();if m.fail{m.fail=false;m.pixels.clear();return;}
            let len=width as usize*height as usize;
            if data.is_null(){m.pixels=vec![0;len];return;}
            if internal==DXT5 {m.compressions+=1;}
            // Deterministic stand-in: tests compare the original driver's bytes.
            let input=std::slice::from_raw_parts(data.cast::<u8>(),len*4);
            m.pixels=input.chunks_exact(4).map(|p|p[0]^p[1]^p[2]^p[3]).collect();
        });
    }
    pub unsafe fn vglGetTexDataPointer(_target:u32)->*mut c_void {MOCK.with(|m|{let mut m=m.borrow_mut();if m.pixels.is_empty(){std::ptr::null_mut()}else{m.pixels.as_mut_ptr().cast()}})}
}
#[cfg(test)]
thread_local! {static TEST_ROOT:std::cell::RefCell<Option<PathBuf>>=const{std::cell::RefCell::new(None)};}
fn enabled()->bool {
    #[cfg(test)] {TEST_ROOT.with(|r|r.borrow().is_some())}
    #[cfg(all(target_os="vita",feature="vita_renderer"))] {crate::render::vita_renderer::config().texture_cache=="on"}
    #[cfg(all(target_os="vita",not(feature="vita_renderer")))] {false}
}
fn path(key:&[u8;32])->PathBuf {
    #[cfg(test)]
    if let Some(root)=TEST_ROOT.with(|r|r.borrow().clone()){return root.join(format!("{}.bin",shader_cache::hex(key)));}
    Path::new("ux0:data/alice/private/cache/textures").join(format!("{}.bin",shader_cache::hex(key)))
}
fn identity()->&'static str {
    #[cfg(test)] {"mock-vitaGL"}
    #[cfg(target_os="vita")] {env!("LG_VITAGL_ID")}
}
fn eligible(target:u32,level:i32,internal:i32,width:i32,height:i32,border:i32,format:u32,kind:u32,data:*const c_void)->bool {
    target==TEXTURE_2D && level==0 && internal==DXT5 && border==0 && format==RGBA && kind==UNSIGNED_BYTE && !data.is_null()
        && (4..=512).contains(&width) && (4..=512).contains(&height) && (width as u32).is_power_of_two() && (height as u32).is_power_of_two()
}
/// Runs after Miniquad's immutable-upload shim chooses DXT5. Dynamic images,
/// render targets, fonts, movies and NPOT inputs keep their existing upload path.
#[no_mangle]
pub unsafe extern "C" fn __wrap_glTexImage2D(target:u32,level:i32,internal:i32,width:i32,height:i32,border:i32,format:u32,kind:u32,data:*const c_void) {
    if !eligible(target,level,internal,width,height,border,format,kind,data) || !enabled() {
        api::upload(target,level,internal,width,height,border,format,kind,data);return;
    }
    let start=Instant::now(); let size=width as usize*height as usize;
    let pixels=std::slice::from_raw_parts(data.cast::<u8>(),size*4);
    let key=shader_cache::key(identity(),b"vitaGL-swizzled-DXT5-level0-v1",&[&width.to_le_bytes(),&height.to_le_bytes(),&[api::fast_texture_compression],pixels]);
    let file=path(&key);
    if let Some(binary)=shader_cache::read(&file,&key).filter(|b|b.len()==size) {
        // Allocate with the same format and dimensions, but no compression work.
        // The public pointer addresses the initialized driver-owned GPU allocation.
        api::upload(target,level,internal,width,height,border,format,kind,std::ptr::null());
        let destination=api::vglGetTexDataPointer(target);
        if !destination.is_null() {
            std::ptr::copy_nonoverlapping(binary.as_ptr(),destination.cast(),size);
            eprintln!("[lg/texture-cache] key={} cache=disk bytes={size} elapsed_ms={:.3}",shader_cache::hex(&key),start.elapsed().as_secs_f64()*1000.);return;
        }
        eprintln!("[lg/texture-cache] key={} allocation_failed fallback=compress",shader_cache::hex(&key));
    }
    api::upload(target,level,internal,width,height,border,format,kind,data);
    let compressed=api::vglGetTexDataPointer(target);
    if !compressed.is_null() {
        let bytes=std::slice::from_raw_parts(compressed.cast::<u8>(),size);
        if let Err(e)=shader_cache::write(&file,&key,bytes){eprintln!("[lg/texture-cache] write_failed key={} error={e}",shader_cache::hex(&key));}
    }
    eprintln!("[lg/texture-cache] key={} cache=compress bytes={size} elapsed_ms={:.3}",shader_cache::hex(&key),start.elapsed().as_secs_f64()*1000.);
}
#[cfg(test)]
mod tests {
    use super::*;
    unsafe fn upload(pixels:&[u8],internal:i32) {__wrap_glTexImage2D(TEXTURE_2D,0,internal,8,4,0,RGBA,UNSIGNED_BYTE,pixels.as_ptr().cast());}
    #[test] fn second_launch_reuses_exact_compressed_bytes_and_invalid_entries_rewrite(){unsafe{
        let root=std::env::temp_dir().join(format!("lg-j7-textures-{}",std::process::id()));TEST_ROOT.with(|r|*r.borrow_mut()=Some(root.clone()));api::reset();
        let pixels=(0..128).map(|v|v as u8).collect::<Vec<_>>();upload(&pixels,DXT5);let expected=api::pixels();assert_eq!(api::compressions(),1);
        api::reset();upload(&pixels,DXT5);assert_eq!(api::pixels(),expected);assert_eq!(api::compressions(),0);
        api::fail_next_allocation();upload(&pixels,DXT5);assert_eq!(api::pixels(),expected);assert_eq!(api::compressions(),1);
        let file=std::fs::read_dir(&root).unwrap().next().unwrap().unwrap().path();std::fs::write(&file,b"truncated").unwrap();upload(&pixels,DXT5);assert_eq!(api::compressions(),2);assert_eq!(api::pixels(),expected);
        let mut different=pixels;different[0]^=1;upload(&different,DXT5);assert_eq!(api::compressions(),3);assert_ne!(api::pixels(),expected);
        TEST_ROOT.with(|r|*r.borrow_mut()=None);std::fs::remove_dir_all(root).unwrap();
    }}
    #[test] fn dynamic_and_unsupported_uploads_bypass_cache(){
        let p=[0u8;128];let ptr=p.as_ptr().cast();
        assert!(eligible(TEXTURE_2D,0,DXT5,8,4,0,RGBA,UNSIGNED_BYTE,ptr));
        for (level,internal,w,h,border,data) in [(0,RGBA as i32,8,4,0,ptr),(1,DXT5,8,4,0,ptr),(0,DXT5,7,4,0,ptr),(0,DXT5,8,2,0,ptr),(0,DXT5,1024,4,0,ptr),(0,DXT5,8,4,1,ptr),(0,DXT5,8,4,0,std::ptr::null())] {
            assert!(!eligible(TEXTURE_2D,level,internal,w,h,border,RGBA,UNSIGNED_BYTE,data));
        }
        unsafe {api::reset();upload(&p,RGBA as i32);assert_eq!(api::compressions(),0);}
    }
}
