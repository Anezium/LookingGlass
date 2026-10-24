"""Exact host world-command, state, image and allocation gates. No transport."""
from pathlib import Path
import hashlib,json,os,shutil,subprocess,sys,time
O=Path(sys.argv[1]).resolve();B=O/'proof/host-bin';DATA='/Users/anezium/vita-work/alice-data/Alice1/bin/base'
result=dict(scope='macOS host finite workload proofs; no console contact or Vita performance forecast',binary_sha256=hashlib.sha256(B.read_bytes()).hexdigest(),runs=[])
def write(): (O/'proof/HOST-RENDER.json').write_text(json.dumps(result,indent=2)+'\n')
def run(mapname,arm,kind='entry',view=None,allocation=False):
 label=f'{mapname}-{kind}-{arm}';d=O/'host'/label;(d/'private').mkdir(parents=True,exist_ok=True)
 for previous in [d/'frame.png',d/f'{mapname}-census-180.json']:
  if previous.exists():previous.unlink()
 c=json.loads((O/'configs'/f'PL-{arm}.json').read_text());c.update(profiler='off' if allocation else 'full',profile_capture=False)
 (d/'private/vita_renderer.json').write_text(json.dumps(c,indent=2)+'\n')
 env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
 env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT='0.060' if mapname=='hedge3' else '0.04184',LOOKING_GLASS_SIM_MAPS_COUNTERS='0' if allocation else '1')
 if allocation:env['LOOKING_GLASS_WORLD_INPUT_ALLOC']='1'
 else:env['LOOKING_GLASS_WORLD_INPUT_TRACE']='1'
 if view:
  (d/'view.json').write_text(json.dumps(view,indent=2)+'\n');env['LOOKING_GLASS_WORLD_INPUT_VIEW']=str(d/'view.json')
 cmd=[str(B),'--data',DATA,'--map',mapname,'--no-audio','--frames','180','--save-dir',str(d/'private/saves'),'--capture',str(d/'frame.png')]
 with (d/'run.log').open('w') as f:p=subprocess.run(cmd,cwd=d,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=360)
 assert p.returncode==0,(label,p.returncode)
 lines=(d/'run.log').read_text().splitlines();frames=[json.loads(l.split(' ',1)[1]) for l in lines if l.startswith('SIM_MAPS_FRAME ')]
 assert len(frames)==180
 row=dict(label=label,map=mapname,arm=arm,kind=kind,image_sha256=hashlib.sha256((d/'frame.png').read_bytes()).hexdigest(),states=[f['state_sha256'] for f in frames],lights=[f['selected_lights_bits'] for f in frames],joint_order=[f['joint_alpha_order'] for f in frames],world_order=[f['blend_world_order'] for f in frames],views=[json.loads(l.split(' ',1)[1]) for l in lines if l.startswith('WORLD_INPUT_VIEW ')])
 if not allocation:
  row['world_commands']=[f['vertex_input_proof']['command_sha256'][1] for f in frames]
  row['effective_inputs']=[f['vertex_input_proof']['effective_draw_input_sha256'] for f in frames]
  row['full_frame_raw_commands']=[f['vertex_input_proof']['command_sha256'][0] for f in frames]
  names=set().union(*(f['vertex_input_proof']['counts'][1] for f in frames[60:]))
  row['world_GL_calls_per_frame']={n:sum(f['vertex_input_proof']['counts'][1].get(n,0) for f in frames[60:])/120 for n in sorted(names)}
 if c.get('phase_census',False):
  sys.path.insert(0,str(O/'console'));from census_report import validate
  validate(json.loads((d/f'{mapname}-census-180.json').read_text()),mapname);row['census_valid']=True
 if allocation:
  values=[int(l.split(' ',1)[1]) for l in lines if l.startswith('WORLD_SUBMIT_ALLOCATIONS ')]
  assert len(values)==180 and all(n==0 for n in values[60:]),(label,values)
  row['submit_allocations_all_frames']=values
 result['runs'].append(row);write();print('PASS run',label,row['image_sha256'],flush=True);return row
