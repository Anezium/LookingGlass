"""Offline Round 3 helper binding and behavioral checks; never console evidence."""
from pathlib import Path
import argparse, contextlib, csv, hashlib, importlib.util, io, json, os, runpy
import socket, subprocess, sys, tempfile, time
from unittest.mock import patch

OUT=Path(__file__).resolve().parents[1]
PY=Path('/Users/anezium/vita-j0/venv/bin/python')
EXTERNAL=Path('/Users/anezium/vita-j0/bin')

def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()

def load(path,name):
    from importlib.machinery import SourceFileLoader
    spec=importlib.util.spec_from_loader(name,SourceFileLoader(name,str(path)))
    module=importlib.util.module_from_spec(spec);sys.modules[name]=module
    spec.loader.exec_module(module)
    return module

@contextlib.contextmanager
def no_transport():
    def deny(*args,**kwargs):raise AssertionError('Offline check attempted transport, listener or subprocess')
    with contextlib.ExitStack() as stack:
        for obj,name in [(socket.socket,'connect'),(socket.socket,'connect_ex'),(socket.socket,'bind'),
                         (socket,'create_connection'),(socket,'getaddrinfo'),(subprocess,'Popen'),(os,'system')]:
            stack.enter_context(patch.object(obj,name,deny))
        yield

