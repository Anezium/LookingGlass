#!/usr/bin/env python3
"""Read-only console CSV summary; retain every selected stall and raw source row."""
import argparse,csv,json,math,statistics
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('console',type=Path);p.add_argument('--out',type=Path,required=True);a=p.parse_args()
def stats(values):
 s=sorted(values);return {'n':len(s),'median':statistics.median(s),'p95':s[math.floor(.95*len(s))],'mean':statistics.mean(s)}
runs=[]
for m,pairs in [('gvillage',3),('potears3',1),('hatter1',1),('potears1',1),('tower2',1)]:
 for pair in range(1,pairs+1):
  arms={}
  for arm in ['on','off']:
   root=a.console/m/f'pair-{pair}'/arm;rows={};files=sorted(root.glob('*light-*.csv'));assert files,root
   for path in files:
    for raw in csv.DictReader(path.open()):
     row={k:int(v) for k,v in raw.items()};n=row['frame']
     if 61<=n<=180:
      assert n not in rows,(root,'duplicate',n)
      rows[n]=row
   assert sorted(rows)==list(range(61,181)),(root,'missing/extra frames')
   values=list(rows.values());assert 'sim_blocks_us' in values[0]
   # Service is a parallel elapsed axis; sim_blocks overlaps its exclusive constituents.
   new_fields=list(values[0])[124:]
   exclusive=[k for k in new_fields if k not in ['sim_blocks_us','skin_worker0_service_us','skin_worker1_service_us']]
   assert all(sum(r[k] for k in exclusive)<=r['frame_cpu_us'] for r in values),(root,'exclusive sum exceeds CPU body')
   if arm=='off':assert all(r[k]==0 for r in values for k in new_fields),(root,'off counters nonzero')
   else:assert any(r['sim_blocks_us']>0 for r in values),(root,'probe never ran')
   arms[arm]={'raw_files':[str(f) for f in files],'selected_rows':values,'columns':{k:stats([r[k] for r in values]) for k in values[0] if k!='frame'}}
  delta={k:{q:arms['on']['columns'][k][q]-arms['off']['columns'][k][q] for q in ['median','p95','mean']} for k in ['wall_us','frame_cpu_us','swap_us']}
  runs.append({'map':m,'pair':pair,'arms':arms,'probe_on_minus_off_us':delta})
pooled={}
for m in sorted({r['map'] for r in runs}):
 pooled[m]={}
 for arm in ['on','off']:
  rows=[v for r in runs if r['map']==m for v in r['arms'][arm]['selected_rows']]
  pooled[m][arm]={k:stats([r[k] for r in rows]) for k in rows[0] if k!='frame'}
a.out.write_text(json.dumps({'runs':runs,'pooled':pooled,'stalls_removed':0,'p95':'sorted[floor(.95*N)]','console_audio_and_provenance_require_manual_log_review':True},indent=2)+'\n')
print('PASS 14 launches / 1680 selected frames; audio and provenance review still required')
