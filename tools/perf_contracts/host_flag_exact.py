#!/usr/bin/env python3
"""Host binary oracle for actor_publication_occurrences.

Leaves tools/skin_exact_r9/host_exact.py untouched. Both arms share one binary and
the same prerequisites; only actor_publication_occurrences changes, and it is set
in private/vita_renderer.json. LOOKING_GLASS_* variables from the parent process
are dropped. The test-suite override never reaches this non-test binary.

phase_census stays true in both arms. It is the measurement instrument for
serial_vertices and worker_vertices, not a second lever.

Skin rows that stay on one path compare in full, including actor_skin. A row that
moves from the serial kernel to a worker publication is compared at the publication
observation point: the ON sha256 must equal the OFF published_opaque_sha256 or
published_alpha_sha256 selected by the worker's alpha variant. The raw serial
sha256 is the pre-normalization mesh and is not the worker buffer.
"""
from pathlib import Path
import argparse, hashlib, json, os, subprocess, sys

R = Path(__file__).resolve().parents[2]
DATA = Path('/Users/anezium/vita-work/alice-data/Alice1/bin/base')
R9 = ['pandemonium', 'hedge3', 'keep', 'grounds2', 'tower1', 'tower2', 'tower3',
      'garden2', 'centipede2', 'funhouse', 'jlair1', 'jlair2', 'grounds1', 'hatter1']
MAPS = ['skool2', 'centipede1', *R9]
DT = {'pandemonium': '0.04184', 'hedge3': '0.06'}
PREREQ = {
    'actor_single_prepare': True,
    'skin_overlap': True,
    'skin_workers': 2,
    'actor_fast_path': True,
    'actor_layer_cache': True,
    'fx_prepare_cache': True,
    'actor_layer_reuse': False,
    'renderer': 'vita',
    'skin_actual_consumers': False,
}
COMPARE = ['state_sha256', 'selected_lights_bits', 'joint_alpha_order', 'blend_world_order']


def equivalent_skins(off, on):
    """Return how many rows moved serial -> direct. Any other difference fails."""
    if len(off) != len(on):
        raise AssertionError(('actor_skin length', len(off), len(on)))
    moved = 0
    for i, (a, b) in enumerate(zip(off, on)):
        if a.get('actor') != b.get('actor') or a.get('surface') != b.get('surface'):
            raise AssertionError(('actor_skin identity', i, a.get('actor'), b.get('actor'), a.get('surface'), b.get('surface')))
        if a == b:
            continue
        variant = b.get('alpha_variant')
        key = {False: 'published_opaque_sha256', True: 'published_alpha_sha256'}.get(variant)
        if a.get('path') == 'serial' and b.get('path') == 'direct' and key and b.get('sha256') == a.get(key):
            moved += 1
            continue
        raise AssertionError(('actor_skin', i, a, b))
    return moved


def packets(frame):
    out = []
    for packet in frame['submitted_packets']:
        if packet['geometry'] != packet['gpu_geometry']:
            raise AssertionError(('gpu mismatch', packet.get('gpu_slot')))
        out.append({k: v for k, v in packet.items() if k != 'gpu_slot'})
    return out


def self_test():
    raw = {'actor': 1, 'surface': 2, 'path': 'serial', 'sha256': 'raw',
           'published_opaque_sha256': 'opq', 'published_alpha_sha256': 'alp'}
    direct = {'actor': 1, 'surface': 2, 'path': 'direct', 'sha256': 'opq', 'alpha_variant': False}
    assert equivalent_skins([raw], [direct]) == 1
    assert equivalent_skins([raw, raw], [raw, {**direct, 'sha256': 'alp', 'alpha_variant': True}]) == 1
    same = {'actor': 1, 'surface': 2, 'path': 'direct', 'sha256': 'opq', 'alpha_variant': False}
    assert equivalent_skins([same], [dict(same)]) == 0
    try:
        equivalent_skins([raw], [{**direct, 'sha256': 'nope'}])
    except AssertionError:
        pass
    else:
        raise AssertionError('a mismatched publication hash was accepted')
    try:
        equivalent_skins([direct], [raw])
    except AssertionError:
        pass
    else:
        raise AssertionError('a direct-to-serial change was accepted')


def load_frames(log):
    lines = log.read_text(encoding='utf-8', errors='replace').splitlines()
    frames = [json.loads(line.split(' ', 1)[1]) for line in lines if line.startswith('SIM_MAPS_FRAME ')]
    images = [json.loads(line.split(' ', 1)[1]) for line in lines if line.startswith('SKIN_EXACT_IMAGE ')]
    config = None
    active = None
    for line in lines:
        mark = '[lg/vita-renderer] config '
        if mark in line and config is None:
            config = json.loads(line.split(mark, 1)[1])
        if line.startswith('[lg/vita-renderer] actor_single_prepare active='):
            active = line.rsplit('=', 1)[1].strip()
    fallback = [line for line in lines if '[lg/skin] serial fallback' in line or 'afterimage consumer must be declared' in line]
    return lines, frames, images, config, active, fallback


