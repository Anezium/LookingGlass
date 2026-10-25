"""Copy the read-only r11c package's documents and freeze the r11d run schedule."""
from pathlib import Path
import hashlib, json, shutil, sys

R = Path(__file__).resolve().parents[2]
T = Path(__file__).resolve().parent
O = Path(sys.argv[1]).resolve()
assert O.name == 'alice-transition-mem-r11d'
old = O.with_name('alice-transition-mem-r11c')
for directory in ['configs', 'console', 'scripts', 'packages/baseline', 'packages/stack', 'logs', 'proof']:
    (O / directory).mkdir(parents=True, exist_ok=True)
snapshot = {str(p.relative_to(old)): dict(bytes=p.stat().st_size,
            sha256=hashlib.sha256(p.read_bytes()).hexdigest())
            for p in sorted(old.rglob('*')) if p.is_file()}
(O / 'proof/R11C-READ-ONLY.json').write_text(json.dumps(snapshot, indent=2) + '\n')
for p in old.glob('*.md'):
    shutil.copyfile(p, O / p.name)
# Historical reports remain historical, with a clearly labelled correction in
# these new copies only. The originals and their seals are never modified.
correction = '''> R11d correction to this historical report: LG uses sample_count=1
> everywhere, with a separately owned resolve texture. No LG portal double-delete
> crash was reachable. The miniquad fix is a defensive guard against deleting a
> possibly recycled GL name. The macroquad sample_count=0 ownership fix is correct
> but currently unused by LG. See SOL-REPORT-R11D for the corrected evidence and
> the attributed frame-one future-layout regression. This is a new package copy;
> the original sealed report is unchanged.

'''
for name in ['SOL-REPORT.md', 'SOL-REPORT-R11B.md', 'SOL-REPORT-R11C.md']:
    path = O / name
    path.write_text(correction + path.read_text())
plan = dict(fixed_before_execution=True, warmup_cycles=3, cycles_per_operation=30,
            actions=['retry', 'native-load', 'chapter'], lever_arms=[False, True],
            frames_between_operations=136, sample_offset_frames=126, simulation_dt_seconds=0.05,
            chapter_operations=60, retry_operations=30, native_load_operations=30,
            range_limit_bytes=16384, absolute_slope_limit_bytes_per_cycle=1024,
            last_ten_absolute_slope_limit_bytes_per_cycle=1024,
            after_failure='Do not lengthen or replace the fixed population; diagnose the source.')
(O / 'proof/E2-FIXED-PLAN.json').write_text(json.dumps(plan, indent=2) + '\n')
print('Copied r11c documents read-only and fixed E2 at 30 cycles for every operation/arm')
