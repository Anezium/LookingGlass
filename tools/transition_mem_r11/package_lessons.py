"""Prove r11b preservation/closure helpers and sealed documents survived r11c."""
from pathlib import Path
import hashlib,json,subprocess,sys,zipfile
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();old=O.with_name('alice-transition-mem-r11')
sha=lambda b:hashlib.sha256(b).hexdigest();rows=[]
names=['CONSOLE-BRIEF.md','SOL-REPORT.md','AMENDMENT-R11.md','AMENDMENT-R11B.md']
names += [f'configs/{n}' for n in ['PL-baseline.json','PL-stack.json','vita-sim-all.json','native-timing-disabled.json','profile_mode-full.txt']]
names += [f'console/{n}.py' for n in ['title_safety','restore-files','restore-ledgers','restore-arm','verify-self','ack','baseline-r3','save-proof','pilot','verify-human-entry','map_identity','session','preserve','admit','operate','check-session-budget','host-log-receiver','verify-arm-self']]
for name in names:
 before=(old/name).read_bytes();after=(O/name).read_bytes();assert before==after,name
 rows.append(dict(file=name,sha256=sha(after),byte_equal=True))
def subset(a,b,path=''):
 if isinstance(a,dict):
  for k,v in a.items():assert k in b,(path,k);subset(v,b[k],path+'/'+k)
 else:assert a==b,path
subset(json.loads((old/'CONSOLE-PROTOCOL.json').read_text()),json.loads((O/'CONSOLE-PROTOCOL.json').read_text()))
with zipfile.ZipFile(O/'CONSOLE-PACKAGE.zip') as z:
 assert not any(n.startswith('session-') or n.lower().endswith(('.pk3','.bsp','.wav','.tga')) for n in z.namelist())
 for name in z.namelist():assert z.read(name)==(O/name).read_bytes(),name
assert not any(p.name.startswith('session-') for p in O.iterdir())
(O/'proof/PACKAGE-LESSONS-R11C.json').write_text(json.dumps(dict(status='PASS',unchanged_files=rows,protocol_changes_additive_only=True,zip_all_members_byte_equal=True,session_directories_absent=True),indent=2)+'\n')
print('PASS r11b sealed documents, configs, closure/preservation helpers and ZIP bytes')
