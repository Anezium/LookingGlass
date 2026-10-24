"""Offline Mac builds from committed source; contains no console transport."""
from pathlib import Path
import datetime, hashlib, json, os, shutil, subprocess, time

OUT=Path(__file__).resolve().parents[1]
ROOT=Path('/Users/anezium/vita-work/lg-lever-matrix-r3')
SDK=Path('/Users/anezium/vitasdk/bin')
records=[]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def git(*args):return subprocess.check_output(['git','-C',str(ROOT),*args],text=True).strip()
def run(name,argv,env):
    log=OUT/'logs'/(name+'.log');start=time.monotonic()
    with log.open('wb') as f:result=subprocess.run(argv,cwd=ROOT,env=env,stdout=f,stderr=subprocess.STDOUT)
    receipt=dict(name=name,argv=argv,cwd=str(ROOT),exit_code=result.returncode,seconds=time.monotonic()-start,
        log=str(log.relative_to(OUT)),log_sha256=sha(log),environment={k:env.get(k) for k in
        ['RUSTFLAGS','RUSTUP_TOOLCHAIN','CARGO_TARGET_DIR','VITASDK','CARGO_NET_OFFLINE','CARGO_INCREMENTAL','CARGO_BUILD_JOBS','TMPDIR']})
    records.append(receipt)
    (OUT/'proof/BUILD-COMMANDS.json').write_text(json.dumps(records,indent=2)+'\n')
    print(name,result.returncode,round(receipt['seconds'],1),flush=True)
    if result.returncode:
        print(log.read_text(errors='replace')[-6000:]);raise SystemExit(result.returncode)
    return receipt

assert not git('status','--porcelain','--','src','Cargo.toml','Cargo.lock','build.rs','.cargo','tools/lever_matrix','configs'), 'Build only committed source and inputs'
revision=git('rev-parse','HEAD')
assert not git('diff','5e072af','HEAD','--','Cargo.lock','build.rs','.cargo')
env=dict(os.environ,PATH=str(SDK)+':/Users/anezium/.cargo/bin:'+os.environ['PATH'],
    VITASDK='/Users/anezium/vitasdk',CARGO_NET_OFFLINE='true',CARGO_INCREMENTAL='0',
    CARGO_BUILD_JOBS='4',CARGO_TERM_COLOR='never',PYTHONDONTWRITEBYTECODE='1')
host=dict(env,RUSTUP_TOOLCHAIN='stable',CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',RUSTFLAGS='--cfg observer_free_B')
run('host-frame-total',['cargo','test','--offline','--locked','--features','frame_total_only','frame_total_tests','--','--nocapture'],host)
run('host-world',['cargo','test','--offline','--locked','--features','frame_total_only','world_','--','--nocapture'],host)
run('host-fx',['cargo','test','--offline','--locked','--features','frame_total_only','fx_','--','--nocapture'],host)
run('host-audio',['cargo','test','--offline','--locked','--features','frame_total_only','audio_','--','--nocapture'],host)
run('host-logging',['cargo','test','--offline','--locked','--features','frame_total_only','logging_','--','--nocapture'],host)
run('host-submission',['cargo','test','--offline','--locked','--features','frame_total_only','render::vita_renderer::submit::tests','--','--nocapture'],host)
(OUT/'tmp').mkdir(exist_ok=True)
native=dict(env,RUSTUP_TOOLCHAIN='nightly-2026-10-11',RUSTFLAGS='-C target-feature=-neon --cfg observer_free_B',
    CARGO_TARGET_DIR=str(OUT/'target-vita'),TMPDIR=str(OUT/'tmp'))
command=['cargo','vita','build','vpk','--release','--offline','--locked','--features','native_timing,vita_renderer,frame_total_only']
receipt=run('native',command,native)
release=OUT/'target-vita/armv7-sony-vita-newlibeabihf/release'
package=OUT/'packages/baseline'
for source,dest in [('looking-glass.elf','looking-glass.elf'),('looking-glass.self','eboot.bin'),('looking-glass.velf','looking-glass.velf')]:
    found=list((OUT/'target-vita').rglob(source));assert len(found)==1,(source,found)
    shutil.copyfile(found[0],package/dest)
for name in ['verify-elf-margin.py','verify-process-params.py']:
    run(name,['/Users/anezium/vita-j0/venv/bin/python',str(OUT/'scripts'/name),str(package/'looking-glass.elf')],env)
run('convert',['vita-elf-create','-vvv',str(package/'looking-glass.elf'),str(package/'regenerated.velf')],env)
run('fself',['vita-make-fself','-s',str(package/'regenerated.velf'),str(package/'regenerated.self')],env)
assert sha(package/'regenerated.self')==sha(package/'eboot.bin')
assert sha(package/'regenerated.velf')==sha(package/'looking-glass.velf')
(package/'regenerated.self').unlink();(package/'regenerated.velf').unlink()
run('symbols',[str(SDK/'arm-vita-eabi-nm'),'-a','-C',str(package/'looking-glass.elf')],env)
for arm in ['baseline','fx','audio','world-material-sort','world-static-merge','world-uniform-persist']:
    dest=OUT/'packages'/arm
    if arm!='baseline':shutil.copyfile(package/'eboot.bin',dest/'eboot.bin')
    verification=dict(arm=arm,source_revision=git('rev-parse','5e072af'),revision=revision,
        instrumentation_commits=git('rev-list','--reverse','5e072af..'+revision).splitlines(),
        branch=git('branch','--show-current'),build_command=command,build_environment=receipt['environment'],
        self_sha256=sha(dest/'eboot.bin'),self_bytes=(dest/'eboot.bin').stat().st_size,
        elf_sha256=sha(package/'looking-glass.elf'),velf_sha256=sha(package/'looking-glass.velf'),
        lock_sha256=sha(ROOT/'Cargo.lock'),config_path=f'configs/PL-{arm}.json',config_sha256=sha(OUT/'configs'/f'PL-{arm}.json'),
        common_binary=True,copy_of='packages/baseline/eboot.bin',native_build_count=1,
        shared_timer_source_sha256=sha(ROOT/'src/frame_total_profile.rs'),
        zero_render_observer_hooks=True,identical_timer='same executable bytes in every package',
        regenerated_self_byte_equal=True,elf_margin_pass=True,process_parameters_pass=True,
        dependency_pins_unchanged=True,console_access=False,runtime_performance='UNMEASURED')
    (dest/'VERIFICATION.json').write_text(json.dumps(verification,indent=2)+'\n')
print('Six packages built from one committed executable.',flush=True)
