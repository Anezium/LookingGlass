"""Mac-only parameterized ON/OFF oracle for one renderer flag. No device transport.

It keeps every host_exact.py map comparison (tools/skin_exact_r9, left unchanged for r9
provenance) and toggles only the named flag. Prerequisites are written explicitly in both
arms. The environment is rebuilt without any LOOKING_GLASS_* variable, then only the probe
variables below are set; the flag reaches the non-test binary only through its JSON config.

Per map and arm it checks the printed effective config (flag and every prerequisite), that
no serial fallback was logged, 180 SIM_MAPS_FRAME rows, geometry == gpu_geometry for every
retained draw (actual GL buffer readback), and zero census serial fallbacks. Across arms it
compares actor_skin, state_sha256, selected_lights_bits, joint_alpha_order, blend_world_order,
uploaded packets including uniform hashes, the intermediate image witnesses and the final PNG,
plus the per-frame vita_fx_dropped_layers counter. With --baseline-binary it also runs a
reference binary that predates the flag (its config omits the flag key) and requires the
candidate OFF arm to equal it on the same fields, including gpu_slot, uniform hashes and every
reference counter except the five elapsed clocks named below.

A finished run directory is reused only when its receipt matches this oracle version, the
executable hash, the source manifest, the toolchain, the requested config, the reset
environment and the log/census/image hashes. A mismatch allocates a new directory and leaves
the old one in place.

Witness: --witness serial_vertices_drop requires the census serial vertices (measured rows
60..179) to drop in the ON arm on at least one map, and on every --witness-map. A run with no
drop anywhere is FAIL (vacuous).
"""
from pathlib import Path
import argparse, hashlib, json, os, subprocess, sys

ORACLE_VERSION = 2
ELAPSED = (
    'skin_worker_service_us',
    'skin_collect_wait_us',
    'skin_gather_us',
    'fx_worker_wait_us',
    'fx_worker_max_service_us',
)
# Exclusive ends. Must match src/render_fx/retained.rs UNIFORM_FIELDS.
UNIFORM_FIELDS = (
    ('Model', 16), ('Projection', 32), ('AtlasGrid', 34), ('Fullbright', 35), ('Lit', 36),
    ('ClampUV', 37), ('Cutout', 38), ('CutoutReference', 39), ('Gain', 43), ('FogMode', 44),
    ('SkySize', 46), ('SkyFade', 47), ('FxModel', 48), ('FxAppearance', 52), ('Lights', 116),
    ('FogEye', 119), ('DistanceFog', 123), ('LiquidFog', 127), ('FogAMin', 130), ('FogAMax', 133),
    ('FogA', 137), ('FogBMin', 140), ('FogBMax', 143), ('FogB', 147),
)
DT = {'pandemonium': '0.04184', 'hedge3': '0.06'}

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def probe_env(base_env, map_name, uniform_bits):
    env = {k: v for k, v in base_env.items() if not k.startswith('LOOKING_GLASS_')}
    env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1', LOOKING_GLASS_SIM_MAPS_DT=DT.get(map_name, '0.05'),
               LOOKING_GLASS_SIM_MAPS_COUNTERS='1', LOOKING_GLASS_SKIN_PROOF='1',
               LOOKING_GLASS_SKIN_MOVING_VIEW='1', LOOKING_GLASS_SKIN_DEATHS='1')
    if uniform_bits:
        env['LOOKING_GLASS_SIM_MAPS_UNIFORM_BITS'] = '1'
    return env

def receipt_core(identity, config, env, argv_tail):
    return dict(oracle_version=ORACLE_VERSION, executable_path=identity.get('executable_path'),
                executable_sha256=identity['executable_sha256'],
                source_revision=identity['source_revision'], source_manifest_sha256=identity['source_manifest_sha256'],
                toolchain=identity['toolchain'], features=identity['features'], rustflags=identity['rustflags'],
                requested_config=config, environment=env, argv_tail=argv_tail)

