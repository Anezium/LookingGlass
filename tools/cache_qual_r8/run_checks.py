"""Run host checks with r6's exact host recipe and production policy."""
from pathlib import Path
import json, os, subprocess, sys, time
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();prefix=sys.argv[2] if len(sys.argv)>2 else 'host'
c=json.loads((O/'configs/PL-baseline.json').read_text());c.update(audio_music_thread=False,audio_log_thread=False,audio_cache_qualification=False)
(O/'proof/deterministic-audio-config.json').write_text(json.dumps(c,indent=2)+'\n')
env=dict(os.environ,PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+os.environ['PATH'],RUSTUP_TOOLCHAIN='stable',CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',RUSTFLAGS='--cfg observer_free_B',LOOKING_GLASS_TEST_AUDIO_CONFIG=str(O/'proof/deterministic-audio-config.json'),LOOKING_GLASS_TEST_DATA='/Users/anezium/vita-work/alice-data/Alice1/bin/base',ALICE_TIMING_TEST_OUTPUT=str(O/'proof'/f'audio-fixtures-{prefix}-{time.time_ns()}'))
Path(env['ALICE_TIMING_TEST_OUTPUT']).mkdir(exist_ok=True)
results=[]
for policy in ['default','production']:
    if policy=='production':env.update(LOOKING_GLASS_TEST_SIM_SWITCHES='on',LOOKING_GLASS_TEST_NPC_THINK='on');env.pop('LOOKING_GLASS_TEST_HEDGE3',None)
    selections=[('static_cache',True)] if policy=='production' else [('cache_qualification',False),('static_cache',False),('static_cache',True),('audio::',False)]
    for index,(selection,ignored) in enumerate(selections):
        cmd=['cargo','test','--release','--offline','--locked','--features','native_timing,frame_total_only,sim_maps_probe',selection,'--','--nocapture','--test-threads=1']+(['--ignored'] if ignored else [])
        log=O/'logs'/f'{prefix}-{policy}-{index}.log';start=time.monotonic()
        with log.open('w') as out:p=subprocess.run(cmd,cwd=R,env=env,stdout=out,stderr=subprocess.STDOUT)
        results.append(dict(policy=policy,argv=cmd,exit_code=p.returncode,seconds=time.monotonic()-start,log=str(log)))
        (O/'proof/HOST-TESTS.json').write_text(json.dumps(results,indent=2)+'\n')
        print(policy,selection,'ignored',ignored,'exit',p.returncode,flush=True)
        assert p.returncode==0
