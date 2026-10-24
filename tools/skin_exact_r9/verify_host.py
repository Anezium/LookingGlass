"""Consolidate host evidence and allocation bounds. No console boundaries imported."""
from pathlib import Path
import hashlib,importlib.util,json,re,sys
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve()
def read(n):return json.loads((O/'proof'/n).read_text())
maps=read('HOST-MAPS.json');fixtures=read('HOST-FIXTURES.json');comparison=read('R7-VERTEX-COMPARISON.json');measure=read('HOST-MEASURE.json');consumers=read('HOST-CONSUMERS.json')
assert all(x['status']=='PASS' for x in [maps,fixtures,comparison,measure,consumers])
assert len(maps['runs'])==28 and len(comparison['maps'])==14
assert all(len(r['frames'])==180 for r in maps['runs'])
assert all(len(r['frames'])==900 and len(r['png_sha256'])==900 for r in fixtures['runs'])
assert fixtures['runs'][0]['png_sha256']==fixtures['runs'][1]['png_sha256']
assert read('R7-REGRESSIONS.json')['status']=='PASS_THREE_R7_DEFECTS_DETECTED'
spec=importlib.util.spec_from_file_location('census',R/'tools/skin_exact_r9/console/census_report.py');c=importlib.util.module_from_spec(spec);spec.loader.exec_module(c)
for r in maps['runs']:
 d=O/'host'/f"{r['map']}-exact-{int(r['arm'])}"
 c.validate(json.loads((d/f"{r['map']}-census-180.json").read_text()),r['map'])
old=json.loads(Path('/Users/anezium/vita-j0/resume-2026-10-24/alice-skin-consumers-r7/proof/HOST-MEASURE.json').read_text())
idle={};alloc={};hits={}
for mapname,data in measure['maps'].items():
 old_runs=[r for r in old['maps'][mapname]['runs'] if r['kind']=='census'];new_runs=[r for r in data['runs'] if r['kind']=='census']
 for a,b in zip(old_runs,new_runs):
  for key in ['mean_serial_vertices','mean_worker_vertices']:assert a[key]==b[key],(mapname,key)
 idle[mapname]=[dict(arm=r['arm'],serial_vertices=r['mean_serial_vertices'],worker_vertices=r['mean_worker_vertices']) for r in new_runs]
 values=[]
 for n,arm in [(5,False),(6,True)]:
  d=O/'host'/f'{mapname}-{n}-alloc';frames=[json.loads(l.split(' ',1)[1]) for l in (d/'run.log').read_text().splitlines() if l.startswith('SIM_MAPS_FRAME ')]
  values.append([f['counters'].get('skin_numeric_allocation_calls',0) for f in frames])
 assert len(values[0])==len(values[1])==180
 assert all(b<=a for a,b in zip(values[0][60:],values[1][60:]))
 alloc[mapname]=dict(first_frame_A=values[0][0],first_frame_B=values[1][0],largest_B_minus_A_first_60=max(b-a for a,b in zip(values[0][:60],values[1][:60])),measured_A=sum(values[0][60:]),measured_B=sum(values[1][60:]),no_added_allocations_on_any_measured_frame=True)
for r in maps['runs']:
 if r['arm']:continue
 log=O/'host'/f"{r['map']}-exact-0/run.log"
 hits[r['map']]=[json.loads(l.split(' ',1)[1]) for l in log.read_text().splitlines() if l.startswith('SIM_SKIN_HITS ')]
tests=[]
for r in read('HOST-TESTS.json'):
 assert r['exit_code']==0
 found=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',Path(r['log']).read_text());assert len(found)==1 and found[0][1:] == ('0','0')
 tests.append(dict(selection=r['tag'],passed=int(found[0][0])))
result=dict(status='PASS',map_pairs=14,map_frames_per_arm=2520,fixture_frames_per_arm=900,fixture_png_pairs=900,gpu_readback_draws=sum(r['gpu_draw_readbacks'] for r in maps['runs']+fixtures['runs']),idle_vertices_identical_to_r7=idle,numeric_capture_allocations=alloc,damage_witnesses=hits,tests=tests,allocation_scope='Numeric capture counters exclude GPU/image oracle observers. Cold/reentry bank tests separately bound lazy storage. No global zero-allocation claim.',native_fixture_exclusion='skin_exact_check is cfg(sim_maps_probe && !vita); the post-native edit only persists each intermediate PNG, without changing any production path.')
(O/'proof/HOST-QUALIFICATION.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({k:result[k] for k in ['status','map_pairs','map_frames_per_arm','fixture_frames_per_arm','fixture_png_pairs','gpu_readback_draws','numeric_capture_allocations','tests']},indent=2))
