"""Execute the entire r6 session offline. All device/driver boundaries are fakes.

This is a synthetic session, never hardware evidence. Real helper main functions
produce every preservation, staging, collection, audit and restoration receipt.
--sealed-helpers replays the identical test using the archived pre-fix sources.
"""
from pathlib import Path
import argparse,contextlib,copy,hashlib,importlib.util,io,json,os,runpy,shutil,socket,subprocess,sys,tempfile,time,types,zipfile
from unittest.mock import patch

HUMAN_SELF=Path('/Users/anezium/vita-j0/resume-2026-10-22/alice-card-think/console-r4/restored-self-independent.bin')
HUMAN_SAVE=Path('/Users/anezium/vita-j0/resume-2026-10-24/alice-lever-matrix-r3/session-s5-world-static-merge/originals/data/alice/private/saves/auto.json')
ENGINE=None
def sha(value):return hashlib.sha256(value).hexdigest()
def write(path,value):path.parent.mkdir(parents=True,exist_ok=True);path.write_text(json.dumps(value,indent=2)+'\n')
def deny(*args,**kwargs):raise AssertionError('OFFLINE BOUNDARY: real transport/process denied')
def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

def check_session_vacancy(package):
 root=package/'session-s7-acoustic-cache'
 if os.path.lexists(root):
  raise FileExistsError(f'Pre-admission package check: {root} already exists; archive blocked evidence before creating a fresh session')
 print('PASS pre-admission package check: real session-s7-acoustic-cache path is absent')

