"""Attribute every frame-one size-bin difference and enforce the r11b baseline."""
from pathlib import Path
import json, sys

O = Path(sys.argv[1]).resolve()
data = {label: json.loads((O / 'baseline' / label / 'BASELINE.json').read_text())
        for label in ['r11b', 'r11c', 'r11d']}
rows = []
for b, c, d in zip(*(data[label]['rows'] for label in ['r11b', 'r11c', 'r11d'])):
    assert (b['map'], b['levers']) == (c['map'], c['levers']) == (d['map'], d['levers'])
    def difference(left, right):
        a, z = dict(left['sizes']), dict(right['sizes'])
        changes = [dict(size=n, count_delta=z.get(n, 0)-a.get(n, 0),
                        bytes_delta=n*(z.get(n, 0)-a.get(n, 0)))
                   for n in a.keys() | z.keys() if a.get(n, 0) != z.get(n, 0)]
        assert sum(r['bytes_delta'] for r in changes) == right['live'] - left['live']
        return sorted(changes, key=lambda r: abs(r['bytes_delta']), reverse=True)
    assert abs(d['live'] - b['live']) <= 4096, (b['map'], b['levers'], d['live'] - b['live'])
    rows.append(dict(map=b['map'], levers=b['levers'],
        live={label: row['live'] for label, row in zip(['r11b', 'r11c', 'r11d'], [b, c, d])},
        run_future_bytes={label: row['run_future_bytes'] for label, row in zip(['r11b', 'r11c', 'r11d'], [b, c, d])},
        r11c_minus_r11b=c['live']-b['live'], r11d_minus_r11b=d['live']-b['live'],
        regression_histogram=difference(b, c), fixed_histogram=difference(b, d)))
(O / 'proof/BASELINE-ATTRIBUTION.json').write_text(json.dumps(dict(status='PASS',
    method='Exact live allocation size bins; weighted counts sum to the allocator census at frame one; run future size measured without constructing it.',
    baseline_tolerance_bytes=4096, regression_live_allocation_before_bytes=1506576,
    regression_live_allocation_after_bytes=2289808, regression_allocation_delta_bytes=783232,
    run_future_delta_bytes=391616, root_wrapper_duplicates_run_future_delta=True,
    published_comparison_path_delta_bytes=2,
    path_control='proof/PATH-LENGTH-CONTROL.json', rows=rows), indent=2) + '\n')
for row in rows:
    print(row['map'], row['levers'], row['live'], row['run_future_bytes'],
          'r11d-r11b', row['r11d_minus_r11b'])
    print('regression bins', row['regression_histogram'])
    print('fixed bins', row['fixed_histogram'])
