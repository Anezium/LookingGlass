#!/usr/bin/env python3
"""Host binary oracle for actor_publication_occurrences.

Both arms run one binary with the same prerequisites; only
actor_publication_occurrences changes, set in private/vita_renderer.json.
phase_census stays true in both arms as the measurement instrument for
serial_vertices and worker_vertices, not a second lever.

Evidence rules:
- Every launch gets a fresh directory, host/<case>/attempt-NN/arm-{0,1}, created
  with exist_ok=False and made read-only when the arm ends. Nothing is overwritten.
- ledger.jsonl is append-only and hash-chained. It binds each launch to its argv,
  complete environment (built from scratch, not inherited), binary hash before
  and after, the build identity file, the game data digest, the base config,
  this comparator's hash and every output file's hash.
- Up to eight paired attempts. An attempt is selected or retried only on
  presentation prerequisites (requested window, framebuffer, scaled target,
  GL viewport, screen size), compared for all 180 frames before any semantic
  output is read. A prerequisite that was not measured fails the case.
  Mismatching attempts stay on disk and in the ledger with their classification.
- After selection the complete frames are compared. Only cpu_ms and spans_ms
  (elapsed time) and packet gpu_slot (physical ring slot) are excluded. A skin
  row may move from the serial kernel to a worker publication only under the
  strict schema in moved_row(). Projection matrices, framebuffer hashes for all
  180 frames, renderer drop counters and work conservation are gates.
- --reuse re-validates a previous PASS from raw files: comparator, binary,
  identity, data and base config hashes must match, every recorded file must
  still hash the same, and the comparison is re-run on the raw logs. Any
  failure rejects the reuse; nothing is relaunched in that mode.
"""
from pathlib import Path
import argparse, copy, hashlib, json, os, re, stat, subprocess, sys, time

R = Path(__file__).resolve().parents[2]
DATA = Path('/Users/anezium/vita-work/alice-data/Alice1/bin/base')
R9 = ['pandemonium', 'hedge3', 'keep', 'grounds2', 'tower1', 'tower2', 'tower3',
      'garden2', 'centipede2', 'funhouse', 'jlair1', 'jlair2', 'grounds1', 'hatter1']
MAPS = ['skool2', 'centipede1', *R9]
FIXTURE = 'occurrence-fixture'
DT = {'pandemonium': '0.04184', 'hedge3': '0.06'}
FRAMES = 180
ATTEMPTS = 8
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
FRAME_KEYS = {'active', 'actor_skin', 'blend_world_order', 'counters', 'cpu_ms', 'frame', 'joint_alpha_order', 'map',
              'npc_decisions', 'render', 'renderer_drops', 'selected_lights_bits', 'spans_ms', 'state_sha256',
              'submitted_packets'}
# Elapsed-time instrumentation. Everything else in a frame is compared.
TIMING_KEYS = {'cpu_ms', 'spans_ms'}
SERIAL_ROW = {'actor', 'surface', 'sha256', 'path', 'published_opaque_sha256', 'published_alpha_sha256'}
DIRECT_ROW = {'actor', 'surface', 'sha256', 'path', 'alpha_variant'}
PACKET_KEYS = {'geometry', 'gpu_geometry', 'gpu_slot', 'indices', 'policy', 'texture', 'uniforms', 'vertices'}
PRESENTATION_KEYS = {'case', 'frame', 'requested_window', 'requested_high_dpi', 'screen_size_bits', 'framebuffer',
                     'scaled_target', 'viewport', 'projection_bits'}
# Selection prerequisites. projection_bits is a semantic output, compared after selection.
PREREQ_FIELDS = ['requested_window', 'requested_high_dpi', 'screen_size_bits', 'framebuffer', 'scaled_target', 'viewport']
MAP_PNG_FRAMES = {0, 59, 60, 119, 120, 179}
OCCURRENCE_MAP_ROWS = {'actor_publication_mem_occurrences_len', 'actor_publication_mem_occurrences_capacity',
                       'actor_publication_mem_maps_bytes'}
HEX = re.compile(r'^[0-9a-f]{64}$')


class Mismatch(AssertionError):
    """A semantic difference between arms, or an invalid arm. Never retried."""


def fail(*what):
    raise Mismatch(what)


def sha_bytes(data):
    return hashlib.sha256(data).hexdigest()


def sha_file(path):
    h = hashlib.sha256()
    with open(path, 'rb') as handle:
        for block in iter(lambda: handle.read(1 << 20), b''):
            h.update(block)
    return h.hexdigest()


