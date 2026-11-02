#!/usr/bin/env python3
"""Host oracle for the two actor_gather levers: actor_publication_uv_reuse, prop_occurrence_reuse.

Runs one sim_maps_probe binary on real maps for 180 frames per arm. Only the lever keys differ
between arms; the config is the production console overlay. Every frame is compared: skin rows,
submitted GPU packets (geometry, indices, uniforms, policy, texture), projection, framebuffer
hash, simulation state hash, light selection, draw order, renderer drop counters, final image.
Only elapsed time and the physical ring slot are excluded. Counters are compared too, apart from
the lever witnesses and the two named below, whose change is the point of the lever.

Live UV audit behind actor_publication_uv_reuse (read from the source, 2026-11-02). Writers of a
skeletal live mesh after load: kernel()/shade_arrays and Store::consume write position, normal
and color; Puppet::draw_afterimage writes position and color; facial::Face::blink,
power_pose::PowerArt::skin and npc::magma_art write the texture; draw_pose takes and restores the
whole Mesh. Nothing writes .uv. Every `.uv =` in src/ is a copy (render_fx layers, electric,
power_pose effects), a weapon or loot mesh, world geometry or a test. The ON arm also compares
every reused UV in this binary and aborts on a stale one.

Negative controls (--negative) sabotage the lever through LOOKING_GLASS_GATHER_NEGATIVE. There
the comparison must fail: a pass would mean this oracle cannot see the fault.

usage: host_flag_exact_gather.py OUTPUT --binary PATH --config CONFIG.json [--maps a b] [--negative uv ...]
"""
from pathlib import Path
import argparse, hashlib, json, os, shutil, subprocess, time

DATA = Path('/Users/anezium/vita-work/alice-data/Alice1/bin/base')
CENSUS = ['skool2', 'utemple', 'wchess2', 'hatter1']
MORE = ['centipede1', 'pandemonium', 'hedge3', 'keep', 'grounds2', 'tower1', 'tower2', 'tower3', 'garden2',
        'centipede2', 'funhouse', 'jlair1', 'jlair2', 'grounds1', 'potears2', 'village']
DT = {'pandemonium': '0.04184', 'hedge3': '0.06'}
FRAMES = 180
ATTEMPTS = 4
LEVERS = ('actor_publication_uv_reuse', 'prop_occurrence_reuse')
ARMS = {'off': (False, False), 'on': (True, True), 'uv': (True, False), 'prop': (False, True)}
TIMING = {'cpu_ms', 'spans_ms'}
WITNESS = {'actor_publication_uv_reused_vertices', 'prop_occurrence_reused_vertices', 'prop_occurrence_copied_vertices'}
# Counts of work the levers remove on purpose. The produced geometry and every packet are
# compared separately. A Prop placement keeps its geometry version, so its layer copy, its merge
# copy and its upload stop. Any other counter that moves fails the case.
REMOVED_WORK = {
    'vita_actor_geometry_copies', 'vita_actor_geometry_copies_skipped',
    'vita_actor_layer_cache_builds', 'vita_actor_layer_cache_hits', 'vita_actor_batched_packets',
    'actor_index_updates', 'actor_uploaded_indices', 'actor_uploaded_vertices', 'actor_vertex_updates',
    'fx_merge_copy_jobs', 'fx_merge_copy_jobs_le_4096', 'fx_merge_local_topology_hits', 'fx_merge_local_vertex_hits',
    'fx_worker_pair_jobs', 'vita_fx_certified_indices_reused', 'vita_fx_geometry_upload_bytes',
    'vita_fx_index_uploads_skipped', 'vita_fx_mesh_uploads', 'vita_fx_pack_indices', 'vita_fx_pack_vertices',
    'vita_fx_prepare_upload_hits', 'vita_fx_unchanged_merged_groups', 'vita_fx_upload_cache_hits',
}
ELAPSED = ('_us', '_ns', '_ms')  # elapsed-time counters, same class as cpu_ms
PREREQ_FIELDS = ('requested_window', 'requested_high_dpi', 'screen_size_bits', 'framebuffer', 'scaled_target', 'viewport')
NEGATIVE = {'uv': ('skool2', 'on'), 'prop-pose': ('skool2', 'on'), 'prop-observe': ('skool2', 'on')}


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


