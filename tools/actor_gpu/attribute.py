"""Per invocation and per draw counts, never infer device time from host samples."""
import collections,gzip,json,re
from pathlib import Path
P=Path(__file__).resolve().parent
reports={}
for tape in ['rw-20261116-wchess1-01.lgtape','fp-centipede1-01.lgtape']:
 f=P/'census-final'/tape/'stdout.log.gz'
 receipt=f.parent/'BOUNDED-RECEIPT.json'
 if not receipt.exists():continue
 draws=collections.defaultdict(list);samples=collections.defaultdict(list);pieces=collections.defaultdict(list);poses=collections.defaultdict(list);summaries=[];raw=[]
 with gzip.open(f,'rt') as stream:
  for line in stream:
   for prefix,target in [('ACTOR_GPU_DRAW ',draws),('SAMPLES_DRAW ',samples),('CHESS_GPU_PIECE ',pieces)]:
    if line.startswith(prefix):
     row=json.loads(line.split(' ',1)[1]);target[row['frame']].append(row);break
   else:
    if line.startswith('[lg/pose-census]'):
     fields=dict(re.findall(r'(pose|frame|identity|path|model)=([^\s]+)',line));poses[int(fields['frame'])].append(fields)
    elif line.startswith('SAMPLES_QUERY_SUMMARY '):summaries.append(json.loads(line.split(' ',1)[1]))
    elif line.startswith('[lg/group-census]') and 'stage=raw ' in line:raw.append(line.strip())
 frames={}
 for frame,ds in draws.items():
  ss=[r for r in samples[frame] if r['pass'] in ('actors','fx')]
  assert len(ds)==len(ss),(frame,len(ds),len(ss))
  if not any(s['raster'] for s in summaries if s['frame']==frame):
   for q in ss:q['raster_samples']=None
  for d,q in zip(ds,ss):
   assert d['indices']==q['triangles']*3,(d,q)
   assert d['effect']==(q['pass']=='fx')
   d['query']=q
  inventory=[]
  for piece,pose in zip(pieces[frame],[r for r in poses[frame] if 'chess' in r['path']]):
   key=[int(pose['identity']),int(pose['pose'])]
   owned=[r for r in ds if r['owner']==key]
   body=[r for r in owned if not r['label'][0]]
   attachments=[r for r in owned if r['label'][0]]
   inventory.append(dict(piece=piece,owner=key,body_draws=len(body),attachment_draws=len(attachments),
    draws=len(owned),indices=sum(d['indices'] for d in owned),packet_vertices=sum(d['vertices'] for d in owned),
    body_indices=sum(d['indices'] for d in body),body_packet_vertices=sum(d['vertices'] for d in body),
    samples=sum(d['query']['samples'] for d in owned),raster_samples=sum(d['query']['raster_samples'] for d in owned) if all(d['query']['raster_samples'] is not None for d in owned) else None,
    max_projected_display_pixels=max((d['projected_long_side_display_pixels'] or 0 for d in owned),default=0),
    layers=[dict(group=d['group'],pass_id=d['pass'],indices=d['indices'],vertices=d['vertices'],blend=d['blend'],depth_write=d['depth_write'],equal=d['equal'],cutout=d['cutout'],discard_free=d['discard_free'],label=d['label'],samples=d['query']['samples'],raster_samples=d['query']['raster_samples']) for d in owned]))
  families={}
  for name,predicate in [('actors',lambda d:not d['effect']),('chess_body',lambda d:not d['effect'] and d['label'] and not d['label'][0] and 'chess' in (d['label'][1] or '')),('ants',lambda d:not d['effect'] and d['label'] and 'ant' in (d['label'][1] or '')),('fx',lambda d:d['effect'])]:
   sub=[d for d in ds if predicate(d)]
   families[name]=dict(draws=len(sub),indices=sum(d['indices'] for d in sub),packet_vertices=sum(d['vertices'] for d in sub),samples=sum(d['query']['samples'] for d in sub),raster_samples=sum(d['query']['raster_samples'] for d in sub) if all(d['query']['raster_samples'] is not None for d in sub) else None,depth_tests=sorted(set(d['query']['depth_test'] for d in sub)),depth_writes=sorted(set(d['depth_write'] for d in sub)),eligible_discard_free=sum(d['discard_free'] for d in sub))
  frames[str(frame)]=dict(pieces=inventory,families=families,draws=ds)
 reports[tape]=dict(provenance=json.loads(receipt.read_text()),frames=frames,sample_summaries=summaries,raw_groups=raw,limits='Host replay with its recorded deterministic host/base divergence. Index counts are not unique vertices. Packet vertices sum repeated layer payloads. SAMPLES_PASSED is post-depth host coverage; raster_samples is an additional masked no-depth diagnostic draw, never production duplication. No native time, native coverage, or native shader compiler claim.')
(P/'attribution.json').write_text(json.dumps(reports,indent=2)+'\n')
for tape,r in reports.items():
 print(tape)
 for frame,row in r['frames'].items():print(frame,row['families'])
