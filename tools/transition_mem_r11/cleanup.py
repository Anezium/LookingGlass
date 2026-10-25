"""Remove only this task's native/Python caches; retain the shared host target."""
from pathlib import Path
import json,subprocess,shutil,sys
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();receipt=O/'proof/CLEANUP.json';previous=json.loads(receipt.read_text()) if receipt.exists() else {};removed=previous.get('removed',[]);before=subprocess.check_output(['df','-h',str(O)],text=True)
paths=[O/'target-vita',O/'tmp',Path('/Users/anezium/Library/Caches/com.apple.python')/str(R).lstrip('/'),Path('/Users/anezium/Library/Caches/com.apple.python')/str(O).lstrip('/')]
paths+=list((R/'tools/transition_mem_r11').rglob('__pycache__'))+list(O.rglob('__pycache__'))
for p in paths:
 if not p.exists():continue
 size=sum(f.stat().st_size for f in p.rglob('*') if f.is_file());shutil.rmtree(p);removed.append(dict(path=str(p),bytes=size))
receipt.write_text(json.dumps(dict(removed=removed,removed_bytes=sum(r['bytes'] for r in removed),df_before=previous.get('df_before',before),df_after=subprocess.check_output(['df','-h',str(O)],text=True),shared_host_target_retained=True),indent=2)+'\n');print('Task caches removed; shared host target retained')
