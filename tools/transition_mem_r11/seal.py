"""Seal only local artifacts. Never imports transport or driver code."""
from pathlib import Path
import hashlib,json,shutil,subprocess,sys,zipfile
R=Path(__file__).resolve().parents[2];T=Path(__file__).resolve().parent;O=Path(sys.argv[1]).resolve()
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n',encoding='utf-8')
for directory in ['console','scripts']:
 for source in (T/directory).glob('*.py'):shutil.copyfile(source,O/directory/source.name)
for name in ['AMENDMENT-R11.md','CONSOLE-BRIEF.md']:shutil.copyfile(T/name,O/name)
assert all(not (O/n).exists() for n in ['session-s11-transition-mem','session-s11-transition-mem-smoke','session-s8-cache-qual','session-s10-stack'])
identity=json.loads((O/'PACKAGE-IDENTITY.json').read_text());identity['branch_head']=subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip();write(O/'PACKAGE-IDENTITY.json',identity)
rows=[json.loads((O/'packages'/arm/'VERIFICATION.json').read_text()) for arm in ['baseline','stack']]
assert len({v['self_sha256'] for v in rows})==1
for v in rows:
 assert v['source_revision']==identity['engine_source_revision'] and sha(O/'configs'/f"PL-{v['arm']}.json")==v['config_sha256']
write(O/'PACKAGE-MATRIX.json',dict(status='BUILT_CONSOLE_NOT_EXECUTED',packages=rows))
# Check the grammar against actual source, not documentation or assumptions.
cmd=Path('/Users/anezium/vita-work/vitacompanion-hash/src/cmd_definitions.c');s=cmd.read_text()
for name in ['help','nosleep','press','quit','release','screen','version']:
 assert f'.name = "{name}"' in s,name
assert 'Error: cannot quit the app. Is the TITLEID correct?' in s and 'strcpy(res_msg, "Quit.\\n")' in s
write(O/'proof/COMMAND-GRAMMAR.json',dict(status='PASS',source=str(cmd),source_sha256=sha(cmd),native_commands=['help','nosleep','press','quit','release','screen','version'],wait_is_driver_sequencer_syntax=True,console_contact=False))
members=['CONSOLE-BRIEF.md','AMENDMENT-R11.md','CONSOLE-PROTOCOL.json','PACKAGE-IDENTITY.json','PACKAGE-MATRIX.json']
members += [str(f.relative_to(O)) for directory in ['console','configs','scripts'] for f in sorted((O/directory).glob('*')) if f.is_file()]
members += [p.name for p in sorted(O.glob('stage-*.json'))]
members += [f'packages/{arm}/{n}' for arm in ['baseline','stack'] for n in ['eboot.bin','VERIFICATION.json']]
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip','w',zipfile.ZIP_DEFLATED) as z:
 for name in members:z.write(O/name,name)
assert all(not name.startswith('session-') for name in zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip').namelist())
write(O/'proof/PACKAGE-VERIFICATION.json',dict(status='PASS',console_executed=False,session_directories_absent=True,zip_has_no_session_directory=True,common_self_sha256=rows[0]['self_sha256'],zip_sha256=sha(O/'CONSOLE-PACKAGE.zip'),members=members))
print('PASS sealed local package; no console execution',flush=True)
