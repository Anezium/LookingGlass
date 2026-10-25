"""Remove only this task's native/Python caches; retain the shared host target."""
from pathlib import Path
import json,subprocess,shutil,sys
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();receipt=O/'proof/CLEANUP.json';previous=json.loads(receipt.read_text()) if receipt.exists() else {};removed=previous.get('removed',[]);before=subprocess.check_output(['df','-h',str(O)],text=True)
paths=[O/'target-vita',O/'tmp',Path('/Users/anezium/Library/Caches/com.apple.python')/str(R).lstrip('/'),Path('/Users/anezium/Library/Caches/com.apple.python')/str(O).lstrip('/')]
paths+=list((R/'tools/transition_mem_r11').rglob('__pycache__'))+list(O.rglob('__pycache__'))
paths.append(Path('/tmp/r11b-heap-source-backup'))
for name in ['r11b-edit.py','r11b-heap-diagnose.py','r11b-heap-analyze.py','r11b-fix-texture-metadata.py','r11b-stop-superseded.py']:
 p=Path('/tmp')/name
 if p.exists():removed.append(dict(path=str(p),bytes=p.stat().st_size));p.unlink()
for name in ['r11b-bin','r11b2-bin','r11b3-bin','heap-diagnostic-bin']:
 p=O/'proof'/name
 if p.exists():
  removed.append(dict(path=str(p),bytes=p.stat().st_size));p.unlink()
for p in paths:
 if not p.exists():continue
 size=sum(f.stat().st_size for f in p.rglob('*') if f.is_file());shutil.rmtree(p);removed.append(dict(path=str(p),bytes=size))
receipt.write_text(json.dumps(dict(removed=removed,removed_bytes=sum(r['bytes'] for r in removed),df_before=previous.get('df_before',before),df_after=subprocess.check_output(['df','-h',str(O)],text=True),shared_host_target_retained=True),indent=2)+'\n');print('Task caches removed; shared host target retained')
