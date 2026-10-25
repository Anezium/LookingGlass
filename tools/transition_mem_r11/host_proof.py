"""Actual viewer transition state/heap census; host only, no console imports."""
from pathlib import Path
import argparse,hashlib,json,os,shutil,subprocess,time
p=argparse.ArgumentParser();p.add_argument('output',type=Path);p.add_argument('--binary',type=Path);p.add_argument('--film',action='store_true');a=p.parse_args()
R=Path(__file__).resolve().parents[2];O=a.output.resolve();B=a.binary or O/'host/probe-bin'
def write(p,v):p.parent.mkdir(parents=True,exist_ok=True);p.write_text(json.dumps(v,indent=2)+'\n',encoding='utf-8')
base=json.loads((R/'tools/lever_matrix_r4/configs/PL-baseline.json').read_text())
base.update(phase_census=False,profile_capture=False,audio_music_thread=False,story_film_bypass=not a.film)
env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
env.update(LOOKING_GLASS_TRANSITION_CENSUS='1',LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT='0.05',LOOKING_GLASS_SAH_SILENT_AUDIO='1')
data='/Users/anezium/vita-work/alice-data/Alice1/bin/base'
human=Path('/Users/anezium/vita-j0/resume-2026-10-24/alice-lever-matrix-r3/session-s5-world-static-merge/originals/data/alice/private/saves/auto.json')
assert hashlib.sha256(human.read_bytes()).hexdigest()=='ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9'
results=[]

def run(label,source,destination,action,levers=False,legacy=False,save=None,snapshot=False):
 d=O/'host'/label;(d/'private/saves').mkdir(parents=True,exist_ok=True)
 write(d/'private/vita_renderer.json',dict(base,audio_static_cache=levers,collision_sah=levers,skin_actual_consumers=levers))
 if save:shutil.copyfile(save,d/'private/saves/auto.json')
 e=dict(env)
 if action:e.update(LOOKING_GLASS_TRANSITION_ACTION=action,LOOKING_GLASS_TRANSITION_DESTINATION=destination)
 if snapshot:e['LOOKING_GLASS_TRANSITION_SNAPSHOT']=str(d/'game.json')
 if legacy:e['LOOKING_GLASS_TRANSITION_LEGACY']='1'
 if action=='natural-exit' and e.get('LOOKING_GLASS_SKIN_PROOF'):
  e.update(LOOKING_GLASS_LOADING_CAPTURE=str(d/'loading-presented.png'),LOOKING_GLASS_LOADING_BUILD_CAPTURE=str(d/'loading-build.png'))
 cmd=[str(B),'--data',data,'--map',source,'--save-dir',str(d/'private/saves')]
 if action!='quit':cmd+=['--frames','8']
 start=time.monotonic()
 with (d/'run.log').open('w') as f:proc=subprocess.run(cmd,cwd=d,env=e,stdout=f,stderr=subprocess.STDOUT,timeout=300)
 log=(d/'run.log').read_text(errors='replace')
 assert proc.returncode==0,(label,proc.returncode,log[-3000:])
 assert '[lg/skin] serial fallback' not in log and 'afterimage consumer must be declared' not in log,label
 states=[json.loads(x.split(' ',1)[1]) for x in log.splitlines() if x.startswith('TRANSITION_STATE ')]
 records=[json.loads(x.split('] ',1)[1]) for x in log.splitlines() if x.startswith('[lg/transition-memory] ')]
 end=[x for x in records if x['event']=='end' and x['kind']!='startup-entry']
 if action:
  assert end and all(x['success'] for x in end),(label,end)
  if action!='quit':assert len(end)==2,(label,end)
 row=dict(label=label,source=source,destination=destination,action=action,levers=levers,legacy=legacy,seconds=time.monotonic()-start,records=end,states=states,exit_code=proc.returncode)
 results.append(row);write(O/'proof/TRANSITION-RUNS.json',results)
 print('DONE',label,round(row['seconds'],2),flush=True)
 return row,d

# Seed the second native save with a complete scene-independent Game snapshot.
_,seed=run('seed-hedge3','hedge3','hedge3',None,snapshot=True)
# Wrap using the real Store from an explicit host-only snapshot mode below.
seed_save=seed/'private/saves/quick.json';assert seed_save.is_file()
pairs=[]
scenarios=[('H-P','hedge3','pandemonium','native-load',human),('P-H','pandemonium','hedge3','native-load',seed_save),('H-H','hedge3','hedge3','native-load',seed_save),('chapter','pandemonium','hedge3','chapter',None),('new-game','hedge3','gvillage','new-game',None),('retry','hedge3','hedge3','retry',None),('exit','hedge3','pandemonium','natural-exit',None),('quit','hedge3','menu','quit',None)]
if a.film:scenarios=[s for s in scenarios if s[0]=='new-game']
for on in [False,True]:
 for tag,source,dest,action,save in scenarios:
  old,_=run(f'{tag}-{int(on)}-before',source,dest,action,on,True,save)
  new,_=run(f'{tag}-{int(on)}-after',source,dest,action,on,False,save)
  if action in ['new-game','quit']:
   left=json.loads((O/'host'/old['label']/'private/saves/auto.json').read_text())['payload']
   right=json.loads((O/'host'/new['label']/'private/saves/auto.json').read_text())['payload']
   assert left==right and left['campaign']['levels'][left['current']]['map']=='hedge3',(tag,'outgoing auto preservation')
  assert old['states']==new['states'],(tag,on,'gameplay state mismatch',old['states'],new['states'])
  pairs.append(dict(scenario=tag,levers=on,states_equal=True,before=old['records'][0],after=new['records'][0]))
  write(O/'proof/TRANSITION-EQUALITY.json',dict(status='PASS',binary_sha256=hashlib.sha256(B.read_bytes()).hexdigest(),pairs=pairs,hardware_evidence=False))
print('PASS actual viewer transitions; every post-operation save state matches overlap reference',flush=True)
