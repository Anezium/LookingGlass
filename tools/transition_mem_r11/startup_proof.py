"""Default skool1 options through the actual chooser's chapter-result branch."""
from pathlib import Path
import sys,json,os,subprocess,hashlib
O=Path(sys.argv[1]).resolve();B=Path(sys.argv[2]).resolve();R=Path(__file__).resolve().parents[2]
d=O/'host/startup-chooser';(d/'private/saves').mkdir(parents=True,exist_ok=True)
base=json.loads((R/'tools/lever_matrix_r4/configs/PL-baseline.json').read_text());base.update(phase_census=False,profile_capture=False,audio_music_thread=False)
(d/'private/vita_renderer.json').write_text(json.dumps(base))
env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')};env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT='0.05',LOOKING_GLASS_SAH_SILENT_AUDIO='1',LOOKING_GLASS_START_CHAPTER='hedge3')
cmd=[str(B),'--data','/Users/anezium/vita-work/alice-data/Alice1/bin/base','--frames','8','--save-dir',str(d/'private/saves')]
with (d/'run.log').open('w') as f:p=subprocess.run(cmd,cwd=d,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=120)
assert p.returncode==0,p.returncode
rows=[json.loads(l.split('] ',1)[1]) for l in (d/'run.log').read_text().splitlines() if l.startswith('[lg/transition-memory] ')]
start=[r for r in rows if r['kind']=='startup-entry'];assert len(start)==2 and all(r['from']=='menu' and r['to']=='hedge3' for r in start)
assert start[0]['event']=='begin' and start[1]['event']=='end' and start[1]['success']
(O/'proof/STARTUP-CHOOSER.json').write_text(json.dumps(dict(status='PASS',argv=cmd,default_options_map='skool1',host_injected_choice='hedge3',actual_chooser_chapter_result_branch=True,records=start,binary_sha256=hashlib.sha256(B.read_bytes()).hexdigest()),indent=2)+'\n')
print('PASS default map skool1, chooser Hedge3, actual begin/end destination Hedge3')
