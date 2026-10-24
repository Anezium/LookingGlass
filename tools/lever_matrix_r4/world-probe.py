"""Replay the r3 CPU probe against r4, then qualify sealed host rendering."""
from pathlib import Path
import hashlib,io,json,os,shutil,subprocess,sys,tarfile,time
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();P=O.parent/'alice-lever-matrix-r3';A=P/'analysis-r3-regressions';S=O/'world-probe'
S.mkdir(exist_ok=True)
with tarfile.open(fileobj=io.BytesIO(subprocess.check_output(['git','archive','HEAD'],cwd=R))) as t:t.extractall(S,filter='data')
instrument=(A/'instrument_world.py').read_text().replace("O=Path(__file__).resolve().parent",'O=Path('+repr(str(O))+')').replace("S=O/'host-probe'","S=O/'world-probe'")
exec(compile(instrument,str(A/'instrument_world.py'),'exec'))
p=S/'src/render/vita_renderer/submit.rs';s=p.read_text()
old=subprocess.check_output(['git','show','881988ec:src/render/vita_renderer/submit.rs'],cwd=R,text=True)
a=old.index('pub(super) fn merge(');b=old.index('\n#[cfg(test)]',a);old=old[a:b].replace('pub(super) fn merge(','fn merge_original(')
a=s.index('fn merge_original(');b=s.index('fn merge_guarded(',a);s=s[:a]+old+s[b:]
# Compare the complete Prepared representation semantically at every replayed pass.
s=s.replace('    if frame != 60 && frame != 120 {return;}\n    BENCH.with(|b|b.set(true));\n    let mut work=input.clone();let mut guarded=input.clone();','    BENCH.with(|b|b.set(true));\n    let mut work=input.clone();let mut guarded=input.clone();')
s=s.replace('    let output=work.len();let loops=2000;', '    if frame != 60 && frame != 120 {BENCH.with(|b|b.set(false));return;}\n    let output=work.len();let loops=2000;')
p.write_text(s)
env=dict(os.environ,PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+os.environ['PATH'],RUSTUP_TOOLCHAIN='stable',CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',RUSTFLAGS='--cfg observer_free_B')
result={'scope':'Host CPU microbenchmarks; not Vita wall forecasts. Prepared equality checks all 180 opaque lists; timings sample frame 60 and 120 with nine alternating rounds of 2000 iterations.','runs':[]}
for mode,source in [('probe',S),('sealed',R)]:
 os.utime(source/'src/main.rs',None)
 log=O/'logs'/f'world-{mode}-build.log'
 with log.open('w') as f:subprocess.run(['cargo','build','--release','--offline','--locked','--features','native_timing,frame_total_only,sim_maps_probe'],cwd=source,env=env,stdout=f,stderr=subprocess.STDOUT,check=True)
 binary=O/('world-'+mode+'-bin');shutil.copy2('/Users/anezium/vita-j0/cargo-target-host/release/looking-glass',binary)
 for arm in ['baseline','world-static-merge']:
  run=O/'world-host'/mode/arm;(run/'private').mkdir(parents=True,exist_ok=True)
  shutil.copy2(P/'configs'/f'PL-{arm}.json',run/'private/vita_renderer.json')
  runenv={k:v for k,v in env.items() if not k.startswith('LOOKING_GLASS_')}
  runenv.update(LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT='0.04184')
  if mode=='probe':runenv['R3_RESEARCH']='1'
  argv=[str(binary),'--data','/Users/anezium/vita-work/alice-data/Alice1/bin/base','--map','pandemonium','--no-audio','--frames','180','--save-dir',str(run/'private/saves'),'--capture',str(run/'frame.png')]
  print('START',mode,arm,flush=True)
  with (run/'run.log').open('w') as f:p=subprocess.run(argv,cwd=run,env=runenv,stdout=f,stderr=subprocess.STDOUT,timeout=300)
  assert p.returncode==0
  lines=(run/'run.log').read_text().splitlines()
  def tagged(tag):return [json.loads(l[len(tag)+1:]) for l in lines if l.startswith(tag+' ')]
  frames=tagged('SIM_MAPS_FRAME');assert len(frames)==180
  row={'mode':mode,'arm':arm,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'image_sha256':hashlib.sha256((run/'frame.png').read_bytes()).hexdigest(),'state_hashes':[f['state_sha256'] for f in frames],'selected_lights_bits':[f['selected_lights_bits'] for f in frames],'merge_bench':tagged('R3_MERGE_BENCH')}
  result['runs'].append(row)
  print('END',mode,arm,row['image_sha256'],row['merge_bench'],flush=True)
  (O/'proof/WORLD-HOST.json').write_text(json.dumps(result,indent=2)+'\n')
for mode in ['probe','sealed']:
 a,b=[r for r in result['runs'] if r['mode']==mode]
 for key in ['image_sha256','state_hashes','selected_lights_bits']:assert a[key]==b[key],(mode,key)
result['exact_sealed_host_equality']=True;result['prepared_output_matches_r3_all_180_lists']=True
(O/'proof/WORLD-HOST.json').write_text(json.dumps(result,indent=2)+'\n')
