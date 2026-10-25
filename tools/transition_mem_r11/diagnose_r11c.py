"""Temporary host histogram; restore all source files before running its binary."""
from pathlib import Path
import json, os, shutil, subprocess, sys
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve()
files=[R/'src/kernel_alloc.rs',R/'src/viewer.rs']
backup={p:p.read_bytes() for p in files}
env=dict(os.environ,PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+os.environ['PATH'],RUSTUP_TOOLCHAIN='stable',CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',RUSTFLAGS='--cfg observer_free_B',CARGO_INCREMENTAL='0',PYTHONDONTWRITEBYTECODE='1')
try:
 p=files[0];s=p.read_text().replace('    fn live_alloc(n: usize) {','''    static SIZES: [std::sync::atomic::AtomicIsize; 2097152] = [const { std::sync::atomic::AtomicIsize::new(0) }; 2097152];
    pub fn live_sizes() -> Vec<(usize,isize)> { SIZES.iter().enumerate().filter_map(|(n,v)| {let count=v.load(Relaxed); (count!=0).then_some((n,count))}).collect() }
    fn live_alloc(n: usize) {
        if n < SIZES.len() { SIZES[n].fetch_add(1, Relaxed); }
        if [540680,1081352,107528,83976].contains(&n) {
            thread_local! { static TRACING: Cell<bool> = const { Cell::new(false) }; }
            let _ = TRACING.try_with(|flag| { if !flag.replace(true) {
                eprintln!("R11C_ALLOCATION_TRACE size={n} {}", std::backtrace::Backtrace::force_capture());
                flag.set(false);
            }});
        }''',1).replace('    fn live_free(n: usize) {','    fn live_free(n: usize) { if n < SIZES.len() { SIZES[n].fetch_sub(1, Relaxed); }',1)
 s+='\n#[cfg(not(target_os="vita"))]\npub(crate) use imp::live_sizes;\n';p.write_text(s)
 p=files[1];s=p.read_text().replace('            use sha2::{Digest,Sha256};\n            let value = game_snapshot!();','''            if frame == 1 || (frame >= 264 && (frame-264)%272==0) {
                println!("TRANSITION_HEAP_SIZES {}", serde_json::json!({"frame":frame,"sizes":crate::kernel_alloc::live_sizes()}));
            }
            use sha2::{Digest,Sha256};
            let value = game_snapshot!();''',1);p.write_text(s)
 with (O/'logs/r11c-diagnostic-build.log').open('w') as f:
  subprocess.run(['cargo','build','--release','--locked','--features','native_timing,frame_total_only,sim_maps_probe'],cwd=R,env=env,stdout=f,stderr=subprocess.STDOUT,check=True)
 B=O/'proof/r11c-diagnostic-bin';shutil.copy2(Path(env['CARGO_TARGET_DIR'])/'release/looking-glass',B)
finally:
 for p,b in backup.items():p.write_bytes(b)
D=O/('diagnostic-trace-r11c' if '--trace' in sys.argv else 'diagnostic-r11c');(D/'host/seed-hedge3/private/saves').mkdir(parents=True,exist_ok=True);(D/'proof').mkdir(exist_ok=True)
shutil.copyfile(O/'host/seed-hedge3/private/saves/quick.json',D/'host/seed-hedge3/private/saves/quick.json')
source=R/'tools/transition_mem_r11/repeated_proof.py';s=source.read_text().replace('for on in [False,True]:','for on in [True]:').replace("for action in ['retry','native-load','chapter']:","for action in ['chapter']:").replace('range(20)','range(60)').replace('timeout=600','timeout=1800')
if '--trace' in sys.argv:s=s.replace('range(60)','range(18)')
sys.argv=[str(source),str(D),str(B)]
exec(compile(s,str(source),'exec'),{'__file__':str(source)})
