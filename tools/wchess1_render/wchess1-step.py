"""Root-only wchess1 simulation slot driver in step mode. One arm per invocation.

Root supplies a backed-up, observed-closed session with its receiver running, the
packaged SELF installed and the first arm's config installed. Each invocation runs
exactly one full 1200- or 6375-step tape with its 481-frame spike window on, then stops
at the kit's pending root observation. Root inspects the printed PNG and runs
`closed --observed` before the next step; restore stays a separate root command.

This driver writes no mark or other session-log line after the root observation.
Arms are attributed by the kit's launch id and state. Immediately before launch it
re-runs the kit's mutation guard, so a stale or invalidated observation stops the
step. It never decides closure, never retries and never restores. A failed step
leaves wchess1-step-active with a FAILED reason and stops for root recovery.

Gates: retrieved tape name and SHA-256, config semantics including the arm's own
switch, replay steps=N/N, reason=tape-end, checks=match=expected count, diff=0,
event_mismatches=0, result=PASS, the ordered spike window, dropped=0, config PASS,
quit outcome exactly not-running, and a final assert_transaction on the captured
observation. Every replay/fatal gate is re-run on the log drained after wait-exit,
and the receipt records the done line from that final snapshot.
"""
import argparse
import hashlib
import importlib.util
import json
import math
import re
import subprocess
import time
from pathlib import Path

SESSION = Path(__file__).resolve().parent
KIT = SESSION.parent / 'rc1-session' / 'rc1kit.py'
LOG = SESSION / 'logs/arms.log'
MANIFEST = SESSION / 'READY.json'
ACTIVE = SESSION / 'wchess1-step-active'
RESULTS = SESSION / 'wchess1-step-results.json'

spec = importlib.util.spec_from_file_location('wchess1_kit', KIT)
kit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(kit)

sha = lambda p: hashlib.sha256(Path(p).read_bytes()).hexdigest()

FATAL = ('render gpu crash', 'amr vita world', '[lg/replay] diverge', '[lg/replay] event mismatch',
         'panicked at', 'invalid config', 'vita-sim error')
# Informational replay config records whose recorded value is absent are not failures:
# the tape was recorded by another build and legitimately contains no such key.
CONFIG_INFO = re.compile(r'\[lg/replay\] config (\S+) recorded=absent now=')
DONE_RE = re.compile(
    r'\[lg/replay\] done (\S+) steps=(\d+)/(\d+) reason=(\S+) checks=(\d+) match=(\d+) '
    r'diff=(\d+) first_diff=(\S+) event_mismatches=(\d+) seconds=[0-9.]+ result=(\S+)$')


def stop(reason):
    """Durable failure latch: root must recover before this marker is removed."""
    if ACTIVE.exists():
        text = ACTIVE.read_text().strip()
        if 'FAILED' not in text:
            ACTIVE.write_text(text + ' FAILED ' + ' '.join(str(reason).split()) + '\n')
    raise SystemExit('STOP WITHOUT VERDICT: ' + str(reason))


def run(*args):
    p = subprocess.run(['python3', str(KIT), '--session', str(SESSION), *args],
                       capture_output=True, text=True, timeout=240)
    print(p.stdout, end='', flush=True)
    print(p.stderr, end='', flush=True)
    if re.search(r'\bINCONCLUSIVE\b', p.stdout + p.stderr) or p.returncode:
        stop('kit ' + args[0])
    if args[0] == 'wait-exit':
        receipt = re.search(r'^AWAITING ROOT OBSERVATION: (.+\.png)\nSHA-256 ([0-9a-f]{64})$',
                            p.stdout, re.M)
        if not receipt:
            stop('missing root-observation receipt')
        if not Path(receipt[1]).is_file() or sha(Path(receipt[1])) != receipt[2]:
            stop('observation receipt does not match its PNG')
        if not re.search(r'^quit reply: .+ \((?:killed|not-running)\)\.', p.stdout, re.M):
            stop('missing quit outcome receipt')
    if args[:2] == ('log', 'check-config') and not re.search(
            r'^RESULT:\s*PASS(?:[:\s]|$)', p.stdout, re.M):
        stop('missing config PASS')
    return p.stdout


def guard(what):
    kit.mutation_guard(kit.Sess(SESSION), what)


def current():
    state = json.loads((SESSION / 'state.json').read_text())
    rows = list(kit.parse_log(LOG, connections=True))
    if not kit.launch_matches(rows, state.get('launch_id', '')):
        stop('launch identity mismatch')
    return '\n'.join(r[3] for r in kit.launch_lines(rows) if r[2] == 'vita' and r[4] is not None)


