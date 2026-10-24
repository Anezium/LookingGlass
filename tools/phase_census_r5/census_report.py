"""Offline census integrity and phase table. No device or driver access."""
from pathlib import Path
import argparse,json,math

def stats(v):
 v=sorted(v);assert v
 return {'n':len(v),'median':v[len(v)//2],'p95':v[len(v)*95//100],'mean':sum(v)/len(v),'min':v[0],'max':v[-1],'sum':sum(v)}

def validate(data,mapname):
 assert data['schema']==1 and data['map']==mapname and data['capacity']==180
 names=data['regions'];n=len(names);assert n==24 and len(set(names))==n
 assert data['parent_mask_frame_bit']==n and data['owners']==['audio','player','npc','camera','other']
 rows=data['rows'];assert len(rows)==180
 assert [r['accepted_frame'] for r in rows]==list(range(1,181))
 assert [r['measured_frame'] for r in rows]==[None]*60+list(range(1,121))
 assert all(a['sequence']<b['sequence'] for a,b in zip(rows,rows[1:]))
 for r in rows:
  assert r['hierarchy_errors']==0 and r['frame_total_ns']>0
  assert r['frame_total_ns']==r['bracket_union_ns']+r['unbracketed_residual_ns']
  assert r['unbracketed_residual_ns']>=0
  for field in ['inclusive_ns','exclusive_ns','calls','parent_masks']:assert len(r[field])==n
  assert sum(r['exclusive_ns'])==r['bracket_union_ns']
  assert all(0<=e<=i for e,i in zip(r['exclusive_ns'],r['inclusive_ns']))
  assert all((m & ~((1<<(n+1))-1))==0 for i,m in enumerate(r['parent_masks']))
  assert len(r['sweep_count'])==len(r['sweep_ns'])==5
  assert r['acoustics_world_sweep_count']<=r['sweep_count'][0]
  assert r['acoustics_world_sweep_ns']<=r['sweep_ns'][0]
  for total,k1,k4,k16 in r['static_queries_total_hit1_hit4_hit16']:assert 0<=k1<=k4<=k16<=total
  assert r['live_table_evictions']>=0
 return rows[60:]

def report(data):
 rows=validate(data,data['map']);names=data['regions'];n=len(names)
 result={'map':data['map'],'frames':120,'frame_total_ms':stats([r['frame_total_ns']/1e6 for r in rows]),'residual_ms':stats([r['unbracketed_residual_ns']/1e6 for r in rows]),'regions':[],'collision':[],'acoustic_opportunity':[]}
 for i,name in enumerate(names):
  mask=0
  for r in rows:mask|=r['parent_masks'][i]
  parents=[names[j] if j<n else 'frame_total' for j in range(n+1) if mask&(1<<j)]
  result['regions'].append({'region':name,'parents':parents,'inclusive_ms':stats([r['inclusive_ns'][i]/1e6 for r in rows]),'exclusive_ms':stats([r['exclusive_ns'][i]/1e6 for r in rows]),'calls':sum(r['calls'][i] for r in rows)})
 for i,name in enumerate(data['owners']):result['collision'].append({'owner':name,'count':sum(r['sweep_count'][i] for r in rows),'elapsed_ms':stats([r['sweep_ns'][i]/1e6 for r in rows])})
 result['acoustics_world_sweep']={'count':sum(r['acoustics_world_sweep_count'] for r in rows),'elapsed_ms':stats([r['acoustics_world_sweep_ns']/1e6 for r in rows])}
 for i,name in enumerate(data['query_classes']):
  sums=[sum(r['static_queries_total_hit1_hit4_hit16'][i][j] for r in rows) for j in range(4)]
  result['acoustic_opportunity'].append({'class':name,'queries':sums[0],'hits_K1_K4_K16':sums[1:],'hit_fraction':[v/sums[0] if sums[0] else None for v in sums[1:]]})
 result['live_table_evictions']=sum(r['live_table_evictions'] for r in rows)
 result['opportunity_exact_within_bounded_history']=not any(r['live_table_evictions'] for r in data['rows'])
 result['exclusive_plus_residual_mean_ms']=sum(v['exclusive_ms']['mean'] for v in result['regions'])+result['residual_ms']['mean']
 result['warning']='Inclusive rows overlap; collision/acoustic sweeps are nested views, never added to the frame partition. All elapsed times include scheduling/API waits. GPU attribution is unavailable.'
 return result

def markdown(result):
 lines=[f"# {result['map']} phase census (diagnostic)",'',result['warning'],'','| Region | Observed parent(s) | Inclusive median / p95 / mean ms | Exclusive mean ms | Calls |','|---|---|---:|---:|---:|']
 for row in result['regions']:
  s=row['inclusive_ms'];lines.append(f"| {row['region']} | {', '.join(row['parents']) or 'absent'} | {s['median']:.3f} / {s['p95']:.3f} / {s['mean']:.3f} | {row['exclusive_ms']['mean']:.3f} | {row['calls']} |")
 s=result['frame_total_ms'];r=result['residual_ms'];lines+=['',f"Frame total median / p95 / mean: {s['median']:.3f} / {s['p95']:.3f} / {s['mean']:.3f} ms.",f"Unbracketed residual median / p95 / mean: {r['median']:.3f} / {r['p95']:.3f} / {r['mean']:.3f} ms.",f"Exclusive means + residual mean: {result['exclusive_plus_residual_mean_ms']:.6f} ms. Do not sum medians or percentiles.",'','Collision by owner (nested elapsed view):']
 for c in result['collision']:lines.append(f"- {c['owner']}: {c['count']} sweeps; {c['elapsed_ms']['mean']:.3f} mean ms/frame")
 lines+=['','Acoustic exact-key opportunity (counter only):']
 for q in result['acoustic_opportunity']:lines.append(f"- {q['class']}: {q['queries']} queries; K=1/4/16 hits {q['hits_K1_K4_K16']}; fractions {q['hit_fraction']}")
 lines+=['',f"Live table evictions: {result['live_table_evictions']}. Exact within bounded history: {result['opportunity_exact_within_bounded_history']}. Lost history makes opportunity a lower bound."]
 return '\n'.join(lines)+'\n'

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('input',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args();r=report(json.loads(a.input.read_text()));a.output.write_text(json.dumps(r,indent=2)+'\n');a.output.with_suffix('.md').write_text(markdown(r))
if __name__=='__main__':main()
