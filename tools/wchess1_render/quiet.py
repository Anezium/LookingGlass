"""Root-only local receiver backlog gate: 15 seconds of unchanged bytes.

Checks local log bytes and receiver liveness. It does not establish that no future
writer exists; root still owns the observation and backup order.
"""
import hashlib
import importlib.util
import json
import time
from pathlib import Path

P = Path(__file__).resolve().parent
K = P.parent / 'rc1-session/rc1kit.py'
MANIFEST = json.loads((P / 'READY.json').read_text())
if hashlib.sha256(K.read_bytes()).hexdigest() != MANIFEST['kit_sha256']:
    raise SystemExit('STOP kit hash mismatch')
spec = importlib.util.spec_from_file_location('quiet_kit', K)
k = importlib.util.module_from_spec(spec)
spec.loader.exec_module(k)

s = k.Sess(P)
state = s.state()
original = state.get('log')
if not original:
    raise SystemExit('STOP no receiver log; run log start first')
log = Path(original['path'])
last = None
since = time.monotonic()
deadline = since + 90
while time.monotonic() < deadline:
    current = s.state()
    if current.get('log') != original:
        raise SystemExit('STOP receiver generation changed')
    if not k.receiver_alive(s, current):
        raise SystemExit('STOP receiver not alive')
    value = hashlib.sha256(log.read_bytes()).hexdigest()
    if value != last:
        last = value
        since = time.monotonic()
    if time.monotonic() - since >= 15:
        print('QUIET 15 seconds; local log SHA-256 %s' % value)
        break
    time.sleep(1)
else:
    raise SystemExit('STOP backlog never became quiet')
