"""Run real r8 helper mains against in-memory transport and a synthetic game.

ALL sockets/processes are blocked before helper imports. This is never device
evidence. Uses locally retained human bytes only to prove exact restoration.
"""
from pathlib import Path
import argparse, contextlib, copy, hashlib, importlib.util, io, json, os, re, runpy, shutil, socket, subprocess, sys, tempfile, time, types, zipfile
from unittest.mock import patch
HUMAN_SELF=Path('/Users/anezium/vita-j0/resume-2026-10-22/alice-card-think/console-r4/restored-self-independent.bin')
HUMAN_SAVE=Path('/Users/anezium/vita-j0/resume-2026-10-24/alice-lever-matrix-r3/session-s5-world-static-merge/originals/data/alice/private/saves/auto.json')
def sha(b):return hashlib.sha256(b).hexdigest()
def write(p,v):p.parent.mkdir(parents=True,exist_ok=True);p.write_text(json.dumps(v,indent=2)+'\n')
def fresh_qualification():return dict(applied=0,recording=False,windows=0,exports_by_id={})
def poll_command(state,command,model):
    """One due poll. Fixed model: an already-applied id does not change state or export.
    Old model: every stop exports, including a repeated id. A per-frame caller is rejected separately."""
    if model not in ('old','fixed'):raise AssertionError('unknown recorder model '+model)
    stop=command.get('action')=='stop'
    cid=int(command['id']);stale=cid<=int(state['applied'])
    if stale:
        if model=='old' and stop:
            new=copy.deepcopy(state);new['exports_by_id'][cid]=new['exports_by_id'].get(cid,0)+1
            return new,True
        return state,False
    if command.get('action') not in ('start','stop'):raise AssertionError('unknown qualification action')
    new=copy.deepcopy(state);new['applied']=cid
    if command['action']=='start':
        if state['recording']:raise AssertionError('qualification already recording')
        new['recording']=True;new['windows']=state['windows']+1
        return new,False
    if not state['recording']:raise AssertionError('qualification is not recording')
    new['recording']=False;new['exports_by_id'][cid]=new['exports_by_id'].get(cid,0)+1
    return new,True
def menu_io_budget(model):
    """Menu frames between due polls must not touch the export. Each command id exports at most once."""
    state=fresh_qualification();writes=[];per_frame_io=0
    def dwell(command,due_polls,undue_frames):
        nonlocal state,per_frame_io
        for _ in range(due_polls):
            for _frame in range(undue_frames):
                if model=='per-frame' and command.get('action')=='stop':
                    per_frame_io+=1;writes.append(int(command['id']))
            use='old' if model=='per-frame' else model
            state,wrote=poll_command(state,command,use)
            if wrote:writes.append(int(command['id']))
    dwell(dict(id=0,action='stop',label='inactive'),3,8)
    dwell(dict(id=1,action='start',label='hedge3-move'),2,4)
    dwell(dict(id=2,action='stop',label='hedge3-move'),3,8)
    counts={}
    for cid in writes:counts[cid]=counts.get(cid,0)+1
    problems=[]
    if per_frame_io:problems.append(f'per-frame file I/O in menus across {per_frame_io} frames')
    for cid,n in sorted(counts.items()):
        if n>1:problems.append(f'repeated export for command id {cid}: {n}')
    if model=='fixed' and counts.get(0,0):problems.append('export of already-applied command id 0')
    if problems:raise AssertionError('; '.join(problems))
    return dict(writes=writes,counts=counts,per_frame_io=per_frame_io,applied=state['applied'],recording=state['recording'])
