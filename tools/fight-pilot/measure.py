"""Root-only RC14 measurement. Reuses play-record gates; no auto-observation or restore."""
import argparse
import hashlib
import importlib.util
import json
import re
import subprocess
import time
from pathlib import Path

SESSION = Path(__import__("os").environ.get("FIGHT_PILOT_SESSION", str(Path(__file__).resolve().parent))).resolve()
KIT = Path(__import__('os').environ.get('PLAY_RECORD_KIT', str(SESSION.parent.parent / 'rc1-session' / 'rc1kit.py')))
LOG = SESSION / 'logs/arms.log'
MANIFEST = SESSION / 'READY.json'
ACTIVE = SESSION / 'measure-step-active'
RESULTS = SESSION / 'measure-step-results.json'

spec = importlib.util.spec_from_file_location('skin_kit', KIT)
kit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(kit)

sha = lambda p: hashlib.sha256(Path(p).read_bytes()).hexdigest()

# Informational replay config records whose recorded value is absent are not failures:
# the tape was recorded by another build and legitimately contains no such key.
CONFIG_INFO = re.compile(r'\[lg/replay\] config (\S+) recorded=absent now=')
FATAL = ('render gpu crash', 'amr vita world', '[lg/replay] diverge', '[lg/replay] event mismatch',
         'panicked at', 'invalid config', 'vita-sim error')
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
    expected = json.loads((SESSION / 'baseline.json').read_text())
    expected.update(replay_record='', replay_play=arm['tape'], replay_start='',
                    replay_stop_after=0, log_frame_time_every=0, spike_log_ms=0.001,
                    spike_log_first_frame=arm['window'][0], spike_log_last_frame=arm['window'][1])
    for key in ('sim_snark_move_skip', 'sim_snark_sight_blocked', 'sim_pool_name_match'):
        if arm['variant'] == 'rc15': expected[key] = True
    if config != expected: stop('config semantics mismatch: ' + arm['name'])


def check_tape(arm):
    """The driver hashes the tape it will play, and requires its console retrieval record."""
    name, digest = arm['tape'], arm['tape_sha256']
    path = SESSION / 'tapes' / name
    if not path.is_file():
        stop('tape %s is not in %s; root runs tape get first' % (name, SESSION / 'tapes'))
    data = path.read_bytes()
    if data[:9] != b'LGTAPE 1\n':
        stop('tape magic mismatch: ' + name)
    if hashlib.sha256(data).hexdigest() != digest:
        stop('tape hash differs from READY: ' + name)
    records = SESSION / 'session.jsonl'
    if not records.is_file():
        stop('no session records; the kit has not retrieved a tape')
    events = []
    for line in records.read_text().splitlines():
        if line.strip():
            events.append(json.loads(line))
    gets = [r for r in events if r.get('command') == 'tape get'
            and r.get('local') == str(path) and r.get('sha256') == digest]
    if not gets:
        stop('no tape get record binds this file to the console: ' + name)


def validate_done(line, arm):
    match = DONE_RE.match(line)
    if not match:
        stop('unparseable replay done: ' + line)
    tape, steps, total, reason, checks, matched, diff, _first, events, result = match.groups()
    for key, got, want in (('tape', tape, arm['tape']),
                           ('steps', steps, str(arm['steps'])),
                           ('steps_total', total, str(arm['steps'])),
                           ('reason', reason, 'tape-end'),
                           ('checks', checks, str(arm['checks'])),
                           ('checks_match', matched, str(arm['checks'])),
                           ('diff', diff, '0'),
                           ('event_mismatches', events, '0'),
                           ('result', result, 'PASS')):
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
    flush = [l for l in text.splitlines() if '[lg/spike] flushed' in l]
    if not flush:
        stop('no spike flush line: ' + arm['name'])
    drops = [dict(re.findall(r'(\w+)=(\S+)', l)).get('dropped') for l in flush]
    if any(d != '0' for d in drops):
        stop('dropped spike rows %r: %s' % (drops, arm['name']))
    return spikes, frames


def statistics(spikes, arm):
    import math, statistics
    maps = json.loads((SESSION / 'tape-index.json').read_text())[arm['tape']]['maps']
    grouped = {}
    for line in spikes:
        fields = dict(re.findall(r'(\w+)=(\S+)', line))
        frame = int(fields['frame'])
        # Spike frame 1 is viewer/tape step 0; events precede their F row.
        name = maps[frame - 1]
        row = grouped.setdefault(name, {k: [] for k in ('ms','sim','render')})
        for k in row:
            value = float(fields[k])
            if not math.isfinite(value) or value < 0: stop('invalid timer')
            row[k].append(value)
    return grouped


