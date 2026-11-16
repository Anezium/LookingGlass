#!/usr/bin/env python3
"""Build the committed candidate using the shared cache, with a bounded timing lock."""
import hashlib, json, os, shutil, subprocess, time
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
OUT = Path('/Users/anezium/vita-j0/plan-2026-11-12/release-week/fight-pilot')
LOCK = Path('/Users/anezium/vita-j0/mac-timing.lock')
TARGET = Path('/Users/anezium/vita-j0/cargo-target-host')
head = subprocess.check_output(['git','rev-parse','HEAD'], cwd=ROOT, text=True).strip()
print('HEAD before build', head, flush=True)
assert shutil.disk_usage(OUT).free >= 41*2**30
started = time.monotonic()
while True:
    try:
        fd = os.open(LOCK, os.O_CREAT|os.O_EXCL|os.O_WRONLY, 0o600)
        break
    except FileExistsError:
        assert time.monotonic()-started < 1800, 'timing lock exceeded 30 minutes'
        print('waiting 60s:', LOCK.read_text(), flush=True)
        time.sleep(60)
os.write(fd,json.dumps(dict(pid=os.getpid(),task='fight-pilot host build',head=head)).encode())
os.close(fd)
try:
    assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()==head
    env=dict(os.environ,CARGO_TARGET_DIR=str(TARGET),RUSTFLAGS='--cfg observer_free_B',CARGO_BUILD_JOBS='4')
    cmd=['cargo','+nightly-2026-10-11','build','--offline','--locked','--release','--features','native_timing,frame_total_only,sim_maps_probe']
    with (OUT/'build-host.log').open('w') as log:
        result=subprocess.run(cmd,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
    assert result.returncode==0, 'build failed; see build-host.log'
    binary=OUT/('looking-glass-'+head[:7])
    shutil.copy2(TARGET/'release/looking-glass',binary)
    (OUT/'host-build.json').write_text(json.dumps(dict(head=head,binary=str(binary),sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),command=cmd),indent=2)+'\n')
    print(binary,flush=True)
finally:
    if json.loads(LOCK.read_text()).get('pid')==os.getpid(): LOCK.unlink()
