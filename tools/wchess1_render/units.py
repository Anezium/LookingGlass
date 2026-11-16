from pathlib import Path
import json,os,sys,subprocess,shutil
P=Path(__file__).resolve().parent;S=Path('/Users/anezium/vita-work/lg-wchess1');sys.path.insert(0,str(P));import host_oracle as locked
locked.acquire('wchess1 render unit suite')
try:
 subprocess.run(['df','-h','/Users'],check=True);assert shutil.disk_usage(P).free>41*2**30
 head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=S,text=True).strip();print(head,flush=True)
 assert not subprocess.check_output(['git','status','--porcelain'],cwd=S)
 (P/'unit-output-final').mkdir(exist_ok=True)
 env=dict(os.environ,ALICE_TIMING_TEST_OUTPUT=str(P/'unit-output-final'),PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin',CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='4',RUSTFLAGS='--cfg observer_free_B',RUSTUP_TOOLCHAIN='nightly-2026-10-11-aarch64-apple-darwin')
 os.utime(S/'src/main.rs')
 with (P/'units-final.log').open('w') as log:
  rc=subprocess.run(['cargo','test','--release','--offline','--locked','--features','native_timing,vita_renderer,frame_total_only,sim_maps_probe','--','--test-threads=1'],cwd=S,env=env,stdout=log,stderr=subprocess.STDOUT).returncode
 (P/'units-final-receipt.json').write_text(json.dumps(dict(head=head,result=rc)));assert rc==0
finally:locked.release()