def fixture_draws(frame):
    """Mirror of viewer.rs occurrence_fixture_draws: the number of Alice draws."""
    if frame <= 89:
        return 3
    if frame <= 119:
        return 5
    if frame <= 149:
        return 2
    return 3


FIXTURE_DOMAINS = [(0, 89, 'three draws, warm-up then steady'), (90, 119, 'five draws: fourth publishes, fifth serial'),
                   (120, 149, 'two draws, reversed order'), (150, 179, 'three draws, order changes every frame')]


# ---------------------------------------------------------------- comparison

def moved_row(off_row, on_row, off_rows, on_rows):
    """A serial OFF row that became a worker publication ON.

    OFF must be exactly the serial schema and ON exactly the direct schema; any
    other field fails. alpha_variant must be a real bool. ON sha256 is the
    worker buffer and must equal the OFF normalized hash of that variant. The
    variant must be the one every other direct row of the same actor surface in
    this frame uses, in either arm. The OFF raw sha256 is the pre-normalization
    serial mesh; it has no worker counterpart and is accounted for by the two
    published hashes, both of which must be well-formed.
    """
    if set(off_row) != SERIAL_ROW or off_row.get('path') != 'serial':
        fail('moved row: OFF is not the serial schema', off_row)
    if set(on_row) != DIRECT_ROW or on_row.get('path') != 'direct':
        fail('moved row: ON is not the direct schema', on_row)
    variant = on_row['alpha_variant']
    if type(variant) is not bool:
        fail('moved row: alpha_variant is not a bool', on_row)
    for key in ('sha256', 'published_opaque_sha256', 'published_alpha_sha256'):
        if not isinstance(off_row[key], str) or not HEX.match(off_row[key]):
            fail('moved row: malformed OFF hash', key, off_row)
    if not isinstance(on_row['sha256'], str) or not HEX.match(on_row['sha256']):
        fail('moved row: malformed ON hash', on_row)
    published = off_row['published_alpha_sha256' if variant else 'published_opaque_sha256']
    if on_row['sha256'] != published:
        fail('moved row: worker buffer differs from the serial output normalized for its variant', off_row, on_row)
    identity = (off_row['actor'], off_row['surface'])
    for rows in (off_rows, on_rows):
        for other in rows:
            if (other.get('actor'), other.get('surface')) == identity and other.get('path') == 'direct' \
                    and other.get('alpha_variant') is not variant:
                fail('moved row: alpha variant differs from the same surface published elsewhere', on_row, other)


def check_row_schema(row):
    keys = set(row)
    if keys == SERIAL_ROW and row['path'] == 'serial':
        return
    if keys == DIRECT_ROW and row['path'] == 'direct' and type(row['alpha_variant']) is bool:
        return
    fail('actor_skin row schema', row)


def equivalent_skins(off, on):
    """Return how many rows moved serial -> direct. Any other difference fails."""
    if not isinstance(off, list) or not isinstance(on, list):
        fail('actor_skin is not a list')
    if len(off) != len(on):
        fail('actor_skin length', len(off), len(on))
    moved = 0
    for i, (a, b) in enumerate(zip(off, on)):
        check_row_schema(a)
        check_row_schema(b)
        if (a['actor'], a['surface']) != (b['actor'], b['surface']):
            fail('actor_skin identity', i, a['actor'], b['actor'], a['surface'], b['surface'])
        if a == b:
            continue
        if a['path'] == 'serial' and b['path'] == 'direct':
            moved_row(a, b, off, on)
            moved += 1
            continue
        fail('actor_skin', i, a, b)
    return moved


def packets(frame):
    out = []
    for packet in frame['submitted_packets']:
        if set(packet) != PACKET_KEYS:
            fail('packet schema', sorted(packet))
        if packet['geometry'] != packet['gpu_geometry']:
            fail('gpu mismatch', packet.get('gpu_slot'))
        out.append({k: v for k, v in packet.items() if k != 'gpu_slot'})
    return out


def check_frames(name, frames):
    if len(frames) != FRAMES:
        fail(name, 'frame count', len(frames))
    for index, frame in enumerate(frames):
        if set(frame) != FRAME_KEYS:
            fail(name, index, 'frame keys', sorted(set(frame) ^ FRAME_KEYS))
        if frame['frame'] != index or frame['map'] != name:
            fail(name, index, 'frame identity', frame['frame'], frame['map'])


