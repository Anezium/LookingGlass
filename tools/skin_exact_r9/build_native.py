from pathlib import Path
import sys
import datetime,hashlib,json,os,shutil,subprocess,time
O=Path(sys.argv[1]).resolve();R=Path('/Users/anezium/vita-work/lg-skin-exact-r9');S=Path('/Users/anezium/vitasdk/bin')
for n in ['logs','proof','tmp','packages/baseline']:(O/n).mkdir(parents=True,exist_ok=True)
env=dict(os.environ,PATH=str(S)+':/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+os.environ['PATH'],VITASDK='/Users/anezium/vitasdk',CARGO_NET_OFFLINE='true',CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='4',CARGO_TERM_COLOR='never',RUSTUP_TOOLCHAIN='nightly-2026-10-11',RUSTFLAGS='-C target-feature=-neon --cfg observer_free_B',CARGO_TARGET_DIR=str(O/'target-vita'),TMPDIR=str(O/'tmp'))
cmd=['cargo','vita','build','vpk','--release','--offline','--locked','--features','native_timing,vita_renderer,frame_total_only']
assert not subprocess.check_output(['git','status','--porcelain','--','src','Cargo.lock','Cargo.toml','build.rs','.cargo'],cwd=R)
assert not subprocess.check_output(['git','diff','6270497','HEAD','--','Cargo.lock','Cargo.toml','build.rs','.cargo'],cwd=R)
t=time.monotonic()
with (O/'logs/native-build.log').open('wb') as f:p=subprocess.run(cmd,cwd=R,env=env,stdout=f,stderr=subprocess.STDOUT)
receipt={'argv':cmd,'cwd':str(R),'revision':subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip(),'exit_code':p.returncode,'seconds':time.monotonic()-t,'environment':{k:env[k] for k in ['PATH','VITASDK','CARGO_NET_OFFLINE','CARGO_INCREMENTAL','CARGO_BUILD_JOBS','RUSTUP_TOOLCHAIN','RUSTFLAGS','CARGO_TARGET_DIR','TMPDIR']}}
(O/'proof/BUILD-COMMAND.json').write_text(json.dumps(receipt,indent=2)+'\n')
assert p.returncode==0
for name,dest in [('looking-glass.elf','looking-glass.elf'),('looking-glass.velf','looking-glass.velf'),('looking-glass.self','eboot.bin'),('looking-glass.vpk','looking-glass.vpk')]:
 found=list((O/'target-vita').rglob(name));assert len(found)==1,(name,found)
 shutil.copyfile(found[0],O/'packages/baseline'/dest)
print('Native artifacts copied',flush=True)
