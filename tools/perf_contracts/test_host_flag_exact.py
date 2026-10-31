#!/usr/bin/env python3
"""Synthetic controls for the host flag oracle. No game, no binary.

Every case runs the oracle's real main() in a fresh directory. Only subprocess.run (the game
launch) and the git source check are replaced. The fake game writes complete frame, image,
census and PNG records for the requested frame count.

Expectations follow the strict contract, not the fake's labels:
- a genuinely exact pair exits 0 and PASSes;
- coherent candidate/repeat intermediate-image instability exits INCOMPLETE (3) with an
  explicit semantic PASS; a same-arm repeat never turns a candidate difference into PASS;
- a malformed schema, foreign identity, missing mandatory output, semantic mismatch or
  unstable-repeat defect FAILs;
- an input-only presentation retry is bounded and retained.
This is a test of the validator, not game evidence.
"""
import importlib.util, json, subprocess, sys, tempfile
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


def packet_fields():
    return {key: [7] * width for key, width in oracle.FIELD_WIDTHS.items()}


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


def world():
    def group(gid, visible):
        g = {'id': gid, 'visible': visible, 'indices': 9, 'base_element': gid * 9, 'triangles': 3,
             'min_bits': [1, 1, 1], 'max_bits': [2, 2, 2], 'center_bits': [1, 1, 1],
             'model_bits': [9] * 16 if visible else None, 'cull_model_bits': [9] * 16,
             'frustum_matrix_bits': [3] * 16, 'in_frustum': visible,
             'drawn_indices': None, 'pipeline': None, 'low': None, 'uniform_fields': None}
        if visible:
            g.update(drawn_indices=9, pipeline=0, low=True, uniform_fields=world_fields())
        return g
    return {'frame': 0, 'cluster': 3, 'projection_bits': [1] * 16, 'eye_bits': [1, 2, 3], 'time_bits': 0,
            'depth_view_bits': [1] * 6, 'fast_frustum': True, 'draw_order': [7], 'tested': [group(7, True), group(8, False)]}


def frame(index, on, map_name):
    counters = {
        'vita_world_opaque_triangles': 3741, 'vita_world_gpu_draws': 565,
        'fx_registration_batch_puffs': 4 if on else 0, 'fx_registration_batch_runs': 1 if on else 0,
    }
    for n, name in enumerate(sorted(oracle.ELAPSED_COUNTERS)):
        counters[name] = n + (100 if on else 1) + index
    packet = {'geometry': 'g', 'gpu_geometry': 'g', 'gpu_slot': 3 if on else 4, 'uniforms': 'u' * 64,
              'policy': [0], 'texture': 't', 'vertices': 4, 'indices': 6}
    if index == 0:
        packet['uniform_fields'] = packet_fields()
    return {'frame': index, 'map': map_name, 'active': True, 'actor_skin': [], 'state_sha256': 's',
            'selected_lights_bits': [], 'joint_alpha_order': [], 'blend_world_order': [], 'npc_decisions': [],
            'submitted_packets': [packet], 'counters': counters, 'render': None, 'cpu_ms': 1.0 + on, 'spans_ms': {}}


def census(map_name):
    row = {'accepted_frame': 1, 'actors': [{'id': 10, 'serial_vertices': 1, 'worker_vertices': 2}]}
    return {'map': map_name, 'capacity': FRAMES, 'rows': [dict(row) for _ in range(FRAMES)],
            'serial_fallbacks': 0}


