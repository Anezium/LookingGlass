"""Attribute persistent small requests and backend table high-water storage."""
from pathlib import Path
import collections,json,subprocess,sys
O=Path(sys.argv[1]).resolve();D=O/'resource-diagnostic';samples=[];backend=[]
for line in (D/'repeated/chapter-1/run.log').open():
 if line.startswith('R11C_SMALL '):
  row=json.loads(line.split(' ',1)[1]);base,values=row['allocations']
  samples.append((row['frame'],{(p,tuple(stack)):n for p,n,stack in values}))
 if line.startswith('R11C_BACKEND_COUNTS '):backend.append(line.strip())
assert len(samples)>=30
survivors=set(samples[-1][1]) & set(samples[-2][1]) & set(samples[-3][1])
new=survivors-set(samples[3][1]);groups=collections.defaultdict(list)
for key in new:
 birth=next(i for i,(_,allocs) in enumerate(samples) if key in allocs)
 groups[(samples[-1][1][key],key[1])].append(birth)
results=[]
for (size,stack),births in sorted(groups.items(),key=lambda kv:len(kv[1])*kv[0][0],reverse=True):
 addresses=[hex(v) for v in stack if v]
 symbols=subprocess.check_output(['atos','-o',str(D/'diagnostic-bin'),'-l',hex(base),*addresses],text=True)
 results.append(dict(size=size,count=len(births),birth_samples=sorted(births),symbols=symbols))
 print(size,len(births),sorted(births),symbols[:1800])
(O/'proof/SMALL-ALLOCATION-ATTRIBUTION.json').write_text(json.dumps(dict(status='ATTRIBUTED',diagnostic_only=True,groups=results,backend_samples=backend),indent=2)+'\n')
