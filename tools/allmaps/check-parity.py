#!/usr/bin/env python3
"""Compare all fifteen village reference images against the archived native pkg-2 binary."""
import argparse, hashlib, json, os, shutil, subprocess
from pathlib import Path
from PIL import Image
p=argparse.ArgumentParser()
for key in ['binary','baseline','evidence','data','config','sim','save']:p.add_argument('--'+key,type=Path,required=True)
a=p.parse_args();E=a.evidence.resolve();E.mkdir(parents=True,exist_ok=True)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
result={'baseline_binary_sha256':sha(a.baseline),'binary_sha256':sha(a.binary),'console_access':False,'cells':{}}
for arm,binary in [('pkg2',a.baseline),('allmaps',a.binary)]:
 cell=E/arm;private=cell/'private';(private/'saves').mkdir(parents=True,exist_ok=True)
 for src,dst in [(a.config,private/'vita_renderer.json'),(a.sim,private/'vita_sim.json'),(a.save,private/'saves/auto.json')]:shutil.copy2(src,dst)
 env={k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')};env.update(LOOKING_GLASS_HOST_CLOCK_STEP='0',LOOKING_GLASS_RENDER_METRICS='1')
 assert not (Path.home()/'vita-j0/mac-timing.lock').exists(),'timing lock active'
 with (cell/'run.log').open('w') as log:subprocess.run([str(binary.resolve()),'--data',str(a.data.resolve()),'--renderer-host-check','--no-audio'],cwd=cell,env=env,stdout=log,stderr=subprocess.STDOUT,check=True,timeout=600)
 assert 'invalid config' not in (cell/'run.log').read_text()
 assert sha(private/'saves/auto.json')==sha(a.save)
 images={p.name:sha(p) for p in sorted((private/'renderer-host').glob('*.png'))}
 assert len(images)==15,images
 result['cells'][arm]=images
assert result['cells']['pkg2']==result['cells']['allmaps'],'PNG byte parity failed'
for name in result['cells']['pkg2']:
 left=Image.open(E/'pkg2/private/renderer-host'/name).convert('RGBA')
 right=Image.open(E/'allmaps/private/renderer-host'/name).convert('RGBA')
 assert left.size==right.size and left.tobytes()==right.tobytes()
result.update(image_count=15,png_byte_identical=True,decoded_rgba_identical=True,save_unchanged=True)
(E/'PARITY.json').write_text(json.dumps(result,indent=2)+'\n');print('PASS 15/15 pkg-2 village PNG-byte and RGBA parity')
