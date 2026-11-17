"""Fake rc1 kit for the offline end-to-end driver test. NEVER used on a console.

It speaks the same CLI and state.json vocabulary as the real rc1kit.py for the subset
the driver uses (observe/closed/install/config/launch/wait-exit/log check-config) and
writes a synthetic tagged log with the real replay/spike line shapes. The real kit
remains the only kit used for a console slot; this file is imported by tests only.
"""
import argparse
import contextlib
import datetime
import fcntl
import hashlib
import json
import math
import os
import re
import shutil
import sys
import uuid
from pathlib import Path

SIM = 'ux0:data/alice/private/vita_sim.json'
FATAL_MODE = json.loads(os.environ.get('FAKE_MODE', '{}'))


class Stop(Exception):
    pass


def now_iso():
    return datetime.datetime.now().isoformat(timespec='milliseconds')


def sha256_file(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write_json(path, data):
    path = Path(path)
    tmp = path.with_name(path.name + '.tmp')
    tmp.write_text(json.dumps(data, indent=2, sort_keys=True) + '\n')
    os.replace(tmp, path)


class Sess:
    def __init__(self, root):
        self.root = Path(root).expanduser().resolve()
        self.root.mkdir(parents=True, exist_ok=True)
        (self.root / 'store').mkdir(exist_ok=True)
        (self.root / 'tmp').mkdir(exist_ok=True)
        (self.root / 'logs').mkdir(exist_ok=True)
        (self.root / 'shots').mkdir(exist_ok=True)
        self.state_path = self.root / 'state.json'

    def state(self):
        if self.state_path.exists():
            return json.loads(self.state_path.read_text())
        return dict(game='unknown', backup=False, console_written=False, installed=None,
                    config=None, pending=None, log=None)

    def save(self, state):
        write_json(self.state_path, state)

    def record(self, command, **fields):
        line = json.dumps(dict(time=now_iso(), command=command, **fields), sort_keys=True)
        with open(self.root / 'session.jsonl', 'a') as f:
            f.write(line + '\n')

    def store(self, path):
        digest = sha256_file(path)
        target = self.root / 'store' / digest
        if target.exists():
            Path(path).unlink()
        else:
            os.replace(path, target)
        return digest

    def stored(self, digest):
        return self.root / 'store' / digest

    def tmp(self, name):
        return self.root / 'tmp' / name

    def log_path(self, label):
        return self.root / 'logs' / (label + '.log')


def keep_copy(sess, path):
    target = sess.tmp(Path(path).name)
    shutil.copyfile(path, target)
    return sess.store(target)


def append_line(path, text):
    with open(path, 'a') as f:
        f.write(text)


def kit_line(path, text):
    append_line(path, '%s | kit | %s\n' % (now_iso(), text))


def parse_log(path, connections=False):
    with open(path, 'rb') as f:
        for n, raw in enumerate(f, 1):
            line = raw.decode('utf-8', 'replace').rstrip('\n').rstrip('\r')
            parts = line.split(' | ', 2)
            if len(parts) == 3 and parts[1] in ('vita', 'kit'):
                text = parts[2].rstrip('\r')
                if not connections and parts[1] == 'vita':
                    text = re.sub(r'^\[conn=\d+\] ', '', text)
                yield n, parts[0], parts[1], text
            else:
                yield n, '', 'other', line


def launch_matches(lines, launch_id):
    marks = [r[3] for r in lines if r[2] == 'kit' and r[3].startswith('launch ')]
    return bool(len([m for m in marks if re.search(r'\bid=' + re.escape(launch_id) + r'(?:\s|$)', m)]) == 1
                and marks and re.search(r'\bid=' + re.escape(launch_id) + r'(?:\s|$)', marks[-1]))


def launch_lines(lines, offline=False):
    start = max((i for i, row in enumerate(lines)
                 if row[2] == 'kit' and row[3].startswith('launch ')), default=None)
    if start is None:
        return []
    opened, out = set(), []
    for n, t, src, text in lines[start + 1:]:
        event = re.match(r'connection (open|closed) id=(\d+) ', text) if src == 'kit' else None
        if event and event[1] == 'open':
            opened.add(event[2])
        tag = re.match(r'\[conn=(\d+)\] (.*)', text) if src == 'vita' else None
        if tag:
            if tag[1] not in opened:
                continue
            text = tag[2]
        if src == 'vita' and not tag and not offline:
            continue
        out.append((n, t, src, text, tag[1] if tag else None))
    return out


def log_snapshot(state):
    path = (state.get('log') or {}).get('path')
    if not path:
        return dict(log_size=0, log_sha256=None, connection_id=None)
    data = Path(path).read_bytes()
    connections = re.findall(rb'connection open id=(\d+) ', data)
    return dict(log_size=len(data), log_sha256=hashlib.sha256(data).hexdigest(),
                connection_id=connections[-1].decode() if connections else None)


def ownership(state):
    log = state.get('log') or {}
    return dict(launch_id=state.get('launch_id'), generation=log.get('generation'),
                log=log.get('path'), quit_id=state.get('quit_id'))


def transaction(sess):
    state = sess.state()
    return dict(session=str(sess.root), **ownership(state), **log_snapshot(state))


def assert_transaction(sess, expected):
    if transaction(sess) != expected:
        raise Stop('Session/launch/generation/log changed during observation')
    if expected.get('launch_id') and not launch_matches(
            list(parse_log(expected['log'], connections=True)), expected['launch_id']):
        raise Stop('Original launch ownership lost')


def mutation_guard(sess, what):
    state = sess.state()
    proof = state.get('verified_exit') or {}
    if state.get('exit_failed') or state.get('game') != 'closed' or not proof:
        raise Stop(what + ' requires closed --observed <latest PNG>')
    expected = proof['transaction']
    if ownership(state) != {k: expected[k] for k in ownership(state)} or \
            log_snapshot(state) != {k: expected[k] for k in log_snapshot(state)}:
        raise Stop('Observation ownership/log changed before ' + what)
    if sha256_file(proof['path']) != proof['sha256']:
        raise Stop('Observed PNG changed before ' + what)
    assert_transaction(sess, expected)


@contextlib.contextmanager
def session_lock(sess, name='command.lock'):
    with open(sess.root / name, 'a') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise Stop('Session command lock held; nothing sent')
        try:
            yield
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)


