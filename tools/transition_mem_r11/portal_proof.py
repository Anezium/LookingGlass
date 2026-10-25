"""Draw a real portal on the live outgoing Scene, then use chapter transition."""
from pathlib import Path
import hashlib,json,os,subprocess,sys
O=Path(sys.argv[1]).resolve();B=Path(sys.argv[2]).resolve();R=Path(__file__).resolve().parents[2]
base=json.loads((R/'tools/lever_matrix_r4/configs/PL-baseline.json').read_text())
rows=[]
for name in ['hatter1','keep','fortress1','skool2']:
 d=O/'portal'/name;(d/'private/saves').mkdir(parents=True,exist_ok=True)
 config=dict(base,legacy_maps=[name],phase_census=False,profile_capture=False,audio_music_thread=False)
 (d/'private/vita_renderer.json').write_text(json.dumps(config))
 env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
 env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT='0.05',LOOKING_GLASS_SAH_SILENT_AUDIO='1',LOOKING_GLASS_TRANSITION_ACTION='chapter',LOOKING_GLASS_TRANSITION_DESTINATION='hedge3')
 if name!='skool2':env['LOOKING_GLASS_TRANSITION_PORTAL_CAPTURE']=str(d/'portal.png')
 with (d/'run.log').open('w') as f:p=subprocess.run([str(B),'--data','/Users/anezium/vita-work/alice-data/Alice1/bin/base','--map',name,'--save-dir',str(d/'private/saves'),'--frames','8'],cwd=d,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=180)
 log=(d/'run.log').read_text();assert p.returncode==0,(name,p.returncode,log[-1500:])
 if name!='skool2':assert 'TRANSITION_PORTAL_ON_SCREEN ready=' in log
 ends=[json.loads(l.split('] ',1)[1]) for l in log.splitlines() if l.startswith('[lg/transition-memory] ') and '"event":"end"' in l]
 assert any(r['kind']=='chapter-or-console' and r['from']==name and r['to']=='hedge3' and r['success'] for r in ends)
 rows.append(dict(map=name,exit_code=0,portal_on_screen=name!='skool2',chapter_to_hedge3=True,capture_sha256=hashlib.sha256((d/'portal.png').read_bytes()).hexdigest() if name!='skool2' else None,note='No camera portal in this corpus; transition control' if name=='skool2' else 'Real portal rendered before chapter operation'))
 (O/'proof/PORTAL-TRANSITIONS.json').write_text(json.dumps(dict(status='PASS',binary_sha256=hashlib.sha256(B.read_bytes()).hexdigest(),rows=rows),indent=2)+'\n')
 print('PASS portal on screen and chapter out',name,flush=True)
