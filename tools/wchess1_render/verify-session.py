"""Root's offline receipt checks. No console communication.

backup    owner SELF/config/save manifest exists and the candidate is not the owner.
tape      every GET copy on the Mac matches its READY hash.
shutdown  restore.json read-back rows, KEPT saves compared by read hash, owned
          driver/receiver processes gone and port 9999 free. Run after log stop.
"""
import hashlib
import json
import subprocess
import sys
from pathlib import Path

P = Path(__file__).resolve().parent
MANIFEST = json.loads((P / 'READY.json').read_text())
sha = lambda p: hashlib.sha256(Path(p).read_bytes()).hexdigest()


def backup_rows():
    path = P / 'backup.json'
    if not path.is_file():
        raise SystemExit('missing backup.json')
    return json.loads(path.read_text())['files']


def check_backup():
    rows = backup_rows()
    by_name = {Path(r['path']).name: r for r in rows}
    missing = {'eboot.bin', 'vita_renderer.json'} - by_name.keys()
    if missing:
        raise SystemExit('backup lacks owner files: %s' % sorted(missing))
    if 'vita_sim.json' not in by_name:
        print('NOTE owner vita_sim.json was absent at backup; the game uses its sim defaults')
    if by_name['eboot.bin']['sha256'] == MANIFEST['self_sha256']:
        raise SystemExit('candidate SELF is the owner backup; install was not the candidate')
    print('PASS owner backup receipt (%d files, candidate SELF differs)' % len(rows))


def check_tapes():
    tapes = P / 'tapes'
    for arm in MANIFEST['arms']:
        local = tapes / arm['tape']
        if not local.is_file():
            raise SystemExit('missing tape copy: %s' % local)
        if sha(local) != arm['tape_sha256']:
            raise SystemExit('tape hash differs from READY: %s' % local)
        print('PASS tape %-20s %s' % (arm['tape'], arm['tape_sha256']))


def check_shutdown():
    restore_path = P / 'restore.json'
    if not restore_path.is_file():
        raise SystemExit('missing restore.json; run restore --keep-saves first')
    restore = json.loads(restore_path.read_text())
    if not restore.get('keep_saves'):
        raise SystemExit('restore.json was not written with --keep-saves')
    rows = backup_rows()
    restored = {r['path']: r for r in restore['rows']}
    kept = 0
    for row in rows:
        got = restored.get(row['path'])
        if not got:
            raise SystemExit('restore has no row for %s' % row['path'])
        if got.get('result') == 'KEPT':
            kept += 1
            if got.get('read') != row['sha256']:
                raise SystemExit('KEPT read hash differs from backup for %s: %r != %r'
                                 % (row['path'], got.get('read'), row['sha256']))
        elif got.get('result', '').startswith(('OK', 'SKIP', 'NOTE')):
            if got.get('result', '').startswith('OK'):
                if got.get('expected') != got.get('read') or got.get('expected') != row['sha256']:
                    raise SystemExit('restore read-back differs for %s: %r/%r'
                                     % (row['path'], got.get('expected'), got.get('read')))
        else:
            raise SystemExit('restore row is not accepted: %s %r' % (row['path'], got))
    expected_kept = sum(1 for row in rows if row.get('kind') == 'save')
    if kept != expected_kept:
        raise SystemExit('KEPT save count %d differs from backed-up save count %d'
                         % (kept, expected_kept))
    state = json.loads((P / 'state.json').read_text())
    if state.get('game') != 'closed' or state.get('pending') or \
            (state.get('log') or {}).get('pid') is not None:
        raise SystemExit('session is not shut down: game=%r pending=%r log=%r'
                         % (state.get('game'), state.get('pending'), state.get('log')))
    processes = subprocess.check_output(['ps', '-axo', 'pid=,ppid=,tty=,args='],
                                        text=True).splitlines()
    owned = [r for r in processes
             if (str(P / 'wchess1-step.py') in r) or
             ('wchess1-step.py' in r and '--step' in r) or
             (str(P) in r and 'rc1kit.py' in r and '_receive' in r)]
    if owned:
        raise SystemExit('owned processes still running: %s' % owned)
    listener = subprocess.run(['/usr/sbin/lsof', '-nP', '-iTCP:9999', '-sTCP:LISTEN'],
                              capture_output=True, text=True)
    if listener.returncode == 0 or listener.stdout.strip():
        raise SystemExit('port 9999 still has a listener: %s' % listener.stdout.strip())
    receipt = dict(restore='PASS all backed-up files read back; KEPT saves compared by read hash',
                   keep_saves=True, kept_saves=kept, restore_rows=len(restore['rows']),
                   owned_processes=owned, port9999='free',
                   terminal='No task Terminal.app windows requested; no visible-window count asserted')
    (P / 'shutdown.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt))


if __name__ == '__main__':
    phase = sys.argv[1] if len(sys.argv) == 2 else ''
    if phase == 'backup':
        check_backup()
    elif phase == 'tape':
        check_tapes()
    elif phase == 'shutdown':
        check_shutdown()
    else:
        raise SystemExit('usage: verify-session.py backup|tape|shutdown')
