#!/usr/bin/env python3
"""Separate private-test SELF/VPK. Never installs or touches a kit/config/console."""
import hashlib,json,os,shutil,subprocess,time
from pathlib import Path
SOURCE=Path(__file__).resolve().parents[2]
ROOT=Path('/Users/anezium/vita-j0/plan-2026-11-12/release-week/fight-pilot')
TARGET=ROOT/'target-vita'
SEED=Path('/Users/anezium/vita-work/alice-r25-production-native-build/target-vita')
LIB=Path('/Users/anezium/vita-j0/plan-2026-11-12/rc14/lib-no-prelude')
assert hashlib.sha256((LIB/'libSceShaccCgExt.a').read_bytes()).hexdigest()=='0e340441f6909c1169b6f33bcd6e0223ffa91c9d7ce5155f3b27eb1bdd0b38a5'
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=SOURCE,text=True).strip()
assert not subprocess.check_output(['git','status','--porcelain','--untracked-files=all'],cwd=SOURCE,text=True),'commit source first'
assert shutil.disk_usage(ROOT).free>=41*2**30
print('HEAD before build',head,flush=True)
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=SOURCE,text=True).strip()==head
assert shutil.disk_usage(ROOT).free>=41*2**30
if not TARGET.exists():subprocess.run(['/bin/cp','-cR',str(SEED),str(TARGET)],check=True)
out=ROOT/'builds'/head[:7];(ROOT/'tmp').mkdir(exist_ok=True)
env=dict(os.environ,VITASDK='/Users/anezium/vitasdk',CARGO_TARGET_DIR=str(TARGET),CARGO_BUILD_JOBS='4',CARGO_NET_OFFLINE='true',CARGO_TERM_COLOR='never',TMPDIR=str(ROOT/'tmp'),PATH='/Users/anezium/.cargo/bin:/Users/anezium/vitasdk/bin:/opt/homebrew/bin:/usr/bin:/bin')
with (ROOT/'build-vita.log').open('w') as log:
 subprocess.run(['/opt/homebrew/bin/bash',str(SOURCE/'tools/vita_release_build.sh'),'--private-test','--lib-dir',str(LIB),'--out',str(out)],cwd=SOURCE,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=SOURCE,text=True).strip()==head
hashes={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in out.iterdir() if p.suffix in ('.bin','.elf','.vpk','.velf')}
(ROOT/'vita-build.json').write_text(json.dumps(dict(head=head,mode='private-test',artifacts=hashes,directory=str(out),console_contact=False),indent=2)+'\n')
print(json.dumps(hashes,indent=2),flush=True)
