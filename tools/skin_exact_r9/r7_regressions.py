"""Run new real-asset witnesses on r7 plus host-only oracle hooks; expect failures."""
from pathlib import Path
import hashlib,json,os,subprocess,sys
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();B=O/'proof/r7-host-bin'
base=json.loads((R/'tools/lever_matrix_r4/configs/PL-baseline.json').read_text());out={'r7_revision':'777460d','host_only_patch':'proof/R7-ORACLE-ONLY.patch','binary_sha256':hashlib.sha256(B.read_bytes()).hexdigest(),'cases':[]}
for case in ['frozen-card','cat-fade','shared-slots']:
 arms=[]
 for arm in [False,True]:
  d=O/'host'/f'r7-regression-{case}-{int(arm)}';(d/'private').mkdir(parents=True,exist_ok=True)
  (d/'private/vita_renderer.json').write_text(json.dumps(dict(base,skin_actual_consumers=arm)))
  env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')};env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SKIN_PROOF='1',LOOKING_GLASS_SKIN_EXACT_FIXTURES='1',LOOKING_GLASS_SKIN_EXACT_CASE=case)
  cmd=[str(B),'--data','/Users/anezium/vita-work/alice-data/Alice1/bin/base','--no-audio','--skin-overlap-render-check','--save-dir',str(d/'private/saves')]
  with (d/'run.log').open('w') as f:p=subprocess.run(cmd,cwd=d,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=300)
  lines=(d/'run.log').read_text().splitlines();frames=[json.loads(l.split(' ',1)[1]) for l in lines if l.startswith('SIM_MAPS_FRAME ')];images=[json.loads(l.split(' ',1)[1]) for l in lines if l.startswith('SKIN_EXACT_IMAGE ')]
  arms.append(dict(arm=arm,exit_code=p.returncode,frames=frames,images=images,errors=[l for l in lines if 'stale GPU' in l or 'panicked' in l]))
 assert arms[0]['exit_code']==0 and len(arms[0]['frames'])==180,(case,'bad reference')
 differences=[]
 for a,b in zip(arms[0]['images'],arms[1]['images']):
  if a!=b:differences.append(a['frame'])
 packet_diff=[]
 for a,b in zip(arms[0]['frames'],arms[1]['frames']):
  canonical=lambda f:[{k:v for k,v in p.items() if k!='gpu_slot'} for p in f['submitted_packets']]
  if canonical(a)!=canonical(b):packet_diff.append(a['frame'])
 detected=bool(differences or packet_diff or arms[1]['errors'])
 assert detected,(case,'did not expose r7 defect')
 row=dict(case=case,status='EXPECTED_R7_FAILURE',first_different_image_frame=min(differences,default=None),different_image_frames=differences,first_different_gpu_packet_frame=min(packet_diff,default=None),on_exit_code=arms[1]['exit_code'],on_errors=arms[1]['errors'])
 out['cases'].append(row);(O/'proof/R7-REGRESSIONS.json').write_text(json.dumps(out,indent=2)+'\n');print(row,flush=True)
out['status']='PASS_THREE_R7_DEFECTS_DETECTED';(O/'proof/R7-REGRESSIONS.json').write_text(json.dumps(out,indent=2)+'\n')
