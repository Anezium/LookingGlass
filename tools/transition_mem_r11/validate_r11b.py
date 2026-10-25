"""Run all local r11/r11b host proofs against one immutable final executable."""
from pathlib import Path
import sys,os,json,shutil,subprocess,time,threading,concurrent.futures,hashlib
O=Path(sys.argv[1]).resolve();R=Path(__file__).resolve().parents[2];T=Path(__file__).resolve().parent
env=dict(os.environ,PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+os.environ['PATH'],RUSTUP_TOOLCHAIN='stable',CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',RUSTFLAGS='--cfg observer_free_B',CARGO_INCREMENTAL='0',CARGO_NET_OFFLINE='true',PYTHONDONTWRITEBYTECODE='1')
receipts=[];receipt_lock=threading.Lock()
prior=json.loads((O/'proof/R11D-VALIDATION.json').read_text())['runs'] if '--resume' in sys.argv else []
passed={r['tag']:r for r in prior if r['exit_code']==0}
if 'host-build' in passed:
 assert hashlib.sha256((O/'proof/recovery-bin').read_bytes()).digest()==hashlib.sha256((Path(env['CARGO_TARGET_DIR'])/'release/looking-glass').read_bytes()).digest()
def run(tag,cmd,**kw):
 if tag in passed:
  receipts.append(passed[tag]);print('RETAIN PASS',tag,flush=True);return
 start=time.monotonic()
 with (O/'logs'/f'r11b-{tag}.log').open('w') as f:p=subprocess.run(cmd,cwd=R,env=env,stdout=f,stderr=subprocess.STDOUT,**kw)
 with receipt_lock:
  receipts.append(dict(tag=tag,argv=cmd,exit_code=p.returncode,seconds=time.monotonic()-start))
  (O/'proof/R11D-VALIDATION.json').write_text(json.dumps(dict(status='RUNNING' if all(r['exit_code']==0 for r in receipts) else 'FAIL',runs=receipts),indent=2)+'\n')
 assert p.returncode==0,(tag,p.returncode)
 print('PASS',tag,round(receipts[-1]['seconds'],2),flush=True)
features='native_timing,frame_total_only,sim_maps_probe'
run('host-build',['cargo','build','--release','--offline','--locked','--features',features])
B=O/'proof/recovery-bin';shutil.copy2(Path(env['CARGO_TARGET_DIR'])/'release/looking-glass',B.with_suffix('.new'));os.replace(B.with_suffix('.new'),B)
assert b'BASELINE_RUN_FUTURE_BYTES' not in B.read_bytes(), 'Reject an instrumented diagnostic executable from the shared target'
run('baseline-final',[sys.executable,str(T/'baseline_final.py'),str(O),str(B)])
run('nested',[sys.executable,str(T/'nested_proof.py'),str(O),str(B)])
run('portal',[sys.executable,str(T/'portal_proof.py'),str(O),str(B)])
run('trend-regression',[sys.executable,str(T/'trend.py')])
run('dependency-units',[sys.executable,str(T/'dependency_tests.py'),str(O)])
run('texture',[sys.executable,str(T/'texture_proof.py'),str(O)])
run('unit-build',['cargo','test','--release','--no-run','--offline','--locked','--features',features,'--message-format=json'])
artifacts=[json.loads(l) for l in (O/'logs/r11b-unit-build.log').read_text().splitlines() if l.startswith('{')]
test=next(a['executable'] for a in artifacts if a.get('reason')=='compiler-artifact' and a.get('profile',{}).get('test') and a.get('executable'))
shutil.copy2(test,O/'proof/test-bin')
run('units',[sys.executable,str(T/'run_tests.py'),str(O)])
run('host-proof',[sys.executable,str(T/'host_proof.py'),str(O),'--binary',str(B)])
run('persistent',[sys.executable,str(T/'persistent_proof.py'),str(O),str(B)],timeout=1800)
independent=[
 ('repeated',[str(T/'repeated_proof.py'),str(O),str(B)]),
 ('visual',[str(T/'host_visual.py'),str(O/'visual'),'--binary',str(B)]),
 ('stack-sim',[str(T/'run_stack_host.py'),str(O)]),
 ('stack-oracle',[str(R/'tools/stack_r10/host_stack_oracle.py'),str(O),'--binary',str(B)])]
with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
 futures=[pool.submit(run,tag,[sys.executable,*argv],timeout=1800) for tag,argv in independent]
 for f in futures:f.result()
run('failure',[sys.executable,str(T/'failure_proof.py'),str(O),str(B)],timeout=1800)
for tag,argv in [
 ('visual-audit',[str(T/'visual_audit.py'),str(O)]),
 ('loading',[str(T/'loading_proof.py'),str(O)]),
 ('startup',[str(T/'startup_proof.py'),str(O),str(B)]),
 ('film',[str(T/'film_proof.py'),str(O/'film'),'--binary',str(B),'--film']),
 ('summarize',[str(T/'summarize.py'),str(O)]),
 ('formatter',[str(T/'log_fixture.py'),str(O)])]:run(tag,[sys.executable,*argv],timeout=1800)
(O/'proof/R11D-VALIDATION.json').write_text(json.dumps(dict(status='PASS',runs=receipts),indent=2)+'\n')
