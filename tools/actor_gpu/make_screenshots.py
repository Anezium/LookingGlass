"""Read-only screenshot comparisons; no image resampling or fabrication."""
from pathlib import Path
import json
from PIL import Image,ImageChops
import host_oracle as h
pairs=[]
def pair(label,before,after,scope):
 a=Image.open(before).convert('RGB');b=Image.open(after).convert('RGB');assert a.size==b.size
 d=ImageChops.difference(a,b)
 pixels=sum(px!=(0,0,0) for px in d.getdata());maximum=max(max(px) for px in d.getdata())
 return dict(label=label,scope=scope,before=str(before),after=str(after),before_sha256=h.sha(before),after_sha256=h.sha(after),size=list(a.size),changed_pixels=pixels,max_channel_delta=maximum,diff_bbox=d.getbbox(),classification='PICTURE-CHANGING; OWNER APPROVAL REQUIRED')
for tape,frames in [('rw-20261116-wchess1-01.lgtape',[849,961,1046]),('fp-centipede1-01.lgtape',[500,600,700,800])]:
 for arm in ['frontface','combined']:
  for frame in frames:
   before=h.P/'census-final'/tape/f'shots/frame-{frame:04}.png';after=h.HOST/(tape+'-'+arm)/f'shots/frame-{frame:04}.png'
   pairs.append(pair(tape+':'+arm+':'+str(frame),before,after,'Recorded-input host replay view; host/base divergence from native applies'))
views=json.loads((h.P/'supplemental-views.json').read_text())
assert len(views)==6
for r in views:
 if r['arm']=='control':continue
 before=next(v for v in views if v['view']==r['view'] and v['arm']=='control')
 pairs.append(pair(r['view']+':'+r['arm'],Path(before['screenshot']),Path(r['screenshot']),r['classification']))
(h.P/'screenshots.json').write_text(json.dumps(pairs,indent=2)+'\n')
text='''# Picture-changing candidate screenshots

Host images only. No owner release approval or native fidelity claim. Supplemental close views override only the render camera and stop after the requested frame; they are not full replays.

| View / arm | Before | After | Changed pixels |
|---|---|---|---:|
'''
for row in pairs:
 text+=f"| {row['label']} | [before]({row['before']}) | [after]({row['after']}) | {row['changed_pixels']} |\n"
(h.P/'SCREENSHOTS.md').write_text(text)
print(len(pairs),'before/after pairs')
for p in pairs:print(p['label'],p['changed_pixels'],'pixels')
