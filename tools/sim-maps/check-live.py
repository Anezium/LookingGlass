#!/usr/bin/env python3
"""Deterministic live viewer windows. Only private cells are writable."""
import argparse, hashlib, json, os, pathlib, statistics, subprocess, time, shutil, tarfile
LOCK=pathlib.Path.home()/'vita-j0/mac-timing.lock'
FLAGS=['sim_school_mesh','sim_tower_templates','sim_npc_queries','sim_mechanism_state']
DT={'skool2':.414199,'hedge3':.301543,'tower2':.253179,'funhouse':.141028,'hatter1':.114675,'potears3':.019476,'fortress1':.032037,'gvillage':.036372,'skool1':.088292,'pandemonium':.04184}
P=argparse.ArgumentParser();
for name in ['tree','evidence','binary','data','config','sim']: P.add_argument('--'+name,type=pathlib.Path,required=True)
P.add_argument('--fix',required=True,choices=FLAGS+['all']);P.add_argument('--maps',nargs='+',required=True);P.add_argument('--label',required=True);P.add_argument('--frames',type=int);P.add_argument('--states',action='store_true');a=P.parse_args()
ROOT=a.evidence.resolve();ROOT.mkdir(parents=True,exist_ok=True)
TREE=a.tree.resolve();DATA=a.data.resolve();BINARY=a.binary.resolve()
def check_lock():
 if LOCK.exists(): raise RuntimeError('Timing lock active: '+LOCK.read_text())
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
snapshot=ROOT/'sources'/a.label;snapshot.mkdir(parents=True,exist_ok=True)
shutil.copy2(BINARY,snapshot/'host-binary')
with tarfile.open(snapshot/'source.tar.gz','w:gz') as archive:
 for name in ['src','Cargo.toml','Cargo.lock','build.rs','.cargo']:
  archive.add(TREE/name,arcname=name)
result={'source_archive_sha256':sha(snapshot/'source.tar.gz'),'source':subprocess.check_output(['git','rev-parse','HEAD'],cwd=TREE,text=True).strip(),'binary_sha256':sha(BINARY),'fix':a.fix,'timing':False,'scope':'full live viewer, fixed timestep, neutral input, native cast; serialized simulation state and published transforms each frame','maps':[]}
for m in a.maps:
 cells=[]; frames=a.frames or (1200 if m=='funhouse' else 300)
 for arm in ['off','on']:
  cell=ROOT/'gates'/a.label/m/arm; private=cell/'private';private.mkdir(parents=True,exist_ok=True)
  cfg=json.loads(a.config.read_text())
  cfg.update({f:arm=='on' and (a.fix=='all' or f==a.fix) for f in FLAGS})
  (private/'vita_renderer.json').write_text(json.dumps(cfg,indent=2)+'\n')
  (private/'vita_sim.json').write_bytes(a.sim.read_bytes())
  env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
  env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT=str(DT.get(m,.05)))
  if a.states: env['LOOKING_GLASS_SIM_MAPS_STATE']='1'
  check_lock()
  with (cell/'run.log').open('w') as log:
   r=subprocess.run([str(BINARY),'--data',str(DATA),'--map',m,'--no-audio','--frames',str(frames),'--save-dir',str(private/'saves')],cwd=cell,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=300)
  txt=(cell/'run.log').read_text();assert r.returncode==0,(cell,r.returncode,txt[-2000:]);assert 'invalid config' not in txt
  rows=[json.loads(x.split(' ',1)[1]) for x in txt.splitlines() if x.startswith('SIM_MAPS_FRAME ')]
  assert len(rows)==frames,(cell,len(rows));assert all(r['map']==m for r in rows)
  window=rows[-240:];assert all(r['active'] for r in window),(m,arm,'inactive window')
  counters={k:statistics.mean(r['counters'].get(k,0) for r in window) for k in sorted({k for r in window for k in r['counters']})}
  cells.append({'arm':arm,'rows':rows,'counters_per_frame':counters,'window':[window[0]['frame'],window[-1]['frame']],'log':str((cell/'run.log').relative_to(ROOT))})
  print('RUN',m,arm,frames,flush=True)
 diffs=[i for i,(x,y) in enumerate(zip(cells[0]['rows'],cells[1]['rows'])) if x['state_sha256']!=y['state_sha256']]
 row={'map':m,'frames':frames,'equal_all_frame_hashes':not diffs,'first_difference':diffs[0] if diffs else None,'cells':[{k:v for k,v in c.items() if k!='rows'} for c in cells]}
 for c in cells:
  row[c['arm']+'_window_hash']=hashlib.sha256(''.join(r['state_sha256'] for r in c['rows'][-240:]).encode()).hexdigest()
 result['maps'].append(row);(ROOT/f'GATE-{a.label}.json').write_text(json.dumps(result,indent=2)+'\n')
 print('PARITY',m,not diffs,'first difference',row['first_difference'],flush=True)
 assert not diffs,(m,diffs[:10])
