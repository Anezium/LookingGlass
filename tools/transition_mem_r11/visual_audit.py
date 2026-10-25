"""Packet equality and exact pixel-difference accounting for completed host runs."""
from pathlib import Path
import argparse,json,hashlib
from PIL import Image
p=argparse.ArgumentParser();p.add_argument('output',type=Path);a=p.parse_args();O=a.output.resolve();V=O/'visual'
proof=json.loads((V/'proof/TRANSITION-EQUALITY.json').read_text());results=[]
def frames(d,name='run.log'):
 return [json.loads(l.split(' ',1)[1]) for l in (d/name).read_text().splitlines() if l.startswith('SIM_MAPS_FRAME ')]
def pixels(a,b):
 left=Image.open(a).convert('RGBA');right=Image.open(b).convert('RGBA');assert left.size==right.size
 raw=list(zip(left.tobytes(),right.tobytes()));count=sum(x!=y for x,y in raw);largest=max((abs(x-y) for x,y in raw),default=0)
 return dict(exact=count==0,different_channels=count,max_channel_delta=largest,pixels=left.width*left.height,files=[str(a),str(b)])
for pair in proof['pairs']:
 tag=pair['scenario'];on=int(pair['levers']);left=V/'host'/f'{tag}-{on}-before';right=V/'host'/f'{tag}-{on}-after';first,second=frames(left),frames(right)
 assert len(first)==len(second)
 draws=0
 for n,(x,y) in enumerate(zip(first,second)):
  for k in ['state_sha256','selected_lights_bits','joint_alpha_order','blend_world_order','actor_skin']:
   assert x[k]==y[k],(tag,on,n,k)
  def packets(f):return [{k:v for k,v in p.items() if k!='gpu_slot'} for p in f['submitted_packets']]
  assert packets(x)==packets(y),(tag,on,n,'ordered GPU packets')
  assert all(p['geometry']==p['gpu_geometry'] for p in x['submitted_packets']+y['submitted_packets'])
  draws+=len(x['submitted_packets'])
 row=dict(scenario=tag,levers=bool(on),frames=len(first),ordered_packets_exact=True,uploaded_geometry_exact=True,draws=draws)
 if tag!='quit':
  row['raster']=pixels(left/'final.png',right/'final.png')
  assert row['raster']['exact'],(tag,on,row['raster'])
 results.append(row)
control=V/'host/retry-1-before'
repeat=pixels(control/'final.png',control/'control.png')
for x,y in zip(frames(control),frames(control,'control.log')):
 assert [{k:v for k,v in p.items() if k!='gpu_slot'} for p in x['submitted_packets']]==[{k:v for k,v in p.items() if k!='gpu_slot'} for p in y['submitted_packets']]
(O/'proof/TRANSITION-VISUAL.json').write_text(json.dumps(dict(status='PASS_ORDERED_GPU_PACKETS',binary_sha256=proof['binary_sha256'],pixel_equality_claim='All destination-transition final decoded PNG pixels must be exact; any difference fails',hardware_evidence=False,pairs=results,same_implementation_repeat_control=repeat),indent=2)+'\n')
print('PASS ordered packets, uploaded geometry and state; raster differences explicitly counted')
