"""Seal the common-SELF handoff. Never imports transport or driver modules."""
from pathlib import Path
import argparse,hashlib,importlib.util,json,os,shutil,subprocess,sys,zipfile
parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('package',type=Path);parser.add_argument('--main-admission',action='store_true');args=parser.parse_args()
R=Path(__file__).resolve().parents[2];T=R/'tools/sah_r8';O=args.package.resolve()
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
assert not os.path.lexists(O/'session-s9-sah')
if args.main_admission:
 spec=importlib.util.spec_from_file_location('seal_smoke_admission',O/'console/smoke_admission.py');smoke=importlib.util.module_from_spec(spec);spec.loader.exec_module(smoke)
 smoke_receipt=smoke.inspect(O,O/'session-s9-sah-load-smoke')
 e2e=json.loads((O/'proof/SESSION-E2E-MAIN-ADMISSION.json').read_text())
 assert e2e['status']=='PASS' and e2e['mode']=='main-admission' and e2e['real_package_session_path_absent'] and e2e['real_smoke_inputs_unchanged'] and e2e['final_status']=='PASS_ORIGINAL_STATE_VERIFIED'
 assert e2e['real_smoke_admission']==smoke_receipt
 assert json.loads((O/'proof/MAIN-ADMISSION-NEGATIVE-FIXTURES.json').read_text())['status']=='PASS'
else:assert not os.path.lexists(O/'session-s9-sah-load-smoke')
receipt=json.loads((O/'proof/BUILD-COMMAND.json').read_text());head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()
if args.main_admission:
 source=json.loads((O/'proof/SOURCE-IDENTITY.json').read_text());source['main_admission_tooling_head']=head
 source['main_admission_tool_sha256']={str(f.relative_to(T)):sha(f) for f in sorted(T.rglob('*')) if f.is_file() and '__pycache__' not in f.parts}
 write(O/'proof/SOURCE-IDENTITY.json',source)
else:
 write(O/'PACKAGE-IDENTITY.json',dict(package='alice-sah-r8',engine_source_revision=receipt['revision'],branch='vita-sah-r8-20261024',branch_head=head))
 write(O/'proof/SOURCE-IDENTITY.json',dict(engine_revision=receipt['revision'],tooling_head=head,files={str(f.relative_to(R)):sha(f)for f in sorted((R/'src').rglob('*.rs'))},console_contact=False,driver_contact=False))
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip','w',compression=zipfile.ZIP_DEFLATED,compresslevel=6)as z:
 for n in ['CONSOLE-BRIEF.md','AMENDMENTS.md','CONTRACT.md','READING-RULES.md','CONSOLE-PROTOCOL.json','PACKAGE-MATRIX.json','PACKAGE-IDENTITY.json','MEASUREMENT-LIMITS.md','README.md']:z.write(O/n,n)
 if (O/'AMENDMENTS-3.md').exists():z.write(O/'AMENDMENTS-3.md','AMENDMENTS-3.md')
 for d in ['configs','console']:
  for f in sorted((O/d).glob('*')):
   if f.is_file():z.write(f,str(f.relative_to(O)))
 for f in sorted(O.glob('stage-*.json')):z.write(f,f.name)
 for arm in ['baseline','sah','baseline-census','sah-census']:
  for n in ['eboot.bin','VERIFICATION.json']:
   f=O/'packages'/arm/n;z.write(f,str(f.relative_to(O)))
 for n in ['log-integrity.py','bind_helpers.py','test_session_e2e.py','test_protocol.py','test_late_callbacks.py','test_main_admission.py','finalize.py']:
  f=O/'scripts'/n;z.write(f,str(f.relative_to(O)))
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip')as z:
 assert not any(part.startswith('session-')for name in z.namelist()for part in Path(name).parts)
 assert not any(name.startswith(('main-admission-attempt/','smoke-admission-verification/')) for name in z.namelist())
 assert z.testzip()is None
 members={i.filename:dict(bytes=i.file_size,sha256=hashlib.sha256(z.read(i)).hexdigest())for i in z.infolist()}
assert all(sha(O/name)==row['sha256'] for name,row in members.items())
common_self=len({sha(O/'packages'/a/'eboot.bin')for a in ['baseline','sah','baseline-census','sah-census']})==1
assert common_self
write(O/'proof/PACKAGE-VERIFICATION.json',dict(status='PASS',mode='main-admission' if args.main_admission else 'full',session_directories_absent=True,session_directories_absent_scope='ZIP members only',source_smoke_session_present=(O/'session-s9-sah-load-smoke').is_dir(),source_main_session_absent=True,evidence_policy='Excluded from ZIP; non-cache evidence files recorded in ARTIFACT-SHA256.json',common_self=common_self,zip_sha256=sha(O/'CONSOLE-PACKAGE.zip'),members=members))
if args.main_admission:
 final=json.loads((O/'proof/FINAL-VERIFICATION.json').read_text())
 final.update(mode='main-admission',session_directories_absent=False,source_main_session_absent=True,source_smoke_session_verified=True,zip_session_directories_absent=True,sealed_members_match_local_files=True,common_self=common_self,main_admission_e2e_sha256=sha(O/'proof/SESSION-E2E-MAIN-ADMISSION.json'),main_admission_negative_fixtures_sha256=sha(O/'proof/MAIN-ADMISSION-NEGATIVE-FIXTURES.json'),console_executed=False,driver_contact=False)
 write(O/'proof/FINAL-VERIFICATION.json',final)
write(O/'ARTIFACT-SHA256.json',{str(f.relative_to(O)):dict(bytes=f.stat().st_size,sha256=sha(f))for f in sorted(O.rglob('*'))if f.is_file()and f.name not in ['ARTIFACT-SHA256.json','SOL-REPORT.md','E2E-MAIN-FIX.md']and not any(p in ('target-vita','tmp','__pycache__')for p in f.relative_to(O).parts)})
print('Sealed common SELF, ZIP excludes session evidence, source main path absent; no device action')
