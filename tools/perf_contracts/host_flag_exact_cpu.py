#!/usr/bin/env python3
"""Host oracle for the r26 CPU levers. Off against on, one key at a time or together.

Same comparison as host_flag_exact_gather.py, on the same probe output: every frame's skin rows,
submitted GPU packets in draw order (geometry, indices, uniforms, policy, texture), projection,
framebuffer hash, simulation state hash, light selection, world order, drops, final image, and a
strict pixel rule. Differences from that tool:

- `--lever` names the keys, `--frames` the length, `--still` drops the moving view and scripted
  hits, and `--sim` stages a vita_sim.json. A bubble field needs more than 180 frames to fill.
- joint_alpha_order is compared by source kind and depth bits. Its third field is a group index,
  and which index holds a batch is exactly what fx_attached_persist changes: a batch led by a raw
  quad takes a new group instead of the first publication's. What that group draws, and in what
  order, is compared through submitted_packets, which the renderer records as it draws.
- Removed-work counters are named per lever. Anything else that moves fails the case.

Elapsed time and the physical ring slot stay excluded, as in the parent tool.
usage: host_flag_exact_cpu.py OUTPUT --binary PATH --config CONFIG.json --lever KEY [--lever KEY] [--maps a b] [--frames N] [--still] [--sim F] [--negative NAME]
"""
from pathlib import Path
import argparse, hashlib, json, os, shutil, subprocess, time

DATA = Path('/Users/anezium/vita-work/alice-data/Alice1/bin/base')
DT = {'pandemonium': '0.04184', 'hedge3': '0.06'}
ATTEMPTS = 6
TIMING = {'cpu_ms', 'spans_ms'}
ELAPSED = ('_us', '_ns', '_ms')
HOST_MEMORY = ('actor_mem_', 'actor_publication_mem_')
NOISY_PIXELS = {'hedge3'}  # OFF against OFF differs here, see host_flag_exact_gather.py
PREREQ_FIELDS = ('requested_window', 'requested_high_dpi', 'screen_size_bits', 'framebuffer', 'scaled_target', 'viewport')
FUNNEL = {'fx_funnel_registered_raw', 'fx_funnel_registered_publication', 'fx_funnel_joint_raw_quads',
          'fx_funnel_joint_raw_outside_view', 'fx_funnel_joint_publication_quads', 'fx_funnel_joint_publication_outside_view'}
# Work a lever removes on purpose, or moves between the raw and publication routes. Draw counts,
# triangle counts, drops and the joint list length are not here: if those move, the case fails.
REMOVED_WORK = {
    # A shader with no light uniforms uploads fewer bytes; packets still carry the full payload. A removed clear moves nothing counted.
    'simple_dynamic_lights': {'vita_fx_uniform_bytes'},
    'display_clear_skip': set(),
    # Both change pixels on purpose. The oracle reports where; a clean PASS means the key did not reach that map.
    'fx_attached_sphere': set(),
    'fx_bubble_true_size': set(),
    'fx_attached_persist': FUNNEL | {
        'particle_direct_quads', 'particle_reused_quads', 'vita_world_puff_quads', 'vita_world_puff_batches',
        'vita_world_puff_batch_storage_bytes', 'vita_fx_merge_plan_hits', 'vita_fx_merge_plan_builds',
        'fx_merge_candidate_comparisons', 'fx_merge_destination_comparisons',
        # Group indices move, so ring slots and upload history do. Bytes are compared in the packets.
        'actor_index_updates', 'actor_uploaded_indices', 'actor_uploaded_vertices', 'actor_vertex_updates',
        'vita_fx_geometry_upload_bytes', 'vita_fx_index_uploads_skipped', 'vita_fx_mesh_uploads',
        'vita_fx_prepare_upload_hits', 'vita_fx_upload_cache_hits', 'vita_fx_vertex_uploads_skipped',
        'vita_fx_stream_capacity_bytes', 'vita_fx_buffer_growths', 'fx_index_updates', 'fx_uploaded_indices',
        'fx_vertex_updates', 'fx_uploaded_vertices', 'joint_center_group_scans', 'joint_center_group_vertices',
        # A set with no raw quads used to skip the bank and merge in merge_cached. As raw quads it
        # merges in the bank, with the same result. jlair2: 12 input groups become 2, 17 merged
        # meshes become 7, and the 26 packets and their bytes stay.
        'vita_fx_particle_input_groups', 'vita_fx_merged_meshes', 'fx_merge_copy_jobs', 'fx_merge_copy_jobs_le_4096',
        'fx_merge_inline_jobs', 'vita_fx_pack_indices', 'vita_fx_pack_vertices', 'vita_fx_certified_indices_reused',
        'fx_merge_local_topology_hits', 'fx_merge_local_vertex_hits', 'fx_merge_source_reusable_vertices',
        'vita_fx_unchanged_merged_groups', 'fx_merge_range_vertices_kept',
    },
    'joint_center_cache': set(),
    # Nothing counted moves: same GL calls, same applications. Only time goes away.
    'world_list_submit': set(),
    'mover_advance_reuse': {'mover_publications', 'mover_far_steps', 'collider_model_calls', 'collider_local_builds', 'dynamic_bucket_builds',
                            # Under --mover-replay the on arm rebuilds what it skipped, to check it. Without it, fewer.
                            'brush_hulls_built'},
    'world_static_merge': {
        'vita_world_gpu_draws', 'vita_world_low_draws', 'vita_world_texture_binds', 'vita_world_buffer_binds',
        'vita_world_binding_applications', 'vita_world_merged_draws', 'vita_world_uniform_bytes',
        'world_merge_pairs', 'world_merge_pairs_same_texture', 'world_merge_pairs_same_state',
        'world_merge_pairs_same_state_contiguous', 'world_merge_candidates',
    },
}


