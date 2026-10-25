"""Mac-only full-frame CPU/GPU geometry and pixel oracle. No device transport."""
from pathlib import Path
import argparse,hashlib,json,os,shutil,subprocess,sys
p=argparse.ArgumentParser();p.add_argument('output',type=Path);p.add_argument('--fixtures-only',action='store_true');p.add_argument('--maps',nargs='*');p.add_argument('--binary',type=Path);args=p.parse_args()
R=Path(__file__).resolve().parents[2];O=args.output.resolve();B=args.binary or (O/'proof/png-host-bin' if args.fixtures_only and (O/'proof/png-host-bin').exists() else O/'proof/host-bin')
base=json.loads((R/'tools/lever_matrix_r4/configs/PL-baseline.json').read_text())
result={'binary_sha256':hashlib.sha256(B.read_bytes()).hexdigest(),'gpu_observer':'glGetBufferSubData on the bound vertex attribute and element buffers before every retained draw, including cache hits','runs':[]}
work=[('fixtures',None)] if args.fixtures_only else [(m,{'pandemonium':'0.04184','hedge3':'0.06'}.get(m,'0.05')) for m in (args.maps or ['pandemonium','hedge3','keep','grounds2','tower1','tower2','tower3','garden2','centipede2','funhouse','jlair1','jlair2','grounds1','hatter1'])]
if args.maps and (O/'proof/HOST-MAPS.json').exists():
 previous=json.loads((O/'proof/HOST-MAPS.json').read_text());assert previous['binary_sha256']==result['binary_sha256']
 result['runs']=[r for r in previous['runs'] if r['map'] not in args.maps]
for m,dt in work:
 arms=[]
 for arm in [False,True]:
  d=O/'host'/f'{m}-exact-{int(arm)}';(d/'private').mkdir(parents=True,exist_ok=True)
  (d/'private/vita_renderer.json').write_text(json.dumps(dict(base,skin_actual_consumers=arm,phase_census=not args.fixtures_only)))
  env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
  env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT=dt or '0.05',LOOKING_GLASS_SIM_MAPS_COUNTERS='0',LOOKING_GLASS_SKIN_PROOF='1')
  if args.fixtures_only:env['LOOKING_GLASS_SKIN_EXACT_FIXTURES']='1'
  else:env.update(LOOKING_GLASS_SKIN_MOVING_VIEW='1',LOOKING_GLASS_SKIN_DEATHS='1')
  cmd=[str(B),'--data','/Users/anezium/vita-work/alice-data/Alice1/bin/base','--no-audio','--save-dir',str(d/'private/saves')]
  cmd+=['--skin-overlap-render-check'] if args.fixtures_only else ['--map',m,'--frames','180','--capture',str(d/'final.png')]
  with (d/'run.log').open('w') as f:proc=subprocess.run(cmd,cwd=d,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=900)
  assert proc.returncode==0,(m,arm,proc.returncode,str(d/'run.log'))
  lines=(d/'run.log').read_text().splitlines()
  frames=[json.loads(l.split(' ',1)[1]) for l in lines if l.startswith('SIM_MAPS_FRAME ')]
  images=[json.loads(l.split(' ',1)[1]) for l in lines if l.startswith('SKIN_EXACT_IMAGE ')]
  assert len(frames)==(900 if args.fixtures_only else 180),(m,arm,len(frames))
  assert all(all(p['geometry']==p['gpu_geometry'] for p in f['submitted_packets']) for f in frames)
  pngs={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((d/'private/exact-frames').glob('*.png'))} if args.fixtures_only else {}
  if args.fixtures_only:assert len(pngs)==900
  row={'png_sha256':pngs,'map':m,'arm':arm,'frames':frames,'images':images,'gpu_draw_readbacks':sum(len(f['submitted_packets']) for f in frames),'gpu_slots':sorted({p['gpu_slot'] for f in frames for p in f['submitted_packets']})}
  if not args.fixtures_only:
   census=json.loads((d/f'{m}-census-180.json').read_text());rs=census['rows'][60:]
   row['serial_vertices']=sum(a['serial_vertices'] for r in rs for a in r['actors'])/120
   row['worker_vertices']=sum(a['worker_vertices'] for r in rs for a in r['actors'])/120
   row['final_png_sha256']=hashlib.sha256((d/'final.png').read_bytes()).hexdigest()
  arms.append(row);result['runs'].append(row)
  (O/'proof'/('HOST-FIXTURES.json' if args.fixtures_only else 'HOST-MAPS.json')).write_text(json.dumps(result,indent=2)+'\n')
  print('DONE',m,arm,len(frames),'GPU draws',row['gpu_draw_readbacks'],flush=True)
 for n,(a,b) in enumerate(zip(arms[0]['frames'],arms[1]['frames'])):
  for k in ['actor_skin','state_sha256','selected_lights_bits','joint_alpha_order','blend_world_order']:
   assert a[k]==b[k],(m,n,k)
  # Physical slot addresses can differ with batching/publication ownership. Every
  # slot's actual GL data must match its intended payload before this comparison.
  packets=lambda f:[{k:v for k,v in p.items() if k!='gpu_slot'} for p in f['submitted_packets']]
  assert packets(a)==packets(b),(m,n,'uploaded GPU packets')
 assert arms[0]['images']==arms[1]['images'],(m,'intermediate pixels')
 if not args.fixtures_only:assert arms[0]['final_png_sha256']==arms[1]['final_png_sha256'],(m,'final image')
 else:
  assert len(arms[0]['images'])==900
  assert arms[0]['png_sha256']==arms[1]['png_sha256'],(m,'intermediate PNG files')
 print('PASS',m,'every intermediate geometry upload and pixel witness',flush=True)
result['status']='PASS'
(O/'proof'/('HOST-FIXTURES.json' if args.fixtures_only else 'HOST-MAPS.json')).write_text(json.dumps(result,indent=2)+'\n')
