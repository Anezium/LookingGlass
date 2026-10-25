"""Same-implementation controls for a provisional two-pixel host raster delta."""
from pathlib import Path
import hashlib,json,shutil,sys
from PIL import Image
O=Path(sys.argv[1]).resolve();B=Path(sys.argv[2]).resolve();T=Path(__file__).resolve().parent
D=O/'raster-diagnostic';D.mkdir(exist_ok=True)
for legacy in [True,False]:
 p=O/'failure'/f'native-load-npcs-0-{int(legacy)}'
 if p.exists():shutil.copytree(p,D/f'initial-{int(legacy)}',dirs_exist_ok=True)
namespace={'__file__':str(T/'failure_proof.py')}
source=(T/'failure_proof.py').read_text().split('for on in [False,True]:',1)[0]
exec(compile(source,str(T/'failure_proof.py'),'exec'),namespace)
rows=[]
for i in range(3):
 for legacy in [True,False]:
  row=namespace['run'](False,'native-load','npcs',legacy)
  target=D/f'control-{int(legacy)}-{i}';shutil.copytree(row['dir'],target,dirs_exist_ok=True)
  rows.append(dict(legacy=legacy,repeat=i,png_sha256=hashlib.sha256((target/'final.png').read_bytes()).hexdigest()))
  print(rows[-1],flush=True)
(O/'proof/RASTER-DIAGNOSTIC.json').write_text(json.dumps(dict(binary_sha256=hashlib.sha256(B.read_bytes()).hexdigest(),provisional_failure='two adjacent blue channels differed by 1/255; Game and packets matched',rows=rows),indent=2)+'\n')