def check_images(name, images, arm_dir, png_frames):
    if len(images) != FRAMES:
        fail(name, 'image count', len(images))
    for index, image in enumerate(images):
        if image.get('case') != name or image.get('frame') != index:
            fail(name, index, 'image identity', image.get('case'), image.get('frame'))
        if not HEX.match(str(image.get('sha256'))) or not image.get('width') or not image.get('height'):
            fail(name, index, 'image record', image)
        png = image.get('png')
        if (index in png_frames) != (png is not None):
            fail(name, index, 'png presence', png)
        if png is not None:
            path = arm_dir / png['path']
            if not path.is_file() or sha_file(path) != png['sha256']:
                fail(name, index, 'png file', png)


def check_presentation(name, records):
    """Measured prerequisites for every frame. Missing values fail; they are never equal."""
    if len(records) != FRAMES:
        fail(name, 'presentation count', len(records))
    for index, record in enumerate(records):
        if set(record) != PRESENTATION_KEYS or record['case'] != name or record['frame'] != index:
            fail(name, index, 'presentation record', record)
        for field in ('requested_window', 'screen_size_bits', 'framebuffer', 'viewport', 'projection_bits'):
            if not isinstance(record[field], list) or not record[field]:
                fail(name, index, 'unmeasured prerequisite', field)
        if type(record['requested_high_dpi']) is not bool:
            fail(name, index, 'unmeasured prerequisite', 'requested_high_dpi')
        if record['viewport'][2] <= 0 or record['viewport'][3] <= 0 or min(record['framebuffer']) <= 0:
            fail(name, index, 'empty viewport or framebuffer', record)
        # Map frames render through the scaled target; the fixture never creates one.
        if name == FIXTURE:
            if record['scaled_target'] is not None:
                fail(name, index, 'fixture unexpectedly used a scaled target', record['scaled_target'])
        elif not isinstance(record['scaled_target'], list) or min(record['scaled_target']) <= 0:
            fail(name, index, 'unmeasured prerequisite', 'scaled_target')


def prerequisite_mismatch(off, on):
    """Frames whose presentation prerequisites differ. Empty means the pair is selectable."""
    out = []
    for a, b in zip(off, on):
        for field in PREREQ_FIELDS:
            if a[field] != b[field]:
                out.append({'frame': a['frame'], 'field': field, 'off': a[field], 'on': b[field]})
    return out


def census_rows(path):
    census = json.loads(path.read_text(encoding='utf-8'))
    if census.get('serial_fallbacks', 0) != 0:
        fail('serial_fallbacks', str(path), census['serial_fallbacks'])
    rows = census['rows']
    if len(rows) != FRAMES:
        fail('census rows', str(path), len(rows))
    out = []
    for row in rows:
        serial = sum(a['serial_vertices'] for a in row['actors'])
        worker = sum(a['worker_vertices'] for a in row['actors'])
        if not all(isinstance(v, int) for v in (serial, worker)):
            fail('census vertex counts are not integers', str(path))
        out.append((serial, worker))
    return out


def conservation(name, off, on, moved_by_frame):
    """Exact per-frame work conservation. Returns window totals over frames 60-179."""
    for index, ((off_s, off_w), (on_s, on_w)) in enumerate(zip(off, on)):
        drop, gain = off_s - on_s, on_w - off_w
        if drop != gain or drop < 0:
            fail(name, index, 'work not conserved', drop, gain)
        if (drop > 0) != (moved_by_frame[index] > 0):
            fail(name, index, 'moved rows and moved vertices disagree', drop, moved_by_frame[index])
    window = slice(60, FRAMES)
    totals = {k: sum(v[i] for v in rows[window]) for k, rows, i in
              [('off_serial', off, 0), ('off_worker', off, 1), ('on_serial', on, 0), ('on_worker', on, 1)]}
    totals['serial_drop'] = totals['off_serial'] - totals['on_serial']
    totals['worker_gain'] = totals['on_worker'] - totals['off_worker']
    return totals


