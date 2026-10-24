"""Offline transport-blocked checks for the r5 package. No console ownership."""
from pathlib import Path
import contextlib,copy,csv,hashlib,importlib.util,io,json,os,shutil,socket,subprocess,sys,tempfile
from unittest.mock import patch
O=Path(sys.argv[1]).resolve() if len(sys.argv)>1 else Path(__file__).resolve().parents[1]
sys.path.insert(0,str(O/'console'));sys.path.insert(1,'/Users/anezium/vita-j0/bin')
checks=[]
def ok(name):checks.append(name)
def reject(name,call):
 try:call()
 except (AssertionError,ValueError,KeyError):ok(name)
 else:raise AssertionError('Accepted invalid fixture: '+name)
def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
@contextlib.contextmanager
def no_transport():
 def deny(*a,**k):raise AssertionError('Offline check attempted transport or a subprocess')
 with contextlib.ExitStack() as e:
  for obj,name in [(socket.socket,'connect'),(socket.socket,'connect_ex'),(socket.socket,'bind'),(socket,'create_connection'),(socket,'getaddrinfo'),(subprocess,'Popen'),(os,'system')]:e.enter_context(patch.object(obj,name,deny))
  yield
with no_transport():
 import pilot as p,session as s,run_health as h,light_csv as light,census_report as census,map_identity as identity
 for f in (O/'console').glob('*.py'):load('offline_'+f.stem.replace('-','_'),f);ok('import '+f.name)
 protocol=O/'CONSOLE-PROTOCOL.json';seq=json.loads(protocol.read_text())['sequence'];assert len(seq)==8
 for e in seq:assert h.entry_for_label(e['label'],protocol)==e;ok('exact protocol '+e['label'])
 reject('unknown label',lambda:h.entry_for_label(seq[0]['label']+'-foreign',protocol))
 baseline=(O/'configs/PL-baseline.json').read_bytes();old=Path('/Users/anezium/vita-j0/resume-2026-10-24/alice-lever-matrix-r4/configs/PL-baseline.json').read_bytes();assert baseline==old;ok('baseline byte identity')
 a=json.loads(baseline);b=json.loads((O/'configs/PL-census.json').read_text());assert {k for k in a.keys()|b.keys() if a.get(k,False)!=b.get(k,False)}=={'phase_census'};ok('one boolean only')
 for arm in ['baseline','census']:
  for mode in ['self','full']:
   m=json.loads((O/f'stage-{arm}-{mode}.json').read_text())
   assert all(Path(f['local']).is_absolute() for f in m['files'])
   if mode=='self':assert [f['remote'] for f in m['files']]==['ux0:/app/ALICELG01/eboot.bin']
   else:assert {f['remote'] for f in m['files']}=={p.R+x for x in ['vita_renderer.json','vita_sim.json','native-timing-config.json','profile_mode']}
   ok('manifest '+arm+' '+mode)
 for text in ['newlib_uordblks=188743681','heap_uordblks_bytes=188743681',"thread 'main' panicked",'out of memory']:
  assert h.health_text(text);ok('heap/crash stop '+text)
 assert not h.health_text('newlib_uordblks=188743680');ok('180MiB boundary')
 for text in ['producer_mix_p95_ms=8','pcm_starved_blocks=1','rejected_sinks=1','dropped_reports=1']:
  assert h.health_text('bench: camera frozen map=hedge3\n[lg/audio-realtime] '+text+'\nbench: done 120 frames mode=fullbench');ok('strict health '+text)
 receipt=dict(sha256=identity.HUMAN_AUTO,bytes=181900,visit_key='pandemonium$first',map='pandemonium$first',canonical_map='pandemonium',fresh_session_start_readback=True,companions_restored_and_verified=True,independent_readbacks=2)
 identity.verify_save_receipt(receipt);ok('pinned save identity')
 for key,value in [('sha256','0'*64),('bytes',1),('companions_restored_and_verified',False),('independent_readbacks',1),('canonical_map','hedge3')]:
  wrong=dict(receipt,**{key:value});reject('save rejects '+key,lambda wrong=wrong:identity.verify_save_receipt(wrong))
 data=json.loads((O/'host/hedge3-2-census/hedge3-census-180.json').read_text());census.validate(data,'hedge3');ok('real host census closes')
 for name,change in [('missing row',lambda d:d['rows'].pop()),('negative residual',lambda d:d['rows'][60].update(unbracketed_residual_ns=-1)),('hierarchy error',lambda d:d['rows'][60].update(hierarchy_errors=1)),('misalignment',lambda d:d['rows'][60].update(measured_frame=2)),('nested double sum',lambda d:d['rows'][60]['exclusive_ns'].__setitem__(0,10**10)),('invalid hits',lambda d:d['rows'][60]['static_queries_total_hit1_hit4_hit16'].__setitem__(0,[0,1,1,1]))]:
  wrong=copy.deepcopy(data);change(wrong);reject('census rejects '+name,lambda wrong=wrong:census.validate(wrong,'hedge3'))
 with tempfile.TemporaryDirectory(prefix='alice-census-offline-',dir=O/'proof') as tmp:
  root=Path(tmp);(root/'BASELINE.json').write_text('{}')
  lightfile=root/'light.csv';doors=root/'doors.csv';cf=root/'census.json';cf.write_text(json.dumps(data))
  def write_light(value):
   with lightfile.open('w',newline='') as f:w=csv.writer(f);w.writerow(['frame','wall_us','frame_cpu_us','gl_observers']);w.writerows([[i,value,'','direct'] for i in range(1,121)])
  write_light(60000);doors.write_text('frame_first,frame_last,frames,hatter_door_world_traces,hatter_door_own_traces,hatter_door_trace_pairs_deferred\n1,60,60,,,\n61,120,60,,,\n')
  seen=[]
  def snapshot(tag,paths):
   seen.append(paths);local=[lightfile,doors]+([cf] if len(paths)==3 else [])
   result=[]
   for remote,source in zip(paths,local):
    dest=p.B/tag/source.name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(source,dest)
    result.append(dict(path=remote,present=True,local=str(dest),sha256=hashlib.sha256(dest.read_bytes()).hexdigest()))
   return result
  for e in seq[4:6]:
   (root/e['label']).mkdir()
   with patch.object(s,'ROOT',root),patch.object(s,'PACKAGE',O),patch.object(p,'ROOT',root),patch.object(p,'snapshot',snapshot),patch.object(s,'capture_log',lambda label:None),contextlib.redirect_stdout(io.StringIO()):s.collect(e['label'])
   assert len(seen[-1])==(3 if e['arm']=='B' else 2);ok('paired independent collect '+e['arm'])
   write_light(60001)
  e=seq[6];(root/e['label']).mkdir()
  with patch.object(s,'ROOT',root),patch.object(s,'PACKAGE',O),patch.object(p,'ROOT',root),patch.object(p,'snapshot',snapshot),patch.object(s,'capture_log',lambda label:None):reject('stale census stops',lambda:s.collect(e['label']))
  # Real stage(), all transfer methods replaced. No transport bypass is possible.
  class FakeSession:
   def __init__(self,*a,**k):pass
   def __enter__(self):return self
   def __exit__(self,*a):pass
  for e in seq[4:6]:
   def stage_manifest(path,ledger):return json.loads(Path(path).read_text())
   def config_snapshot(tag,paths):return [dict(path=f['remote'],present=True,local=f['local']) for f in json.loads((O/f"stage-{e['config_arm']}-full.json").read_text())['files']]
   with patch.object(s,'ROOT',root),patch.object(s,'PACKAGE',O),patch.object(p,'ROOT',root),patch.object(p,'Session',FakeSession),patch.object(p,'listing',lambda *a:{}),patch.object(p,'snapshot',config_snapshot),patch.object(s,'stage_manifest',stage_manifest),contextlib.redirect_stdout(io.StringIO()):s.stage(e['label'],e['arm'],'full')
   ok('stage exact config '+e['arm'])
  calls=[]
  with patch.object(s,'ROOT',root),patch.object(p,'ROOT',root),patch.object(s.subprocess,'run',lambda args,**kw:calls.append(Path(args[1]).name)):
   s.stage('human-restore','human-restore','full')
  assert calls==['restore-ledgers.py','verify-self.py','restore-files.py'];ok('restoration dispatch order')
 # Bind only a fresh helper-only session, with exact source hashes.
 binder=load('offline_binder',O/'scripts/bind_helpers.py')
 with tempfile.TemporaryDirectory(prefix='alice-bind-fixture-',dir=O/'proof') as tmp:
  package=Path(tmp);(package/'console').mkdir();root=package/'session-s6-phase-census';root.mkdir()
  for f in (O/'console').glob('*.py'):
   shutil.copyfile(f,package/'console'/f.name);shutil.copyfile(f,root/f.name)
  with patch.object(sys,'argv',['bind_helpers.py','--root',str(root),'--package',str(package)]),contextlib.redirect_stdout(io.StringIO()):binder.main()
  assert json.loads((root/'HELPER-BINDING.json').read_text())['status']=='PASS';ok('fresh helper binding')
  with patch.object(sys,'argv',['bind_helpers.py','--root',str(root),'--package',str(package)]):reject('nonfresh binding refused',binder.main)
 # Observer labels use the fixed strict >1 ms rule in each orientation.
 comparator=load('offline_comparator_rules',O/'console/compare-abba.py')
 with tempfile.TemporaryDirectory(prefix='alice-observer-rules-',dir=O/'proof') as tmp:
  root=Path(tmp)
  for e in seq[4:]:
   run=root/e['label'];run.mkdir();(run/'STRICT-AUDIO.json').write_text('{"status":"PASS"}')
   (run/'ENTRY-STATE.json').write_text(json.dumps({'startup_camera_observations':[dict(camera_eye=[0,0,0],camera_target=[1,0,0],camera_up=[0,0,1],feet=[0,0,0])]}))
  for medians,wanted in [([60,60.999,60.999,60],'NO_RESOLVED_OBSERVER_COST'),([60,61,61,60],'NO_RESOLVED_OBSERVER_COST'),([60,61.001,59,60],'OBSERVER_PERTURBED'),([60,59,61.001,60],'OBSERVER_PERTURBED')]:
   for e,median in zip(seq[4:],medians):
    (root/e['label']/'AUDIT.json').write_text(json.dumps(dict(status='PASS',fields_ms={'wall_us':dict(n=120,median=median)},gl_observers='direct',observer_checks=dict(status='PASS',expected_mode='direct'),runtime_config_matches_readback=True,complete_benchmark=True,clean_exit=True)))
   with patch.object(sys,'argv',['compare-abba.py','--root',str(root),'--protocol',str(protocol),'--block',seq[4]['block']]),contextlib.redirect_stdout(io.StringIO()):
    try:comparator.main()
    except SystemExit as e:assert e.code==0
   value=json.loads((root/(seq[4]['block']+'-ABBA-GATE.json')).read_text());assert value['status']==wanted;ok('observer threshold '+str(medians))
 # End-to-end report assembly from eight synthetic admitted audits and real host census rows.
 assembler=load('offline_result_assembler',O/'console/make-report.py')
 with tempfile.TemporaryDirectory(prefix='alice-report-fixture-',dir=O/'proof') as tmp:
  root=Path(tmp)
  for e in seq:
   run=root/e['label'];run.mkdir();(run/'STRICT-AUDIO.json').write_text('{"status":"PASS"}')
   (run/'ENTRY-STATE.json').write_text(json.dumps({'startup_camera_observations':[dict(camera_eye=[0,0,0],camera_target=[1,0,0],camera_up=[0,0,1],feet=[0,0,0])]}))
   audit=dict(status='PASS',fields_ms={'wall_us':dict(n=120,median=60 if e['arm']=='A' else 60.2)},gl_observers='direct',observer_checks=dict(status='PASS',expected_mode='direct'),runtime_config_matches_readback=True,complete_benchmark=True,clean_exit=True,memory_samples=[dict(newlib_uordblks=100*1048576)],audio={which:{'raw':{'realtime':[dict(pcm_starved_blocks='0',rejected_sinks='0',dropped_reports='0')]}} for which in ['entry_loading_information_only','shutdown_information']})
   (run/'AUDIT.json').write_text(json.dumps(audit))
   if e['map']=='pandemonium':(run/'SAVE-IDENTITY.json').write_text(json.dumps(receipt))
   if e['arm']=='B':
    frame=O/'host'/f"{e['map']}-2-census"/f"{e['map']}-census-180.json"
    (run/'COLLECT.json').write_text(json.dumps({'files':[{}, {},dict(local=str(frame),sha256=hashlib.sha256(frame.read_bytes()).hexdigest())]}))
  for e in [seq[0],seq[4]]:
   with patch.object(sys,'argv',['compare-abba.py','--root',str(root),'--protocol',str(protocol),'--block',e['block']]),contextlib.redirect_stdout(io.StringIO()):
    try:comparator.main()
    except SystemExit as ex:assert ex.code==0
  with patch.object(sys,'argv',['make-report.py','--root',str(root),'--package',str(O),'--output',str(root/'RESULTS.json')]):assembler.main()
  result=json.loads((root/'RESULTS.json').read_text());assert len(result['maps'])==2 and all(len(m['breakdowns'])==2 for m in result['maps']);ok('eight-run report assembly')
 # Immutable r4 transport, preservation and restoration contracts stay byte-identical.
 old=Path('/Users/anezium/vita-work/lg-phase-census-r5/tools/lever_matrix_r4/console')
 for name in ['pilot.py','preserve.py','baseline-r3.py','save-proof.py','verify-human-entry.py','verify-self.py','restore-files.py','restore-ledgers.py','restore-arm.py','verify-arm-self.py','map_identity.py','check-session-budget.py','ui.py','ack.py','strict-audio.py']:
  assert (old/name).read_bytes()==(O/'console'/name).read_bytes();ok('inherited contract bytes '+name)
result={'status':'PASS','offline_only':True,'transport_blocked':True,'checks':checks,'count':len(checks)}
(O/'proof/CONSOLE-DRY-RUN.json').write_text(json.dumps(result,indent=2)+'\n');print(len(checks),'offline checks passed')
