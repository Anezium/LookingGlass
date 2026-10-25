from pathlib import Path
import json,re,math,csv,sys,hashlib
from run_health import health_text, arm_for_label
from light_csv import read as read_light, summarize as summarize_light, check_observers
from heap_readout import readout as heap_readout
r=Path(__file__).resolve().parent
def stats(values):
 v=list(values)
 if any(x is None for x in v):return None
 v.sort()
 return dict(n=len(v),median=v[len(v)//2],p95=v[len(v)*95//100],min=v[0],max=v[-1],sum=sum(v),mean=sum(v)/len(v)) if v else None
def kv(line):return dict(re.findall(r'(\w+)=([^ ]+)',line))
def audio(text):
 groups={k:[kv(l) for l in text.splitlines() if tag in l and must in l] for k,tag,must in [('realtime','[lg/audio-realtime]','producer_blocks='),('music','[lg/audio-music]','decode_ms='),('cpal','[cpal/vita]','late_callbacks=')]}
 metrics={}
 for group,rows in groups.items():
  keys=sorted(set().union(*(x.keys() for x in rows))) if rows else []
  metrics[group]={}
  for k in keys:
   try:values=[float(x[k]) for x in rows]
   except (ValueError,KeyError):continue
   metrics[group][k]=stats(values)
 return dict(window_counts={k:len(v) for k,v in groups.items()},metrics=metrics,raw=groups,dropped_reports_note='Individual reported values; sum is not a cumulative loss total.')
def audit_offmain(text,entry):
 lines=[l for l in text.splitlines() if '[lg/audio-offmain]' in l]
 expected=False  # Both r5 arms keep the old off-main lever disabled.
 engagement=[kv(l) for l in lines if 'requested=' in l or 'active=' in l]
 wanted='true' if expected else 'false'
 assert engagement and all(x.get('requested')==wanted and x.get('active')==wanted for x in engagement), 'audio_offmain requested/active mismatch or missing receipt'
 counters=[]
 for line in lines:
  row=kv(line)
  if any(k in row for k in ('submitted','reused','fallback')):
   values={k:int(row[k]) for k in ('submitted','reused','fallback')}
   assert all(v>=0 for v in values.values()), 'Negative offmain counter'
   counters.append(values)
 return dict(status='PASS',requested=expected,active=expected,counters=counters,
     fallback_policy='Report each raw window; nonzero fallback is not a stop and values are not pooled')

def audit(label,root=r,package=None,protocol=None):
 root=Path(root);package=Path(package) if package else root.parent
 protocol=Path(protocol) if protocol else package/'CONSOLE-PROTOCOL.json'
 run=root/label
 # Inspect the CSV and actual observer counters first, including on B2.
 coll=json.loads((run/'COLLECT.json').read_text())
 rows=read_light(coll['files'][0]['local'])
 availability=summarize_light(rows)
 observer_checks=check_observers(label,availability,protocol)
 assert len(rows)==120
 entry=__import__('light_csv').protocol_entry(label,protocol)
 mapname=entry['map']
 identity=json.loads((run/'SELF-IDENTITY.json').read_text())
 binary=Path(identity['local_readback_path'])
 expected_self=json.loads((package/'packages'/entry['package']/'VERIFICATION.json').read_text())
 assert binary.stat().st_size==expected_self['self_bytes']
 assert hashlib.sha256(binary.read_bytes()).hexdigest()==expected_self['self_sha256']
 t=(run/'runtime-log.txt').read_text(errors='replace')
 assert '[lg/frame-total] measured=120 gl_hooks=0 phase_timers=0 clock=begin-to-begin' in t
 assert [x['frame'] for x in rows]==list(range(1,121))
 configs=[json.loads(l.split('[lg/vita-renderer] config ',1)[1]) for l in t.splitlines() if '[lg/vita-renderer] config ' in l]
 expected=json.loads((run/'inputs/vita_renderer.json').read_text());assert len(configs)==1
 assert json.loads((run/'inputs/vita_sim.json').read_text())==json.loads((package/'configs/vita-sim-all.json').read_text())
 assert json.loads((run/'inputs/native-timing-config.json').read_text())==json.loads((package/'configs/native-timing-disabled.json').read_text())
 assert (run/'inputs/profile_mode').read_bytes()==(package/'configs/profile_mode-fullbench.txt').read_bytes()
 assert expected==json.loads((package/'configs'/('PL-'+entry['config_arm']+'.json')).read_text())
 assert configs[0].get('phase_census',False)==entry.get('census',False)
 assert expected.get('phase_census',False)==entry.get('census',False)
 assert configs[0].get('skin_actual_consumers',False)==(entry['arm']=='B')
 if entry.get('census',False):
  from census_report import validate
  validate(json.loads(Path(coll['files'][2]['local']).read_text()),mapname)
 assert expected['profiler']=='off' and expected['profile_capture'] is True
 for k,v in expected.items():assert math.isclose(configs[0][k],v,rel_tol=1e-6,abs_tol=1e-9) if isinstance(v,float) else configs[0][k]==v,(label,k)
 assert '[lg/startup] clocks mode=max rc=[0, 0, 0, 0] actual=[444, 222, 222, 166]' in t
 from map_identity import verify_runtime
 verify_runtime(t,entry,frozen=True)
 if entry['map']=='pandemonium':
  from map_identity import verify_save_receipt
  verify_save_receipt(json.loads((run/'SAVE-IDENTITY.json').read_text()))
 assert 'retained=true' in t and 'retained construction failed' not in t
 assert '[lg/audio-output] buffered=true' in t
 for policy in ['effective_priority=161 effective_affinity=0x40000','effective_priority=170 effective_affinity=0x40000','effective_priority=128 effective_affinity=0x20000']:assert policy in t
 sim=[json.loads(l.split('[lg/vita-sim] config ',1)[1]) for l in t.splitlines() if '[lg/vita-sim] config ' in l]
 assert sim==[json.loads((package/'configs/vita-sim-all.json').read_text())]
 assert t.count('bench: camera frozen map='+mapname)==1
 assert 'bench: done 120 frames mode=fullbench profiler=light' in t and 'Viewer closed cleanly.' in t
 before,tail=t.split('bench: camera frozen map='+mapname,1);bench,after=tail.split('bench: done 120 frames mode=fullbench',1)
 doors=list(csv.DictReader(Path(coll['files'][1]['local']).open()))
 assert [(d['frame_first'],d['frame_last'],d['frames']) for d in doors]==[('1','60','60'),('61','120','60')]
 assert all(d[k]=='' for d in doors for k in ('hatter_door_world_traces','hatter_door_own_traces','hatter_door_trace_pairs_deferred'))
 reasons=health_text(t,A=arm_for_label(label,protocol)=='A');assert not reasons,reasons
 audio_bench=audio(bench)
 assert all(v>0 for v in audio_bench['window_counts'].values()),'Missing benchmark audio counters'
 phases=[k+'_cpu_us' for k in ['world_opaque','world_alpha','actors','fx','hud','present']]
 offmain=[l for l in t.splitlines() if '[lg/audio-offmain]' in l]
 offmain_audit=audit_offmain(t,entry)
 result=dict(status='PASS',observer_checks=observer_checks,gl_observers=availability['gl_observers'],unavailable_fields=availability['unavailable_fields'],timing_gate_field='wall_us',map=mapname,runtime_config_matches_readback=True,actual_config=configs[0],actual_clocks_MHz=[444,222,222,166],retained=True,complete_benchmark=True,clean_exit=True,fields_ms={k:stats(None if x[k] is None else x[k]/1000 for x in rows) for k in rows[0] if k.endswith('_us')},six_render_phases_ms=stats(None if any(x[k] is None for k in phases) else sum(x[k] for k in phases)/1000 for x in rows),counter_stats={k:stats(x[k] for x in rows) for k in rows[0] if not k.endswith('_us') and k not in ['frame','gl_observers']},doors=list(csv.DictReader(Path(coll['files'][1]['local']).open())),audio=dict(entry_loading_information_only=audio(before),benchmark_including_warmup=audio_bench,shutdown_information=audio(after)),offmain_lines=offmain,offmain_audit=offmain_audit,memory_samples=[kv(l) for l in t.splitlines() if '[miniquad/vita] memory reason=' in l],heap_readout=heap_readout(run),camera_lines=[l for l in t.splitlines() if 'camera_eye=' in l or 'bench: camera frozen' in l],world_age='unavailable',complete_NPC_state='unavailable',listening='unavailable',continuous_loading_peak='not measured; sampled receipts only')
 (run/'AUDIT.json').write_text(json.dumps(result,indent=2)+'\n')
 print(label,'PASS wall ms',result['fields_ms']['wall_us'])
 return result
if __name__=='__main__':
 import argparse
 parser=argparse.ArgumentParser(description='Host-only per-run audit; checks protocol observer counters before other run evidence.')
 parser.add_argument('labels',nargs='+');parser.add_argument('--root',type=Path,default=r)
 parser.add_argument('--package',type=Path);parser.add_argument('--protocol',type=Path)
 args=parser.parse_args()
 for label in args.labels:audit(label,args.root,args.package,args.protocol)
