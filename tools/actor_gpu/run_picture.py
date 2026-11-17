"""Labelled picture-changing comparisons; never pass as an exact candidate."""
import json
import host_oracle as h
rows=[]
for tape,frames in [('rw-20261116-wchess1-01.lgtape','849,961,1046'),('fp-centipede1-01.lgtape','500,600,700,800')]:
 reference=h.run('base',tape,h.BINS/'lg-host-base',{})
 for label,overrides in [('frontface',{'actor_frontface_cull':True}),('combined',{'actor_frontface_cull':True,'actor_discard_free_shader':True})]:
  arm=h.run(label,tape,h.BINS/'lg-host-final',overrides,env_extra={'LOOKING_GLASS_SHOT_FRAMES':frames})
  assert reference['events']==arm['events'] and reference['replay']==arm['replay']
  # Shader policy/cull experiment intentionally changes pixels; all complete simulation state hashes must agree.
  import gzip
  def states(name):
   with gzip.open(h.HOST/(tape+'-'+name)/'stdout.log.gz','rt') as f:
    return [(r['frame'],r['state_sha256']) for line in f if line.startswith('SIM_MAPS_FRAME ') for r in [json.loads(line.split(' ',1)[1])]]
  assert states('base')==states(label)
  changed=sum(a!=b for a,b in zip(reference['images'],arm['images']))
  row=dict(tape=tape,candidate=label,classification='PICTURE-CHANGING; OWNER APPROVAL REQUIRED',state_events_replay_exact=True,changed_image_frames=changed,frames=arm['frames'],binary_sha256=arm['binary_sha256'])
  rows.append(row);(h.P/'picture-proof.json').write_text(json.dumps(rows,indent=2)+'\n');print(row,flush=True)
