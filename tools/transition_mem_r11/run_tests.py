"""Focused host regression with the test suites' required output environment."""
from pathlib import Path
import subprocess,hashlib,json,time,os,sys
O=Path(sys.argv[1]).resolve();B=O/'proof/test-bin';env=dict(os.environ,ALICE_TIMING_TEST_OUTPUT=str(O/'proof'/('audio-tests-'+str(time.time_ns()))),LOOKING_GLASS_TEST_DATA='/Users/anezium/vita-work/alice-data/Alice1/bin/base',SAH_OUTPUT='/Users/anezium/vita-j0/resume-2026-10-24/alice-sah-r8')
rows=[]
for selection,ignored in [('transition_memory::tests',False),('save::tests',False),('save::format12',False),('recovery::',False),('campaign::',False),('collision::',False),('audio::',False),('render::vita_renderer::tests',False),('phase_census::tests',False),('character::skinning::overlap::tests',False),('sah_real_maps_corpus_masks_and_load',True),('static_cache_map_output_is_sample_exact',True),('static_cache_randomized_maps_and_kernel_measurement',True)]:
 log=O/'logs'/('unit-'+selection.replace('::','-')+'.log');cmd=[str(B),selection,'--nocapture','--test-threads=1']+(['--ignored'] if ignored else []);t=time.monotonic()
 with log.open('w') as f:p=subprocess.run(cmd,stdout=f,stderr=subprocess.STDOUT,env=env)
 assert 'running 0 tests' not in log.read_text(), selection
 rows.append(dict(argv=cmd,selection=selection,exit_code=p.returncode,seconds=time.monotonic()-t,log=str(log)));assert p.returncode==0,selection
(O/'proof/HOST-TESTS.json').write_text(json.dumps(dict(status='PASS',binary_sha256=hashlib.sha256(B.read_bytes()).hexdigest(),environment={k:env[k] for k in ['ALICE_TIMING_TEST_OUTPUT','LOOKING_GLASS_TEST_DATA','SAH_OUTPUT']},runs=rows),indent=2)+'\n')
