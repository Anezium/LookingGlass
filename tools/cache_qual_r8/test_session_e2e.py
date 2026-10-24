"""Run real r8 helper mains against in-memory transport and a synthetic game.

ALL sockets/processes are blocked before helper imports. This is never device
evidence. Uses locally retained human bytes only to prove exact restoration.
"""
from pathlib import Path
import argparse, contextlib, copy, hashlib, importlib.util, io, json, os, runpy, shutil, socket, subprocess, sys, tempfile, time, types, zipfile
from unittest.mock import patch
HUMAN_SELF=Path('/Users/anezium/vita-j0/resume-2026-10-22/alice-card-think/console-r4/restored-self-independent.bin')
HUMAN_SAVE=Path('/Users/anezium/vita-j0/resume-2026-10-24/alice-lever-matrix-r3/session-s5-world-static-merge/originals/data/alice/private/saves/auto.json')
def sha(b):return hashlib.sha256(b).hexdigest()
def write(p,v):p.parent.mkdir(parents=True,exist_ok=True);p.write_text(json.dumps(v,indent=2)+'\n')
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
    def event(kind,**v):data['events'].append(dict(kind=kind,us=us,heap=150*1048576,**v))
    def load_map(to,kind=None):
        nonlocal us,scene,mapname,warm
        if kind:
            event('transition',operation=kind,**{'from':mapname,'to':to},stage='before-drop');us+=10
            event('transition',operation=kind,**{'from':mapname,'to':to},stage='after-drop');us+=10
            append('[lg/transition-memory] '+json.dumps(dict(kind=kind,**{'from':mapname,'to':to})))
        if current['arm']=='B':event('reset',reason='audio-load',old_scene=scene,entries_before=512 if warm else 0,entries_after=0,pin_after=False,old_refs_before=1)
        scene+=1;mapname=to;warm=False;us+=10
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
        nonlocal us,warm
        assert pending and c['label']==pending['label']
        moving=c['label'].endswith('-move');count=1281 if moving else 101
        rows=[]
        for i in range(count):
            cached=current['arm']=='B';counts=[[3,3,0],[2,1,0],[7,1,0]] if cached else [[0,6,0],[0,3,0],[0,8,0]]
            rows.append(dict(us=us+i*50000,prepare_ns=2500000,heap=150*1048576+(32000 if cached else 0),scene=scene,
                eye=[float(i)*.1 if moving else 128.,0.,0.],queries=[6,3,8],cache=counts,
                checks=int(cached and c['label'].endswith('-check')),mismatches=0))
        us=rows[-1]['us']+10000
        data['windows'].append(dict(id=pending['id'],label=c['label'],map=mapname,check_stride=pending['check_stride'],rows=rows,overflow=False,end_us=us))
        for line in reporters():append(line)
        append(f"[lg/cache-qual] stop id={c['id']} label={c['label']}")
        snapshot_export();warm=True;us+=10000
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
            nonlocal pending
            assert active==1;ops.append((op,path))
            if op=='list':return '\n'.join(('d' if v['directory'] else '-')+'rwxr-xr-x 1 vita vita '+str(v['bytes'])+' Oct 24 00:00 '+n for n,v in listing(path).items())
            if op=='size':return len(remote[path])
            if op=='get':Path(local).parent.mkdir(parents=True,exist_ok=True);Path(local).write_bytes(remote[path]);return
            if op=='put':
                remote[path]=Path(local).read_bytes()
                if path==R+'cache_qual_command.json':
                    try:c=json.loads(remote[path])
                    except json.JSONDecodeError:return
                    if c['id'] and c['action']=='start':
                        pending=c;append(f"[lg/cache-qual] start id={c['id']} label={c['label']} check_stride={c['check_stride']}")
                    elif c['id']:finish_window(c)
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
        nonlocal data,us,scene,mapname,warm,next_button
        assert active==0,'driver with FTP open';p.owner();args=args or {};driver_calls.append((name,args))
        if name=='vita_launch':
            data=dict(schema=1,capacity=8192,query_classes=['room','ambient','effects'],windows=[],events=[]);us=100;scene=10;mapname='gvillage';warm=False
            cfg=json.loads(remote[R+'vita_renderer.json']);append('[lg/vita-renderer] config '+json.dumps(cfg,separators=(',',':')))
            load_map('gvillage')
        elif name=='vita_press' and next_button:
            kind,to=next_button;next_button=None
            if kind=='quit':
                remote[R+'saves/auto.json']=b'synthetic quit autosave';remote[R+'saves/quit-created.json']=b'synthetic quit companion'
                append('Viewer closed cleanly. Data folder: OFFLINE')
                if current['arm']=='B':event('reset',reason='audio-finish',old_scene=scene,entries_before=512,entries_after=0,pin_after=False,old_refs_before=2)
                snapshot_export();append('[lg/cache-qual] exported windows=6 events='+str(len(data['events'])))
            else:load_map(to,kind)
        elif name=='vita_shot':return str(work/'fixture.png')
        else:assert name in ['vita_press','vita_cmd','vita_quit']
        return 'OFFLINE_FAKE_OK'
    def monotonic():
        nonlocal clock
        clock+=.25;return clock
    # Deny real transport and subprocesses BEFORE importing any console helper.
    with contextlib.ExitStack() as stack:
        for obj,name in [(socket.socket,'connect'),(socket.socket,'connect_ex'),(socket.socket,'bind'),(socket,'create_connection'),(socket,'getaddrinfo'),(subprocess,'Popen'),(os,'system')]:stack.enter_context(patch.object(obj,name,deny))
        stack.enter_context(patch.object(subprocess,'run',fake_run));stack.enter_context(patch.object(os,'kill',lambda pid,sig:None if (pid,sig)==(4321,0) else deny()))
        stack.enter_context(patch.object(time,'sleep',lambda n:None));stack.enter_context(patch.object(time,'monotonic',monotonic))
        stack.enter_context(patch.dict(sys.modules,{'vita_ftp':types.SimpleNamespace(Session=Transport)}));stack.enter_context(patch.object(sys,'path',[str(root)]+sys.path))
        import pilot as p, session, audit, run_health
        p.LOG=log;p.CACHE_INDEX=cache;stack.enter_context(patch.object(p,'request',driver))
        (work/'fixture.png').write_bytes(b'OFFLINE_FAKE_IMAGE')
        def call(name,*args):
            with patch.object(sys,'argv',[name,*map(str,args)]),contextlib.redirect_stdout(io.StringIO()):
                try:runpy.run_path(str(root/name if (root/name).exists() else package/'scripts'/name),run_name='__main__')
                except SystemExit as e:assert e.code in (None,0),(name,e.code)
        call('bind_helpers.py','--root',root,'--package',package)
        ready=work/'driver-ready.json';write(ready,dict(pid=4321,started='2026-10-24T08:00:00+00:00',fixed_expiry='2026-10-24T20:00:00+00:00',lifetime_seconds=43200,auto_restart=False,ready=True))
        call('check-session-budget.py','--driver-ready',ready,'--admit',root/'DRIVER-ADMISSION.json','--session-start','2026-10-24T08:00:00+00:00','--now','2026-10-24T08:00:00+00:00','--launches','2','--pandemonium-launches','2','--preservation-minutes','64.5')
        call('admit.py','--sole-console-owner');call('verify-human-entry.py');call('preserve.py');call('baseline-r3.py');call('save-proof.py')
        session.stage_manifest(package/'stage-baseline-self.json',root/'self-stage-ledger.json')
        for current in seq:
            number=seq.index(current)
            call('check-session-budget.py','--driver-ready',ready,'--admitted',root/'DRIVER-ADMISSION.json',
                '--session-start','2026-10-24T08:00:00+00:00','--now',f'2026-10-24T08:{20+number*20:02d}:00+00:00',
                '--launches',str(2-number),'--self-stages','0','--pandemonium-launches',str(2-number),'--preservation-minutes',str(44.5-number*20))
            label=current['label'];call('session.py','stage',label);call('verify-arm-self.py',label)
            call('operate.py',label,'launch');call('ack.py','OFFLINE synthetic launch image')
            next_button=('chapter-or-console','hedge3');call('operate.py',label,'button','cross');call('ack.py','OFFLINE synthetic chapter entry')
            for label_window,action in [('hedge3-move','move'),('hedge3-check','check'),('pandemonium-move','move'),('pandemonium-check','check'),('reload-post','post'),('transition-post','post')]:
                if label_window in ['pandemonium-move','reload-post','transition-post']:
                    call('session.py','saves','before-'+label+'-'+label_window)
                    next_button=('chapter-or-console' if label_window=='transition-post' else 'restore','hedge3' if label_window=='transition-post' else 'pandemonium')
                    call('operate.py',label,'button','cross');call('ack.py','OFFLINE synthetic native load confirmation')
                call('operate.py',label,action,label_window,'circle');call('ack.py','OFFLINE synthetic stopped window')
            next_button=('quit',None);call('operate.py',label,'button','cross');call('ack.py','OFFLINE synthetic quit')
            call('operate.py',label,'wait-exit');call('ack.py','OFFLINE synthetic HOME')
            call('session.py','collect',label);call('log-integrity.py',label,'--root',root);call('audit.py',label)
            checked=json.loads((root/label/'AUDIT.json').read_text());assert checked['status']=='PASS'
            assert checked['audio_session']['BENCHMARK_LATE_CALLBACKS']['total']==6
            # Deliberate failures verify fixed rules, independently of restoration.
            raw=(root/label/'runtime-log.txt').read_text();fixture=json.loads(remote[R+'frame_profile/cache-qual.json'])
            for name,mutate in [('underrun',lambda d,t:(d,t.replace('underrun_estimates=0','underrun_estimates=1'))),
                ('missing counter',lambda d,t:(d,t.replace('producer_blocks=80 ','').replace('callbacks=88 ',''))),
                ('short motion',lambda d,t:(dict(d,windows=[dict(d['windows'][0],rows=d['windows'][0]['rows'][:100])]+d['windows'][1:]),t)),
                ('heap',lambda d,t:(dict(d,windows=[dict(d['windows'][0],rows=[dict(d['windows'][0]['rows'][0],heap=181*1048576)]+d['windows'][0]['rows'][1:])]+d['windows'][1:]),t))]:
                bad,txt=mutate(copy.deepcopy(fixture),raw)
                try:audit.validate(bad,txt,current['arm'])
                except AssertionError:pass
                else:raise AssertionError('bad '+name+' accepted')
            if current['arm']=='B':
                for key,value in [('mismatches',1)]:
                    bad=copy.deepcopy(fixture);bad['windows'][1]['rows'][0][key]=value
                    try:audit.validate(bad,raw,'B')
                    except AssertionError:pass
                    else:raise AssertionError('mismatch accepted')
                bad=copy.deepcopy(fixture);next(e for e in bad['events'] if e['kind']=='reset')['entries_after']=1
                try:audit.validate(bad,raw,'B')
                except AssertionError:pass
                else:raise AssertionError('uncleared reset accepted')
                bad=copy.deepcopy(fixture)
                for row in bad['windows'][1]['rows']:row['checks']=0
                try:audit.validate(bad,raw,'B')
                except AssertionError:pass
                else:raise AssertionError('zero sampled hits accepted')
            for corruption in [raw+'orphan [lg/audio] tag\n',raw+' field=orphan\n']:
                (root/label/'runtime-log.txt').write_text(corruption)
                try:call('log-integrity.py',label,'--root',root)
                except AssertionError:pass
                else:raise AssertionError('corrupt log accepted')
            (root/label/'runtime-log.txt').write_text(raw);call('log-integrity.py',label,'--root',root)
            call('restore-arm.py',label)
        call('audit.py','report');assert json.loads((root/'RESULTS.json').read_text())['default_on'] is False
        bpath=root/seq[1]['label']/'AUDIT.json';boriginal=bpath.read_bytes();altered=json.loads(boriginal)
        abase=json.loads((root/seq[0]['label']/'AUDIT.json').read_text())
        altered['windows'][0]['heap_bytes']['maximum']=abase['windows'][0]['heap_bytes']['maximum']+1048577
        write(bpath,altered)
        try:call('audit.py','report')
        except AssertionError:pass
        else:raise AssertionError('paired heap above +1 MiB accepted')
        altered['windows'][0]['heap_bytes']['maximum']=abase['windows'][0]['heap_bytes']['maximum']+1048576
        write(bpath,altered);call('audit.py','report')
        bpath.write_bytes(boriginal);call('audit.py','report')
        call('session.py','restore')
        final=json.loads((root/'RESTORE-FINAL.json').read_text());assert final['status']=='PASS_ORIGINAL_STATE_VERIFIED'
        assert remote==initial and cache.read_bytes()==cache_before and maximum==1 and active==0
        assert stages.count('stage-baseline-self.json')==1 and len([v for v in driver_calls if v[0]=='vita_launch'])==2
        assert all(json.loads(f.read_text()).get('restore_status')=='restored' for f in root.rglob('*ledger.json'))
        assert any('transition-created.json' in path and op=='get' for op,path in ops)
        assert any('quit-created.json' in path and op=='get' for op,path in ops)
        # Original census bytes and a prior qualification export were restored;
        # initial qualification tmp absence and new transition/quit saves match.
        return dict(status='PASS',synthetic_offline_only=True,device_contact=False,driver_contact=False,real_transport_imported=False,
            final_status=final['status'],full_restoration=True,launches=2,windows_per_arm=6,self_stages=1,
            serialized_fake_ftp_maximum=maximum,driver_fake_calls=len(driver_calls),late_callbacks_report_only=True,
            negative_fixtures=['underrun','heap','missing counter','short motion','mismatch','uncleared reset','zero sampled hits','paired heap >1MiB','interleaved tag','orphan field'],real_package_session_path_absent=True,
            helper_main_path=True,zip_replay=replay,created_saves_preserved_before_deletion=True)
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
