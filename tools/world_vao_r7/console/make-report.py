"""Assemble r7 wall-only evidence offline; optional census is never a gate arm."""
from pathlib import Path
import argparse,hashlib,json

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,required=True);p.add_argument('--package',type=Path);p.add_argument('--output',type=Path,required=True);p.add_argument('--diagnostic-only',action='store_true');a=p.parse_args();package=a.package or a.root.parent
 protocol=json.loads((package/'CONSOLE-PROTOCOL.json').read_text());maps=[]
 for name in ['pandemonium','hedge3']:
  seq=[e for e in protocol['sequence'] if e['map']==name]
  assert [e['arm'] for e in seq]==['A','B','B','A'] and all(not e['census'] for e in seq)
  gate=json.loads((a.root/(seq[0]['block']+'-ABBA-GATE.json')).read_text()) if not a.diagnostic_only else dict(status='NOT_A_WALL_SESSION',oriented_pair_deltas_ms=[])
  audits=[json.loads((a.root/e['label']/'AUDIT.json').read_text()) for e in seq] if not a.diagnostic_only else []
  assert all(v['status']=='PASS' for v in audits)
  for e in ([] if a.diagnostic_only else seq):assert json.loads((a.root/e['label']/'STRICT-AUDIO.json').read_text())['status']=='PASS'
  windows=[v['heap_readout'] for v in audits];heaps=[v['benchmark']['sampled_max_MiB'] for v in windows]
  assert all(h is not None and 0<h<=180 for h in heaps)
  hd=[heaps[1]-heaps[0],heaps[2]-heaps[3]] if heaps else []
  fields=['pcm_starved_blocks','rejected_sinks','dropped_reports','underrun_frames','underrun_estimates','late_callbacks']
  flags={w:[any(float(row.get(k,0))>0 for rows in v['audio'][w]['raw'].values() for row in rows for k in fields) for v in audits] for w in ['entry_loading_information_only','shutdown_information']}
  regression=any((f[1] or f[2]) and not f[0] and not f[3] for f in flags.values()) if audits else False
  row=dict(map=name,wall_gate=gate,heap_windows=windows,heap_oriented_differences_MiB=hd,heap_status='HEAP_NOT_CONFIRMED' if any(d>1 for d in hd) else 'WITHIN_1_MIB',loading_shutdown_nonzero_raw_flags=flags,loading_status='AUDIO_LOADING_REGRESSION' if regression else 'NO_ASYMMETRIC_LOADING_COUNTERS',optional_census=[])
  for entry in protocol.get('optional_sequence',[]):
   collect=a.root/entry['label']/'COLLECT.json'
   if entry['map']!=name or not collect.exists():continue
   run=collect.parent
   for receipt in ['AUDIT.json','STRICT-AUDIO.json','LOG-INTEGRITY.json']:assert json.loads((run/receipt).read_text())['status']=='PASS'
   item=json.loads(collect.read_text())['files'][2];raw=Path(item['local']).read_bytes();assert hashlib.sha256(raw).hexdigest()==item['sha256']
   from census_report import validate
   data=json.loads(raw);validate(data,name);rows=data['rows'][60:]
   from census_report import stats
   table=[]
   for region in ['world_submit','world_vertex_input','world_state_bindings','world_uniform_writes','world_draw_calls']:
    index=data['regions'].index(region)
    table.append(dict(region=region,inclusive_ms=stats([r['inclusive_ns'][index]/1e6 for r in rows]),exclusive_ms=stats([r['exclusive_ns'][index]/1e6 for r in rows]),calls_per_frame=sum(r['calls'][index] for r in rows)/120))
   row['optional_census'].append(dict(label=entry['label'],arm=entry['arm'],diagnostic_only=True,table=table,reading='Nested regions; never sum inclusive rows. No observer correction or Vita CPU/GPU attribution.'))
  row['diagnostic_pair_complete']={v['arm'] for v in row['optional_census']}=={'A','B'}
  maps.append(row)
 promoted=maps[0]['wall_gate']['status']=='NONREGRESSION' and maps[1]['wall_gate']['status']=='BENEFIT' and all(m['heap_status']=='WITHIN_1_MIB' and m['loading_status']!='AUDIO_LOADING_REGRESSION' for m in maps)
 result=dict(status='DIAGNOSTICS_ONLY' if a.diagnostic_only else 'BENEFIT_GATES_PASS' if promoted else 'NO_PROMOTION',reading_rule=protocol['pass_rule'],maps=maps,diagnostics_complete=all(m['diagnostic_pair_complete'] for m in maps),hardware_gameplay_correctness='Finite idle runs do not establish full moving-gameplay, portal/reflection or console image equivalence. Switch remains default-off.')
 a.output.write_text(json.dumps(result,indent=2)+'\n')
 text=['# Alice world vertex input r7 console results','',result['status'],'',protocol['pass_rule'],'']
 for m in maps:text += [m['map']+': '+m['wall_gate']['status'],str(m['wall_gate']['oriented_pair_deltas_ms'])+' ms oriented B-A',str(m['heap_oriented_differences_MiB'])+' MiB sampled benchmark heap deltas; '+m['heap_status'],m['loading_status'],'Optional census: '+json.dumps(m['optional_census']),'']
 a.output.with_suffix('.md').write_text('\n'.join(text)+'\n')
if __name__=='__main__':main()
