"""Machine-readable attribution of the review's discrete retained-memory steps."""
from pathlib import Path
import hashlib,json,re,sys
O=Path(sys.argv[1]).resolve();hist=O/'diagnostic-r11c/repeated/chapter-1/run.log';trace=O/'diagnostic-trace-r11c/repeated/chapter-1/run.log'
samples=[json.loads(l.split(' ',1)[1]) for l in hist.open() if l.startswith('TRANSITION_HEAP_SIZES ')]
assert len(samples)==31
steps=[]
for cycle,(left,right) in enumerate(zip(samples,samples[1:]),1):
 a=dict(left['sizes']);b=dict(right['sizes'])
 changes=[dict(request_bytes=n,count_delta=b.get(n,0)-a.get(n,0),byte_delta=n*(b.get(n,0)-a.get(n,0))) for n in a.keys()|b.keys() if a.get(n,0)!=b.get(n,0)]
 steps.append(dict(cycle=cycle,from_frame=left['frame'],to_frame=right['frame'],net_bytes=sum(c['byte_delta'] for c in changes),changes=sorted(changes,key=lambda c:abs(c['byte_delta']),reverse=True)))
step=steps[7];assert step['net_bytes']==541104
assert next(c['count_delta'] for c in step['changes'] if c['request_bytes']==540680)==-1
assert next(c['count_delta'] for c in step['changes'] if c['request_bytes']==1081352)==1
text=trace.read_text();blocks=text.split('R11C_ALLOCATION_TRACE ')[1:]
buffer=next(b.split('\n\n',1)[0] for b in blocks if b.startswith('size=1081352'))
pipeline=next(b.split('\n\n',1)[0] for b in blocks if b.startswith('size=107528'))
assert 'gl::Buffer' in buffer and 'Gpu>::upload' in buffer
assert 'gl::PipelineInternal' in pipeline
small=json.loads((O/'proof/SMALL-ALLOCATION-ATTRIBUTION.json').read_text())
console=[g for g in small['groups'] if 'Console5print' in g['symbols']]
assert {g['size'] for g in console}=={16,32}
final=json.loads((O/'proof/REPEATED-TRANSITIONS.json').read_text());assert final['status']=='PASS' and len(final['rows'])==6 and all(r['pass_trend'] for r in final['rows'])
result=dict(status='PASS',diagnostics_are_separate_from_final_census=True,
 histogram_sha256=hashlib.sha256(hist.read_bytes()).hexdigest(),histogram_steps=steps,
 cycle_8=dict(total_bytes=541104,buffer_table_capacity_bytes=540672,console_ring_capacity_bytes=384,console_text_bytes=48,buffer_allocation_stack=buffer),
 pipeline_capacity_step_bytes=53760,pipeline_allocation_stack=pipeline,
 weak_atlas_candidate_fix='Drop dead weak pending candidates at end of frame; 32/64/128/256 KiB capacity staircase eliminated',
 backend_table_fix='Reclaim capacity after large live-set retirement; live populations remain fixed per map',
 console=dict(per_round_trip_bytes=48,max_lines=128,max_characters_per_line=512,attribution=console,bounded_unit_test='console::tests::history_restores_draft_and_text_and_logs_are_bounded'),
 final_trend_binary_sha256=final['binary_sha256'])
(O/'proof/E2-GROWTH-DIAGNOSIS.json').write_text(json.dumps(result,indent=2)+'\n')
print('PASS histogram/backtrace attribution and final full-population trend')