def exercise(source,work,sealed=None,evidence=None,include_optional=False):
 package=work/'package';root=package/'session-s7-acoustic-cache';root.mkdir(parents=True)
 for name in ['CONSOLE-PROTOCOL.json','PACKAGE-IDENTITY.json']:
  if (source/name).exists():shutil.copyfile(source/name,package/name)
 for name in ['configs','packages','host','console']:(package/name).symlink_to(source/name,target_is_directory=True)
 (package/'scripts').mkdir()
 if sealed:
  with zipfile.ZipFile(sealed) as archive:
   for name in archive.namelist():
    if name.startswith('console/'):dest=root/Path(name).name
    else:dest=package/name
    dest.parent.mkdir(parents=True,exist_ok=True);dest.write_bytes(archive.read(name))
 else:
  for path in (source/'console').glob('*.py'):shutil.copyfile(path,root/path.name)
  for path in (source/'scripts').glob('*.py'):shutil.copyfile(path,package/'scripts'/path.name)
 for manifest in source.glob('stage-*.json'):shutil.copyfile(manifest,package/manifest.name)
 global ENGINE
 ENGINE=json.loads((package/'PACKAGE-IDENTITY.json').read_text())['engine_source_revision']
 protocol=json.loads((package/'CONSOLE-PROTOCOL.json').read_text())
 seq=protocol['sequence']+(protocol['optional_sequence'] if include_optional else [])
 expected_launches=10 if include_optional else 8
 assert len(seq)==expected_launches and {e['map'] for e in seq}=={'pandemonium','hedge3'}
 R='ux0:/data/alice/private/';A='ux0:/app/ALICELG01/'
 human=HUMAN_SELF.read_bytes();save=HUMAN_SAVE.read_bytes()
 assert len(human)==37371674 and sha(human)=='387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990'
 assert len(save)==181900 and sha(save)=='ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9'
 remote={A+'eboot.bin':human,A+'sce_sys/param.sfo':b'fixture SFO preserved',R+'saves/auto.json':save,
  R+'saves/auto.previous.json':b'fixture save companion',R+'route-saves/companion.json':b'fixture route companion',
  R+'preferences.json':b'fixture human preferences',R+'audio-settings.txt':b'fixture audio settings',
  R+'vita_renderer.json':b'fixture human renderer',R+'vita_sim.json':b'fixture human sim',
  R+'native-timing-config.json':b'fixture human timing',R+'profile_mode':b'full\n',
  R+'frame_profile/pandemonium-fullbench-census-180.json':b'fixture prior census',
  R+'cache/shaders/original.bin':b'fixture shader cache'}
 initial=dict(remote);transfers=[];driver_calls=[];stages=[];rollbacks=[];backups={};active=0;maximum=0;current=None;tick=0
 log=work/'vita.log';log.write_bytes(b'OFFLINE SYNTHETIC SESSION\n')
 cache=work/'vita-stage-index.json';cache.write_bytes(b'{"fixture":"preserve me"}\n');cache_before=cache.read_bytes()
 # Empty pre-existing directories are modelled separately from file contents.
 directories={R,A,A+'sce_sys/',R+'frame_profile/',R+'saves/',R+'route-saves/',R+'cache/',R+'cache/shaders/'}
 def listing(directory):
  rows={}
  for d in directories:
   tail=d.removeprefix(directory).rstrip('/')
   if d.startswith(directory) and tail and '/' not in tail:rows[tail]=dict(bytes=0,directory=True)
  for path,value in remote.items():
   if path.startswith(directory) and '/' not in path[len(directory):]:rows[path[len(directory):]]=dict(bytes=len(value),directory=False)
  return rows
 class Transport:
  def __init__(self,*args,**kwargs):self.ftp=self
  def __enter__(self):
   nonlocal active,maximum
   assert active==0,'parallel FTP';active+=1;maximum=max(active,maximum);return self
  def __exit__(self,*args):
   nonlocal active
   active-=1
  def record(self,*args,**kwargs):pass
  def voidcmd(self,command):assert command=='TYPE I'
  def execute(self,op,remote_path,local=None):
   assert active==1
   transfers.append((op,remote_path))
   if op=='list':return '\n'.join(('d' if row['directory'] else '-')+'rwxr-xr-x 1 vita vita '+str(row['bytes'])+' Oct 24 00:00 '+name for name,row in listing(remote_path).items())
   if op=='size':return len(remote[remote_path])
   if op=='get':Path(local).parent.mkdir(parents=True,exist_ok=True);Path(local).write_bytes(remote[remote_path]);return
   if op=='put':remote[remote_path]=Path(local).read_bytes();return
   raise AssertionError(('Unexpected transport operation',op))
  def delete(self,path):assert active==1;transfers.append(('delete',path));del remote[path]
 def fake_run(args,**kwargs):
  nonlocal current
  assert isinstance(args,list)
  if args[:2]==['/bin/ps','-p']:
   assert args[2]=='4321';return subprocess.CompletedProcess(args,0,'python terminal-driver.py\n','')
  if args[0]=='/usr/sbin/lsof':
   return subprocess.CompletedProcess(args,0,'p123\n' if '-iTCP:9999' in args else 'n/Users/anezium/vita-j0/work/vita.log\n','')
  assert len(args)>1
  if Path(args[1]).name in ['restore-ledgers.py','verify-self.py','restore-files.py']:
   call(Path(args[1]).name);return subprocess.CompletedProcess(args,0)
  assert args[1]=='/Users/anezium/vita-j0/bin/vita-stage',('Unexpected subprocess',args)
  assert active==0
  if '--restore' in args:
   ledger=Path(args[args.index('--restore')+1]);data=json.loads(ledger.read_text())
   for path,value in backups[str(ledger.resolve())].items():
    if value is None:remote.pop(path,None)
    else:remote[path]=value
   data['restore_status']='restored';write(ledger,data);rollbacks.append(str(ledger))
  else:
   manifest=Path(args[args.index('--manifest')+1]);ledger=Path(args[args.index('--ledger')+1]);m=json.loads(manifest.read_text());original={};entries=[]
   for item in m['files']:
    path=item['remote'];original[path]=remote.get(path)
    old=remote.get(path);entries.append(dict(remote=path,original=dict(size=len(old),sha256=sha(old)) if old is not None else dict(present=False)))
    local=Path(item['local']);local=local if local.is_absolute() else manifest.parent/local
    remote[path]=local.read_bytes()
   backups[str(ledger.resolve())]=original;write(ledger,dict(status='PASS',entries=entries,parts=[],cache_created=[]));stages.append(str(manifest.name))
  return subprocess.CompletedProcess(args,0)
 def monotonic():
  nonlocal tick
  tick+=1;return tick*10.0
 def startup(entry):
  cfg=json.loads(remote[R+'vita_renderer.json']);cfg.setdefault('phase_census',False);cfg.setdefault('audio_static_cache',False)
  sim=json.loads(remote[R+'vita_sim.json'])
  lines=['[lg/vita-renderer] config '+json.dumps(cfg,separators=(',',':')),
   '[lg/vita-sim] config '+json.dumps(sim,separators=(',',':')),
   '[lg/startup] map_load map='+entry['map'],
   '[lg/startup] clocks mode=max rc=[0, 0, 0, 0] actual=[444, 222, 222, 166]',
   '[lg/renderer] retained=true','[lg/audio-output] buffered=true',
   '[lg/audio-offmain] requested=false active=false',
   '[lg/thread] effective_priority=161 effective_affinity=0x40000',
   '[lg/thread] effective_priority=170 effective_affinity=0x40000',
   '[lg/thread] effective_priority=128 effective_affinity=0x20000',
   '[lg/load-diagnostic] camera_eye=Vec3(0, 0, 0) camera_target=Vec3(1, 0, 0) camera_up=Vec3(0, 0, 1) feet=Vec3(0, 0, 0) paused=true',
   '[miniquad/vita] memory reason=loading newlib_uordblks=173015040']
  if entry['map']=='pandemonium':lines.append('[lg/startup] restored scene reused map=pandemonium source=continue-or-bench')
  return '\n'.join(lines)+'\n'
 def finish(entry):
  i=seq.index(entry)
  columns=['frame','wall_us','frame_cpu_us']+[name+'_cpu_us' for name in ['world_opaque','world_alpha','actors','fx','hud','present']]+['gl_observers']
  wall=60000+(1500 if entry['arm']=='B' else 0)+i
  remote[R+'frame_profile/'+entry['map']+'-fullbench-light-120.csv']=(','.join(columns)+'\n'+'\n'.join(','.join([str(j),str(wall)]+['']*(len(columns)-3)+['direct']) for j in range(1,121))+'\n').encode()
  remote[R+'frame_profile/'+entry['map']+'-fullbench-light-120-doors.csv']=b'frame_first,frame_last,frames,hatter_door_world_traces,hatter_door_own_traces,hatter_door_trace_pairs_deferred\n1,60,60,,,\n61,120,60,,,\n'
  if entry.get('census',False):
   names=['npc_think_movement','player_movement','fixed_physics','movers','objects','audio_prepare','prepare_acoustics','light_selection','world_opaque','world_visibility','world_parameters','world_submit','alpha_prepare','alpha_submit','actor_gather','actor_collect','actor_publication','actor_submit','fx_gather','joint_finish','fx_submit','hud','present','swap']
   data=dict(schema=1,map=entry['map'],capacity=180,regions=names,query_classes=['room','ambient','effects'],table_entries=4096,table_probes=16,parent_mask_frame_bit=24,owners=['audio','player','npc','camera','other'],rows=[])
   for n in range(180):
    data['rows'].append(dict(accepted_frame=n+1,measured_frame=n-59 if n>=60 else None,sequence=n+1,hierarchy_errors=0,frame_total_ns=25000000,bracket_union_ns=24000000,unbracketed_residual_ns=1000000,inclusive_ns=[1000000]*24,exclusive_ns=[1000000]*24,calls=[1]*24,parent_masks=[1<<24]*24,sweep_count=[16,0,0,0,0],sweep_ns=[1000,0,0,0,0],acoustics_world_sweep_count=16,acoustics_world_sweep_ns=1000,static_queries_total_hit1_hit4_hit16=[[6,0,0,0],[8,0,0,0],[2,0,0,0]],static_cache_hit_miss_eviction=[[5,1,0],[7,1,0],[1,1,0]],live_table_evictions=0))
   for row in data['rows']:row['sequence']+=1000*i
   remote[R+'frame_profile/'+entry['map']+'-fullbench-census-180.json']=(json.dumps(data)+'\n').encode()
  # Give B a >1 MiB benchmark delta but allow all eight arms to finish.
  heap=151*1048576 if entry['arm']=='B' else 149*1048576
  return '\n'.join(['bench: camera frozen map='+entry['map'],
   '[miniquad/vita] memory reason=benchmark newlib_uordblks='+str(heap),
   '[lg/audio-realtime] window_ms=1000 producer_blocks=1 producer_mix_p95_ms=1 pcm_starved_blocks=0 rejected_sinks=0 dropped_reports=0 queued_samples=100 producer_gap_max_ms=1 producer_sleep_overshoot_max_ms=1',
   '[lg/audio-music] decode_ms=1 underrun_frames=0 chunks=1',
   '[cpal/vita] late_callbacks=0 underrun_estimates=0 callbacks=1 callback_p95_us=1 callback_max_us=1',
   '[lg/frame-total] measured=120 gl_hooks=0 phase_timers=0 clock=begin-to-begin',
   '[miniquad/vita] memory reason=post_export newlib_uordblks='+str(heap+1048576),
   'bench: done 120 frames mode=fullbench profiler=light','Viewer closed cleanly.'])+'\n'
 def driver(name,args=None):
  assert active==0,'driver action with FTP open'
  p.owner();args=args or {};driver_calls.append(dict(name=name,args=args))
  if name=='vita_launch':
   assert current is not None and args==dict(title_id='ALICELG01')
   with log.open('ab') as out:out.write(startup(current).encode())
  elif name=='vita_press':
   if args['sequence'].startswith('press circle;'):
    with log.open('ab') as out:out.write(finish(current).encode())
  elif name=='vita_shot':return str(work/'fixture.png')
  elif name=='vita_cmd':assert args['raw']=='release all'
  else:assert name=='vita_quit'
  return 'OFFLINE_FAKE_OK'
 # Even imports cannot access the real FTP module. All other networking and
 # process execution is denied before any helper is imported.
 with contextlib.ExitStack() as stack:
  for obj,name in [(socket.socket,'connect'),(socket.socket,'connect_ex'),(socket.socket,'bind'),(socket,'create_connection'),(socket,'getaddrinfo'),(subprocess,'Popen'),(os,'system')]:stack.enter_context(patch.object(obj,name,deny))
  stack.enter_context(patch.object(os,'kill',lambda pid,sig: None if (pid,sig)==(4321,0) else deny()));
  stack.enter_context(patch.object(subprocess,'run',fake_run));stack.enter_context(patch.object(time,'sleep',lambda seconds:None));stack.enter_context(patch.object(time,'monotonic',monotonic))
  stack.enter_context(patch.dict(sys.modules,{'vita_ftp':types.SimpleNamespace(Session=Transport)}))
  stack.enter_context(patch.object(sys,'path',[str(root)]+sys.path))
  import pilot as p,session as session,run_health,census_report
  p.LOG=log;p.CACHE_INDEX=cache
  stack.enter_context(patch.object(p,'request',driver))
  # Tiny image bytes are only for exercising image acknowledgement bookkeeping.
  (work/'fixture.png').write_bytes(b'OFFLINE_FAKE_IMAGE')
  def call(name,*args):
   with patch.object(sys,'argv',[name,*map(str,args)]),contextlib.redirect_stdout(io.StringIO()):
    try:runpy.run_path(str(root/name if (root/name).exists() else package/'scripts'/name),run_name='__main__')
    except SystemExit as error:
     assert error.code in (None,0),(name,error.code)
  call('bind_helpers.py','--root',root,'--package',package)
  assert json.loads((root/'HELPER-BINDING.json').read_text())['status']=='PASS'
  call('verify-human-entry.py');call('preserve.py')
  receipt=json.loads((root/'SESSION.json').read_text())
  assert set(receipt['package_self_sha256'])=={'baseline','cache','cache-census'}
  assert receipt['engine_source_revision']==ENGINE and receipt['branch_head']==json.loads((package/'PACKAGE-IDENTITY.json').read_text())['branch_head']
  assert (root/'cache-index-before.json').read_bytes()==cache_before
  print('PASS preservation main: r6 identities, engine revision, branch HEAD and cache index')
  ready=work/'DRIVER-READY.json'
  write(ready,dict(pid=4321,started='2026-10-24T08:00:00+00:00',fixed_expiry='2026-10-24T20:00:00+00:00',lifetime_seconds=43200,auto_restart=False,ready=True))
  call('check-session-budget.py','--driver-ready',ready,'--admit',root/'DRIVER-ADMISSION.json','--session-start','2026-10-24T08:00:00+00:00','--now','2026-10-24T08:00:00+00:00','--launches',expected_launches,'--pandemonium-launches',5 if include_optional else 4)
  budget=load('offline_session_budget',root/'check-session-budget.py')
  result=budget.check(json.loads(ready.read_text()),budget.timestamp('2026-10-24T08:00:00+00:00'),budget.timestamp('2026-10-24T08:00:00+00:00'),launches=expected_launches,pandemonium_launches=5 if include_optional else 4)
  assert result['remaining_minutes']==(147 if include_optional else 127.5)
  print(f"PASS offline admission: {expected_launches} launches, {result['remaining_minutes']} minutes; fixed driver identity")
  call('baseline-r3.py');call('save-proof.py')
  session.stage_manifest(package/'stage-baseline-self.json',root/'self-stage-ledger.json')
  for entry in seq:
   label=entry['label'];current=entry
   remaining=seq[seq.index(entry):]
   call('check-session-budget.py','--driver-ready',ready,'--admitted',root/'DRIVER-ADMISSION.json','--session-start','2026-10-24T08:00:00+00:00','--now','2026-10-24T08:20:00+00:00','--self-stages','0','--preservation-minutes','0','--launches',len(remaining),'--pandemonium-launches',sum(e['map']=='pandemonium' for e in remaining))
   session.stage(label,entry['arm']);call('verify-arm-self.py',label)
   call('ui.py',label,'launch');call('ack.py','offline fixture image acknowledgement')
   if entry['map']=='pandemonium':
    call('ui.py',label,'load-auto');call('ack.py','offline selected Auto Load')
    p.rec('ENTRY-ACTIVATION.json',dict(source='OFFLINE_FAKE',entry='Load Save'))
   else:
    call('ui.py',label,'chapters','32');call('ack.py','offline Chapters 32')
    call('ui.py',label,'begin');call('ack.py','offline entry pose')
   call('arm-mode-only.py',label)
   call('ui.py',label,'resume','circle');call('ack.py','offline clean exit HOME')
   session.collect(label)
   call('log-integrity.py',label,'--root',root)
   call('entry-state.py',label)
   call('audit-run.py',label,'--root',root,'--package',package)
   call('strict-audio.py',label,'--root',root)
   for file in ['STAGE.json','SELF-IDENTITY.json','LAUNCH.json','BENCH-ARMED.json','COLLECT.json','LOG-INTEGRITY.json','ENTRY-STATE.json','AUDIT.json','STRICT-AUDIO.json']:assert (root/label/file).exists(),file
   audit=json.loads((root/label/'AUDIT.json').read_text())
   assert audit['heap_readout']['benchmark']['sampled_max_MiB']==(151 if entry['arm']=='B' else 149)
   assert audit['heap_readout']['whole_run']['sampled_max_MiB']==165
   # Prove late callbacks fail the actual strict-audio main even when delta=0.
   if label==seq[0]['label']:
    path=root/label/'runtime-log.txt';raw=path.read_bytes();path.write_bytes(raw.replace(b'late_callbacks=0',b'late_callbacks=1'))
    try:call('strict-audio.py',label,'--root',root)
    except AssertionError as error:assert error.args[0]==('strict-audio.py',2)
    else:raise AssertionError('nonzero CPAL late callback accepted')
    path.write_bytes(raw);call('strict-audio.py',label,'--root',root)
   call('restore-arm.py',label)
   remote[R+'saves/auto.previous.json']=b'fake modified companion requiring restoration'
   print('PASS full arm main path:',label)
   if label.endswith('-4-A'):call('compare-abba.py','--root',root,'--protocol',package/'CONSOLE-PROTOCOL.json','--block',entry['block'])
  call('make-report.py','--root',root,'--package',package,'--output',root/'RESULTS.json')
  result=json.loads((root/'RESULTS.json').read_text());md=(root/'RESULTS.md').read_text()
  assert len(result['maps'])==2
  assert result['status']=='NO_PROMOTION'
  assert [v['wall_gate']['status'] for v in result['maps']]==['REGRESSION','NO_CONFIRMED_BENEFIT']
  assert all(v['heap_windows'][0]['benchmark']['sampled_max_MiB']==149 for v in result['maps'])
  assert all(len(v['optional_census'])==(1 if include_optional else 0) for v in result['maps'])
  print('PASS both-map wall gate and benchmark heap report; census off in all gate runs')
  session.stage('human-restore','human-restore')
  final=json.loads((root/'RESTORE-FINAL.json').read_text())
  assert final['status']=='PASS_ORIGINAL_STATE_VERIFIED'
  assert remote==initial and cache.read_bytes()==cache_before
  assert len([v for v in stages if v.endswith('-self.json')])==1
  assert len([v for v in driver_calls if v['name']=='vita_launch'])==expected_launches
  assert maximum==1 and active==0
  assert all(json.loads(path.read_text()).get('restore_status')=='restored' for path in root.rglob('*ledger.json'))
  if evidence:
   evidence.mkdir(parents=True,exist_ok=True)
   for name in ['RESULTS.json','SESSION.json','RESTORE-FINAL.json']:
    value=json.loads((root/name).read_text());write(evidence/name,dict(synthetic_offline_only=True,real_device_evidence=False,receipt=value))
   (evidence/'RESULTS.md').write_text('# SYNTHETIC OFFLINE FIXTURE — NOT HARDWARE RESULTS\n\n'+md)
  print('PASS final restoration: PASS_ORIGINAL_STATE_VERIFIED; all ledgers, SELF, saves, configs, exports and inventories')
  return dict(status='PASS',synthetic_offline_only=True,real_transport_imported=False,device_contact=False,driver_contact=False,
   maps=sorted({e['map'] for e in seq}),labels=[e['label'] for e in seq],launches=expected_launches,self_stages=1,small_stages=len(stages)-1,
   serialized_fake_ftp_maximum=maximum,driver_fake_calls=len(driver_calls),fake_transfer_operations=len(transfers),
   final_status=final['status'],full_restoration=True,engine_source_revision=receipt['engine_source_revision'],branch_head=receipt['branch_head'])

def main():
 parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('package',type=Path);parser.add_argument('--include-optional',action='store_true');parser.add_argument('--sealed-helpers',type=Path);parser.add_argument('--output',type=Path,required=True);parser.add_argument('--evidence-dir',type=Path);args=parser.parse_args()
 result=None
 try:
  check_session_vacancy(args.package.resolve())
  with tempfile.TemporaryDirectory(prefix='alice-r6-e2e-') as directory:result=exercise(args.package.resolve(),Path(directory),args.sealed_helpers,args.evidence_dir,args.include_optional)
  result['real_package_session_path_absent']=True
 except Exception as error:
  result=dict(status='FAIL',synthetic_offline_only=True,device_contact=False,driver_contact=False,error_type=type(error).__name__,error=str(error))
  print('FAIL full session main path:',type(error).__name__+':',error)
  raise
 finally:
  if result:write(args.output,result)
 print(f"PASS full session main path: 2 maps, {result['launches']} labels, PASS_ORIGINAL_STATE_VERIFIED")
if __name__=='__main__':main()
