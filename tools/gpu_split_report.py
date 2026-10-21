#!/usr/bin/env python3
"""Strict reducer for passive GPU sidecars; no device access or source writes."""
import argparse
import csv
import json
from pathlib import Path

# Schema deliberately independent of the existing light/native-timing CSVs.
NAMES = [
    'gpu_submit_wall_us', 'gpu_submit_cpu_us', 'gpu_submit_nonwait_cpu_us',
    'gpu_wait_api_wall_us', 'gpu_wait_api_cpu_us', 'gpu_wait_off_cpu_us',
    'gpu_gl_flush_us', 'gpu_gl_flush_calls', 'gpu_gl_finish_us', 'gpu_gl_finish_calls',
    'gpu_vgl_swap_us', 'gpu_vgl_swap_calls', 'gpu_gxm_finish_us', 'gpu_gxm_finish_calls',
    'gpu_gxm_transfer_finish_us', 'gpu_gxm_transfer_finish_calls',
    'gpu_gxm_notification_wait_us', 'gpu_gxm_notification_wait_calls',
    'gpu_gxm_display_queue_add_us', 'gpu_gxm_display_queue_add_calls',
    'gpu_gxm_display_queue_finish_us', 'gpu_gxm_display_queue_finish_calls',
    'gpu_gxm_begin_scene_us', 'gpu_gxm_begin_scene_calls',
    'gpu_gxm_end_scene_us', 'gpu_gxm_end_scene_calls',
    'gpu_submit_scopes', 'gpu_clock_reads', 'gpu_clock_errors', 'gpu_probe_frames',
    'gpu_gl_flush_cpu_us', 'gpu_gl_finish_cpu_us', 'gpu_vgl_swap_cpu_us',
    'gpu_pre_swap_wall_us', 'gpu_pre_swap_cpu_us', 'gpu_pre_swap_nonwait_cpu_us',
]
HEADER = ['frame', 'wall_us'] + NAMES


def stats(values):
    s = sorted(values)
    return dict(samples=len(s), median_us=s[len(s)//2], p95_us=s[len(s)*95//100],
                max_us=s[-1], mean_us=sum(s)/len(s), total_us=sum(s))


def read(path):
    with Path(path).open(encoding='utf-8', newline='') as f:
        r = csv.reader(f)
        if next(r, None) != HEADER:
            raise ValueError('missing, duplicate, reordered or unknown GPU columns')
        rows = []
        for raw in r:
            if len(raw) != len(HEADER) or any(not v.isascii() or not v.isdecimal() for v in raw):
                raise ValueError('malformed/noninteger/nonnegative GPU row')
            rows.append(dict(zip(HEADER, map(int, raw))))
    if [r['frame'] for r in rows] != list(range(1, len(rows)+1)):
        raise ValueError('duplicate, missing or reordered frame IDs')
    return rows


def summarize(rows, mode='on', first=61, frames=120):
    if first < 1 or frames < 1 or frames % 60:
        raise ValueError('select complete 60-frame windows')
    chosen = [r for r in rows if first <= r['frame'] < first+frames]
    if [r['frame'] for r in chosen] != list(range(first, first+frames)):
        raise ValueError('incomplete selected population')
    if any(r['wall_us'] == 0 for r in chosen):
        raise ValueError('zero wall interval')
    if mode == 'off':
        if any(r[k] for r in chosen for k in NAMES):
            raise ValueError('nonzero probe-off value')
    else:
        for r in chosen:
            if r['gpu_probe_frames'] != 1 or r['gpu_clock_errors'] or r['gpu_vgl_swap_calls'] != 1:
                raise ValueError('invalid clock, unarmed row or missing/duplicate swap')
            if not r['gpu_submit_scopes'] or not r['gpu_clock_reads']:
                raise ValueError('missing submission observation')
            for prefix in ('gpu_submit', 'gpu_pre_swap'):
                if r[prefix+'_nonwait_cpu_us'] > r[prefix+'_cpu_us']:
                    raise ValueError('nonwait CPU exceeds inclusive CPU')
        for window in (chosen[i:i+60] for i in range(0, frames, 60)):
            for key in ('gpu_submit_cpu_us', 'gpu_pre_swap_cpu_us',
                        'gpu_gxm_begin_scene_calls', 'gpu_gxm_end_scene_calls',
                        'gpu_gxm_display_queue_add_calls'):
                if sum(r[key] for r in window) == 0:
                    raise ValueError('clock did not advance or missing hook coverage: '+key)
    def window_report(window):
        result = {k: stats([r[k] for r in window]) for k in HEADER[1:]}
        # Per-frame differences FIRST; never subtract independently computed quantiles.
        for parent in ('gl_flush', 'gl_finish', 'vgl_swap'):
            result['gpu_'+parent+'_off_cpu_us_estimate'] = stats([
                max(0, r['gpu_'+parent+'_us']-r['gpu_'+parent+'_cpu_us']) for r in window])
        return dict(frame_first=window[0]['frame'], frame_last=window[-1]['frame'], metrics=result)
    return dict(mode=mode, gpu_busy_us=None,
                caveat='API elapsed includes CPU and preemption; off-CPU is not GPU execution time. Parent and child brackets overlap. Pre-swap includes simulation/bookkeeping.',
                quantiles='sorted[n//2], sorted[n*95//100]',
                selected=window_report(chosen),
                windows=[window_report(chosen[i:i+60]) for i in range(0, frames, 60)])


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('csv', type=Path)
    p.add_argument('--mode', choices=['on', 'off'], required=True)
    p.add_argument('--first', type=int, default=61)
    p.add_argument('--frames', type=int, default=120)
    a = p.parse_args()
    print(json.dumps(summarize(read(a.csv), a.mode, a.first, a.frames), indent=2))


if __name__ == '__main__':
    main()
