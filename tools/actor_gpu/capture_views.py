"""Bounded supplemental render-camera views, never credited as whole-tape or native proof."""
import hashlib,json,os,shutil,subprocess,gzip
from pathlib import Path
import host_oracle as h
rows=[]
for name,tape,frame,view,origin in [('bishop','rw-20261116-wchess1-01.lgtape',849,'0 -1856 250 -68 -1856 320 -72',h.P.parent/'release-week/play-record/tapes'),('army-ant','fp-centipede1-01.lgtape',500,'0 -600 375 -550 -470 375 -575',h.P.parent/'release-week/fight-measure-01/tapes')]:
 states={}
 for arm,overrides in [('control',{}),('frontface',{'actor_frontface_cull':True}),('combined',{'actor_frontface_cull':True,'actor_discard_free_shader':True})]:
  out=h.P/'supplemental-views'/(name+'-'+arm);private=out/'private';private.mkdir(parents=True,exist_ok=False)
  c=json.loads((h.SOURCE/'configs/PL-release.json').read_text());c.update(replay_play=tape,log_frame_time_every=0);c.update(overrides)
  (private/'vita_renderer.json').write_text(json.dumps(c));shutil.copy2(h.SOURCE/'configs/vita-sim/vita-sim-release.json',private/'vita_sim.json');os.symlink(origin/tape,private/tape)
  env={k:os.environ[k] for k in ['HOME','USER','TMPDIR'] if k in os.environ};env.update(PATH='/usr/bin:/bin',LANG='C',LOOKING_GLASS_REPLAY_CROSS='1',LOOKING_GLASS_VITA_CONTROLS='1',LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_FIXED_VIEWS=view,LOOKING_GLASS_SHOT_FRAMES=str(frame))
  captured=[]
  with (out/'stderr.log').open('w') as err,gzip.open(out/'stdout.log.gz','wt') as log:
   proc=subprocess.Popen([str(h.BINS/'lg-host-final'),'--data',str(h.DATA),'--save-dir',str(private/'saves')],cwd=out,env=env,stdout=subprocess.PIPE,stderr=err,text=True)
   try:
    for line in proc.stdout:
     log.write(line)
     if line.startswith('SIM_MAPS_FRAME '):
      r=json.loads(line.split(' ',1)[1]);captured.append((r['frame'],r['state_sha256']))
      if r['frame']>frame+1 and (out/f'shots/frame-{frame:04}.png').exists():break
    else:raise RuntimeError('capture did not reach its bounded target')
   finally:
    proc.terminate()
    try:proc.wait(timeout=30)
    except subprocess.TimeoutExpired:proc.kill();proc.wait()
  states[arm]=captured
  if arm!='control':assert states['control']==captured
  shot=out/f'shots/frame-{frame:04}.png'
  row=dict(view=name,arm=arm,frame=frame,view_override=view,screenshot=str(shot),sha256=h.sha(shot),binary_sha256=h.sha(h.BINS/'lg-host-final'),config=c,classification='BOUNDED SUPPLEMENTAL HOST VIEW; render camera overridden; no whole-tape or native PASS',state_exact_to_control=True,frames=len(captured),owned_pid=proc.pid,process_exited=True,returncode=proc.returncode)
  rows.append(row);(h.P/'supplemental-views.json').write_text(json.dumps(rows,indent=2)+'\n');print(name,arm,'captured',frame,flush=True)
