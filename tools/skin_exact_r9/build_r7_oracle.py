from pathlib import Path
import os,subprocess,shutil,sys
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();D=O/'tmp/r7-oracle'
D.parent.mkdir(exist_ok=True)
subprocess.run(['git','clone','--shared','--no-hardlinks',str(R),str(D)],check=True,stdout=subprocess.DEVNULL)
subprocess.run(['git','switch','--detach','777460d'],cwd=D,check=True,stdout=subprocess.DEVNULL)
# Only test hooks are added to unchanged r7 production consumer/publication code.
for n in ['src/sim_maps/probe.rs','src/render_fx/retained.rs','src/npc/skin_exact_check.rs']:
 shutil.copy2(R/n,D/n)
p=D/'src/npc/skin_exact_check.rs';s=p.read_text().replace('        if case.ends_with("fade") {puppet=puppet.with_afterimages();}','        if std::env::var("LOOKING_GLASS_SKIN_EXACT_CASE").is_ok_and(|only|only!=case) {continue;}');p.write_text(s)
p=D/'src/npc.rs';s=p.read_text().replace('mod card_art;','mod card_art;\n#[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]\npub(crate) mod skin_exact_check;',1);p.write_text(s)
p=D/'src/viewer.rs';s=p.read_text().replace('    let mut comparisons=0;\n    for name in','    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]\n    if std::env::var_os("LOOKING_GLASS_SKIN_EXACT_FIXTURES").is_some() {\n        return crate::npc::skin_exact_check::check(assets,options).await;\n    }\n    let mut comparisons=0;\n    for name in',1);p.write_text(s)
extra=subprocess.run(['git','diff','--no-index','--','/dev/null','src/npc/skin_exact_check.rs'],cwd=D,capture_output=True).stdout
(O/'proof/R7-ORACLE-ONLY.patch').write_bytes(subprocess.check_output(['git','diff','--','src'],cwd=D)+extra)
env=dict(os.environ,PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:'+os.environ['PATH'],RUSTUP_TOOLCHAIN='stable',CARGO_TARGET_DIR='/Users/anezium/vita-j0/cargo-target-host',RUSTFLAGS='--cfg observer_free_B')
with (O/'logs/r7-oracle-build.log').open('w') as f:
 p=subprocess.run(['cargo','build','--release','--offline','--locked','--features','native_timing,frame_total_only,sim_maps_probe'],cwd=D,env=env,stdout=f,stderr=subprocess.STDOUT)
assert p.returncode==0
shutil.copy2('/Users/anezium/vita-j0/cargo-target-host/release/looking-glass',O/'proof/r7-host-bin')
print('Built r7 production code plus host-only oracle hooks',flush=True)
