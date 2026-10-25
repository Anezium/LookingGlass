"""Real assets: serial-reference consumer witnesses and forced ambient refresh, Mac only."""
from pathlib import Path
import hashlib,json,os,subprocess,sys
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();B=O/'proof/host-bin'
base=json.loads((R/'tools/lever_matrix_r4/configs/PL-baseline.json').read_text())
result={'scope':'Host fixtures only; native scheduling/performance unmeasured','consumer_checks':[]}
for kind in ([] if '--ambient-only' in sys.argv else ['exact','ambient']):
 d=O/'host'/('consumers-final-'+kind);(d/'private').mkdir(parents=True,exist_ok=True)
 (d/'private/vita_renderer.json').write_text(json.dumps(dict(base,skin_actual_consumers=True)))
 if kind=='ambient':(d/'private/vita_sim.json').write_text(json.dumps(dict(anim_cull=False,anim_far_distance=1,anim_far_every=4)))
 cmd=[str(B),'--data','/Users/anezium/vita-work/alice-data/Alice1/bin/base','--no-audio','--skin-overlap-render-check','--save-dir',str(d/'private/saves')]
 env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
 with (d/'run.log').open('w') as f:p=subprocess.run(cmd,cwd=d,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=360)
 assert p.returncode==0,(kind,p.returncode)
 text=(d/'run.log').read_text();assert '60 byte-identical frames' in text
 result['consumer_checks'].append(dict(kind=kind,status='PASS',comparisons=60,images={str(p.relative_to(d)):hashlib.sha256(p.read_bytes()).hexdigest() for p in d.rglob('*.png')}))
 print('PASS',kind,'60 byte-identical consumer comparisons',flush=True)
# Force actual ambient refresh/reuse through production Npcs::draw, retaining the
# census reason witness and per-frame full packet and simulation comparisons.
arms=[]
for arm in [False,True]:
 d=O/'host'/f'ambient-refresh-{arm}';(d/'private').mkdir(parents=True,exist_ok=True)
 (d/'private/vita_renderer.json').write_text(json.dumps(dict(base,skin_actual_consumers=arm,phase_census=True)))
 (d/'private/vita_sim.json').write_text(json.dumps(dict(anim_cull=False,anim_far_distance=1,anim_far_every=4)))
 env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
 env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT='0.05',LOOKING_GLASS_SIM_MAPS_COUNTERS='0',LOOKING_GLASS_SKIN_PROOF='1',LOOKING_GLASS_SKIN_MOVING_VIEW='1')
 cmd=[str(B),'--data','/Users/anezium/vita-work/alice-data/Alice1/bin/base','--map','skool1','--no-audio','--frames','180','--save-dir',str(d/'private/saves')]
 with (d/'run.log').open('w') as f:p=subprocess.run(cmd,cwd=d,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=360)
 assert p.returncode==0
 frames=[json.loads(l.split(' ',1)[1]) for l in (d/'run.log').read_text().splitlines() if l.startswith('SIM_MAPS_FRAME ')]
 assert len(frames)==180
 data=json.loads((d/'skool1-census-180.json').read_text());sys.path.insert(0,str(R/'tools/skin_exact_r9/console'));from census_report import validate
 validate(data,'skool1')
 vertices=sum(a['reason_vertices'][4] for r in data['rows'] for a in r['actors'])
 assert vertices>0,'Fixture failed to exercise an ambient mutable consumer'
 arms.append(dict(arm=arm,frames=frames,ambient_serial_vertices=vertices))
for a,b in zip(arms[0]['frames'],arms[1]['frames']):
 for k in ['actor_skin','state_sha256','selected_lights_bits','joint_alpha_order','blend_world_order']:assert a[k]==b[k],k
 assert [{k:v for k,v in p.items() if k!='gpu_slot'} for p in a['submitted_packets']]==[{k:v for k,v in p.items() if k!='gpu_slot'} for p in b['submitted_packets']]
result.update(status='PASS',ambient_refresh=arms,binary_sha256=hashlib.sha256(B.read_bytes()).hexdigest())
(O/'proof/HOST-CONSUMERS.json').write_text(json.dumps(result,indent=2)+'\n')
print('PASS ambient refresh: 180 frames, real serial reason, exact actor/packet/simulation bits',flush=True)
