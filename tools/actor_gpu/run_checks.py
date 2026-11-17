"""Optimized unit suite and feature-off compile; fresh output fixture directory."""
import json,os,subprocess,shutil,time
import host_oracle as h
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=h.SOURCE,text=True).strip()
assert not subprocess.check_output(['git','status','--porcelain'],cwd=h.SOURCE)
assert shutil.disk_usage(h.P).free>41*2**30
out=h.P/'unit-output'/('check-'+head[:7]+'-'+str(time.time_ns()));out.mkdir(parents=True)
env=dict(os.environ,ALICE_TIMING_TEST_OUTPUT=str(out),PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin',CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',CARGO_NET_OFFLINE='true',CARGO_BUILD_JOBS='3',RUSTFLAGS='--cfg observer_free_B',RUSTUP_TOOLCHAIN='nightly-2026-10-11-aarch64-apple-darwin',CARGO_PROFILE_RELEASE_LTO='false',CARGO_PROFILE_RELEASE_CODEGEN_UNITS='256')
os.utime(h.SOURCE/'src/main.rs')
with (h.P/'logs/unit.log').open('w') as log:
 result=subprocess.run(['cargo','test','--release','--offline','--locked','--features','native_timing,vita_renderer,frame_total_only,sim_maps_probe','--','--test-threads=1'],cwd=h.SOURCE,env=env,stdout=log,stderr=subprocess.STDOUT)
(h.P/'unit-receipt.json').write_text(json.dumps(dict(head=head,result=result.returncode,output=str(out)))+'\n')
if result.returncode:raise SystemExit(result.returncode)
with (h.P/'logs/feature-off.log').open('w') as log:
 result=subprocess.run(['cargo','check','--offline','--locked'],cwd=h.SOURCE,env=env,stdout=log,stderr=subprocess.STDOUT)
(h.P/'feature-off-receipt.json').write_text(json.dumps(dict(head=head,result=result.returncode))+'\n')
raise SystemExit(result.returncode)
