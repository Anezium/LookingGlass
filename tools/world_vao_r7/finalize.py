"""Seal local handoff bytes and verify the exact vacant session path. No transport."""
from pathlib import Path
import hashlib,json,os,shutil,subprocess,sys,zipfile
R=Path(__file__).resolve().parents[2];T=R/'tools/world_vao_r7';O=Path(sys.argv[1]).resolve()
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
def git(*a):return subprocess.check_output(['git',*a],cwd=R,text=True).strip()
assert not os.path.lexists(O/'session-s8-world-vao')
assert not git('diff','--','src','Cargo.toml','Cargo.lock','build.rs','.cargo')
build=json.loads((O/'proof/BUILD-COMMAND.json').read_text());engine=build['revision'];head=git('rev-parse','HEAD')
assert not git('diff',engine,'HEAD','--','src','Cargo.toml','Cargo.lock','build.rs','.cargo')
for f in T.glob('*.py'):shutil.copy2(f,O/'scripts'/f.name)
for n in ['CONSOLE-BRIEF.md','DESIGN.md','README.md']:shutil.copy2(T/n,O/n)
write(O/'PACKAGE-IDENTITY.json',dict(package='alice-world-vao-r7',engine_source_revision=engine,branch='vita-world-vao-r7-20261024',branch_head=head))
write(O/'proof/SOURCE-IDENTITY.json',dict(engine_revision=engine,tooling_head=head,base_branch='vita-phase-census-r5-20261024',base_engine='62704971576f7e3d30499962388b69bf2ed692d7',miniquad_revision='2dbf0416118f5ffab6cdab482888f13cf3810d28',files={str(f.relative_to(R)):sha(f) for f in sorted((R/'src').rglob('*.rs'))},host_binary_sha256=json.loads((O/'proof/HOST-RENDER.json').read_text())['binary_sha256']))
# Remove Python caches before listing archive/manifest members.
for root in [T,O]:
 for p in list(root.rglob('__pycache__')):shutil.rmtree(p)
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip','w',compression=zipfile.ZIP_DEFLATED,compresslevel=6) as z:
 for n in ['CONSOLE-BRIEF.md','DESIGN.md','README.md','CONSOLE-PROTOCOL.json','PACKAGE-MATRIX.json','PACKAGE-IDENTITY.json']:z.write(O/n,n)
 for d in ['configs','console']:
  for f in sorted((O/d).glob('*')):
   if f.is_file():z.write(f,str(f.relative_to(O)))
 for f in sorted(O.glob('stage-*.json')):z.write(f,f.name)
 for arm in ['baseline','vao','baseline-census','vao-census']:
  for n in ['eboot.bin','VERIFICATION.json']:
   f=O/'packages'/arm/n;z.write(f,str(f.relative_to(O)))
 for n in ['log-integrity.py','bind_helpers.py','test_session_e2e.py','test_protocol.py']:
  f=O/'scripts'/n;z.write(f,str(f.relative_to(O)))
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip') as z:
 assert z.testzip() is None
 assert not any(part.startswith('session-') for n in z.namelist() for part in Path(n).parts)
 members={i.filename:dict(bytes=i.file_size,sha256=hashlib.sha256(z.read(i)).hexdigest()) for i in z.infolist()}
 assert all((O/n).stat().st_size==v['bytes'] and sha(O/n)==v['sha256'] for n,v in members.items())
write(O/'proof/PACKAGE-VERIFICATION.json',dict(status='PASS',session_directory='session-s8-world-vao',session_directory_absent=True,zip_has_no_session_directory=True,zip_crc_pass=True,common_self=len({sha(O/'packages'/a/'eboot.bin') for a in ['baseline','vao','baseline-census','vao-census']})==1,members=members))
manifest={str(f.relative_to(O)):dict(bytes=f.stat().st_size,sha256=sha(f)) for f in sorted(O.rglob('*')) if f.is_file() and f.name not in ['ARTIFACT-SHA256.json','SOL-REPORT.md']}
write(O/'ARTIFACT-SHA256.json',manifest)
print('Sealed',len(manifest),'files, verified',len(members),'ZIP members; exact session path absent')
