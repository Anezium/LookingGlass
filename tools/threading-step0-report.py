#!/usr/bin/env python3
"""Strict step-0b CSV reduction. Missing/duplicate columns and partial windows fail."""
import argparse
import csv
import json
import math
import statistics
from pathlib import Path

BASE = 'frame wall_us frame_cpu_us swap_us skin_collect_wait_us skin_gather_us skin_fallbacks skin_overflow particle_producer_us fx_merge_us fx_upload_cpu_us fx_upload_gl_us fx_upload_bytes fx_finish_prepare_us fx_worker_wait_us fx_finish_recycle_us'.split()
PHASES = 'world_opaque world_alpha actors fx hud present'.split()
FIELDS = 'cpu_us draws pipelines depth_states bindings uniform_uploads uniform_bytes buffer_updates buffer_bytes texture_bind_slots visibility_tests uniform_gl_calls visibility_cpu_us parameter_cpu_us deferred_cpu_us texture_gl_binds state_gl_calls program_gl_changes'.split()
PROBES = 'input interaction_search interactions movers movement story particles combat alice audio camera listener scene_post_sim hud_queries hud_other other world_opaque_visibility_numeric world_opaque_parameter_numeric world_opaque_texture_resource world_opaque_pass_resource fx_sort_cpu fx_batch_mixed fx_observe_cpu fx_merge_mixed fx_upload_pack_mixed fx_upload_buffer_gl fx_recycle_cpu fx_recycle_resource pickup_other_mixed prop_numeric prop_layer_numeric pickup_select_cpu prop_publish_resource pickup_resource attached_sample_cpu skin_worker0_service skin_worker1_service'.split()
PROBES = [name + '_us' for name in PROBES]
WORLD = 'world_prepare_jobs world_prepare_fallbacks world_prepare_dispatch_us world_prepare_wait_us world_prepare_service_us world_prepare_inline_us world_prepare_queue_us'.split()
GPU = ['gpu_submit_wall_us', 'gpu_submit_cpu_us', 'gpu_submit_nonwait_cpu_us', 'gpu_wait_api_wall_us', 'gpu_wait_api_cpu_us', 'gpu_wait_off_cpu_us', 'gpu_gl_flush_us', 'gpu_gl_flush_calls', 'gpu_gl_finish_us', 'gpu_gl_finish_calls', 'gpu_vgl_swap_us', 'gpu_vgl_swap_calls', 'gpu_gxm_finish_us', 'gpu_gxm_finish_calls', 'gpu_gxm_transfer_finish_us', 'gpu_gxm_transfer_finish_calls', 'gpu_gxm_notification_wait_us', 'gpu_gxm_notification_wait_calls', 'gpu_gxm_display_queue_add_us', 'gpu_gxm_display_queue_add_calls', 'gpu_gxm_display_queue_finish_us', 'gpu_gxm_display_queue_finish_calls', 'gpu_gxm_begin_scene_us', 'gpu_gxm_begin_scene_calls', 'gpu_gxm_end_scene_us', 'gpu_gxm_end_scene_calls', 'gpu_submit_scopes', 'gpu_clock_reads', 'gpu_clock_errors', 'gpu_probe_frames']
COLUMNS = BASE + [f'{p}_{f}' for p in PHASES for f in FIELDS] + PROBES + ['sim_blocks_us', 'probe_scopes'] + WORLD + GPU
PARALLEL = {'skin_worker0_service_us', 'skin_worker1_service_us'}
EXCLUSIVE = [name for name in PROBES if name not in PARALLEL]

def require(condition, message):
    if not condition:
        raise ValueError(message)

def read_window(root, probe_on):
    rows = {}
    files = sorted(root.glob('*light-*.csv'))
    require(files, f'{root}: no light CSVs')
    for path in files:
        with path.open(newline='') as stream:
            reader = csv.DictReader(stream)
            header = reader.fieldnames or []
            require(len(header) == len(set(header)), f'{path}: duplicate column names')
            require(header == COLUMNS, f'{path}: invalid schema; missing={sorted(set(COLUMNS)-set(header))}, extra={sorted(set(header)-set(COLUMNS))}, or wrong column order')
            for raw in reader:
                require(None not in raw and all(v is not None for v in raw.values()), f'{path}: malformed row')
                row = {k: int(v) for k, v in raw.items()}
                require(all(v >= 0 for v in row.values()), f'{path}: negative counter')
                frame = row['frame']
                if 61 <= frame <= 180:
                    require(frame not in rows, f'{root}: duplicate frame {frame}')
                    rows[frame] = row
    require(sorted(rows) == list(range(61,181)), f'{root}: missing/extra selected frames')
    values = [rows[f] for f in sorted(rows)]
    require(all(sum(r[k] for k in EXCLUSIVE) <= r['frame_cpu_us'] for r in values), f'{root}: exclusive sum exceeds CPU body')
    if probe_on:
        require(any(r['sim_blocks_us'] > 0 for r in values), f'{root}: probe never ran')
        require(all(r['gpu_probe_frames'] == 1 and r['gpu_clock_errors'] == 0 and r['gpu_clock_reads'] > 0 for r in values), f'{root}: GPU clocks inactive/invalid')
        require(all(r['gpu_vgl_swap_calls'] == 1 and r['gpu_submit_scopes'] > 0 for r in values), f'{root}: missing swap/submission coverage')
        require(any(r['gpu_gxm_begin_scene_calls'] > 0 and r['gpu_gxm_end_scene_calls'] > 0 for r in values), f'{root}: backend GXM hooks not observed')
    else:
        require(all(r[k] == 0 for r in values for k in PROBES + ['sim_blocks_us','probe_scopes'] + GPU), f'{root}: off probe counters nonzero')
    return files, values

def stats(values):
    s = sorted(values)
    return dict(n=len(s), median=statistics.median(s), p95=s[math.floor(.95*len(s))], max=max(s), mean=statistics.mean(s))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('console', type=Path)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    runs = []
    for name, pairs in [('gvillage',3),('potears3',1),('hatter1',1),('potears1',1),('tower2',1)]:
        for pair in range(1,pairs+1):
            arms = {}
            for arm in ['on','off']:
                files, rows = read_window(args.console/name/f'pair-{pair}'/arm, arm == 'on')
                arms[arm] = dict(raw_files=list(map(str,files)), selected_rows=rows,
                    columns={k:stats([r[k] for r in rows]) for k in COLUMNS if k != 'frame'})
            delta = {k:{q:arms['on']['columns'][k][q]-arms['off']['columns'][k][q] for q in ['median','p95','max','mean']} for k in ['wall_us','frame_cpu_us','swap_us']}
            runs.append(dict(map=name,pair=pair,arms=arms,probe_on_minus_off_us=delta))
    pooled = {}
    for name in sorted({r['map'] for r in runs}):
        pooled[name] = {}
        for arm in ['on','off']:
            rows = [v for r in runs if r['map'] == name for v in r['arms'][arm]['selected_rows']]
            pooled[name][arm] = {k:stats([r[k] for r in rows]) for k in COLUMNS if k != 'frame'}
    args.out.write_text(json.dumps(dict(runs=runs,pooled=pooled,stalls_removed=0,p95='sorted[floor(.95*N)]',console_audio_and_provenance_require_manual_log_review=True),indent=2)+'\n')
    print('PASS 14 launches / 1680 selected frames; audio and provenance review still required')

if __name__ == '__main__':
    main()
