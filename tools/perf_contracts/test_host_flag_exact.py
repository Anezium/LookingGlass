#!/usr/bin/env python3
"""Synthetic controls for the host flag oracle. No game, no binary.

Every case runs the oracle's real main() in a fresh directory. Only subprocess.run (the game
launch) and the git source check are replaced. The fake game writes complete producer-shaped
frame, image, view, world, census and PNG records for the requested frame count: typed
semantic arrays, named frame-0 uniform slices bound to the uniform hash, a full phase_census
schema with accepted_frame/sequence/measured_frame/actor/fallback evidence and warmup
alignment. Its nullable fields follow the producers (camera viewport, world cluster, texture,
actor id) and vacuous arrays are exercised explicitly.

Expectations follow the strict contract, not the fake's labels:
- a genuinely exact producer-shaped pair exits 0 and PASSes;
- coherent candidate/repeat intermediate-image instability exits INCOMPLETE (3) with an
  explicit semantic PASS; a same-arm repeat never turns a candidate difference into PASS;
- a malformed schema, malformed launch, foreign identity, missing mandatory output, semantic
  mismatch or unstable-repeat defect FAILs with a terminal FAIL receipt and the failed
  attempt in the ledger;
- an input-only presentation retry is bounded and retained.
This is a test of the validator, not game evidence.
"""
import importlib.util, json, struct, subprocess, sys, tempfile
from pathlib import Path
from unittest.mock import patch

MODULE = Path(__file__).with_name('host_flag_exact.py')
spec = importlib.util.spec_from_file_location('host_flag_exact', MODULE)
oracle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(oracle)

FLAG = 'fx_registration_batch'
FRAMES = 65
REPEAT_ATTEMPT = 8
TREE = dict(head='h' * 40, tree='t' * 40, porcelain=' M docs/RCHESS1.md\n')
LOAD = ('[lg/load-diagnostic] reason=startup-entry stage=prepare-3d-returned framebuffer_binding=0 '
        'viewport=[0, 0, {w}, {h}] framebuffer_status=0x8cd5 gl_error_before=0x0 gl_error_after=0x0 '
        'scale_target=Some([{w}, {h}]) render_scale=0.75')
REGIONS = list(oracle.CENSUS_REGIONS)
CALLS = {}


def digest(data):
    import hashlib
    return hashlib.sha256(data).hexdigest()


def packet_fields():
    return {key: [7] * width for key, width in oracle.FIELD_WIDTHS.items()}


def uniform_hash(fields):
    bits = [value for key in oracle.FIELD_WIDTHS for value in fields[key]]
    return digest(struct.pack('<147I', *bits))


def world_fields():
    fields = {key: [5] * width for key, width in oracle.WORLD_FIELD_WIDTHS.items()}
    fields['all'] = [bit for key in oracle.WORLD_FIELD_WIDTHS for bit in fields[key]]
    return fields


def view():
    return {
        'frame': 0, 'camera_viewport': None, 'requested_window': [960, 544],
        'screen_bits': [1148190720, 1141374976], 'gl_viewport': [0, 0, 722, 409], 'scale_target': [722, 409],
        'projection_bits': [1] * 16, 'eye_bits': [1, 2, 3], 'target_bits': [4, 5, 6], 'up_bits': [0, 0, 1],
        'depth_view_bits': [1] * 6, 'camera_aspect_bits': 1071877689, 'fovy_bits': 1065619849,
        'near_bits': 1073741824, 'far_bits': 1199570944, 'time_bits': 0,
    }


def world(cluster=3):
    def group(gid, visible):
        g = {'id': gid, 'visible': visible, 'indices': 9, 'base_element': gid * 9, 'triangles': 3,
             'min_bits': [1, 1, 1], 'max_bits': [2, 2, 2], 'center_bits': [1, 1, 1],
             'model_bits': [9] * 16 if visible else None, 'cull_model_bits': [9] * 16,
             'frustum_matrix_bits': [3] * 16, 'in_frustum': visible,
             'drawn_indices': None, 'pipeline': None, 'low': None, 'uniform_fields': None}
        if visible:
            g.update(drawn_indices=9, pipeline=0, low=True, uniform_fields=world_fields())
        return g
    return {'frame': 0, 'cluster': cluster, 'projection_bits': [1] * 16, 'eye_bits': [1, 2, 3], 'time_bits': 0,
            'depth_view_bits': [1] * 6, 'fast_frustum': True, 'draw_order': [7], 'tested': [group(7, True), group(8, False)]}


