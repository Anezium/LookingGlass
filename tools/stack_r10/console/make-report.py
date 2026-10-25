"""Assemble r8 wall-only evidence offline; optional census is never a gate arm."""
from pathlib import Path
import argparse,hashlib,json

def heap_status(name,heaps):
 assert len(heaps)==4 and all(h is not None and 0<h<=180 for h in heaps)
 delta=[heaps[1]-heaps[0],heaps[2]-heaps[3]]
 return 'HEAP_NOT_CONFIRMED' if any(d>8 for d in delta) or (name=='hedge3' and any(h>=165 for h in heaps)) else 'WITHIN_SAH_BUDGET'

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,required=True);p.add_argument('--package',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args();package=a.package or a.root.parent
 protocol=json.loads((package/'CONSOLE-PROTOCOL.json').read_text());maps=[]
 for name in ['pandemonium','hedge3']:
  seq=[e for e in protocol['sequence'] if e['map']==name]
  assert [e['arm'] for e in seq]==['A','B','B','A'] and all(not e['census'] for e in seq)
  gate=json.loads((a.root/(seq[0]['block']+'-ABBA-GATE.json')).read_text())
  audits=[json.loads((a.root/e['label']/'AUDIT.json').read_text()) for e in seq]
  assert all(v['status']=='PASS' for v in audits)
  for e in seq:assert json.loads((a.root/e['label']/'STRICT-AUDIO.json').read_text())['status']=='PASS'
  windows=[v['heap_readout'] for v in audits];heaps=[v['benchmark']['sampled_max_MiB'] for v in windows]
  assert all(h is not None and 0<h<=180 for h in heaps)
  hd=[heaps[1]-heaps[0],heaps[2]-heaps[3]]
  fields=['pcm_starved_blocks','rejected_sinks','dropped_reports','underrun_frames','underrun_estimates','underrun_estimates']
  flags={w:[any(float(row.get(k,0))>0 for rows in v['audio'][w]['raw'].values() for row in rows for k in fields) for v in audits] for w in ['entry_loading_information_only','shutdown_information']}
  regression=any((f[1] or f[2]) and not f[0] and not f[3] for f in flags.values())
  row=dict(map=name,wall_gate=gate,heap_windows=windows,heap_oriented_differences_MiB=hd,heap_status=heap_status(name,heaps),loading_shutdown_nonzero_raw_flags=flags,loading_status='AUDIO_LOADING_REGRESSION' if regression else 'NO_ASYMMETRIC_LOADING_COUNTERS',optional_census=[])
  for entry in protocol.get('optional_sequence',[]):
   collect=a.root/entry['label']/'COLLECT.json'
   if entry['map']!=name:continue
   assert collect.exists(), 'Both Hedge3 census arms are required for the complete report'
   run=collect.parent
   for receipt in ['AUDIT.json','STRICT-AUDIO.json','LOG-INTEGRITY.json']:assert json.loads((run/receipt).read_text())['status']=='PASS'
   item=json.loads(collect.read_text())['files'][2];raw=Path(item['local']).read_bytes();assert hashlib.sha256(raw).hexdigest()==item['sha256']
   from census_report import validate
   data=json.loads(raw);validate(data,name);index=data['regions'].index('prepare_acoustics');rows=data['rows'][60:]
   values=sorted(r['inclusive_ns'][index]/1e6 for r in rows)
   counters=[[sum(r['static_cache_hit_miss_eviction'][c][i] for r in rows) for i in range(3)] for c in range(3)]
   static=[dict(owner=owner,queries=sum(r['static_traversal_counts'][o][0] for r in rows),mean_ms=sum(r['static_traversal_ns'][o] for r in rows)/120e6,counts=[sum(r['static_traversal_counts'][o][j] for r in rows) for j in range(7)]) for o,owner in enumerate(data['owners'])]
   row['optional_census'].append(dict(label=entry['label'],diagnostic_only=True,arm=entry['arm'],static_traversal=static,prepare_acoustics_ms=dict(mean=sum(values)/120,median=values[60],p95=values[114]),query_classes=data['query_classes'],hit_miss_eviction=counters))
  maps.append(row)
 promoted=maps[0]['wall_gate']['status']=='NONREGRESSION' and maps[1]['wall_gate']['status']=='BENEFIT' and all(m['heap_status']=='WITHIN_SAH_BUDGET' and m['loading_status']!='AUDIO_LOADING_REGRESSION' for m in maps)
 from smoke_admission import verify
 smoke=verify(a.root)
 result=dict(smoke_admission=smoke,status='BENEFIT_GATES_PASS' if promoted else 'NO_PROMOTION',reading_rule=protocol['pass_rule'],separate_idle_gains_ms=protocol['separate_idle_gains_ms'],separate_gains_are_not_a_wall_prediction=True,maps=maps,hardware_gameplay_correctness='Finite idle runs do not establish moving-gameplay hit rates or full NPC state equality.')
 late={}
 for root in [a.root]:
  for entry in protocol['sequence']+protocol.get('optional_sequence',[])+protocol.get('smoke_sequence',[]):
   receipt=root/entry['label']/'STRICT-AUDIO.json'
   if receipt.exists():late[entry['label']]=json.loads(receipt.read_text())['BENCHMARK_LATE_CALLBACKS']
 result['BENCHMARK_LATE_CALLBACKS']=late
 a.output.write_text(json.dumps(result,indent=2)+'\n')
 text=['# Alice static SAH r8 console results','',result['status'],'',protocol['pass_rule'],'']
 for m in maps:text += [m['map']+': '+m['wall_gate']['status'],str(m['wall_gate']['oriented_pair_deltas_ms'])+' ms oriented B-A',str(m['heap_oriented_differences_MiB'])+' MiB sampled benchmark heap deltas; '+m['heap_status'],m['loading_status'],'Optional census: '+json.dumps(m['optional_census']),'']
 text += ['BENCHMARK_LATE_CALLBACKS (report-only; sum of benchmark window counts):']+[label+': '+str(v['total'])+'; per window '+str(v['per_window']) for label,v in late.items()]
 a.output.with_suffix('.md').write_text('\n'.join(text)+'\n')
if __name__=='__main__':main()
