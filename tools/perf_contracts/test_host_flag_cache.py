"""Receipt reuse rejects a completed log whose executable hash does not match. No game run."""
import json, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import host_flag_exact as oracle

def test_mismatch_keeps_the_old_directory(tmp):
    identity = dict(executable_sha256='a' * 64, source_revision='rev', source_manifest_sha256='b' * 64,
                    toolchain='nightly-2026-10-11', features='native_timing', rustflags='--cfg observer_free_B')
    config = {'actor_deferred_layers': False}
    env = {'LOOKING_GLASS_SIM_MAPS_PROBE': '1'}
    expected = oracle.receipt_core(identity, config, env, ['--map', 'jlair1'])
    stale = dict(expected, executable_sha256='c' * 64, exit_code=0, log_sha256='d' * 64,
                 census_sha256='e' * 64, final_png_sha256='f' * 64, config_sha256='1' * 64)
    digest = __import__('hashlib').sha256(json.dumps(expected, sort_keys=True).encode()).hexdigest()[:20]
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
    identity = dict(executable_sha256='a' * 64, source_revision='rev', source_manifest_sha256='b' * 64,
                    toolchain='nightly-2026-10-11', features='native_timing', rustflags='--cfg observer_free_B')
    config = {'actor_deferred_layers': True}
    env = {'LOOKING_GLASS_SIM_MAPS_PROBE': '1'}
    expected = oracle.receipt_core(identity, config, env, ['--map', 'wchess2'])
    digest = __import__('hashlib').sha256(json.dumps(expected, sort_keys=True).encode()).hexdigest()[:20]
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
                   config_sha256=oracle.sha(directory / 'private/vita_renderer.json'))
    (directory / 'run-receipt.json').write_text(json.dumps(receipt) + '\n')
    chosen, reuse = oracle.allocate_run_dir(tmp, 'wchess2', 'on', expected)
    assert reuse is True and chosen == directory

def test_field_diff_names_only_the_changed_range():
    left = [0] * 147
    right = left.copy()
    right[16] = 1
    assert oracle.divergent_uniform_fields(left, right) == ['Projection']

if __name__ == '__main__':
    import tempfile
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        test_mismatch_keeps_the_old_directory(root)
        test_matching_receipt_reuses(root)
        test_field_diff_names_only_the_changed_range()
    print('PASS cache receipt')