def bookkeeping_line(sess, state, kind, text):
    path = Path(state['log']['path'])
    proof = state.get('verified_exit')
    if not (proof and state.get('game') == 'closed'):
        kit_line(path, text)
        return
    expected = proof['transaction']
    before = path.read_bytes()
    if len(before) != expected['log_size'] or hashlib.sha256(before).hexdigest() != expected['log_sha256']:
        raise Stop('Log changed after the root observation, before %s bookkeeping' % kind)
    kit_line(path, text)
    after = path.read_bytes()
    expected['log_size'] = len(after)
    expected['log_sha256'] = hashlib.sha256(after).hexdigest()
    proof.setdefault('bookkeeping', []).append(dict(kind=kind, text=text))
    sess.save(state)


def png(sess):
    path = sess.root / 'shots' / ('observe-' + uuid.uuid4().hex + '.png')
    path.write_bytes(b'PNG offline fixture')
    return path


def arm_for_config(sess, digest):
    ready = json.loads((sess.root / 'READY.json').read_text())
    for arm in ready['arms']:
        if arm['config_sha256'] == digest:
            return arm
    raise Stop('no READY arm for config ' + digest)


def game_text(arm):
    first, last = arm['window']
    base = 42.2
    cuts = {'actors': 20.0, 'world_low': 10.0, 'world_full': 3.0, 'world_alpha': 4.0,
            'sky': 2.0, 'fx_add': 5.0, 'fx_alpha': 1.0, 'fx_other': 0.5}
    cut = sum(cuts[name] for name in arm['mask'])
    if arm['name'] == 'base-live':
        cut = 0.0
    lines = ['[lg/replay] play %s steps=%s start=chapter:%s recorded_on=vita package=offline '
             'checks_every=30 ended=true truncated=false' % (arm['tape'], arm['steps'], arm['map'])]
    lines.append('[lg/gpu-mask] mask=%s world_gxm=%s' % (','.join(arm['mask']) or '-', arm['world_gxm']))
    if FATAL_MODE.get('shader_failed'):lines.append('[lg/vita-renderer] corpse opaque program failed, using original')
    for i, frame in enumerate(range(first, last + 1)):
        if FATAL_MODE.get('omit') == frame:
            continue
        shown = frame + 1 if FATAL_MODE.get('wrong_frame') and i == 10 else frame
        gxm = max(0.5, base - cut + 2.0 * math.sin(i / 17.0))
        if FATAL_MODE.get('nonfinite') and i==10:gxm=float('nan')
        total = gxm + 34.0 + 0.05 * (i % 20)
        render = 18.0 + 0.02 * (i % 30)
        sim = 4.0 + 0.01 * (i % 10)
        fields = (' ms=%.2f sim=%.2f render=%.2f hud=0.80 present=1.60' %
                  (total, sim, render))
        if not (FATAL_MODE.get('no_gxm') and i == 10):
            fields += ' gxm_begin=0.10 gxm_end=%.2f' % gxm
        fields += ' sim_split=input:0.80,scene:%.2f,particles:0.40' % gxm
        if ' gxm_end=' in fields:
            fields += ' gxm_end_in=scene:%.2f/1' % gxm
        lines.append('[lg/spike] frame=%d%s' % (shown, fields))
    dropped = 1 if FATAL_MODE.get('drop') else 0
    lines.append('[lg/spike] flushed at=replay-end lines=%d dropped=%d bytes=1 ms=1.0'
                 % (last - first + 1 - (1 if FATAL_MODE.get('omit') else 0), dropped))
    diff = 1 if FATAL_MODE.get('diff') else 0
    events = 2 if FATAL_MODE.get('events') else 0
    lines.append('[lg/replay] done %s steps=%s/%s reason=tape-end checks=%s match=%s '
                 'diff=%d first_diff=- event_mismatches=%d seconds=1.0 result=PASS'
                 % (arm['tape'], arm['steps'], arm['steps'], arm['checks'], arm['checks'],
                    diff, events))
    lines.append('[lg/transition-memory] {"event":"end","kind":"quit","success":true}')
    return lines


