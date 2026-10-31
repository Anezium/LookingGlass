"""Retained cache/oracle controls. Synthetic fixtures only; no game run.

Covers the r3 residuals:
- a recorded/requested uniform-bit sidecar cannot change, disappear or appear unbound;
- the effective config and exact oracle identity are part of reuse;
- aggregate uniform hashes stay in every packet comparison with or without sidecars;
- sidecar width, unique (frame,index) coverage and hash linkage are validated;
- a late change of the scaled 3D viewport is rejected.
"""
import hashlib, json, sys, tempfile
from pathlib import Path

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent))
import host_flag_exact as oracle


def identity():
    return dict(executable_sha256='a' * 64, source_revision='rev', source_manifest_sha256='b' * 64,
                toolchain='nightly-2026-10-11', features='native_timing', rustflags='--cfg observer_free_B')


def packet(uniforms='0' * 64):
    return dict(geometry='g', gpu_geometry='g', gpu_slot=0, vertices=3, indices=3,
                uniforms=uniforms, policy=[1, 2], texture=None)


def frame(index, units=None):
    return dict(frame=index, submitted_packets=[packet(u) for u in (units or ['0' * 64])])


def bits(seed=0):
    return [seed + i for i in range(oracle.UNIFORM_WIDTH)]


def write_sidecar(directory, rows):
    path = Path(directory) / 'uniform-bits.jsonl'
    path.write_text(''.join(json.dumps(dict(frame=f, index=i, bits=b)) + '\n' for (f, i), b in rows.items()))
    return path


def cache_dir(tmp, sidecar=True, recorded=True, expected=True):
    directory = Path(tmp) / 'run'
    (directory / 'private').mkdir(parents=True)
    (directory / 'run.log').write_text('log\n')
    (directory / 'jlair1-census-180.json').write_text('{}\n')
    (directory / 'final.png').write_bytes(b'png')
    (directory / 'private/vita_renderer.json').write_text('{}\n')
    receipt = {'log_sha256': oracle.sha(directory / 'run.log'),
               'census_sha256': oracle.sha(directory / 'jlair1-census-180.json'),
               'final_png_sha256': oracle.sha(directory / 'final.png'),
               'config_sha256': oracle.sha(directory / 'private/vita_renderer.json'),
               'effective_config_sha256': 'e' * 64,
               'uniform_bits_expected': expected,
               'oracle_version': oracle.ORACLE_VERSION}
    if sidecar:
        path = write_sidecar(directory, {(0, 0): bits()})
        if recorded:
            receipt['uniform_bits_sha256'] = oracle.sha(path)
    (directory / 'run-receipt.json').write_text(json.dumps(receipt))
    return directory


def test_mismatch_keeps_the_old_directory(tmp):
    expected = oracle.receipt_core(identity(), {'actor_deferred_layers': False},
                                   {'LOOKING_GLASS_SIM_MAPS_PROBE': '1'}, ['--map', 'jlair1'])
    stale = dict(expected, executable_sha256='c' * 64, exit_code=0, log_sha256='d' * 64,
                 census_sha256='e' * 64, final_png_sha256='f' * 64, config_sha256='1' * 64)
    digest = hashlib.sha256(json.dumps(expected, sort_keys=True).encode()).hexdigest()[:20]
    old = tmp / 'host' / f'jlair1-off-{digest}'
    old.mkdir(parents=True)
    (old / 'run.log').write_text('old log\n')
    (old / 'jlair1-census-180.json').write_text('{}\n')
    (old / 'final.png').write_bytes(b'png')
    (old / 'private').mkdir()
    (old / 'private/vita_renderer.json').write_text('{}\n')
    (old / 'run-receipt.json').write_text(json.dumps(stale) + '\n')
    chosen, reuse = oracle.allocate_run_dir(tmp, 'jlair1', 'off', expected)
    assert reuse is False
    assert chosen != old
    assert (old / 'run.log').read_text() == 'old log\n'
    assert not chosen.exists()


