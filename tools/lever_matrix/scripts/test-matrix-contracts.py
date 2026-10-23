"""Offline behavior checks for identity, arming, budgets and fixed-path reads."""
from pathlib import Path
from datetime import timedelta
import contextlib,importlib.util,io,json,os,socket,sys,tempfile,subprocess
from unittest.mock import patch

O=Path(__file__).resolve().parents[1]
def load(path,name):
    spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
@contextlib.contextmanager
def offline():
    def deny(*a,**k):raise AssertionError('Transport/subprocess attempted in offline behavior check')
    with contextlib.ExitStack() as stack:
        for obj,name in [(socket.socket,'connect'),(socket.socket,'connect_ex'),(socket.socket,'bind'),(socket,'create_connection'),(socket,'getaddrinfo'),(subprocess,'Popen'),(os,'system')]:
            stack.enter_context(patch.object(obj,name,deny))
        yield

def main():
    import argparse
    argparse.ArgumentParser(description=__doc__).parse_args()
    sys.path.insert(0,str(O/'console'))
    import pilot as p,session as c
    from map_identity import save_identity,verify_runtime,HUMAN_AUTO
    budget=load(O/'console/check-session-budget.py','budget')
    arm_mode=load(O/'console/arm-mode-only.py','arm_mode')
    ui=load(O/'console/ui.py','ui_matrix')
    verify_self=load(O/'console/verify-arm-self.py','verify_arm')
    offmain=load(O/'console/audit-run.py','audit_matrix')
    compare=load(O/'console/compare-abba.py','compare_matrix')
    logger=load(O/'console/host-log-receiver.py','logger_matrix')
    protocol=json.loads((O/'CONSOLE-PROTOCOL.json').read_text())
    checks=[]
    def record(name,**kw):checks.append(dict(check=name,status='PASS',**kw))
    def rejects(fn):
        try:fn()
        except (AssertionError,KeyError,ValueError):return
        raise AssertionError('Invalid input was accepted')
    with offline():
        for entry in protocol['sequence']:
            text='[lg/startup] map_load map='+entry['map']+'\n'
            if entry['map']=='pandemonium':text+='[lg/startup] restored scene reused map=pandemonium source=continue-or-bench\n'
            verify_runtime(text,entry,frozen=False)
            verify_runtime(text+'bench: camera frozen map='+entry['map']+' feet=Vec3(1,2,3)\n',entry)
            for invalid in [text.replace('map_load map='+entry['map'],'map_load map='+entry['map']+'$first'),text.replace('map_load map='+entry['map'],'map_load map=wrong'),text+text]:
                rejects(lambda:verify_runtime(invalid,entry,frozen=False))
            record('exact canonical map tokens',label=entry['label'])
        synthetic=json.dumps({'payload':{'current':'pandemonium$first','campaign':{'levels':{'pandemonium$first':{'map':'pandemonium'}}}}}).encode()
        identity=save_identity(synthetic,require_human=False)
        assert identity['visit_key']=='pandemonium$first' and identity['canonical_map']=='pandemonium'
        rejects(lambda:save_identity(synthetic))
        rejects(lambda:save_identity(synthetic.replace(b'"map": "pandemonium"',b'"map": "hedge3"'),require_human=False))
        record('logical visit distinct; synthetic/wrong human save rejected')
        ready=dict(pid=424242,started='2030-01-02T02:00:00+02:00',fixed_expiry='2030-01-02T14:00:00+02:00',lifetime_seconds=43200,auto_restart=False)
        expiry=budget.timestamp(ready['fixed_expiry'])
        boundary=expiry-timedelta(minutes=157.5);before=boundary-timedelta(seconds=1)
        ps=subprocess.CompletedProcess([],0,stdout='/venv/bin/python /local/terminal-driver.py --offline-fixture',stderr='')
        # Local metadata is mocked too: never signal or inspect the actual driver.
        with patch.object(budget.os,'kill') as kill,patch.object(budget.subprocess,'run',return_value=ps) as process:
            assert budget.check(ready,boundary,boundary)['status']=='STOP_RESTORE'
            assert budget.check(ready,before,before)['status']=='BUDGET_ADMITTED'
            assert budget.check(ready,before,expiry-timedelta(minutes=145))['status']=='STOP_RESTORE'
            rejects(lambda:budget.check(dict(ready,ready=False),before,before))
            record('strict >30 minute completion margin and readiness')
            later=dict(ready,pid=424243,started='2030-01-03T02:00:00+02:00',fixed_expiry='2030-01-03T14:00:00+02:00')
            later_now=budget.timestamp(later['started'])
            admitted=budget.driver_identity(ready)
            assert budget.check(later,later_now,later_now)['status']=='BUDGET_ADMITTED'
            record('restarted driver with later expiry accepted at fresh admission',receipt=later)
            changed=budget.check(later,later_now,later_now,admitted=admitted)
            assert changed['status']=='STOP_RESTORE' and 'driver changed inside an admitted session' in changed['reasons']
            for field,value in [('pid',424244),('started','2030-01-02T02:00:00.500+02:00'),('fixed_expiry','2030-01-02T14:00:00.500+02:00')]:
                result=budget.check(dict(ready,**{field:value}),before,before,admitted=admitted)
                assert result['status']=='STOP_RESTORE' and 'driver changed inside an admitted session' in result['reasons']
                record('mid-session driver identity change rejected',field=field)
            for fields in [dict(fixed_expiry='2030-01-02T15:00:00+02:00'),dict(lifetime_seconds=43199),dict(auto_restart=True),dict(pid=0)]:
                rejects(lambda:budget.driver_identity(dict(ready,**fields)))
                record('invalid driver receipt rejected',fields=fields)
            with patch.object(budget.os,'kill',side_effect=ProcessLookupError):
                rejects(lambda:budget.check(ready,before,before))
            record('dead driver pid rejected')
            with patch.object(budget.subprocess,'run',return_value=subprocess.CompletedProcess([],0,stdout='python /local/other.py',stderr='')):
                rejects(lambda:budget.check(ready,before,before))
            record('live unrelated pid rejected by command line')
            with patch.object(budget.subprocess,'run',return_value=subprocess.CompletedProcess([],0,stdout='python /local/not-terminal-driver.py',stderr='')):
                rejects(lambda:budget.check(ready,before,before))
            record('driver command line requires exact script token')
            assert budget.check(ready,before,before+timedelta(minutes=24),launches=8)['status']=='STOP_RESTORE'
            record('150 minute cap includes elapsed time and restoration')
            kill.assert_called_with(ready['pid'],0)
        for entry in protocol['sequence']:
            enabled=entry['config_arm']=='audio' and entry['arm']=='B';value=str(enabled).lower()
            text=f'[lg/audio-offmain] requested={value} active={value} mode=static-traces-only\n'
            if enabled:text+='[lg/audio-offmain] submitted=12 reused=3 fallback=2\n'
            result=offmain.audit_offmain(text,entry)
            assert result['requested']==enabled and result['active']==enabled
            if enabled:assert result['counters']==[dict(submitted=12,reused=3,fallback=2)]
            rejects(lambda:offmain.audit_offmain('',entry))
            rejects(lambda:offmain.audit_offmain(text.replace('active='+value,'active='+str(not enabled).lower()),entry))
            rejects(lambda:offmain.audit_offmain(text.replace('requested='+value,'requested='+str(not enabled).lower()),entry))
            if enabled:rejects(lambda:offmain.audit_offmain(text.splitlines()[0],entry))
            record('audio offmain engagement and raw fallback audit',label=entry['label'],enabled=enabled)
        def listener_run(command,**kwargs):
            if '-iTCP:9999' in command:return subprocess.CompletedProcess(command,0,stdout='p54321\n',stderr='')
            assert command==['/usr/sbin/lsof','-nP','-p','54321','-Fn']
            return subprocess.CompletedProcess(command,0,stdout='p54321\nn/Users/anezium/vita-j0/work/vita.log\n',stderr='')
        with patch.object(subprocess,'run',listener_run):
            assert logger.listener_provenance()['status']=='PASS'
        record('existing listener accepted only with exact host log open')
        for returncode,stdout,expected in [(1,'','ABSENT'),(2,'',None),(0,'p54321\n',None),(0,'p54321\nn/another/vita.log\n',None)]:
            with patch.object(subprocess,'run',return_value=subprocess.CompletedProcess([],returncode,stdout=stdout,stderr='')):
                if expected:assert logger.listener_provenance()['status']==expected
                else:rejects(logger.listener_provenance)
            record('listener absence or provenance refusal',returncode=returncode,output=stdout)
        with tempfile.TemporaryDirectory(prefix='matrix-contracts-',dir=O) as tmp:
            root=Path(tmp)
            ready_path=root/'synthetic-restarted-driver.json';ready_path.write_text(json.dumps(later))
            admission=root/'DRIVER-ADMISSION.json'
            argv=['check-session-budget.py','--driver-ready',str(ready_path),'--session-start',later['started'],'--now',later['started']]
            with patch.object(budget.os,'kill'),patch.object(budget.subprocess,'run',return_value=ps):
                with patch.object(sys,'argv',argv+['--admit',str(admission)]),contextlib.redirect_stdout(io.StringIO()) as out:
                    assert budget.main()==0
                receipt=json.loads(out.getvalue())
                assert json.loads(admission.read_text())==budget.driver_identity(later)
                assert receipt['latest_admission_for_one_session']==(budget.timestamp(later['fixed_expiry'])-timedelta(minutes=157.5)).isoformat()
                record('actual budget CLI accepts synthetic restarted receipt and writes identity',receipt=receipt)
                with patch.object(sys,'argv',argv+['--admit',str(admission)]):rejects(budget.main)
                record('admission receipt cannot be overwritten')
                with patch.object(sys,'argv',argv+['--admitted',str(admission)]),contextlib.redirect_stdout(io.StringIO()):assert budget.main()==0
                record('actual budget CLI boundary requires same identity')
                ready_path.write_text(json.dumps(dict(later,pid=424245)))
                with patch.object(sys,'argv',argv+['--admitted',str(admission)]),contextlib.redirect_stdout(io.StringIO()):assert budget.main()==2
                record('actual budget CLI rejects mid-session restart')
            with patch.object(sys,'argv',argv),contextlib.redirect_stderr(io.StringIO()):
                try:budget.main()
                except SystemExit as result:assert result.code==2
                else:raise AssertionError('Boundary without admission identity accepted')
            record('budget CLI requires admission or admitted identity')
            for manifest in sorted(O.glob('stage-*.json')):
                ledger=root/(manifest.stem+'-ledger.json');commands=[]
                def run_stage(command,**kwargs):commands.append(command)
                with patch.object(subprocess,'run',run_stage):
                    result=c.stage_manifest(manifest,ledger)
                assert len(commands)==1 and commands[0][commands[0].index('--manifest')+1]==str(manifest)
                assert json.loads(Path(str(ledger)+'.manifest.json').read_text())==result
                assert all(x['kind'] in ('self','small') for x in result['files'])
                record('real stage_manifest kind and path check on actual manifest',manifest=manifest.name,kinds=[x['kind'] for x in result['files']])
            invalid=root/'invalid-kind.json';manifest=json.loads((O/'stage-world-full.json').read_text());manifest['files'][0]['kind']='invalid';invalid.write_text(json.dumps(manifest))
            with patch.object(subprocess,'run',side_effect=RuntimeError('Stager before validation')):
                rejects(lambda:c.stage_manifest(invalid,root/'invalid-ledger.json'))
            record('real stage_manifest rejects invalid kind before stager')
            for entry in protocol['sequence']:
                label=entry['label'];run=root/label;run.mkdir()
                (run/'STAGE.json').write_text(json.dumps(dict(status='PASS',arm=entry['arm'])))
                if entry['map']=='pandemonium':
                    (run/'SAVE-IDENTITY.json').write_text(json.dumps(dict(sha256=HUMAN_AUTO,bytes=181900,map='pandemonium$first',visit_key='pandemonium$first',canonical_map='pandemonium',fresh_session_start_readback=True,companions_restored_and_verified=True,independent_readbacks=2)))
                text='[lg/startup] map_load map='+entry['map']+'\n'
                if entry['map']=='pandemonium':text+='[lg/startup] restored scene reused map=pandemonium source=continue-or-bench\n'
                calls=[]
                def capture(label): (root/label/'runtime-log.txt').write_text(text)
                full=json.loads((O/f"stage-{entry['config_arm']}-full.json").read_text())['files']
                inputs=[dict(present=True,path=x['remote'],local=str(O/'configs/profile_mode-fullbench.txt') if x['remote'].endswith('/profile_mode') else x['local']) for x in full]
                with patch.object(p,'ROOT',root),patch.object(c,'ROOT',root),patch.object(c,'capture_log',capture),patch.object(c,'stage_manifest',lambda *a:calls.append(a)),patch.object(p,'snapshot',lambda *a:inputs),patch.object(sys,'argv',['arm-mode-only.py',label]),contextlib.redirect_stdout(io.StringIO()):
                    arm_mode.main()
                assert len(calls)==1 and Path(calls[0][0]).name=='stage-fullbench-mode-only.json'
                assert len(list((run/'inputs').iterdir()))==4
                calls.clear();text=text.replace('map_load map='+entry['map'],'map_load map='+entry['map']+'$first')
                with patch.object(p,'ROOT',root),patch.object(c,'ROOT',root),patch.object(c,'capture_log',capture),patch.object(c,'stage_manifest',lambda *a:calls.append(a)),patch.object(sys,'argv',['arm-mode-only.py',label]):
                    rejects(arm_mode.main)
                assert not calls
                if entry['map']=='pandemonium':
                    with patch.object(p,'ROOT',root),patch.object(p,'request',lambda *a:(_ for _ in ()).throw(RuntimeError('Input before validation'))),patch.object(sys,'argv',['ui.py',label,'begin']):
                        rejects(ui.main)
                record('fullbench mode-only arming and pre-write map rejection',label=label)
            for entry in protocol['sequence']:
                for control in (None,'start'):
                    argv=['ui.py',entry['label'],'resume']+([control] if control else [])
                    with patch.object(p,'ROOT',root),patch.object(p,'cmd',side_effect=RuntimeError('Input before control validation')),patch.object(sys,'argv',argv):
                        rejects(ui.main)
                record('resume requires explicit supported control before input',label=entry['label'])
            block='s2-world-pandemonium';seq=[e for e in protocol['sequence'] if e['block']==block]
            camera=dict(camera_eye=[1,2,3],camera_target=[4,5,6],camera_up=[0,0,1],feet=[1,2,0])
            for values,status,continue_expected in [([60,61.1,60.5,60],'INCONCLUSIVE_REPEAT_REQUIRED',True),([60,62,62,60],'FAIL_WALL_GATE',False)]:
                for entry,value in zip(seq,values):
                    run=root/entry['label']
                    audit=dict(status='PASS',fields_ms={'wall_us':dict(n=120,median=value)},gl_observers='direct',observer_checks=dict(status='PASS',expected_mode='direct'),runtime_config_matches_readback=True,complete_benchmark=True,clean_exit=True)
                    (run/'AUDIT.json').write_text(json.dumps(audit));(run/'STRICT-AUDIO.json').write_text('{"status":"PASS"}')
                    (run/'ENTRY-STATE.json').write_text(json.dumps(dict(startup_camera_observations=[camera])))
                with patch.object(sys,'argv',['compare-abba.py','--root',str(root),'--block',block]),contextlib.redirect_stdout(io.StringIO()):
                    try:compare.main()
                    except SystemExit as result:assert result.code==2
                    else:raise AssertionError('Expected non-pass comparator exit')
                result=json.loads((root/(block+'-ABBA-GATE.json')).read_text())
                assert result['status']==status and ('Continue to Hedge3' in result['next_action'])==continue_expected
                record('Pandemonium continuation guidance preserves independent gate',status_result=status)
            # Independent fixed-path SELF helper, mocked transport returning actual sealed local bytes.
            entry=protocol['sequence'][0];label=entry['label'];requests=[]
            class Session:
                def __init__(self,*a,**k):pass
                def __enter__(self):return self
                def __exit__(self,*a):pass
                def execute(self,action,remote,local=None):
                    requests.append((action,remote))
                    binary=O/'packages'/entry['package']/'eboot.bin'
                    if action=='size':return binary.stat().st_size
                    assert action=='get';Path(local).write_bytes(binary.read_bytes())
            with patch.object(p,'ROOT',root),patch.object(p,'Session',Session),patch.object(sys,'argv',['verify-arm-self.py',label]),contextlib.redirect_stdout(io.StringIO()):
                verify_self.main()
            receipt=json.loads((root/label/'SELF-IDENTITY.json').read_text())
            assert receipt['status']=='PASS' and all(remote=='ux0:/app/ALICELG01/eboot.bin' for _,remote in requests)
            record('independent fixed-path arm SELF read',operations=requests)
    output=O/'proof/MATRIX-CONTRACT-TESTS.json'
    output.write_text(json.dumps(dict(status='PASS',offline_only=True,synthetic_only=True,device_contact=False,driver_contact=False,checks=checks),indent=2)+'\n')
    print(len(checks),'matrix contract checks passed')

if __name__=='__main__':main()
