"""V3 overlay wire text and CRC; CRC detects transcription errors, not authenticity."""
import re

LABELS = ('run', 'auth', 'token/frame/us', 'generation/operation/start')

def crc16(text):
    crc = 0xffff
    for byte in text.replace(' ', '').encode('ascii'):
        crc ^= byte << 8
        for _ in range(8):
            crc = ((crc << 1) ^ (0x1021 if crc & 0x8000 else 0)) & 0xffff
    return crc

def grouped(text):
    return ' '.join(text[i:i+4] for i in range(0, len(text), 4))

def display_lines(run_id, token, auth, start_us):
    bodies = ['run=' + grouped(run_id), 'auth=' + grouped(auth)]
    for labels, values in [('tfu', token[:3]), ('gos', [*token[3:], start_us])]:
        bodies.append('|'.join(k+'='+grouped(f'{v:016x}') for k, v in zip(labels, values)))
    return [body + f' ck={crc16(body):04x}' for body in bodies]

def parse_lines(lines):
    if len(lines) != 4:
        raise ValueError('expected four overlay lines')
    bodies = []
    for index, (name, line) in enumerate(zip(LABELS, lines), 1):
        if not line.isascii() or not re.fullmatch(r'[a-z0-9=| ]+ ck=[0-9a-f]{4}', line):
            raise ValueError(f'line {index} ({name}): malformed text/check code')
        body, check = line.rsplit(' ck=', 1)
        if crc16(body) != int(check, 16):
            raise ValueError(f'line {index} ({name}): CRC mismatch; inspect and retranscribe the image')
        bodies.append(body.replace(' ', ''))
    patterns = [r'run=([0-9a-f]{32})', r'auth=([0-9a-f]{32})',
                r't=([0-9a-f]{16})\|f=([0-9a-f]{16})\|u=([0-9a-f]{16})',
                r'g=([0-9a-f]{16})\|o=([0-9a-f]{16})\|s=([0-9a-f]{16})']
    fields = []
    for i, (pattern, body) in enumerate(zip(patterns, bodies), 1):
        match = re.fullmatch(pattern, body)
        if not match:
            raise ValueError(f'line {i} ({LABELS[i-1]}): invalid fields')
        fields.append(match.groups())
    run, auth = bytes.fromhex(fields[0][0]), bytes.fromhex(fields[1][0])
    values = [int(v, 16) for v in (*fields[2], *fields[3])]
    if run == bytes(16) or any(values[i] == 0 for i in (0, 1, 3)):
        raise ValueError('invalid zero run identity or token ordinal/frame/generation')
    token = dict(zip(('ordinal', 'viewer_seq', 'submitted_us', 'generation', 'operation'), values[:5]))
    token['auth'] = list(auth)
    return dict(run_id=list(run), token=token)
