"""Local fixed-rules, health-window and package vacancy assertions."""
from pathlib import Path
import importlib.util,json,sys
O=Path(sys.argv[1]).resolve()
def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
p=json.loads((O/'CONSOLE-PROTOCOL.json').read_text());assert len(p['sequence'])==8 and len(p['optional_sequence'])==4
base=json.loads((O/'configs/PL-baseline.json').read_text());cache=json.loads((O/'configs/PL-skin.json').read_text())
assert {k for k in base.keys()|cache.keys() if base.get(k,False)!=cache.get(k,False)}=={'skin_actual_consumers'}
assert not base.get('phase_census',False) and not cache.get('phase_census',False)
assert all(not __import__('os').path.lexists(O/n) for n in p['session_directories'])
h=load('health',O/'console/run_health.py')
assert not h.health_text('[cpal/vita] late_callbacks=1\nbench: camera frozen map=hedge3\n[cpal/vita] late_callbacks=0\nbench: done 120 frames mode=fullbench\n[cpal/vita] late_callbacks=2')
assert h.health_text('bench: camera frozen map=hedge3\n[cpal/vita] late_callbacks=1\nbench: done 120 frames mode=fullbench')
assert not h.health_text('[miniquad/vita] memory reason=loading newlib_uordblks=190000000\nbench: camera frozen map=hedge3\n[miniquad/vita] memory reason=benchmark newlib_uordblks=150000000\nbench: done 120 frames mode=fullbench')
assert h.health_text('bench: camera frozen map=hedge3\n[miniquad/vita] memory reason=benchmark newlib_uordblks=190000000\nbench: done 120 frames mode=fullbench')
for e in p['sequence']+p['optional_sequence']:assert h.entry_for_label(e['label'],O/'CONSOLE-PROTOCOL.json')==e
# Strict boundary values, including two orientations with opposite signs.
comparator=load('comparator',O/'console/compare-abba.py')
benefit=lambda d:comparator.classify('pandemonium',d)=='BENEFIT'
nonregression=lambda d:comparator.classify('hedge3',d)=='NONREGRESSION'
assert benefit([-1001,-1001]) and not benefit([-1000,-1001]) and not benefit([-3000,0])
assert nonregression([1000,-2000]) and not nonregression([1001,-2000])
print('PASS protocol, exact effective switch, optional identities, benchmark-only audio/heap and strict reading boundaries')

assert h.health_text("fatal error during loading\nbench: camera frozen map=hedge3\nbench: done 120 frames mode=fullbench")
