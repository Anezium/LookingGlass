"""New Game owns simultaneous Retry/F9 input, through the actual viewer."""
from pathlib import Path
import sys,json,hashlib
source=Path(__file__).with_name('host_proof.py');s=source.read_text();prefix=s[:s.index('# Seed the second native save')]
ns={'__file__':str(source),'__name__':'nested_proof'};sys.argv=[str(source),sys.argv[1],'--binary',sys.argv[2]]
exec(compile(prefix,str(source),'exec'),ns)
rows=[]
for on in [False,True]:
 reference,_=ns['run'](f'nested-reference-{int(on)}','hedge3','gvillage','new-game',on)
 for action in ['new-game+retry','new-game+quick-load']:
  actual,_=ns['run'](f'nested-{action}-{int(on)}','hedge3','gvillage',action,on)
  assert actual['states']==reference['states'],(on,action)
  assert [r['kind'] for r in actual['records']]==['new-game','quit']
  rows.append(dict(levers=on,simultaneous=action,semantics='Confirmed New Game wins; conflicting Retry/F9 discarded',game_state_exact=True,no_nested_bracket=True))
 for conflict in ['retry','quick-load']:
  ns['env']['LOOKING_GLASS_TRANSITION_PLAN']=json.dumps([(2,'new-game','gvillage'),(3,conflict,'hedge3')])
  actual,_=ns['run'](f'nested-active-{conflict}-{int(on)}','hedge3','gvillage','new-game',on)
  ns['env'].pop('LOOKING_GLASS_TRANSITION_PLAN')
  assert actual['states']==reference['states'],(on,conflict,'input during active New Game')
  assert [r['kind'] for r in actual['records']]==['new-game','quit']
  rows.append(dict(levers=on,simultaneous=conflict,phase='active New Game bracket after throttled confirmation',game_state_exact=True,no_nested_bracket=True))
(ns['O']/'proof/NESTED-TRANSITIONS.json').write_text(json.dumps(dict(status='PASS',binary_sha256=hashlib.sha256(ns['B'].read_bytes()).hexdigest(),rows=rows),indent=2)+'\n')