def census_means(path):
    census = json.loads(path.read_text(encoding='utf-8'))
    if 'serial_fallbacks' in census and census['serial_fallbacks'] != 0:
        raise AssertionError(('serial_fallbacks', path, census['serial_fallbacks']))
    rows = census['rows'][60:]
    if len(rows) != 120:
        raise AssertionError(('census window', path, len(rows)))
    serial = sum(a['serial_vertices'] for row in rows for a in row['actors']) / 120
    worker = sum(a['worker_vertices'] for row in rows for a in row['actors']) / 120
    return serial, worker


def run_arm(binary, output, name, enabled, command, base):
    work = output / 'host' / f'{name}-exact-{int(enabled)}'
    (work / 'private').mkdir(parents=True, exist_ok=True)
    config = dict(base, **PREREQ, phase_census=True, actor_publication_occurrences=enabled)
    (work / 'private/vita_renderer.json').write_text(json.dumps(config), encoding='utf-8')
    env = {k: v for k, v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
    env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1', LOOKING_GLASS_SIM_MAPS_DT=DT.get(name, '0.05'),
               LOOKING_GLASS_SIM_MAPS_COUNTERS='0', LOOKING_GLASS_SKIN_PROOF='1')
    if name != 'occurrence-fixture':
        env.update(LOOKING_GLASS_SKIN_MOVING_VIEW='1', LOOKING_GLASS_SKIN_DEATHS='1')
    else:
        env['LOOKING_GLASS_PUBLICATION_OCCURRENCE_FIXTURE'] = '1'
    log = work / 'run.log'
    with log.open('w', encoding='utf-8') as handle:
        proc = subprocess.run(command, cwd=work, env=env, stdout=handle, stderr=subprocess.STDOUT, timeout=900)
    if proc.returncode != 0:
        raise SystemExit(f'{name} arm={enabled} exit={proc.returncode} log={log}')
    _lines, frames, images, printed, active, fallback = load_frames(log)
    if fallback:
        raise AssertionError((name, enabled, 'serial fallback', fallback[:3]))
    if len(frames) != 180:
        raise AssertionError((name, enabled, 'frames', len(frames)))
    if printed is None or active != 'true':
        raise AssertionError((name, enabled, 'missing effective config', active))
    expect = dict(PREREQ, phase_census=True, actor_publication_occurrences=enabled)
    for key, value in expect.items():
        if printed.get(key) != value:
            raise AssertionError((name, enabled, key, printed.get(key), value))
    serial, worker = census_means(work / f'{name}-census-180.json')
    png = work / 'final.png'
    if not png.is_file():
        raise AssertionError((name, enabled, 'missing final.png'))
    print('DONE', name, enabled, 'serial', serial, 'worker', worker, flush=True)
    return {
        'arm': enabled, 'frames': frames, 'images': images,
        'serial_vertices': serial, 'worker_vertices': worker,
        'final_png_sha256': hashlib.sha256(png.read_bytes()).hexdigest(),
        'config_sha256': hashlib.sha256((work / 'private/vita_renderer.json').read_bytes()).hexdigest(),
        'log_sha256': hashlib.sha256(log.read_bytes()).hexdigest(),
    }


def viewport(output, name, enabled):
    log = output / 'host' / f'{name}-exact-{int(enabled)}' / 'run.log'
    for line in log.read_text(encoding='utf-8', errors='replace').splitlines():
        if 'prepare-3d-returned' in line and 'viewport=' in line:
            start = line.index('viewport=') + len('viewport=')
            end = line.index(']', start)
            return line[start:end + 1]
    return 'fixture' if name == 'occurrence-fixture' else None


def compare_arms(name, off, on):
    moved = 0
    for index, (left, right) in enumerate(zip(off['frames'], on['frames'])):
        for key in COMPARE:
            if left[key] != right[key]:
                raise AssertionError((name, index, key))
        moved += equivalent_skins(left['actor_skin'], right['actor_skin'])
        if packets(left) != packets(right):
            raise AssertionError((name, index, 'uploaded GPU packets'))
    if off['images'] != on['images']:
        raise AssertionError((name, 'intermediate pixels'))
    if off['final_png_sha256'] != on['final_png_sha256']:
        raise AssertionError((name, 'final image'))
    drop = off['serial_vertices'] - on['serial_vertices']
    gain = on['worker_vertices'] - off['worker_vertices']
    row = {
        'map': name, 'status': 'PASS', 'frames': 180,
        'off_serial_vertices': off['serial_vertices'], 'on_serial_vertices': on['serial_vertices'],
        'off_worker_vertices': off['worker_vertices'], 'on_worker_vertices': on['worker_vertices'],
        'serial_drop': drop, 'worker_gain': gain, 'moved_skin_rows': moved,
        'final_png_sha256': off['final_png_sha256'],
        'logs': {'off': off['log_sha256'], 'on': on['log_sha256']},
        'configs': {'off': off['config_sha256'], 'on': on['config_sha256']},
    }
    if drop > 0 or gain > 0 or moved:
        if not (drop > 0 and gain > 0 and moved > 0):
            raise AssertionError((name, 'partial move', row))
    print('PASS', name, 'drop', drop, 'gain', gain, 'moved_rows', moved, flush=True)
    return row


