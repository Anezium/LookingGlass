#!/usr/bin/env python3
"""Real viewer record/replay proof; outputs remain outside Git, game data read-only."""
import argparse,hashlib,json,math,os,re,shutil,struct,subprocess,time
from pathlib import Path
SOURCE=Path(__file__).resolve().parents[2]
DATA=Path('/Users/anezium/vita-work/alice-data/Alice1/bin/base')
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def main():
 p=argparse.ArgumentParser();p.add_argument('--binary',required=True,type=Path);p.add_argument('--out',required=True,type=Path);p.add_argument('--map',default='hatter2',choices=['hatter2','centipede1','skool1','grounds1','grounds2','jlair2','potears3','qlair','hatter1','fortress1','utemple','wchess2']);p.add_argument('--analyze-existing',action='store_true');a=p.parse_args()
 root=a.out.resolve();root.mkdir(parents=True,exist_ok=a.analyze_existing);binary=a.binary.resolve()
 assert shutil.disk_usage(root).free>=41*2**30
 cfg=json.loads((SOURCE/'configs/PL-rc13-release-play04.json').read_text())
 cfg.update(replay_record='',replay_play='',replay_start='',replay_stop_after=0,replay_check_every=30,replay_state_every=300)
 tape=f'fp-{a.map}-host-01.lgtape'
 env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
 env['LOOKING_GLASS_VITA_CONTROLS']='1'
 def run(mode):
  cell=root/mode;private=cell/'private';private.mkdir(parents=True)
  config=dict(cfg,replay_fight_pilot=mode=='record')
  if mode=='record':config.update(replay_record=tape,replay_start=f'chapter:{a.map}@normal')
  else:config.update(replay_play=tape);shutil.copy2(root/'record/private'/tape,private/tape)
  (private/'vita_renderer.json').write_text(json.dumps(config,indent=2)+'\n')
  logfile=cell/'run.log';command=[str(binary),'--data',str(DATA),'--save-dir',str(private/'saves'),'--no-audio']
  with logfile.open('wb') as log:
   proc=subprocess.Popen(command,cwd=cell,env=env,stdout=log,stderr=subprocess.STDOUT)
   started=time.monotonic()
   try:
    while proc.poll() is None:
     assert time.monotonic()-started<300,'host viewer timeout'
     text=logfile.read_text(errors='replace')
     if mode=='record' and f'[lg/replay] recorded {tape} ' in text:
      assert 'reason=menu' in text,'not a normal menu stop'
      proc.terminate();proc.wait(timeout=15);break
     time.sleep(1)
    if mode=='play':assert proc.returncode==0,'replay process failed'
   finally:
    if proc.poll() is None:proc.terminate();proc.wait(timeout=15)
  return logfile.read_text(errors='replace')
 recording=(root/'record/run.log').read_text() if a.analyze_existing else run('record')
 assert re.search(r'\[lg/replay\] recorded .* reason=menu ',recording),'missing clean recording log'
 assert '[lg/pilot] fight_start' in recording,'did not reach a visible fight'
 playback=(root/'play/run.log').read_text() if a.analyze_existing else run('play')
 assert re.search(r'\[lg/replay\] done .* diff=0 .*event_mismatches=0 .*result=PASS',playback),'replay not exact'
 assert json.loads((root/'play/private/vita_renderer.json').read_text())['replay_fight_pilot'] is False
 assert '[lg/pilot] enabled' not in playback,'pilot active during replay'
 path=root/'record/private'/tape
 header=None;end=None;pad=None;keys=0;moving=attacks=frames=0;seconds=0.;positions=[]
 for line in path.read_text().splitlines():
  if line.startswith('H '):header=json.loads(line[2:])
  if line.startswith('END '):end=json.loads(line[4:])
  if line.startswith('C '):positions.append(tuple(map(float,line.split()[5].split(','))))
  if not line.startswith('F '):continue
  fields=line.split();frames+=1;seconds+=struct.unpack('!f',bytes.fromhex(fields[1]))[0]
  for f in fields[2:]:
   if f.startswith('pad='):pad=None if f[4:]=='-' else bytes.fromhex(f[4:])
   if f.startswith('kd='):keys=int(f[3:],16)
  moving+=int(pad is not None and pad[:2]!=bytes([128,128]))
  # Header stores pinned miniquad/X11 codes; LeftControl is 0xffe3.
  attacks+=int(bool(keys & (1<<header['keys'].index(0xffe3))))
 assert end and end['reason']=='menu' and end['steps']==frames,'unclean/truncated tape'
 health=[float(v) for v in re.findall(r'boss_health=([0-9.]+)',recording)]
 dealt=sum(max(0,x-y) for x,y in zip(health,health[1:])) if a.map in ('hatter2','grounds1','jlair2') else sum(map(float,re.findall(r'health_loss=([0-9.]+)',recording)))
 damage_rows=[tuple(map(float,v)) for v in re.findall(r'damage_taken=([0-9.]+) seconds=([0-9.]+)',recording)]
 contacts=re.findall(r'weapon_contact id=(\d+) damage=([0-9.]+)',recording)
 displacement=max(math.dist(positions[0],v) for v in positions)
 sampled_path=sum(math.dist(x,y) for x,y in zip(positions,positions[1:]))
 first_fight=float(re.search(r'fight_start seconds=([0-9.]+)',recording)[1])
 taken=sum(damage for damage,when in damage_rows if when>=first_fight)
 stopped=re.search(r'\[lg/pilot\] stop map='+re.escape(a.map)+r' seconds=([0-9.]+) fight=([0-9.]+)',recording)
 assert stopped,'missing bounded pilot stop'
 # Map-specific walking sanity: arena maps start the fight near the chapter spawn.
 walk_min={'skool1':300,'grounds1':300,'jlair2':200,'potears3':400,'qlair':400,'grounds2':600,'fortress1':500}.get(a.map,1000)
 assert dealt>0 and taken>0 and contacts and moving>0 and attacks>0 and displacement>=walk_min,'combat proof incomplete'
 done=next(l for l in playback.splitlines() if l.startswith('[lg/replay] done '))
 result=dict(map=a.map,difficulty='normal',binary_sha256=sha(binary),tape_sha256=sha(path),frames=frames,tape_seconds=seconds,first_fight_seconds=first_fight,fight_window_seconds=float(stopped[1])-first_fight,visible_target_fight_seconds=float(stopped[2]),max_displacement_units=displacement,walk_min_units=walk_min,sampled_path_units=sampled_path,movement_frames=moving,attack_held_frames=attacks,weapon_contacts=len(contacts),sampled_enemy_damage=dealt,damage_taken=taken,pilot_off_replay=done,console_contact=False)
 (root/'proof.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2),flush=True)
if __name__=='__main__':main()
