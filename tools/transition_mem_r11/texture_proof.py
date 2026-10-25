"""Real GL regression: baseline must fail, each layer's fix is isolated."""
from pathlib import Path
import hashlib,io,json,os,shutil,subprocess,sys,tarfile,tempfile
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();rows=[]
env=dict(os.environ,PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+os.environ['PATH'],RUSTUP_TOOLCHAIN='stable',CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',RUSTFLAGS='--cfg observer_free_B',CARGO_INCREMENTAL='0')
with tempfile.TemporaryDirectory(prefix='r11c-texture-') as temp:
 T=Path(temp)
 archive=subprocess.check_output(['git','archive','aec5590','vendor/macroquad','vendor/miniquad'],cwd=R)
 with tarfile.open(fileobj=io.BytesIO(archive)) as tar:tar.extractall(T,filter='data')
 for name,mq,mqmini,mode,success in [
  ('r11b-ownership',T/'vendor/macroquad',T/'vendor/miniquad','ownership',False),
  ('r11b-guard',T/'vendor/macroquad',T/'vendor/miniquad','guard',False),
  ('ownership-only',Path('/Users/anezium/vita-work/macroquad-material-r11c'),T/'vendor/miniquad','ownership',True),
  ('guard-only',T/'vendor/macroquad',Path('/Users/anezium/vita-work/miniquad-texture-r11c'),'all',True)]:
  d=T/name;(d/'src').mkdir(parents=True)
  shutil.copyfile(R/'examples/transition_texture_lifetime.rs',d/'src/main.rs')
  (d/'Cargo.toml').write_text(f'[package]\nname="r11c-texture-regression"\nversion="0.1.0"\nedition="2021"\n[dependencies]\nmacroquad={{path={json.dumps(str(mq))}}}\n[patch.crates-io]\nminiquad={{path={json.dumps(str(mqmini))}}}\n')
  shutil.copyfile(R/'Cargo.lock',d/'Cargo.lock')
  with (O/'logs'/f'{name}-build.log').open('w') as f:subprocess.run(['cargo','build','--release','--offline'],cwd=d,env=env,stdout=f,stderr=subprocess.STDOUT,check=True)
  b=T/f'{name}-bin';shutil.copy2(Path(env['CARGO_TARGET_DIR'])/'release/r11c-texture-regression',b)
  with (O/'logs'/f'{name}-run.log').open('w') as f:p=subprocess.run([str(b),mode],env=env,stdout=f,stderr=subprocess.STDOUT,timeout=120)
  log=(O/'logs'/f'{name}-run.log').read_text()
  assert (p.returncode==0)==success,(name,p.returncode,log[-1000:])
  if not success:assert 'no entry found for key' in log or 'key not found' in log
  rows.append(dict(test=name,mode=mode,expected_success=success,exit_code=p.returncode,binary_sha256=hashlib.sha256(b.read_bytes()).hexdigest(),passed=True))
  (O/'proof/TEXTURE-REGRESSIONS.json').write_text(json.dumps(dict(status='PASS',rows=rows),indent=2)+'\n')
  print('PASS texture regression',name,flush=True)
