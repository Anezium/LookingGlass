"""Verify two completed B-only load smokes before allowing any wall launch."""
from pathlib import Path
import hashlib,json

def verify(root):
 receipt=json.loads((Path(root)/'SMOKE-ADMISSION.json').read_text())
 assert receipt['status']=='PASS' and receipt['maps']==['hedge3','pandemonium']
 expected=json.loads((Path(root).parent/'packages/sah/VERIFICATION.json').read_text())['self_sha256']
 assert receipt['self_sha256']==expected
 for row in receipt['receipts']:
  path=Path(row['path']);assert hashlib.sha256(path.read_bytes()).hexdigest()==row['sha256']
 return receipt

def inspect(package,smoke_root):
 """Validate completed smoke evidence read-only; return hash-bound inputs."""
 package=Path(package);smoke_root=Path(smoke_root)
 protocol=json.loads((package/'CONSOLE-PROTOCOL.json').read_text());receipts=[]
 def record(path):
  receipts.append(dict(path=str(path.resolve()),sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
 def identity(path,expected,size):
  assert path.stat().st_size==size and hashlib.sha256(path.read_bytes()).hexdigest()==expected, f'Identity mismatch: {path}'
  record(path)
 final=smoke_root/'RESTORE-FINAL.json';assert json.loads(final.read_text())['status']=='PASS_ORIGINAL_STATE_VERIFIED'
 expected_package=json.loads((package/'packages/sah/VERIFICATION.json').read_text());expected=expected_package['self_sha256']
 binary=package/'packages/sah/eboot.bin'
 assert binary.stat().st_size==expected_package['self_bytes'] and hashlib.sha256(binary.read_bytes()).hexdigest()==expected
 sequence=protocol['smoke_sequence']
 assert len(sequence)==2 and sorted(e['map'] for e in sequence)==['hedge3','pandemonium']
 for entry in sequence:
  run=smoke_root/entry['label'];audit=json.loads((run/'AUDIT.json').read_text())
  assert entry['arm']=='B' and audit['actual_config']['collision_sah'] and not audit['actual_config'].get('audio_static_cache',False)
  assert audit['heap_readout']['whole_run']['sampled_max_MiB']<=180
  assert audit['heap_readout']['benchmark']['sampled_max_MiB']<165
  self_identity=json.loads((run/'SELF-IDENTITY.json').read_text())
  assert self_identity['status']=='PASS' and self_identity['sha256']==expected and self_identity['bytes']==expected_package['self_bytes']
  identity(run/'self-independent-readback.bin',expected,expected_package['self_bytes'])
  record(run/'SELF-IDENTITY.json')
  for name in ['AUDIT.json','STRICT-AUDIO.json','LOG-INTEGRITY.json']:
   path=run/name;assert json.loads(path.read_text())['status']=='PASS'
   record(path)
 human='387ccf678251d4267b8ced53e207830358826f8e0642631a0459c6b0b9939990'
 for receipt_name,binary_name in [('SELF-INDEPENDENT-VERIFY.json','restored-self-independent.bin'),('HUMAN-SELF-SECOND-INDEPENDENT-VERIFY.json','restored-human-self-second-independent.bin')]:
  restored=json.loads((smoke_root/receipt_name).read_text())
  assert restored['status']=='PASS' and restored['sha256']==human and restored['bytes']==37371674
  identity(smoke_root/binary_name,human,37371674);record(smoke_root/receipt_name)
 save='ea4cd221d9f950e613188ccf3ecf2c1eabb1d34106f413125864320d364c8fa9'
 saved=json.loads((smoke_root/'PANDEMONIUM-ENTRY.json').read_text())
 assert saved['sha256']==save and saved['bytes']==181900 and saved['canonical_map']=='pandemonium' and saved['visit_key']=='pandemonium$first'
 assert saved['fresh_session_start_readback'] and saved['companions_restored_and_verified'] and saved['independent_readbacks']==2
 for directory in ['originals','originals-independent-readbacks','restoration-readbacks']:
  identity(smoke_root/directory/'data/alice/private/saves/auto.json',save,181900)
 record(smoke_root/'PANDEMONIUM-ENTRY.json');record(final)
 return dict(status='PASS',maps=['hedge3','pandemonium'],self_sha256=expected,human_self_sha256=human,save_sha256=save,receipts=receipts,smoke_session_restored=True)

def admit(root,smoke_root):
 root=Path(root)
 result=inspect(root.parent,smoke_root)
 with (root/'SMOKE-ADMISSION.json').open('x')as f:f.write(json.dumps(result,indent=2)+'\n')
 return verify(root)
