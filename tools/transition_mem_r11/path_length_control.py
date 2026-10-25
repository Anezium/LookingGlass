"""Account for the two path bytes in the published +783,234-byte comparison."""
from pathlib import Path
import json, os, shutil, subprocess, sys
O = Path(sys.argv[1]).resolve()
B = O / 'baseline/r11c/diagnostic-bin'
rows = []
for name in ['short', 'longer']:
    d = O / 'baseline/path-control' / name
    (d / 'private/saves').mkdir(parents=True, exist_ok=True)
    shutil.copyfile(O / 'baseline/r11c/hedge3-0/private/vita_renderer.json', d / 'private/vita_renderer.json')
    env = {k: v for k, v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
    env.update(LOOKING_GLASS_TRANSITION_CENSUS='1', LOOKING_GLASS_TRANSITION_PLAN='[]',
               LOOKING_GLASS_SIM_MAPS_PROBE='1', LOOKING_GLASS_SIM_MAPS_DT='0.05',
               LOOKING_GLASS_SAH_SILENT_AUDIO='1')
    with (d / 'run.log').open('w') as log:
        subprocess.run([str(B), '--data', '/Users/anezium/vita-work/alice-data/Alice1/bin/base',
            '--map', 'hedge3', '--frames', '3', '--save-dir', str(d / 'private/saves')],
            cwd=d, env=env, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=180)
    heap = next(json.loads(l.split(' ', 1)[1]) for l in (d/'run.log').read_text().splitlines() if l.startswith('BASELINE_HEAP '))
    rows.append(dict(path=str(d / 'private/saves'), **heap))
a, b = rows
x, y = dict(a['sizes']), dict(b['sizes'])
changes = [dict(size=n, count_delta=y.get(n, 0)-x.get(n, 0), bytes_delta=n*(y.get(n, 0)-x.get(n, 0)))
           for n in x.keys() | y.keys() if x.get(n, 0) != y.get(n, 0)]
assert b['live'] - a['live'] == 2
assert sum(v['bytes_delta'] for v in changes) == 2
(O / 'proof/PATH-LENGTH-CONTROL.json').write_text(json.dumps(dict(status='PASS',
    extra_save_path_characters=1, extra_live_bytes=2, histogram_changes=changes, rows=rows), indent=2) + '\n')
print('PASS one extra save-path character adds exactly two live request bytes', changes)