def cmd_observe(sess, args):
    path = png(sess)
    state = sess.state()
    state['observation'] = dict(path=str(path), sha256=sha256_file(path),
                                transaction=transaction(sess))
    state['game'] = 'awaiting-root-observation'
    state['pending'] = None
    sess.save(state)
    print('AWAITING ROOT OBSERVATION: %s\nSHA-256 %s' % (path, sha256_file(path)))


def cmd_closed(sess, args):
    state = sess.state()
    proof = state.get('observation')
    if state.get('game') != 'awaiting-root-observation' or not proof:
        raise Stop('No pending observation')
    if Path(args.observed).expanduser().resolve() != Path(proof['path']).resolve():
        raise Stop('Only the latest observation PNG can be acknowledged')
    assert_transaction(sess, proof['transaction'])
    state['game'] = 'closed'
    state['verified_exit'] = proof
    sess.save(state)
    print('Recorded root observation: Alice closed.')


def cmd_install(sess, args):
    state = sess.state()
    digest = keep_copy(sess, args.config)
    state['installed'] = dict(eboot=sha256_file(args.eboot), eboot_path=str(Path(args.eboot).resolve()))
    state['config'] = dict(sha256=digest, source=str(Path(args.config).resolve()))
    state['game'] = 'closed'
    state['pending'] = None
    sess.save(state)
    bookkeeping_line(sess, state, 'install', 'install SELF %s config %s (%s)' % (
        sha256_file(args.eboot)[:12], digest[:12], Path(args.config).name))
    print('Installed and read back: SELF %s, config %s' % (sha256_file(args.eboot)[:16], digest[:16]))


def cmd_config(sess, args):
    state = sess.state()
    digest = keep_copy(sess, args.file)
    state['config'] = dict(sha256=digest, source=str(Path(args.file).resolve()))
    state['game'] = 'closed'
    state['pending'] = None
    sess.save(state)
    bookkeeping_line(sess, state, 'config', 'config %s (%s)' % (digest[:12], Path(args.file).name))
    if FATAL_MODE.get('foreign_after_config'):
        append_line(state['log']['path'], '%s | vita | [conn=1] foreign line after observation\n'
                    % now_iso())
    print('Config swapped and read back: %s %s' % (Path(args.file).name, digest[:16]))


def cmd_launch(sess, args):
    state = sess.state()
    mutation_guard(sess, 'launch')
    state = sess.state()
    state['launch_id'] = uuid.uuid4().hex
    state.pop('verified_exit', None)
    path = state['log']['path']
    kit_line(path, 'launch ALICELG01 id=%s (SELF %s, config %s)' % (
        state['launch_id'], (state.get('installed') or {}).get('eboot', 'owner')[:12],
        (state.get('config') or {}).get('sha256', 'owner')[:12]))
    append_line(path, '%s | kit | connection open id=1 fixture\n' % now_iso())
    arm = arm_for_config(sess, state['config']['sha256'])
    for line in game_text(arm):
        append_line(path, '%s | vita | [conn=1] %s\n' % (now_iso(), line))
    append_line(path, '%s | kit | connection closed id=1 fixture\n' % now_iso())
    state['game'] = 'running'
    sess.save(state)
    print('launch ALICELG01: fixture')