def check_config_semantics(config, arm):
    if not (abs(float(config.get('spike_log_ms', -1)) - 0.01) < 1e-12):
        stop('spike_log_ms is not 0.01: ' + arm['name'])
    if config.get('log_frame_time_every') != 0:
        stop('the miniquad timing report is not off: ' + arm['name'])
    if [config.get('spike_log_first_frame'), config.get('spike_log_last_frame')] != arm['window']:
        stop('spike window differs from READY: ' + arm['name'])
    if config.get('replay_play') != arm['tape']:
        stop('replay_play differs from the arm tape: ' + arm['name'])
    for lever in arm['levers']:
        want = lever == arm['lever'] or arm['lever'] == 'combined'
        if config.get(lever) is not want:
            stop('lever %s is %r, want %r: %s' % (lever, config.get(lever), want, arm['name']))


def check_tape(arm):
    tape = SESSION / 'tapes' / arm['tape']
    if not tape.is_file():
        stop('tape missing; run tape get first: ' + arm['tape'])
    if tape.read_bytes()[:9] != b'LGTAPE 1\n':
        stop('tape magic mismatch: ' + arm['tape'])
    if sha(tape) != arm['tape_sha256']:
        stop('tape hash differs from READY: ' + arm['tape'])


def validate_done(line, arm):
    match = DONE_RE.match(line)
    if not match:
        stop('unparseable replay done: ' + line)
    tape, steps, total, reason, checks, matched, diff, _first, events, result = match.groups()
    if tape != arm['tape']:
        stop('replay done names %r, want %r' % (tape, arm['tape']))
    for key, got, want in (('result', result, 'PASS'), ('diff', diff, '0'),
                           ('event_mismatches', events, '0'), ('reason', reason, 'tape-end'),
                           ('steps', steps, arm['steps']), ('total', total, arm['steps']),
                           ('checks', checks, str(arm['checks'])),
                           ('match', matched, str(arm['checks']))):
        if got != want:
            stop('replay done %s=%r, want %r: %s' % (key, got, want, line))
    return line


def fatal_gates(text, arm):
    bad = [l for l in text.splitlines()
           if any(word in l.lower() for word in FATAL) and not CONFIG_INFO.search(l)]
    if bad:
        stop('first failure line: ' + arm['name'] + ': ' + bad[0])


def replay_done_line(text, arm):
    done = [l for l in text.splitlines() if '[lg/replay] done ' in l]
    if len(done) != 1:
        stop('ambiguous replay-done count %d: %s' % (len(done), arm['name']))
    return validate_done(done[0], arm)


def replay_gates(text, arm):
    """All replay/fatal gates on one log snapshot.

    This runs both before and after wait-exit. The drained log is the one frozen into
    the root observation and the arm log, so a fatal line, a second replay-done or a
    changed counter that appears after the first check must still stop the step.
    """
    fatal_gates(text, arm)
    return replay_done_line(text, arm)


def validate_spikes(text, arm):
    spikes = [l for l in text.splitlines() if '[lg/spike] frame=' in l]
    frames = [int(re.search(r'\bframe=(\d+)', l)[1]) for l in spikes]
    first, last = arm['window']
    if frames != list(range(first, last + 1)):
        stop('incomplete/duplicate spike window %d rows, want %d..%d: %s'
             % (len(frames), first, last, arm['name']))
    for line in spikes:
        metrics=dict(re.findall(r'\b(ms|sim|render|hud|present)=([^\s]+)', line))
        try:
            valid=all(k in metrics and math.isfinite(float(metrics[k])) and float(metrics[k])>=0
                      for k in ('ms','sim','render','hud','present'))
        except ValueError:
            valid=False
        if not valid: stop('invalid outer spike metrics: ' + arm['name'])
    if any('sim_split=' not in l for l in spikes):
        stop('missing sim split in the spike window: ' + arm['name'])
    if any('npc=' not in l for l in spikes):
        stop('missing NPC attribution in the spike window: ' + arm['name'])
    flush = [l for l in text.splitlines() if '[lg/spike] flushed' in l]
    if not flush:
        stop('no spike flush line: ' + arm['name'])
    drops = [dict(re.findall(r'(\w+)=(\S+)', l)).get('dropped') for l in flush]
    if any(d != '0' for d in drops):
        stop('dropped spike rows %r: %s' % (drops, arm['name']))
    return spikes, frames


