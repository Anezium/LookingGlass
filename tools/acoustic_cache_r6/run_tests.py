"""Host-only tests using the shared target and deterministic audio fixture."""
from pathlib import Path
import json,os,subprocess,sys,time
R=Path('/Users/anezium/vita-work/lg-acoustic-cache-r6');O=Path(sys.argv[1]).resolve()
c=json.loads((O/'configs/PL-baseline.json').read_text());c.update(audio_music_thread=False,audio_log_thread=False)
(O/'proof/deterministic-audio-config.json').write_text(json.dumps(c,indent=2)+'\n')
env=dict(os.environ,PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+os.environ['PATH'],RUSTUP_TOOLCHAIN='stable',CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',RUSTFLAGS='--cfg observer_free_B',LOOKING_GLASS_TEST_AUDIO_CONFIG=str(O/'proof/deterministic-audio-config.json'),LOOKING_GLASS_TEST_DATA='/Users/anezium/vita-work/alice-data/Alice1/bin/base',ALICE_TIMING_TEST_OUTPUT=str(O/'proof'/('audio-fixtures-'+str(time.time_ns()))))
Path(env['ALICE_TIMING_TEST_OUTPUT']).mkdir()
results=[]
for tag,selection,ignored in [('cache-maps','static_cache',True),('cache','static_cache',False),('audio','audio::',False),('collision','collision::',False),('census','phase_census::tests',False),('frame-total','frame_total_tests',False),('logging','logging_',False)]:
 cmd=['cargo','test','--release','--offline','--locked','--features','native_timing,frame_total_only,sim_maps_probe',selection,'--','--nocapture','--test-threads=1']+(['--ignored'] if ignored else [])
 t=time.monotonic();log=O/'logs'/f'test-{tag}.log'
 with log.open('w') as f:p=subprocess.run(cmd,cwd=R,env=env,stdout=f,stderr=subprocess.STDOUT)
 result={'tag':tag,'selection':selection,'ignored_explicitly':ignored,'exit_code':p.returncode,'seconds':time.monotonic()-t,'log':str(log)};results.append(result)
 (O/'proof/HOST-TESTS.json').write_text(json.dumps(results,indent=2)+'\n');print(tag,p.returncode,flush=True);assert p.returncode==0
# The production baseline enables NPC-query culling and disables dynamic buckets.
# Cover that completion policy explicitly, in addition to the default test policy.
env.update(LOOKING_GLASS_TEST_SIM_SWITCHES='on',LOOKING_GLASS_TEST_NPC_THINK='on')
# The test-only bucket selector reads env on every call. Unset is false without
# allocating a String; an explicit 'off' would measure test-control allocations.
env.pop('LOOKING_GLASS_TEST_HEDGE3',None)
cmd=['cargo','test','--release','--offline','--locked','--features','native_timing,frame_total_only,sim_maps_probe','static_cache','--','--nocapture','--test-threads=1','--ignored']
t=time.monotonic();log=O/'logs/test-cache-maps-production.log'
with log.open('w') as f:p=subprocess.run(cmd,cwd=R,env=env,stdout=f,stderr=subprocess.STDOUT)
results.append(dict(tag='cache-maps-production',selection='static_cache',ignored_explicitly=True,simulation_switches='on; dynamic buckets off',exit_code=p.returncode,seconds=time.monotonic()-t,log=str(log)))
(O/'proof/HOST-TESTS.json').write_text(json.dumps(results,indent=2)+'\n');assert p.returncode==0
