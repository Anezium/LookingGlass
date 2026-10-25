"""Test exact fork library sources in isolated crates without example dev deps."""
from pathlib import Path
import hashlib,io,json,os,re,shutil,subprocess,sys,tarfile,tempfile,tomllib
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();rows=[]
before=(R/'Cargo.lock').read_bytes();patches=tomllib.loads((R/'Cargo.toml').read_text())['patch']['crates-io']
env=dict(os.environ,PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+os.environ['PATH'],RUSTUP_TOOLCHAIN='stable',CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',RUSTFLAGS='--cfg observer_free_B',CARGO_INCREMENTAL='0')
for name,folder,selection,count in [('miniquad','miniquad-texture-r11c','texture_lifetime_tests',2),('macroquad','macroquad-material-r11c','retired_scene_textures_do_not_accumulate_as_pending_atlas_candidates',1)]:
 with tempfile.TemporaryDirectory(prefix='r11c-dependency-test-') as temp:
  T=Path(temp);fork=Path('/Users/anezium/vita-work')/folder
  with tarfile.open(fileobj=io.BytesIO(subprocess.check_output(['git','archive',patches[name]['rev']],cwd=fork))) as tar:tar.extractall(T,filter='data')
  p=T/'Cargo.toml';manifest=p.read_text()
  manifest=re.sub(r'(?ms)^\[dev-dependencies\].*?(?=^\[|\Z)','',manifest)
  if name=='macroquad':
   # The engine enables none of these optional subsystems. A standalone root
   # resolves their packages anyway; omit inactive options in this test-only
   # manifest so no unrelated lock entries or downloads are introduced.
   manifest=re.sub(r'(?m)^(quad-snd|backtrace|log)\s*=.*\n','',manifest)
   manifest=manifest.replace('audio = ["quad-snd"]','audio = []').replace('log-rs = ["log"]','log-rs = []').replace('[features]','[features]\nbacktrace = []')
   patch='miniquad = '+json.dumps(patches['miniquad']).replace('"git":','git =').replace('"rev":','rev =')+'\n'
   manifest=manifest.replace('[patch.crates-io]','[patch.crates-io]\n'+patch)
  p.write_text(manifest);shutil.copyfile(R/'Cargo.lock',T/'Cargo.lock')
  log=O/'logs'/f'r11c-{name}-unit.log'
  cmd=['cargo','test','--release','--offline','--manifest-path',str(p),'--lib',selection,'--','--nocapture','--test-threads=1']
  with log.open('w') as f:proc=subprocess.run(cmd,cwd=T,env=env,stdout=f,stderr=subprocess.STDOUT)
  text=log.read_text();assert proc.returncode==0,(name,text[-2500:])
  assert f'test result: ok. {count} passed;' in text,(name,'wrong selected test count')
  rows.append(dict(crate=name,revision=patches[name]['rev'],test_count=count,exit_code=0,source_files_from_exact_fork_archive=True,manifest_change='example-only dev-dependencies and disabled optional subsystems removed; macroquad miniquad source pinned; shipping sources unchanged',log=str(log)))
  print('PASS dependency unit tests',name,count,flush=True)
assert (R/'Cargo.lock').read_bytes()==before
(O/'proof/DEPENDENCY-UNIT-TESTS.json').write_text(json.dumps(dict(status='PASS',root_lock_unchanged=True,rows=rows),indent=2)+'\n')