def test_matching_receipt_reuses(tmp):
    expected = oracle.receipt_core(identity(), {'actor_deferred_layers': True},
                                   {'LOOKING_GLASS_SIM_MAPS_PROBE': '1'}, ['--map', 'wchess2'])
    digest = hashlib.sha256(json.dumps(expected, sort_keys=True).encode()).hexdigest()[:20]
    directory = tmp / 'host' / f'wchess2-on-{digest}'
    (directory / 'private').mkdir(parents=True)
    (directory / 'run.log').write_text('log\n')
    (directory / 'wchess2-census-180.json').write_text('{}\n')
    (directory / 'final.png').write_bytes(b'png')
    (directory / 'private/vita_renderer.json').write_text('{}\n')
    receipt = dict(expected, exit_code=0,
                   log_sha256=oracle.sha(directory / 'run.log'),
                   census_sha256=oracle.sha(directory / 'wchess2-census-180.json'),
                   final_png_sha256=oracle.sha(directory / 'final.png'),
                   config_sha256=oracle.sha(directory / 'private/vita_renderer.json'),
                   effective_config_sha256='e' * 64)
    (directory / 'run-receipt.json').write_text(json.dumps(receipt) + '\n')
    chosen, reuse = oracle.allocate_run_dir(tmp, 'wchess2', 'on', expected)
    assert reuse is True and chosen == directory


def test_sidecar_mutation_is_rejected(tmp):
    directory = cache_dir(tmp)
    assert oracle.files_match_receipt(directory, 'jlair1-census-180.json')
    (directory / 'uniform-bits.jsonl').write_text('changed after the receipt\n')
    assert oracle.files_match_receipt(directory, 'jlair1-census-180.json') is False


def test_missing_recorded_sidecar_is_rejected(tmp):
    directory = cache_dir(tmp)
    (directory / 'uniform-bits.jsonl').unlink()
    assert oracle.files_match_receipt(directory, 'jlair1-census-180.json') is False


def test_unrecorded_sidecar_is_rejected(tmp):
    directory = cache_dir(tmp, recorded=False)
    assert oracle.files_match_receipt(directory, 'jlair1-census-180.json') is False


def test_requested_sidecar_absent_is_rejected(tmp):
    directory = cache_dir(tmp, sidecar=False, expected=True)
    assert oracle.files_match_receipt(directory, 'jlair1-census-180.json') is False


def test_effective_config_binding_is_required(tmp):
    directory = cache_dir(tmp)
    saved = json.loads((directory / 'run-receipt.json').read_text())
    saved.pop('effective_config_sha256')
    (directory / 'run-receipt.json').write_text(json.dumps(saved))
    assert oracle.files_match_receipt(directory, 'jlair1-census-180.json') is False


def test_oracle_identity_mismatch_is_not_reused(tmp):
    expected = oracle.receipt_core(identity(), {'actor_deferred_layers': False},
                                   {'LOOKING_GLASS_SIM_MAPS_PROBE': '1'}, ['--map', 'jlair1'])
    digest = hashlib.sha256(json.dumps(expected, sort_keys=True).encode()).hexdigest()[:20]
    directory = tmp / 'host' / f'jlair1-off-{digest}'
    (directory / 'private').mkdir(parents=True)
    (directory / 'run.log').write_text('log\n')
    (directory / 'jlair1-census-180.json').write_text('{}\n')
    (directory / 'final.png').write_bytes(b'png')
    (directory / 'private/vita_renderer.json').write_text('{}\n')
    stale = dict(expected, oracle_sha256='f' * 64, exit_code=0,
                 log_sha256=oracle.sha(directory / 'run.log'),
                 census_sha256=oracle.sha(directory / 'jlair1-census-180.json'),
                 final_png_sha256=oracle.sha(directory / 'final.png'),
                 config_sha256=oracle.sha(directory / 'private/vita_renderer.json'),
                 effective_config_sha256='e' * 64)
    (directory / 'run-receipt.json').write_text(json.dumps(stale))
    chosen, reuse = oracle.allocate_run_dir(tmp, 'jlair1', 'off', expected)
    assert reuse is False and chosen != directory


def test_changed_aggregate_with_unchanged_sidecar_is_detected(tmp):
    left = [frame(0, [bits_sha()]), frame(1, [bits_sha()])]
    right = [frame(0, [bits_sha()]), frame(1, ['f' * 64])]
    directory = Path(tmp)
    write_sidecar(directory, {(0, 0): bits(), (1, 0): bits()})
    assert oracle.packet_differences(left, right, keep_slot=False, keep_uniforms=True) == [1]
    # The left arm links while the right arm's packet hash no longer matches its sidecar row.
    assert oracle.uniform_bits_issues(directory, right)


