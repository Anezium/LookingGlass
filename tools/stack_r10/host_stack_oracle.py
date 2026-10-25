"""GPU-packet oracle with all three switches together. No device transport."""
from pathlib import Path
import argparse,hashlib,json,os,subprocess,sys
p=argparse.ArgumentParser();p.add_argument('output',type=Path);p.add_argument('--maps',nargs='*');p.add_argument('--binary',type=Path);args=p.parse_args()
R=Path(__file__).resolve().parents[2];O=args.output.resolve();B=args.binary or O/'proof/host-bin'
base=json.loads((R/'tools/lever_matrix_r4/configs/PL-baseline.json').read_text())
result={'binary_sha256':hashlib.sha256(B.read_bytes()).hexdigest(),'switches':['audio_static_cache','collision_sah','skin_actual_consumers'],'runs':[]}
for m in (args.maps or ['hedge3','pandemonium']):
 dt={'pandemonium':'0.04184','hedge3':'0.06'}.get(m,'0.05');arms=[]
 for arm in [False,True]:
  d=O/'host'/f'{m}-stack-oracle-{int(arm)}';(d/'private').mkdir(parents=True,exist_ok=True)
  cfg=dict(base,audio_static_cache=arm,collision_sah=arm,skin_actual_consumers=arm,phase_census=True)
  (d/'private/vita_renderer.json').write_text(json.dumps(cfg))
  env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
  env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT=dt,LOOKING_GLASS_SIM_MAPS_COUNTERS='0',LOOKING_GLASS_SKIN_PROOF='1',LOOKING_GLASS_SKIN_MOVING_VIEW='1',LOOKING_GLASS_SKIN_DEATHS='1')
  cmd=[str(B),'--data','/Users/anezium/vita-work/alice-data/Alice1/bin/base','--no-audio','--map',m,'--frames','180','--capture',str(d/'final.png'),'--save-dir',str(d/'private/saves')]
  with (d/'run.log').open('w') as f:proc=subprocess.run(cmd,cwd=d,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=900)
  assert proc.returncode==0,(m,arm,proc.returncode)
  lines=(d/'run.log').read_text().splitlines()
  frames=[json.loads(l.split(' ',1)[1]) for l in lines if l.startswith('SIM_MAPS_FRAME ')]
  images=[json.loads(l.split(' ',1)[1]) for l in lines if l.startswith('SKIN_EXACT_IMAGE ')]
  assert len(frames)==180,(m,arm,len(frames))
  assert all(all(p['geometry']==p['gpu_geometry'] for p in f['submitted_packets']) for f in frames)
  loads=[l for l in lines if l.startswith('COLLISION_SAH_LOAD ')]
  assert loads==[] if not arm else len(loads)==1
  row={'map':m,'arm':arm,'frames':len(frames),'images':images,'gpu_draw_readbacks':sum(len(f['submitted_packets']) for f in frames),'final_png_sha256':hashlib.sha256((d/'final.png').read_bytes()).hexdigest(),'packet_frames':frames}
  arms.append(row);result['runs'].append({k:v for k,v in row.items() if k!='packet_frames'})
  print('DONE',m,arm,row['gpu_draw_readbacks'],flush=True)
 for n,(a,b) in enumerate(zip(arms[0]['packet_frames'],arms[1]['packet_frames'])):
  for k in ['actor_skin','state_sha256','selected_lights_bits','joint_alpha_order','blend_world_order']:
   assert a[k]==b[k],(m,n,k)
  packets=lambda f:[{k:v for k,v in p.items() if k!='gpu_slot'} for p in f['submitted_packets']]
  assert packets(a)==packets(b),(m,n,'uploaded GPU packets')
 assert arms[0]['images']==arms[1]['images'] and arms[0]['final_png_sha256']==arms[1]['final_png_sha256'],m
 print('PASS',m,'all-on GPU packets and pixels match all-off',flush=True)
result['status']='PASS'
(O/'proof/STACK-ORACLE.json').write_text(json.dumps(result,indent=2)+'\n')
