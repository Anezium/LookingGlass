"""Read-only verification of the detached seal and every inventoried artifact."""
from pathlib import Path
import sys,json,hashlib
O=Path(sys.argv[1]).resolve();sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
seal=json.loads((O/'SEAL-R11C.json').read_text())
for name,row in seal['files'].items():
 p=O/name;assert p.stat().st_size==row['bytes'] and sha(p)==row['sha256'],name
inventory=json.loads((O/'ARTIFACT-SHA256.json').read_text())
for name,row in inventory.items():
 p=O/name;assert p.stat().st_size==row['bytes'] and sha(p)==row['sha256'],name
actual={str(p.relative_to(O)) for p in O.rglob('*') if p.is_file()}
assert actual==set(inventory)|{'ARTIFACT-SHA256.json','SOL-REPORT-R11C.md','SEAL-R11C.json'},'Missing or extra artifact'
assert not any(p.name.startswith('session-') for p in O.iterdir())
assert not list(O.rglob('__pycache__'))
print('PASS detached seal and',len(inventory),'artifact hashes')
for name in ['CONSOLE-PACKAGE.zip','ARTIFACT-SHA256.json','SOL-REPORT-R11C.md','SEAL-R11C.json']:print(sha(O/name),name)