def frame(index, on, map_name):
    counters = {
        'vita_world_opaque_triangles': 3741, 'vita_world_gpu_draws': 565,
        'fx_registration_batch_puffs': 4 if on else 0, 'fx_registration_batch_runs': 1 if on else 0,
    }
    for n, name in enumerate(sorted(oracle.ELAPSED_COUNTERS)):
        counters[name] = n + (100 if on else 1) + index
    fields = packet_fields() if index == 0 else None
    packet = {'geometry': digest(b'geometry'), 'gpu_geometry': digest(b'geometry'),
              'gpu_slot': 3 if on else 4, 'uniforms': uniform_hash(fields) if fields else digest(b'uniforms'),
              'policy': [0, 0, 1], 'texture': digest(b'texture'), 'vertices': 4, 'indices': 6}
    if fields is not None:
        packet['uniform_fields'] = fields
    return {'frame': index, 'map': map_name, 'active': True,
            'actor_skin': [{'actor': 9, 'surface': 0, 'sha256': digest(b'skin-0')},
                           {'actor': None, 'surface': 1, 'sha256': digest(b'skin-1')}],
            'state_sha256': digest(b'state' + str(index).encode()),
            'selected_lights_bits': [[1, 2, 3, 0, 0, 0, 0, 0, 1]],
            'joint_alpha_order': [[0, 15, 0], [0, 16, 0]],
            'blend_world_order': [[695, True], [698, False]],
            'npc_decisions': [{'actor': '0:alice:alice', 'observations': 3, 'sha256': digest(b'decision')}],
            'submitted_packets': [packet], 'counters': counters, 'render': None,
            'cpu_ms': 1.0 + on, 'spans_ms': {'light_selection': 0.1}}


def census_row(index):
    total = 1000 + index
    coverage = 500
    return {
        'accepted_frame': index + 1, 'sequence': index + 1,
        'measured_frame': None if index < oracle.CENSUS_WARMUP_FRAMES else index - oracle.CENSUS_WARMUP_FRAMES + 1,
        'frame_total_ns': total, 'bracket_union_ns': coverage,
        'unbracketed_residual_ns': total - coverage,
        'inclusive_ns': [0] * len(REGIONS), 'exclusive_ns': [0] * len(REGIONS),
        'calls': [0] * len(REGIONS), 'parent_masks': [0] * len(REGIONS),
        'static_traversal_ns': [0] * 5, 'static_traversal_counts': [[0] * 7 for _ in range(5)],
        'sweep_count': [0] * 5, 'sweep_ns': [0] * 5,
        'acoustics_world_sweep_count': 0, 'acoustics_world_sweep_ns': 0,
        'static_queries_total_hit1_hit4_hit16': [[0] * 4 for _ in range(3)],
        'static_cache_hit_miss_eviction': [[0] * 3 for _ in range(3)],
        'live_table_evictions': 0, 'hierarchy_errors': 0,
        'actors': [{'id': 10, 'serial_vertices': 1, 'worker_vertices': 2, 'serial_reasons': 0,
                    'reason_vertices': [0] * len(oracle.CENSUS_SERIAL_REASONS)}],
        'worker_skin_ns': [0, 0], 'serial_fallbacks': 0,
    }


def census(map_name):
    return {
        'schema': 3, 'actor_capacity': 128,
        'serial_reasons': list(oracle.CENSUS_SERIAL_REASONS), 'audio_static_cache': False,
        'map': map_name, 'regions': REGIONS, 'owners': list(oracle.CENSUS_OWNERS),
        'static_count_columns': list(oracle.CENSUS_STATIC_COLUMNS),
        'query_classes': list(oracle.CENSUS_QUERY_CLASSES),
        'rows': [census_row(i) for i in range(FRAMES)], 'capacity': FRAMES,
        'table_entries': 4096, 'table_probes': 16, 'serial_fallbacks': 0,
        'parent_mask_frame_bit': len(REGIONS),
        'clock': 'monotonic elapsed nanoseconds; includes preemption and API waits; no GPU timing',
        'residual': 'frame_total minus union of root brackets; nested inclusive values must never be summed',
        'query_rule': 'exact scene and endpoint bits; previous 1/4/16 attempted frames',
        'observer_threshold_ms': 1.0,
    }


