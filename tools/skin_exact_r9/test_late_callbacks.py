"""Offline audio policy fixtures; never imports device or driver helpers."""
from pathlib import Path
import argparse, contextlib, importlib.util, io, json, os, runpy, socket, subprocess, sys, tempfile
from unittest.mock import patch


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('package', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    package = args.package.resolve()
    protocol = package / 'CONSOLE-PROTOCOL.json'
    label = json.loads(protocol.read_text())['sequence'][0]['label']
    spec = importlib.util.spec_from_file_location('run_health', package / 'console/run_health.py')
    health = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(health)
    realtime = '[lg/audio-realtime] window_ms=1000 producer_blocks=1 producer_mix_p95_ms=1 pcm_starved_blocks=0 rejected_sinks=0 dropped_reports=0 queued_samples=1 producer_gap_max_ms=1 producer_sleep_overshoot_max_ms=1'
    music = '[lg/audio-music] decode_ms=1 underrun_frames=0 chunks=1'
    cpal = '[cpal/vita] late_callbacks=1 underrun_estimates=0 callbacks=88 callback_p95_us=800 callback_max_us=846'
    baseline = '\n'.join([
        cpal.replace('late_callbacks=1', 'late_callbacks=99').replace('underrun_estimates=0', 'underrun_estimates=1'),
        'bench: camera frozen map=hedge3', realtime, music, cpal,
        'bench: done 120 frames mode=fullbench',
        cpal.replace('late_callbacks=1', 'late_callbacks=99').replace('underrun_estimates=0', 'underrun_estimates=1'),
    ])
    cases = [('late-only', baseline, 0, [1]),
             ('multiple-windows', baseline.replace(cpal+'\nbench: done', cpal+'\n'+cpal.replace('late_callbacks=1', 'late_callbacks=2')+'\nbench: done'), 0, [1, 2]),
             ('underrun-stop', baseline.replace(cpal+'\nbench: done', cpal.replace('underrun_estimates=0', 'underrun_estimates=1')+'\nbench: done'), 2, [1])]
    for field, value in [('pcm_starved_blocks', '1'), ('rejected_sinks', '1'), ('dropped_reports', '1'), ('underrun_frames', '1'), ('producer_mix_p95_ms', '8')]:
        original = '1' if field == 'producer_mix_p95_ms' else '0'
        cases.append((field+'-stop', baseline.replace(field+'='+original, field+'='+value), 2, [1]))
    cases += [('missing-late-field', baseline.replace('late_callbacks=1 ', ''), 2, []),
              ('corrupt-late-field', baseline.replace('late_callbacks=1 ', 'late_callbacks=bad '), 2, [None]),
              ('missing-end-bound', baseline.replace('bench: done 120 frames mode=fullbench', ''), 2, None)]
    results = []

    def deny(*args, **kwargs):
        raise AssertionError('OFFLINE BOUNDARY: network/process denied')

    with tempfile.TemporaryDirectory(prefix='alice-latecb-fixtures-') as directory, contextlib.ExitStack() as stack:
        for obj, name in [(socket.socket, 'connect'), (socket.socket, 'connect_ex'), (socket.socket, 'bind'), (socket, 'create_connection'), (socket, 'getaddrinfo'), (subprocess, 'Popen'), (os, 'system')]:
            stack.enter_context(patch.object(obj, name, deny))
        stack.enter_context(patch.dict(sys.modules, {'run_health': health}))
        root = Path(directory)
        run = root / label
        run.mkdir()
        for name, text, expected, counts in cases:
            (run / 'runtime-log.txt').write_text(text)
            with patch.object(sys, 'argv', ['strict-audio.py', label, '--root', str(root), '--protocol', str(protocol)]), contextlib.redirect_stdout(io.StringIO()):
                try:
                    runpy.run_path(str(package / 'console/strict-audio.py'), run_name='__main__')
                except SystemExit as error:
                    code = error.code
                else:
                    code = 0
            result = json.loads((run / 'STRICT-AUDIO.json').read_text())
            assert code == expected, (name, code, result)
            assert result['status'] == ('PASS' if expected == 0 else 'FAIL_OR_UNKNOWN')
            late = result['BENCHMARK_LATE_CALLBACKS']
            if counts is not None:
                assert late['per_window'] == counts, (name, late)
                assert late['total'] == sum(v for v in counts if v is not None)
            assert late['report_only']
            if name in ('late-only', 'multiple-windows'):
                assert not health.health_text(text)
            if name.endswith('-stop'):
                assert health.health_text(text)
            results.append(dict(fixture=name, status='PASS', strict_exit_code=code, BENCHMARK_LATE_CALLBACKS=late, reasons=result['reasons']))
            print('PASS', name, 'strict exit', code, 'BENCHMARK_LATE_CALLBACKS', late['per_window'], 'total', late['total'])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(dict(status='PASS', offline_only=True, device_contact=False, driver_contact=False, checks=results), indent=2)+'\n')


if __name__ == '__main__':
    main()
