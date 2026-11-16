import hashlib,json
from pathlib import Path
P=Path(__file__).resolve().parent
BASE=P.parent/'rc14/qual/configs/rc14-release-a2-04.json'
LEVERS=['world_alpha_parameter_once','actor_skin_output_reuse']
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
    base=json.loads(BASE.read_text()); (P/'console-configs').mkdir(exist_ok=True)
    plan=[]
    for label,tape,window in [('a4','a4-wchess1-01.lgtape',[700,1180]),('play-worst','rw-20261116-wchess1-01.lgtape',[1486,1966]),('play-second','rw-20261116-wchess1-01.lgtape',[796,1276])]:
        source=P.parent/('release-week/play-record/tapes' if tape.startswith('rw-') else 'a5-heavy/tapes')/tape
        lines=source.read_text().splitlines(); steps=sum(x.startswith('F ') for x in lines);checks=sum(x.startswith('C ') for x in lines)
        for arm,lever in [('control','control'),('alpha-once',LEVERS[0]),('skin-reuse',LEVERS[1]),('combined','combined')]:
            name=label+'-'+arm;c=dict(base)
            c.update(replay_play=tape,replay_record='',replay_start='',spike_log_ms=.01,spike_log_first_frame=window[0],spike_log_last_frame=window[1],log_frame_time_every=0)
            for key in LEVERS:c[key]=lever==key or lever=='combined'
            rel='console-configs/'+name+'.json';(P/rel).write_text(json.dumps(c,indent=2,sort_keys=True)+'\n')
            plan.append(dict(order=len(plan),name=name,lever=lever,levers=LEVERS,config=rel,config_sha256=sha(P/rel),tape=tape,tape_sha256=sha(source),steps=str(steps),checks=checks,window=window,frames=481))
    (P/'arm-plan.json').write_text(json.dumps(plan,indent=2)+'\n')
    (P/'TEST-MANIFEST.json').write_text(json.dumps({'arms':plan},indent=2)+'\n')
if __name__=='__main__':main()
