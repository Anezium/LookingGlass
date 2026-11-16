#!/usr/bin/env python3
"""Build without the timing lock, in a private APFS-cloned cache removed at handoff."""
import hashlib,json,os,shutil,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
OUT=Path('/Users/anezium/vita-j0/plan-2026-11-12/release-week/fight-pilot')
TARGET=ROOT/'target-pilot'
SEED=Path('/Users/anezium/vita-j0/cargo-target-host')
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
print('HEAD before build',head,flush=True)
assert shutil.disk_usage(ROOT).free>=41*2**30
if not TARGET.exists():subprocess.run(['/bin/cp','-cR',str(SEED),str(TARGET)],check=True)
env=dict(os.environ,CARGO_TARGET_DIR=str(TARGET),RUSTFLAGS='--cfg observer_free_B',CARGO_BUILD_JOBS='4')
cmd=['/Users/anezium/.cargo/bin/cargo','+nightly-2026-10-11','build','--offline','--locked','--release','--features','native_timing,frame_total_only,sim_maps_probe']
with (OUT/('build-host-'+head[:7]+'.log')).open('w') as log:
    result=subprocess.run(cmd,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
assert result.returncode==0,'build failed; see build-host-'+head[:7]+'.log'
assert shutil.disk_usage(ROOT).free>=41*2**30
binary=OUT/('looking-glass-'+head[:7])
shutil.copy2(TARGET/'release/looking-glass',binary)
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()==head
(OUT/'host-build.json').write_text(json.dumps(dict(head=head,binary=str(binary),sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),command=cmd,target=str(TARGET)),indent=2)+'\n')
print(binary,flush=True)
