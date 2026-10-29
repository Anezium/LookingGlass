#!/usr/bin/env python3
"""Every console auditor in tools/ must still parse the startup config record.

The fixtures are the `[lg/startup] renderer_config` and `[lg/vita-renderer]` lines printed by the
host binary for three runtime configs: cadence 0, cadence omitted (default 10) and a malformed
cadence (key rejected, default 10). The test finds each auditor's own parser expression in its
source and evaluates that exact text on the fixture, so a consumer added later is covered too.
It also checks that the d43b573 single-line format (JSON plus a cadence suffix) fails the same
expressions, which is the regression this layout fixes.
"""
from pathlib import Path
import json, re, sys

HERE = Path(__file__).resolve().parent
TOOLS = HERE.parent
TAG = '[lg/vita-renderer] config '
# The two parser shapes in use: json.loads on the text after the tag, and the byte-length check.
AUDIT = re.compile(r"configs=(\[json\.loads\(l\.split\('\[lg/vita-renderer\] config ',1\)\[1\]\) for l in t\.splitlines\(\) if '\[lg/vita-renderer\] config ' in l\])")
LENGTH = re.compile(r"lengths=(\[len\(v\) for v in lines if v\.startswith\(b'\[lg/vita-renderer\] config '\)\])")
CASES = {'zero': 'off', 'omitted': '10', 'bad': '10'}


def consumers():
    found = {'audit': [], 'length': []}
    for path in sorted(TOOLS.rglob('*.py')):
        if path.parent == HERE:
            continue
        text = path.read_text(encoding='utf-8')
        if TAG not in text:
            continue
        for kind, pattern in [('audit', AUDIT), ('length', LENGTH)]:
            for m in pattern.finditer(text):
                found[kind].append((path.relative_to(TOOLS), m[1]))
    return found


def old_format(log):
    """The d43b573 record: the cadence appended to the config line."""
    out = []
    for line in log.splitlines():
        if line.startswith('[lg/vita-renderer] report_cadence '):
            out[-1] += ' ' + line.split(' ', 2)[2]
        else:
            out.append(line)
    return '\n'.join(out) + '\n'


def main():
    found = consumers()
    # Every file that mentions the tag and parses it must match one of the two known shapes.
    parsers = [p for p in sorted(TOOLS.rglob('*.py')) if p.parent != HERE and TAG in p.read_text(encoding='utf-8')
               and re.search(r"json\.loads\(l\.split\('\[lg/vita-renderer\] config '|startswith\(b'\[lg/vita-renderer\] config '", p.read_text(encoding='utf-8'))]
    matched = {p for kind in found.values() for p, _ in kind}
    assert {p.relative_to(TOOLS) for p in parsers} == matched, ('unrecognized parser shape', parsers, matched)
    assert len(found['audit']) >= 9 and len(found['length']) >= 2, found
    for case, cadence in CASES.items():
        log = (HERE / 'fixtures' / f'host-{case}.txt').read_text(encoding='utf-8')
        given = json.loads((HERE / 'fixtures' / f'host-{case}.json').read_text(encoding='utf-8'))
        assert log.count('[lg/vita-renderer] report_cadence ') == 1
        assert f'[lg/vita-renderer] report_cadence effective_log_frame_time_every={cadence}\n' in log, case
        assert ('invalid config key' in log) == (case == 'bad'), case
        for path, expr in found['audit']:
            configs = eval(expr, {'json': json}, {'t': log})
            assert len(configs) == 1, (path, case)
            printed = configs[0]
            want = dict(given)
            if case != 'zero':
                want['log_frame_time_every'] = 10
            # The record prints every Config field; each key the file gave must come back unchanged.
            assert {k: printed[k] for k in want} == want, (path, case)
            try:
                eval(expr, {'json': json}, {'t': old_format(log)})
            except json.JSONDecodeError:
                pass
            else:
                raise AssertionError(f'{path}: old single-line format unexpectedly parsed')
        for path, expr in found['length']:
            lengths = eval(expr, {}, {'lines': log.encode().splitlines()})
            assert len(lengths) == 1 and lengths[0] > 2000, (path, case, lengths)
    print(json.dumps(dict(status='PASS', audit_parsers=[str(p) for p, _ in found['audit']],
                          length_parsers=[str(p) for p, _ in found['length']], cases=CASES), indent=1))


if __name__ == '__main__':
    sys.exit(main())
