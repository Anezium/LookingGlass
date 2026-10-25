"""Bind the immutable proof executable to a clean committed engine source tree."""
from pathlib import Path
import hashlib,json,os,subprocess,sys,time
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve()
paths=['src','Cargo.toml','Cargo.lock','build.rs','.cargo','examples']
assert not subprocess.check_output(['git','status','--porcelain','--',*paths],cwd=R)
assert not subprocess.check_output(['git','status','--porcelain'],cwd=R), 'Commit tooling before final clean build'
rev=subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()
env=dict(os.environ,PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+os.environ['PATH'],RUSTUP_TOOLCHAIN='stable',CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',RUSTFLAGS='--cfg observer_free_B',CARGO_INCREMENTAL='0',CARGO_NET_OFFLINE='true')
cmd=['cargo','build','--release','--offline','--locked','--features','native_timing,frame_total_only,sim_maps_probe']
with (O/'logs/r11c-clean-host-build.log').open('w') as f:subprocess.run(cmd,cwd=R,env=env,stdout=f,stderr=subprocess.STDOUT,check=True)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
built=sha(Path(env['CARGO_TARGET_DIR'])/'release/looking-glass');proof=sha(O/'proof/recovery-bin')
assert built==proof,'Proof executable differs from clean source rebuild'
assert not subprocess.check_output(['git','diff',rev,'--',*paths],cwd=R)
(O/'proof/CLEAN-HOST-BUILD.json').write_text(json.dumps(dict(status='PASS',revision=rev,engine_paths_clean=True,working_tree_clean=True,argv=cmd,binary_sha256=built,proof_binary_byte_equal=True),indent=2)+'\n')
print('PASS clean committed engine build equals immutable proof executable')
