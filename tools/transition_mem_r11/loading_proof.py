"""Require exact original and last-submitted-before-build loading pixels."""
from pathlib import Path
from PIL import Image
import sys,json,hashlib,subprocess
O=Path(sys.argv[1]);R=Path(__file__).resolve().parents[2];rows=[]
for on in [False,True]:
 files=[O/'visual/host'/f'exit-{int(on)}-{arm}'/name for arm in ['before','after'] for name in ['loading-presented.png','loading-build.png']]
 images=[Image.open(p).convert('RGBA') for p in files];raw=[im.tobytes() for im in images]
 assert len(set(raw))==1 and len(set(raw[0]))>2,(on,'Loading/title frame missing at build boundary')
 rows.append(dict(levers=on,exact=True,last_submitted_frame_before_build=True,files=[str(p.relative_to(O)) for p in files],pixel_sha256=hashlib.sha256(raw[0]).hexdigest()))
# Bind parity to the original loading drawing operations, before release-first.
old=subprocess.check_output(['git','show','e19f73c:src/viewer.rs'],cwd=R,text=True)
current=(R/'src/viewer.rs').read_text();body=current.split('fn draw_transition_loading(name: &str) {',1)[1].split('\n}',1)[0]
assert body.strip() in old,'Original Loading/title drawing commands changed'
(O/'proof/LOADING-FRAME-EQUALITY.json').write_text(json.dumps(dict(status='PASS',reference_revision='e19f73c',original_drawing_commands_unchanged=True,rows=rows),indent=2)+'\n')
print('PASS original Loading/title pixels retained through the build boundary')
