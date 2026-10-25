"""Summarize completed local proofs without any device access."""
from pathlib import Path
import json,hashlib,subprocess,sys
O=Path(sys.argv[1]).resolve();d=json.loads((O/'proof/TRANSITION-EQUALITY.json').read_text());rows=[];saves=[]
for p in d['pairs']:
 tag=p['scenario'];on=p['levers'];a=p['before'];b=p['after'];end=max(b['before']['heap_used'],b['after']['heap_used'])
 stages=[]
 path=O/'host'/f'{tag}-{int(on)}-after/run.log'
 for line in path.read_text().splitlines():
  if line.startswith('TRANSITION_CENSUS '):stages.append(json.loads(line.split(' ',1)[1]))
 rows.append(dict(scenario=tag,levers=on,overlap_request_peak=a['peak_heap_used'],release_request_peak=b['peak_heap_used'],overlap_newlib32_upper_bound=a['host_requests_and_newlib32_upper_bound'][3],release_newlib32_upper_bound=b['host_requests_and_newlib32_upper_bound'][3],before=b['before'],released=b['released'],after=b['after'],extra_above_max_endpoints_bytes=b['peak_heap_used']-end,phases=stages))
 if tag in ['new-game','quit']:
  values=[]
  for which in ['before','after']:
   file=O/'host'/f'{tag}-{int(on)}-{which}/private/saves/auto.json';v=json.loads(file.read_text())['payload'];values.append(v)
  assert values[0]==values[1] and values[0]['current']=='hedge3$first'
  saves.append(dict(scenario=tag,levers=on,outgoing_auto_payload_equal=True,outgoing_visit=values[0]['current'],payload_sha256=hashlib.sha256(json.dumps(values[0],sort_keys=True,separators=(',',':')).encode()).hexdigest()))
(O/'proof/HOST-CENSUS.json').write_text(json.dumps(dict(status='PASS',binary_sha256=d['binary_sha256'],model='32-bit newlib chunks max(16,(request+11)&~7); pointer-bearing host requests deliberately retained as conservative target-size upper bounds; old+new realloc overlap',scope='All Rust GlobalAlloc requests across host threads. C/GL/OS allocations, native fragmentation and physical RAM excluded. No claim of an exact target-ABI process census.',max_extra_above_max_endpoints_bytes=max(r['extra_above_max_endpoints_bytes'] for r in rows),rows=rows),indent=2)+'\n')
(O/'proof/SAVE-PAYLOAD-EQUALITY.json').write_text(json.dumps(dict(status='PASS',rows=saves),indent=2)+'\n')
print('PASS allocation census and old Auto payload equality')