class Game:
    """Fake probe binary. mutate(map, on, attempt, parts, game) edits one launch's output."""

    def __init__(self, binary, mutate=None):
        self.binary, self.mutate, self.calls = binary, mutate, []

    def __call__(self, cmd, cwd, env, stdout, timeout=None, **_):
        d = Path(cwd)
        config = json.loads((d / 'private/vita_renderer.json').read_text())
        on = config[FLAG]
        map_name = cmd[cmd.index('--map') + 1]
        attempt = int(d.name.rsplit('-a', 1)[1])
        self.calls.append((map_name, on, attempt))
        parts = dict(
            config=config, size=[722, 409], view=view(), world=[world()],
            frames=[frame(i, on, map_name) for i in range(FRAMES)],
            images=[{'frame': i, 'sha256': digest(b'image' + str(i).encode()), 'width': 960, 'height': 544,
                     'bytes': 960 * 544 * 4} for i in range(FRAMES)],
            census=census(map_name), opening_png=b'png0', final_png=b'pngN', exit=0, extra=[])
        if self.mutate:
            self.mutate(map_name, on, attempt, parts, self)
        lines = ['[lg/vita-renderer] config ' + json.dumps(parts['config']),
                 '[lg/vita-renderer] actor_single_prepare active=true',
                 LOAD.format(w=parts['size'][0], h=parts['size'][1])]
        if parts['view'] is not None:
            lines.append('SIM_MAPS_VIEW ' + json.dumps(parts['view']))
        lines += ['SIM_MAPS_WORLD ' + json.dumps(w) for w in parts['world']]
        for f, image in zip(parts['frames'], parts['images'] + [None] * FRAMES):
            lines.append('SIM_MAPS_FRAME ' + json.dumps(f))
            if image is not None:
                lines.append('SKIN_EXACT_IMAGE ' + json.dumps(image))
        lines += parts['extra']
        stdout.write('\n'.join(lines) + '\n')
        if parts['opening_png'] is not None:
            (d / 'private/opening-frame.png').write_bytes(parts['opening_png'])
        (d / 'final.png').write_bytes(parts['final_png'])
        (d / f'{map_name}-census-{FRAMES}.json').write_text(json.dumps(parts['census']))
        return subprocess.CompletedProcess(cmd, parts['exit'])


def run(label, root, mutate=None, maps=('utemple', 'other'), extra=(), trees=None, prepare=None,
        launch_timeout=None):
    out = root / label
    binary = root / f'{label}.bin'
    binary.write_bytes(b'probe-binary')
    if prepare:
        prepare(out)
    game = Game(binary, mutate)
    CALLS[label] = game.calls
    trees = iter(trees or [TREE, TREE])
    argv = ['host_flag_exact.py', str(out), '--binary', str(binary), '--flag', FLAG, '--maps', *maps,
            '--prerequisites', '{"particle_producer_reuse": true}', '--witness', 'fx_registration_batch_puffs',
            '--witness', 'fx_registration_batch_runs', '--require-positive', 'utemple',
            '--counter-prefix', 'fx_registration_batch_', '--frames', str(FRAMES), *extra]
    if launch_timeout is not None:
        argv += ['--launch-timeout', str(launch_timeout)]
    with patch.object(sys, 'argv', argv), patch.object(oracle.subprocess, 'run', game), \
            patch.object(oracle, 'source_tree', lambda _root: next(trees)), \
            patch.object(oracle, 'DATA', str(root)):
        oracle.main()
    return json.loads((out / 'proof/host-flag-exact.json').read_text()), game, out


def ledger(out):
    return [json.loads(line) for line in (out / 'proof/attempts.jsonl').read_text().splitlines()]


