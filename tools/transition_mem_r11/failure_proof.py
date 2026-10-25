"""Partial destination construction failure versus a refused native save read."""
from pathlib import Path
import sys,json,os,subprocess,shutil,hashlib
O=Path(sys.argv[1]).resolve();B=Path(sys.argv[2]).resolve();R=Path(__file__).resolve().parents[2]
base=json.loads((R/'tools/lever_matrix_r4/configs/PL-baseline.json').read_text());base.update(phase_census=False,profile_capture=False,audio_music_thread=False,story_film_bypass=True)
human=Path('/Users/anezium/vita-j0/resume-2026-10-24/alice-lever-matrix-r3/session-s5-world-static-merge/originals/data/alice/private/saves/auto.json');seed=O/'host/seed-hedge3/private/saves/quick.json';rows=[]
for on in [False,True]:
 result=[]
 for fail in [False,True]:
  d=O/'failure'/f'{int(on)}-{int(fail)}';(d/'private/saves').mkdir(parents=True,exist_ok=True)
  (d/'private/vita_renderer.json').write_text(json.dumps(dict(base,audio_static_cache=on,collision_sah=on,skin_actual_consumers=on)))
  shutil.copyfile(seed,d/'private/saves/quick.json');(d/'private/saves/auto.json').write_bytes(human.read_bytes() if fail else b'invalid native save read fixture')
  env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')};env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT='0.05',LOOKING_GLASS_SAH_SILENT_AUDIO='1',LOOKING_GLASS_SKIN_PROOF='1',LOOKING_GLASS_TRANSITION_ACTION='native-load')
  if fail:env['LOOKING_GLASS_TRANSITION_FAIL_BUILD']='1'
  argv=[str(B),'--data','/Users/anezium/vita-work/alice-data/Alice1/bin/base','--map','hedge3','--frames','8','--save-dir',str(d/'private/saves'),'--capture',str(d/'final.png')]
  with (d/'run.log').open('w') as f:p=subprocess.run(argv,cwd=d,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=120)
  assert p.returncode==0,(on,fail,p.returncode)
  log=(d/'run.log').read_text();states=[json.loads(x.split(' ',1)[1]) for x in log.splitlines() if x.startswith('TRANSITION_STATE ')];frames=[json.loads(x.split(' ',1)[1]) for x in log.splitlines() if x.startswith('SIM_MAPS_FRAME ')]
  assert len(states)==8 and all(s['map']=='hedge3' for s in states)
  if fail:
   assert log.count('save_restore npc_models_restore elapsed_ms=')==2, 'Both partial incoming and recovered outgoing NPC construction must be reached'
   end=[json.loads(x.split('] ',1)[1]) for x in log.splitlines() if x.startswith('[lg/transition-memory] ') and '"event":"end"' in x]
   assert any(e['kind']=='native-load' and e['success'] is False and e['released'] for e in end)
  result.append(dict(states=states,frames=frames,dir=d))
 assert result[0]['states']==result[1]['states'],(on,'failed transition changed Game state')
 for x,y in zip(result[0]['frames'],result[1]['frames']):
  def packets(f):return [{k:v for k,v in p.items() if k!='gpu_slot'} for p in f['submitted_packets']]
  assert packets(x)==packets(y),(on,'failed transition changed render packets')
 assert hashlib.sha256((result[0]['dir']/'final.png').read_bytes()).digest()==hashlib.sha256((result[1]['dir']/'final.png').read_bytes()).digest(),(on,'failed transition final PNG')
 rows.append(dict(levers=on,frames=8,state_equal=True,ordered_packets_equal=True,final_png_equal=True,partial_destination_failure_recovered=True))
(O/'proof/FAILED-TRANSITION-EQUALITY.json').write_text(json.dumps(dict(status='PASS',binary_sha256=hashlib.sha256(B.read_bytes()).hexdigest(),rows=rows),indent=2)+'\n');print('PASS partial destination failure recovers exact Game state, packets and PNG')
