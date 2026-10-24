"""Sealed-source deterministic host rendering parity. No console transport."""
from pathlib import Path
import hashlib,json,os,shutil,subprocess,sys,time
O=Path(sys.argv[1]).resolve();R=Path('/Users/anezium/vita-work/lg-acoustic-cache-r6')
B=O/'proof/host-bin';shutil.copy2('/Users/anezium/vita-j0/cargo-target-host/release/looking-glass',B)
result={'scope':'Mac host rendering, no device scheduling. Neutral deterministic input, fixed dt; audio is separately sample-clock tested. Frame CPU includes snapshot construction and excludes encoding/log I/O.','binary_sha256':hashlib.sha256(B.read_bytes()).hexdigest(),'runs':[]}
for mapname,dt in [('pandemonium','0.04184'),('hedge3','0.060'),('gvillage','0.033333333'),('skool2','0.033333333'),('hatter1','0.033333333')]:
 for i,arm in enumerate(['baseline','cache'],1):
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
  result['runs'].append(row);(O/'proof/HOST-RENDER.json').write_text(json.dumps(result,indent=2)+'\n')
  print('DONE',mapname,i,arm,row['image_sha256'],flush=True)
 for key in ['image_sha256','states','lights','joint_order','world_order']:
  rows=[r for r in result['runs'] if r['map']==mapname];assert all(r[key]==rows[0][key] for r in rows), (mapname,key)
result['all_180_frame_states_lights_draw_order_and_png_equal']=True
(O/'proof/HOST-RENDER.json').write_text(json.dumps(result,indent=2)+'\n')
