"""Repeat all four frame-one measurements using the immutable uninstrumented proof binary."""
from pathlib import Path
import hashlib, json, os, shutil, subprocess, sys
O = Path(sys.argv[1]).resolve()
B = Path(sys.argv[2]).resolve()
assert b'BASELINE_RUN_FUTURE_BYTES' not in B.read_bytes()
reference = json.loads((O / 'baseline/r11b/BASELINE.json').read_text())['rows']
old = O.with_name('alice-transition-mem-r11')
old_binary = old/'proof/recovery-bin'
old_sha = hashlib.sha256(old_binary.read_bytes()).hexdigest()
assert old_sha == json.loads((old/'proof/TRANSITION-EQUALITY.json').read_text())['binary_sha256']
comparison_binary = O/'baseline/final-comparison-bin'
rows = []
for ref in reference:
    d = O / 'baseline/r11d' / f"{ref['map']}-{int(ref['levers'])}"
    env = {k:v for k,v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
    env.update(LOOKING_GLASS_TRANSITION_CENSUS='1', LOOKING_GLASS_TRANSITION_PLAN='[]',
               LOOKING_GLASS_SIM_MAPS_PROBE='1', LOOKING_GLASS_SIM_MAPS_DT='0.05',
               LOOKING_GLASS_SAH_SILENT_AUDIO='1')
    values = {}
    for label, binary in [('r11b',old_binary),('r11d',B)]:
        # Identical executable, cwd, save paths and environment between versions.
        shutil.copy2(binary,comparison_binary.with_suffix('.new'))
        os.replace(comparison_binary.with_suffix('.new'),comparison_binary)
        log_path = d/f'final-{label}.log'
        with log_path.open('w') as log:
            subprocess.run([str(comparison_binary), '--data', '/Users/anezium/vita-work/alice-data/Alice1/bin/base',
                '--map', ref['map'], '--frames', '3', '--save-dir', str(d/'private/saves')],
                cwd=d, env=env, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=180)
        sample = next(json.loads(l.split(' ',1)[1]) for l in log_path.read_text().splitlines()
            if l.startswith('TRANSITION_STEADY ') and json.loads(l.split(' ',1)[1])['frame']==1)
        values[label] = sample['live_rust_request_bytes']
    difference = values['r11d'] - values['r11b']
    assert abs(difference) <= 4096, (ref['map'],ref['levers'],difference)
    rows.append(dict(map=ref['map'],levers=ref['levers'],**values,difference=difference))
(O/'proof/BASELINE-FINAL.json').write_text(json.dumps(dict(status='PASS',
    binary_sha256=hashlib.sha256(B.read_bytes()).hexdigest(),r11b_binary_sha256=old_sha,
    same_executable_path_and_fixture=True,rows=rows),indent=2)+'\n')
print('PASS uninstrumented final frame-one baseline',rows)
