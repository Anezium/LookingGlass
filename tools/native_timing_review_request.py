#!/usr/bin/env python3
"""Write an exclusive ACK only after separate image return and visual inspection.
Accept four exact transcribed lines (--line repeated or --lines-file), or decimal
fields plus all four copied --checks and --start-us. Never uploads or approves.
CRC detects transcription errors; only the native backend authenticates HMAC.
"""
import argparse
import json
import re
from pathlib import Path
from native_timing_overlay import display_lines, parse_lines

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--line', action='append', help='one complete transcribed line; repeat four times')
    p.add_argument('--lines-file', type=Path, help='four transcribed lines, including ck=')
    p.add_argument('--run-id')
    p.add_argument('--token', type=int, nargs=5, metavar=('ORDINAL', 'FRAME', 'NATIVE_US', 'GENERATION', 'OPERATION'))
    p.add_argument('--auth')
    p.add_argument('--start-us', type=int, help='S copied from the image (decimal conversion of s=)')
    p.add_argument('--checks', nargs=4, help='four copied CRCs in line order; never calculate from unverified fields')
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    try:
        fields = (a.run_id, a.token, a.auth, a.start_us, a.checks)
        if a.line is not None or a.lines_file is not None:
            if (a.line is not None and a.lines_file is not None) or any(v is not None for v in fields):
                raise ValueError('use only one transcription input mode')
            lines = a.line if a.line is not None else a.lines_file.read_text(encoding='ascii').splitlines()
        else:
            if any(v is None for v in fields):
                raise ValueError('fields require --run-id, --token, --auth, --start-us and four --checks')
            run, auth = a.run_id.replace(' ', ''), a.auth.replace(' ', '')
            if not re.fullmatch('[0-9a-f]{32}', run) or not re.fullmatch('[0-9a-f]{32}', auth):
                raise ValueError('run-id and authentication tag must each be 32 lowercase hexadecimal characters')
            if not all(0 <= v < 2**64 for v in [*a.token, a.start_us]):
                raise ValueError('invalid unsigned u64 field')
            if not all(re.fullmatch('[0-9a-f]{4}', c) for c in a.checks):
                raise ValueError('checks must each be four lowercase hexadecimal characters')
            lines = [line.rsplit(' ck=', 1)[0] + ' ck=' + check
                     for line, check in zip(display_lines(run, a.token, auth, a.start_us), a.checks)]
        request = parse_lines(lines)
        with a.output.open('x', encoding='utf-8') as f:
            json.dump(request, f, separators=(',', ':'))
            f.write('\n')
    except (ValueError, OSError) as error:
        p.error(str(error))

if __name__ == '__main__':
    main()
