"""Matching 180-frame entry scripts: r7 versus r9 vertex workloads, no GPU observer."""
from pathlib import Path
import hashlib,json,os,subprocess,sys
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();B=O/'proof/r7-host-bin';new=json.loads((O/'proof/HOST-MAPS.json').read_text());base=json.loads((R/'tools/lever_matrix_r4/configs/PL-baseline.json').read_text());out={'r7_binary_sha256':hashlib.sha256(B.read_bytes()).hexdigest(),'r9_binary_sha256':new['binary_sha256'],'measured_frames':'61..180','maps':{}}
for m in dict.fromkeys(r['map'] for r in new['runs']):
 rows=[]
 for arm in [False,True]:
  d=O/'host'/f'r7-counts-{m}-{int(arm)}';(d/'private').mkdir(parents=True,exist_ok=True)
  (d/'private/vita_renderer.json').write_text(json.dumps(dict(base,skin_actual_consumers=arm,phase_census=True)))
  env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')};env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT={'pandemonium':'0.04184','hedge3':'0.06'}.get(m,'0.05'),LOOKING_GLASS_SIM_MAPS_COUNTERS='0',LOOKING_GLASS_SKIN_MOVING_VIEW='1',LOOKING_GLASS_SKIN_DEATHS='1')
  with (d/'run.log').open('w') as f:p=subprocess.run([str(B),'--data','/Users/anezium/vita-work/alice-data/Alice1/bin/base','--map',m,'--no-audio','--frames','180','--save-dir',str(d/'private/saves')],cwd=d,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=300)
  assert p.returncode==0,(m,arm)
  rs=json.loads((d/f'{m}-census-180.json').read_text())['rows'][60:];r9=next(r for r in new['runs'] if r['map']==m and r['arm']==arm)
  r7={k:sum(a[k] for r in rs for a in r['actors'])/120 for k in ['serial_vertices','worker_vertices']};r9={k:r9[k] for k in r7}
  rows.append(dict(arm=arm,r7=r7,r9=r9,delta={k:r9[k]-r7[k] for k in r7}))
 out['maps'][m]=rows;(O/'proof/R7-VERTEX-COMPARISON.json').write_text(json.dumps(out,indent=2)+'\n');print(m,rows,flush=True)
out['status']='PASS';(O/'proof/R7-VERTEX-COMPARISON.json').write_text(json.dumps(out,indent=2)+'\n')
