"""Four-map real-query/simulation proof. Never contacts a console or driver."""
from pathlib import Path
import hashlib,json,os,shutil,subprocess,sys,time
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve()
def write(p,v):p.parent.mkdir(parents=True,exist_ok=True);p.write_text(json.dumps(v,indent=2)+'\n')
env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
env.update(PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+env['PATH'],RUSTUP_TOOLCHAIN='stable',CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',RUSTFLAGS='--cfg observer_free_B',CARGO_INCREMENTAL='0')
cmd=['cargo','build','--release','--offline','--locked','--features','native_timing,frame_total_only,sim_maps_probe']
with (O/'logs/host-build.log').open('w')as f:subprocess.run(cmd,cwd=R,env=env,stdout=f,stderr=subprocess.STDOUT,check=True)
binary=O/'host/probe-bin';shutil.copy2(Path(env['CARGO_TARGET_DIR'])/'release/looking-glass',binary)
base=json.loads((R/'tools/lever_matrix_r4/configs/PL-baseline.json').read_text())
receipts=[];proof={}
runs=O/os.environ.get('SAH_RUNS_SUBDIR','host');runs.mkdir(parents=True,exist_ok=True)
for name,dt in [('hedge3','0.060'),('pandemonium','0.04184'),('gvillage','0.05'),('skool2','0.05')]:
 rows=[]
 for arm in ['A','B']:
  run=runs/f'{name}-{arm}';(run/'private').mkdir(parents=True,exist_ok=True)
  assert not (run/'sah-corpus.jsonl').exists(),'Never overwrite an existing corpus'
  cfg=dict(base,audio_music_thread=False,audio_static_cache=False,phase_census=False,collision_sah=arm=='B',profile_capture=False)
  write(run/'private/vita_renderer.json',cfg)
  e=dict(env,LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT=dt,LOOKING_GLASS_SAH_CORPUS='1',LOOKING_GLASS_SAH_SILENT_AUDIO='1')
  argv=[str(binary),'--data','/Users/anezium/vita-work/alice-data/Alice1/bin/base','--map',name,'--no-audio','--frames','180','--save-dir',str(run/'private/saves'),'--capture',str(run/'frame.png')]
  start=time.monotonic()
  with (run/'run.log').open('w')as f:result=subprocess.run(argv,cwd=run,env=e,stdout=f,stderr=subprocess.STDOUT,timeout=360)
  receipts.append(dict(map=name,arm=arm,argv=argv,exit_code=result.returncode,seconds=time.monotonic()-start,binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest()));write(O/'proof/HOST-RUNS.json',receipts)
  assert result.returncode==0,(name,arm)
  frames=[json.loads(x.split('SIM_MAPS_FRAME ',1)[1]) for x in (run/'run.log').read_text().splitlines()if x.startswith('SIM_MAPS_FRAME ')]
  assert len(frames)==180;rows.append(frames)
  print(name,arm,'PASS 180 frames',flush=True)
 keys=['state_sha256','selected_lights_bits','joint_alpha_order','blend_world_order','npc_decisions']
 assert all(all(a[k]==b[k]for k in keys)for a,b in zip(*rows)),name
 pngs=[hashlib.sha256((runs/f'{name}-{a}'/'frame.png').read_bytes()).hexdigest()for a in ['A','B']];assert pngs[0]==pngs[1]
 proof[name]=dict(frames=180,all_equal=True,fields=keys,final_png_sha256=pngs[0],state_hashes=[r['state_sha256']for r in rows[0]])
 write(O/'proof/SIMULATION-EQUALITY.json',proof)
