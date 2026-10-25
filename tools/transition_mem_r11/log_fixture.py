"""Capture the engine formatter's actual records for the offline fake boundary."""
from pathlib import Path
import hashlib,json,sys
O=Path(sys.argv[1]);T=Path(__file__).resolve().parent
rows=[]
for label,kind in [('startup-chooser','startup-entry'),('H-P-0-after','native-load'),('chapter-0-after','chapter-or-console'),('retry-0-after','death-retry')]:
 path=O/'host'/label/'run.log'
 records=[json.loads(l.split('] ',1)[1]) for l in path.read_text().splitlines() if l.startswith('[lg/transition-memory] ')]
 for event in ['begin','end']:
  row=next(r for r in records if r['event']==event and r['kind']==kind)
  rows.append(row)
old=[dict(r) for r in rows if r['kind']=='startup-entry']
old[0]['to']='skool1'
v=dict(source='actual host engine stderr formatter; native allocator values substituted only at the offline transport boundary',formatter_sha256=hashlib.sha256((T.parents[1]/'src/transition_memory.rs').read_bytes()).hexdigest(),records=rows,old_startup_mismatch=old)
(T/'scripts/transition-log-fixture.json').write_text(json.dumps(v,indent=2)+'\n')
