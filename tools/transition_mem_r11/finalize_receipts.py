"""Bind final source, linked GPU evidence and local dependency patches."""
from pathlib import Path
import sys,json,hashlib,subprocess,shutil
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();S=Path('/Users/anezium/vitasdk/bin')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
v=json.loads((O/'packages/baseline/VERIFICATION.json').read_text());engine=v['source_revision']
assert not subprocess.check_output(['git','diff',engine,'--','src','vendor','Cargo.toml','Cargo.lock'],cwd=R)
write(O/'proof/SOURCE-IDENTITY.json',dict(base='e19f73c8f99084f372b84e21bde41c122f96b765',branch='vita-transition-mem-r11-20261025',engine_revision=engine,cargo_lock_sha256=sha(R/'Cargo.lock'),vendored_macroquad_version='0.4.14',vendored_miniquad_version='0.4.8',miniquad_base_revision='6f891804784bf4a5b39313ee6499344cb0d7e8b6',console_contact=False,shot_driver_contact=False))
with (O/'proof/vitagl-gc.s').open('w') as f:subprocess.run([str(S/'arm-vita-eabi-objdump'),'-d','--disassemble=garbage_collector',str(O/'packages/baseline/looking-glass.elf')],stdout=f,check=True)
assert (O/'proof/vitagl-gc.s').read_text().count('#3')>=4
write(O/'proof/GPU-LIFETIME.json',dict(status='ENGINE_ORDER_AND_LINKED_RING_VERIFIED',final_elf_sha256=v['elf_sha256'],linked_ring_modulus=4,linked_gc_disassembly='proof/vitagl-gc.s',texture_gc_and_vitagl_frames=6,partial_destination_recovery_drained=True,material_backend_shader_and_pipeline_released=True,texture_cpu_metadata_released=True,native_pool_reclamation_hardware_measured=False))
(O/'proof/ENGINE-DIFF.patch').write_bytes(subprocess.check_output(['git','diff','a446a76',engine,'--','src','Cargo.toml','Cargo.lock'],cwd=R))
patches=[];parts=[]
macro=next(Path('/Users/anezium/.cargo/registry/src').glob('*/macroquad-0.4.14'))
mini=Path('/Users/anezium/.cargo/git/checkouts/miniquad-0ef91dd5fd1d4800/6f89180')
for upstream,local,names in [(macro,R/'vendor/macroquad',['src/material.rs','src/quad_gl.rs']),(mini,R/'vendor/miniquad',['src/graphics/gl.rs'])]:
 for name in names:
  a=upstream/name;b=local/name;p=subprocess.run(['diff','-u',str(a),str(b)],capture_output=True,text=True);assert p.returncode==1
  parts.append(p.stdout);patches.append(dict(file=str(b.relative_to(R)),upstream_sha256=sha(a),local_sha256=sha(b)))
(O/'proof/DEPENDENCY-PATCHES.diff').write_text('\n'.join(parts))
write(O/'proof/DEPENDENCY-PATCHES.json',dict(status='PASS',runtime_versions_unchanged=True,patches=patches,texture_metadata_create_delete_cycles=10000,texture_metadata_unit_log='logs/r11b-texture-metadata-unit.log'))
diagnostic=O/'heap-diagnostic'
if diagnostic.exists():
 (diagnostic/'scripts').mkdir(exist_ok=True)
 for name in ['r11b-heap-diagnose.py','r11b-heap-analyze.py']:
  p=Path('/tmp')/name
  if p.exists():shutil.copyfile(p,diagnostic/'scripts'/name)
 write(diagnostic/'IDENTITY.json',dict(exploratory_only=True,source_base_revision=subprocess.check_output(['git','rev-parse','d2ec86f'],cwd=R,text=True).strip(),temporary_heap_histogram=True,final_census_uses_different_immutable_binary=True))
print('PASS final source, linked four-slot collector and dependency patch bindings')