def fixture_domains(off_frames, on_frames):
    """Per frame and actor surface: the expected number of direct rows in each arm."""
    seen = set()
    for index, (a, b) in enumerate(zip(off_frames, on_frames)):
        n = fixture_draws(index)
        for arm, frame, limit in (('off', a, 2), ('on', b, 4)):
            by_surface = {}
            for row in frame['actor_skin']:
                by_surface.setdefault((row['actor'], row['surface']), []).append(row['path'])
            if not by_surface:
                fail(FIXTURE, index, arm, 'no skin rows')
            for surface, paths in by_surface.items():
                if len(paths) != n or paths.count('direct') != min(n, limit):
                    fail(FIXTURE, index, arm, 'domain path counts', surface, paths)
        seen.add(n)
    if seen != {2, 3, 5}:
        fail(FIXTURE, 'domains not all exercised', sorted(seen))


def memory_rows(records):
    return [r['rows'] for r in records]


def memory_summary(rows):
    if not rows:
        return None
    keys = sorted(rows[0])
    return {
        'frames': len(rows),
        'first': rows[0],
        'last': rows[-1],
        'max': {k: max(r[k] for r in rows) for k in keys},
    }


def compare_arms(name, off, on):
    """Complete semantic comparison of one selected pair. Raises Mismatch."""
    for arm in (off, on):
        check_frames(name, arm['frames'])
        check_presentation(name, arm['presentation'])
        check_images(name, arm['images'], arm['dir'], set(range(FRAMES)) if name == FIXTURE else MAP_PNG_FRAMES)
    if prerequisite_mismatch(off['presentation'], on['presentation']):
        fail(name, 'compare_arms called on an unselected pair')
    moved_by_frame = []
    for index, (left, right) in enumerate(zip(off['frames'], on['frames'])):
        for key in FRAME_KEYS - TIMING_KEYS - {'actor_skin', 'submitted_packets'}:
            if left[key] != right[key]:
                fail(name, index, key)
        moved_by_frame.append(equivalent_skins(left['actor_skin'], right['actor_skin']))
        if packets(left) != packets(right):
            fail(name, index, 'uploaded GPU packets')
        if off['presentation'][index]['projection_bits'] != on['presentation'][index]['projection_bits']:
            fail(name, index, 'projection')
        if off['images'][index] != on['images'][index]:
            fail(name, index, 'framebuffer pixels')
    if off['final_png_sha256'] != on['final_png_sha256']:
        fail(name, 'final image')
    totals = conservation(name, off['census'], on['census'], moved_by_frame)
    moved = sum(moved_by_frame)
    witness = totals['serial_drop'] > 0
    if witness != (totals['worker_gain'] > 0) or witness != (moved > 0):
        fail(name, 'partial move', totals, moved)
    if name == FIXTURE:
        if not witness:
            fail(name, 'the fixture moved no work')
        fixture_domains(off['frames'], on['frames'])
    else:
        # Nothing moved, so the banks must be the same in both arms. Only the
        # occurrence map differs: the off arm never inserts into it.
        for a, b in zip(off['memory'], on['memory']):
            if a['actor_publication_mem_occurrences_len'] or a['actor_publication_mem_occurrences_capacity']:
                fail(name, 'the off arm used the occurrence map')
            if {k: v for k, v in a.items() if k not in OCCURRENCE_MAP_ROWS} != {k: v for k, v in b.items() if k not in OCCURRENCE_MAP_ROWS}:
                fail(name, 'publication memory differs without moved work')
    if len(off['memory']) != FRAMES or len(on['memory']) != FRAMES:
        fail(name, 'publication memory records', len(off['memory']), len(on['memory']))
    return {
        'map': name, 'status': 'PASS', 'frames': FRAMES,
        **{k: v for k, v in totals.items()},
        'off_serial_mean': totals['off_serial'] / 120, 'on_serial_mean': totals['on_serial'] / 120,
        'off_worker_mean': totals['off_worker'] / 120, 'on_worker_mean': totals['on_worker'] / 120,
        'moved_skin_rows': moved,
        'moved_frames': sum(1 for m in moved_by_frame if m),
        'skin_rows': sum(len(f['actor_skin']) for f in off['frames']),
        'packets': sum(len(f['submitted_packets']) for f in off['frames']),
        'image_frames': len(off['images']),
        'png_frames': sum(1 for i in off['images'] if i['png']),
        'renderer_drops': {'groups': sum(f['renderer_drops']['groups'] for f in off['frames']),
                           'layers': sum(f['renderer_drops']['layers'] for f in off['frames'])},
        'final_png_sha256': off['final_png_sha256'],
        'final_png_equals_frame_179_png': off['final_png_equals_179'],
        'memory': {'off': memory_summary(off['memory']), 'on': memory_summary(on['memory'])},
    }