def arm_environment(name, negative):
    env = {'HOME': os.environ['HOME'], 'USER': os.environ.get('USER', ''), 'LOGNAME': os.environ.get('LOGNAME', ''),
           'PATH': '/usr/bin:/bin', 'LANG': 'C', 'TMPDIR': os.environ.get('TMPDIR', '/tmp'),
           'LOOKING_GLASS_SIM_MAPS_PROBE': '1', 'LOOKING_GLASS_SIM_MAPS_DT': DT.get(name, '0.05'),
           'LOOKING_GLASS_SKIN_PROOF': '1', 'LOOKING_GLASS_SKIN_MOVING_VIEW': '1', 'LOOKING_GLASS_SKIN_DEATHS': '1'}
    if negative:
        env['LOOKING_GLASS_GATHER_NEGATIVE'] = negative
    return env


def run_arm(binary, config, name, arm, arm_dir, negative):
    (arm_dir / 'private').mkdir(parents=True)
    uv, prop = ARMS[arm]
    (arm_dir / 'private/vita_renderer.json').write_text(json.dumps(dict(config, actor_publication_uv_reuse=uv, prop_occurrence_reuse=prop)))
    command = [str(binary), '--data', str(DATA), '--no-audio', '--save-dir', str(arm_dir / 'private/saves'),
               '--map', name, '--frames', str(FRAMES), '--capture', str(arm_dir / 'final.png')]
    start = time.monotonic()
    with (arm_dir / 'run.log').open('x') as log:
        code = subprocess.run(command, cwd=arm_dir, env=arm_environment(name, negative), stdout=log, stderr=subprocess.STDOUT, timeout=1200).returncode
    return dict(arm=arm, exit_code=code, seconds=round(time.monotonic() - start, 1), run_log_sha256=sha_file(arm_dir / 'run.log'), run_log_bytes=(arm_dir / 'run.log').stat().st_size)


def parse(arm_dir, name):
    log = arm_dir / 'run.log'
    frames = tagged(log, 'SIM_MAPS_FRAME')
    if len(frames) != FRAMES:
        raise Mismatch((name, 'frame count', len(frames)))
    for i, f in enumerate(frames):
        if f['frame'] != i or f['map'] != name:
            raise Mismatch((name, i, 'frame identity', f['frame'], f['map']))
    images = tagged(log, 'SKIN_EXACT_IMAGE')
    presentation = tagged(log, 'SKIN_EXACT_PRESENTATION')
    if len(images) != FRAMES or len(presentation) != FRAMES:
        raise Mismatch((name, 'image rows', len(images), len(presentation)))
    config = next((line for line in log.open(errors='replace') if '[lg/vita-renderer] config {' in line), None)
    if config is None:
        raise Mismatch((name, 'no config line: the run may have fallen back to defaults'))
    printed = json.loads(config.split('[lg/vita-renderer] config ', 1)[1])
    return dict(frames=frames, images=images, presentation=presentation, printed=printed, final=sha_file(arm_dir / 'final.png'))


def prerequisite_differences(a, b):
    return [(i, k) for i, (x, y) in enumerate(zip(a['presentation'], b['presentation'])) for k in PREREQ_FIELDS if x.get(k) != y.get(k)]


def compare(name, off, on):
    counter_changes = {}
    for i, (a, b) in enumerate(zip(off['frames'], on['frames'])):
        for key in sorted(set(a) | set(b)):
            if key in TIMING or key in ('submitted_packets', 'counters'):
                continue
            if a.get(key) != b.get(key):
                raise Mismatch((name, i, key))
        pa = [{k: v for k, v in p.items() if k != 'gpu_slot'} for p in a['submitted_packets']]
        pb = [{k: v for k, v in p.items() if k != 'gpu_slot'} for p in b['submitted_packets']]
        if pa != pb:
            first = next((j for j, (x, y) in enumerate(zip(pa, pb)) if x != y), min(len(pa), len(pb)))
            raise Mismatch((name, i, 'submitted packets', len(pa), len(pb), first))
        keys = (set(a['counters']) | set(b['counters'])) - WITNESS
        for key in sorted(keys):
            if key.endswith(ELAPSED):
                continue
            if a['counters'].get(key, 0) != b['counters'].get(key, 0):
                if key not in REMOVED_WORK:
                    raise Mismatch((name, i, 'counter', key, a['counters'].get(key, 0), b['counters'].get(key, 0)))
                counter_changes[key] = counter_changes.get(key, 0) + b['counters'].get(key, 0) - a['counters'].get(key, 0)
        if off['images'][i] != on['images'][i]:
            raise Mismatch((name, i, 'framebuffer'))
        if off['presentation'][i]['projection_bits'] != on['presentation'][i]['projection_bits']:
            raise Mismatch((name, i, 'projection'))
    if off['final'] != on['final']:
        raise Mismatch((name, 'final image'))
    witness = {k: sum(f['counters'].get(k, 0) for f in on['frames']) for k in sorted(WITNESS)}
    return dict(map=name, status='PASS', frames=FRAMES, packets=sum(len(f['submitted_packets']) for f in off['frames']),
                skin_rows=sum(len(f['actor_skin']) for f in off['frames']), witness=witness,
                witness_mean_per_frame={k: round(v / FRAMES, 1) for k, v in witness.items()},
                removed_work_change=counter_changes, final_image_sha256=off['final'],
                state_sha256_frame_179=off['frames'][179]['state_sha256'])


