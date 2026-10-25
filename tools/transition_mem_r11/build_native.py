"""Build with the unchanged r6 native recipe, then verify conversion and package."""
from pathlib import Path
import hashlib, json, os, shutil, subprocess, sys, time, zipfile
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();S=Path('/Users/anezium/vitasdk/bin');B=O/'packages/baseline'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n',encoding='utf-8')
def run(tag,argv):
    with (O/'logs'/tag).open('w') as out:p=subprocess.run(argv,stdout=out,stderr=subprocess.STDOUT)
    assert p.returncode==0,(tag,p.returncode)
for n in ['logs','proof','tmp','packages/baseline']:(O/n).mkdir(parents=True,exist_ok=True)
env=dict(os.environ,PATH=str(S)+':/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+os.environ['PATH'],VITASDK='/Users/anezium/vitasdk',CARGO_NET_OFFLINE='true',CARGO_INCREMENTAL='0',CARGO_BUILD_JOBS='4',CARGO_TERM_COLOR='never',RUSTUP_TOOLCHAIN='nightly-2026-10-11',RUSTFLAGS='-C target-feature=-neon --cfg observer_free_B',CARGO_TARGET_DIR=str(O/'target-vita'),TMPDIR=str(O/'tmp'))
cmd=['cargo','vita','build','vpk','--release','--offline','--locked','--features','native_timing,vita_renderer,frame_total_only']
assert not subprocess.check_output(['git','status','--porcelain','--','src','vendor','Cargo.lock','Cargo.toml','build.rs','.cargo'],cwd=R)
assert not subprocess.check_output(['git','diff','e19f73c','HEAD','--','build.rs','.cargo','src/logging.rs','src/render_direct.rs'],cwd=R)
# r11b changes only Macroquad's source location, keeping every locked version.
import tomllib
old=tomllib.loads(subprocess.check_output(['git','show','e19f73c:Cargo.lock'],cwd=R,text=True))
now=tomllib.loads((R/'Cargo.lock').read_text())
assert [(p['name'],p['version']) for p in old['package']]==[(p['name'],p['version']) for p in now['package']]
assert tomllib.loads((R/'Cargo.toml').read_text())['patch']['crates-io']['macroquad']=={'path':'vendor/macroquad'}
assert tomllib.loads((R/'Cargo.toml').read_text())['patch']['crates-io']['miniquad']=={'path':'vendor/miniquad'}
revision=subprocess.check_output(['git','rev-parse','HEAD'],cwd=R,text=True).strip()
started=time.monotonic()
with (O/'logs/native-build.log').open('w') as out:p=subprocess.run(cmd,cwd=R,env=env,stdout=out,stderr=subprocess.STDOUT)
assert not subprocess.check_output(['git','diff',revision,'--','src','vendor','Cargo.lock','Cargo.toml','build.rs','.cargo'],cwd=R),'Source changed during build'
receipt=dict(argv=cmd,cwd=str(R),revision=revision,exit_code=p.returncode,seconds=time.monotonic()-started,environment={k:env[k] for k in ['VITASDK','CARGO_NET_OFFLINE','CARGO_INCREMENTAL','CARGO_BUILD_JOBS','RUSTUP_TOOLCHAIN','RUSTFLAGS','CARGO_TARGET_DIR','TMPDIR']})
write(O/'proof/BUILD-COMMAND.json',receipt);assert p.returncode==0
for name,dest in [('looking-glass.elf','looking-glass.elf'),('looking-glass.velf','looking-glass.velf'),('looking-glass.self','eboot.bin'),('looking-glass.vpk','looking-glass.vpk')]:
    found=list((O/'target-vita').rglob(name));assert len(found)==1,(name,found);shutil.copyfile(found[0],B/dest)
for name,artifact in [('verify-elf-margin.py','looking-glass.elf'),('verify-process-params.py','looking-glass.elf')]:
    run(name+'.log',[sys.executable,str(O/'scripts'/name),str(B/artifact)])
run('native-symbols.log',[str(S/'arm-vita-eabi-nm'),'-C',str(B/'looking-glass.elf')])
assert 'looking_glass::render_tail::hooks' not in (O/'logs/native-symbols.log').read_text()
run('regenerate-velf.log',[str(S/'vita-elf-create'),str(B/'looking-glass.elf'),str(O/'tmp/regenerated.velf')])
run('regenerate-self.log',[str(S/'vita-make-fself'),'-s',str(O/'tmp/regenerated.velf'),str(O/'tmp/regenerated.self')])
assert sha(O/'tmp/regenerated.velf')==sha(B/'looking-glass.velf')
assert sha(O/'tmp/regenerated.self')==sha(B/'eboot.bin')
with zipfile.ZipFile(B/'looking-glass.vpk') as z:
    assert set(z.namelist())=={'eboot.bin','sce_sys/param.sfo'} and hashlib.sha256(z.read('eboot.bin')).hexdigest()==sha(B/'eboot.bin')
    write(O/'proof/VPK-CONTENTS.json',{i.filename:i.file_size for i in z.infolist()})
rows=[]
for arm in ['baseline','stack']:
    d=O/'packages'/arm
    if arm!='baseline':shutil.copyfile(B/'eboot.bin',d/'eboot.bin')
    v=dict(arm=arm,revision=receipt['revision'],source_revision=receipt['revision'],branch='vita-transition-mem-r11-20261025',
        build_command=cmd,build_environment=receipt['environment'],self_sha256=sha(d/'eboot.bin'),self_bytes=(d/'eboot.bin').stat().st_size,
        elf_sha256=sha(B/'looking-glass.elf'),velf_sha256=sha(B/'looking-glass.velf'),vpk_sha256=sha(B/'looking-glass.vpk'),
        lock_sha256=sha(R/'Cargo.lock'),config_sha256=sha(O/'configs'/f'PL-{arm}.json'),common_binary=True,
        regenerated_self_byte_equal=True,regenerated_velf_byte_equal=True,elf_margin_pass=True,process_parameters_pass=True,
        default_cache=False,qualification_opt_in=False,console_access=False)
    write(d/'VERIFICATION.json',v);rows.append(v)
write(O/'PACKAGE-MATRIX.json',dict(status='BUILT_CONSOLE_NOT_EXECUTED',packages=rows))
print('Native build and conversion PASS; SELF',rows[0]['self_sha256'],flush=True)