def cmd_wait_exit(sess, args):
    state = sess.state()
    path = state['log']['path']
    if FATAL_MODE.get('duplicate_done'):
        arm = arm_for_config(sess, state['config']['sha256'])
        append_line(path, '%s | vita | [conn=1] [lg/replay] done %s steps=%s/%s reason=tape-end '
                          'checks=%s match=%s diff=0 first_diff=- event_mismatches=0 seconds=1.0 '
                          'result=PASS\n' % (now_iso(), arm['tape'], arm['steps'], arm['steps'],
                                             arm['checks'], arm['checks']))
    outcome = 'killed' if FATAL_MODE.get('killed') else 'not-running'
    kit_line(path, 'quit (vitacompanion kill, no save guarantee) outcome=%s' % outcome)
    state['quit_id'] = uuid.uuid4().hex
    sess.save(state)
    image = png(sess)
    state = sess.state()
    state['observation'] = dict(path=str(image), sha256=sha256_file(image),
                                transaction=transaction(sess), quit_outcome=outcome)
    state['game'] = 'awaiting-root-observation'
    sess.save(state)
    print('AWAITING ROOT OBSERVATION: %s\nSHA-256 %s' % (image, sha256_file(image)))
    print('quit reply: Error: cannot quit the app. Is the TITLEID correct? (%s). The image '
          'decides: stop unless it shows the LiveArea with Alice closed.' % outcome)


def cmd_backup(sess, args):
    state = sess.state()
    write_json(sess.root / 'backup.json', dict(files=[], absent=[SIM]))
    state['backup'] = True
    sess.save(state)
    print('Backup receipt written (offline fixture).')


def cmd_preflight(sess, args):
    print('PASS offline fixture preflight')


def cmd_restore(sess, args):
    state = sess.state()
    rows = [dict(path=SIM, result='KEPT', read='0' * 64, expected='0' * 64)]
    write_json(sess.root / 'restore.json', dict(keep_saves=bool(args.keep_saves), rows=rows))
    state['game'] = 'closed'
    state['pending'] = None
    sess.save(state)
    print('restore --keep-saves fixture')


def cmd_log(sess, args):
    if args.action == 'check-config':
        print('RESULT: PASS: fixture startup config matches the installed config')
        return 0
    if args.action == 'stop':
        state = sess.state()
        (state.get('log') or {})['pid'] = None
        sess.save(state)
        print('receiver stopped (fixture)')
        return 0
    raise Stop('fixture log action ' + str(args.action))


def main(argv=None):
    parser = argparse.ArgumentParser()
    parser.add_argument('--session', required=True)
    sub = parser.add_subparsers(dest='command')
    sub.add_parser('preflight')
    sub.add_parser('observe')
    p = sub.add_parser('closed')
    p.add_argument('--observed', required=True)
    p = sub.add_parser('install')
    p.add_argument('--eboot', required=True)
    p.add_argument('--config', required=True)
    p = sub.add_parser('config')
    p.add_argument('file')
    sub.add_parser('launch')
    p = sub.add_parser('wait-exit')
    p.add_argument('--timeout', type=float, default=30.)
    p = sub.add_parser('backup')
    p.add_argument('--again', action='store_true')
    p = sub.add_parser('restore')
    p.add_argument('--keep-saves', action='store_true')
    p = sub.add_parser('log')
    p.add_argument('action')
    p.add_argument('label', nargs='?')
    args = parser.parse_args(argv)
    sess = Sess(args.session)
    try:
        {'preflight': cmd_preflight, 'observe': cmd_observe, 'closed': cmd_closed,
         'install': cmd_install, 'config': cmd_config, 'launch': cmd_launch,
         'wait-exit': cmd_wait_exit, 'backup': cmd_backup, 'restore': cmd_restore,
         'log': cmd_log}[args.command](sess, args)
        return 0
    except Stop as e:
        print('STOP: ' + str(e))
        return 3


if __name__ == '__main__':
    sys.exit(main())
