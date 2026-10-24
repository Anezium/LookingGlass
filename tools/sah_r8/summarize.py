from pathlib import Path
import hashlib,json,re,sys
O=Path(sys.argv[1]).resolve();log=(O/'logs/corpus-tests.log').read_text();maps={}
for line in log.splitlines():
 if line.startswith('SAH_LOAD '):
  row=json.loads(line.split(' ',1)[1]);maps.setdefault(row['map'],{})['load']=row
 if line.startswith('SAH_CORPUS '):
  row=json.loads(line.split(' ',1)[1]);row['owner']=['audio','player','npc','camera','other'][row['owner']]
  n=row['queries'];row['per_query']={k:({field:value/n for field,value in row[k].items()}if n else None)for k in ['old','sah']}
  # Raw timing loops also scan unrelated owners; they are not isolated kernel timings.
  row['timing_scope']='counter-enabled host loop with owner filtering; concurrent native/host builds; descriptive only'
  if not n:row['abba_ns_per_query']=None
  maps.setdefault(row['map'],{}).setdefault('owners',{})[row['owner']]=row
 if line.startswith('SAH_MASKS '):
  row=dict(re.findall(r'(\w+)=([^ ]+)',line));maps[row['map']]['masks']={k:int(v)for k,v in row.items()if k!='map'}
assert len(maps)==4 and all(len(v['owners'])==5 for v in maps.values())
total=sum(r['queries']for m in maps.values()for r in m['owners'].values());masks=sum(m['masks']['queries']for m in maps.values())
for m in maps.values():
 for row in m['owners'].values():assert row['mismatches']==0 and row['old']['planes']==row['sah']['planes'] and row['old']['clipped']==row['sah']['clipped']
 for k in ['newlib32_retained','newlib32_peak_upper_bound']:m['load'][k+'_MiB']=m['load'][k]/1048576
h=maps['hedge3']['load'];summary=dict(status='PASS',recorded_uncached_static_queries=total,additional_mask_edge_queries=masks,maps=maps,heap_budget=dict(fixed_before_console_data=True,expected_retained_delta_MiB=h['newlib32_retained']/1048576,expected_build_peak_delta_upper_bound_MiB=h['newlib32_peak_upper_bound']/1048576,oriented_benchmark_delta_limit_MiB=8,hedge3_plateau_limit_exclusive_MiB=165,whole_run_stop_above_MiB=180,r6_reference_plateau_MiB=146.96,modelled_plateau_MiB=146.96+h['newlib32_retained']/1048576,r6_reference_loading_sample_MiB=161.027618,modelled_loading_upper_bound_from_that_sample_MiB=161.027618+h['newlib32_peak_upper_bound']/1048576),limits='Allocation-request high-water ledger with conservative old+new realloc overlap, not continuous process heap or allocator fragmentation. Same array element sizes on 32-bit ABI; newlib chunks max(16,(request+11)&~7). No elapsed Mac result predicts Vita milliseconds. Zero-query owner population is explicit, not synthesized.')
(O/'proof/HOST-SUMMARY.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({k:summary[k]for k in ['recorded_uncached_static_queries','additional_mask_edge_queries','heap_budget']},indent=2))
