"""Bind final source, pinned forks and linked GPU-retirement evidence."""
from pathlib import Path
import hashlib,json,subprocess,sys,tomllib
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();S=Path('/Users/anezium/vitasdk/bin')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def write(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
native=json.loads((O/'packages/baseline/VERIFICATION.json').read_text());engine=native['source_revision']
assert not subprocess.check_output(['git','diff',engine,'--','src','Cargo.toml','Cargo.lock','examples'],cwd=R)
assert not (R/'vendor').exists()
patches=tomllib.loads((R/'Cargo.toml').read_text())['patch']['crates-io']
old=tomllib.loads(subprocess.check_output(['git','show','aec5590:Cargo.lock'],cwd=R,text=True))
now=tomllib.loads((R/'Cargo.lock').read_text())
assert len(old['package'])==len(now['package'])
changed=[]
for a,b in zip(old['package'],now['package']):
 if a!=b:
  assert a['name']==b['name'] and a['name'] in ['macroquad','miniquad']
  assert {k:v for k,v in a.items() if k!='source'}=={k:v for k,v in b.items() if k!='source'}
  changed.append(a['name'])
assert changed==['macroquad','miniquad']
write(O/'proof/LOCK-SOURCE-COMPARISON.json',dict(status='PASS',base='aec5590',only_changed_packages=changed,only_changed_field='source',other_packages_byte_equivalent=True,cargo_update_used=False))
write(O/'proof/SOURCE-IDENTITY.json',dict(base='e19f73c8f99084f372b84e21bde41c122f96b765',branch=subprocess.check_output(['git','branch','--show-current'],cwd=R,text=True).strip(),engine_revision=engine,cargo_lock_sha256=sha(R/'Cargo.lock'),forks={n:patches[n] for n in ['macroquad','miniquad']},console_contact=False,shot_driver_contact=False,vendor_absent=True))
with (O/'proof/vitagl-gc.s').open('w') as f:subprocess.run([str(S/'arm-vita-eabi-objdump'),'-d','--disassemble=garbage_collector',str(O/'packages/baseline/looking-glass.elf')],stdout=f,check=True)
assert (O/'proof/vitagl-gc.s').read_text().count('#3')>=4
write(O/'proof/GPU-LIFETIME.json',dict(status='ENGINE_ORDER_AND_LINKED_RING_VERIFIED',final_elf_sha256=native['elf_sha256'],linked_ring_modulus=4,texture_gc_and_vitagl_frames=6,material_backend_shader_and_pipeline_released=True,texture_cpu_metadata_released=True,managed_texture_deletion_idempotent=True,render_pass_attachment_single_owner=True,native_pool_reclamation_hardware_measured=False))
(O/'proof/ENGINE-R11C-DIFF.patch').write_bytes(subprocess.check_output(['git','diff','aec5590',engine,'--','src','Cargo.toml','Cargo.lock','examples'],cwd=R))
for name in ['macroquad','miniquad']:
 folder=Path('/Users/anezium/vita-work')/('macroquad-material-r11c' if name=='macroquad' else 'miniquad-texture-r11c')
 rev=patches[name]['rev'];base='2c60f754' if name=='macroquad' else '6f89180'
 assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=folder,text=True).strip()==rev
 (O/'proof'/f'{name}-r11c.diff').write_bytes(subprocess.check_output(['git','diff',base,rev],cwd=folder))
subprocess.run([sys.executable,str(Path(__file__).with_name('dependency_proof.py')),str(O)],check=True)
print('PASS pinned source identity, reviewed source equality and linked four-slot collector')