def main():
    manifest = json.loads(MANIFEST.read_text())
    if sha(KIT) != manifest['kit_sha256']:
        raise SystemExit('STOP kit hash mismatch')
    if sha(Path(manifest['self_path'])) != manifest['self_sha256']:
        raise SystemExit('STOP SELF hash mismatch')
    for arm in manifest['arms']:
        if sha(SESSION / arm['config']) != arm['config_sha256']:
            raise SystemExit('STOP config hash mismatch: ' + arm['name'])
    parser = argparse.ArgumentParser(
        description='One wchess1 arm per invocation; root observes between steps')
    parser.add_argument('--step', action='store_true', required=True)
    parser.parse_args()
    results = json.loads(RESULTS.read_text()) if RESULTS.exists() else []
    # Refuses unless root acknowledged the latest PNG with closed --observed.
    guard('driver step')
    index = len(results)
    if index >= len(manifest['arms']):
        raise SystemExit('STOP all arms complete; root runs the separate kit restore')
    if ACTIVE.exists():
        raise SystemExit('STOP prior interrupted/failed step %s; root recovery required'
                         % ACTIVE.read_text().strip())
    arm = manifest['arms'][index]
    config = SESSION / arm['config']
    check_config_semantics(json.loads(config.read_text()), arm)
    check_tape(arm)
    ACTIVE.write_text('%d %s\n' % (index, arm['name']))
    if index:
        run('config', str(config))
    state = json.loads((SESSION / 'state.json').read_text())
    if state.get('game') != 'closed' or state.get('pending'):
        stop('session not closed/clean before launch: ' + arm['name'])
    if (state.get('installed') or {}).get('eboot') != manifest['self_sha256']:
        stop('installed SELF is not the READY SELF: ' + arm['name'])
    if (state.get('config') or {}).get('sha256') != arm['config_sha256']:
        stop('installed config is not the READY config: ' + arm['name'])
    # Re-check the observation immediately before launch: config bookkeeping is the
    # only allowed append, and any foreign log line or stale transaction stops here.
    try:
        guard(arm['name'] + ' launch')
    except kit.Stop as e:
        stop('observation invalid before launch: %s' % e)
    run('launch')
    deadline = time.monotonic() + 600
    while time.monotonic() < deadline:
        text = current()
        fatal_gates(text, arm)
        done = [l for l in text.splitlines() if '[lg/replay] done ' in l]
        if done:
            replay_done_line(text, arm)
            # The kit requires the launch-bound replay done and clean quit transition,
            # captures once and stops awaiting root observation. It never marks closed.
            run('wait-exit', '--timeout', '30')
            state = json.loads((SESSION / 'state.json').read_text())
            outcome = (state.get('observation') or {}).get('quit_outcome')
            if outcome != 'not-running':
                # The replay exits by itself; a quit that still killed the app is anomalous.
                stop('quit outcome %r after a self-exiting replay: %s' % (outcome, arm['name']))
            run('log', 'check-config')
            # Re-read the drained log, then revalidate every replay/fatal gate on it: the
            # receipt must carry the done line from this final snapshot, not the earlier one.
            text = current()
            done_line = replay_gates(text, arm)
            spikes, frames = validate_spikes(text, arm)
            # The pending observation must still be valid when this step is declared complete:
            # same launch/generation/log bytes/session receiver.
            state = json.loads((SESSION / 'state.json').read_text())
            observation = state.get('observation') or {}
            try:
                kit.assert_transaction(kit.Sess(SESSION), observation.get('transaction'))
            except kit.Stop as e:
                stop('observation transaction changed: %s' % e)
            armlog = SESSION / 'logs' / ('arm-' + arm['name'] + '.log')
            armlog.write_text(text + '\n')
            results.append(dict(
                arm=arm['name'], lever=arm['lever'], tape=arm['tape'], window=arm['window'],
                spike_frames=len(frames), replay=done_line, config='PASS', exit='VERIFIED',
                quit=outcome, observation=dict(path=observation.get('path'),
                                               sha256=observation.get('sha256')),
                arm_log=str(armlog), log_sha256=sha(LOG)))
            RESULTS.write_text(json.dumps(results, indent=2) + '\n')
            ACTIVE.unlink()
            print('STEP COMPLETE (%d/%d); root inspects the PNG and runs closed --observed '
                  'before the next step or the separate restore.'
                  % (len(results), len(manifest['arms'])), flush=True)
            return
        time.sleep(1)
    stop('replay deadline: ' + arm['name'])


if __name__ == '__main__':
    with kit.session_lock(kit.Sess(SESSION), 'driver.lock'):
        main()