def expect_pass(label, root, **kw):
    receipt, game, out = run(label, root, **kw)
    assert receipt['status'] == 'PASS', receipt
    assert receipt['semantic_status'] == 'PASS' and receipt['image_status'] == 'EXACT', receipt
    print('PASS_AS_REQUIRED', label, 'launches', len(game.calls))
    return receipt, game, out


def expect_incomplete(label, root, **kw):
    out = root / label
    try:
        run(label, root, **kw)
    except SystemExit as exc:
        assert exc.code == oracle.INCOMPLETE_EXIT, exc.code
    else:
        raise SystemExit(f'incomplete control passed exactly: {label}')
    receipt = json.loads((out / 'proof/host-flag-exact.json').read_text())
    assert receipt['status'] == 'INCOMPLETE', receipt
    assert receipt['semantic_status'] == 'PASS' and receipt['image_status'] == 'INCOMPLETE', receipt
    assert receipt['incomplete_maps'], receipt
    assert all(row['semantic_status'] == 'PASS' for row in receipt['maps']), receipt['maps']
    print('INCOMPLETE_AS_REQUIRED', label, receipt['incomplete_maps'])
    return receipt


def expect_fail(label, root, reason=None, **kw):
    try:
        run(label, root, **kw)
    except SystemExit as exc:
        if exc.code in (0, oracle.INCOMPLETE_EXIT):
            raise SystemExit(f'negative control ended with exit {exc.code}: {label}')
    except AssertionError:
        pass
    else:
        raise SystemExit(f'negative control passed: {label}')
    out = root / label
    receipt = json.loads((out / 'proof/host-flag-exact.json').read_text())
    assert receipt['status'] == 'FAIL', (label, receipt['status'])
    assert receipt.get('error'), (label, 'no recorded error')
    if reason is not None:
        assert reason in json.dumps(receipt['error']), (label, reason, receipt['error'])
    print('FAIL_AS_REQUIRED', label, json.dumps(receipt['error'])[:150])
    return receipt, out


def on_map(map_name, edit):
    def mutate(m, on, attempt, parts, game):
        if m == map_name and on:
            edit(parts, game)
    return mutate


def unstable_repeat(image_index, threshold=REPEAT_ATTEMPT):
    """Candidate difference in ON attempt 0 plus an unrelated repeat difference."""
    def mutate(m, on, attempt, parts, game):
        if m != 'other':
            return
        if on and attempt == 0:
            parts['images'][1]['sha256'] = 'a' * 64
        if (not on) and attempt >= threshold:
            parts['images'][image_index]['sha256'] = 'b' * 64
    return mutate


