from pathlib import Path
import os,json,subprocess,hashlib
O=Path('/Users/anezium/vita-j0/resume-2026-10-24/alice-skin-consumers-r7');R=Path('/Users/anezium/vita-work/lg-skin-consumers-r7');B=O/'proof/host-bin'
base=json.loads((R/'tools/lever_matrix_r4/configs/PL-baseline.json').read_text());result={'binary_sha256':hashlib.sha256(B.read_bytes()).hexdigest(),'scope':'Mac main-thread actor_gather; fixed dt, normal entry, no audio, proof oracle disabled. No Vita conversion.','maps':{}}
for m,dt in [('pandemonium','0.04184'),('hedge3','0.06'),('hatter1','0.05')]:
 rows=[]
 for n,(arm,kind) in enumerate([(False,'time'),(True,'time'),(True,'time'),(False,'time'),(False,'alloc'),(True,'alloc'),(False,'census'),(True,'census')],1):
  d=O/'host'/f'{m}-{n}-{kind}';(d/'private').mkdir(parents=True,exist_ok=True)
  (d/'private/vita_renderer.json').write_text(json.dumps(dict(base,skin_actual_consumers=arm,phase_census=kind=='census')))
  env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
  env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT=dt,LOOKING_GLASS_SIM_MAPS_COUNTERS='1' if kind=='alloc' else '0',LOOKING_GLASS_SKIN_MOVING_VIEW='1')
  if kind=='alloc':env['LOOKING_GLASS_SKIN_ALLOC']='1'
  cmd=[str(B),'--data','/Users/anezium/vita-work/alice-data/Alice1/bin/base','--map',m,'--no-audio','--frames','180','--save-dir',str(d/'private/saves')]
  with (d/'run.log').open('w') as f:p=subprocess.run(cmd,cwd=d,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=360)
  assert p.returncode==0,(d,p.returncode)
  frames=[json.loads(l.split(' ',1)[1]) for l in (d/'run.log').read_text().splitlines() if l.startswith('SIM_MAPS_FRAME ')]
  assert len(frames)==180
  row=dict(arm=arm,kind=kind,actor_gather_ms=[f['spans_ms']['actor_gather'] for f in frames[60:]],states=[f['state_sha256'] for f in frames])
  if kind=='alloc':
   for key in ['skin_numeric_allocation_calls','skin_worker_prepare_allocation_calls','skin_rejected_prepare_allocation_calls']:
    row[key]=[f['counters'].get(key,0) for f in frames[60:]]
   row['numeric_capture_allocation_calls']=row['skin_numeric_allocation_calls']
  if kind=='census':
   import sys;sys.path.insert(0,str(R/'tools/skin_consumers_r7/console'))
   from census_report import validate,report
   data=json.loads((d/f'{m}-census-180.json').read_text());validate(data,m)
   row['census_report']=report(data);rs=data['rows'][60:]
   row['mean_serial_vertices']=sum(a['serial_vertices'] for r in rs for a in r['actors'])/120
   row['mean_worker_vertices']=sum(a['worker_vertices'] for r in rs for a in r['actors'])/120
  rows.append(row)
  assert row['states']==rows[0]['states'],(m,kind,'simulation')
  print('DONE',m,n,kind,arm,('alloc='+str(sum(row['numeric_capture_allocation_calls'])) if kind=='alloc' else ''),flush=True)
 times=rows[:4];means=[sum(r['actor_gather_ms'])/120 for r in times];med=[sorted(r['actor_gather_ms'])[60] for r in times]
 alloc=[r['numeric_capture_allocation_calls'] for r in rows if r['kind']=='alloc']
 assert all(b<=a for a,b in zip(*alloc)),(m,'new per-frame allocation increase')
 result['maps'][m]=dict(runs=rows,ABBA_mean_ms=means,ABBA_median_ms=med,oriented_mean_delta_ms=[means[1]-means[0],means[2]-means[3]],oriented_median_delta_ms=[med[1]-med[0],med[2]-med[3]])
 result['maps'][m]['no_added_numeric_capture_allocations_on_any_measured_frame']=True
 (O/'proof/HOST-MEASURE.json').write_text(json.dumps(result,indent=2)+'\n')
result['status']='PASS';(O/'proof/HOST-MEASURE.json').write_text(json.dumps(result,indent=2)+'\n')
