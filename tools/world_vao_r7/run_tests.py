"""Release, locked, offline regression gates. Uses only the shared host target."""
from pathlib import Path
import json,os,subprocess,sys
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve()
env=dict(os.environ,PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+os.environ['PATH'],CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',RUSTUP_TOOLCHAIN='stable',RUSTFLAGS='--cfg observer_free_B')
rows=[]
for tag,selection in [('census','phase_census::tests'),('state-cache','render::vita_renderer::state_cache::tests'),('direct-resolver','frame_total_tests'),('uniform-fields','render::uniform_fields::tests'),('logging','logging_')]:
 cmd=['cargo','test','--release','--offline','--locked','--features','native_timing,frame_total_only,sim_maps_probe',selection,'--','--nocapture','--test-threads=1']
 with (O/'logs'/f'final-tests-{tag}.log').open('w') as f:p=subprocess.run(cmd,cwd=R,env=env,stdout=f,stderr=subprocess.STDOUT)
 rows.append(dict(selection=selection,exit_code=p.returncode,argv=cmd));(O/'proof/HOST-TESTS.json').write_text(json.dumps(rows,indent=2)+'\n')
 print(tag,p.returncode,flush=True);assert p.returncode==0
