#!/usr/bin/env python3
"""Root-only console comparison. No receiver or terminal is started here.
Root supplies a backed-up, observed-closed session with its log running.
Use --step for one arm, acknowledge its PNG, then repeat. --step --restore restores separately.
"""
import argparse, hashlib, importlib.util, json, re, shutil, subprocess, sys, time
from pathlib import Path

def sha(b): return hashlib.sha256(b).hexdigest()
def payload(b, key):
    assert len(b)>76 and b[:8]==b'LGCACHE1' and b[8:40].hex()==key
    assert int.from_bytes(b[72:76],'little')==len(b)-76
    assert sha(b[76:])==b[40:72].hex()
    return sha(b[76:])

def compare(a,b):
    assert a and a==b, 'shader cache file sets or payload hashes differ (or empty)'

def run(package, backend, restore=False):
    """Exactly one replay arm per invocation; root acknowledges between invocations."""
    ready=json.loads((package/'READY.json').read_text())
    assert ready['title']=='ALICELG01'
    for name,digest in ready['files'].items():
        assert sha((package/name).read_bytes())==digest, name
    backend.preflight(ready)
    progress=backend.session/'no-prelude-progress.json'
    state=json.loads(progress.read_text()) if progress.exists() else dict(index=0,results=[],manifests={},failed=False)
    assert not state['failed'], 'prior driver failure; stop for root recovery'
    available=[]
    for tape in ready['tapes']:
        data=backend.get_tape(tape['name'],tape['required'])
        if data is None: continue
        assert sha(data)==tape['sha256'], 'tape differs: '+tape['name']
        available.append(tape)
    assert any(t['required'] for t in available)
    plan=[(arm,tape) for arm in ('old','new') for tape in available]
    identity=sha(json.dumps(ready,sort_keys=True).encode())
    assert state.get('identity',identity)==identity and state.get('plan',[[a,t['name']] for a,t in plan])==[[a,t['name']] for a,t in plan], 'package/tape set changed'
    state.update(identity=identity,plan=[(a,t['name']) for a,t in plan])
    # JSON normalizes tuples to lists.
    state['plan']=[[a,t['name']] for a,t in plan]
    def save():
        backend.k.write_json(progress,state)
    if restore:
        assert state['index']==len(plan), 'arms incomplete'
        compare(state['manifests']['old'],state['manifests']['new'])
        backend.restore()
        backend.result(dict(result='PASS',replays=state['results'],payloads=state['manifests']))
        state['restored']=True;save();return
    assert state['index']<len(plan), 'all arms complete; observe final PNG then use --restore'
    arm,tape=plan[state['index']]
    backend.require_absent(ready['namespaces'][arm]+'-'+tape['label'])
    state['failed']=True;save()  # crash/failure must not silently retry a launch
    config=package/'configs'/f"{arm}-{tape['label']}.json"
    backend.install(package/f'{arm}-eboot.bin',config)
    text=backend.launch_wait()
    assert not any(x in text.lower() for x in ('panicked at','invalid config','render gpu crash','[lg/replay] diverge','[lg/replay] mismatch','cache_write_failed','export_unavailable'))
    done=[l for l in text.splitlines() if '[lg/replay] done ' in l]
    assert len(done)==1 and all(re.search(r'\b'+x+r'(?:\s|$)',done[0]) for x in ('result=PASS','diff=0','event_mismatches=0')), text[-3000:]
    shaders=[l for l in text.splitlines() if '[lg/shader] name=' in l]
    assert shaders and all('linked=1 ' in l and 'cache=disk ' not in l for l in shaders), 'not a cold successful compilation'
    expected=set(re.findall(r'cache_key=([0-9a-f]{64})', '\n'.join(shaders)))
    files=backend.cache(ready['namespaces'][arm]+'-'+tape['label'])
    assert set(files)=={key+'.bin' for key in expected}, 'cache does not cover logged shader keys'
    state['manifests'].setdefault(arm,{})[tape['label']]={name:payload(data,name[:-4]) for name,data in sorted(files.items())}
    backend.save(arm+'-'+tape['label'],files)
    state['results'].append(dict(arm=arm,tape=tape['name'],result=done[0]))
    state['index']+=1;state['failed']=False;save()
    print('STEP COMPLETE; inspect latest PNG and run closed --observed before next step or --restore')

