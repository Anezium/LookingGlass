"""Seal handoff bytes. No networking or console helper execution."""
from pathlib import Path
import argparse, ast, hashlib, json, os, shutil, subprocess, zipfile
R=Path(__file__).resolve().parents[2]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n',encoding='utf-8')
def main():
    a=argparse.ArgumentParser(description=__doc__);a.add_argument('package',type=Path);a.add_argument('--clean',action='store_true');v=a.parse_args();o=v.package.resolve()
    assert not os.path.lexists(o/'session-s8-cache-qual')
    for f in (o/'console').glob('*.py'):ast.parse(f.read_text(),filename=str(f))
    head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()
    build=json.loads((o/'proof/BUILD-COMMAND.json').read_text());assert build['exit_code']==0
    identity=o/'PACKAGE-IDENTITY.json'
    if identity.exists():assert json.loads(identity.read_text())['sealed_brief_sha256']==sha(o/'CONSOLE-BRIEF.md'),'Never edit the sealed brief; use a separate amendment'
    write(identity,dict(package='alice-cache-qual-r8',engine_source_revision=build['revision'],branch='vita-cache-qual-r8-20261024',branch_head=head,sealed_brief_sha256=sha(o/'CONSOLE-BRIEF.md'),console_status='NOT_EXECUTED'))
    write(o/'proof/SOURCE-IDENTITY.json',dict(engine_revision=build['revision'],tooling_head=head,base='73e303c49e62338c6cc3b603259eb11ae02bee19',source_sha256={str(f.relative_to(R)):sha(f) for f in sorted((R/'src').rglob('*.rs'))}))
    removed=[]
    if v.clean:
        df_before=subprocess.check_output(['df','-h',str(o)],text=True)
        for root in [o,R/'tools/cache_qual_r8']:
            for f in list(root.rglob('__pycache__')):
                removed.append(dict(path=str(f),bytes=sum(p.stat().st_size for p in f.rglob('*') if p.is_file())));shutil.rmtree(f)
        for f in [o/'target-vita',o/'tmp']:
            if f.exists():removed.append(dict(path=str(f),bytes=sum(p.stat().st_size for p in f.rglob('*') if p.is_file())));shutil.rmtree(f)
        old=json.loads((o/'proof/CLEANUP.json').read_text()) if (o/'proof/CLEANUP.json').exists() else {}
        removed=old.get('removed',[])+removed
        write(o/'proof/CLEANUP.json',dict(removed=removed,removed_bytes=sum(r['bytes'] for r in removed),df_before=old.get('df_before',df_before),df_after=subprocess.check_output(['df','-h',str(o)],text=True),shared_host_target_retained=True))
    members=['CONSOLE-BRIEF.md','DESIGN.md','CONSOLE-PROTOCOL.json','PACKAGE-IDENTITY.json','PACKAGE-MATRIX.json']
    members += [p.name for p in sorted(o.glob('*AMENDMENT*.md'))]
    for d in ['console','configs']:members += [str(f.relative_to(o)) for f in sorted((o/d).glob('*')) if f.is_file()]
    members += [p.name for p in sorted(o.glob('stage-*.json'))]
    for arm in ['baseline','cache']:
        for name in ['eboot.bin','VERIFICATION.json']:members.append(f'packages/{arm}/{name}')
    members += ['scripts/'+n for n in ['log-integrity.py','bind_helpers.py','test_session_e2e.py']]
    assert len(members)==len(set(members))
    assert not any(any(part.startswith('session-') for part in Path(p).parts[:-1]) or Path(p).suffix.lower() in ['.pk3','.bsp','.wav','.mp3','.tik','.tga','.jpg','.png'] for p in members)
    with zipfile.ZipFile(o/'CONSOLE-PACKAGE.zip','w',compression=zipfile.ZIP_DEFLATED,compresslevel=6) as z:
        for name in members:z.write(o/name,name)
    with zipfile.ZipFile(o/'CONSOLE-PACKAGE.zip') as z:
        assert z.testzip() is None
        assert set(z.namelist())==set(members)
        for name in members:assert z.read(name)==(o/name).read_bytes()
    write(o/'proof/PACKAGE-VERIFICATION.json',dict(status='PASS',session_directory='session-s8-cache-qual',session_directory_absent=True,zip_has_no_session_directory=True,
        common_self=sha(o/'packages/baseline/eboot.bin')==sha(o/'packages/cache/eboot.bin'),
        members={name:dict(bytes=(o/name).stat().st_size,sha256=sha(o/name)) for name in members},zip_sha256=sha(o/'CONSOLE-PACKAGE.zip')))
    write(o/'ARTIFACT-SHA256.json',{str(f.relative_to(o)):dict(bytes=f.stat().st_size,sha256=sha(f)) for f in sorted(o.rglob('*')) if f.is_file() and f.name not in ['ARTIFACT-SHA256.json','SOL-REPORT.md']})
    print('Sealed',len(members),'members; exact session directory absent')
if __name__=='__main__':main()
