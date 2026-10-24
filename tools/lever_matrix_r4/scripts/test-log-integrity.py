"""Offline r4-1 regression fixtures; historical logs are never r4 console evidence."""
from pathlib import Path
import hashlib, json, shutil, subprocess, sys, tempfile

OUT = Path(__file__).resolve().parents[1]
FIXTURES = OUT / 'proof/log-integrity-fixtures'
STOP = 'INVALID_LOG_INTEGRITY_STOP_ALL_REMAINING_R4'

def main():
    protocol = json.loads((OUT / 'CONSOLE-PROTOCOL.json').read_text())
    labels = [entry['label'] for entry in protocol['sequence']]
    checks = []
    with tempfile.TemporaryDirectory(prefix='alice-r4-1-log-fixtures-') as temporary:
        package = Path(temporary)
        (package / 'CONSOLE-PROTOCOL.json').write_text(json.dumps(protocol))

        def run_case(name, runs):
            root = package / name
            root.mkdir()
            receipts = []
            for label, source, expected_code in runs:
                run = root / label
                run.mkdir()
                shutil.copyfile(FIXTURES / source, run / 'runtime-log.txt')
                proc = subprocess.run([sys.executable, str(OUT / 'scripts/log-integrity.py'),
                                       label, '--root', str(root)], capture_output=True, text=True, timeout=10)
                assert proc.returncode == expected_code, (name, label, proc.stdout, proc.stderr)
                receipt = json.loads((run / 'LOG-INTEGRITY.json').read_text())
                assert receipt['status'] == ('PASS' if expected_code == 0 else STOP), receipt
                receipts.append(receipt)
            checks.append(dict(fixture=name, exit_code=runs[-1][2], runs=[
                dict(label=label, exit_code=code, status=receipt['status'],
                     longest_bytes=receipt['longest_renderer_config_record_bytes'],
                     A_minus_B_bytes=receipt['A_minus_B_bytes'])
                for (label, _, code), receipt in zip(runs, receipts)]))
            return receipts

        a, b, b3, a4 = labels[:4]
        run_case('complete', [(a, 'complete.txt', 0)])
        run_case('nested', [(a, 'nested.txt', 2)])
        run_case('orphan', [(a, 'orphan.txt', 2)])
        run_case('different config length', [(a, 'complete.txt', 0), (a4, 'longer-A.txt', 2)])

        source_info = json.loads((FIXTURES / 'R3-SOURCES.json').read_text())
        for relative, source in source_info.items():
            assert hashlib.sha256((FIXTURES / relative).read_bytes()).hexdigest() == source['sha256']
        real_a = a + '/runtime-log.txt'
        real_b = b + '/runtime-log.txt'
        receipts = run_case('r3 actual A then B', [(a, real_a, 0), (b, real_b, 0)])
        assert [r['longest_renderer_config_record_bytes'] for r in receipts] == [2783, 2782]
        assert receipts[1]['prior_run_longest_bytes'] == {}
        assert receipts[1]['prior_session_run_longest_bytes'] == {a: 2783}
        run_case('A minus B differs by 2 bytes', [(a, 'negative-2-A.txt', 0), (b, 'negative-2-B.txt', 2)])
        run_case('A minus B differs by 0 bytes', [(a, 'complete.txt', 0), (b, 'complete.txt', 2)])
        run_case('A minus B differs by minus 1 byte', [(a, 'negative-2-B.txt', 0), (b, 'negative-2-A.txt', 2)])
        receipts = run_case('same-arm ABBA repetition', [(a, real_a, 0), (b, real_b, 0),
                                                        (b3, real_b, 0), (a4, real_a, 0)])
        assert receipts[-1]['prior_run_longest_bytes'] == {a: 2783}
        run_case('same-arm B length mismatch', [(b, real_b, 0), (b3, 'complete.txt', 2)])
        receipts = run_case('sessions remain independent', [(a, real_a, 0), (labels[8], 'complete.txt', 0)])
        assert receipts[-1]['prior_session_run_longest_bytes'] == {}
        receipts = run_case('only prior protocol runs compared', [(a4, 'longer-A.txt', 0), (a, 'complete.txt', 0)])
        assert receipts[-1]['prior_run_longest_bytes'] == {}

    proof = OUT / 'proof/LOG-INTEGRITY-FIXTURES.json'
    proof.write_text(json.dumps(dict(status='PASS', offline_only=True, device_contact=False,
                                    driver_contact=False, historical_logs_are_fixture_only=True,
                                    checks=checks), indent=2) + '\n')
    print(f'{len(checks)} offline log-integrity fixtures passed.')

if __name__ == '__main__':
    main()
