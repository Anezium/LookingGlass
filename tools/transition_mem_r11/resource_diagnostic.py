"""Isolated diagnostic copy: backend populations and surviving small allocations."""
from pathlib import Path
import io,json,os,shutil,subprocess,sys,tarfile,tempfile
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();D=O/'resource-diagnostic';D.mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix='r11c-resource-') as temp:
 T=Path(temp);A=T/'engine';A.mkdir();M=T/'miniquad';M.mkdir()
 for repo,rev,dest in [(R,'HEAD',A),(Path('/Users/anezium/vita-work/miniquad-texture-r11c'),'HEAD',M)]:
  with tarfile.open(fileobj=io.BytesIO(subprocess.check_output(['git','archive',rev],cwd=repo))) as tar:tar.extractall(dest,filter='data')
 p=A/'Cargo.toml';s=p.read_text();import re
 s=re.sub(r'miniquad = \{ git = "https://github.com/Anezium/miniquad", rev = "[^"]+" \}',f'miniquad = {{ path = "{M}" }}',s);p.write_text(s)
 p=M/'src/graphics/gl.rs';s=p.read_text().replace('    fn commit_frame(&mut self) {','''    fn commit_frame(&mut self) {
        static FRAME: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        if FRAME.fetch_add(1,std::sync::atomic::Ordering::Relaxed)%136==0 {
            eprintln!("R11C_BACKEND_COUNTS buffers={}/{} pipelines={}/{} textures={}/{} shaders={}/{} passes={}/{}",
                self.buffers.resources.len(),self.buffers.resources.capacity(),self.pipelines.resources.len(),self.pipelines.resources.capacity(),
                self.textures.0.resources.len(),self.textures.0.resources.capacity(),self.shaders.resources.len(),self.shaders.resources.capacity(),self.passes.resources.len(),self.passes.resources.capacity());
        }''',1);p.write_text(s)
 p=A/'src/kernel_alloc.rs';s=p.read_text().replace('    struct Allocator;','''
    #[derive(Clone,Copy)]
    struct Small {ptr:usize,n:usize,frames:[usize;16]}
    static SMALL: std::sync::Mutex<[Small;65536]> = std::sync::Mutex::new([Small {ptr:0,n:0,frames:[0;16]};65536]);
    extern "C" {fn backtrace(buffer:*mut *mut core::ffi::c_void,count:i32)->i32; fn _dyld_get_image_header(index:u32)->*const core::ffi::c_void;}
    fn small_alloc(p:*mut u8,n:usize) {
        if !p.is_null() && (n==16 || n==32) {
            let mut frames=[0usize;16];unsafe {backtrace(frames.as_mut_ptr().cast(),16);}
            let mut slots=SMALL.lock().unwrap();let mut i=(p as usize>>4)%slots.len();
            for _ in 0..slots.len() {if slots[i].ptr<=1 {slots[i]=Small{ptr:p as usize,n,frames};return;}i=(i+1)%slots.len();}
            panic!("small-allocation diagnostic table full");
        }
    }
    fn small_free(p:*mut u8,n:usize) {
        if n==16 || n==32 {
            let mut slots=SMALL.lock().unwrap();let mut i=(p as usize>>4)%slots.len();
            for _ in 0..slots.len() {if slots[i].ptr==0{return;}if slots[i].ptr==p as usize{slots[i].ptr=1;return;}i=(i+1)%slots.len();}
        }
    }
    pub fn live_small()->(usize,Vec<(usize,usize,[usize;16])>) {
        let values=SMALL.lock().unwrap().iter().filter(|s|s.ptr>1).map(|s|(s.ptr,s.n,s.frames)).collect();
        (unsafe{_dyld_get_image_header(0)} as usize,values)
    }
    struct Allocator;''',1)
 s=s.replace('heap_alloc(l.size());live_alloc(l.size());','heap_alloc(l.size());live_alloc(l.size());small_alloc(p,l.size());')
 s=s.replace('heap_alloc(n);live_alloc(n);heap_free(l.size());live_free(l.size());','heap_alloc(n);live_alloc(n);small_free(p,l.size());small_alloc(out,n);heap_free(l.size());live_free(l.size());')
 s=s.replace('heap_free(l.size());live_free(l.size());System.dealloc','small_free(p,l.size());heap_free(l.size());live_free(l.size());System.dealloc')
 s+='\n#[cfg(not(target_os="vita"))]\npub(crate) use imp::live_small;\n';p.write_text(s)
 p=A/'src/viewer.rs';s=p.read_text().replace('            use sha2::{Digest,Sha256};\n            let value = game_snapshot!();','''            if frame == 1 || (frame>=264 && (frame-264)%272==0) {
                println!("R11C_SMALL {}",serde_json::json!({"frame":frame,"allocations":crate::kernel_alloc::live_small()}));
            }
            use sha2::{Digest,Sha256};
            let value = game_snapshot!();''',1);p.write_text(s)
 env=dict(os.environ,PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+os.environ['PATH'],RUSTUP_TOOLCHAIN='stable',CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',RUSTFLAGS='--cfg observer_free_B',CARGO_INCREMENTAL='0')
 with (D/'build.log').open('w') as f:subprocess.run(['cargo','build','--release','--offline','--features','native_timing,frame_total_only,sim_maps_probe'],cwd=A,env=env,stdout=f,stderr=subprocess.STDOUT,check=True)
 B=D/'diagnostic-bin';shutil.copy2(Path(env['CARGO_TARGET_DIR'])/'release/looking-glass',B)
# Source tree and local dependency override are gone before this runs.
(D/'host/seed-hedge3/private/saves').mkdir(parents=True,exist_ok=True);(D/'proof').mkdir(exist_ok=True)
shutil.copyfile(O/'host/seed-hedge3/private/saves/quick.json',D/'host/seed-hedge3/private/saves/quick.json')
source=R/'tools/transition_mem_r11/repeated_proof.py';s=source.read_text().replace('for on in [False,True]:','for on in [True]:').replace("for action in ['retry','native-load','chapter']:","for action in ['chapter']:")
sys.argv=[str(source),str(D),str(B)];exec(compile(s,str(source),'exec'),{'__file__':str(source)})
