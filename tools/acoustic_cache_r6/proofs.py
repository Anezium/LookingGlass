"""Summarize retained host logs without executing game/transport code."""
from pathlib import Path
import json,re,sys
O=Path(sys.argv[1]);samples={};maps={}
text=(O/'logs/test-cache-maps-production.log').read_text()
for line in text.splitlines():
 if line.startswith('AUDIO_MAIN_EQUAL'):
  v=dict(re.findall(r'(\w+)=([^\s]+)',line));samples[v['fixture']]=dict(frames=int(v['frames']),scalar_samples=int(v['scalar_samples']),sha256=v['sha256'],all_sample_bits_equal=True)
 if line.startswith('STATIC_CACHE_RANDOM_EQUAL'):
  v=dict(re.findall(r'(\w+)=([^\s]+)',line));maps[v['map']]=dict(random_queries=int(v['queries']),hits=int(v['hits']),misses=int(v['misses']),mutable_result_changes=int(v['mutable_result_changes']),scene_replacements=int(v['scene_replacements']),explicit_resets=int(v['explicit_resets']),kernel=[])
 if line.startswith('STATIC_CACHE_KERNEL'):
  v=dict(re.findall(r'(\w+)=([^\s]+)',line));maps[v['map']]['kernel'].append(dict(cache_on=v['enabled']=='true',frames=int(v['frames']),mean_ms=float(v['mean_ms']),hits=int(v['hits']),misses=int(v['misses'])))
 if line.startswith('STATIC_CACHE_ALLOCATION'):
  v=dict(re.findall(r'(\w+)=([^\s]+)',line));m=maps[v['map']];m['allocation_frames']=int(v['frames']);m['allocation_calls']=int(v['allocation_calls']);m['host_test_cache_bytes']=int(v['cache_bytes']);m['oriented_saved_ms']=json.loads(re.search(r'oriented_saved_ms=(\[.*\])',line)[1])
assert set(samples)=={'hedge3','gvillage','skool2','hatter1'} and set(maps)=={'hedge3','pandemonium'}
for m in maps.values():
 assert m['random_queries']==10240 and m['hits']+m['misses']==10240 and m['mutable_result_changes']>0 and m['allocation_calls']==0
 assert [r['cache_on'] for r in m['kernel']]==[False,True,True,False]
 for r in m['kernel']:r['hit_rate']=r['hits']/(r['hits']+r['misses']) if r['hits']+r['misses'] else None
 m['random_hit_rate']=m['hits']/m['random_queries']
result=dict(status='PASS',scope='Mac host. Production baseline simulation switches on; dynamic buckets off (unset test env selector). Not Vita timing, scheduler or moving-gameplay evidence.',samples=samples,maps=maps,allocation_scope='Fixed acoustic cache plus complete room/source trace kernel with current eager dynamic/settled/weapon geometry; publication outside scope. Synthetic actual Audio prepare_cached_acoustics separately measures zero allocations. No claim that complete game preparation or asset decode allocates nothing.')
(O/'proof/HOST-CACHE.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({m:dict(saved_ms=v['oriented_saved_ms'],allocation_calls=v['allocation_calls']) for m,v in maps.items()},indent=2))
