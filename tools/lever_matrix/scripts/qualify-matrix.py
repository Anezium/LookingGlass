"""Qualify committed source, native artifacts and observer-free lever configs."""
from pathlib import Path
import hashlib,json,re,subprocess,sys

O=Path(__file__).resolve().parents[1]
R=Path('/Users/anezium/vita-work/lg-lever-matrix')
P=O.parent/'alice-observer-free'
SDK=Path('/Users/anezium/vitasdk/bin')
sys.path.insert(0,str(O/'console'))
from map_identity import save_identity,verify_runtime
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def git(*args):return subprocess.check_output(['git','-C',str(R),*args],text=True).strip()
def disassemble(elf,name,dest):
    data=subprocess.check_output([str(SDK/'arm-vita-eabi-objdump'),'-d','-C','--disassemble='+name,str(elf)],text=True)
    dest.write_text(data)
    instructions=[]
    for line in data.splitlines():
        m=re.match(r'\s*([0-9a-f]+):\s+(?:(?:[0-9a-f]{8}|[0-9a-f]{4})\s+)+\s*([a-z][a-z0-9.]*)\s*(.*)',line)
        if m:instructions.append(dict(mnemonic=m[2],operands=m[3]))
    assert instructions
    return instructions

arms=['baseline','world','fx','audio']
packages=[json.loads((O/'packages'/a/'VERIFICATION.json').read_text()) for a in arms]
assert len({v['self_sha256'] for v in packages})==1
assert len({v['revision'] for v in packages})==1
base=json.loads((O/'configs/PL-baseline.json').read_text())
protocol=json.loads((O/'CONSOLE-PROTOCOL.json').read_text())
config_proof={}
for arm,v in zip(arms,packages):
    assert sha(O/'packages'/arm/'eboot.bin')==v['self_sha256']
    config=json.loads((O/'configs'/f'PL-{arm}.json').read_text())
    changed={k:dict(baseline=base[k],arm=config[k]) for k in base if base[k]!=config[k]}
    assert set(changed)==set(protocol['config_changes'][arm])
    assert config['profiler']=='off' and config['profile_capture']
    assert not any(config[k] for k in ['light_gl_observers','world_binding_cache','world_gl_state_cache','fx_merge_ranges','sim_dynamic_buckets','render_draw_timing','frame_phase_probe','worker_probe'])
    assert all(config[k] for k in ['lights_static_cache','sim_npc_sight_candidates','sim_npc_sight_cache','sim_npc_target_stack'])
    config_proof[arm]=dict(config_sha256=sha(O/'configs'/f'PL-{arm}.json'),exact_diff=changed)
    for mode in ['self','full']:
        manifest=json.loads((O/f'stage-{arm}-{mode}.json').read_text())
        if mode=='self':assert len(manifest['files'])==1 and manifest['files'][0]['remote']=='ux0:/app/ALICELG01/eboot.bin'
        for f in manifest['files']:assert Path(f['local']).is_file()
assert base==json.loads((P/'configs/PL-B.json').read_text()), 'Baseline settings drifted from parity candidate'
(O/'proof/CONFIG-ARM-DIFFS.json').write_text(json.dumps(config_proof,indent=2)+'\n')

symbols=(O/'logs/symbols.log').read_text()
assert not any('looking_glass::render_tail::hooks' in line for line in symbols.splitlines())
for symbol in ['alice_frame_total_begin','alice_frame_total_sample','alice_frame_total_ring']:
    assert any(line.split()[-1:]==[symbol] for line in symbols.splitlines()),symbol
assert 'tested 29 names: zero GL observer hooks' in (O/'logs/host-frame-total.log').read_text()
assert '3 passed; 0 failed' in (O/'logs/host-frame-total.log').read_text()
timer={}
for name in ['alice_frame_total_begin','alice_frame_total_sample']:
    current=disassemble(O/'packages/baseline/looking-glass.elf',name,O/'proof'/f'{name}.asm')
    previous=disassemble(P/'packages/B-5e072af/looking-glass.elf',name,O/'proof'/f'parity-{name}.asm')
    assert [x['mnemonic'] for x in current]==[x['mnemonic'] for x in previous]
    timer[name]=dict(instructions=len(current),same_mnemonics_as_parity=True,
        calls=[x['operands'] for x in current if x['mnemonic'] in ['bl','blx']])
assert sum('::now>' in x for x in timer['alice_frame_total_begin']['calls'])==1
for name in ['src/frame_total_profile.rs','src/render_direct.rs','src/frame_total_gl_names.txt']:
    assert sha(R/name)==sha(Path('/Users/anezium/vita-work/lg-observer-free-B')/name)
assert not git('diff','0039e476079513de0fa8d7267da453a2eb51da5a','HEAD','--','src','Cargo.toml','Cargo.lock','build.rs','.cargo')
(O/'proof/ZERO-GL-HOOKS.json').write_text(json.dumps(dict(status='PASS',observer_hooks=0,
    resolver_names_tested=29,native_hook_symbols=[],timer=timer,
    all_arm_executables_byte_identical=True,engine_sources_identical_to_parity_build=True,
    timer_source_sha256=sha(R/'src/frame_total_profile.rs'),
    disassembly_limit='Mnemonic comparison against prior parity package; all matrix arms have exactly the same machine code because they share every executable byte.',
    caches='Shader/texture functionality retained; two world interception caches OFF; no per-call attribution available'),indent=2)+'\n')

# Historical files are read only as diagnostic evidence, never copied or staged.
old=P/'session-pandemonium'
save=old/'originals/data/alice/private/saves/auto.json'
log=old/'reg01-pandemonium-A/entry-log-before-arm.txt'
identity=save_identity(save.read_bytes())
entry=next(x for x in protocol['sequence'] if x['map']=='pandemonium')
runtime=verify_runtime(log.read_text(errors='replace'),entry,frozen=False)
(O/'proof/PANDEMONIUM-LABEL.json').write_text(json.dumps(dict(status='PASS',decision='Fix host protocol/audit; runtime canonical map is correct',
    save_identity=identity,runtime_identity=runtime,historical_save_path=str(save),historical_save_sha256=sha(save),
    historical_log_path=str(log),historical_log_sha256=sha(log),
    source_evidence={'src/save.rs:85':'visit_key formats map$first or map$return','src/save.rs:97':'Level.map stores canonical map',
        'src/render.rs:1545':'map_load receipt prints canonical map name','src/viewer.rs:4291':'camera freeze receipt prints maps[current]'},
    fixed_at='CONSOLE-PROTOCOL.json map and visit_key; console/map_identity.py; pre-arm and audit call sites',
    no_historical_save_reuse=True),indent=2)+'\n')

manifest={p:sha(R/p) for p in git('ls-files','src','Cargo.toml','Cargo.lock','build.rs','.cargo').splitlines()}
(O/'proof/SOURCE-MANIFEST.json').write_text(json.dumps(dict(build_revision=packages[0]['revision'],files=manifest,
    unchanged_dependency_pins=True),indent=2)+'\n')
(O/'PACKAGE-MATRIX.json').write_text(json.dumps(dict(status='BUILT_HOST_QUALIFIED_RUNTIME_UNMEASURED',packages=packages,
    common_self_sha256=packages[0]['self_sha256'],console_access=False,driver_contact=False),indent=2)+'\n')
print('PASS: four sealed config packages, zero hooks, shared timer, canonical Pandemonium map.')
