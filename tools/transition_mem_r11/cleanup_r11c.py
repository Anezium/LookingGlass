"""Delete task-owned caches; retain final proof binaries and shared host builds."""
from pathlib import Path
import json,shutil,subprocess,sys
R=Path(__file__).resolve().parents[2];O=Path(sys.argv[1]).resolve();receipt=O/'proof/CLEANUP-R11C.json'
old=json.loads(receipt.read_text()) if receipt.exists() else {};removed=old.get('removed',[])
before=subprocess.check_output(['df','-h',str(O)],text=True)
paths=[O/'target-vita',O/'tmp',O/'proof/r11c-diagnostic-bin',O/'resource-diagnostic/diagnostic-bin',O/'proof/transition-bin',Path('/tmp/r11c-metadata.json'),Path('/tmp/r11c-vendor-removal.log')]
for root in [R/'tools/transition_mem_r11',O]:paths+=list(root.rglob('__pycache__'))
for root in [R,O]:paths.append(Path('/Users/anezium/Library/Caches/com.apple.python')/str(root).lstrip('/'))
target=Path('/Users/anezium/vita-j0/cargo-target-host/release')
for p in (target/'deps').glob('*.d'):
 text=p.read_text(errors='replace')
 if any(s in text for s in ['r11c-dependency-test-','r11c-texture-','r11c-resource-']):
  stem=p.stem;paths+=list(p.parent.glob(stem+'.*'))+list(p.parent.glob('lib'+stem+'.*'))
  paths.append(p.parent/stem)
  paths.append(target/'.fingerprint'/stem)
paths += list(target.glob('r11c-texture-regression*'))+list((target/'.fingerprint').glob('r11c-texture-regression-*'))
for p in dict.fromkeys(paths):
 if not p.exists():continue
 size=sum(f.stat().st_size for f in p.rglob('*') if f.is_file()) if p.is_dir() else p.stat().st_size
 if p.is_dir():shutil.rmtree(p)
 else:p.unlink()
 removed.append(dict(path=str(p),bytes=size))
receipt.write_text(json.dumps(dict(removed=removed,removed_bytes=sum(v['bytes'] for v in removed),df_before=old.get('df_before',before),df_after=subprocess.check_output(['df','-h',str(O)],text=True),shared_host_target_retained=True),indent=2)+'\n')
print('PASS task caches removed; shared host target retained')
