"""Every live path/build stage: retained outgoing reference versus reconstruction."""
from pathlib import Path
import sys,json,os,subprocess,shutil,hashlib
from PIL import Image
O=Path(sys.argv[1]).resolve();B=Path(sys.argv[2]).resolve();R=Path(__file__).resolve().parents[2]
base=json.loads((R/'tools/lever_matrix_r4/configs/PL-baseline.json').read_text());base.update(phase_census=False,profile_capture=False,audio_music_thread=False,story_film_bypass=True)
human=Path('/Users/anezium/vita-j0/resume-2026-10-24/alice-lever-matrix-r3/session-s5-world-static-merge/originals/data/alice/private/saves/auto.json');seed=O/'host/seed-hedge3/private/saves/quick.json';rows=[]
def run(on,action,stage,legacy,double=False):
 d=O/'failure'/f'{action}-{stage}-{int(on)}-{int(legacy)}{"-double" if double else ""}';(d/'private/saves').mkdir(parents=True,exist_ok=True)
 (d/'private/vita_renderer.json').write_text(json.dumps(dict(base,audio_static_cache=on,collision_sah=on,skin_actual_consumers=on)))
 shutil.copyfile(seed,d/'private/saves/quick.json');shutil.copyfile(human,d/'private/saves/auto.json')
 env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')};env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT='0.05',LOOKING_GLASS_SAH_SILENT_AUDIO='1',LOOKING_GLASS_SKIN_PROOF='1',LOOKING_GLASS_TRANSITION_ACTION=action,LOOKING_GLASS_TRANSITION_DESTINATION='pandemonium',LOOKING_GLASS_TRANSITION_FAIL_BUILD=stage)
 if legacy:env['LOOKING_GLASS_TRANSITION_LEGACY']='1'
 if double and not legacy:env['LOOKING_GLASS_TRANSITION_FAIL_RECOVERY']='once'
 argv=[str(B),'--data','/Users/anezium/vita-work/alice-data/Alice1/bin/base','--map','hedge3','--frames','8','--save-dir',str(d/'private/saves'),'--capture',str(d/'final.png')]
 with (d/'run.log').open('w') as f:p=subprocess.run(argv,cwd=d,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=180)
 log=(d/'run.log').read_text()
 if double and not legacy:
  assert 'host outgoing recovery fault' in log, 'Recovery fault not reached'
  assert '[lg/transition-recovery] outgoing rebuild failed attempt=1' in log
 assert p.returncode==0,(on,action,stage,legacy,p.returncode,log[-2000:])
 states=[json.loads(x.split(' ',1)[1]) for x in log.splitlines() if x.startswith('TRANSITION_STATE ')]
 frames=[json.loads(x.split(' ',1)[1]) for x in log.splitlines() if x.startswith('SIM_MAPS_FRAME ')]
 assert len(states)==8 and all(s['map']=='hedge3' for s in states),(action,stage,'outgoing live state lost')
 end=[json.loads(x.split('] ',1)[1]) for x in log.splitlines() if x.startswith('[lg/transition-memory] ') and '"event":"end"' in x]
 if stage!='invalid-level':
  assert f'host transition fault at {stage}' in log,(action,stage,'fault not reached')
  assert any(e['kind'] not in ['startup-entry','quit'] and not e['success'] and e['released'] for e in end)
  if not legacy:
   audio=next(l for l in log.splitlines() if l.startswith('[lg/transition-audio] '))
   import re
   values=re.findall(r'(?:before|after)_ns=(\S+)',audio)
   assert len(values)==2 and values[0]==values[1],(action,stage,'music decoder clock restarted',audio)
 else:
  assert 'Load failed: Saved current level is missing' in log
  assert not any(e['kind']=='native-load' for e in end),'Invalid level must fail before release'
 return dict(states=states,frames=frames,dir=d)
for on in [False,True]:
 for action in ['chapter','natural-exit','retry','quick-load','native-load']:
  stages=['level','scene','npcs','steam','level_art'] if action in ['chapter','natural-exit'] else ['prepare','scene','logic','npcs','alice','steam']
  if action=='native-load':stages+=['invalid-level']
  for stage in stages:
   left=run(on,action,stage,True);right=run(on,action,stage,False)
   assert left['states']==right['states'],(on,action,stage,'failed transition changed Game state')
   for x,y in zip(left['frames'],right['frames']):
    def packets(f):return [{k:v for k,v in p.items() if k!='gpu_slot'} for p in f['submitted_packets']]
    assert packets(x)==packets(y),(on,action,stage,'failed transition changed render packets')
   assert Image.open(left['dir']/'final.png').tobytes()==Image.open(right['dir']/'final.png').tobytes(),(on,action,stage,'failed transition final PNG')
   rows.append(dict(levers=on,path=action,stage=stage,frames=8,state_equal=True,ordered_packets_equal=True,final_png_equal=True,music_decoder_clock_retained=True,partial_destination_failure_recovered=stage!='invalid-level'))
   (O/'proof/FAILED-TRANSITION-EQUALITY.json').write_text(json.dumps(dict(status='PASS',reference='actual failed builder with original outgoing resources retained; no recovery rebuild in reference',binary_sha256=hashlib.sha256(B.read_bytes()).hexdigest(),rows=rows),indent=2)+'\n')
   print('PASS recovery',on,action,stage,flush=True)
print('PASS every live path and build-stage fault; exact Game, packets and PNG')
double_rows=[]
for on in [False,True]:
 for action in ['chapter','natural-exit','retry','quick-load','native-load']:
  left=run(on,action,'scene',True,True);right=run(on,action,'scene',False,True)
  assert left['states']==right['states'], (on,action,'double failure lost outgoing Game')
  for x,y in zip(left['frames'],right['frames']):
   assert [{k:v for k,v in p.items() if k!='gpu_slot'} for p in x['submitted_packets']]==[{k:v for k,v in p.items() if k!='gpu_slot'} for p in y['submitted_packets']]
  assert Image.open(left['dir']/'final.png').tobytes()==Image.open(right['dir']/'final.png').tobytes()
  double_rows.append(dict(levers=on,path=action,destination_failed=True,first_outgoing_rebuild_failed=True,viewer_continued=True,frames=8,state_equal=True,ordered_packets_equal=True,final_png_equal=True))
  (O/'proof/DOUBLE-FAILURE-EQUALITY.json').write_text(json.dumps(dict(status='PASS',binary_sha256=hashlib.sha256(B.read_bytes()).hexdigest(),rows=double_rows),indent=2)+'\n')
  print('PASS double failure',on,action,flush=True)