class Game:
    """Fake probe binary. mutate(map, on, attempt, parts) edits one launch's output."""

    def __init__(self, binary, mutate=None):
        self.binary, self.mutate, self.calls = binary, mutate, []

    def __call__(self, cmd, cwd, env, stdout, **_):
        d = Path(cwd)
        config = json.loads((d / 'private/vita_renderer.json').read_text())
        on = config[FLAG]
        map_name = cmd[cmd.index('--map') + 1]
        attempt = int(d.name.rsplit('-a', 1)[1])
        self.calls.append((map_name, on, attempt))
        parts = dict(
            config=config, size=[722, 409], view=view(), world=[world()],
            frames=[frame(i, on, map_name) for i in range(FRAMES)],
            images=[{'frame': i, 'sha256': f'{i:064x}', 'width': 960, 'height': 544, 'bytes': 960 * 544 * 4}
                    for i in range(FRAMES)],
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


def run(label, root, mutate=None, maps=('utemple', 'other'), extra=(), trees=None, prepare=None):
    out = root / label
    binary = root / f'{label}.bin'
    binary.write_bytes(b'probe-binary')
    if prepare:
        prepare(out)
    game = Game(binary, mutate)
    trees = iter(trees or [TREE, TREE])
    argv = ['host_flag_exact.py', str(out), '--binary', str(binary), '--flag', FLAG, '--maps', *maps,
            '--prerequisites', '{"particle_producer_reuse": true}', '--witness', 'fx_registration_batch_puffs',
            '--witness', 'fx_registration_batch_runs', '--require-positive', 'utemple',
            '--counter-prefix', 'fx_registration_batch_', '--frames', str(FRAMES), *extra]
    with patch.object(sys, 'argv', argv), patch.object(oracle.subprocess, 'run', game), \
            patch.object(oracle, 'source_tree', lambda _root: next(trees)), \
            patch.object(oracle, 'DATA', str(root)):
        oracle.main()
    return json.loads((out / 'proof/host-flag-exact.json').read_text()), game, out


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


def expect_fail(label, root, **kw):
    try:
        run(label, root, **kw)
    except (AssertionError, SystemExit, KeyError, TypeError, ValueError) as exc:
        if isinstance(exc, SystemExit) and exc.code == 0:
            raise
        text = str(exc)
        print('FAIL_AS_REQUIRED', label, text[:160])
        return text
    raise SystemExit(f'negative control passed: {label}')


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

    # The seven producer-validated elapsed fields differ in every frame of the baseline
    # already (frame()). Any other counter, map or frame is gated.
    expect_fail('unrelated-map frame0 uniform hash', root, mutate=on_map(
        'other', lambda p, g: p['frames'][0]['submitted_packets'][0].update(uniforms='v' * 64)))
    expect_fail('frame0 workload counter', root, mutate=on_map(
        'other', lambda p, g: p['frames'][0]['counters'].update(vita_world_opaque_triangles=3740)))
    expect_fail('future elapsed-looking counter', root, mutate=on_map(
        'utemple', lambda p, g: p['frames'][1]['counters'].update(semantic_future_us=5)))
    expect_fail('frame0 named uniform field', root, mutate=on_map(
        'other', lambda p, g: p['frames'][0]['submitted_packets'][0]['uniform_fields']['projection'].__setitem__(0, 8)))
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

    def rewrite_binary(p, g):
        g.binary.write_bytes(b'rebuilt-binary')
    expect_fail('binary changed during launch', root, mutate=on_map('utemple', rewrite_binary))
    expect_fail('source changed during oracle', root,
                trees=[TREE, dict(TREE, tree='x' * 40)])

    def stale(out):
        (out / 'host').mkdir(parents=True)
        (out / 'host/cached-run.log').write_text('SIM_MAPS_FRAME {}\n')
    expect_fail('existing output directory (stale cache)', root, prepare=stale)

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
    ledger = [json.loads(line) for line in (out / 'proof/attempts.jsonl').read_text().splitlines()]
    assert any(row.get('classification') == 'prerequisite-mismatch' and row['map'] == 'other' for row in ledger)

    def always_moves(m, on, attempt, p, g):
        if m == 'other' and on:
            p['images'][2].update(width=961, bytes=961 * 544 * 4)
    text = expect_fail('input never matches', root, mutate=always_moves, extra=('--max-attempts', '3'))
    assert 'no matched renderer prerequisite' in text, text
    expect_fail('output mismatch is not retried', root, mutate=lambda m, on, a, p, g: (
        m == 'other' and on and a == 0 and p['frames'][2]['counters'].update(vita_world_gpu_draws=1)))

    receipt, game, _ = expect_pass('repeated OFF', root, extra=('--repeat-off', 'utemple'))
    assert receipt['maps'][0]['repeat_off']['frame_digest'], receipt['maps'][0]
    assert receipt['maps'][0]['stability_observation']['explicit'] is True, receipt['maps'][0]
    expect_fail('repeated OFF semantic mismatch', root, extra=('--repeat-off', 'utemple'),
                mutate=lambda m, on, a, p, g: (m == 'utemple' and not on and a >= REPEAT_ATTEMPT
                                               and p['frames'][2].update(state_sha256='foreign-state')))
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


if __name__ == '__main__':
    main()
