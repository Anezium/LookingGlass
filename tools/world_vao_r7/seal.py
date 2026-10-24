"""Verify common native artifacts and record the four config arms. Mac only."""
from pathlib import Path
import hashlib,json,shutil,subprocess,sys,zipfile
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();B=O/'packages/baseline';SDK=Path('/Users/anezium/vitasdk/bin')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def git(*a):return subprocess.check_output(['git',*a],cwd=R,text=True).strip()
def run(tag,cmd):
 p=subprocess.run(cmd,capture_output=True,text=True);(O/'logs'/tag).write_text(p.stdout+p.stderr);assert p.returncode==0,(cmd,p.stderr);return p.stdout
receipt=json.loads((O/'proof/BUILD-COMMAND.json').read_text());assert receipt['exit_code']==0
run('elf-margin.log',[sys.executable,str(O/'scripts/verify-elf-margin.py'),str(B/'looking-glass.elf')])
run('process-params.log',[sys.executable,str(O/'scripts/verify-process-params.py'),str(B/'looking-glass.elf')])
run('symbols.log',[str(SDK/'arm-vita-eabi-nm'),'-C',str(B/'looking-glass.elf')])
symbols=(O/'logs/symbols.log').read_text();assert 'looking_glass::render_tail::hooks' not in symbols and 'looking_glass::world_input_proof' not in symbols
assert not git('diff','6270497',receipt['revision'],'--','build.rs','.cargo','src/logging.rs','src/render_direct.rs')
run('regenerate-velf.log',[str(SDK/'vita-elf-create'),str(B/'looking-glass.elf'),str(B/'regenerated.velf')])
run('regenerate-self.log',[str(SDK/'vita-make-fself'),'-s',str(B/'regenerated.velf'),str(B/'regenerated.self')])
assert sha(B/'regenerated.velf')==sha(B/'looking-glass.velf');assert sha(B/'regenerated.self')==sha(B/'eboot.bin')
with zipfile.ZipFile(B/'looking-glass.vpk') as z:
 members=[{'name':i.filename,'bytes':i.file_size} for i in z.infolist()];assert {i['name'] for i in members}=={'eboot.bin','sce_sys/param.sfo'}
 assert hashlib.sha256(z.read('eboot.bin')).hexdigest()==sha(B/'eboot.bin')
(O/'proof/VPK-CONTENTS.json').write_text(json.dumps(members,indent=2)+'\n')
rows=[];base=(O/'configs/PL-baseline.json').read_bytes();assert base==Path('/Users/anezium/vita-j0/resume-2026-10-24/alice-phase-census-r5/configs/PL-baseline.json').read_bytes();a=json.loads(base)
for arm in ['baseline','vao','baseline-census','vao-census']:
 d=O/'packages'/arm;d.mkdir(exist_ok=True)
 if arm!='baseline':shutil.copy2(B/'eboot.bin',d/'eboot.bin')
 c=json.loads((O/'configs'/f'PL-{arm}.json').read_text());diff={k:[a.get(k,False),c.get(k,False)] for k in a.keys()|c.keys() if a.get(k,False)!=c.get(k,False)}
 expected={}
 if arm.startswith('vao'):expected['world_vertex_input']=[False,True]
 if arm.endswith('census'):expected['phase_census']=[False,True]
 assert diff==expected
 v=dict(arm=arm,revision=receipt['revision'],source_revision=receipt['revision'],branch=git('branch','--show-current'),build_command=receipt['argv'],build_environment=receipt['environment'],self_sha256=sha(d/'eboot.bin'),self_bytes=(d/'eboot.bin').stat().st_size,elf_sha256=sha(B/'looking-glass.elf'),velf_sha256=sha(B/'looking-glass.velf'),vpk_sha256=sha(B/'looking-glass.vpk'),lock_sha256=sha(R/'Cargo.lock'),config_path=f'configs/PL-{arm}.json',config_sha256=sha(O/'configs'/f'PL-{arm}.json'),exact_effective_switch_diff=diff,common_binary=True,native_build_count=2,regenerated_self_byte_equal=True,regenerated_velf_byte_equal=True,elf_margin_pass=True,process_parameters_pass=True,zero_render_observer_hooks=True,console_access=False,runtime_performance='UNMEASURED',miniquad_revision='2dbf0416118f5ffab6cdab482888f13cf3810d28',vitaGL_sha256=sha(Path('/Users/anezium/vitasdk/arm-vita-eabi/lib/libvitaGL.a')))
 (d/'VERIFICATION.json').write_text(json.dumps(v,indent=2)+'\n');rows.append(v)
(O/'PACKAGE-MATRIX.json').write_text(json.dumps(dict(status='BUILT_CONSOLE_NOT_EXECUTED',packages=rows),indent=2)+'\n')
print(json.dumps(rows[0],indent=2))
