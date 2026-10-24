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

def admit(root,smoke_root):
 root=Path(root);smoke_root=Path(smoke_root);package=root.parent
 protocol=json.loads((package/'CONSOLE-PROTOCOL.json').read_text());receipts=[]
 final=smoke_root/'RESTORE-FINAL.json';assert json.loads(final.read_text())['status']=='PASS_ORIGINAL_STATE_VERIFIED'
 expected=json.loads((package/'packages/sah/VERIFICATION.json').read_text())['self_sha256']
 for entry in protocol['smoke_sequence']:
  run=smoke_root/entry['label'];audit=json.loads((run/'AUDIT.json').read_text())
  assert entry['arm']=='B' and audit['actual_config']['collision_sah'] and not audit['actual_config'].get('audio_static_cache',False)
  assert audit['heap_readout']['whole_run']['sampled_max_MiB']<=180
  assert audit['heap_readout']['benchmark']['sampled_max_MiB']<165
  identity=json.loads((run/'SELF-IDENTITY.json').read_text());assert identity['sha256']==expected
  for name in ['AUDIT.json','STRICT-AUDIO.json','LOG-INTEGRITY.json']:
   path=run/name;assert json.loads(path.read_text())['status']=='PASS'
   receipts.append(dict(path=str(path.resolve()),sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
 receipts.append(dict(path=str(final.resolve()),sha256=hashlib.sha256(final.read_bytes()).hexdigest()))
 result=dict(status='PASS',maps=['hedge3','pandemonium'],self_sha256=expected,receipts=receipts,smoke_session_restored=True)
 with (root/'SMOKE-ADMISSION.json').open('x')as f:f.write(json.dumps(result,indent=2)+'\n')
 return verify(root)