# ---------------------------------------------------------------- arms on disk

def parse_arm(name, arm_dir, enabled):
    log = arm_dir / 'run.log'
    lines = log.read_text(encoding='utf-8', errors='replace').splitlines()

    def tagged(tag):
        return [json.loads(line.split(' ', 1)[1]) for line in lines if line.startswith(tag + ' ')]
    config = None
    active = None
    for line in lines:
        mark = '[lg/vita-renderer] config '
        if mark in line and config is None:
            config = json.loads(line.split(mark, 1)[1])
        if line.startswith('[lg/vita-renderer] actor_single_prepare active='):
            active = line.rsplit('=', 1)[1].strip()
    fallback = [line for line in lines if '[lg/skin] serial fallback' in line or 'afterimage consumer must be declared' in line]
    if fallback:
        fail(name, enabled, 'serial fallback', fallback[:3])
    if config is None or active != 'true':
        fail(name, enabled, 'missing effective config', active)
    for key, value in dict(PREREQ, phase_census=True, actor_publication_occurrences=enabled).items():
        if config.get(key) != value:
            fail(name, enabled, 'effective config', key, config.get(key), value)
    written = json.loads((arm_dir / 'private/vita_renderer.json').read_text(encoding='utf-8'))
    if written.get('actor_publication_occurrences') is not enabled:
        fail(name, enabled, 'written config arm')
    png = arm_dir / 'final.png'
    if not png.is_file():
        fail(name, enabled, 'missing final.png')
    last = arm_dir / 'exact-frames' / f'{name}-179.png'
    return {
        'arm': enabled, 'dir': arm_dir,
        'frames': tagged('SIM_MAPS_FRAME'), 'images': tagged('SKIN_EXACT_IMAGE'),
        'presentation': tagged('SKIN_EXACT_PRESENTATION'), 'memory': memory_rows(tagged('PUBLICATION_MEMORY')),
        'census': census_rows(arm_dir / f'{name}-census-{FRAMES}.json'),
        'final_png_sha256': sha_file(png),
        'final_png_equals_179': last.is_file() and sha_file(last) == sha_file(png),
    }


def files_of(arm_dir):
    return {str(p.relative_to(arm_dir)): sha_file(p) for p in sorted(arm_dir.rglob('*')) if p.is_file()}


def seal(arm_dir):
    for p in arm_dir.rglob('*'):
        if p.is_file():
            p.chmod(stat.S_IRUSR | stat.S_IRGRP | stat.S_IROTH)


# ---------------------------------------------------------------- ledger

class Ledger:
    def __init__(self, path):
        self.path = path
        self.prev = None
        self.seq = 0
        if path.exists():
            for line in path.read_text(encoding='utf-8').splitlines():
                entry = json.loads(line)
                if entry.get('prev_sha256') != self.prev:
                    raise SystemExit(f'ledger chain broken at seq {entry.get("seq")}')
                self.prev = sha_bytes(line.encode())
                self.seq = entry['seq'] + 1

    def entries(self):
        if not self.path.exists():
            return []
        return [json.loads(line) for line in self.path.read_text(encoding='utf-8').splitlines()]

    def append(self, **entry):
        entry = dict(seq=self.seq, prev_sha256=self.prev, utc=time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()), **entry)
        line = json.dumps(entry, sort_keys=True, default=str)
        fd = os.open(self.path, os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o644)
        try:
            os.write(fd, (line + '\n').encode())
            os.fsync(fd)
        finally:
            os.close(fd)
        self.prev = sha_bytes(line.encode())
        self.seq += 1
        return entry


# ---------------------------------------------------------------- launching

def arm_environment(name):
    env = {'HOME': os.environ['HOME'], 'USER': os.environ.get('USER', ''), 'LOGNAME': os.environ.get('LOGNAME', ''),
           'PATH': '/usr/bin:/bin', 'LANG': 'C', 'TMPDIR': os.environ.get('TMPDIR', '/tmp'),
           'LOOKING_GLASS_SIM_MAPS_PROBE': '1', 'LOOKING_GLASS_SIM_MAPS_DT': DT.get(name, '0.05'),
           'LOOKING_GLASS_SIM_MAPS_COUNTERS': '0', 'LOOKING_GLASS_SKIN_PROOF': '1'}
    if name == FIXTURE:
        env['LOOKING_GLASS_PUBLICATION_OCCURRENCE_FIXTURE'] = '1'
    else:
        env.update(LOOKING_GLASS_SKIN_MOVING_VIEW='1', LOOKING_GLASS_SKIN_DEATHS='1')
    return env