class Mismatch(AssertionError):
    pass


def sha_file(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for block in iter(lambda: f.read(1 << 20), b''):
            h.update(block)
    return h.hexdigest()


def tagged(path, tag):
    prefix = tag + ' '
    with open(path, errors='replace') as f:
        return [json.loads(line[len(prefix):]) for line in f if line.startswith(prefix)]


def environment(ctx, name, negative):
    env = {'HOME': os.environ['HOME'], 'USER': os.environ.get('USER', ''), 'LOGNAME': os.environ.get('LOGNAME', ''),
           'PATH': '/usr/bin:/bin', 'LANG': 'C', 'TMPDIR': os.environ.get('TMPDIR', '/tmp'),
           'LOOKING_GLASS_SIM_MAPS_PROBE': '1', 'LOOKING_GLASS_SIM_MAPS_DT': DT.get(name, '0.05'),
           'LOOKING_GLASS_SKIN_PROOF': '1'}
    if not ctx['still']:
        env.update(LOOKING_GLASS_SKIN_MOVING_VIEW='1', LOOKING_GLASS_SKIN_DEATHS='1')
    if negative:
        env['LOOKING_GLASS_GATHER_NEGATIVE'] = negative
    if ctx.get('gl_trace'):
        env['LOOKING_GLASS_GL_TRACE'] = '1'
    if ctx.get('world_packets'):
        env['LOOKING_GLASS_WORLD_PACKETS'] = '1'
    if ctx['replay']:
        env['LOOKING_GLASS_MOVER_REPLAY'] = '1'
    if ctx['ride']:
        env['LOOKING_GLASS_MOVER_RIDE'] = '1'
    if ctx['dt']:
        env['LOOKING_GLASS_SIM_MAPS_DT'] = ctx['dt']
    return env


def run_arm(ctx, name, on, arm_dir, negative):
    (arm_dir / 'private').mkdir(parents=True)
    (arm_dir / 'private/vita_renderer.json').write_text(json.dumps(dict(ctx['config'], **{k: on for k in ctx['levers']})))
    if ctx['sim']:
        shutil.copy(ctx['sim'], arm_dir / 'private/vita_sim.json')
    command = [str(ctx['binary']), '--data', str(DATA), '--no-audio', '--save-dir', str(arm_dir / 'private/saves'),
               '--map', name, '--frames', str(ctx['frames']), '--capture', str(arm_dir / 'final.png')]
    start = time.monotonic()
    with (arm_dir / 'run.log').open('x') as log:
        code = subprocess.run(command, cwd=arm_dir, env=environment(ctx, name, negative), stdout=log, stderr=subprocess.STDOUT, timeout=3600).returncode
    return dict(on=on, exit_code=code, seconds=round(time.monotonic() - start, 1), run_log_sha256=sha_file(arm_dir / 'run.log'))


def parse(ctx, arm_dir, name):
    log = arm_dir / 'run.log'
    frames = tagged(log, 'SIM_MAPS_FRAME')
    if len(frames) != ctx['frames']:
        raise Mismatch((name, 'frame count', len(frames)))
    images = tagged(log, 'SKIN_EXACT_IMAGE')
    presentation = tagged(log, 'SKIN_EXACT_PRESENTATION')
    if len(images) != ctx['frames'] or len(presentation) != ctx['frames']:
        raise Mismatch((name, 'image rows', len(images), len(presentation)))
    config = next((line for line in log.open(errors='replace') if '[lg/vita-renderer] config {' in line), None)
    if config is None:
        raise Mismatch((name, 'no config line'))
    printed = json.loads(config.split('[lg/vita-renderer] config ', 1)[1])
    return dict(frames=frames, images=images, presentation=presentation, printed=printed, final=sha_file(arm_dir / 'final.png'))


def prerequisite_differences(a, b):
    return [(i, k) for i, (x, y) in enumerate(zip(a['presentation'], b['presentation'])) for k in PREREQ_FIELDS if x.get(k) != y.get(k)]


def compare(ctx, name, off, on, strict_images):
    removed = set().union(*(REMOVED_WORK[k] for k in ctx['levers']))
    allow_group_ids = 'fx_attached_persist' in ctx['levers']
    image_frames = []
    changes, unexplained = {}, {}
    for i, (a, b) in enumerate(zip(off['frames'], on['frames'])):
        for key in sorted(set(a) | set(b)):
            if key in TIMING or key in ('submitted_packets', 'counters'):
                continue
            x, y = a.get(key), b.get(key)
            if key == 'joint_alpha_order' and allow_group_ids:
                x, y = [(e[0], e[2]) for e in x], [(e[0], e[2]) for e in y]
            if x != y:
                raise Mismatch((name, i, key))
        pa = [{k: v for k, v in p.items() if k != 'gpu_slot'} for p in a['submitted_packets']]
        pb = [{k: v for k, v in p.items() if k != 'gpu_slot'} for p in b['submitted_packets']]
        if pa != pb:
            first = next((j for j, (x, y) in enumerate(zip(pa, pb)) if x != y), min(len(pa), len(pb)))
            raise Mismatch((name, i, 'submitted packets', len(pa), len(pb), first))
        for key in sorted(set(a['counters']) | set(b['counters'])):
            if key.endswith(ELAPSED) or key.startswith(HOST_MEMORY):
                continue
            if a['counters'].get(key, 0) != b['counters'].get(key, 0):
                changes[key] = changes.get(key, 0) + b['counters'].get(key, 0) - a['counters'].get(key, 0)
                if key not in removed:
                    unexplained[key] = dict(frame=i, off=a['counters'].get(key, 0), on=b['counters'].get(key, 0))
        if off['images'][i] != on['images'][i]:
            if strict_images:
                raise Mismatch((name, i, 'framebuffer'))
            image_frames.append(i)
        if off['presentation'][i]['projection_bits'] != on['presentation'][i]['projection_bits']:
            raise Mismatch((name, i, 'projection'))
    if strict_images and off['final'] != on['final']:
        raise Mismatch((name, 'final image'))
    if unexplained:
        raise Mismatch((name, 'counters no lever explains, last frame of each', unexplained))
    last = off['frames'][-1]
    return dict(map=name, status='PASS', frames=ctx['frames'], packets=sum(len(f['submitted_packets']) for f in off['frames']),
                removed_work_change=changes, final_image_sha256=off['final'], images_strict=strict_images,
                image_mismatch_frames=image_frames, state_sha256_last=last['state_sha256'],
                funnel_last={k: v for k, v in on['frames'][-1]['counters'].items() if k.startswith('fx_funnel_') or k.startswith('mover_')},
                mover_totals_on={k: sum(f['counters'].get(k, 0) for f in on['frames']) for k in ('mover_group_steps', 'mover_far_steps', 'mover_support_steps', 'mover_blocked_steps', 'mover_publications', 'mover_ride_teleports', 'collider_model_calls')},
                mover_totals_off={k: sum(f['counters'].get(k, 0) for f in off['frames']) for k in ('mover_group_steps', 'mover_support_steps', 'mover_blocked_steps', 'mover_publications', 'collider_model_calls')},
                mover_replay_frames=sum(1 for f in off['frames'] if f.get('mover_replay')))


def run_case(ctx, name, negative=None):
    """Launch off and on until a pair shares the window prerequisites, as the parent tool does."""
    case = ctx['output'] / ('negative-' + negative if negative else 'host') / name
    if case.exists():
        shutil.rmtree(case)
    case.mkdir(parents=True)
    offs, ons, launches = [], [], []
    for attempt in range(ATTEMPTS):
        d = case / f'attempt-{attempt:02d}'
        d.mkdir()
        for on, runs in ((False, offs), (True, ons)):
            arm = d / ('arm-on' if on else 'arm-off')
            row = run_arm(ctx, name, on, arm, negative)
            launches.append(dict(row, attempt=attempt))
            if row['exit_code'] != 0:
                raise SystemExit(f'{name} attempt {attempt}: exit {row["exit_code"]}, see {arm}')
            parsed = parse(ctx, arm, name)
            for k in ctx['levers']:
                if parsed['printed'].get(k) is not on:
                    raise Mismatch((name, 'printed lever state', k, on, parsed['printed'].get(k)))
            runs.append(parsed)
        pair = next(((x, y) for x in offs for y in ons if not prerequisite_differences(x, y)), None)
        if pair is None:
            print('RETRY', name, 'attempt', attempt, flush=True)
            continue
        try:
            row = compare(ctx, name, pair[0], pair[1], name not in NOISY_PIXELS)
        except Mismatch as error:
            if not negative:
                raise
            row = dict(map=name, status='DETECTED', mismatch=repr(error.args)[:400])
        else:
            if negative:
                row = dict(map=name, status='NOT_DETECTED')
        row['launches'] = launches
        if not ctx['keep']:
            shutil.rmtree(case)
        return row
    raise SystemExit(f'{name}: no pair met the window prerequisites')


def main():
    p = argparse.ArgumentParser()
    p.add_argument('output', type=Path)
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--config', type=Path, required=True)
    p.add_argument('--lever', action='append', required=True)
    p.add_argument('--maps', nargs='*', required=True)
    p.add_argument('--frames', type=int, default=180)
    p.add_argument('--still', action='store_true')
    p.add_argument('--sim', type=Path)
    p.add_argument('--negative', action='append', default=[])
    p.add_argument('--keep-logs', action='store_true')
    p.add_argument('--mover-replay', action='store_true', help='record mover transforms, collider bits and collision answers per frame')
    p.add_argument('--ride', action='store_true', help='with --mover-replay, also stand the player on a mover every 24 frames')
    p.add_argument('--dt', help='override the fixed simulation step')
    p.add_argument('--gl-trace', action='store_true', help='compare a per-frame hash of every observed GL call, name, arguments and bytes read')
    p.add_argument('--world-packets', action='store_true', help='compare a per-frame hash of each world draw: group, resolved geometry, texture, uniforms, program')
    a = p.parse_args()
    for k in a.lever:
        assert k in REMOVED_WORK, k
    a.output.mkdir(parents=True, exist_ok=True)
    ctx = dict(output=a.output.resolve(), binary=a.binary.resolve(), config=json.loads(a.config.read_text()), levers=a.lever,
               frames=a.frames, still=a.still, sim=a.sim and a.sim.resolve(), keep=a.keep_logs, replay=a.mover_replay, ride=a.ride, dt=a.dt,
               gl_trace=a.gl_trace, world_packets=a.world_packets)
    rows = []
    for negative in a.negative:
        row = run_case(ctx, a.maps[0], negative)
        row['negative'] = negative
        rows.append(row)
        print(json.dumps({k: v for k, v in row.items() if k != 'launches'}), flush=True)
    if not a.negative:
        for name in a.maps:
            try:
                row = run_case(ctx, name)
            except Mismatch as error:
                row = dict(map=name, status='FAIL', mismatch=repr(error.args)[:1500])
            rows.append(row)
            print(json.dumps({k: v for k, v in row.items() if k != 'launches'}), flush=True)
    bindings = dict(binary=str(ctx['binary']), binary_sha256=sha_file(ctx['binary']), config_sha256=sha_file(a.config),
                    sim_sha256=a.sim and sha_file(a.sim), comparator_sha256=sha_file(Path(__file__).resolve()),
                    levers=a.lever, frames=a.frames, still=a.still, gl_trace=a.gl_trace, world_packets=a.world_packets)
    tag = ('negative-' if a.negative else '') + '-'.join(a.lever)
    (a.output / f'HOST-FLAG-{tag}.json').write_text(json.dumps(dict(bindings=bindings, rows=rows), indent=1) + '\n')
    bad = [r for r in rows if r['status'] in ('FAIL', 'NOT_DETECTED')]
    if bad:
        raise SystemExit(f'FAIL {[(r["map"], r["status"], r.get("mismatch")) for r in bad]}')
    print('ALL PASS', flush=True)


if __name__ == '__main__':
    main()
