"""Offline analysis of the final validated per-arm logs; selected window only.

Per arm: total and render median/p95/max, gxm_end median/p95/max/mean, scene bucket
median/p95, the gxm_end p95 delta against `control` (world_gxm off, mask empty), and a
per-frame row for 850, 962, 1047, 1048. `base-live` keeps world_gxm live and no mask; its
delta is the world_gxm mode change, not a masked class.
"""
import json
import math
import re
import statistics
from pathlib import Path

P = Path(__file__).resolve().parent
FRAMES = [850, 962, 1047, 1048]


def percentile(values, q):
    return values[math.ceil(q * len(values)) - 1]


def stats(values):
    return dict(n=len(values), median=statistics.median(values),
                p95=percentile(values, .95), max=max(values))


def number(row, key):
    value = row.get(key)
    return float(value) if value not in (None, '-') else 0.0


def scene_ms(row):
    match = re.search(r'(?:^|,)scene:([0-9.]+)', row['sim_split'])
    return float(match[1]) if match else 0.


def paired_scene_deltas(rows):
    return sorted(abs(scene_ms(row)-number(row, "gxm_end")) for row in rows)

def main():
    manifest = json.loads((P / 'READY.json').read_text())
    results = json.loads((P / 'actor-gpu-step-results.json').read_text())
    output = []
    for result in results:
        arm = next(a for a in manifest['arms'] if a['name'] == result['arm'])
        text = Path(result['arm_log']).read_text()
        rows = {}
        for line in text.splitlines():
            if '[lg/spike] frame=' not in line:
                continue
            values = dict(re.findall(
                r'\b(frame|ms|sim|render|hud|present|gxm_begin|gxm_end|sim_split|gxm_end_in)=([^\s]+)',
                line))
            frame = int(values['frame'])
            if arm['window'][0] <= frame <= arm['window'][1]:
                rows[frame] = values
        assert len(rows) == 481, (arm['name'], len(rows))
        ordered = [rows[f] for f in range(arm['window'][0], arm['window'][1] + 1)]
        totals = sorted(number(r, 'ms') for r in ordered)
        renders = sorted(number(r, 'render') for r in ordered)
        ends = sorted(number(r, 'gxm_end') for r in ordered)
        scenes = sorted(scene_ms(r) for r in ordered)
        assert all(math.isfinite(v) and v >= 0 for v in totals + renders + ends + scenes)
        scene_wait = 0.0
        regions = {}
        for r in ordered:
            for name, ms, calls in re.findall(r'([a-z_0-9]+):([0-9.]+)/([0-9]+)', r['gxm_end_in']):
                regions[name] = regions.get(name, 0) + float(ms)
                if name == 'scene':
                    scene_wait += float(ms)
        total_end = sum(ends)
        close = paired_scene_deltas(ordered)
        output.append(dict(
            arm=arm['name'], map=arm['map'], world_gxm=arm['world_gxm'], mask=arm['mask'],
            tape=arm['tape'], window=arm['window'], n=len(ordered),
            total=stats(totals), render=stats(renders),
            gxm_end=dict(stats(ends), mean=statistics.mean(ends)),
            scene=stats(scenes),
            gxm_end_in=dict(sum=regions,
                            scene_share=(scene_wait / total_end) if total_end else 0.),
            abs_scene_minus_gxm_end=dict(median=statistics.median(close), p95=percentile(close, .95)),
            frames={str(f): dict(total=number(rows[f], 'ms'), gxm_end=number(rows[f], 'gxm_end'),
                                 scene=scene_ms(rows[f]))
                    for f in FRAMES if f in rows},
            delta_gxm_end_p95_vs_control=None, delta_total_p95_vs_control=None))
    by_name = {row['arm']: row for row in output}
    controls = {row['map']:row for row in output if row['arm'].endswith('-control')}
    for row in output:
        control=controls.get(row['map'])
        if control:
            row['delta_gxm_end_p95_vs_control'] = round(
                row['gxm_end']['p95'] - control['gxm_end']['p95'], 3)
            row['delta_total_p95_vs_control'] = round(
                row['total']['p95'] - control['total']['p95'], 3)
            row['gxm_end_p95_ratio_vs_control'] = round(
                row['gxm_end']['p95'] / control['gxm_end']['p95'], 3) if control['gxm_end']['p95'] else None
    assert set(controls)=={row['map'] for row in output}, 'missing map control'
    (P / 'analysis.json').write_text(json.dumps(
        dict(scope='SELECTED_WINDOW_ONLY', control='per-map control', frame_table=FRAMES,
             arms=output), indent=2) + '\n')
    header = ('%-24s %7s %7s %7s %8s %8s %8s %8s %8s' %
              ('arm', 'ms p50', 'ms p95', 'ms max', 'end p50', 'end p95', 'end max',
               'scene p95', 'dEnd95'))
    print(header)
    for row in output:
        print('%-24s %7.2f %7.2f %7.2f %8.2f %8.2f %8.2f %8.2f %+8.2f' % (
            row['arm'], row['total']['median'], row['total']['p95'], row['total']['max'],
            row['gxm_end']['median'], row['gxm_end']['p95'], row['gxm_end']['max'],
            row['scene']['p95'], row['delta_gxm_end_p95_vs_control'] or 0.))
    print('\nspike frames (total / gxm_end ms)')
    print('%-24s %17s %17s %17s %17s' % ('arm', *['%d' % f for f in FRAMES]))
    for row in output:
        cells = []
        for f in FRAMES:
            v = row['frames'].get(str(f))
            cells.append('%.1f/%.1f' % (v['total'], v['gxm_end']) if v else '-')
        print('%-24s %17s %17s %17s %17s' % (row['arm'], *cells))


if __name__ == '__main__':
    main()