def main():
    self_test()
    parser = argparse.ArgumentParser()
    parser.add_argument('output', type=Path)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--maps', nargs='*')
    parser.add_argument('--fixture-only', action='store_true')
    args = parser.parse_args()
    output = args.output.resolve()
    binary = args.binary.resolve()
    base = json.loads((R / 'tools/lever_matrix_r4/configs/PL-baseline.json').read_text(encoding='utf-8'))
    if 'skin_actual_consumers' in base and base['skin_actual_consumers'] is not False:
        raise SystemExit('PL-baseline skin_actual_consumers is not the false default this oracle pins')
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    proof_dir = output / 'proof'
    proof_dir.mkdir(parents=True, exist_ok=True)
    proof_path = proof_dir / 'HOST-FLAG.json'
    previous = {}
    if proof_path.is_file():
        loaded = json.loads(proof_path.read_text(encoding='utf-8'))
        if loaded.get('binary_sha256') == digest:
            previous = {row['map']: row for row in loaded.get('maps', []) if row.get('status') == 'PASS'}
    maps = [] if args.fixture_only else (args.maps or MAPS)
    unknown = [name for name in maps if name not in MAPS]
    if unknown:
        raise SystemExit(f'unexpected maps {unknown}')
    rows = []
    cases = []
    if args.fixture_only or not args.maps:
        cases.append(('occurrence-fixture', ['--skin-overlap-render-check']))
    cases.extend((name, ['--map', name, '--frames', '180']) for name in maps)
    for name, extra in cases:
        if name in previous:
            rows.append(previous[name])
            print('SKIP', name, 'already passed for this binary', flush=True)
            continue
        arms = None
        view = None
        for attempt in range(8):
            arms = []
            for enabled in (False, True):
                work = output / 'host' / f'{name}-exact-{int(enabled)}'
                command = [str(binary), '--data', str(DATA), '--no-audio', '--save-dir', str(work / 'private/saves'), *extra]
                if name != 'occurrence-fixture':
                    command = [str(binary), '--data', str(DATA), '--no-audio', '--save-dir', str(work / 'private/saves'),
                               '--map', name, '--frames', '180', '--capture', str(work / 'final.png')]
                arms.append(run_arm(binary, output, name, enabled, command, base))
            view = (viewport(output, name, False), viewport(output, name, True))
            if view[0] == view[1]:
                break
            print('RETRY', name, 'host viewport', view, 'attempt', attempt + 1, flush=True)
        else:
            raise SystemExit(f'{name} drawable viewport differed between arms {view}')
        row = compare_arms(name, arms[0], arms[1])
        row['viewport'] = view[0]
        rows.append(row)
        proof = {
            'binary_sha256': digest, 'binary': str(binary), 'prerequisites': PREREQ,
            'phase_census': 'instrument, true in both arms; not the lever',
            'skin_comparison': 'full actor_skin equality, except a serial-to-direct row whose ON sha256 equals the OFF published hash of the worker alpha variant',
            'fixture': 'occurrence-fixture draws Alice three times per frame on the overlap render-check path; host map entries did not repeat an eligible surface',
            'maps': rows,
        }
        proof_path.write_text(json.dumps(proof, indent=2) + '\n', encoding='utf-8')
    witnesses = [row['map'] for row in rows if row['serial_drop'] > 0 and row['worker_gain'] > 0 and row['moved_skin_rows'] > 0]
    status = 'PASS' if witnesses else 'FAIL'
    proof = json.loads(proof_path.read_text(encoding='utf-8'))
    proof.update(status=status, witness_maps=witnesses, vacuous=not witnesses,
                 required_maps=maps)
    proof_path.write_text(json.dumps(proof, indent=2) + '\n', encoding='utf-8')
    if not witnesses:
        raise SystemExit('FAIL vacuous: no map moved serial vertices onto workers')
    print('PASS witness', ', '.join(witnesses), flush=True)


if __name__ == '__main__':
    try:
        main()
    except subprocess.TimeoutExpired as error:
        sys.exit(f'FAIL timeout {error}')
