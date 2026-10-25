"""Repeated actual viewer operations, private local data only; no device imports."""
from pathlib import Path
import argparse, hashlib, json, os, shutil, subprocess
from trend import measure
p=argparse.ArgumentParser();p.add_argument('output',type=Path);p.add_argument('binary',type=Path);a=p.parse_args()
O=a.output.resolve();B=a.binary.resolve();R=Path(__file__).resolve().parents[2]
base=json.loads((R/'tools/lever_matrix_r4/configs/PL-baseline.json').read_text())
base.update(phase_census=False,profile_capture=False,audio_music_thread=False,story_film_bypass=True)
rows=[]
for on in [False,True]:
 for action in ['retry','native-load','chapter']:
  d=O/'repeated'/f'{action}-{int(on)}';(d/'private/saves').mkdir(parents=True,exist_ok=True)
  (d/'private/vita_renderer.json').write_text(json.dumps(dict(base,audio_static_cache=on,collision_sah=on,skin_actual_consumers=on)))
  seed=O/'host/seed-hedge3/private/saves/quick.json'
  shutil.copyfile(seed,d/'private/saves/auto.json')
  # Hedge3's authored fade is five seconds. Resume loads and allow six seconds
  # of real silent-mixer/game clocks between operations before sampling steady.
  if action=='chapter':plan=[(2+i*136,'chapter','pandemonium' if i%2==0 else 'hedge3') for i in range(60)]
  else:plan=[(2+i*136,action,'hedge3') for i in range(20)]
  env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
  env.update(LOOKING_GLASS_TRANSITION_CENSUS='1',LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT='0.05',LOOKING_GLASS_SAH_SILENT_AUDIO='1',LOOKING_GLASS_TRANSITION_RESUME='1',LOOKING_GLASS_TRANSITION_PLAN=json.dumps(plan))
  cmd=[str(B),'--data','/Users/anezium/vita-work/alice-data/Alice1/bin/base','--map','hedge3','--save-dir',str(d/'private/saves'),'--frames',str(plan[-1][0]+130)]
  with (d/'run.log').open('w') as f:r=subprocess.run(cmd,cwd=d,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=1800)
  assert r.returncode==0,(action,on,r.returncode)
  lines=(d/'run.log').read_text().splitlines()
  records=[json.loads(l.split('] ',1)[1]) for l in lines if l.startswith('[lg/transition-memory] ')]
  ends=[r for r in records if r['event']=='end' and r['kind'] not in ['startup-entry','quit']]
  assert len(ends)==len(plan) and all(r['success'] for r in ends)
  steady=[json.loads(l.split(' ',1)[1]) for l in lines if l.startswith('TRANSITION_STEADY ')]
  checkpoints=[next(s for s in steady if s['frame']==1)]
  checkpoints += [next(s for s in steady if s['frame']==f+126) for f,_,dest in plan if dest=='hedge3']
  values=[s['live_rust_request_bytes'] for s in checkpoints]
  assert len(values)==(31 if action=='chapter' else 21)
  # Gate the full fixed post-warm-up population, not the final adjacent pair.
  growth=values[-1]-values[-2]
  row=dict(levers=on,action=action,cycles=30 if action=='chapter' else 20,required_cycles=30 if action=='chapter' else 10,steady=checkpoints,
           first_rebuild_residue_bytes=values[1]-values[0],later_growth_bytes=growth,
           total_later_variation_bytes=max(values[1:])-min(values[1:]),
           records=ends,plateau_tolerance_bytes=16*1024,**measure(values))
  rows.append(row)
  status=('PASS' if len(rows)==6 else 'RUNNING') if all(r['pass_trend'] for r in rows) else 'GROWTH_REQUIRES_FIX'
  (O/'proof/REPEATED-TRANSITIONS.json').write_text(json.dumps(dict(status=status,binary_sha256=hashlib.sha256(B.read_bytes()).hexdigest(),rows=rows),indent=2)+'\n')
  print(action,on,'steady MiB',[round(v/1048576,6) for v in values],'later bytes',growth,flush=True)
assert all(r['pass_trend'] for r in rows),'Repeated-transition trend requires source diagnosis'
