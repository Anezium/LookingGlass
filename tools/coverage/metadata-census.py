#!/usr/bin/env python3
import json,re,subprocess,zipfile
from pathlib import Path
import argparse
ap=argparse.ArgumentParser();ap.add_argument('--binary',type=Path,required=True);ap.add_argument('--output',type=Path,required=True);ap.add_argument('--data',type=Path,required=True);args=ap.parse_args()
root=args.output;root.mkdir(parents=True,exist_ok=True);(root/'logs').mkdir(exist_ok=True)
data=args.data
with zipfile.ZipFile(data/'pak0.pk3') as archive:
    names=sorted(Path(n).stem for n in archive.namelist() if n.lower().startswith('maps/') and n.lower().endswith('.bsp'))
out=[]
for name in names:
    p=subprocess.run([str(args.binary),'--data',str(data),'--map',name,'--inspect'],capture_output=True,text=True,timeout=30)
    (root/'logs'/('inspect-'+name+'.log')).write_text(p.stdout+p.stderr)
    row=dict(map=name,exit_code=p.returncode)
    for k,label in [('vertices','Vertices'),('surfaces','Surfaces'),('entities','Entities'),('lightmaps','Lightmaps')]:
        m=re.search(r'^'+label+r': (\d+)',p.stdout,re.M)
        row[k]=int(m.group(1)) if m else None
    out.append(row)
(root/'MAP-METADATA.json').write_text(json.dumps(out,indent=2)+'\n')
print('maps',len(out),'garden3',next(r for r in out if r['map']=='garden3'),'gvillage',next(r for r in out if r['map']=='gvillage'))
