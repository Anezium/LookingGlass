import json
from pathlib import Path
P=Path(__file__).resolve().parent
rows=[]
for part in ['owner','fight','maps']:rows+=json.loads((P/('host-proof-'+part+'.json')).read_text())
assert len(rows)==14 and len({(r['tape'],r['candidate']) for r in rows})==14
(P/'host-proof.json').write_text(json.dumps(rows,indent=2)+'\n')
print(len(rows),'strict comparisons',sum(r['frames'] for r in rows),'frames')
