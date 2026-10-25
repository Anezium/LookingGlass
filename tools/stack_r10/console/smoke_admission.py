"""Verify two completed B-only load smokes before allowing any wall launch."""
from pathlib import Path
import hashlib,json,re
MARKER='SYNTHETIC-OFFLINE-ONLY'
UUID=re.compile(r'[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}')

def verify(root):
 receipt=json.loads((Path(root)/'SMOKE-ADMISSION.json').read_text())
 assert receipt['status']=='PASS' and receipt['maps']==['hedge3','pandemonium']
 expected=json.loads((Path(root).parent/'packages/stack/VERIFICATION.json').read_text())['self_sha256']
 assert receipt['self_sha256']==expected
 for row in receipt['receipts']:
  path=Path(row['path']);assert hashlib.sha256(path.read_bytes()).hexdigest()==row['sha256']
 return receipt

def reject_synthetic(smoke_root,sequence):
 if (smoke_root/MARKER).exists():
  raise AssertionError('synthetic smoke refused: SYNTHETIC-OFFLINE-ONLY marker')
 for path in [smoke_root/'SESSION.json',smoke_root/'RESTORE-FINAL.json']:
  if json.loads(path.read_text()).get('synthetic_offline_only') is True:
   raise AssertionError(f'synthetic smoke refused: {path.name}')
 for entry in sequence:
  audit=json.loads((smoke_root/entry['label']/'AUDIT.json').read_text())
  if audit.get('synthetic_offline_only') is True:
   raise AssertionError('synthetic smoke refused: AUDIT.json')

def _sha_blobs(entry):
 for blob in (entry.get('source'),entry.get('original'),entry):
  if isinstance(blob,dict) and isinstance(blob.get('sha256'),str) and len(blob['sha256'])==64:
   return True
 return False

def require_provenance(smoke_root):
 ready_path,admitted_path=smoke_root/'DRIVER-READY.json',smoke_root/'DRIVER-ADMISSION.json'
 assert ready_path.is_file() and admitted_path.is_file(), 'DRIVER-READY pid/expiry was not recorded at admission'
 ready,admitted=json.loads(ready_path.read_text()),json.loads(admitted_path.read_text())
 for key in ('pid','started','fixed_expiry'):
  assert ready.get(key) not in (None,'') and admitted.get(key)==ready[key], f'DRIVER-READY {key} missing from admission'
 files=[ready_path,admitted_path];ids=[];id_files=[]
 for path in sorted(smoke_root.rglob('driver-ids.jsonl')):
  id_files.append(path)
  ids.extend(json.loads(line) for line in path.read_text().splitlines() if line.strip())
 assert ids, 'driver request/response IDs missing'
 for row in ids:
  request_id,response_id=row.get('request_id'),row.get('response_id')
  assert isinstance(request_id,str) and request_id==response_id and UUID.search(request_id), 'driver request/response ID is not a driver filename'
 files.extend(id_files)
 ledgers=[path for path in smoke_root.rglob('*ledger.json') if path.is_file()]
 assert ledgers, 'FTP ledger missing'
 ftp_ok=False
 for ledger in ledgers:
  ftp=Path(str(ledger)+'.ftp.jsonl')
  if not ftp.is_file():continue
  lines=[json.loads(line) for line in ftp.read_text().splitlines() if line.strip()]
  connected=any(row.get('event')=='connected' and row.get('host') and row.get('port')==1337 for row in lines)
  data=json.loads(ledger.read_text())
  if connected and any(_sha_blobs(entry) for entry in data.get('entries',[])):
   ftp_ok=True;files.extend([ledger,ftp])
 assert ftp_ok, 'real FTP ledger receipt missing'
 inventory_ok=False
 for manifest in list(smoke_root.rglob('manifest.json')):
  data=json.loads(manifest.read_text())
  rows=data if isinstance(data,list) else data.get('files',[]) if isinstance(data,dict) else []
  for row in rows if isinstance(rows,list) else []:
   if not isinstance(row,dict):continue
   digest,local=row.get('sha256'),row.get('local')
   candidate=Path(local) if local else None
   if candidate is not None and not candidate.is_file():candidate=manifest.parent/local
   if isinstance(digest,str) and len(digest)==64 and candidate is not None and candidate.is_file() and hashlib.sha256(candidate.read_bytes()).hexdigest()==digest:
    inventory_ok=True;files.extend([manifest,candidate])
 assert inventory_ok, 'device inventory hashes missing'
 return dict(driver_request_ids=[row['request_id'] for row in ids],driver_pid=admitted['pid'],driver_fixed_expiry=admitted['fixed_expiry'],files=files)

def heap_gate(entry,audit):
 """Hedge3 benchmark plateau is a smoke gate. Pandemonium heap is not."""
 whole=audit['heap_readout']['whole_run']['sampled_max_MiB']
 bench=audit['heap_readout']['benchmark']['sampled_max_MiB']
 assert isinstance(whole,(int,float)) and not isinstance(whole,bool) and whole<=180, f'whole-run heap sample missing or above 180 MiB: {whole}'
 if entry['map']=='hedge3':
  assert isinstance(bench,(int,float)) and not isinstance(bench,bool) and bench<165, 'Hedge3 smoke requires a benchmark heap sample under 165 MiB'
  return bench
 return bench if isinstance(bench,(int,float)) and not isinstance(bench,bool) else 'unavailable'

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
 record(smoke_root/'SESSION.json')
 expected_package=json.loads((package/'packages/stack/VERIFICATION.json').read_text());expected=expected_package['self_sha256']
 binary=package/'packages/stack/eboot.bin'
 assert binary.stat().st_size==expected_package['self_bytes'] and hashlib.sha256(binary.read_bytes()).hexdigest()==expected
 sequence=protocol['smoke_sequence']
 assert len(sequence)==2 and sorted(e['map'] for e in sequence)==['hedge3','pandemonium']
 reject_synthetic(smoke_root,sequence)
 provenance=require_provenance(smoke_root)
 for path in provenance.pop('files'):record(path)
 plateau={}
 for entry in sequence:
  run=smoke_root/entry['label'];audit=json.loads((run/'AUDIT.json').read_text())
  assert entry['arm']=='B'
  for key in ('audio_static_cache','collision_sah','skin_actual_consumers'):assert audit['actual_config'][key]
  plateau[entry['map']]=heap_gate(entry,audit)
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
 record(smoke_root/'DRIVER-READY.json');record(smoke_root/'DRIVER-ADMISSION.json')
 return dict(status='PASS',maps=['hedge3','pandemonium'],self_sha256=expected,human_self_sha256=human,save_sha256=save,receipts=receipts,smoke_session_restored=True,provenance=provenance,heap_benchmark_plateau_MiB=plateau,pandemonium_benchmark_heap_is_smoke_gate=False,synthetic_offline_only=False)

def admit(root,smoke_root):
 root=Path(root)
 result=inspect(root.parent,smoke_root)
 with (root/'SMOKE-ADMISSION.json').open('x')as f:f.write(json.dumps(result,indent=2)+'\n')
 return verify(root)
