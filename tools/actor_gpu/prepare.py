"""Generate only local arm configs. No transport. No default flips."""
import hashlib,json
from pathlib import Path
P=Path(__file__).resolve().parent
BASE=P/'base/PL-rc15-release-play.json'
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
ARMS=[('control',{},'exact control'),('discard-free',{'actor_discard_free_shader':True},'exact candidate; native pixels pending'),('frontface',{'actor_frontface_cull':True},'PICTURE-CHANGING: owner approval required'),('combined',{'actor_discard_free_shader':True,'actor_frontface_cull':True},'PICTURE-CHANGING: owner approval required')]
def main():
 (P/'console-configs').mkdir(exist_ok=True)
 assert sha(BASE)=='580f73ba3e3bd166fe2362e2e2a9ab229ad2080d0de8f1036b24d80975585c70'
 base=json.loads(BASE.read_text());plan=[]
 for map,tape,window,origin in [('wchess1','rw-20261116-wchess1-01.lgtape',[796,1276],P.parent/'release-week/play-record/tapes'),('centipede1','fp-centipede1-01.lgtape',[376,856],P.parent/'release-week/fight-measure-01/tapes')]:
  data=(origin/tape).read_bytes();steps=sum(l.startswith(b'F ') for l in data.splitlines());checks=sum(l.startswith(b'C ') for l in data.splitlines())
  for label,switches,why in ARMS:
   name=map+'-'+label
   overrides=dict(replay_play=tape,replay_record='',replay_start='',spike_log_ms=0.01,spike_log_first_frame=window[0],spike_log_last_frame=window[1],log_frame_time_every=0,actor_discard_free_shader=False,actor_frontface_cull=False)
   overrides.update(switches);config=dict(base);config.update(overrides)
   rel='console-configs/'+name+'.json';(P/rel).write_text(json.dumps(config,indent=2,sort_keys=True)+'\n')
   plan.append(dict(order=len(plan),name=name,map=map,tape=tape,tape_sha256=sha(origin/tape),steps=str(steps),checks=checks,window=window,frames=481,lever=label,world_gxm=config['world_gxm'],mask=[],overrides=overrides,why=why,config=rel,config_sha256=sha(P/rel)))
 for name in ['arm-plan.json','TEST-MANIFEST.json']:(P/name).write_text(json.dumps(plan,indent=2)+'\n')
 print('wrote',len(plan),'arms')
if __name__=='__main__':main()