def equal(rows,trace=True):
 keys=['image_sha256','states','lights','joint_order','world_order','views']
 if trace:keys+=['world_commands','effective_inputs']
 for k in keys:
  assert all(r[k]==rows[0][k] for r in rows),(k,[r['label'] for r in rows])
for m in ['hedge3','pandemonium']:
 rows=[run(m,a) for a in ['baseline','vao','baseline-census','vao-census']];equal(rows)
 pose=rows[0]['views'][0];view=dict(eye=pose['eye'],target=pose['target'],radians_per_frame=0.003)
 moving=[run(m,a,'moving-saved-view',view) for a in ['baseline','vao']];equal(moving)
 alloc=[run(m,a,'allocation',allocation=True) for a in ['baseline','vao']];equal(alloc,False)
result.update(all_180_state_light_order_hashes_equal=True,all_world_command_streams_exact_except_vertex_setup=True,all_full_frame_effective_vertex_inputs_equal=True,all_saved_and_moving_final_png_bytes_equal=True,all_measured_submit_loop_Rust_allocations_zero=True)
write()
# Full native GL fixtures, including reuploads, deleted buffers/layouts and target interruptions.
d=O/'host/interop';(d/'private').mkdir(parents=True,exist_ok=True)
shutil.copy2(O/'configs/PL-vao.json',d/'private/vita_renderer.json')
for flag,tag in [('--vita-alpha-check','interop'),('--vita-scale-check','scale')]:
 with (O/'logs'/f'{tag}-final.log').open('w') as f:p=subprocess.run([str(B),'--data',DATA,flag],cwd=d,stdout=f,stderr=subprocess.STDOUT,timeout=240)
 assert p.returncode==0,(tag,p.returncode)
result['native_host_interop_and_scaled_HUD_fixtures_pass']=True;write()
# Actual human saved scene: seven poses plus HUD and mutable content.
human=Path('/Users/anezium/vita-j0/resume-2026-10-24/alice-lever-matrix-r3/session-s5-world-static-merge/originals/data/alice/private/saves/auto.json')
assert hashlib.sha256(human.read_bytes()).hexdigest()=='ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9'
saved=[]
for arm in ['baseline','vao']:
 d=O/'host'/f'human-save-{arm}';(d/'private/saves').mkdir(parents=True,exist_ok=True);shutil.copy2(human,d/'private/saves/auto.json')
 if (d/'private/renderer-host').exists():shutil.rmtree(d/'private/renderer-host')
 c=json.loads((O/'configs'/f'PL-{arm}.json').read_text());c.update(profiler='off',profile_capture=False)
 (d/'private/vita_renderer.json').write_text(json.dumps(c,indent=2)+'\n')
 env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')};env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_COUNTERS='0',LOOKING_GLASS_WORLD_INPUT_SAVED_ONLY='1')
 with (d/'run.log').open('w') as f:p=subprocess.run([str(B),'--data',DATA,'--save-dir',str(d/'private/saves'),'--renderer-host-check'],cwd=d,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=360)
 assert p.returncode==0,(arm,p.returncode)
 images={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in (d/'private/renderer-host').glob('*.png')};assert len(images)==14
 data=json.loads((d/'private/renderer-host/samples.json').read_text());assert len(data['samples'])==180
 assert all(s['world_allocations']==0 for s in data['samples']),(arm,[s['world_allocations'] for s in data['samples']])
 saved.append(dict(arm=arm,images=images,world_allocations=[s['world_allocations'] for s in data['samples']]))
 print('PASS human saved view fixture',arm,flush=True)
assert saved[0]['images']==saved[1]['images']
result['human_saved_scene']=saved;result['status']='PASS';write();print('PASS all host gates',flush=True)
