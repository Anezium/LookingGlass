"""Count instrumentation must leave selected full packet payloads/state/pixels intact."""
import gzip,json
import host_oracle as h
rows=[]
def read(folder,selected):
 frames={};images={}
 with gzip.open(folder/'stdout.log.gz','rt') as f:
  for line in f:
   if line.startswith(('SIM_MAPS_FRAME ','SKIN_EXACT_IMAGE ')):
    kind,text=line.split(' ',1);r=json.loads(text)
    if r['frame'] in selected:
     if kind=='SIM_MAPS_FRAME':frames[r['frame']]=h.normalize(r)
     else:images[r['frame']]=r
    if r['frame']>max(selected)+1:break
 return frames,images
for tape in ['rw-20261116-wchess1-01.lgtape','fp-centipede1-01.lgtape']:
 folder=h.P/'census-final'/tape;receipt=json.loads((folder/'BOUNDED-RECEIPT.json').read_text());selected=set(receipt['selected'])
 a,ai=read(h.HOST/(tape+'-base'),selected);b,bi=read(folder,selected)
 assert set(a)==set(b)==set(ai)==set(bi)==selected
 assert ai==bi,'instrumentation changed images'
 fields=['state_sha256','submitted_packets','world_packets','joint_alpha_order','blend_world_order','selected_lights_bits','world_viewport','renderer_drops','mover_replay']
 for frame in selected:
  for key in fields:assert a[frame].get(key)==b[frame].get(key),(tape,frame,key)
 rows.append(dict(tape=tape,frames=sorted(selected),state_packets_pixels=True,classification=receipt['classification'],head=receipt['head'],binary_sha256=receipt['binary_sha256']))
(h.P/'census-proof.json').write_text(json.dumps(rows,indent=2)+'\n');print(rows)