def receipt_matches(saved, expected):
    for key, value in expected.items():
        if saved.get(key) != value:
            return False
    if saved.get('exit_code') != 0:
        return False
    return True

def files_match_receipt(directory, census_name):
    receipt_path = directory / 'run-receipt.json'
    if not receipt_path.is_file():
        return False
    saved = json.loads(receipt_path.read_text())
    needed = {
        'run.log': saved.get('log_sha256'),
        census_name: saved.get('census_sha256'),
        'final.png': saved.get('final_png_sha256'),
        'private/vita_renderer.json': saved.get('config_sha256'),
    }
    for name, digest in needed.items():
        path = directory / name
        if not digest or not path.is_file() or sha(path) != digest:
            return False
    return True

def allocate_run_dir(root, map_name, tag, expected):
    """Return (directory, reuse). Never deletes or rewrites an existing directory."""
    digest = hashlib.sha256(json.dumps(expected, sort_keys=True).encode()).hexdigest()[:20]
    base = root / 'host' / f'{map_name}-{tag}-{digest}'
    if not base.exists():
        return base, False
    saved_path = base / 'run-receipt.json'
    if saved_path.is_file():
        saved = json.loads(saved_path.read_text())
        if receipt_matches(saved, expected) and files_match_receipt(base, f'{map_name}-census-180.json'):
            return base, True
    n = 2
    while True:
        sibling = root / 'host' / f'{map_name}-{tag}-{digest}-r{n}'
        if not sibling.exists():
            return sibling, False
        n += 1

def divergent_uniform_fields(left, right):
    if len(left) != len(right):
        return ['length']
    start = 0
    names = []
    for name, end in UNIFORM_FIELDS:
        if left[start:end] != right[start:end]:
            names.append(name)
        start = end
    if start != len(left):
        names.append('tail')
    return names

def load_uniform_bits(directory):
    path = directory / 'uniform-bits.jsonl'
    if not path.is_file():
        return {}
    rows = {}
    for line in path.read_text().splitlines():
        if not line:
            continue
        row = json.loads(line)
        rows[(row['frame'], row['index'])] = row['bits']
    return rows

def startup_viewport(directory):
    """The GL viewport the binary itself logged before the first frame. Part of the
    environment: the same binary on a different startup viewport projects differently."""
    import re
    pattern = re.compile(r'viewport=(\[[^\]]*\])')
    for line in (directory / 'run.log').read_text().splitlines():
        if 'stage=prepare-3d-returned' in line:
            match = pattern.search(line)
            if match:
                return match.group(1)
    return None