def report(results):
    import math, statistics
    grouped = {}
    for r in results:
        for m, values in r['samples'].items():
            key = (r['tape'],r['variant'],m)
            out = grouped.setdefault(key,{k:[] for k in values})
            for k,v in values.items(): out[k].extend(v)
    rows = []
    for (tape,variant,m),values in grouped.items():
        row = dict(tape=tape,variant=variant,map=m,n=len(values['ms']))
        for k,v in values.items():
            row[k] = dict(median=statistics.median(v),p95=sorted(v)[math.ceil(.95*len(v))-1],worst=max(v))
        expected = json.loads((SESSION/'tape-index.json').read_text())[tape]['maps'].count(m)
        row['coverage'] = 'COMPLETE' if row['n'] == expected else 'PARTIAL'
        row['scope'] = 'SELECTED_WINDOWS_ONLY'
        row['bar'] = 'UNQUALIFIED (targeted replay; no whole-map verdict)'
        row['window_p95_le_50ms'] = row['ms']['p95'] <= 50
        rows.append(row)
    (SESSION/'report.json').write_text(json.dumps(rows,indent=2)+'\n')


def main():
    manifest = json.loads(MANIFEST.read_text())
    if sha(KIT) != manifest['kit_sha256']:
        raise SystemExit('STOP kit hash mismatch')
    if sha(SESSION/'baseline.json') != manifest['baseline_sha256'] or sha(SESSION/'tape-index.json') != manifest['tape_index_sha256']:
        raise SystemExit('STOP baseline/index hash mismatch')
    for variant in manifest['selves'].values():
        if sha(Path(variant['path'])) != variant['sha256']: raise SystemExit('STOP SELF hash mismatch')
    for arm in manifest['arms']:
        if sha(SESSION / arm['config']) != arm['config_sha256']:
            raise SystemExit('STOP config hash mismatch: ' + arm['name'])
    parser = argparse.ArgumentParser(
        description='One contention arm per invocation; root observes between steps')
    parser.add_argument('--step', action='store_true', required=True)
    parser.parse_args()
    results = json.loads(RESULTS.read_text()) if RESULTS.exists() else []
    # A recording session that has already restored/stopped is not a replay session.
    state = json.loads((SESSION / 'state.json').read_text())
    if state.get('restored'):
        raise SystemExit('STOP restored session; start a fresh measurement session with preflight/receiver/observe/backup')
    if state.get('pending'):
        raise SystemExit('STOP pending session operation; root recovery required')
    if not kit.receiver_alive(kit.Sess(SESSION), state):
        raise SystemExit('STOP measurement receiver is not alive')
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
    candidate = manifest['selves'][arm['variant']]
    state = json.loads((SESSION / 'state.json').read_text())
    if (state.get('installed') or {}).get('eboot') != candidate['sha256']:
        run('install', candidate['path'], str(config))
    elif (state.get('config') or {}).get('sha256') != arm['config_sha256']:
        run('config', str(config))
    state = json.loads((SESSION / 'state.json').read_text())
    if state.get('game') != 'closed' or state.get('pending'):
        stop('session not closed/clean before launch: ' + arm['name'])
    if (state.get('installed') or {}).get('eboot') != candidate['sha256']:
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
    deadline = time.monotonic() + max(600, arm['steps'] * 0.2 + 180)
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
            observation = state.get('observation') or {}
            outcome = observation.get('quit_outcome')
            if outcome != 'not-running':
                # The replay exits by itself; a quit that still killed the app is anomalous.
                stop('quit outcome %r after a self-exiting replay: %s' % (outcome, arm['name']))
            run('log', 'check-config')
            # Re-read the drained log, then revalidate every replay/fatal gate on it: the
            # receipt must carry the done line from this final snapshot, not the earlier one.
            text = current()
            done_line = replay_gates(text, arm)
            spikes, frames = validate_spikes(text, arm)
            # The pending observation must still be valid when this step is declared
            # complete: same launch/generation/log bytes/session receiver.
            state = json.loads((SESSION / 'state.json').read_text())
            observation = state.get('observation') or {}
            try:
                kit.assert_transaction(kit.Sess(SESSION), observation.get('transaction'))
            except kit.Stop as e:
                stop('observation transaction changed: %s' % e)
            armlog = SESSION / 'logs' / ('arm-' + arm['name'] + '.log')
            armlog.write_text(text + '\n')
            results.append(dict(
                arm=arm['name'], tape=arm['tape'], tape_sha256=arm['tape_sha256'],
                steps=arm['steps'], checks=arm['checks'], window=arm['window'],
                variant=arm['variant'], samples=statistics(spikes, arm), spike_frames=len(frames), replay=done_line,
                config='PASS', exit='VERIFIED', quit=outcome,
                observation=dict(path=observation.get('path'), sha256=observation.get('sha256')),
                arm_log=str(armlog), log_sha256=sha(LOG)))
            RESULTS.write_text(json.dumps(results, indent=2) + '\n')
            report(results)
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
