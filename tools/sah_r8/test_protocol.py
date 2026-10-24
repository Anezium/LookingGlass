"""Fixed pre-data gates and protocol identities; no device imports."""
from pathlib import Path
import importlib.util,json,os,sys
O=Path(sys.argv[1]).resolve()
def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
p=json.loads((O/'CONSOLE-PROTOCOL.json').read_text())
assert len(p['sequence'])==8 and len(p['optional_sequence'])==2 and len(p['smoke_sequence'])==2
assert [e['map']for e in p['sequence']]==['hedge3']*4+['pandemonium']*4
assert [e['arm']for e in p['optional_sequence']]==['A','B'] and all(e['map']=='hedge3'and e['census'] for e in p['optional_sequence'])
assert all(e['arm']=='B'and e['smoke'] for e in p['smoke_sequence'])
base=json.loads((O/'configs/PL-baseline.json').read_text());b=json.loads((O/'configs/PL-sah.json').read_text())
assert {k for k in base.keys()|b.keys()if base.get(k,False)!=b.get(k,False)}=={'collision_sah'}
for name in ['baseline','sah','baseline-census','sah-census']:
 cfg=json.loads((O/'configs'/f'PL-{name}.json').read_text());assert not cfg['audio_static_cache']
for name in ['session-s9-sah','session-s9-sah-load-smoke']:assert not os.path.lexists(O/name)
h=load('health',O/'console/run_health.py')
assert not h.health_text('bench: camera frozen map=hedge3\n[cpal/vita] late_callbacks=1 underrun_estimates=0\nbench: done 120 frames mode=fullbench')
assert h.health_text('bench: camera frozen map=hedge3\n[cpal/vita] late_callbacks=0 underrun_estimates=1\nbench: done 120 frames mode=fullbench')
assert h.health_text('[miniquad/vita] memory reason=loading newlib_uordblks=190000000\nbench: camera frozen map=hedge3\n[miniquad/vita] memory reason=benchmark newlib_uordblks=150000000\nbench: done 120 frames mode=fullbench')
assert h.health_text("thread 'main' panicked before benchmark")
for e in p['sequence']+p['optional_sequence']+p['smoke_sequence']:assert h.entry_for_label(e['label'],O/'CONSOLE-PROTOCOL.json')==e
c=load('comparator',O/'console/compare-abba.py')
assert c.classify('hedge3',[-1001,-1001])=='BENEFIT'
assert c.classify('hedge3',[-1000,-1001])!='BENEFIT'
assert c.classify('hedge3',[-3000,0])!='BENEFIT'
assert c.classify('pandemonium',[1000,-2000])=='NONREGRESSION'
assert c.classify('pandemonium',[1001,-2000])=='REGRESSION'
assert p['heap_budget']['oriented_delta_max_MiB']==8 and p['heap_budget']['hedge3_plateau_exclusive_MiB']==165
print('PASS fixed rules, 12 labels, two fresh sessions, one-switch arms, whole-run heap stop, report-only late callbacks')

r=load('report',O/'console/make-report.py')
assert r.heap_status('hedge3',[150,158,158,150])=='WITHIN_SAH_BUDGET'
assert r.heap_status('hedge3',[150,158.001,158,150])=='HEAP_NOT_CONFIRMED'
assert r.heap_status('hedge3',[157,165,164,157])=='HEAP_NOT_CONFIRMED'
assert r.heap_status('hedge3',[157,164.999,164.999,157])=='WITHIN_SAH_BUDGET'
print('PASS +8 MiB inclusive and 165 MiB exclusive heap boundaries')
