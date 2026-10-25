from pathlib import Path
import os,json,subprocess,hashlib,shutil,time
O=Path('/Users/anezium/vita-j0/resume-2026-10-24/alice-skin-consumers-r7')
R=Path('/Users/anezium/vita-work/lg-skin-consumers-r7')
B=O/'proof/host-bin';shutil.copy2('/Users/anezium/vita-j0/cargo-target-host/release/looking-glass',B)
base=json.loads((R/'tools/lever_matrix_r4/configs/PL-baseline.json').read_text())
result={'binary_sha256':hashlib.sha256(B.read_bytes()).hexdigest(),'runs':[]}
for m,dt in [('pandemonium','0.04184'),('hedge3','0.06'),('hatter1','0.05')]:
 for arm in ['off','on']:
  d=O/'host'/f'{m}-{arm}-moving';(d/'private').mkdir(parents=True,exist_ok=True)
  cfg=dict(base,skin_actual_consumers=arm=='on');(d/'private/vita_renderer.json').write_text(json.dumps(cfg))
  env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
  env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT=dt,LOOKING_GLASS_SIM_MAPS_COUNTERS='0',LOOKING_GLASS_SKIN_PROOF='1',LOOKING_GLASS_SKIN_MOVING_VIEW='1',LOOKING_GLASS_SKIN_DEATHS='1')
  cmd=[str(B),'--data','/Users/anezium/vita-work/alice-data/Alice1/bin/base','--map',m,'--no-audio','--frames','180','--save-dir',str(d/'private/saves'),'--capture',str(d/'frame.png')]
  with (d/'run.log').open('w') as f:p=subprocess.run(cmd,cwd=d,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=360)
  assert p.returncode==0,(m,arm,p.returncode)
  frames=[json.loads(l.split(' ',1)[1]) for l in (d/'run.log').read_text().splitlines() if l.startswith('SIM_MAPS_FRAME ')]
  assert len(frames)==180
  assert all(f['actor_skin'] and all('sha256' in v for v in f['actor_skin']) and f['submitted_packets'] for f in frames)
  row={'map':m,'arm':arm,'frames':frames,'image_sha256':hashlib.sha256((d/'frame.png').read_bytes()).hexdigest()}
  result['runs'].append(row)
  (O/'proof/HOST-MAPS.json').write_text(json.dumps(result,indent=2)+'\n')
  print('DONE',m,arm,len(frames),'skin surfaces',sum(len(f['actor_skin']) for f in frames),flush=True)
 rows=[r for r in result['runs'] if r['map']==m]
 keys=['actor_skin','submitted_packets','state_sha256','selected_lights_bits','joint_alpha_order','blend_world_order']
 for n,(a,b) in enumerate(zip(rows[0]['frames'],rows[1]['frames'])):
  for k in keys:assert a[k]==b[k],(m,n,k,a[k] if k=='actor_skin' else '')
 assert rows[0]['image_sha256']==rows[1]['image_sha256'],(m,'image')
 print('PASS',m,'180 frames, vertices/UV/indices, packets, simulation, lights, order, PNG',flush=True)
result['status']='PASS';(O/'proof/HOST-MAPS.json').write_text(json.dumps(result,indent=2)+'\n')
