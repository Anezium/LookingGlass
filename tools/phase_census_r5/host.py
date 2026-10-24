"""Sealed-source deterministic host rendering ABBA. No console transport."""
from pathlib import Path
import hashlib,json,os,shutil,subprocess,sys,time
O=Path(sys.argv[1]).resolve();R=Path('/Users/anezium/vita-work/lg-phase-census-r5')
B=O/'proof/host-bin';shutil.copy2('/Users/anezium/vita-j0/cargo-target-host/release/looking-glass',B)
result={'scope':'Mac host rendering, no device scheduling. Neutral deterministic input, fixed dt; audio is separately sample-clock tested. Frame CPU includes snapshot construction and excludes encoding/log I/O.','binary_sha256':hashlib.sha256(B.read_bytes()).hexdigest(),'runs':[]}
for mapname,dt in [('pandemonium','0.04184'),('hedge3','0.060')]:
 for i,arm in enumerate(['baseline','census','census','baseline'],1):
  run=O/'host'/f'{mapname}-{i}-{arm}';(run/'private').mkdir(parents=True,exist_ok=True)
  shutil.copy2(O/'configs'/f'PL-{arm}.json',run/'private/vita_renderer.json')
  env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')};env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT=dt,LOOKING_GLASS_SIM_MAPS_COUNTERS='0')
  cmd=[str(B),'--data','/Users/anezium/vita-work/alice-data/Alice1/bin/base','--map',mapname,'--no-audio','--frames','180','--save-dir',str(run/'private/saves'),'--capture',str(run/'frame.png')]
  t=time.monotonic()
  with (run/'run.log').open('w') as f:p=subprocess.run(cmd,cwd=run,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=360)
  assert p.returncode==0,(run,p.returncode)
  frames=[json.loads(l.split(' ',1)[1]) for l in (run/'run.log').read_text().splitlines() if l.startswith('SIM_MAPS_FRAME ')]
  assert len(frames)==180
  row={'map':mapname,'arm':arm,'position':i,'seconds':time.monotonic()-t,'image_sha256':hashlib.sha256((run/'frame.png').read_bytes()).hexdigest(),'states':[f['state_sha256'] for f in frames],'lights':[f['selected_lights_bits'] for f in frames],'joint_order':[f['joint_alpha_order'] for f in frames],'world_order':[f['blend_world_order'] for f in frames],'cpu_ms':[f['cpu_ms'] for f in frames[60:]]}
  if arm=='census':
   sys.path.insert(0,str(O/'console'));from census_report import validate
   census=json.loads((run/f'{mapname}-census-180.json').read_text());validate(census,mapname)
   row['census_valid']=True
  result['runs'].append(row);(O/'proof/HOST-RENDER.json').write_text(json.dumps(result,indent=2)+'\n')
  print('DONE',mapname,i,arm,row['image_sha256'],flush=True)
 for key in ['image_sha256','states','lights','joint_order','world_order']:
  rows=[r for r in result['runs'] if r['map']==mapname];assert all(r[key]==rows[0][key] for r in rows), (mapname,key)
result['all_180_frame_states_lights_draw_order_and_png_equal']=True
result['overhead']={}
for m in ['pandemonium','hedge3']:
 rows=[r for r in result['runs'] if r['map']==m];med=[sorted(r['cpu_ms'])[60] for r in rows];d=[med[1]-med[0],med[2]-med[3]]
 result['overhead'][m]={'arm_cpu_median_ms':med,'oriented_delta_ms':d,'mean_delta_ms':sum(d)/2,'scope':'Host CPU brackets, no audio, no conversion to Vita milliseconds'}
(O/'proof/HOST-RENDER.json').write_text(json.dumps(result,indent=2)+'\n')
print(result['overhead'],flush=True)
