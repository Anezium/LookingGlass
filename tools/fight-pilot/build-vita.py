#!/usr/bin/env python3
"""Separate private-test SELF/VPK. Never installs or touches a kit/config/console."""
import hashlib,json,os,shutil,subprocess,time
from pathlib import Path
SOURCE=Path(__file__).resolve().parents[2]
ROOT=Path('/Users/anezium/vita-j0/plan-2026-11-12/release-week/fight-pilot')
LOCK=Path('/Users/anezium/vita-j0/mac-timing.lock')
TARGET=ROOT/'target-vita'
SEED=Path('/Users/anezium/vita-work/alice-r25-production-native-build/target-vita')
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=SOURCE,text=True).strip()
assert not subprocess.check_output(['git','status','--porcelain','--untracked-files=all'],cwd=SOURCE,text=True),'commit source first'
assert shutil.disk_usage(ROOT).free>=41*2**30
print('HEAD before build',head,flush=True)
started=time.monotonic()
while True:
 try:fd=os.open(LOCK,os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600);break
 except FileExistsError:
  assert time.monotonic()-started<1800,'timing lock exceeded 30 minutes'
  print('waiting 60s:',LOCK.read_text(),flush=True);time.sleep(60)
os.write(fd,json.dumps(dict(pid=os.getpid(),task='fight-pilot private Vita build',head=head)).encode());os.close(fd)
try:
 assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=SOURCE,text=True).strip()==head
 assert shutil.disk_usage(ROOT).free>=41*2**30
 if not TARGET.exists():subprocess.run(['/bin/cp','-cR',str(SEED),str(TARGET)],check=True)
 out=ROOT/'builds'/head[:7];(ROOT/'tmp').mkdir(exist_ok=True)
 env=dict(os.environ,VITASDK='/Users/anezium/vitasdk',CARGO_TARGET_DIR=str(TARGET),CARGO_BUILD_JOBS='4',CARGO_NET_OFFLINE='true',CARGO_TERM_COLOR='never',TMPDIR=str(ROOT/'tmp'),PATH='/Users/anezium/.cargo/bin:/Users/anezium/vitasdk/bin:/opt/homebrew/bin:/usr/bin:/bin')
 with (ROOT/'build-vita.log').open('w') as log:
  subprocess.run(['/opt/homebrew/bin/bash',str(SOURCE/'tools/vita_release_build.sh'),'--private-test','--out',str(out)],cwd=SOURCE,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
 assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=SOURCE,text=True).strip()==head
 hashes={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in out.iterdir() if p.suffix in ('.bin','.elf','.vpk','.velf')}
 (ROOT/'vita-build.json').write_text(json.dumps(dict(head=head,mode='private-test',artifacts=hashes,directory=str(out),console_contact=False),indent=2)+'\n')
 print(json.dumps(hashes,indent=2),flush=True)
finally:
 if json.loads(LOCK.read_text()).get('pid')==os.getpid():LOCK.unlink()
