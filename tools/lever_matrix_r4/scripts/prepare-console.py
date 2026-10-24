"""Materialize the reviewed r4 console draft outside the source repository."""
from pathlib import Path
import shutil,sys
T=Path(__file__).resolve().parents[1];O=Path(sys.argv[1]).resolve()
assert not O.is_relative_to(T.parents[1]), "Output must be outside the source repository"
O.mkdir(parents=True,exist_ok=True)
for name in ['CONSOLE-BRIEF.md','DESIGN.md','INHERITED-R3-BRIEF.md','CONSOLE-PROTOCOL.json']:shutil.copy2(T/name,O/name)
for d in ['configs','console','scripts']:
 (O/d).mkdir(exist_ok=True)
 for f in (T/d).iterdir():
  if f.is_file():shutil.copy2(f,O/d/f.name)
for f in (T/'manifests').glob('*.json'):(O/f.name).write_text(f.read_text().replace('@OUTPUT@',str(O)))
print('Reviewed draft materialized; no console contact')
