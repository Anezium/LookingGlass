"""Assemble already-collected r5 console evidence. Never accesses the console."""
from pathlib import Path
import argparse,hashlib,json
from census_report import report,markdown

def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--root',type=Path,required=True);p.add_argument('--package',type=Path);p.add_argument('--output',type=Path,required=True);a=p.parse_args();package=a.package or a.root.parent
 protocol=json.loads((package/'CONSOLE-PROTOCOL.json').read_text());results=[];text=['# Alice phase census r5 console results','']
 for mapname in ['pandemonium','hedge3']:
  seq=[e for e in protocol['sequence'] if e['map']==mapname];assert [e['arm'] for e in seq]==['A','B','B','A']
  block=seq[0]['block'];gate=json.loads((a.root/(block+'-ABBA-GATE.json')).read_text());assert gate['status'] in ['OBSERVER_PERTURBED','NO_RESOLVED_OBSERVER_COST']
  audits=[json.loads((a.root/e['label']/'AUDIT.json').read_text()) for e in seq];assert all(v['status']=='PASS' for v in audits)
  heaps=[max([int(s.get('newlib_uordblks',s.get('heap_uordblks_bytes',0))) for s in v['memory_samples']],default=None) for v in audits];assert all(h is not None and h>0 and h<=180*1048576 for h in heaps)
  hd=[(heaps[1]-heaps[0])/1048576,(heaps[2]-heaps[3])/1048576]
  def bad(v,which):
   fields=['pcm_starved_blocks','rejected_sinks','dropped_reports','underrun_frames','underrun_estimates','late_callbacks']
   return any(float(row.get(k,0))>0 for rows in v['audio'][which]['raw'].values() for row in rows for k in fields)
  loading={which:[bad(v,which) for v in audits] for which in ['entry_loading_information_only','shutdown_information']}
  loading_regression=any((v[1] or v[2]) and not v[0] and not v[3] for v in loading.values())
  row={'map':mapname,'observer_cost':gate,'sampled_heap_maxima_MiB':[h/1048576 for h in heaps],'heap_oriented_differences_MiB':hd,'heap_status':'CENSUS_HEAP_NOT_CONFIRMED' if any(v>1 for v in hd) else 'WITHIN_CENSUS_HEAP_ALLOWANCE','loading_shutdown_nonzero_raw_flags':loading,'loading_status':'AUDIO_LOADING_REGRESSION' if loading_regression else 'NO_ASYMMETRIC_LOADING_COUNTERS','breakdowns':[]}
  text += [f"## {mapname}: {gate['status']}",'',f"Oriented wall B-A: {gate['oriented_pair_deltas_ms']} ms; mean {gate['average_pair_delta_ms']} ms. Observer cost is never subtracted.",f"Sampled heap maxima A1/B2/B3/A4: {row['sampled_heap_maxima_MiB']} MiB; oriented differences {hd} MiB. {row['heap_status']}.",row['loading_status']+'. Loading/shutdown raw windows remain in each AUDIT.json.','']
  for e in seq:
   if e['arm']!='B':continue
   c=json.loads((a.root/e['label']/'COLLECT.json').read_text())['files'][2];raw=Path(c['local']).read_bytes();assert hashlib.sha256(raw).hexdigest()==c['sha256']
   summary=report(json.loads(raw));summary['label']=e['label'];summary['observer_status']=gate['status'];row['breakdowns'].append(summary)
   text += [f"### {e['label']}: {gate['status']}",'',markdown(summary)]
  if mapname=='pandemonium':
   mean=gate['A_average_arm_median_ms'];row['descriptive_new_self_parity']={'reference_ms':41.861,'new_A_mean_ms':mean,'flag':abs(mean-41.861)>1,'non_gating':True};text += [f"Descriptive cross-SELF A mean {mean:.3f} ms vs 41.861 ms; {'NEW_SELF_PARITY_FLAG' if abs(mean-41.861)>1 else 'no parity flag'}. Non-gating.",'']
  results.append(row)
 a.output.write_text(json.dumps({'status':'DIAGNOSTIC_ONLY_NO_PROMOTION','maps':results},indent=2)+'\n');a.output.with_suffix('.md').write_text('\n'.join(text)+'\n')
if __name__=='__main__':main()
