from pathlib import Path
import sys
import json,os,subprocess,time
O=Path(sys.argv[1]).resolve();R=Path('/Users/anezium/vita-work/lg-lever-matrix-r4');env=dict(os.environ,PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+os.environ['PATH'],RUSTUP_TOOLCHAIN='stable',CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',RUSTFLAGS='--cfg observer_free_B')
env['ALICE_TIMING_TEST_OUTPUT']=str(O/'proof/audio-native-fixtures')
(O/'proof/audio-native-fixtures').mkdir(exist_ok=True)
os.utime(R/'src/main.rs',None)
rows=[]
for tag,selection in [('audio','audio::'),('collision','collision::'),('sweep','sweep'),('world','world_'),('timer','frame_total_tests'),('submission','render::vita_renderer::submit::tests'),('logging','logging_')]:
 cmd=['cargo','test','--release','--offline','--locked','--features','native_timing,frame_total_only',selection,'--','--nocapture','--test-threads=1'];t=time.monotonic();log=O/'logs'/f'host-{tag}.log'
 with log.open('w') as f:p=subprocess.run(cmd,cwd=R,env=env,stdout=f,stderr=subprocess.STDOUT)
 rows.append({'selection':selection,'exit_code':p.returncode,'seconds':time.monotonic()-t,'log':str(log)})
 (O/'proof/HOST-TESTS.json').write_text(json.dumps(rows,indent=2)+'\n');print(tag,p.returncode,flush=True)
 assert p.returncode==0
subprocess.run(['/Users/anezium/vita-j0/venv/bin/python',str(R/'tools/lever_matrix_r4/world-probe.py'),str(O)],cwd=R,env=env,check=True)