def run_case(ctx, name, arm='on', negative=None):
    case = ctx['output'] / ('negative-' + negative if negative else 'host-' + arm) / name
    case.mkdir(parents=True, exist_ok=True)
    for attempt in range(ATTEMPTS):
        attempt_dir = case / f'attempt-{attempt:02d}'
        attempt_dir.mkdir(exist_ok=False)
        launches = [run_arm(ctx['binary'], ctx['config'], name, which, attempt_dir / f'arm-{which}', negative) for which in ('off', arm)]
        for row in launches:
            if row['exit_code'] != 0:
                raise SystemExit(f'{name} attempt {attempt}: arm {row["arm"]} exit {row["exit_code"]}, see {attempt_dir}')
        off, on = parse(attempt_dir / 'arm-off', name), parse(attempt_dir / f'arm-{arm}', name)
        for k, expected in zip(LEVERS, ARMS[arm]):
            if off['printed'].get(k) is not False or on['printed'].get(k) is not expected:
                raise Mismatch((name, 'printed lever state', k, off['printed'].get(k), on['printed'].get(k)))
        if prerequisite_differences(off, on):
            print('RETRY', name, 'attempt', attempt, 'window prerequisites differ', flush=True)
            continue
        try:
            row = compare(name, off, on)
        except Mismatch as error:
            if not negative:
                raise
            row = dict(map=name, status='DETECTED', mismatch=repr(error.args)[:400])
        else:
            if negative:
                row = dict(map=name, status='NOT_DETECTED', mismatch=None)
        row.update(attempt=attempt, launches=launches)
        if ctx['discard_logs']:
            shutil.rmtree(attempt_dir)
        return row
    raise SystemExit(f'{name}: no attempt met the window prerequisites')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('output', type=Path)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--maps', nargs='*')
    parser.add_argument('--arm', choices=['on', 'uv', 'prop'], default='on')
    parser.add_argument('--negative', nargs='*', default=[])
    parser.add_argument('--keep-logs', action='store_true')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    binary = args.binary.resolve()
    config = json.loads(args.config.read_text())
    ctx = dict(output=args.output.resolve(), binary=binary, config=config, discard_logs=not args.keep_logs)
    bindings = dict(binary=str(binary), binary_sha256=sha_file(binary), config_sha256=sha_file(args.config),
                    comparator_sha256=sha_file(Path(__file__).resolve()))
    rows = []
    for negative in args.negative:
        name, arm = NEGATIVE[negative]
        row = run_case(ctx, name, arm, negative)
        row['negative'] = negative
        rows.append(row)
        print(json.dumps({k: v for k, v in row.items() if k != 'launches'}), flush=True)
    maps = [] if args.negative and args.maps is None else (args.maps if args.maps is not None else CENSUS + MORE)
    for name in maps:
        row = run_case(ctx, name, args.arm)
        rows.append(row)
        print(json.dumps({k: v for k, v in row.items() if k not in ('launches',)}), flush=True)
    tag = 'negative' if args.negative and not maps else f'arm-{args.arm}'
    (args.output / f'HOST-FLAG-{tag}.json').write_text(json.dumps(dict(bindings=bindings, arm=args.arm, rows=rows), indent=1) + '\n')
    bad = [r for r in rows if r['status'] not in ('PASS', 'DETECTED')]
    if bad:
        raise SystemExit(f'FAIL {bad}')
    print('ALL PASS', flush=True)


if __name__ == '__main__':
    main()