def test_sidecar_hash_mismatch_is_detected(tmp):
    directory = Path(tmp)
    rows = {(0, 0): bits()}
    write_sidecar(directory, rows)
    frames = [frame(0, [bits_sha()])]
    assert oracle.uniform_bits_issues(directory, frames) == []
    rows[(0, 0)] = bits(seed=1)
    write_sidecar(directory, rows)
    assert oracle.uniform_bits_issues(directory, frames)


def test_duplicate_and_missing_coverage_are_detected(tmp):
    directory = Path(tmp)
    frames = [frame(0, [bits_sha(), bits_sha()])]
    rows = {(0, 0): bits(), (0, 1): bits()}
    write_sidecar(directory, rows)
    assert oracle.uniform_bits_issues(directory, frames) == []
    path = directory / 'uniform-bits.jsonl'
    path.write_text(path.read_text() + path.read_text().splitlines()[0] + '\n')
    assert any('duplicate coverage' in issue for issue in oracle.uniform_bits_issues(directory, frames))
    write_sidecar(directory, {(0, 0): bits()})
    assert any('missing coverage' in issue for issue in oracle.uniform_bits_issues(directory, frames))


def test_late_viewport_change_is_rejected(tmp):
    directory = Path(tmp)
    (directory / 'run.log').write_text(
        '[lg/load-diagnostic] reason=startup-entry stage=prepare-3d-returned framebuffer_binding=0 '
        'viewport=[0, 0, 721, 409] framebuffer_status=0x8cd5\n')
    assert oracle.startup_viewport(directory) == '[0, 0, 721, 409]'
    (directory / 'run.log').write_text(
        (directory / 'run.log').read_text() +
        '[lg/load-diagnostic] reason=restore stage=prepare-3d-returned framebuffer_binding=0 '
        'viewport=[0, 0, 722, 409] framebuffer_status=0x8cd5\n')
    assert oracle.startup_viewport(directory) is None
    # The only permitted change is the macOS startup settle at frame 0 -> 1.
    settled = [dict(frame=i, world_viewport=[0, 0, 721, 409]) for i in range(1, 6)]
    steady, issues = oracle.viewport_trajectory([dict(frame=0, world_viewport=[0, 0, 720, 408])] + settled,
                                                [0, 0, 720, 408])
    assert issues == [] and steady == [0, 0, 721, 409]
    assert oracle.startup_transition_frame([dict(frame=0, world_viewport=[0, 0, 720, 408])] + settled) == 1
    # A change after frame 1 is late and must be rejected.
    late = settled[:3] + [dict(frame=4, world_viewport=[0, 0, 722, 409])]
    steady, issues = oracle.viewport_trajectory([dict(frame=0, world_viewport=[0, 0, 721, 409])] + late,
                                                [0, 0, 721, 409])
    assert steady == [0, 0, 722, 409] and issues
    # Oscillation is rejected too.
    assert oracle.viewport_trajectory(settled + [dict(frame=6, world_viewport=[0, 0, 720, 408])],
                                      [0, 0, 721, 409])[1]
    assert oracle.viewport_trajectory([dict(frame=0, world_viewport=None)], [0, 0, 721, 409])[1]
    assert oracle.viewport_trajectory(settled, [0, 0, 722, 409])[1]
    assert oracle.viewport_issues([dict(frame=0, world_viewport=[0, 0, 721, 409])], [0, 0, 721, 409]) == []


def test_field_diff_names_only_the_changed_range(tmp):
    left = [0] * 147
    right = left.copy()
    right[16] = 1
    assert oracle.divergent_uniform_fields(left, right) == ['Projection']


def bits_sha(seed=0):
    return oracle.bits_sha256(bits(seed))


if __name__ == '__main__':
    tests = [value for name, value in sorted(globals().items()) if name.startswith('test_')]
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        for index, test in enumerate(tests):
            scratch = root / str(index)
            scratch.mkdir()
            test(scratch)
    print(f'PASS cache/sidecar/viewport controls ({len(tests)} tests)')
