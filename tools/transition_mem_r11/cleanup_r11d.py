"""Delete only owned build/diagnostic/Python caches, retaining final proof binaries."""
from pathlib import Path
import json, shutil, subprocess, sys
R = Path(__file__).resolve().parents[2]
O = Path(sys.argv[1]).resolve()
assert O.name == 'alice-transition-mem-r11d'
receipt = O / 'proof/CLEANUP-R11D.json'
prior = json.loads(receipt.read_text()) if receipt.exists() else {}
removed = prior.get('removed', [])
before = subprocess.check_output(['df','-h',str(O)],text=True)
paths = [O/'target-vita', O/'target-diagnostic', O/'tmp']
paths += list((O/'baseline').glob('*/diagnostic-bin'))
for root in [R/'tools/transition_mem_r11', O]:
    paths += list(root.rglob('__pycache__'))
for root in [R,O]:
    paths.append(Path('/Users/anezium/Library/Caches/com.apple.python')/str(root).lstrip('/'))
target = Path('/Users/anezium/vita-j0/cargo-target-host/release')
for p in (target/'deps').glob('*.d'):
    if any(marker in p.read_text(errors='replace') for marker in ['r11c-dependency-test-', 'r11c-texture-', 'r11d-baseline-']):
        stem=p.stem
        paths += list(p.parent.glob(stem+'.*')) + list(p.parent.glob('lib'+stem+'.*'))
        paths += [p.parent/stem, target/'.fingerprint'/stem]
paths += list(target.glob('r11c-texture-regression*'))
paths += list((target/'.fingerprint').glob('r11c-texture-regression-*'))
for path in dict.fromkeys(paths):
    if not path.exists(): continue
    size = sum(p.stat().st_size for p in path.rglob('*') if p.is_file()) if path.is_dir() else path.stat().st_size
    if path.is_dir(): shutil.rmtree(path)
    else: path.unlink()
    removed.append(dict(path=str(path),bytes=size))
receipt.write_text(json.dumps(dict(removed=removed,removed_bytes=sum(r['bytes'] for r in removed),
    df_before=prior.get('df_before',before),df_after=subprocess.check_output(['df','-h',str(O)],text=True),
    shared_host_target_retained=True),indent=2)+'\n')
print('PASS task-owned caches removed; shared host target retained')
