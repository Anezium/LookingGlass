#!/usr/bin/env python3
"""Reproduce step-0 functional checks on Mac; never contact the console."""
from pathlib import Path
import argparse, hashlib, json, os, shutil, subprocess, time
P=argparse.ArgumentParser();P.add_argument('--evidence',type=Path,required=True);P.add_argument('--phase',choices=['suites','parity'],required=True);a=P.parse_args()
R=Path(__file__).resolve().parents[1];E=a.evidence.resolve();E.mkdir(parents=True,exist_ok=True)
D=Path('/Users/anezium/vita-work/alice-data/Alice1/bin/base');REF=Path.home()/'vita-j0/atoz/reference-build-r3b'
LOCK=Path.home()/'vita-j0/mac-timing.lock';B=E/'target-host/debug/looking-glass'
ENV={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
ENV.update(CARGO_BUILD_JOBS='4',CARGO_TARGET_DIR=str(E/'target-host'),PATH=str(Path.home()/'.cargo/bin')+os.pathsep+ENV['PATH'])
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def run(label,command,cwd=R,extra=None,timeout=1800):
 while LOCK.exists():
  print('Waiting for mac-timing.lock',LOCK.read_text().strip(),flush=True);time.sleep(30)
 log=E/(label+'.log');log.parent.mkdir(parents=True,exist_ok=True)
 with log.open('w') as out:r=subprocess.run([str(v) for v in command],cwd=cwd,env=dict(ENV,**(extra or {})),stdout=out,stderr=subprocess.STDOUT,timeout=timeout)
 assert r.returncode==0,(label,r.returncode,log.read_text()[-4000:])
 print('PASS',label,flush=True);return log
if a.phase=='suites':
 rows=[]
 for label,cmd,extra in [
  ('tests-all',['cargo','+stable','test','--locked','--all-features'],{}),
  ('tests-default',['cargo','+stable','test','--locked'],{}),
  ('tests-fan',['cargo','+stable','test','--locked','--all-features','tower2_fan_models_match','--','--ignored','--nocapture'],{'LOOKING_GLASS_TEST_DATA':str(D)}),
  ('source-audit',['python3','tools/check_source.py','--worktree'],{}),
  ('confinement',['python3','tools/check_gl_confinement.py'],{}),
  ('host-build',['cargo','+stable','build','--locked','--features','vita_renderer,sim_maps_probe'],{})]:
  log=run(label,cmd,extra=extra);rows.append({'label':label,'log_sha256':sha(log),'exit_code':0})
  (E/'SUITES.json').write_text(json.dumps(rows,indent=2)+'\n')
else:
 cfg=json.loads((R/'configs/PL-def-r3.json').read_text());results={'binary_sha256':sha(B),'simulation':[],'images':[],'audio':None}
 metrics={'LOOKING_GLASS_RENDER_METRICS':'1','LOOKING_GLASS_HOST_CLOCK_STEP':'0'}
 def cell(name,arm,config=cfg):
  c=E/'host'/name/arm;(c/'private').mkdir(parents=True,exist_ok=True)
  (c/'private/vita_renderer.json').write_text(json.dumps(dict(config,sim_span_probe=arm=='on'),indent=2)+'\n')
  shutil.copy2(REF/'configs/vita-sim-all.json',c/'private/vita_sim.json');return c
 def save(): (E/'PARITY.json').write_text(json.dumps(results,indent=2)+'\n')
 maps=['skool2','hedge3','tower2','funhouse','hatter1','potears3','fortress1','gvillage','skool1','pandemonium']
 for m in maps:
  pair=[];n=1200 if m=='funhouse' else 300
  for arm in ['off','on']:
   c=cell('sim-'+m,arm)
   log=run('host/sim-'+m+'/'+arm+'/run',[B,'--data',D,'--map',m,'--no-audio','--frames',n,'--save-dir',c/'private/saves'],cwd=c,extra=dict(metrics,LOOKING_GLASS_SIM_MAPS_PROBE='1',LOOKING_GLASS_SIM_MAPS_DT='0.05',LOOKING_GLASS_SIM_MAPS_COUNTERS='1'))
   text=log.read_text();assert 'invalid config' not in text
   rows=[json.loads(line.split(' ',1)[1]) for line in text.splitlines() if line.startswith('SIM_MAPS_FRAME ')]
   assert len(rows)==n,(m,arm,len(rows));assert all(r['step0']['enabled']==(arm=='on') for r in rows)
   if arm=='on':assert any(r['step0']['sim_blocks_ns']>0 and sum(r['step0']['exclusive_ns'])>0 for r in rows)
   else:assert all(r['step0']['sim_blocks_ns']==0 and sum(r['step0']['exclusive_ns'])==0 for r in rows)
   pair.append(rows)
  assert [r['state_sha256'] for r in pair[0]]==[r['state_sha256'] for r in pair[1]],m
  assert [r['counters'] for r in pair[0]]==[r['counters'] for r in pair[1]],(m,'workload counters')
  results['simulation'].append({'map':m,'frames_per_arm':n,'state_bits_equal':True,'counters_equal':True});save()
 for mode in ['renderer-host-check','skin-overlap-render-check']:
  pairs=[]
  for arm in ['off','on']:
   c=cell(mode,arm,json.loads((REF/'configs/PL-parity-r2.json').read_text()))
   save_dir=c/'private/saves';save_dir.mkdir(exist_ok=True)
   shutil.copy2(Path('/Users/anezium/vita-j0/village-retained-world/jalon-23-bls12/mac-timing/inputs/auto.json'),save_dir/'auto.json')
   run('host/'+mode+'/'+arm+'/run',[B,'--data',D,'--'+mode,'--no-audio'],cwd=c,extra=metrics)
   images={str(p.relative_to(c)):sha(p) for p in c.rglob('*.png')};assert images,(mode,arm);pairs.append(images)
  assert pairs[0]==pairs[1],mode
  results['images'].append({'mode':mode,'pngs_per_arm':len(pairs[0]),'byte_equal':True,'hashes':pairs[0]});save()
 c=cell('audio','on')
 log=run('host/audio/on/run',[B,'--data',D,'--audio-test','--map','skool1'],cwd=c,extra=metrics)
 text=log.read_text();assert 'Audio device opened:' in text and 'measured playback intervals passed' in text
 results['audio']={'real_device_opened':True,'interval_check':True,'log_sha256':sha(log),'vita_runtime_pending':True};save()
 print('PASS same-binary probe off/on parity and real Mac audio',flush=True)
