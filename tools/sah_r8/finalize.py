"""Seal the common-SELF handoff. Never imports transport or driver modules."""
from pathlib import Path
import hashlib,json,os,shutil,subprocess,sys,zipfile
R=Path(__file__).resolve().parents[2];T=R/'tools/sah_r8';O=Path(sys.argv[1]).resolve()
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
for n in ['session-s9-sah','session-s9-sah-load-smoke']:assert not os.path.lexists(O/n)
receipt=json.loads((O/'proof/BUILD-COMMAND.json').read_text());head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()
write(O/'PACKAGE-IDENTITY.json',dict(package='alice-sah-r8',engine_source_revision=receipt['revision'],branch='vita-sah-r8-20261024',branch_head=head))
write(O/'proof/SOURCE-IDENTITY.json',dict(engine_revision=receipt['revision'],tooling_head=head,files={str(f.relative_to(R)):sha(f)for f in sorted((R/'src').rglob('*.rs'))},console_contact=False,driver_contact=False))
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip','w',compression=zipfile.ZIP_DEFLATED,compresslevel=6)as z:
 for n in ['CONSOLE-BRIEF.md','AMENDMENTS.md','CONTRACT.md','READING-RULES.md','CONSOLE-PROTOCOL.json','PACKAGE-MATRIX.json','PACKAGE-IDENTITY.json','MEASUREMENT-LIMITS.md','README.md']:z.write(O/n,n)
 for d in ['configs','console']:
  for f in sorted((O/d).glob('*')):
   if f.is_file():z.write(f,str(f.relative_to(O)))
 for f in sorted(O.glob('stage-*.json')):z.write(f,f.name)
 for arm in ['baseline','sah','baseline-census','sah-census']:
  for n in ['eboot.bin','VERIFICATION.json']:
   f=O/'packages'/arm/n;z.write(f,str(f.relative_to(O)))
 for n in ['log-integrity.py','bind_helpers.py','test_session_e2e.py','test_protocol.py','test_late_callbacks.py']:
  f=O/'scripts'/n;z.write(f,str(f.relative_to(O)))
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip')as z:
 assert not any(part.startswith('session-')for name in z.namelist()for part in Path(name).parts)
 assert z.testzip()is None
 members={i.filename:dict(bytes=i.file_size,sha256=hashlib.sha256(z.read(i)).hexdigest())for i in z.infolist()}
write(O/'proof/PACKAGE-VERIFICATION.json',dict(status='PASS',session_directories_absent=True,common_self=len({sha(O/'packages'/a/'eboot.bin')for a in ['baseline','sah','baseline-census','sah-census']})==1,members=members))
write(O/'ARTIFACT-SHA256.json',{str(f.relative_to(O)):dict(bytes=f.stat().st_size,sha256=sha(f))for f in sorted(O.rglob('*'))if f.is_file()and f.name not in ['ARTIFACT-SHA256.json','SOL-REPORT.md']and not any(p in ('target-vita','tmp','__pycache__')for p in f.relative_to(O).parts)})
print('Sealed common SELF, exact fresh session directories absent, no device action')