def probe(path,mode,root):
    sys.path.insert(0,str(root));sys.path.insert(1,str(EXTERNAL))
    with no_transport():
        if mode=='import':load(path,'offline_'+path.stem.replace('-','_'))
        else:
            sys.argv=[str(path),'--help']
            try:runpy.run_path(str(path),run_name='__main__')
            except SystemExit as result:
                assert result.code in (0,None),result.code
    print('PASS',mode,path.name)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,default=OUT/'console')
    parser.add_argument('--output',type=Path,default=OUT/'proof/ROUND3-DRY-RUN.json')
    parser.add_argument('--probe',type=Path)
    parser.add_argument('--mode',choices=['import','help'])
    args=parser.parse_args();root=args.root.resolve()
    if args.probe:
        probe(args.probe,args.mode,root);return
    assert root.parent==OUT and root.is_dir(),'ROOT must be console/ or a fresh OUT/session directory'
    results=[]
    def record(name,**details):results.append(dict(check=name,status='PASS',**details))
    env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1',PATH=str(PY.parent)+os.pathsep+os.environ['PATH'])
    files=sorted(root.glob('*.py'))
    assert {p.name for p in files}=={p.name for p in (OUT/'console').glob('*.py')}
    for path in files:assert path.read_bytes()==(OUT/'console'/path.name).read_bytes(),path
    command=[str(PY),'-c',"import sys; sys.path.insert(0,'.'); import pilot, session, run_health, light_csv"]
    imported=subprocess.run(command,cwd=root,env=env,capture_output=True,text=True,timeout=15)
    assert imported.returncode==0,imported.stderr
    record('review import command in place',command=command,cwd=str(root),returncode=imported.returncode,stdout=imported.stdout,stderr=imported.stderr)
    for path in files+[OUT/'scripts/dry-run-console.py',OUT/'scripts/test-console-tools.py',OUT/'scripts/test-matrix-contracts.py',EXTERNAL/'vita-stage',EXTERNAL/'vita_ftp.py']:
        for mode in ('import','help'):
            command=[str(PY),str(Path(__file__).resolve()),'--root',str(root),'--probe',str(path),'--mode',mode]
            proc=subprocess.run(command,cwd=root,env=env,capture_output=True,text=True,timeout=15)
            assert proc.returncode==0,(path,mode,proc.stderr,proc.stdout)
            record('helper '+mode,path=str(path),returncode=proc.returncode,stdout=proc.stdout,stderr=proc.stderr,transport_guard=True)
    proc=subprocess.run(['/bin/sh',str(EXTERNAL/'vita-ftp'),'--help'],cwd=root,env=env,capture_output=True,text=True,timeout=15)
    assert proc.returncode==0,proc.stderr
    record('shell wrapper help',path=str(EXTERNAL/'vita-ftp'),returncode=proc.returncode,stdout=proc.stdout,stderr=proc.stderr)
    pattern=r"reference|NATIVE-PACKAGE|frame_cpu_us'\]\)>0|a1134e"
    proc=subprocess.run(['/usr/bin/grep','-nE',pattern,*map(str,files)],capture_output=True,text=True)
    assert proc.returncode==1 and not proc.stdout and not proc.stderr,proc.stdout+proc.stderr
    record('review stale-binding grep is empty',pattern=pattern,returncode=proc.returncode,stdout=proc.stdout,stderr=proc.stderr)
    pattern=r'\barm\b|\blabel\b|reference|baseline|candidate|endswith|startswith'
    proc=subprocess.run(['rg','-n',pattern,*map(str,files)],capture_output=True,text=True)
    assert proc.returncode==0
    record('complete arm and label grep',pattern=pattern,returncode=proc.returncode,stdout=proc.stdout,
           assessment='Arm policy uses exact protocol label equality then A/B equality. Map parsing and path prefix checks do not select arms. baseline snapshot names and response candidates are not arm identities.')
    sys.path.insert(0,str(root))
    import pilot as p, session as c, run_health as health
    ui=load(root/'ui.py','offline_ui')
    sequence=json.loads((OUT/'CONSOLE-PROTOCOL.json').read_text())['sequence']
    with tempfile.TemporaryDirectory(prefix='alice-round3-synthetic-') as tmp:
        temp=Path(tmp)
        # Full session.collect(), including paired readbacks and every existing CSV assertion.
        light=temp/'light.csv';doors=temp/'doors.csv'
        with light.open('w',newline='') as f:
            writer=csv.writer(f);writer.writerow(['frame','wall_us','frame_cpu_us','gl_observers'])
            writer.writerows([[i,60000,'','direct'] for i in range(1,121)])
        doors.write_text('frame_first,frame_last,frames,hatter_door_world_traces,hatter_door_own_traces,hatter_door_trace_pairs_deferred\n1,60,60,,,\n61,120,60,,,\n')
        label=sequence[0]['label'];run=temp/label;run.mkdir()
        snapshots=[]
        def snapshot(tag,paths):
            snapshots.append(tag)
            return [dict(path=remote,present=True,local=str(local),sha256=sha(local)) for remote,local in zip(paths,[light,doors])]
        with no_transport(),patch.object(c,'ROOT',temp),patch.object(p,'ROOT',temp),patch.object(p,'snapshot',snapshot),patch.object(c,'capture_log',lambda label:None),contextlib.redirect_stdout(io.StringIO()):
            c.collect(label)
        collected=json.loads((run/'COLLECT.json').read_text())
        assert snapshots==['exports','exports-independent-readback']
        assert [x['local'] for x in collected['files']]==[str(light),str(doors)]
        assert all(Path(x['local']).is_absolute() for x in collected['files'])
        record('collect accepts blank frame_cpu_us',rows=120,blank_frame_cpu_us_rows=120,positive_wall_rows=120,doors_coverage=[[1,60],[61,120]],snapshot_calls=snapshots,absolute_local_paths=True)
        # Existing row count/positive wall/doors checks must still reject bad exports.
        valid=light.read_text();valid_doors=doors.read_text()
        for name,bad,bad_doors in [('zero wall',valid.replace('60000','0',1),valid_doors),('missing row','\n'.join(valid.splitlines()[:-1])+'\n',valid_doors),('broken doors',valid,valid_doors.replace('61,120,60','62,120,59'))]:
            light.write_text(bad);doors.write_text(bad_doors)
            with no_transport(),patch.object(c,'ROOT',temp),patch.object(p,'ROOT',temp),patch.object(p,'snapshot',snapshot),patch.object(c,'capture_log',lambda label:None):
                try:c.collect(label)
                except AssertionError:pass
                else:raise AssertionError('collect accepted '+name)
            record('collect rejects '+name)
        light.write_text(valid);doors.write_text(valid_doors)
        # Run actual stage() with transport mocked, keeping manifest/config readbacks real.
        (temp/'BASELINE.json').write_text('{}')
        class FakeSession:
            def __init__(self,*a,**k):pass
            def __enter__(self):return self
            def __exit__(self,*a):pass
        for entry in [next(x for x in sequence if x['config_arm']==arm and x['map']=='hedge3') for arm in ['baseline','world','fx','audio']]:
            arm=entry['arm'];manifest=OUT/f"stage-{entry['config_arm']}-full.json"
            def config_snapshot(tag,paths):
                items=json.loads(manifest.read_text())['files']
                return [dict(present=True,local=v['local'],path=v['remote']) for v in items]
            with no_transport(),patch.object(c,'ROOT',temp),patch.object(p,'ROOT',temp),patch.object(p,'Session',FakeSession),patch.object(p,'listing',lambda *a:{}),patch.object(c,'stage_manifest',lambda m,l:json.loads(m.read_text())),patch.object(p,'snapshot',config_snapshot),contextlib.redirect_stdout(io.StringIO()):
                c.stage(entry['label'],arm,'full')
            staged=json.loads((temp/entry['label']/'STAGE.json').read_text())
            expected=json.loads((OUT/'packages'/entry['package']/'VERIFICATION.json').read_text())['self_sha256']
            assert staged['arm']==arm and staged['self_hash']==expected
            record('stage accepts '+arm,manifest=str(manifest),self_sha256=expected)
        calls=[]
        with no_transport(),patch.object(c,'ROOT',temp),patch.object(subprocess,'run',lambda command,**kw:calls.append(command)):
            c.stage('human-restore','human-restore','full')
        assert [Path(command[1]).name for command in calls]==['restore-ledgers.py','verify-self.py','restore-files.py']
        record('stage accepts human-restore via exhaustive restoration',helpers=[Path(command[1]).name for command in calls])
        preservation=load(root/'preserve.py','offline_preservation')
        import shutil
        preservation_root=temp/'preservation';preservation_root.mkdir()
        (temp/'packages').symlink_to(OUT/'packages',target_is_directory=True)
        with no_transport(),patch.object(p,'ROOT',preservation_root),patch.object(p,'B',preservation_root),patch.object(p,'Session',FakeSession),patch.object(p,'listing',lambda *a:{}),patch.object(shutil,'copy2',lambda *a:None),patch.object(sys,'argv',['preserve.py']),contextlib.redirect_stdout(io.StringIO()):
            preservation.main()
        preserved=json.loads((preservation_root/'SESSION.json').read_text())
        for arm,package in [(a,a) for a in ['baseline','world','fx','audio']]:
            assert preserved['package_self_sha256'][arm]==json.loads((OUT/'packages'/package/'VERIFICATION.json').read_text())['self_sha256']
        assert 'candidate_sha256' not in preserved
        record('preserve records current A and B SELF hashes',packages=preserved['package_self_sha256'])
        # Exercise the real UI begin/resume loops; never call pilot transport or read the live log.
        for entry in sequence:
            for action in (('begin','resume') if entry['map']=='hedge3' else ('resume',)):
                for p95 in (7.999,8.0,9.6,100.0):
                    label=entry['label'];run=temp/label;run.mkdir(exist_ok=True)
                    (run/'LAUNCH.json').write_text('{"log_offset":0}')
                    requests=[];clock=[0.0];reads=[0]
                    base='bench: camera frozen map='+entry['map']+'\n[lg/audio-realtime] producer_mix_p95_ms='+str(p95)+' pcm_starved_blocks=0 rejected_sinks=0 dropped_reports=0\n'
                    real_read=Path.read_bytes
                    def read_bytes(path):
                        if str(path)=='/Users/anezium/vita-j0/work/vita.log':
                            reads[0]+=1
                            return (base+('bench: done 120 frames mode=fullbench\nViewer closed cleanly.\n' if reads[0]>1 else '')).encode()
                        return real_read(path)
                    def request(name,*a,**k):requests.append(name);return 'MOCK ONLY'
                    def sleep(seconds):clock[0]+=max(seconds,1)
                    with no_transport(),patch.object(p,'ROOT',temp),patch.object(p,'request',request),patch.object(p,'cmd',lambda *a:None),patch.object(p,'button',lambda *a:None),patch.object(p,'shot',lambda *a:'MOCK IMAGE'),patch.object(Path,'read_bytes',read_bytes),patch.object(time,'monotonic',lambda:clock[0]),patch.object(time,'sleep',sleep),patch.object(sys,'argv',['ui.py',label,action]),contextlib.redirect_stdout(io.StringIO()):
                        ui.main()
                    quit_count=requests.count('vita_quit');should_quit=p95>=8
                    assert quit_count==int(should_quit),(label,action,p95,requests)
                    if action=='resume' and not should_quit:assert reads[0]>=2,'Must observe unfinished bench before completion'
                    record('UI candidate producer stop on both roles',label=label,arm=entry['arm'],action=action,producer_p95_ms=p95,quit_calls=quit_count,expected_quit=should_quit)
        for label in ['reference','fixture-A','fake-reg01-hedge3-A','reg02-hedge3-B-A','reg01-hedge3-A-extra','human-restore']:
            for action in ('begin','resume'):
                with no_transport(),patch.object(p,'request',lambda *a:(_ for _ in ()).throw(AssertionError('UI input before validation'))),patch.object(p,'cmd',lambda *a:(_ for _ in ()).throw(RuntimeError('UI input before validation'))),patch.object(sys,'argv',['ui.py',label,action]):
                    try:ui.main()
                    except AssertionError as error:assert 'Unknown or duplicate protocol label' in str(error)
                    else:raise AssertionError('UI admitted unknown identity')
            record('unknown UI label rejected before input',label=label)
        for marker in ['Viewer failed: incomplete frame-total capture','Frame profile report failed','[lg/audio-realtime] pcm_starved_blocks=1','[lg/audio-realtime] rejected_sinks=1','[lg/audio-realtime] dropped_reports=1','[lg/audio-music] underrun_frames=1','[cpal/vita] underrun_estimates=1']:
            for arm in ('A','B'):
                text='bench: camera frozen map=hedge3\n'+marker+'\n'
                assert health.health_text(text,A=arm=='A'),(arm,marker)
            record('common stop preserved on both arms',marker=marker)
        # SFO change must abort before the first write/delete, including original absence.
        restore=load(root/'restore-files.py','offline_restore_files')
        for originally_present in (True,False):
            sfo=p.A+'sce_sys/param.sfo'
            original=dict(path=sfo,present=originally_present,sha256='old')
            (temp/'BASELINE.json').write_text(json.dumps(dict(manifest=[original],inventories={})))
            def changed_snapshot(*a):return [dict(path=sfo,present=True,sha256='changed')]
            with no_transport(),patch.object(p,'ROOT',temp),patch.object(p,'snapshot',changed_snapshot),patch.object(sys,'argv',['restore-files.py']):
                try:restore.main()
                except AssertionError as error:assert 'SFO changed' in str(error)
                else:raise AssertionError('SFO change admitted')
            record('SFO change stops before mutation',originally_present=originally_present)
    # Reproduce the review's copy layout: ROOT.parent must still be OUT.
    with tempfile.TemporaryDirectory(prefix='round3-helper-layout-',dir=OUT) as copied:
        import shutil
        copied=Path(copied)
        for path in files:shutil.copyfile(path,copied/path.name)
        command=[str(PY),'-c',"import sys; sys.path.insert(0,'.'); import pilot, session, run_health, light_csv"]
        proc=subprocess.run(command,cwd=copied,env=env,capture_output=True,text=True,timeout=15)
        assert proc.returncode==0,proc.stderr
        record('fresh copied layout review import',command=command,cwd=str(copied),returncode=proc.returncode,stdout=proc.stdout,stderr=proc.stderr,temporary_copy_removed=True)
        for path in sorted(copied.glob('*.py')):
            for mode in ('import','help'):
                proc=subprocess.run([str(PY),str(Path(__file__).resolve()),'--root',str(copied),'--probe',str(path),'--mode',mode],cwd=copied,env=env,capture_output=True,text=True,timeout=15)
                assert proc.returncode==0,(path,mode,proc.stderr)
                record('fresh copied layout '+mode,helper=path.name,returncode=proc.returncode,stdout=proc.stdout,stderr=proc.stderr,transport_guard=True)
        stale_pattern=r"reference|NATIVE-PACKAGE|frame_cpu_us'\]\)>0|a1134e"
        proc=subprocess.run(['/usr/bin/grep','-nE',stale_pattern,*map(str,sorted(copied.glob('*.py')))],capture_output=True,text=True)
        assert proc.returncode==1 and not proc.stdout and not proc.stderr
        record('fresh copied layout review grep',pattern=stale_pattern,returncode=proc.returncode,stdout=proc.stdout,stderr=proc.stderr)
    regression=subprocess.run([str(PY),str(OUT/'scripts/test-console-tools.py')],cwd=OUT,env=env,capture_output=True,text=True,timeout=30)
    assert regression.returncode==0,regression.stdout+regression.stderr
    record('matrix regression fixtures',returncode=regression.returncode,stdout=regression.stdout,stderr=regression.stderr)
    contracts=subprocess.run([str(PY),str(OUT/'scripts/test-matrix-contracts.py')],cwd=OUT,env=env,capture_output=True,text=True,timeout=60)
    assert contracts.returncode==0,contracts.stdout+contracts.stderr
    record('matrix identity/arming/budget contracts',returncode=contracts.returncode,stdout=contracts.stdout,stderr=contracts.stderr)
    identities={str(path.relative_to(OUT)):dict(bytes=path.stat().st_size,sha256=sha(path)) for path in sorted((OUT/'packages').rglob('*')) if path.is_file()}
    record('current package identities',files=identities)
    seal=OUT/'ARTIFACT-SHA256.json'
    if seal.exists():
        expected=json.loads(seal.read_text())
        for path,identity in identities.items():assert identity['sha256']==expected[path]
        assert sha(OUT/'proof/CONSOLE-TOOLS-TESTS.json')==expected['proof/CONSOLE-TOOLS-TESTS.json']
        record('sealed packages and regression proof unchanged')
    result=dict(status='PASS',round=3,offline_only=True,synthetic_only=True,device_contact=False,driver_contact=False,receiver_started=False,root=str(root),helper_hashes={str(path.relative_to(OUT)):sha(path) for path in files+[OUT/'scripts/dry-run-console.py',OUT/'scripts/test-console-tools.py']},external_helper_hashes={str(path):sha(path) for path in [EXTERNAL/'vita-stage',EXTERNAL/'vita-ftp',EXTERNAL/'vita_ftp.py']},checks=results,limitations='No console execution or timing measured. Existing driver is not invoked, probed or contacted. All behavioral transport is mocked; import/help probes deny transport and subprocesses. Existing exhaustive restore retained; no fast verifier integration.')
    args.output.write_text(json.dumps(result,indent=2)+'\n')
    print(f'{len(results)} offline checks passed; {args.output}')

if __name__=='__main__':main()