def arm_command(binary, name, arm_dir):
    command = [str(binary), '--data', str(DATA), '--no-audio', '--save-dir', str(arm_dir / 'private/saves')]
    if name == FIXTURE:
        return command + ['--skin-overlap-render-check']
    return command + ['--map', name, '--frames', str(FRAMES), '--capture', str(arm_dir / 'final.png')]


def launch(ctx, name, attempt, enabled, arm_dir):
    """Run one arm in its fresh directory. Returns the exit code."""
    (arm_dir / 'private').mkdir(parents=True)
    config = dict(ctx['base'], **PREREQ, phase_census=True, actor_publication_occurrences=enabled)
    (arm_dir / 'private/vita_renderer.json').write_text(json.dumps(config), encoding='utf-8')
    env = arm_environment(name)
    command = arm_command(ctx['binary'], name, arm_dir)
    before = sha_file(ctx['binary'])
    if before != ctx['binary_sha256']:
        raise SystemExit(f'binary changed before {name} attempt {attempt}')
    ctx['ledger'].append(event='launch', case=name, attempt=attempt, arm=int(enabled), dir=str(arm_dir), argv=command,
                         env=env, cwd=str(arm_dir), binary_sha256_before=before,
                         config_sha256=sha_file(arm_dir / 'private/vita_renderer.json'), **ctx['bindings'])
    start = time.monotonic()
    with (arm_dir / 'run.log').open('x', encoding='utf-8') as handle:
        try:
            code = subprocess.run(command, cwd=arm_dir, env=env, stdout=handle, stderr=subprocess.STDOUT, timeout=900).returncode
        except subprocess.TimeoutExpired:
            code = 'timeout'
    after = sha_file(ctx['binary'])
    files = files_of(arm_dir)
    seal(arm_dir)
    ctx['ledger'].append(event='exit', case=name, attempt=attempt, arm=int(enabled), exit_code=code,
                         seconds=round(time.monotonic() - start, 1), binary_sha256_after=after, files=files)
    if after != before:
        raise SystemExit(f'binary changed during {name} attempt {attempt}')
    return code


def run_case(ctx, name):
    case_dir = ctx['output'] / 'host' / name
    case_dir.mkdir(parents=True, exist_ok=True)
    for attempt in range(ATTEMPTS):
        attempt_dir = case_dir / f'attempt-{attempt:02d}'
        attempt_dir.mkdir(exist_ok=False)
        arms = []
        for enabled in (False, True):
            arm_dir = attempt_dir / f'arm-{int(enabled)}'
            code = ctx['launch'](ctx, name, attempt, enabled, arm_dir)
            if code != 0:
                ctx['ledger'].append(event='result', case=name, attempt=attempt, status='FAIL', reason=f'exit {code}', arm=int(enabled))
                raise SystemExit(f'FAIL {name} attempt {attempt} arm {int(enabled)} exit {code}')
            arms.append(arm_dir)
        parsed = []
        try:
            for enabled, arm_dir in zip((False, True), arms):
                arm = parse_arm(name, arm_dir, enabled)
                check_presentation(name, arm['presentation'])
                parsed.append(arm)
        except Mismatch as error:
            ctx['ledger'].append(event='result', case=name, attempt=attempt, status='FAIL', reason=repr(error.args)[:4000])
            raise SystemExit(f'FAIL {name} attempt {attempt}: {error.args}')
        mismatch = prerequisite_mismatch(parsed[0]['presentation'], parsed[1]['presentation'])
        ctx['ledger'].append(event='prerequisites', case=name, attempt=attempt, selected=not mismatch,
                             mismatched_frames=len({m['frame'] for m in mismatch}),
                             fields=sorted({m['field'] for m in mismatch}), first=mismatch[:8])
        if mismatch:
            print('RETRY', name, 'attempt', attempt, 'prerequisites differ', sorted({m['field'] for m in mismatch}), flush=True)
            continue
        try:
            row = compare_arms(name, parsed[0], parsed[1])
        except Mismatch as error:
            ctx['ledger'].append(event='result', case=name, attempt=attempt, status='FAIL', reason=repr(error.args)[:4000])
            raise SystemExit(f'FAIL {name} attempt {attempt}: {error.args}')
        row.update(attempt=attempt, attempts=attempt + 1, dirs=[str(a) for a in arms])
        ctx['ledger'].append(event='result', case=name, attempt=attempt, status='PASS', row=row, **ctx['bindings'])
        print('PASS', name, 'attempt', attempt, 'drop', row['serial_drop'], 'gain', row['worker_gain'],
              'moved_rows', row['moved_skin_rows'], flush=True)
        return row
    ctx['ledger'].append(event='result', case=name, attempt=ATTEMPTS - 1, status='FAIL', reason='prerequisites never matched')
    raise SystemExit(f'FAIL {name}: presentation prerequisites differed in all {ATTEMPTS} attempts')


