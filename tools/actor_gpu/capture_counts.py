"""Bounded final-runtime draw census; counts only, never whole-tape/native timing proof."""
import json,os,shutil,subprocess,gzip,sys
import host_oracle as h
for tape,selected,origin in [('rw-20261116-wchess1-01.lgtape',[849,850,961,962,1046,1047,1048],h.P.parent/'release-week/play-record/tapes'),('fp-centipede1-01.lgtape',[500,600,700,800],h.P.parent/'release-week/fight-measure-01/tapes')]:
 if len(sys.argv)>1 and tape not in sys.argv[1:]:continue
 out=h.P/'census-final'/tape;private=out/'private';private.mkdir(parents=True,exist_ok=False)
 c=json.loads((h.SOURCE/'configs/PL-release.json').read_text());c.update(replay_play=tape,log_frame_time_every=0)
 (private/'vita_renderer.json').write_text(json.dumps(c));shutil.copy2(h.SOURCE/'configs/vita-sim/vita-sim-release.json',private/'vita_sim.json');os.symlink(origin/tape,private/tape)
 env={k:os.environ[k] for k in ['HOME','USER','TMPDIR'] if k in os.environ}
 frames=','.join(map(str,selected))
 env.update(PATH='/usr/bin:/bin',LANG='C',LOOKING_GLASS_REPLAY_CROSS='1',LOOKING_GLASS_VITA_CONTROLS='1',LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_PROBE_KEEP_WINDOW='1',LOOKING_GLASS_SKIN_PROOF='1',LOOKING_GLASS_WORLD_PACKETS='1',LOOKING_GLASS_MOVER_REPLAY='1',LOOKING_GLASS_SHOT_FRAMES=frames,LOOKING_GLASS_ACTOR_CENSUS='1',LOOKING_GLASS_ACTOR_CENSUS_FRAMES=frames,LOOKING_GLASS_DRAW_QUERY='1',LOOKING_GLASS_DRAW_QUERY_FRAMES=frames,LOOKING_GLASS_DRAW_QUERY_RASTER='1')
 if tape.startswith("fp-"):env.pop("LOOKING_GLASS_DRAW_QUERY_RASTER",None)
 count=0
 with (out/'stderr.log').open('w') as err,gzip.open(out/'stdout.log.gz','wt') as log:
  proc=subprocess.Popen([str(h.BINS/'lg-host-final'),'--data',str(h.DATA),'--save-dir',str(private/'saves')],cwd=out,env=env,stdout=subprocess.PIPE,stderr=err,text=True)
  try:
   for line in proc.stdout:
    log.write(line)
    if line.startswith('SIM_MAPS_FRAME '):
     row=json.loads(line.split(' ',1)[1]);count+=1
     if row['frame']>max(selected)+1:break
   else:raise RuntimeError('census did not reach target')
  finally:
   proc.terminate()
   try:proc.wait(timeout=30)
   except subprocess.TimeoutExpired:proc.kill();proc.wait()
 receipt=dict(classification='BOUNDED DRAW CENSUS ONLY; intentional stop; no replay/native timing PASS',tape=tape,tape_sha256=h.sha(origin/tape),binary_sha256=h.sha(h.BINS/'lg-host-final'),selected=selected,frames=count,returncode=proc.returncode,process_exited=True,head=json.loads((h.P/'logs/build-host-final-binding.json').read_text())['head'])
 (out/'BOUNDED-RECEIPT.json').write_text(json.dumps(receipt,indent=2)+'\n');print(tape,count,'frames',flush=True)