def deny(*a,**kw):raise AssertionError('OFFLINE BOUNDARY: real process/network denied')
def exercise(source,work,replay):
    package=work/'package';package.mkdir()
    if replay:
        with zipfile.ZipFile(source/'CONSOLE-PACKAGE.zip') as z:z.extractall(package)
    else:
        for name in ['console','configs','packages','scripts']:shutil.copytree(source/name,package/name)
        for name in ['CONSOLE-PROTOCOL.json','PACKAGE-IDENTITY.json']:shutil.copyfile(source/name,package/name)
        for f in source.glob('stage-*.json'):shutil.copyfile(f,package/f.name)
    root=package/'session-s8-cache-qual';root.mkdir()
    for f in (package/'console').glob('*.py'):shutil.copyfile(f,root/f.name)
    R='ux0:/data/alice/private/';A='ux0:/app/ALICELG01/'
    human=HUMAN_SELF.read_bytes();save=HUMAN_SAVE.read_bytes()
    assert len(human)==37371674 and sha(human)=='387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990'
    assert len(save)==181900 and sha(save)=='ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9'
    remote={A+'eboot.bin':human,A+'sce_sys/param.sfo':b'fixture SFO',R+'saves/auto.json':save,
        R+'saves/auto.previous.json':b'fixture companion',R+'route-saves/human.json':b'fixture route save',
        R+'vita_renderer.json':b'original renderer',R+'vita_sim.json':b'original simulation',
        R+'native-timing-config.json':b'original timing',R+'profile_mode':b'full\n',
        R+'preferences.json':b'original preferences',R+'cache_qual_command.json':b'original command',
        R+'frame_profile/cache-qual.json':b'prior qualification export',
        R+'frame_profile/pandemonium-fullbench-census-180.json':b'prior census bytes'}
    initial=dict(remote);directories={R,A,A+'sce_sys/',R+'saves/',R+'route-saves/',R+'frame_profile/'}
    log=work/'vita.log';log.write_bytes(b'OFFLINE SYNTHETIC\n');cache=work/'index.json';cache.write_bytes(b'{"original":true}\n');cache_before=cache.read_bytes()
    active=maximum=0;backups={};stages=[];driver_calls=[];ops=[];current=None;clock=0.;us=0;scene=10;mapname='gvillage';warm=False;data=None;pending=None;next_button=None
    ui='closed';resumed=False;last_eye=0.;negative_results=[];closure_fixtures=[]
    applied_id=0;recording=False;process_exports={};final_exports=0
    for model in ('old','per-frame'):
        try:menu_io_budget(model)
        except AssertionError as error:negative_results.append(dict(fixture='recorder '+model+' menu reexport',status='REJECTED_AS_INTENDED',reason=str(error)))
        else:raise AssertionError('recorder model accepted: '+model)
    fixed_budget=menu_io_budget('fixed')
    assert fixed_budget['counts']=={2:1} and fixed_budget['per_frame_io']==0 and fixed_budget['applied']==2 and fixed_budget['recording'] is False,fixed_budget
    seq=json.loads((package/'CONSOLE-PROTOCOL.json').read_text())['sequence']
    def append(line):
        with log.open('a') as f:f.write(line+'\n')
    def listing(d):
        rows={}
        for name in directories:
            tail=name.removeprefix(d).rstrip('/')
            if name.startswith(d) and tail and '/' not in tail:rows[tail]=dict(bytes=0,directory=True)
        for name,b in remote.items():
            if name.startswith(d) and '/' not in name[len(d):]:rows[name[len(d):]]=dict(bytes=len(b),directory=False)
        return rows
    def snapshot_export():remote[R+'frame_profile/cache-qual.json']=(json.dumps(data)+'\n').encode()
    def event(kind,**v):data['events'].append(dict(kind=kind,us=us,heap=(185 if kind in ['transition','load'] or v.get('reason')=='audio-load' else 150)*1048576,**v))
    def load_map(to,kind=None):
        nonlocal us,scene,mapname,warm,ui,last_eye
        if kind:
            event('transition',operation=kind,**{'from':mapname,'to':to},stage='before-drop');us+=10
            event('transition',operation=kind,**{'from':mapname,'to':to},stage='after-drop');us+=10
            append('[lg/transition-memory] '+json.dumps(dict(kind=kind,**{'from':mapname,'to':to})))
        if current['arm']=='B':event('reset',reason='audio-load',old_scene=scene,entries_before=512 if warm else 0,entries_after=0,pin_after=False,old_refs_before=1)
        scene+=1;mapname=to;warm=False;us+=10
        ui='overlay' if kind=='save-reload' else 'running';last_eye=0.
        event('load',map=to,cache_on=current['arm']=='B');append('[lg/audio-load] level='+to+' elapsed_ms=1.000');us+=10
        # Exercise saves created by BOTH a transition and a later quit.
        if kind:
            remote[R+'saves/transition-created.json']=b'synthetic transition save'
            remote[R+'saves/auto.previous.json']=b'synthetic changed companion'
            remote[R+'frame_profile/hedge3-fullbench-census-180.json']=b'synthetic new allowed census export'
    def reporters():
        return ['[lg/audio-realtime] window_ms=1000 producer_blocks=80 producer_mix_p95_ms=1 pcm_starved_blocks=0 rejected_sinks=0 dropped_reports=0 queued_samples=100 producer_gap_max_ms=1 producer_sleep_overshoot_max_ms=1',
            '[lg/audio-music] decode_ms=1 underrun_frames=0 chunks=1',
            '[cpal/vita] late_callbacks=1 underrun_estimates=0 callbacks=88 callback_p95_us=1 callback_max_us=1']
    def finish_window(c):
        nonlocal us,warm,last_eye
        assert pending and c['label']==pending['label']
        moving=c['label'].endswith('-move');count=1281 if moving else 101
        rows=[];origin=0. if moving else last_eye
        for i in range(count):
            cached=current['arm']=='B';counts=[[3,3,0],[2,1,0],[7,1,0]] if cached else [[0,6,0],[0,3,0],[0,8,0]]
            rows.append(dict(us=us+i*50000,prepare_ns=2500000,heap=150*1048576+(32000 if cached else 0),scene=scene,
                eye=[float(i)*(.1+(25./1280 if cached else 0)) if moving else origin,0.,0.],queries=[6,3,8],cache=counts,
                checks=int(cached and pending['check_stride']==64),mismatches=0))
        if not resumed:rows=[]
        if rows:last_eye=rows[-1]['eye'][0]
        us=(rows[-1]['us'] if rows else us+5_000_000)+10000
        data['windows'].append(dict(id=pending['id'],label=c['label'],map=mapname,check_stride=pending['check_stride'],rows=rows,overflow=False,end_us=us))
        for line in reporters():append(line)
        append(f"[lg/cache-qual] stop id={c['id']} label={c['label']}")
        snapshot_export();process_exports[c['id']]=process_exports.get(c['id'],0)+1
        if process_exports[c['id']]>1:raise AssertionError(f"repeated export for command id {c['id']}")
        warm=True;us+=10000
    class Transport:
        def __init__(self,*a,**kw):self.ftp=self
        def __enter__(self):
            nonlocal active,maximum
            assert active==0,'parallel FTP';active+=1;maximum=max(maximum,active);return self
        def __exit__(self,*a):
            nonlocal active
            active-=1
        def record(self,*a,**kw):pass
        def voidcmd(self,c):assert c=='TYPE I'
        def execute(self,op,path,local=None):
            nonlocal pending,applied_id,recording
            assert active==1;ops.append((op,path))
            if op=='list':return '\n'.join(('d' if v['directory'] else '-')+'rwxr-xr-x 1 vita vita '+str(v['bytes'])+' Oct 24 00:00 '+n for n,v in listing(path).items())
            if op=='size':return len(remote[path])
            if op=='get':Path(local).parent.mkdir(parents=True,exist_ok=True);Path(local).write_bytes(remote[path]);return
            if op=='put':
                remote[path]=Path(local).read_bytes()
                if path==R+'cache_qual_command.json':
                    try:c=json.loads(remote[path])
                    except json.JSONDecodeError:return
                    # The file is only stored here. A repeated id is the engine no-op:
                    # no second window and no second export. Id 0 is the inert stop.
                    if c.get('id') and c['id']>applied_id:
                        if c['action']=='start':
                            pending=c;applied_id=c['id'];recording=True
                            append(f"[lg/cache-qual] start id={c['id']} label={c['label']} check_stride={c['check_stride']}")
                        else:
                            finish_window(c);applied_id=c['id'];recording=False
                return
            raise AssertionError(('unallowed FTP operation',op))
        def delete(self,path):assert active==1;ops.append(('delete',path));del remote[path]
    def fake_run(argv,**kw):
        assert isinstance(argv,list)
        if argv[:2]==['/bin/ps','-p']:return subprocess.CompletedProcess(argv,0,'python terminal-driver.py\n','')
        if argv[0]=='/sbin/route':return subprocess.CompletedProcess(argv,0,'interface: en0\n','')
        if argv[0]=='/usr/sbin/lsof':return subprocess.CompletedProcess(argv,0,'p123\n' if '-iTCP:9999' in argv else 'n/Users/anezium/vita-j0/work/vita.log\n','')
        if Path(argv[1]).name in ['restore-ledgers.py','verify-self.py','restore-files.py']:
            call(Path(argv[1]).name);return subprocess.CompletedProcess(argv,0)
        assert argv[1]=='/Users/anezium/vita-j0/bin/vita-stage' and active==0
        if '--restore' in argv:
            ledger=Path(argv[argv.index('--restore')+1]);d=json.loads(ledger.read_text())
            for path,b in backups[str(ledger.resolve())].items():
                if b is None:remote.pop(path,None)
                else:remote[path]=b
            d['restore_status']='restored';write(ledger,d)
        else:
            manifest=Path(argv[argv.index('--manifest')+1]);ledger=Path(argv[argv.index('--ledger')+1]);m=json.loads(manifest.read_text());old={};entries=[]
            for v in m['files']:
                path=v['remote'];old[path]=remote.get(path);b=old[path]
                entries.append(dict(remote=path,original=dict(size=len(b),sha256=sha(b)) if b is not None else dict(present=False)))
                local=Path(v['local']);remote[path]=(local if local.is_absolute() else manifest.parent/local).read_bytes()
            backups[str(ledger.resolve())]=old;write(ledger,dict(status='PASS',entries=entries,parts=[],cache_created=[]));stages.append(manifest.name)
            cache.write_bytes(b'{"synthetic_stager_mutation":true}\n')
        return subprocess.CompletedProcess(argv,0)
    def driver(name,args=None):
        nonlocal data,us,scene,mapname,warm,next_button,ui,resumed,clock,applied_id,recording,process_exports,final_exports
        assert active==0,'driver with FTP open';p.owner();args=args or {};driver_calls.append((name,args))
        if name=='vita_launch':
            applied_id=0;recording=False;process_exports={}
            data=dict(schema=1,capacity=8192,query_classes=['room','ambient','effects'],windows=[],events=[]);us=100;scene=10;mapname='gvillage';warm=False
            cfg=json.loads(remote[R+'vita_renderer.json']);append('[lg/vita-renderer] config '+json.dumps(cfg,separators=(',',':')))
            load_map('gvillage')
        elif name=='vita_cmd' and args['raw']=='quit ALICELG01':
            reply='Error: cannot quit the app. Is the TITLEID correct?' if ui=='closed' else 'Quit.'
            ui='closed'  # Process kill creates NO autosave or clean-exit log.
            return reply
        elif name=='vita_quit':ui='closed'
        elif name=='vita_press' and next_button:
            kind,to=next_button;next_button=None
            if kind=='quit':
                remote[R+'saves/auto.json']=b'synthetic quit autosave';remote[R+'saves/quit-created.json']=b'synthetic quit companion'
                append('Viewer closed cleanly. Data folder: OFFLINE')
                if current['arm']=='B':event('reset',reason='audio-finish',old_scene=scene,entries_before=512,entries_after=0,pin_after=False,old_refs_before=2)
                snapshot_export();final_exports+=1
                append('[lg/cache-qual] exported windows=6 events='+str(len(data['events'])))
                ui='closed'
            else:load_map(to,kind)
        elif name=='vita_press':
            sequence=args['sequence']
            if sequence.startswith('press start;'):ui='running' if ui=='modal' else 'modal'
            elif sequence.startswith('press circle;'):
                if ui=='modal':ui='running';resumed=True
                # Circle in the non-modal overlay does nothing.
            if sequence.startswith('press left-stick '):clock+=2.02
        elif name=='vita_shot':return str(work/'fixture.png')
        else:assert name in ['vita_press','vita_cmd','vita_quit']
        return 'OFFLINE_FAKE_OK'
    def dwell_live(name):
        raw=remote.get(R+'cache_qual_command.json',b'')
        try:command=json.loads(raw)
        except (json.JSONDecodeError,UnicodeDecodeError,TypeError):return
        if not isinstance(command,dict) or 'id' not in command or 'action' not in command:return
        state=dict(applied=applied_id,recording=recording,windows=0,exports_by_id={})
        before=(applied_id,recording)
        for _ in range(3):
            for _frame in range(8):
                pass
            state,wrote=poll_command(state,command,'fixed')
            if wrote or (state['applied'],state['recording'])!=before:
                raise AssertionError(f'repeated export or per-frame file I/O in menu {name} for command id {command["id"]}')
    def monotonic():
        nonlocal clock
        clock+=.25;return clock
    # Deny real transport and subprocesses BEFORE importing any console helper.
    with contextlib.ExitStack() as stack:
        for obj,name in [(socket.socket,'connect'),(socket.socket,'connect_ex'),(socket.socket,'bind'),(socket,'create_connection'),(socket,'getaddrinfo'),(subprocess,'Popen'),(os,'system')]:stack.enter_context(patch.object(obj,name,deny))
        stack.enter_context(patch.object(subprocess,'run',fake_run));stack.enter_context(patch.object(os,'kill',lambda pid,sig:None if (pid,sig)==(4321,0) else deny()))
        stack.enter_context(patch.object(time,'sleep',lambda n:None));stack.enter_context(patch.object(time,'monotonic',monotonic))
        stack.enter_context(patch.object(time,'monotonic_ns',lambda:int(clock*1_000_000_000)))
        stack.enter_context(patch.dict(sys.modules,{'vita_ftp':types.SimpleNamespace(Session=Transport)}));stack.enter_context(patch.object(sys,'path',[str(root)]+sys.path))
        import pilot as p, session, audit, run_health, operate
        p.LOG=log;p.CACHE_INDEX=cache;real_request=p.request;stack.enter_context(patch.object(p,'request',driver))
        (work/'fixture.png').write_bytes(b'OFFLINE_FAKE_IMAGE')
        def call(name,*args):
            with patch.object(sys,'argv',[name,*map(str,args)]),contextlib.redirect_stdout(io.StringIO()):
                try:runpy.run_path(str(root/name if (root/name).exists() else package/'scripts'/name),run_name='__main__')
                except SystemExit as e:assert e.code in (None,0),(name,e.code)
        def reject(name,action,contains=None):
            try:action()
            except (AssertionError,FileNotFoundError,ConnectionError) as error:
                if contains:assert contains in str(error),(name,str(error))
                negative_results.append(dict(fixture=name,status='REJECTED_AS_INTENDED',reason=str(error)))
            else:raise AssertionError('negative fixture accepted: '+name)
        def closure(label):
            call('operate.py',label,'prove-closed');call('ack.py','--home','OFFLINE HOME inspected; title closed')
        def reversed_operations(d):
            reload=next(e for e in d['events'] if e.get('operation')=='save-reload' and e.get('from')==e.get('to'))
            cross=next(e for e in d['events'] if e.get('operation')=='save-reload' and e.get('from')!=e.get('to') and e['stage']=='before-drop')
            cross['us']=reload['us']+1
        call('bind_helpers.py','--root',root,'--package',package)
        ready=work/'driver-ready.json';write(ready,dict(pid=4321,started='2026-10-24T08:00:00+00:00',fixed_expiry='2026-10-24T20:00:00+00:00',lifetime_seconds=43200,auto_restart=False,ready=True))
        call('check-session-budget.py','--driver-ready',ready,'--admit',root/'DRIVER-ADMISSION.json','--session-start','2026-10-24T08:00:00+00:00','--now','2026-10-24T08:00:00+00:00','--launches','2','--pandemonium-launches','2','--preservation-minutes','64.5')
        call('admit.py','--sole-console-owner');call('verify-human-entry.py');call('preserve.py');call('baseline-r3.py');call('save-proof.py')
        # Exercise real cmd/request error handling and prove-closed/ack mains,
        # with only the subprocess response boundary replaced. Unknown command
        # must remain an error; only the exact cannot-quit reply is permitted.
        cannot='Error: cannot quit the app. Is the TITLEID correct?'
        for name,replies,accepted in [
            ('Quit then error',['Quit.\n',cannot+'\n'],True),
            ('error then error',[cannot,cannot],True),
            ('Quit then Quit',['Quit.','Quit.'],False),
            ('Unknown command',['Error: Unknown command.'],False),
            ('Unknown command second',['Quit.','Error: Unknown command.'],False),
            ('ambiguous reply',['OFFLINE_FAKE_OK'],False),
            ('other error',['Error: other failure'],False)]:
            calls=[];before_remote=dict(remote);before_ops=list(ops)
            def response(argv,**kwargs):
                assert argv[-2]=='vita_cmd' and json.loads(argv[-1])==dict(raw='quit ALICELG01')
                raw=replies[len(calls)];calls.append(raw)
                return json.dumps(dict(result=dict(structuredContent=dict(result=raw))))
            (root/'TITLE-CLOSED.json').write_text('{}')  # stale proof must be invalidated
            def closure_request(name,args=None):
                return real_request(name,args) if name=='vita_cmd' else driver(name,args)
            with patch.object(p,'request',closure_request),patch.object(subprocess,'check_output',response):
                action=lambda:call('operate.py',seq[0]['label'],'prove-closed')
                if accepted:
                    action()
                    assert not (root/'TITLE-CLOSED.json').exists()
                    reject('closure before HOME acknowledgement '+name,lambda:call('session.py','restore'))
                    call('ack.py','--home','OFFLINE HOME/LiveArea inspected; no running Alice')
                    receipt=json.loads((root/'TITLE-CLOSED.json').read_text())
                    assert receipt['closure']['replies']==replies
                else:
                    reject('closure '+name,action)
                    assert not (root/'TITLE-CLOSED.json').exists() and not (root/'PENDING-IMAGE.json').exists()
                    failed=json.loads((root/'QUIT-CLOSURE.json').read_text())
                    assert failed['status']=='UNPROVEN' and failed['failure']
                    reject('restoration after unproven '+name,lambda:call('session.py','restore'))
            assert calls==replies and remote==before_remote and ops==before_ops
            closure_fixtures.append(dict(fixture=name,replies=replies,status='PROVEN_AFTER_HOME_ACK' if accepted else 'UNPROVEN_NO_WRITES',real_request_wrapper=True))
            (root/'TITLE-CLOSED.json').unlink(missing_ok=True)
        # The allowed error must not leak into other driver commands.
        with patch.object(subprocess,'check_output',return_value=json.dumps(dict(result=dict(structuredContent=dict(result=cannot))))):
            reject('cannot-quit error on unrelated command',lambda:real_request('vita_cmd',dict(raw='version')))
        closure(seq[0]['label'])
        # The synthetic process kill models a running app and preserves every
        # remote byte, including Auto, companions and exports.
        ui='running';before_kill=dict(remote);closure(seq[0]['label'])
        assert ui=='closed' and remote==before_kill
        session.stage_manifest(package/'stage-baseline-self.json',root/'self-stage-ledger.json')
        for current in seq:
            number=seq.index(current)
            call('check-session-budget.py','--driver-ready',ready,'--admitted',root/'DRIVER-ADMISSION.json',
                '--session-start','2026-10-24T08:00:00+00:00','--now',f'2026-10-24T08:{20+number*20:02d}:00+00:00',
                '--launches',str(2-number),'--self-stages','0','--pandemonium-launches',str(2-number),'--preservation-minutes',str(44.5-number*20))
            label=current['label'];call('session.py','stage',label);call('verify-arm-self.py',label)
            stale_home=(root/'TITLE-CLOSED.json').read_bytes()
            call('operate.py',label,'launch');call('ack.py','OFFLINE synthetic launch image');dwell_live('title')
            # A running title, even with a stale HOME receipt, cannot be restored.
            before_writes=list(ops)
            reject('restore while running',lambda:call('session.py','restore'))
            (root/'TITLE-CLOSED.json').write_bytes(stale_home)
            for helper,args in [('session.py',['restore']),('restore-ledgers.py',[]),('restore-files.py',[]),('restore-arm.py',[label])]:
                ui='running';(root/'TITLE-CLOSED.json').write_bytes(stale_home)
                reject('stale HOME with running title '+helper,lambda:call(helper,*args),contains='still running')
                assert not (root/'TITLE-CLOSED.json').exists()
            ui='running';(root/'TITLE-CLOSED.json').write_bytes(stale_home)
            write(root/'STOP.json',dict(reason='OFFLINE deliberate stop'))
            reject('save restoration after stop while running',lambda:call('session.py','saves','stopped-negative'),contains='still running')
            (root/'STOP.json').unlink();(root/'TITLE-CLOSED.json').unlink(missing_ok=True);ui='running'
            assert ops==before_writes and not (root/'RESTORATION-STARTED.json').exists()
            next_button=('chapter-or-console','hedge3');call('operate.py',label,'button','cross');call('ack.py','OFFLINE synthetic chapter entry')
            for label_window,action in [('hedge3-move','move'),('hedge3-check','check'),('pandemonium-move','move'),('pandemonium-check','check'),('reload-post','post'),('transition-post','post')]:
                if label_window in ['pandemonium-move','reload-post','transition-post']:
                    call('session.py','saves','before-'+label+'-'+label_window)
                    next_button=('chapter-or-console' if label_window=='transition-post' else 'save-reload','hedge3' if label_window=='transition-post' else 'pandemonium')
                    call('operate.py',label,'button','cross');dwell_live('load-save-'+label_window)
                    reject('load image is not modal Main '+label_window,lambda:call('ack.py','--main-menu','OFFLINE overlay must not qualify'),contains='Not a Start-menu image')
                    call('ack.py','OFFLINE synthetic native load confirmation')
                if not label_window.endswith('-check'):
                    reject('window without inspected modal '+label_window,lambda:call('operate.py',label,action,label_window,'circle'))
                    call('operate.py',label,'button','start');call('ack.py','--main-menu','OFFLINE Main menu; Circle Back/Resume visible');dwell_live('main-'+label_window)
                resumed=False
                call('operate.py',label,action,label_window,'circle');call('ack.py','--main-menu','OFFLINE stopped Main menu; Circle Back/Resume visible');dwell_live('stopped-'+label_window)
            dwell_live('quit')
            next_button=('quit',None);call('operate.py',label,'button','cross');call('ack.py','OFFLINE synthetic quit')
            call('operate.py',label,'wait-exit');call('ack.py','OFFLINE synthetic HOME')
            closure(label)
            call('session.py','collect',label);call('log-integrity.py',label,'--root',root);call('audit.py',label)
            checked=json.loads((root/label/'AUDIT.json').read_text());assert checked['status']=='PASS'
            assert checked['audio_session']['BENCHMARK_LATE_CALLBACKS']['total']==6
            # Deliberate failures verify fixed rules, independently of restoration.
            raw=(root/label/'runtime-log.txt').read_text();fixture=json.loads(remote[R+'frame_profile/cache-qual.json'])
            for name,mutate in [('underrun',lambda d,t:(d,t.replace('underrun_estimates=0','underrun_estimates=1'))),
                ('OOM',lambda d,t:(d,t+'out of memory\n')),
                ('producer p95 8 ms',lambda d,t:(d,t.replace('producer_mix_p95_ms=1','producer_mix_p95_ms=8'))),
                ('missing counter',lambda d,t:(d,t.replace('producer_blocks=80 ','').replace('callbacks=88 ',''))),
                ('short motion',lambda d,t:(dict(d,windows=[dict(d['windows'][0],rows=d['windows'][0]['rows'][:100])]+d['windows'][1:]),t)),
                ('heap',lambda d,t:(dict(d,windows=[dict(d['windows'][0],rows=[dict(d['windows'][0]['rows'][0],heap=181*1048576)]+d['windows'][0]['rows'][1:])]+d['windows'][1:]),t))]:
                bad,txt=mutate(copy.deepcopy(fixture),raw)
                try:audit.validate(bad,txt,current['arm'])
                except AssertionError:pass
                else:raise AssertionError('bad '+name+' accepted')
                negative_results.append(dict(fixture=name,arm=current['arm'],status='REJECTED_AS_INTENDED'))
            for name,field in [('load starvation remains gated','pcm_starved_blocks=1'),('load producer p95 8 ms remains gated','producer_mix_p95_ms=8')]:
                reporter=reporters()[0]
                key=field.split('=')[0]
                reporter=re.sub(r'\b'+key+r'=\S+',field,reporter)
                txt=raw.replace('[lg/audio-load] level=pandemonium',reporter+'\n[lg/audio-load] level=pandemonium',1)
                reject(name,lambda:audit.validate(fixture,txt,current['arm']))
            for name,mutate in [
                ('missing native reload',lambda d:[e.update(operation='restore') for e in d['events'] if e.get('operation')=='save-reload' and e.get('from')==e.get('to')]),
                ('wrong operation order',reversed_operations),
                ('post check stride disabled',lambda d:d['windows'][4].update(check_stride=0)),
                ('heap failure flag',lambda d:d.update(heap_failed=True))]:
                bad=copy.deepcopy(fixture);mutate(bad)
                reject(name,lambda:audit.validate(bad,raw,current['arm']))
            # Model the actual overlay behavior and exercise the real command,
            # Circle resume and stop path with only the entry guard bypassed.
            saved=(copy.deepcopy(data),pending,us,warm,last_eye,ui)
            ui='overlay';resumed=False;p.B=root/label
            operate.command('overlay-negative','start',0);p.button('circle')
            operate.command('overlay-negative','stop',0)
            assert data['windows'][-1]['rows']==[] and ui=='overlay'
            bad=copy.deepcopy(fixture);bad['windows'][0]['rows']=data['windows'][-1]['rows']
            reject('non-modal overlay Circle gives empty window',lambda:audit.validate(bad,raw,current['arm']))
            data,pending,us,warm,last_eye,ui=saved;snapshot_export()
            with patch.object(p,'cmd',side_effect=ConnectionError('OFFLINE fixture driver lost')):
                count_writes=len([o for o in ops if o[0] in ['put','delete']])
                reject('driver lost before restoration',lambda:call('session.py','restore'))
                assert len([o for o in ops if o[0] in ['put','delete']])==count_writes
            assert not (root/'TITLE-CLOSED.json').exists()
            closure(label)
            # Each setting is checked before any save put/delete, including absence.
            for setting in ['preferences.json','audio-settings.txt']:
                path=R+setting;previous=remote.get(path);remote[path]=b'fixture settings drift'
                count_writes=len([o for o in ops if o[0] in ['put','delete']])
                reject('settings drift '+setting,lambda:call('session.py','saves','settings-negative'),contains='Settings drift')
                reject('stage settings drift '+setting,lambda:call('session.py','stage',label),contains='Settings drift')
                assert len([o for o in ops if o[0] in ['put','delete']])==count_writes
                if previous is None:remote.pop(path)
                else:remote[path]=previous
            if current['arm']=='B':
                for key,value in [('mismatches',1)]:
                    bad=copy.deepcopy(fixture);bad['windows'][1]['rows'][0][key]=value
                    try:audit.validate(bad,raw,'B')
                    except AssertionError:pass
                    else:raise AssertionError('mismatch accepted')
                negative_results.append(dict(fixture='mismatch',status='REJECTED_AS_INTENDED'))
                bad=copy.deepcopy(fixture);next(e for e in bad['events'] if e['kind']=='reset')['entries_after']=1
                try:audit.validate(bad,raw,'B')
                except AssertionError:pass
                else:raise AssertionError('uncleared reset accepted')
                negative_results.append(dict(fixture='uncleared reset',status='REJECTED_AS_INTENDED'))
                bad=copy.deepcopy(fixture)
                for row in bad['windows'][1]['rows']:row['checks']=0
                try:audit.validate(bad,raw,'B')
                except AssertionError:pass
                else:raise AssertionError('zero sampled hits accepted')
                negative_results.append(dict(fixture='zero sampled hits',status='REJECTED_AS_INTENDED'))
                for index in [4,5]:
                    bad=copy.deepcopy(fixture)
                    for row in bad['windows'][index]['rows']:row['checks']=0
                    reject('post-load zero sampled hits '+bad['windows'][index]['label'],lambda:audit.validate(bad,raw,'B'))
            # A failed real audit main retains pulse diagnostics for a near-miss.
            collect_path=root/label/'COLLECT.json';collected=collect_path.read_bytes()
            receipt=json.loads(collected);export=Path(receipt['files'][0]['local']);good_export=export.read_bytes()
            short=copy.deepcopy(fixture);short['windows'][0]['rows']=short['windows'][0]['rows'][:100]
            export.write_text(json.dumps(short));receipt['files'][0]['sha256']=sha(export.read_bytes());write(collect_path,receipt)
            reject('failed motion audit preserves per-pulse diagnostics',lambda:call('audit.py',label),contains='less than 60')
            failed=json.loads((root/label/'AUDIT.json').read_text())
            assert failed['status']=='FAIL' and len(failed['per_pulse_motion']['hedge3-move']['pulses'])==32
            export.write_bytes(good_export);collect_path.write_bytes(collected);call('audit.py',label)
            for corruption in [raw+'orphan [lg/audio] tag\n',raw+' field=orphan\n']:
                (root/label/'runtime-log.txt').write_text(corruption)
                try:call('log-integrity.py',label,'--root',root)
                except AssertionError:pass
                else:raise AssertionError('corrupt log accepted')
                negative_results.append(dict(fixture='corrupt log',status='REJECTED_AS_INTENDED'))
            (root/label/'runtime-log.txt').write_text(raw);call('log-integrity.py',label,'--root',root)
            call('restore-arm.py',label)
        call('audit.py','report');assert json.loads((root/'RESULTS.json').read_text())['default_on'] is False
        bpath=root/seq[1]['label']/'AUDIT.json';boriginal=bpath.read_bytes();altered=json.loads(boriginal)
        abase=json.loads((root/seq[0]['label']/'AUDIT.json').read_text())
        assert math_distance(abase['windows'][1]['first_eye'],altered['windows'][1]['first_eye'])==25
        altered['windows'][0]['first_eye'][0]+=2
        write(bpath,altered);reject('entry pose >1 unit',lambda:call('audit.py','report'),contains='pose mismatch')
        altered=json.loads(boriginal)
        altered['windows'][0]['heap_bytes']['maximum']=abase['windows'][0]['heap_bytes']['maximum']+1048577
        write(bpath,altered)
        try:call('audit.py','report')
        except AssertionError:pass
        else:raise AssertionError('paired heap above +1 MiB accepted')
        negative_results.append(dict(fixture='paired heap >1MiB',status='REJECTED_AS_INTENDED'))
        altered['runtime_audio_memory']=['[lg/audio-memory] event=runtime:fixture cache_bytes=10']
        write(bpath,altered);call('audit.py','report')
        assert json.loads((root/'RESULTS.json').read_text())['status']=='UNKNOWN_MATCHED_PAIR_REQUIRED'
        altered=json.loads(boriginal)
        altered['runtime_audio_memory']=['[lg/audio-memory] event=runtime:fixture cache_bytes=10']
        for event in altered['events']:
            if event['kind']=='transition':event['heap']+=1048577;break
        write(bpath,altered);reject('transition paired heap strict',lambda:call('audit.py','report'))
        altered=json.loads(boriginal)
        altered['windows'][0]['prepare_acoustics_ms']['mean']+=.25
        write(bpath,altered);call('audit.py','report')
        assert json.loads((root/'RESULTS.json').read_text())['default_on_performance_eligible'] is True
        altered=json.loads(boriginal)
        altered['windows'][0]['prepare_acoustics_ms']['mean']+=.251
        write(bpath,altered);call('audit.py','report')
        assert json.loads((root/'RESULTS.json').read_text())['default_on_performance_eligible'] is False
        altered=json.loads(boriginal)
        altered['windows'][0]['heap_bytes']['maximum']=abase['windows'][0]['heap_bytes']['maximum']+1048576
        write(bpath,altered);call('audit.py','report')
        bpath.write_bytes(boriginal);call('audit.py','report')
        call('session.py','restore')
        reject('in-game Quit after restoration starts',lambda:call('operate.py',seq[-1]['label'],'button','cross'),contains='Restoration started')
        final=json.loads((root/'RESTORE-FINAL.json').read_text());assert final['status']=='PASS_ORIGINAL_STATE_VERIFIED'
        assert final['human_auto']==dict(bytes=181900,sha256=sha(save),independent_readbacks=2)
        assert final['human_self']==dict(bytes=37371674,sha256=sha(human)) and final['title_closed']
        with patch.object(__import__('map_identity'),'HUMAN_AUTO','0'*64):
            reject('final Auto literal mismatch',lambda:call('restore-files.py'),contains='Auto literal mismatch')
        call('session.py','restore')
        assert remote==initial and cache.read_bytes()==cache_before and maximum==1 and active==0
        assert stages.count('stage-baseline-self.json')==1 and len([v for v in driver_calls if v[0]=='vita_launch'])==2
        assert final_exports==2 and all(n==1 for n in process_exports.values())
        assert all(json.loads(f.read_text()).get('restore_status')=='restored' for f in root.rglob('*ledger.json'))
        assert any('transition-created.json' in path and op=='get' for op,path in ops)
        assert any('quit-created.json' in path and op=='get' for op,path in ops)
        # Original census bytes and a prior qualification export were restored;
        # initial qualification tmp absence and new transition/quit saves match.
        return dict(status='PASS',synthetic_offline_only=True,device_contact=False,driver_contact=False,real_transport_imported=False,
            final_status=final['status'],full_restoration=True,launches=2,windows_per_arm=6,self_stages=1,
            serialized_fake_ftp_maximum=maximum,driver_fake_calls=len(driver_calls),late_callbacks_report_only=True,
            negative_fixtures=negative_results,closure_fixtures=closure_fixtures,process_kill_no_autosave=True,real_package_session_path_absent=True,
            positive_rules=['185 MiB transition/load events descriptive; gameplay remains gated',
                '25-unit movement endpoint/check pose difference accepted',
                'reload-post and transition-post stride 64 with sampled B hits',
                'per-pulse diagnostics retained on failed motion audit',
                'runtime audio memory drift plus movement heap excess yields UNKNOWN',
                'transition/load paired heap remains strict with runtime drift',
                '+0.25 ms mean permits performance eligibility; +0.251 ms excludes it',
                'RESTORE-FINAL restates exact human Auto and SELF literals'],
            helper_main_path=True,zip_replay=replay,tested_zip_sha256=sha((source/'CONSOLE-PACKAGE.zip').read_bytes()) if replay else None,
            created_saves_preserved_before_deletion=True)
def math_distance(a,b):
    import math
    return math.dist(a,b)
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('package',type=Path);p.add_argument('--output',type=Path,required=True);p.add_argument('--zip',action='store_true');a=p.parse_args()
    assert not os.path.lexists(a.package/'session-s8-cache-qual'),'fresh real session path must be absent'
    assert not a.output.resolve().is_relative_to(a.package.resolve()),'test output must be outside package'
    try:
        with tempfile.TemporaryDirectory(prefix='alice-r8-e2e-') as tmp:result=exercise(a.package.resolve(),Path(tmp),a.zip)
    except Exception as e:
        write(a.output,dict(status='FAIL',synthetic_offline_only=True,device_contact=False,driver_contact=False,error_type=type(e).__name__,error=str(e)));raise
    write(a.output,result);print('PASS_ORIGINAL_STATE_VERIFIED; synthetic offline only; real session path absent')
if __name__=='__main__':main()