def reuse_case(ctx, name):
    """Re-validate a ledgered PASS from raw files. Raises SystemExit on any doubt."""
    entries = ctx['ledger'].entries()
    passes = [e for e in entries if e.get('event') == 'result' and e.get('case') == name and e.get('status') == 'PASS']
    if not passes:
        raise SystemExit(f'REUSE REJECTED {name}: no ledgered PASS')
    entry = passes[-1]
    for key, value in ctx['bindings'].items():
        if entry.get(key) != value:
            raise SystemExit(f'REUSE REJECTED {name}: {key} differs ({entry.get(key)} != {value})')
    attempt = entry['attempt']
    arms = []
    for enabled in (False, True):
        exits = [e for e in entries if e.get('event') == 'exit' and e.get('case') == name
                 and e.get('attempt') == attempt and e.get('arm') == int(enabled)]
        launches = [e for e in entries if e.get('event') == 'launch' and e.get('case') == name
                    and e.get('attempt') == attempt and e.get('arm') == int(enabled)]
        if len(exits) != 1 or len(launches) != 1 or exits[0]['exit_code'] != 0:
            raise SystemExit(f'REUSE REJECTED {name}: launch record for arm {int(enabled)}')
        arm_dir = Path(launches[0]['dir'])
        if not arm_dir.is_dir():
            raise SystemExit(f'REUSE REJECTED {name}: missing {arm_dir}')
        recorded = exits[0]['files']
        if files_of(arm_dir) != recorded:
            raise SystemExit(f'REUSE REJECTED {name}: files in {arm_dir} differ from the ledger')
        if launches[0]['binary_sha256_before'] != ctx['binary_sha256'] or exits[0]['binary_sha256_after'] != ctx['binary_sha256']:
            raise SystemExit(f'REUSE REJECTED {name}: binary')
        for path in arm_dir.rglob('*'):
            if path.is_file() and path.stat().st_mode & (stat.S_IWUSR | stat.S_IWGRP | stat.S_IWOTH):
                raise SystemExit(f'REUSE REJECTED {name}: {path} is writable')
        arms.append(arm_dir)
    try:
        parsed = [parse_arm(name, a, e) for a, e in zip(arms, (False, True))]
        row = compare_arms(name, parsed[0], parsed[1])
    except Mismatch as error:
        raise SystemExit(f'REUSE REJECTED {name}: raw comparison failed {error.args}')
    row.update(attempt=attempt, attempts=entry['row']['attempts'], dirs=[str(a) for a in arms], reused=True)
    ctx['ledger'].append(event='reuse', case=name, attempt=attempt, row=row, **ctx['bindings'])
    print('REUSED', name, 'attempt', attempt, flush=True)
    return row


# ---------------------------------------------------------------- self test

def self_test():
    h = lambda c: c * 64
    raw = {'actor': 1, 'surface': 2, 'path': 'serial', 'sha256': h('a'),
           'published_opaque_sha256': h('b'), 'published_alpha_sha256': h('c')}
    direct = {'actor': 1, 'surface': 2, 'path': 'direct', 'sha256': h('b'), 'alpha_variant': False}

    def rejects(fn, label):
        try:
            fn()
        except Mismatch:
            return
        raise AssertionError(f'self-test accepted: {label}')
    assert equivalent_skins([raw], [direct]) == 1
    assert equivalent_skins([raw, raw], [raw, dict(direct, sha256=h('c'), alpha_variant=True)]) == 1
    assert equivalent_skins([direct], [dict(direct)]) == 0
    rejects(lambda: equivalent_skins([raw], [dict(direct, sha256=h('d'))]), 'wrong publication hash')
    rejects(lambda: equivalent_skins([direct], [raw]), 'direct to serial')
    rejects(lambda: equivalent_skins([dict(raw, semantic=1)], [dict(direct, semantic=2)]), 'extra field')
    rejects(lambda: equivalent_skins([raw], [dict(direct, alpha_variant=0)]), 'integer alpha variant')
    rejects(lambda: equivalent_skins([raw, direct], [dict(direct, alpha_variant=True, sha256=h('c')), direct]),
            'variant differs from the same surface')
    rejects(lambda: equivalent_skins([raw], []), 'length')
    rejects(lambda: prerequisite_mismatch([], []) or check_presentation(FIXTURE, []), 'empty presentation')
    rejects(lambda: conservation('x', [(10, 20)] * FRAMES, [(0, 21)] * FRAMES, [1] * FRAMES), '10 versus 1')
    rejects(lambda: check_frames('x', []), 'empty frames')
    print('self-test PASS', flush=True)


