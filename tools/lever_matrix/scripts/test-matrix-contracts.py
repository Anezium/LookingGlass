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
        expiry=budget.timestamp(budget.EXPIRY)
        boundary=expiry-timedelta(minutes=157.5)
        ready={'fixed_expiry':budget.EXPIRY,'ready':True}
        assert budget.check(ready,boundary,boundary)['status']=='STOP_RESTORE'
        before=boundary-timedelta(seconds=1)
        assert budget.check(ready,before,before)['status']=='BUDGET_ADMITTED'
        assert budget.check(ready,before,expiry-timedelta(minutes=145))['status']=='STOP_RESTORE'
        rejects(lambda:budget.check(dict(ready,ready=False),before,before))
        rejects(lambda:budget.check(dict(ready,fixed_expiry='2026-10-24T08:10:31+02:00'),before,before))
        record('strict >30 minute completion margin; readiness and immutable expiry')
        with tempfile.TemporaryDirectory(prefix='matrix-contracts-',dir=O) as tmp:
            root=Path(tmp)
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
