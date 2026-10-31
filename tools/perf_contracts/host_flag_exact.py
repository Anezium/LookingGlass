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

ORACLE_VERSION = 3
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
UNIFORM_WIDTH = UNIFORM_FIELDS[-1][1]
DT = {'pandemonium': '0.04184', 'hedge3': '0.06'}

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def oracle_sha256():
    return sha(Path(__file__).resolve())

def probe_env(base_env, map_name, uniform_bits):
    env = {k: v for k, v in base_env.items() if not k.startswith('LOOKING_GLASS_')}
    env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1', LOOKING_GLASS_SIM_MAPS_DT=DT.get(map_name, '0.05'),
               LOOKING_GLASS_SIM_MAPS_COUNTERS='1', LOOKING_GLASS_SKIN_PROOF='1',
               LOOKING_GLASS_SKIN_MOVING_VIEW='1', LOOKING_GLASS_SKIN_DEATHS='1')
    if uniform_bits:
        env['LOOKING_GLASS_SIM_MAPS_UNIFORM_BITS'] = '1'
    return env

def receipt_core(identity, config, env, argv_tail):
    return dict(oracle_version=ORACLE_VERSION, oracle_sha256=oracle_sha256(),
                executable_path=identity.get('executable_path'),
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
    """A reusable directory is bound to every file it records, including the optional
    uniform-bit sidecar: a recorded or requested sidecar must exist and hash-match, and an
    unrecorded sidecar makes the directory non-reusable (it would be an unbound artifact).
    The effective config is covered by the log hash and its own recorded digest."""
    receipt_path = directory / 'run-receipt.json'
    if not receipt_path.is_file():
        return False
    saved = json.loads(receipt_path.read_text())
    if saved.get('oracle_version') != ORACLE_VERSION:
        return False
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
    if not saved.get('effective_config_sha256'):
        return False
    sidecar = directory / 'uniform-bits.jsonl'
    recorded = saved.get('uniform_bits_sha256')
    if saved.get('uniform_bits_expected'):
        if not recorded or not sidecar.is_file() or sha(sidecar) != recorded:
            return False
    elif recorded:
        if not sidecar.is_file() or sha(sidecar) != recorded:
            return False
    elif sidecar.is_file():
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

def bits_sha256(bits):
    return hashlib.sha256(b''.join(int(value).to_bytes(4, 'little', signed=False) for value in bits)).hexdigest()

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

def uniform_bits_issues(directory, frames):
    """Every recorded sidecar row must be exactly 147 little-endian u32 values with unique
    (frame,index) coverage, and its SHA-256 must equal that packet's actual aggregate hash."""
    path = Path(directory) / 'uniform-bits.jsonl'
    if not path.is_file():
        return ['missing uniform-bits.jsonl']
    issues = []
    rows = {}
    for number, line in enumerate(path.read_text().splitlines(), 1):
        if not line:
            continue
        row = json.loads(line)
        key = (row.get('frame'), row.get('index'))
        if key in rows:
            issues.append(f'duplicate coverage {key} line {number}')
        rows[key] = row.get('bits')
    expected = {}
    for frame in frames:
        for index, packet in enumerate(frame['submitted_packets']):
            expected[(frame['frame'], index)] = packet['uniforms']
    for key, bits in rows.items():
        if not isinstance(bits, list) or len(bits) != UNIFORM_WIDTH:
            issues.append(f'width {key}')
            continue
        if any(not isinstance(value, int) or value < 0 or value > 0xffffffff for value in bits):
            issues.append(f'range {key}')
            continue
        if bits_sha256(bits) != expected.get(key):
            issues.append(f'hash linkage {key}')
    missing = sorted(set(expected) - set(rows))
    extra = sorted(set(rows) - set(expected))
    if missing:
        issues.append(f'missing coverage {len(missing)} first {missing[:4]}')
    if extra:
        issues.append(f'extra coverage {len(extra)} first {extra[:4]}')
    return issues

def startup_viewports(directory):
    """All prepare-3d-returned viewports the binary logged. More than one distinct value
    means the scaled 3D viewport changed during the run and must not be selected."""
    import re
    pattern = re.compile(r'viewport=(\[[^\]]*\])')
    views = []
    for line in (directory / 'run.log').read_text().splitlines():
        if 'stage=prepare-3d-returned' in line:
            match = pattern.search(line)
            if match:
                views.append(match.group(1))
    return views

def startup_viewport(directory):
    """The GL viewport the binary itself logged before the first frame. Part of the
    environment: the same binary on a different startup viewport projects differently.
    A conflicting later prepare-3d-returned viewport yields None instead of the first one."""
    views = startup_viewports(directory)
    return views[0] if views and len(set(views)) == 1 else None

def viewport_issues(frames, expected):
    """Exact per-frame equality. Kept for controls that do not model the startup settle."""
    issues = []
    for frame in frames:
        value = frame.get('world_viewport')
        if not isinstance(value, list) or len(value) != 4 or any(not isinstance(part, int) for part in value):
            issues.append(f"frame {frame.get('frame')}: missing world_viewport {value!r}")
        elif expected is not None and list(value) != list(expected):
            issues.append(f"frame {frame.get('frame')}: viewport {value} != selected {expected}")
    return issues

def viewport_trajectory(frames, startup=None):
    """Gate the recorded per-frame 3D viewport trajectory.

    The macOS host reports the requested drawable on frame 0 and the settled drawable from
    frame 1; the only permitted change is that single startup settle (frame 0 -> 1). Every
    frame is still recorded and checked: frame 0 must equal the `prepare-3d-returned`
    diagnostic, frames 1..179 must be one constant value, and any change at frame >= 2, any
    missing sample or any oscillation is rejected. Returns (steady, issues)."""
    values = []
    for frame in frames:
        value = frame.get('world_viewport')
        if not isinstance(value, list) or len(value) != 4 or any(not isinstance(part, int) for part in value):
            return None, [f"frame {frame.get('frame')}: missing world_viewport {value!r}"]
        values.append(list(value))
    if not values:
        return None, ['no frames']
    issues = []
    if startup is not None and list(startup) != values[0]:
        issues.append(f'startup diagnostic {startup} != frame 0 viewport {values[0]}')
    steady = values[-1]
    start = len(values) - 1
    while start > 0 and values[start - 1] == steady:
        start -= 1
    if start > 1:
        issues.append(f'late viewport change at frame {start}: {values[start - 1]} -> {steady}')
    return steady, issues

def startup_transition_frame(frames):
    for index in range(1, len(frames)):
        if frames[index].get('world_viewport') != frames[index - 1].get('world_viewport'):
            return index
    return None

def packets(frame, keep_slot, keep_uniforms):
    drop = set() if keep_slot else {'gpu_slot'}
    if not keep_uniforms:
        drop.add('uniforms')
    return [{key: value for key, value in packet.items() if key not in drop} for packet in frame['submitted_packets']]

def packet_differences(left_frames, right_frames, keep_slot=True, keep_uniforms=True):
    """Frame indices whose submitted packets differ. keep_uniforms defaults to True so the
    aggregate uniform hash is always part of every packet comparison."""
    if len(left_frames) != len(right_frames):
        return ['frame-count', len(left_frames), len(right_frames)]
    return [index for index, (left, right) in enumerate(zip(left_frames, right_frames))
            if packets(left, keep_slot, keep_uniforms) != packets(right, keep_slot, keep_uniforms)]

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
                  world_viewport_stage='per frame at prepare-3d-returned (scaled 3D target); HUD/default framebuffer stages excluded',
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
        expected['uniform_bits_expected'] = uniform_bits
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
        viewport = startup_viewport(directory)
        assert viewport is not None, (map_name, tag, 'missing or conflicting prepare-3d-returned viewport', startup_viewports(directory))
        steady, issues = viewport_trajectory(frames, json.loads(viewport))
        assert not issues, (map_name, tag, 'per-frame 3D viewport', issues[:4])
        if uniform_bits:
            assert (directory / 'uniform-bits.jsonl').is_file(), (map_name, tag, 'requested uniform bits missing')
            issues = uniform_bits_issues(directory, frames)
            assert not issues, (map_name, tag, 'uniform bits sidecar', issues[:6])
        else:
            assert not (directory / 'uniform-bits.jsonl').is_file(), (map_name, tag, 'unrequested uniform bits sidecar')
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
        sidecar = directory / 'uniform-bits.jsonl'
        row = dict(map=map_name, arm=tag, run_dir=str(directory), reused=reuse, attempt=attempt,
                   viewport=json.dumps(steady), startup_viewport=viewport,
                   viewport_key=json.dumps([json.loads(viewport), steady]),
                   viewport_transition_frame=startup_transition_frame(frames),
                   config_sha256=sha(directory / 'private/vita_renderer.json'), log_sha256=sha(directory / 'run.log'),
                   receipt_sha256=(sha(directory / 'run-receipt.json') if reuse else None),
                   effective_config_sha256=hashlib.sha256(json.dumps(effective, sort_keys=True, separators=(',', ':')).encode()).hexdigest(),
                   uniform_bits_expected=uniform_bits,
                   uniform_bits_sha256=(sha(sidecar) if sidecar.is_file() else None),
                   serial_vertices=sum(actor['serial_vertices'] for row in rows for actor in row['actors']) / 120,
                   worker_vertices=sum(actor['worker_vertices'] for row in rows for actor in row['actors']) / 120,
                   final_png_sha256=sha(directory / 'final.png'),
                   gpu_draw_readbacks=sum(len(frame['submitted_packets']) for frame in frames),
                   max_rss_bytes=int(rss[0]) if rss else None,
                   counters_measured_mean={key: mean(key) for key in sorted({key for counter in measured for key in counter})},
                   counters_peak={key: peak(key) for key in actor_keys},
                   eligibility=eligibility[-1] if eligibility else None)
        if not reuse:
            receipt = dict(expected, exit_code=0, argv=cmd, run_dir=str(directory),
                           log_sha256=row['log_sha256'], config_sha256=row['config_sha256'],
                           final_png_sha256=row['final_png_sha256'],
                           census_sha256=sha(directory / f'{map_name}-census-180.json'),
                           effective_config_sha256=row['effective_config_sha256'],
                           uniform_bits_expected=uniform_bits,
                           uniform_bits_sha256=row['uniform_bits_sha256'])
            receipt_path = directory / 'run-receipt.json'
            with receipt_path.open('x') as handle:
                handle.write(json.dumps(receipt, indent=1) + '\n')
            row['receipt_sha256'] = sha(receipt_path)
        else:
            saved = json.loads((directory / 'run-receipt.json').read_text())
            assert saved['effective_config_sha256'] == row['effective_config_sha256'], (map_name, tag, 'effective config changed')
            assert saved['uniform_bits_sha256'] == row['uniform_bits_sha256'], (map_name, tag, 'sidecar binding')
            assert saved['uniform_bits_expected'] == uniform_bits, (map_name, tag, 'sidecar expectation')
        print('DONE', map_name, tag, 'serial', row['serial_vertices'], 'worker', row['worker_vertices'], 'draws', row['gpu_draw_readbacks'], 'reused', reuse, flush=True)
        return row, frames, images

    def compare(map_name, left, right, label, full, bits):
        (left_row, left_frames, left_images), (right_row, right_frames, right_images) = left, right
        # The scaled 3D viewport is a prerequisite observed per frame, never a semantic field
        # and never the HUD/default framebuffer stage.
        for row, frames in ((left_row, left_frames), (right_row, right_frames)):
            startup = json.loads(row['startup_viewport']) if row.get('startup_viewport') else None
            steady, issues = viewport_trajectory(frames, startup)
            assert not issues, (map_name, label, row['arm'], 'per-frame 3D viewport', issues[:4])
            assert json.dumps(steady) == row['viewport'], (map_name, label, row['arm'], 'steady viewport', steady, row['viewport'])
        # Aggregate uniform hashes are compared in every packet comparison, with or without
        # a bit sidecar. The sidecar is an additional field-level gate, never a replacement.
        assert not packet_differences(left_frames, right_frames, keep_slot=False, keep_uniforms=True), \
            (map_name, label, 'uploaded GPU packets including aggregate uniform hash')
        if bits:
            for row, frames in ((left_row, left_frames), (right_row, right_frames)):
                if not row.get('uniform_bits_expected'):
                    continue
                issues = uniform_bits_issues(row['run_dir'], frames)
                assert not issues, (map_name, label, row['arm'], 'uniform bits sidecar', issues[:6])
                sidecar = Path(row['run_dir']) / 'uniform-bits.jsonl'
                receipt = json.loads((Path(row['run_dir']) / 'run-receipt.json').read_text())
                assert receipt.get('uniform_bits_sha256') == sha(sidecar), (map_name, label, row['arm'], 'sidecar receipt hash')
        field_bits = bits and (Path(left_row['run_dir']) / 'uniform-bits.jsonl').is_file() \
            and (Path(right_row['run_dir']) / 'uniform-bits.jsonl').is_file()
        uniform_diffs = []
        for index, (x, y) in enumerate(zip(left_frames, right_frames)):
            for key in ['actor_skin', 'state_sha256', 'selected_lights_bits', 'joint_alpha_order', 'blend_world_order']:
                assert x[key] == y[key], (map_name, label, index, key)
            assert x['counters'].get('vita_fx_dropped_layers', 0) == y['counters'].get('vita_fx_dropped_layers', 0), (map_name, label, index, 'dropped layers')
            if full:
                assert not packet_differences([x], [y], keep_slot=True, keep_uniforms=True), (map_name, label, index, 'packets including gpu_slot')
                assert all(y['counters'].get(key) == value for key, value in x['counters'].items() if key not in ELAPSED), (map_name, label, index, 'reference counters')
        assert len(left_frames) == len(right_frames) == 180
        assert left_images == right_images, (map_name, label, 'intermediate pixels')
        assert left_row['final_png_sha256'] == right_row['final_png_sha256'], (map_name, label, 'final image')
        if field_bits:
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
        pool = {arm: {} for arm in (['off', 'on', 'baseline'] if args.baseline_binary else ['off', 'on'])}
        attempts = []
        selected_view = None
        for attempt in range(max(1, args.viewport_attempts)):
            added = {}
            for arm in pool:
                binary = args.baseline_binary if arm == 'baseline' else args.binary
                run_identity = baseline_identity if arm == 'baseline' else identity
                config = dict(base, **prereq) if arm == 'baseline' else dict(base, **prereq, **{args.flag: arm == 'on'})
                rep = sum(len(v) for v in pool[arm].values())
                bits = wants_bits and arm != 'baseline'
                result_run = run(map_name, arm, binary, config, run_identity, attempt, bits, rep)
                view = result_run[0]['viewport_key']
                pool[arm].setdefault(view, []).append(result_run)
                added[f'{arm}-r{rep}'] = dict(viewport=result_run[0]['viewport'],
                                              startup_viewport=result_run[0]['startup_viewport'],
                                              run_dir=result_run[0]['run_dir'])
            attempts.append(dict(attempt=attempt, runs=added))
            for view in pool['off']:
                if view is None:
                    continue
                if all(len(pool[arm].get(view, [])) >= reps for arm in pool):
                    selected_view = view
                    break
            if selected_view is not None:
                break
            print('RETRY', map_name, 'waiting for one startup viewport with', reps, 'run(s) per arm',
                  {arm: {str(view): len(runs) for view, runs in views.items()} for arm, views in pool.items()}, flush=True)
        entry['viewport_attempts'] = attempts
        if selected_view is None:
            result['status'] = f'FAIL ({map_name}: arms never shared one startup viewport with {reps} run(s) each)'
            write(); sys.exit(1)
        off = pool['off'][selected_view][:reps]
        on = pool['on'][selected_view][:reps]
        ref = pool['baseline'][selected_view][:reps] if 'baseline' in pool else []
        entry['viewport_key'] = selected_view
        entry['startup_viewport'], entry['steady_viewport'] = json.loads(selected_view)
        entry['viewport'] = entry['steady_viewport']
        entry['viewport_frames_gated'] = 180
        entry['viewport_transitions'] = {arm: [run[0]['viewport_transition_frame'] for run in runs]
                                        for arm, runs in (('off', off), ('on', on), ('baseline', ref))}
        entry['runs_at_viewport'] = {arm: len(runs) for arm, runs in
                                     ((name, pool[name][selected_view]) for name in pool)}
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