def main():
    root = Path(tempfile.mkdtemp(prefix='host-flag-exact-synthetic-'))
    receipt, game, _ = expect_pass('baseline', root)
    assert [m['matched_attempt'] for m in receipt['maps']] == [0, 0]
    assert receipt['witness_on_total'] == {'fx_registration_batch_puffs': 2 * 4 * FRAMES,
                                           'fx_registration_batch_runs': 2 * FRAMES}

    # Legitimate nullable producer fields and vacuous arrays still pass.
    def nullable(parts, game):
        parts['world'][0]['cluster'] = None
        for row in parts['frames']:
            row['actor_skin'] = []
            row['selected_lights_bits'] = []
            row['joint_alpha_order'] = []
            row['blend_world_order'] = []
            row['npc_decisions'] = []
            for packet in row['submitted_packets']:
                packet['texture'] = None
    expect_pass('nullable producer fields', root, mutate=lambda m, on, a, p, g: nullable(p, g))

    # The seven producer-validated elapsed fields differ in every frame of the baseline
    # already (frame()). Any other counter, map or frame is gated.
    expect_fail('unrelated-map frame0 uniform hash', root, mutate=on_map(
        'other', lambda p, g: p['frames'][0]['submitted_packets'][0].update(uniforms='v' * 64)))
    expect_fail('frame0 workload counter', root, mutate=on_map(
        'other', lambda p, g: p['frames'][0]['counters'].update(vita_world_opaque_triangles=3740)))
    expect_fail('future elapsed-looking counter', root, mutate=on_map(
        'utemple', lambda p, g: p['frames'][1]['counters'].update(semantic_future_us=5)))
    expect_fail('frame0 named uniform field', root, reason='uniform',
                mutate=on_map('other', lambda p, g: p['frames'][0]['submitted_packets'][0]['uniform_fields']['projection'].__setitem__(0, 8)))
    expect_fail('frame0 fovy', root, mutate=on_map('other', lambda p, g: p['view'].update(fovy_bits=1065619850)))
    expect_fail('world group culled', root, mutate=on_map(
        'other', lambda p, g: p['world'][0]['tested'][0].update(
            visible=False, in_frustum=False, model_bits=None, drawn_indices=None, pipeline=None, low=None,
            uniform_fields=None)
        or p['world'][0].update(draw_order=[])))
    expect_fail('world draw uniform', root, mutate=on_map(
        'other', lambda p, g: p['world'][0]['tested'][0]['uniform_fields'].update(
            mvp=[6] * 16, all=[6] * 16 + [5] * 127)))
    expect_fail('intermediate image with a stable same arm', root, mutate=on_map(
        'other', lambda p, g: p['images'][1].update(sha256='f' * 64)))

    # Coherent candidate instability, with a fully matched same-arm repeat, is INCOMPLETE.
    # The repeat is an observation, never a waiver.
    receipt = expect_incomplete('same-arm unstable intermediate image', root, mutate=unstable_repeat(1))
    row = receipt['maps'][1]
    assert row['image_differences'] == [1] and row['stability_observation']['differing'] == [1], row
    assert row['stability_observation']['validated_against_off'] is True, row

    # Unrelated-frame and arbitrary-magnitude candidate differences stay incomplete: the
    # validator cannot and does not infer that any candidate pixel is benign.
    receipt = expect_incomplete('unrelated frame noise', root, mutate=unstable_repeat(3))
    assert receipt['maps'][1]['image_differences'] == [1]
    assert receipt['maps'][1]['stability_observation']['differing'] == [3]
    expect_incomplete('unrelated multi-frame noise', root, mutate=lambda m, on, a, p, g: (
        m == 'other' and ((on and a == 0 and [im.update(sha256='c' * 64) for i, im in enumerate(p['images']) if i in (1, 2, 4)])
                          or ((not on) and a >= REPEAT_ATTEMPT and p['images'][3].update(sha256='b' * 64)))))

    # Mandatory opening/final/census checks cannot be skipped by an intermediate mismatch
    # plus unrelated repeat noise.
    def combined(parts_edit):
        def mutate(m, on, attempt, parts, game):
            if m != 'other':
                return
            if on and attempt == 0:
                parts['images'][1]['sha256'] = 'a' * 64
                parts_edit(parts)
            if (not on) and attempt >= REPEAT_ATTEMPT:
                parts['images'][3]['sha256'] = 'b' * 64
        return mutate
    expect_fail('combined opening bypass', root, mutate=combined(lambda p: p.update(opening_png=b'CHANGED OPENING')))
    expect_fail('combined final bypass', root, mutate=combined(lambda p: p.update(final_png=b'CHANGED FINAL')))
    expect_fail('combined census bypass', root, mutate=combined(
        lambda p: p['census']['rows'][60]['actors'][0].update(serial_vertices=999)))
    expect_fail('opening frame image', root, mutate=on_map('other', lambda p, g: p.update(opening_png=b'png1')))
    expect_fail('final image', root, mutate=on_map('other', lambda p, g: p.update(final_png=b'png1')))

    # Complete schema: missing, malformed, foreign or empty records fail before selection.
    expect_fail('missing images', root, mutate=lambda m, on, a, p, g: p.update(images=[]))
    expect_fail('missing opening png', root, mutate=on_map('utemple', lambda p, g: p.update(opening_png=None)))
    expect_fail('missing frame0 view', root, mutate=lambda m, on, a, p, g: p.update(view=None))
    expect_fail('missing frame0 uniform fields', root, mutate=on_map(
        'utemple', lambda p, g: p['frames'][0]['submitted_packets'][0].pop('uniform_fields')))
    expect_fail('ON malformed image schema', root, mutate=on_map(
        'other', lambda p, g: p['images'][1].update(frame=999, sha256='not-a-hash', bytes=-1)))
    expect_fail('both arms non-hex hash', root, mutate=lambda m, on, a, p, g: [
        image.update(sha256='z' * 64) for image in p['images']])
    expect_fail('both arms wrong RGBA bytes', root, mutate=lambda m, on, a, p, g: [
        image.update(bytes=1) for image in p['images']])
    expect_fail('both arms foreign frame map', root, mutate=lambda m, on, a, p, g: [
        row.update(map='foreign-map') for row in p['frames']])
    expect_fail('frame not active', root, mutate=on_map('utemple', lambda p, g: p['frames'][2].update(active=False)))
    expect_fail('image frame sequence', root, mutate=on_map('utemple', lambda p, g: p['images'][3].update(frame=4)))
    expect_fail('empty equal census rows', root, mutate=lambda m, on, a, p, g: p['census'].update(rows=[]))
    expect_fail('foreign census map', root, mutate=lambda m, on, a, p, g: p['census'].update(map='foreign-map'))
    expect_fail('vacuous required witness', root, mutate=on_map(
        'utemple', lambda p, g: [f['counters'].update(fx_registration_batch_puffs=0) for f in p['frames']]))
    expect_fail('serial fallback', root, mutate=on_map(
        'other', lambda p, g: p['extra'].append('[lg/skin] serial fallback actor=1')))
    expect_fail('nonzero exit', root, mutate=on_map('other', lambda p, g: p.update(exit=1)))

    # R3-1: equal malformed typed records must fail, never establish an exact PASS.
    expect_fail('census sequence 999 in both arms', root, reason='accepted_frame', mutate=lambda m, on, a, p, g: [
        row.update(accepted_frame=999, sequence=999, measured_frame=999) for row in p['census']['rows']])
    expect_fail('census bool sequence', root, reason='accepted_frame', mutate=lambda m, on, a, p, g: [
        row.update(accepted_frame=True, sequence=True) for row in p['census']['rows'][:1]])
    def census_missing_identity(m, on, attempt, parts, game):
        for row in parts['census']['rows']:
            for key in ('accepted_frame', 'sequence', 'measured_frame'):
                row.pop(key)
            for actor in row['actors']:
                actor.pop('id')
    expect_fail('census missing identity', root, reason='schema', mutate=census_missing_identity)
    expect_fail('census missing fallback evidence', root, reason='schema',
                mutate=lambda m, on, a, p, g: p['census'].pop('serial_fallbacks'))
    expect_fail('census bool actor identity', root, mutate=lambda m, on, a, p, g: [
        actor.update(id=True) for row in p['census']['rows'] for actor in row['actors']])
    expect_fail('census bad warmup alignment', root, reason='measured_frame', mutate=lambda m, on, a, p, g: [
        row.update(measured_frame=None) for row in p['census']['rows'][60:61]])
    expect_fail('state hash not hex', root, mutate=lambda m, on, a, p, g: [
        row.update(state_sha256='z' * 64) for row in p['frames']])
    expect_fail('packet uniform hash not hex', root, mutate=lambda m, on, a, p, g: [
        packet.update(uniforms='z' * 64) for row in p['frames'] for packet in row['submitted_packets']])
    expect_fail('null semantic arrays', root, reason='actor_skin', mutate=lambda m, on, a, p, g: [
        row.update({k: None for k in ('actor_skin', 'selected_lights_bits', 'joint_alpha_order',
                                      'blend_world_order', 'npc_decisions')}) for row in p['frames']])
    expect_fail('world bool frame', root, reason='frame', mutate=lambda m, on, a, p, g: p['world'][0].update(frame=False))
    expect_fail('world null record', root, reason='record', mutate=lambda m, on, a, p, g: p.update(world=[None]))
    expect_fail('decoded text is not an object', root, mutate=lambda m, on, a, p, g: [
        row.update(actor_skin=[{'actor': 9, 'surface': 0, 'sha256': 7}]) for row in p['frames']])

    # R3-2: a malformed launch is terminal before pair or repeat selection.
    def pair_prereq_missing_uniform(m, on, attempt, parts, game):
        if m == 'other' and on and attempt == 0:
            parts['view']['screen_bits'][0] += 1
            parts['frames'][0]['submitted_packets'][0].pop('uniform_fields')
    receipt, out = expect_fail('pair prerequisite malformed launch', root, reason='invalid record',
                               mutate=pair_prereq_missing_uniform)
    assert [c for c in game_calls('pair prerequisite malformed launch', root) if c[0] == 'other'] == [
        ('other', False, 0), ('other', True, 0)], game_calls('pair prerequisite malformed launch', root)
    rows = ledger(out)
    assert any(row.get('map') == 'other' and row.get('arm') is True and row.get('attempt') == 0
               and row.get('classification') == 'invalid-record' for row in rows), rows
    assert any(row.get('classification') == 'launch-start' and row.get('map') == 'other'
               and row.get('arm') is True and row.get('attempt') == 0 for row in rows), rows

    def repeat_prereq_missing_uniform(m, on, attempt, parts, game):
        if m != 'other':
            return
        if on and attempt == 0:
            parts['images'][1]['sha256'] = 'a' * 64
        if not on and attempt >= REPEAT_ATTEMPT:
            parts['images'][3]['sha256'] = 'b' * 64
            if attempt == REPEAT_ATTEMPT:
                parts['view']['screen_bits'][0] += 1
                parts['frames'][0]['submitted_packets'][0].pop('uniform_fields')
    receipt, out = expect_fail('repeat prerequisite malformed launch', root, reason='invalid record',
                               mutate=repeat_prereq_missing_uniform)
    calls = game_calls('repeat prerequisite malformed launch', root)
    assert ('other', False, REPEAT_ATTEMPT) in calls, calls
    assert ('other', False, REPEAT_ATTEMPT + 1) not in calls, calls
    rows = ledger(out)
    assert any(row.get('map') == 'other' and row.get('arm') is False and row.get('attempt') == REPEAT_ATTEMPT
               and row.get('classification') == 'invalid-record' for row in rows), rows

    # R3-3: every operational failure closes one terminal FAIL receipt, never stale RUNNING.
    def exec_error(m, on, attempt, parts, game):
        if m == 'other' and on and attempt == 0:
            raise PermissionError('synthetic exec denied')
    receipt, out = expect_fail('launch process error', root, reason='launch process error', mutate=exec_error)
    assert receipt['status'] == 'FAIL'
    rows = ledger(out)
    assert any(row.get('map') == 'other' and row.get('arm') is True and row.get('attempt') == 0
               and row.get('classification') == 'launch-error' for row in rows), rows
    assert any(row.get('classification') == 'launch-start' and row.get('map') == 'other'
               and row.get('attempt') == 0 for row in rows), rows

    expect_fail('malformed json', root, reason='decode error', mutate=on_map(
        'other', lambda p, g: p['extra'].append('SIM_MAPS_FRAME {broken')))

    def missing_config(parts, game):
        parts['config'] = dict(parts['config'])
        parts['config'].pop('particle_producer_reuse')
    expect_fail('missing printed config key', root, reason='missing prerequisite',
                mutate=on_map('other', missing_config))

    def timeout(m, on, attempt, parts, game):
        raise subprocess.TimeoutExpired('probe', 0.5)
    receipt, out = expect_fail('bounded launch timeout', root, reason='launch timeout',
                               mutate=timeout, extra=('--launch-timeout', '0.5'))
    assert receipt['status'] == 'FAIL'
    assert any(row.get('classification') == 'timeout' for row in ledger(out)), ledger(out)

    def rewrite_binary(p, g):
        g.binary.write_bytes(b'rebuilt-binary')
    expect_fail('binary changed during launch', root, mutate=on_map('utemple', rewrite_binary))
    expect_fail('source changed during oracle', root,
                trees=[TREE, dict(TREE, tree='x' * 40)])

    def stale(out_dir):
        (out_dir / 'host').mkdir(parents=True)
        (out_dir / 'host/cached-run.log').write_text('SIM_MAPS_FRAME {}\n')
    out = root / 'existing output directory (stale cache)'
    binary = root / 'stale.bin'
    binary.write_bytes(b'probe-binary')
    stale(out)
    argv = ['host_flag_exact.py', str(out), '--binary', str(binary), '--flag', FLAG,
            '--maps', 'utemple', '--prerequisites', '{"particle_producer_reuse": true}',
            '--witness', 'fx_registration_batch_puffs', '--require-positive', 'utemple',
            '--counter-prefix', 'fx_registration_batch_', '--frames', str(FRAMES)]
    with patch.object(sys, 'argv', argv), patch.object(oracle, 'DATA', str(root)):
        try:
            oracle.main()
        except SystemExit as exc:
            assert 'refuse reuse' in str(exc), exc
        else:
            raise SystemExit('stale output directory was reused')

    # Presentation input: a pair whose frame-0 window differs is kept and not compared.
    def first_pair_moves(m, on, attempt, p, g):
        if m == 'other' and on and attempt == 0:
            p['size'] = [721, 408]
            p['view'].update(gl_viewport=[0, 0, 721, 408], scale_target=[721, 408], fovy_bits=1065619999)
            p['frames'][0]['counters'].update(vita_world_opaque_triangles=3740)
    receipt, game, out = expect_pass('input mismatch retried in a new directory', root, mutate=first_pair_moves)
    assert receipt['maps'][1]['matched_attempt'] == 1, receipt['maps'][1]
    kept = sorted(p.name for p in (out / 'host/host-flag-exact').iterdir() if p.name.startswith('other-'))
    assert kept == [f'other-{FLAG}-0-a0', f'other-{FLAG}-0-a1', f'other-{FLAG}-1-a0', f'other-{FLAG}-1-a1'], kept
    rows = ledger(out)
    assert any(row.get('classification') == 'prerequisite-mismatch' and row['map'] == 'other' for row in rows)

    def always_moves(m, on, attempt, p, g):
        if m == 'other' and on:
            p['images'][2].update(width=961, bytes=961 * 544 * 4)
    receipt, _ = expect_fail('input never matches', root, reason='no matched renderer prerequisite',
                             mutate=always_moves, extra=('--max-attempts', '3'))
    expect_fail('output mismatch is not retried', root, mutate=lambda m, on, a, p, g: (
        m == 'other' and on and a == 0 and p['frames'][2]['counters'].update(vita_world_gpu_draws=1)))

    receipt, game, _ = expect_pass('repeated OFF', root, extra=('--repeat-off', 'utemple'))
    assert receipt['maps'][0]['repeat_off']['frame_digest'], receipt['maps'][0]
    assert receipt['maps'][0]['stability_observation']['explicit'] is True, receipt['maps'][0]
    expect_fail('repeated OFF semantic mismatch', root, extra=('--repeat-off', 'utemple'),
                mutate=lambda m, on, a, p, g: (m == 'utemple' and not on and a >= REPEAT_ATTEMPT
                                               and p['frames'][2].update(state_sha256='a' * 64)))
    expect_fail('repeated OFF foreign map', root, extra=('--repeat-off', 'utemple'),
                mutate=lambda m, on, a, p, g: (m == 'utemple' and not on and a >= REPEAT_ATTEMPT
                                               and [row.update(map='foreign-map') for row in p['frames']]))
    expect_fail('repeated OFF malformed schema', root, extra=('--repeat-off', 'utemple'),
                mutate=lambda m, on, a, p, g: (m == 'utemple' and not on and a >= REPEAT_ATTEMPT
                                               and p['images'][3].update(frame=999, sha256='x', bytes=-1)))
    expect_fail('repeated OFF census mismatch', root, extra=('--repeat-off', 'utemple'),
                mutate=lambda m, on, a, p, g: (m == 'utemple' and not on and a >= REPEAT_ATTEMPT
                                               and p['census']['rows'][60]['actors'][0].update(serial_vertices=9)))
    expect_fail('repeated OFF differs', root, extra=('--repeat-off', 'utemple'),
                mutate=lambda m, on, a, p, g: (m == 'utemple' and not on and a >= REPEAT_ATTEMPT
                                               and p.update(final_png=b'other')))
    print('PASS synthetic oracle controls', root)


def game_calls(label, root):
    """The fake game's call list for one label, recorded before the run starts."""
    return CALLS.get(label, [])


if __name__ == '__main__':
    main()
