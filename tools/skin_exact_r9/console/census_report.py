"""Offline census integrity and phase table. No device or driver access."""
from pathlib import Path
import argparse,json,math

def stats(v):
 v=sorted(v);assert v
 return {'n':len(v),'median':v[len(v)//2],'p95':v[len(v)*95//100],'mean':sum(v)/len(v),'min':v[0],'max':v[-1],'sum':sum(v)}

def validate(data,mapname):
 assert data['schema']==2 and data['map']==mapname and data['capacity']==180
 names=data['regions'];n=len(names);assert n==30 and len(set(names))==n
 assert data['query_classes']==['room','ambient','effects'] and data['table_entries']==4096 and data['table_probes']==16
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
  assert all(isinstance(v,int) and v>=0 for field in ['inclusive_ns','exclusive_ns','calls','parent_masks','sweep_count','sweep_ns'] for v in r[field])
  assert sum(r['exclusive_ns'])==r['bracket_union_ns']
  assert all(0<=e<=i for e,i in zip(r['exclusive_ns'],r['inclusive_ns']))
  assert all((m & ~((1<<(n+1))-1))==0 for i,m in enumerate(r['parent_masks']))
  assert not any(m & (1<<n) for m in r['parent_masks'][24:]), 'Actor subdivisions escaped actor_gather'
  assert len(r['sweep_count'])==len(r['sweep_ns'])==5
  assert 0<=r['acoustics_world_sweep_count']<=r['sweep_count'][0]
  assert 0<=r['acoustics_world_sweep_ns']<=r['sweep_ns'][0]
  assert len(r['static_queries_total_hit1_hit4_hit16'])==3
  for total,k1,k4,k16 in r['static_queries_total_hit1_hit4_hit16']:assert 0<=k1<=k4<=k16<=total
  assert r['live_table_evictions']>=0
  assert data['actor_capacity']==128 and len(data['serial_reasons'])==8
  assert len(r['actors'])<=128 and len({a['id'] for a in r['actors']})==len(r['actors'])
  assert len(r['worker_skin_ns'])==2 and all(isinstance(v,int) and v>=0 for v in r['worker_skin_ns'])
  for a in r['actors']:
   assert a['id']>=0 and a['serial_vertices']>=0 and a['worker_vertices']>=0 and 0<=a['serial_reasons']<256
   assert len(a['reason_vertices'])==8 and all(0<=v<=a['serial_vertices'] for v in a['reason_vertices'])
   assert all(bool(a['serial_reasons']&(1<<i))==bool(v) for i,v in enumerate(a['reason_vertices']))
   assert (a['serial_vertices']==0)==(a['serial_reasons']==0)
 return rows[60:]

def report(data,wall_delta_ms=None,observer_status=None):
 rows=validate(data,data['map']);names=data['regions'];n=len(names)
 result={'map':data['map'],'frames':120,'frame_total_ms':stats([r['frame_total_ns']/1e6 for r in rows]),'residual_ms':stats([r['unbracketed_residual_ns']/1e6 for r in rows]),'regions':[],'collision':[],'acoustic_opportunity':[]}
 for i,name in enumerate(names):
  mask=0
  for r in rows:mask|=r['parent_masks'][i]
  parents=[names[j] if j<n else 'frame_total' for j in range(n+1) if mask&(1<<j)]
  children=[names[j] for j in range(n) if any(r['parent_masks'][j]&(1<<i) for r in rows)]
  nested=name in parents
  result['regions'].append({'region':name,'parents':parents,'children':children,'nesting':'SELF_NESTED' if nested else 'ordinary',
   'inclusive_ms':None if nested else stats([r['inclusive_ns'][i]/1e6 for r in rows]),
   'exclusive_ms':stats([r['exclusive_ns'][i]/1e6 for r in rows]),'calls':sum(r['calls'][i] for r in rows),
   'calls_per_frame':sum(r['calls'][i] for r in rows)/120})
 for i,name in enumerate(data['owners']):result['collision'].append({'owner':name,'count':sum(r['sweep_count'][i] for r in rows),'elapsed_ms':stats([r['sweep_ns'][i]/1e6 for r in rows])})
 result['acoustics_world_sweep']={'count':sum(r['acoustics_world_sweep_count'] for r in rows),'elapsed_ms':stats([r['acoustics_world_sweep_ns']/1e6 for r in rows])}
 for i,name in enumerate(data['query_classes']):
  sums=[sum(r['static_queries_total_hit1_hit4_hit16'][i][j] for r in rows) for j in range(4)]
  result['acoustic_opportunity'].append({'class':name,'queries':sums[0],'hits_K1_K4_K16':sums[1:],'hit_fraction':[v/sums[0] if sums[0] else None for v in sums[1:]]})
 result['live_table_evictions']=sum(r['live_table_evictions'] for r in rows)
 result['warmup_live_table_evictions']=sum(r['live_table_evictions'] for r in data['rows'][:60])
 result['all_accepted_live_table_evictions']=sum(r['live_table_evictions'] for r in data['rows'])
 result['opportunity_exact_within_bounded_history']=not any(r['live_table_evictions'] for r in data['rows'])
 result['exclusive_plus_residual_mean_ms']=sum(v['exclusive_ms']['mean'] for v in result['regions'])+result['residual_ms']['mean']
 # Only root inclusive values with exact per-frame closure are additive.
 root_bit=1<<n
 exact_roots=[v['region'] for v in result['regions'] if v['parents']==['frame_total']]
 mixed=[v['region'] for v in result['regions'] if 'frame_total' in v['parents'] and v['parents']!=['frame_total']]
 usable=[i for i,row in enumerate(result['regions']) if row['nesting']!='SELF_NESTED']
 closing=[r for r in rows if sum(r['inclusive_ns'][i] for i in usable if r['parent_masks'][i]==root_bit)==r['bracket_union_ns']]
 root_rows=[]
 if closing:
  for i in usable:
   name=names[i]
   if any(r['parent_masks'][i]==root_bit for r in closing):
    root_rows.append(dict(region=name,root_inclusive_mean_ms=sum(r['inclusive_ns'][i] if r['parent_masks'][i]==root_bit else 0 for r in closing)/len(closing)/1e6))
 result['root_partition']={'exact_root_regions':exact_roots,'mixed_root_and_nested_regions':mixed,'frames':len(closing),
  'rows':root_rows,'residual_mean_ms':sum(r['unbracketed_residual_ns'] for r in closing)/len(closing)/1e6 if closing else None,
  'frame_total_mean_ms':sum(r['frame_total_ns'] for r in closing)/len(closing)/1e6 if closing else None,
  'per_frame_integer_closure':True,'scope':'Only frames whose usable root inclusive sum equals bracket_union_ns; SELF_NESTED inclusive values are excluded entirely and non-root occurrences contribute zero'}
 events=[sum(r['calls'])+sum(r['sweep_count']) for r in rows]
 epf=sum(events)/120
 cost=wall_delta_ms/epf if wall_delta_ms is not None and epf else None
 result['observer_bias']={'assumption':'ASSUMED_UNIFORM','events_per_frame':stats(events),'implied_signed_ms_per_event':cost,
  'mean_oriented_wall_delta_ms':wall_delta_ms,'note':'Longer B frames add physics ticks (dt feedback). No observer cost is subtracted. Bounds use the absolute implied cost.'}
 owner={'npc_think_movement':2,'player_movement':1,'audio_prepare':0}
 for row in result['regions']:
  attributable=row['calls_per_frame']+(sum(r['sweep_count'][owner[row['region']]] for r in rows)/120 if row['region'] in owner else 0)
  bound=attributable*abs(cost) if cost is not None else None
  blocked=observer_status=='OBSERVER_PERTURBED' and bound is not None and bound>0.2*row['exclusive_ms']['mean']
  row.update(attributable_events_per_frame=attributable,assumed_uniform_bias_bound_ms=bound,
   lever_ranking='DO_NOT_RANK_BIAS_EXCEEDS_20_PERCENT' if blocked else 'EXCLUSIVE_AND_ROOT_ONLY' if cost is not None else 'UNAVAILABLE_WITHOUT_WALL_DELTA',
   follow_up='Reduced census: regions only, sweep timing off' if blocked else None)
 result['nesting_annotations']=['fx_gather inclusive contains alpha_prepare and joint_finish, including all alpha, actor and FX submission.',
  'world_visibility and world_parameters sum opaque and alpha passes and cannot be split.',
  'npc_think_movement sums five controller entry points. Lever ranking uses exclusive means and the root table only.']
 result['acoustic_reading']={'room_ambient_K':4,'K1_near_zero_expected':True,
  'history_bound':'LOWER_BOUND' if not result['opportunity_exact_within_bounded_history'] else 'EXACT_WITHIN_BOUNDED_HISTORY',
  'limits':'K counts viewer frames; room/ambient use a 0.1 s clock. Neutral input and a stationary listener make repeats optimistic for moving gameplay. Exact keys cover static geometry only; World::sweep also traces dynamic, settled and weapon geometry. Lever 1 can save at most the unmeasured static share of acoustics_world_sweep_ns. Evictions give a lower bound on counts; idle pose and dynamic geometry give an upper bound on real savings.'}
 result['collision_coverage']='Outermost sweep, sweep_except, camera_sweep and sweep_geometry only. Excludes direct ledge_trace (src/ledge.rs:40,58), light-certificate static traversals (src/collision.rs:812-822) and npc_sight cache hits. Vita-only set_owner attribution was not exercised by host proofs. Movers, objects, story, particles, lights and first person land in other.'
 fraction=result['residual_ms']['mean']/result['frame_total_ms']['mean']
 result['residual_reading']={'fraction_of_frame_mean':fraction,'label':'RESIDUAL_UNATTRIBUTED' if fraction>0.15 else 'WITHIN_15_PERCENT',
  'known_contents':'Input and pre-object viewer work, story, stats, particle simulation, camera, power-art, first person and capture, save tail, profiler bookkeeping, and next_frame except vglSwapBuffers (assumed to include macroquad HUD batch flush). Never assign to a subsystem.',
  'follow_up':'Name existing Region::Other intervals in src/viewer.rs at 2576, 3288, 3713, 3811, 4197 in a later build.'}
 result['warning']='Inclusive rows overlap; collision/acoustic sweeps are nested views, never added to the frame partition. All elapsed times include scheduling/API waits. GPU attribution is unavailable.'
 return result

def markdown(result):
 lines=[f"# {result['map']} phase census (diagnostic)",'',result['warning'],'',
  '| Region | Observed parents | Observed children | Inclusive median / p95 / mean ms | Exclusive mean ms | Calls/frame | ASSUMED_UNIFORM bias bound ms | Ranking |',
  '|---|---|---|---:|---:|---:|---:|---|']
 for row in result['regions']:
  s=row['inclusive_ms'];inc=f"{s['median']:.3f} / {s['p95']:.3f} / {s['mean']:.3f}" if s else 'SELF_NESTED: inclusive unavailable'
  lines.append(f"| {row['region']} | {', '.join(row['parents']) or 'absent'} | {', '.join(row['children']) or 'none'} | {inc} | {row['exclusive_ms']['mean']:.3f} | {row['calls_per_frame']:.3f} | {row['assumed_uniform_bias_bound_ms']} | {row['lever_ranking']} |")
 lines+=['']+result['nesting_annotations']
 root=result['root_partition']
 lines+=['',f"Root partition table: {root['frames']}/120 closing frames. Mixed root/nested regions: {root['mixed_root_and_nested_regions']}.",root['scope'],'','| Root region | Inclusive mean ms on closing frames |','|---|---:|']
 for row in root['rows']:lines.append(f"| {row['region']} | {row['root_inclusive_mean_ms']:.6f} |")
 lines += [f"| Residual | {root['residual_mean_ms']} |",f"| Frame total | {root['frame_total_mean_ms']} |"]
 bias=result['observer_bias'];lines+=['',f"ASSUMED_UNIFORM events/frame: {bias['events_per_frame']['mean']}; implied signed ms/event: {bias['implied_signed_ms_per_event']}.",bias['note']]
 follow=[r['region'] for r in result['regions'] if r['follow_up']]
 if follow:lines+=['Reduced-census follow-up (regions only, sweep timing off): '+', '.join(follow)+'.']
 s=result['frame_total_ms'];r=result['residual_ms'];lines+=['',f"Frame total median / p95 / mean: {s['median']:.3f} / {s['p95']:.3f} / {s['mean']:.3f} ms.",f"Residual median / p95 / mean: {r['median']:.3f} / {r['p95']:.3f} / {r['mean']:.3f} ms.",f"Exclusive means + residual mean: {result['exclusive_plus_residual_mean_ms']:.6f} ms. Per-frame integer closure is verified; do not sum medians/p95s.",'','| Collision owner (nested) | Sweeps | Mean elapsed ms/frame |','|---|---:|---:|']
 for c in result['collision']:lines.append(f"| {c['owner']} | {c['count']} | {c['elapsed_ms']['mean']:.3f} |")
 lines+=['',result['collision_coverage'],'','| Acoustic class | Queries | K=1/4/16 hits | K=1/4/16 fractions |','|---|---:|---|---|']
 for q in result['acoustic_opportunity']:lines.append(f"| {q['class']} | {q['queries']} | {q['hits_K1_K4_K16']} | {q['hit_fraction']} |")
 lines += ['',f"Acoustic world sweeps (nested audio subset): {result['acoustics_world_sweep']}",f"Live table evictions measured/warmup/all: {result['live_table_evictions']}/{result['warmup_live_table_evictions']}/{result['all_accepted_live_table_evictions']}; history: {result['acoustic_reading']['history_bound']}. Read K=4 for room/ambient; K=1 near zero is expected.",result['acoustic_reading']['limits'],'',result['residual_reading']['label'],result['residual_reading']['known_contents'],result['residual_reading']['follow_up']]
 return '\n'.join(lines)+'\n'

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('input',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args();r=report(json.loads(a.input.read_text()));a.output.write_text(json.dumps(r,indent=2)+'\n');a.output.with_suffix('.md').write_text(markdown(r))
if __name__=='__main__':main()
