"""Final host unit regression. Captures our binary before another shared build."""
from pathlib import Path
import hashlib,json,os,re,shutil,subprocess,sys,time
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();rows=[]
env=dict(os.environ,PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+os.environ['PATH'],RUSTUP_TOOLCHAIN='stable',CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',RUSTFLAGS='--cfg observer_free_B',CARGO_INCREMENTAL='0')
cmd=['cargo','test','--release','--offline','--locked','--features','native_timing,frame_total_only,sim_maps_probe','sah_','--','--nocapture','--test-threads=1']
log=O/'logs/tests-final.log';start=time.monotonic()
with log.open('w')as out:p=subprocess.run(cmd,cwd=R,env=env,stdout=out,stderr=subprocess.STDOUT)
assert p.returncode==0
rows.append(dict(argv=cmd,exit_code=p.returncode,seconds=time.monotonic()-start,log=str(log)))
path=Path(re.search(r'Running unittests .*\(([^)]+)\)',log.read_text())[1]);binary=O/'proof/test-bin';shutil.copy2(path,binary)
for tag,selection in [('collision','collision::'),('census','phase_census::tests'),('config','render::vita_renderer::tests'),('frame-total','frame_total_tests')]:
 cmd=[str(binary),selection,'--nocapture','--test-threads=1'];log=O/'logs'/f'final-{tag}.log';start=time.monotonic()
 with log.open('w')as out:p=subprocess.run(cmd,cwd=R,env=env,stdout=out,stderr=subprocess.STDOUT)
 rows.append(dict(argv=cmd,exit_code=p.returncode,seconds=time.monotonic()-start,log=str(log)));assert p.returncode==0,tag
(O/'proof/HOST-TESTS.json').write_text(json.dumps(dict(status='PASS',binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),runs=rows),indent=2)+'\n')
print('PASS final synthetic, collision, census, config and frame-total regression',flush=True)