def main(argv=None):
    p = argparse.ArgumentParser()
    p.add_argument('output', type=Path)
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--binary-identity', type=Path, required=True)
    p.add_argument('--baseline-binary', type=Path)
    p.add_argument('--baseline-identity', type=Path)
    p.add_argument('--flag', required=True)
    p.add_argument('--prereq', default='{}', help='JSON object written in every arm')
    p.add_argument('--maps', nargs='+', required=True)
    p.add_argument('--witness', choices=['serial_vertices_drop'], required=True)
    p.add_argument('--witness-map', action='append', default=[])
    p.add_argument('--uniform-bits', action='store_true')
    p.add_argument('--uniform-bits-map', action='append', default=[],
                   help='Only write per-packet uniform bits for these maps (repeatable)')
    p.add_argument('--viewport-attempts', type=int, default=8,
                   help='Maximum matched-viewport attempts per map')
    p.add_argument('--repeat', type=int, default=1,
                   help='Runs per arm on --repeat-map, to show the same environment is repeatable')
    p.add_argument('--repeat-map', action='append', default=[])
    p.add_argument('--data', default='/Users/anezium/vita-work/alice-data/Alice1/bin/base')
    args = p.parse_args(argv)
    if args.baseline_binary and not args.baseline_identity:
        p.error('--baseline-identity is required with --baseline-binary')
    repo = Path(__file__).resolve().parents[2]
    output = args.output.resolve()
    (output / 'proof').mkdir(parents=True, exist_ok=True)
    base = json.loads((repo / 'tools/lever_matrix_r4/configs/PL-baseline.json').read_text())
    prereq = json.loads(args.prereq)
    assert args.flag not in prereq
    identity = json.loads(args.binary_identity.read_text())
    assert sha(args.binary) == identity['executable_sha256']
    baseline_identity = json.loads(args.baseline_identity.read_text()) if args.baseline_identity else None
    if baseline_identity:
        assert sha(args.baseline_binary) == baseline_identity['executable_sha256']
    result = dict(oracle_version=ORACLE_VERSION, flag=args.flag, prerequisites=prereq,
                  base_config=str(repo / 'tools/lever_matrix_r4/configs/PL-baseline.json'),
                  base_config_sha256=sha(repo / 'tools/lever_matrix_r4/configs/PL-baseline.json'),
                  binary=str(args.binary), binary_sha256=identity['executable_sha256'],
                  binary_identity=identity,
                  baseline_binary=str(args.baseline_binary) if args.baseline_binary else None,
                  baseline_binary_sha256=baseline_identity['executable_sha256'] if baseline_identity else None,
                  baseline_identity=baseline_identity,
                  gpu_observer='glGetBufferSubData on the bound vertex attribute and element buffers before every retained draw',
                  elapsed_counters_not_compared=list(ELAPSED),
                  uniform_fields=[list(item) for item in UNIFORM_FIELDS],
                  uniform_bits_maps=sorted(map_name for map_name in args.maps
                                           if args.uniform_bits and (not args.uniform_bits_map or map_name in args.uniform_bits_map)),
                  maps={}, status='INCOMPLETE')
    out_json = output / 'proof' / f'HOST-FLAG-EXACT-{args.flag}.json'

    def write():
        out_json.write_text(json.dumps(result, indent=1) + '\n')

    def run(map_name, tag, binary, config, run_identity, attempt=0, uniform_bits=False, rep=0):
        env = probe_env(os.environ, map_name, uniform_bits)
        argv_tail = ['--data', args.data, '--no-audio', '--map', map_name, '--frames', '180']
        expected = receipt_core(run_identity, config, {k: env[k] for k in sorted(env) if k.startswith('LOOKING_GLASS_')}, argv_tail)
        expected['attempt'] = attempt
        expected['repeat'] = rep
        if rep not in (0, None):
            tag = f'{tag}-r{rep}'
        directory, reuse = allocate_run_dir(output, map_name, tag, expected)
        if not reuse:
            (directory / 'private').mkdir(parents=True, exist_ok=False)
            (directory / 'private/vita_renderer.json').write_text(json.dumps(config))
            cmd = ['/usr/bin/time', '-l', '-o', str(directory / 'rusage.txt'), str(binary), *argv_tail[:3],
                   '--save-dir', str(directory / 'private/saves'), '--map', map_name, '--frames', '180',
                   '--capture', str(directory / 'final.png')]
            with (directory / 'run.log').open('w') as handle:
                proc = subprocess.run(cmd, cwd=directory, env=env, stdout=handle, stderr=subprocess.STDOUT, timeout=1800)
            receipt = dict(expected, exit_code=proc.returncode, argv=cmd, run_dir=str(directory),
                           log_sha256=sha(directory / 'run.log'),
                           config_sha256=sha(directory / 'private/vita_renderer.json'),
                           final_png_sha256=sha(directory / 'final.png') if (directory / 'final.png').is_file() else None,
                           census_sha256=sha(directory / f'{map_name}-census-180.json') if (directory / f'{map_name}-census-180.json').is_file() else None)
            if (directory / 'uniform-bits.jsonl').is_file():
                receipt['uniform_bits_sha256'] = sha(directory / 'uniform-bits.jsonl')
            (directory / 'run-receipt.json').write_text(json.dumps(receipt, indent=1) + '\n')
            assert proc.returncode == 0, (map_name, tag, proc.returncode, str(directory / 'run.log'))
        lines = (directory / 'run.log').read_text().splitlines()
        configs = [json.loads(line.split('[lg/vita-renderer] config ', 1)[1]) for line in lines if line.startswith('[lg/vita-renderer] config ')]
        assert len(configs) == 1, (map_name, tag, 'one effective config record')
        effective = configs[0]
        for key, value in prereq.items():
            assert effective[key] == value, (map_name, tag, key, effective[key], value)
        if args.flag in config:
            assert effective[args.flag] is config[args.flag], (map_name, tag, args.flag)
        else:
            assert args.flag not in effective, (map_name, tag, 'reference binary must predate the flag')
        assert '[lg/vita-renderer] actor_single_prepare active=true' in lines, (map_name, tag, 'actor_single_prepare inactive')
        fallback = [line for line in lines if '[lg/skin] serial fallback' in line or 'afterimage consumer must be declared' in line]
        assert not fallback, (map_name, tag, fallback[:3])
        frames = [json.loads(line.split(' ', 1)[1]) for line in lines if line.startswith('SIM_MAPS_FRAME ')]
        images = [json.loads(line.split(' ', 1)[1]) for line in lines if line.startswith('SKIN_EXACT_IMAGE ')]
        eligibility = [json.loads(line.split(' ', 1)[1]) for line in lines if line.startswith('ACTOR_LAYER_ELIGIBILITY ')]
        assert len(frames) == 180, (map_name, tag, len(frames))
        assert all(all(packet['geometry'] == packet['gpu_geometry'] for packet in frame['submitted_packets']) for frame in frames), (map_name, tag, 'GPU readback')
        census = json.loads((directory / f'{map_name}-census-180.json').read_text())
        assert census['serial_fallbacks'] == 0, (map_name, tag)
        rows = census['rows'][60:]
        rss = [line.split()[0] for line in (directory / 'rusage.txt').read_text().splitlines() if 'maximum resident set size' in line]
        counters = [frame['counters'] for frame in frames]
        measured = counters[60:]
        def mean(name):
            return sum(counter.get(name, 0) for counter in measured) / len(measured)
        def peak(name):
            return max(counter.get(name, 0) for counter in counters)
        actor_keys = sorted({key for counter in counters for key in counter if key.startswith('actor_')})
        row = dict(map=map_name, arm=tag, run_dir=str(directory), reused=reuse, attempt=attempt,
                   viewport=startup_viewport(directory),
                   config_sha256=sha(directory / 'private/vita_renderer.json'), log_sha256=sha(directory / 'run.log'),
                   receipt_sha256=sha(directory / 'run-receipt.json'),
                   serial_vertices=sum(actor['serial_vertices'] for row in rows for actor in row['actors']) / 120,
                   worker_vertices=sum(actor['worker_vertices'] for row in rows for actor in row['actors']) / 120,
                   final_png_sha256=sha(directory / 'final.png'),
                   gpu_draw_readbacks=sum(len(frame['submitted_packets']) for frame in frames),
                   max_rss_bytes=int(rss[0]) if rss else None,
                   counters_measured_mean={key: mean(key) for key in sorted({key for counter in measured for key in counter})},
                   counters_peak={key: peak(key) for key in actor_keys},
                   eligibility=eligibility[-1] if eligibility else None)
        print('DONE', map_name, tag, 'serial', row['serial_vertices'], 'worker', row['worker_vertices'], 'draws', row['gpu_draw_readbacks'], 'reused', reuse, flush=True)
        return row, frames, images

    def packets(frame, keep_slot, keep_uniforms=True):
        drop = set() if keep_slot else {'gpu_slot'}
        if not keep_uniforms:
            drop.add('uniforms')
        return [{key: value for key, value in packet.items() if key not in drop} for packet in frame['submitted_packets']]

    def compare(map_name, left, right, label, full, bits):
        (left_row, left_frames, left_images), (right_row, right_frames, right_images) = left, right
        uniform_diffs = []
        for index, (x, y) in enumerate(zip(left_frames, right_frames)):
            for key in ['actor_skin', 'state_sha256', 'selected_lights_bits', 'joint_alpha_order', 'blend_world_order']:
                assert x[key] == y[key], (map_name, label, index, key)
            if bits:
                assert packets(x, False, False) == packets(y, False, False), (map_name, label, index, 'uploaded GPU packets')
            else:
                assert packets(x, False, True) == packets(y, False, True), (map_name, label, index, 'uploaded GPU packets')
            assert x['counters'].get('vita_fx_dropped_layers', 0) == y['counters'].get('vita_fx_dropped_layers', 0), (map_name, label, index, 'dropped layers')
            if full:
                assert packets(x, True, not bits) == packets(y, True, not bits), (map_name, label, index, 'packets including gpu_slot')
                assert all(y['counters'].get(key) == value for key, value in x['counters'].items() if key not in ELAPSED), (map_name, label, index, 'reference counters')
        assert len(left_frames) == len(right_frames) == 180
        assert left_images == right_images, (map_name, label, 'intermediate pixels')
        assert left_row['final_png_sha256'] == right_row['final_png_sha256'], (map_name, label, 'final image')
        if bits:
            bits_l = load_uniform_bits(Path(left_row['run_dir']))
            bits_r = load_uniform_bits(Path(right_row['run_dir']))
            expected_l = sum(len(frame['submitted_packets']) for frame in left_frames)
            expected_r = sum(len(frame['submitted_packets']) for frame in right_frames)
            assert len(bits_l) == expected_l and len(bits_r) == expected_r, (map_name, label, 'uniform bit sidecar count', len(bits_l), expected_l, len(bits_r), expected_r)
            for key in sorted(set(bits_l) | set(bits_r)):
                names = divergent_uniform_fields(bits_l.get(key, []), bits_r.get(key, []))
                if names:
                    uniform_diffs.append(dict(frame=key[0], index=key[1], fields=names))
            if uniform_diffs:
                (output / 'proof').mkdir(parents=True, exist_ok=True)
                record = dict(map=map_name, label=label, oracle_version=ORACLE_VERSION,
                              left_binary=(result['baseline_binary_sha256'] if label == 'baseline-vs-off' else result['binary_sha256']),
                              right_binary=result['binary_sha256'],
                              packets_per_arm={left_row['arm']: len(bits_l), right_row['arm']: len(bits_r)},
                              divergent=uniform_diffs)
                (output / 'proof' / f'uniform-fields-{map_name}-{label}.json').write_text(json.dumps(record, indent=1) + '\n')
                result.setdefault('uniform_field_diffs', {})[f'{map_name}-{label}'] = len(uniform_diffs)
            assert not uniform_diffs, (map_name, label, 'uniform f32 bits', uniform_diffs[:8])
        print('PASS', map_name, label, flush=True)

    for map_name in args.maps:
        wants_bits = args.uniform_bits and (not args.uniform_bits_map or map_name in args.uniform_bits_map)
        reps = args.repeat if (not args.repeat_map or map_name in args.repeat_map) else 1
        entry = result['maps'][map_name] = {}
        attempts = []
        selected = None
        for attempt in range(max(1, args.viewport_attempts)):
            off = [run(map_name, 'off', args.binary, dict(base, **prereq, **{args.flag: False}), identity, attempt, wants_bits, rep)
                   for rep in range(reps)]
            on = [run(map_name, 'on', args.binary, dict(base, **prereq, **{args.flag: True}), identity, attempt, wants_bits, rep)
                  for rep in range(reps)]
            ref = [run(map_name, 'baseline', args.baseline_binary, dict(base, **prereq), baseline_identity, attempt, wants_bits, rep)
                   for rep in range(reps)] if args.baseline_binary else []
            views = {}
            for arm, runs in (('off', off), ('on', on), ('baseline', ref)):
                for rep, result_run in enumerate(runs):
                    views[f'{arm}-r{rep}'] = result_run[0]['viewport']
            attempts.append(dict(attempt=attempt, viewports=views,
                                 run_dirs={name: result_run[0]['run_dir'] for name, result_run in
                                           [(f'off-r{rep}', r) for rep, r in enumerate(off)]
                                           + [(f'on-r{rep}', r) for rep, r in enumerate(on)]
                                           + [(f'baseline-r{rep}', r) for rep, r in enumerate(ref)]}))
            if None not in views.values() and len(set(views.values())) == 1:
                selected = (off, on, ref)
                break
            print('RETRY', map_name, 'startup viewport differs across arms', views, flush=True)
        entry['viewport_attempts'] = attempts
        if selected is None:
            result['status'] = f'FAIL ({map_name}: arms never shared one startup viewport)'
            write(); sys.exit(1)
        off, on, ref = selected
        entry['viewport'] = off[0][0]['viewport']
        entry['attempt'] = off[0][0]['attempt']
        entry['off'], entry['on'] = off[0][0], on[0][0]
        if ref:
            entry['baseline'] = ref[0][0]
        for arm, runs in (('off', off), ('on', on), ('baseline', ref)):
            for rep in range(1, len(runs)):
                compare(map_name, runs[0], runs[rep], f'{arm}-repeat', True, wants_bits)
        compare(map_name, off[0], on[0], 'off-vs-on', False, wants_bits)
        entry['off_vs_on'] = 'PASS'
        if ref:
            compare(map_name, ref[0], off[0], 'baseline-vs-off', True, wants_bits)
            entry['baseline_vs_off'] = 'PASS'
        entry['witness_serial_vertex_drop'] = off[0][0]['serial_vertices'] - on[0][0]['serial_vertices']
        write()

    drops = {map_name: entry['witness_serial_vertex_drop'] for map_name, entry in result['maps'].items()}
    result['witness'] = dict(kind=args.witness, serial_vertex_drop_per_map=drops, required_maps=args.witness_map)
    inventory = {map_name: entry.get('on', {}).get('eligibility') for map_name, entry in result['maps'].items()}
    missing_inventory = [map_name for map_name, record in inventory.items() if not record]
    if missing_inventory:
        result['status'] = f'FAIL (no stage/alpha eligibility inventory {missing_inventory})'
        write(); sys.exit(1)
    empty_inventory = [map_name for map_name, record in inventory.items() if not record.get('surfaces')]
    result['eligibility_inventory'] = dict(
        maps=len(inventory), surfaces={map_name: len(record.get('surfaces') or []) for map_name, record in inventory.items()},
        environment_actors={map_name: record.get('environment_actors', 0) for map_name, record in inventory.items()},
        maps_without_actor_surfaces=empty_inventory,
        environment_surfaces={map_name: sum(1 for surface in (record.get('surfaces') or []) if surface.get('environment')) for map_name, record in inventory.items()})
    environment_maps = sorted(map_name for map_name, record in inventory.items() if record.get('environment_actors', 0) > 0)
    result['environment_actor_maps'] = environment_maps
    covered_environment = [surface for record in inventory.values() for surface in (record.get('surfaces') or []) if surface.get('environment')]
    result['environment_surface_classes'] = sorted({(surface.get('reason'), surface.get('classification')) for surface in covered_environment})
    if environment_maps != sorted(map_name for map_name in args.maps if inventory[map_name].get('environment_actors', 0) > 0):
        result['status'] = 'FAIL (environment-actor map list disagrees with the run set)'
        write(); sys.exit(1)
    if not covered_environment:
        result['status'] = 'FAIL (no environment-mapped actor surface in the inventory)'
        write(); sys.exit(1)
    if not any(value > 0 for value in drops.values()):
        result['status'] = 'FAIL (vacuous: no map moved serial vertices to the workers)'
        write(); sys.exit(1)
    missing = [map_name for map_name in args.witness_map if not drops.get(map_name, 0) > 0]
    if missing:
        result['status'] = f'FAIL (no witness on required maps {missing})'
        write(); sys.exit(1)
    result['status'] = 'PASS'
    write()
    print('PASS', json.dumps(drops), flush=True)

if __name__ == '__main__':
    main()