# ---------------------------------------------------------------- main

def data_digest():
    rows = [(p.name, p.stat().st_size, sha_file(p)) for p in sorted(DATA.glob('*.pk3'))]
    if not rows:
        raise SystemExit(f'no pk3 in {DATA}')
    return sha_bytes(json.dumps(rows).encode()), rows


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument('output', type=Path, nargs='?')
    parser.add_argument('--binary', type=Path)
    parser.add_argument('--identity', type=Path, help='build receipt that names the binary hash and source snapshot')
    parser.add_argument('--maps', nargs='*')
    parser.add_argument('--fixture-only', action='store_true')
    parser.add_argument('--reuse', action='store_true', help='re-validate ledgered PASS rows from raw files; never launch')
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args(argv)
    self_test()
    if args.self_test:
        return
    if not (args.output and args.binary and args.identity):
        parser.error('output, --binary and --identity are required')
    output = args.output.resolve()
    binary = args.binary.resolve()
    identity = json.loads(args.identity.read_text(encoding='utf-8'))
    digest = sha_file(binary)
    if identity.get('binary_sha256') != digest:
        raise SystemExit('the identity file does not name this binary')
    base_path = R / 'tools/lever_matrix_r4/configs/PL-baseline.json'
    base = json.loads(base_path.read_text(encoding='utf-8'))
    if 'skin_actual_consumers' in base and base['skin_actual_consumers'] is not False:
        raise SystemExit('PL-baseline skin_actual_consumers is not the false default this oracle pins')
    data_sha, data_rows = data_digest()
    bindings = {
        'comparator_sha256': sha_file(Path(__file__).resolve()),
        'binary_sha256': digest,
        'identity_sha256': sha_file(args.identity),
        'data_sha256': data_sha,
        'base_config_sha256': sha_file(base_path),
    }
    output.mkdir(parents=True, exist_ok=True)
    ledger = Ledger(output / 'ledger.jsonl')
    ledger.append(event='session', argv=sys.argv if argv is None else argv, pid=os.getpid(), binary=str(binary),
                  identity=identity, data=data_rows, reuse=args.reuse, **bindings)
    ctx = dict(output=output, binary=binary, binary_sha256=digest, base=base, ledger=ledger, bindings=bindings, launch=launch)
    maps = [] if args.fixture_only else (args.maps or MAPS)
    unknown = [name for name in maps if name not in MAPS]
    if unknown:
        raise SystemExit(f'unexpected maps {unknown}')
    cases = ([FIXTURE] if args.fixture_only or not args.maps else []) + maps
    rows = [(reuse_case if args.reuse else run_case)(ctx, name) for name in cases]
    witnesses = [row['map'] for row in rows if row['serial_drop'] > 0]
    status = 'PASS' if witnesses else 'FAIL'
    proof = {'status': status, 'witness_maps': witnesses, 'vacuous': not witnesses, 'cases': cases,
             'bindings': bindings, 'prerequisites': PREREQ, 'rows': rows,
             'ledger_sha256': sha_file(output / 'ledger.jsonl')}
    ledger.append(event='summary', status=status, witness_maps=witnesses, cases=cases)
    (output / 'proof').mkdir(exist_ok=True)
    name = 'HOST-FLAG-REUSE.json' if args.reuse else 'HOST-FLAG.json'
    (output / 'proof' / name).write_text(json.dumps(proof, indent=2, default=str) + '\n', encoding='utf-8')
    if not witnesses:
        raise SystemExit('FAIL vacuous: no case moved serial vertices onto workers')
    print(status, 'witness', ', '.join(witnesses), flush=True)


if __name__ == '__main__':
    main()
