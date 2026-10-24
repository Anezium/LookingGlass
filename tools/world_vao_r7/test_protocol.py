"""Pure local protocol/threshold/schema checks; no transport import."""
from pathlib import Path
import importlib.util,json,socket,sys,copy,tempfile,runpy,io,contextlib
from unittest.mock import patch
O=Path(sys.argv[1]).resolve()
def load(n,p):
 s=importlib.util.spec_from_file_location(n,p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
def deny(*a,**k):raise AssertionError('No network in package tests')
with patch.object(socket.socket,'connect',deny),patch.object(socket,'create_connection',deny):
 protocol=json.loads((O/'CONSOLE-PROTOCOL.json').read_text());seq=protocol['sequence'];diag=protocol['optional_sequence']
 assert len(seq)==8 and len(diag)==4 and protocol['diagnostic_runs_required']
 assert len({e['label'] for e in seq+diag})==12
 for m in ['pandemonium','hedge3']:
  assert [e['arm'] for e in seq if e['map']==m]==['A','B','B','A']
  assert [e['arm'] for e in diag if e['map']==m]==['A','B']
 assert all(not e['census'] for e in seq) and all(e['census'] for e in diag)
 cmp=load('comparator',O/'console/compare-abba.py')
 assert cmp.classify('hedge3',[-1000,-1001])=='NO_CONFIRMED_BENEFIT'
 assert cmp.classify('hedge3',[-1001,-1001])=='BENEFIT'
 assert cmp.classify('hedge3',[-2000,0])=='NO_CONFIRMED_BENEFIT'
 assert cmp.classify('pandemonium',[1000,999])=='NONREGRESSION'
 assert cmp.classify('pandemonium',[1001,-5000])=='REGRESSION'
 health=load('health',O/'console/run_health.py')
 for e in seq+diag:assert health.entry_for_label(e['label'],O/'CONSOLE-PROTOCOL.json')==e
 bad='bench: camera frozen map=hedge3\n[cpal/vita] late_callbacks=1 underrun_estimates=0\nbench: done 120 frames mode=fullbench\n'
 assert health.health_text(bad)
 budget=load('budget',O/'console/check-session-budget.py')
 identity=dict(pid=4321,started='2026-10-24T08:00:00+00:00',fixed_expiry='2026-10-24T20:00:00+00:00')
 with patch.object(budget,'driver_identity',lambda ready:identity):
  start=budget.timestamp(identity['started'])
  assert budget.check({},start,start,launches=12,pandemonium_launches=6)['status']=='STOP_RESTORE'
  assert budget.check({},start,start,launches=8,pandemonium_launches=4)['remaining_minutes']==127.5
  assert budget.check({},start,start,launches=4,pandemonium_launches=2)['remaining_minutes']==88.5
 for arm in protocol['config_changes']:
  cfg=json.loads((O/'configs'/f'PL-{arm}.json').read_text())
  assert cfg.get('world_vertex_input',False)==arm.startswith('vao')
  assert cfg.get('phase_census',False)==arm.endswith('census')
  for mode in ['self','full']:
   manifest=json.loads((O/f'stage-{arm}-{mode}.json').read_text())
   remotes=[f['remote'] for f in manifest['files']]
   assert len(remotes)==(1 if mode=='self' else 4)
   assert all('sce_sys' not in p for p in remotes)
   if mode=='self':assert remotes==['ux0:/app/ALICELG01/eboot.bin']
 # Exercise the actual whole-log main with both wall and census sessions.
 with tempfile.TemporaryDirectory(prefix='vao-log-integrity-') as tmp:
  package=Path(tmp);root=package/'session-s8-world-vao';root.mkdir()
  (package/'CONSOLE-PROTOCOL.json').write_bytes((O/'CONSOLE-PROTOCOL.json').read_bytes())
  def integrity(e,extra='',padding=0,expected=0):
   d=root/e['label'];d.mkdir(exist_ok=True)
   c=json.loads((O/'configs'/f"PL-{e['config_arm']}.json").read_text());c.setdefault('phase_census',False);c.setdefault('world_vertex_input',False)
   (d/'runtime-log.txt').write_text('[lg/vita-renderer] config '+json.dumps(c,separators=(',',':'))+' '*padding+'\n'+extra)
   with patch.object(sys,'argv',['log-integrity.py',e['label'],'--root',str(root)]),contextlib.redirect_stdout(io.StringIO()):
    try:runpy.run_path(str(O/'scripts/log-integrity.py'),run_name='__main__')
    except SystemExit as error:assert error.code==expected,(e['label'],error.code,expected)
  for e in seq+diag:integrity(e)
  integrity(seq[2],extra=' orphan_field=1\n',expected=2)
  integrity(seq[2],padding=1,expected=2)
  integrity(diag[-1],extra='prefix[lg/broken] field=1\n',expected=2)
 print('PASS twelve identities, four configs, manifests, 150-minute cap, strict late callbacks, both oriented thresholds, arm-aware log integrity and corruption/drift rejection')
