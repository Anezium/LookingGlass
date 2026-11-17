"""Read-only package verification. No kit command or transport is invoked."""
import hashlib,json,shutil
from pathlib import Path
P=Path(__file__).resolve().parent
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
def verify():
 r=json.loads((P/'READY.json').read_text())
 for name,want in r['files'].items():assert sha(P/name)==want,name
 for field in ['self','kit','base']:assert sha(r[field+'_path'])==r[field+'_sha256'],field
 assert r['driver_sha256']==sha(P/r['driver'])
 proof=json.loads((P/'host-proof.json').read_text());assert len(proof)==14 and all(row['exact'] for row in proof)
 assert sum(row['frames'] for row in proof)==32706
 assert len({row['tape'] for row in proof})==7
 assert {row['candidate'] for row in proof}=={'env-off','env-discard-free'}
 assert all(row['arm_binary_sha256']==r['host_binary']['binary_sha256'] for row in proof)
 for row in json.loads((P/'screenshots.json').read_text()):
  assert sha(row['before'])==row['before_sha256'] and sha(row['after'])==row['after_sha256']
 for row in json.loads((P/'picture-interaction.json').read_text()):assert row['combined_pixels_equal_frontface']
 for row in json.loads((P/'census-proof.json').read_text()):assert row['state_packets_pixels'] and row['head']==r['head']
 assert json.loads((P/'TEST-RESULTS.json').read_text())['result']=='OK'
 assert json.loads((P/'unit-receipt.json').read_text())['result']==0
 assert json.loads((P/'feature-off-receipt.json').read_text())['result']==0
 assert not (P/'target-vita').exists(),'private native cache must be removed at hand-off'
 assert shutil.disk_usage(P).free>41*2**30
 print('PASS READY files, artifacts, 14 exact pairs / 32706 frames, screenshots, interaction, census, offline tests, cache closure and reserve')
 return r
if __name__=='__main__':verify()
