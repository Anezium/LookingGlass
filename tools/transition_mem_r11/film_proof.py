"""Full opening film and New Game, with muted real Mac callback clocks.
Four independent private processes; no console, driver, or shared save writes.
"""
from pathlib import Path
import sys,concurrent.futures,json,hashlib
source=Path(__file__).with_name('host_proof.py')
s=source.read_text();prefix=s[:s.index('# Seed the second native save')]
namespace={'__file__':str(source),'__name__':'r11_film_host'}
exec(compile(prefix,str(source),'exec'),namespace)
O=namespace['O'];run=namespace['run'];base=namespace['base'];env=namespace['env'];base['story_film_bypass']=False;env.pop('LOOKING_GLASS_SAH_SILENT_AUDIO',None)
for on in [False,True]:
 for legacy in [True,False]:
  d=O/'host'/f"film-{int(on)}-{'before' if legacy else 'after'}";(d/'private').mkdir(parents=True,exist_ok=True)
  (d/'private/audio-settings.txt').write_text('music=0\neffects=0\nmuted=true\n')
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
 jobs=[pool.submit(run,f"film-{int(on)}-{'before' if legacy else 'after'}",'hedge3','gvillage','new-game',on,legacy) for on in [False,True] for legacy in [True,False]]
 rows=[f.result()[0] for f in jobs]
# Publish once after all four workers have finished their per-run receipts.
# This final ordered write cannot race the shared scenario driver's snapshots.
(O/'proof/TRANSITION-RUNS.json').write_text(json.dumps(rows,indent=2)+'\n')
result=[]
for on in [False,True]:
 old=next(r for r in rows if r['levers']==on and r['legacy']);new=next(r for r in rows if r['levers']==on and not r['legacy'])
 assert old['states']==new['states'],('full film state',on)
 for r in [old,new]:
  log=(O/'host'/r['label']/'run.log').read_text();assert 'owner=roq end=complete' in log and 'config-bypass' not in log,(r['label'],'film did not complete')
  left=json.loads((O/'host'/r['label']/'private/saves/auto.json').read_text())['payload']
  assert left['current']=='hedge3$first'
 result.append(dict(levers=on,old=old,new=new,states_equal=True))
(O/'proof/FULL-FILM-EQUALITY.json').write_text(json.dumps(dict(status='PASS',muted_real_mac_audio_callback=True,binary_sha256=hashlib.sha256(namespace['B'].read_bytes()).hexdigest(),pairs=result),indent=2)+'\n')
print('PASS full opening films, New Game state and old Auto preservation')
