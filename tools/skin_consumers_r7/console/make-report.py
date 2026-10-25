"""Offline wall gate and separate actor sub-brackets; accept one or both restored sessions."""
from pathlib import Path
import argparse,hashlib,json
from census_report import validate,report

def main():
 p=argparse.ArgumentParser(description=__doc__)
 p.add_argument('--root',type=Path,action='append',required=True)
 p.add_argument('--package',type=Path);p.add_argument('--output',type=Path,required=True)
 a=p.parse_args();package=a.package or a.root[0].parent
 protocol=json.loads((package/'CONSOLE-PROTOCOL.json').read_text());maps=[]
 for name in ['pandemonium','hedge3']:
  seq=[e for e in protocol['sequence'] if e['map']==name]
  roots=[r for r in a.root if (r/seq[0]['label']/'AUDIT.json').exists()]
  if not roots:continue
  assert len(roots)==1;root=roots[0]
  assert [e['arm'] for e in seq]==['A','B','B','A'] and all(not e['census'] for e in seq)
  gate=json.loads((root/(seq[0]['block']+'-ABBA-GATE.json')).read_text())
  audits=[json.loads((root/e['label']/'AUDIT.json').read_text()) for e in seq]
  for e in seq:
   for receipt in ['AUDIT.json','STRICT-AUDIO.json','LOG-INTEGRITY.json']:
    assert json.loads((root/e['label']/receipt).read_text())['status']=='PASS'
  windows=[v['heap_readout'] for v in audits];heaps=[v['benchmark']['sampled_max_MiB'] for v in windows]
  assert all(h is not None and 0<h<=180 for h in heaps)
  hd=[heaps[1]-heaps[0],heaps[2]-heaps[3]]
  fields=['pcm_starved_blocks','rejected_sinks','dropped_reports','underrun_frames','underrun_estimates','late_callbacks']
  flags={w:[any(float(row.get(k,0))>0 for rows in v['audio'][w]['raw'].values() for row in rows for k in fields) for v in audits] for w in ['entry_loading_information_only','shutdown_information']}
  regression=any((f[1] or f[2]) and not f[0] and not f[3] for f in flags.values())
  row=dict(map=name,wall_gate=gate,heap_windows=windows,heap_oriented_differences_MiB=hd,
   heap_status='HEAP_NOT_CONFIRMED' if any(d>1 for d in hd) else 'WITHIN_1_MIB',
   loading_shutdown_nonzero_raw_flags=flags,loading_status='AUDIO_LOADING_REGRESSION' if regression else 'NO_ASYMMETRIC_LOADING_COUNTERS',diagnostic_census=[])
  entries=[e for e in protocol['optional_sequence'] if e['map']==name]
  for entry in entries:
   run=root/entry['label']
   if not (run/'COLLECT.json').exists():continue
   for receipt in ['AUDIT.json','STRICT-AUDIO.json','LOG-INTEGRITY.json']:
    assert json.loads((run/receipt).read_text())['status']=='PASS'
   item=json.loads((run/'COLLECT.json').read_text())['files'][2]
   raw=Path(item['local']).read_bytes();assert hashlib.sha256(raw).hexdigest()==item['sha256']
   data=json.loads(raw);validate(data,name)
   rows=data['rows'][60:];actor_totals={}
   for frame in rows:
    for actor in frame['actors']:
     out=actor_totals.setdefault(str(actor['id']),dict(serial_vertices=0,worker_vertices=0,serial_reasons=0,reason_vertices=[0]*8))
     for k in ['serial_vertices','worker_vertices']:out[k]+=actor[k]
     out['serial_reasons']|=actor['serial_reasons']
     out['reason_vertices']=[x+y for x,y in zip(out['reason_vertices'],actor['reason_vertices'])]
   table=report(data)
   diagnostic_audit=json.loads((run/'AUDIT.json').read_text())
   row['diagnostic_census'].append(dict(label=entry['label'],arm=entry['arm'],diagnostic_only=True,
    heap_windows=diagnostic_audit['heap_readout'],
    loading_shutdown_audio={k:diagnostic_audit['audio'][k] for k in ['entry_loading_information_only','shutdown_information']},
    serial_reasons=data['serial_reasons'],actor_totals_120_frames=actor_totals,
    regions=table['regions'],worker_service_ns=[sum(r['worker_skin_ns'][i] for r in rows) for i in range(2)],
    warning='Elapsed scopes include preemption/API waits. Worker service overlaps main elapsed time; never add it. No observer subtraction.'))
  row['diagnostics_complete']=len(row['diagnostic_census'])==2
  maps.append(row)
 assert maps,'No complete wall block found'
 passed=len(maps)==2 and maps[0]['wall_gate']['status']=='BENEFIT' and maps[1]['wall_gate']['status']=='NONREGRESSION'
 passed=passed and all(m['diagnostics_complete'] and m['heap_status']=='WITHIN_1_MIB' and m['loading_status']!='AUDIO_LOADING_REGRESSION' for m in maps)
 result=dict(status='BENEFIT_GATES_PASS' if passed else 'NO_PROMOTION',reading_rule=protocol['pass_rule'],maps=maps,
  hardware_gameplay_correctness='Finite idle runs do not establish all moving gameplay transitions.')
 late={}
 for root in a.root:
  for entry in protocol['sequence']+protocol.get('optional_sequence',[]):
   receipt=root/entry['label']/'STRICT-AUDIO.json'
   if receipt.exists():late[entry['label']]=json.loads(receipt.read_text())['BENCHMARK_LATE_CALLBACKS']
 result['BENCHMARK_LATE_CALLBACKS']=late
 a.output.write_text(json.dumps(result,indent=2)+'\n')
 a.output.with_suffix('.md').write_text('# Alice skin consumers r7 console results\n\n'+result['status']+'\n\n'+protocol['pass_rule']+'\n\n'+'\n'.join(m['map']+': '+m['wall_gate']['status']+'; B-A '+str(m['wall_gate']['oriented_pair_deltas_ms'])+' ms; diagnostics complete='+str(m['diagnostics_complete']) for m in maps)+'\n\nBENCHMARK_LATE_CALLBACKS (report-only; sum of benchmark window counts):\n'+'\n'.join(label+': '+str(v['total'])+'; per window '+str(v['per_window']) for label,v in late.items())+'\n')
if __name__=='__main__':main()