class Console:
    def __init__(self,kit,session):
        spec=importlib.util.spec_from_file_location('kit',kit); self.k=importlib.util.module_from_spec(spec);spec.loader.exec_module(self.k)
        assert self.k.TITLE == "ALICELG01" and self.k.SELF == "ux0:/app/ALICELG01/eboot.bin"
        self.exit_failed=False
        self.s=self.k.Sess(session);self.kit=kit;self.session=session
        # Hard gate even if a kit is later edited to use deletes or MKD.
        original=self.k.vita_ftp.Session.execute
        def execute(obj,action,*args,**kwargs):
            assert action in ('list','get','put'), action
            return original(obj,action,*args,**kwargs)
        self.k.vita_ftp.Session.execute=execute
        self.k.Ftp.delete=lambda *a: (_ for _ in ()).throw(AssertionError('no deletes'))
    def command(self,*args):
        # Run in-process so the FTP allowlist also covers install/restore.
        code=self.k.main(['--session',str(self.session),*map(str,args)])
        if code not in (None,0):
            if args[0] == 'wait-exit': self.exit_failed=True
            reason='INCONCLUSIVE; stop, no verdict, no retry' if args[:2]==('log','check-config') and code==4 else 'stop for root recovery'
            raise RuntimeError(f'{args}: exit {code}; {reason}')
    def preflight(self,ready):
        assert sha(self.kit.read_bytes()) == ready["kit_sha256"], "rc1kit changed since packaging"
        assert shutil.disk_usage(self.session).free>=40*2**30
        s=self.s.state(); self.k.mutation_guard(self.s,'driver step'); assert s['backup'] and not s['pending'] and s['log']['pid']
        self.log=Path(s['log']['path']); assert self.log.is_file()
        b=self.s.backup(); assert self.k.RENDERER not in b['absent']
    def get_tape(self,name,required):
        with self.k.FtpSession(self.s) as f:
            rows=f.ls(self.k.PRIV)
            if name not in rows:
                assert not required, 'required tape missing'
                return None
            _,_,digest=f.fetch(self.k.PRIV+name)
            return self.s.stored(digest).read_bytes()
    def require_absent(self,name):
        with self.k.FtpSession(self.s) as f:
            # Parent is the existing release shader cache; never create it by FTP.
            if 'cache' not in f.ls(self.k.PRIV): return
            if 'shaders' not in f.ls(self.k.PRIV+'cache/'): return
            assert name not in f.ls(self.k.PRIV+'cache/shaders/'), 'namespace already exists'
    def install(self,self_path,config):
        if self.exit_failed: raise RuntimeError('exit failure latched; root recovery required')
        self.command('install',self_path,config)
    def launch_wait(self):
        self.command('launch')
        # No replay retry and no verdict until exit and a complete attributed config.
        self.command('wait-exit','--timeout','360')
        # The replay exits by itself; a quit that still had to kill the app is an anomaly.
        outcome=(self.s.state().get('observation') or {}).get('quit_outcome')
        if outcome!='not-running':
            self.exit_failed=True
            raise RuntimeError(f'quit outcome {outcome!r} after a self-exiting replay; stop for root recovery')
        self.command('log','check-config')  # code 4 is INCONCLUSIVE: stop, no verdict
        rows=self.k.launch_lines(list(self.k.parse_log(self.log,connections=True)))
        return '\n'.join(text for n,t,src,text,cid in rows if src=='vita')
    def cache(self,name):
        out={}
        with self.k.FtpSession(self.s) as f:
            root=self.k.PRIV+'cache/shaders/'+name+'/'
            for entry,(directory,size) in f.ls(root).items():
                assert not directory and re.fullmatch(r'[0-9a-f]{64}\.bin',entry)
                _,_,digest=f.fetch(root+entry);out[entry]=self.s.stored(digest).read_bytes()
        return out
    def save(self,arm,files):
        out=self.session/'no-prelude-cache'/arm;out.mkdir(parents=True)
        for name,data in files.items():(out/name).write_bytes(data)
    def result(self,result):(self.session/'no-prelude-result.json').write_text(json.dumps(result,indent=2)+'\n')
    def restore(self):
        # --keep-saves avoids every kit delete path. New game-created caches remain.
        if self.exit_failed: raise RuntimeError('exit failure latched; no restoration; root recovery required')
        self.command('restore','--keep-saves')

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--step',action='store_true',required=True);p.add_argument('--restore',action='store_true');p.add_argument('--package',type=Path,default=Path(__file__).resolve().parent);p.add_argument('--kit',type=Path,required=True);p.add_argument('--session',type=Path,required=True);a=p.parse_args()
    backend=Console(a.kit,a.session)
    with backend.k.session_lock(backend.s,'driver.lock'):
        run(a.package,backend,restore=a.restore)
