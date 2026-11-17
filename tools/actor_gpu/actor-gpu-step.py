"""Root-only actor GPU experiment, one arm per invocation.

Fresh session; root backs up the owner, installs READY's candidate SELF and first
config, retrieves both tapes, then acknowledges each inspected exit PNG. The driver
never acknowledges, retries, restores or contacts transport directly; it calls the
unchanged rc1 kit. Both maps use their own control, then exact discard-free, labelled
picture-changing backface, then combined. Every arm keeps render_scale 0.75,
world_gxm live, gpu_timing off, gpu_mask empty, all omission diagnostics off.

Credit requires exact tape/config/SELF, full replay PASS/diff=events=0, every finite
481-frame GPU-wait row, zero dropped logs, config PASS, not-running exit, final drained
replay gates and assert_transaction on the captured observation. Failure latches STOP.
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
LOG = SESSION / 'logs' / 'arms.log'
MANIFEST = SESSION / 'READY.json'
ACTIVE = SESSION / 'actor-gpu-step-active'
RESULTS = SESSION / 'actor-gpu-step-results.json'

spec = importlib.util.spec_from_file_location('actor_gpu_kit', KIT)
kit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(kit)

sha = lambda p: hashlib.sha256(Path(p).read_bytes()).hexdigest()

FATAL = ('corpse opaque program failed', 'render gpu crash', 'amr vita world', '[lg/replay] diverge', '[lg/replay] event mismatch',
         'panicked at', 'invalid config', 'vita-sim error')
# Informational replay config records whose recorded value is absent are not failures:
# the tape was recorded by another build and legitimately contains no such key.
CONFIG_INFO = re.compile(r'\[lg/replay\] config (\S+) recorded=absent now=')
DONE_RE = re.compile(
    r'\[lg/replay\] done (\S+) steps=(\d+)/(\d+) reason=(\S+) checks=(\d+) match=(\d+) '
    r'diff=(\d+) first_diff=(\S+) event_mismatches=(\d+) seconds=[0-9.]+ result=(\S+)$')

MASK_CLASSES = ['sky', 'world_low', 'world_full', 'world_alpha', 'actors',
                'fx_alpha', 'fx_add', 'fx_other']
DIAG = ['diag_skip_fx', 'diag_skip_alpha_world', 'diag_skip_live_actors',
        'diag_skip_decals', 'diag_skip_water', 'diag_skip_sky', 'diag_skip_ant_corpses']


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
        # The real kit appends a sentence after "(not-running)."; the state
        # observation.quit_outcome is checked separately below. Do not anchor the end.
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


def expected_config(manifest, arm):
    base = json.loads(Path(manifest['base_path']).read_text())
    expected = dict(base)
    expected.update(arm['overrides'])
    return expected


def first_difference(a, b):
    for key in sorted(set(a) | set(b)):
        if a.get(key) != b.get(key):
            return '%s is %r, want %r' % (key, a.get(key), b.get(key))
    return 'no key differs'


def check_config_semantics(manifest, config, arm):
    if set(arm['mask']) - set(MASK_CLASSES):
        stop('arm names an unknown gpu_mask class: ' + arm['name'])
    if config != expected_config(manifest, arm):
        stop('config is not RC15 base plus declared overrides: ' + first_difference(
            config, expected_config(manifest, arm)) + ': ' + arm['name'])
    if config.get('replay_play') != arm['tape']:
        stop('replay_play differs from the arm tape: ' + arm['name'])
    if [config.get('spike_log_first_frame'), config.get('spike_log_last_frame')] != arm['window']:
        stop('spike window differs from READY: ' + arm['name'])
    if not abs(float(config.get('spike_log_ms', -1)) - 0.01) < 1e-12:
        stop('spike_log_ms is not 0.01: ' + arm['name'])
    if config.get('log_frame_time_every') != 0:
        stop('the miniquad timing report is not off: ' + arm['name'])
    # gpu_timing is survey-only on RC15 and cannot arm in a tape replay; all arms keep it off.
    if config.get('gpu_timing') != 'off':
        stop('gpu_timing is not off: ' + arm['name'])
    if config.get('gpu_mask') != arm['mask']:
        stop('gpu_mask is %r, want %r: %s'
             % (config.get('gpu_mask'), arm['mask'], arm['name']))
    if config.get('world_gxm') != arm['world_gxm']:
        stop('world_gxm is %r, want %r: %s'
             % (config.get('world_gxm'), arm['world_gxm'], arm['name']))
    if config.get('quality') != 'low' or not config.get('world_list_submit'):
        stop('quality/world_list_submit do not match the mask contract: ' + arm['name'])
    for key in DIAG:
        if config.get(key, False) is not False:
            stop('diag key %s is on in a gpu_mask package: %s' % (key, arm['name']))


def check_tape(arm):
    tape = SESSION / 'tapes' / arm['tape']
    if not tape.is_file():
        stop('tape missing; run tape get first: ' + arm['tape'])
    data = tape.read_bytes()
    if data[:9] != b'LGTAPE 1\n':
        stop('tape magic mismatch: ' + arm['tape'])
    if sha(tape) != arm['tape_sha256']:
        stop('tape hash differs from READY: ' + arm['tape'])
    steps = sum(1 for line in data.split(b'\n') if line.startswith(b'F '))
    checks = sum(1 for line in data.split(b'\n') if line.startswith(b'C '))
    if (str(steps), str(checks)) != (arm['steps'], str(arm['checks'])):
        stop('tape has steps/checks %d/%d, READY says %s/%s: %s'
             % (steps, checks, arm['steps'], arm['checks'], arm['tape']))


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
    frames = []
    for line in spikes:
        match = re.search(r'\bframe=(\d+)', line)
        if not match:
            stop('unparseable spike frame: ' + arm['name'])
        frames.append(int(match[1]))
    first, last = arm['window']
    if frames != list(range(first, last + 1)):
        stop('incomplete/duplicate spike window %d rows, want %d..%d: %s'
             % (len(frames), first, last, arm['name']))
    for line in spikes:
        metrics = dict(re.findall(r'\b(ms|sim|render|hud|present)=([^\s]+)', line))
        try:
            valid = all(k in metrics and math.isfinite(float(metrics[k])) and float(metrics[k]) >= 0
                        for k in ('ms', 'sim', 'render', 'hud', 'present'))
        except ValueError:
            valid = False
        if not valid:
            stop('invalid outer spike metrics: ' + arm['name'])
        if 'sim_split=' not in line:
            stop('missing sim split in the spike window: ' + arm['name'])
        # The experiment reads the EndScene wait. A row without both fields cannot answer
        # the question, and a row without gxm_end is the signature of an armed survey
        # (gpu_timing), which this package must never run.
        if ' gxm_end=' not in line or ' gxm_end_in=' not in line:
            stop('spike row lacks gxm_end/gxm_end_in: %d: %s' % (first, arm['name']))
        value = re.search(r'\bgxm_end=([^\s]+)', line)
        try:
            ok = value and math.isfinite(float(value[1])) and float(value[1]) >= 0
        except ValueError:
            ok = False
        if not ok:
            stop('invalid gxm_end: ' + arm['name'])
    # No npc= gate: wchess1 prints none in this window; centipede1 may, but it is not a
    # condition of a valid GPU-ablation arm.
    flush = [l for l in text.splitlines() if '[lg/spike] flushed' in l]
    if not flush:
        stop('no spike flush line: ' + arm['name'])
    drops = [dict(re.findall(r'(\w+)=(\S+)', l)).get('dropped') for l in flush]
    if any(d != '0' for d in drops):
        stop('dropped spike rows %r: %s' % (drops, arm['name']))
    return spikes, frames


def main():
    manifest = json.loads(MANIFEST.read_text())
    if str(KIT) != manifest['kit_path']:
        raise SystemExit('STOP kit path differs from READY: %s' % manifest['kit_path'])
    if sha(KIT) != manifest['kit_sha256']:
        raise SystemExit('STOP kit hash mismatch')
    if sha(Path(manifest['self_path'])) != manifest['self_sha256']:
        raise SystemExit('STOP SELF hash mismatch')
    if sha(Path(manifest['base_path'])) != manifest['base_sha256']:
        raise SystemExit('STOP base config hash mismatch')
    for arm in manifest['arms']:
        if sha(SESSION / arm['config']) != arm['config_sha256']:
            raise SystemExit('STOP config hash mismatch: ' + arm['name'])
    parser = argparse.ArgumentParser(
        description='One GPU-ablation arm per invocation; root observes between steps')
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
    config_path = SESSION / arm['config']
    config = json.loads(config_path.read_text())
    check_config_semantics(manifest, config, arm)
    check_tape(arm)
    ACTIVE.write_text('%d %s\n' % (index, arm['name']))
    if index:
        run('config', str(config_path))
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
                arm=arm['name'], map=arm['map'], lever=arm['lever'],
                mask=arm['mask'], world_gxm=arm['world_gxm'],
                tape=arm['tape'], window=arm['window'], spike_frames=len(frames),
                replay=done_line, config='PASS', exit='VERIFIED', quit=outcome,
                draw_diag=[l for l in text.splitlines() if '[lg/draw-diag]' in l],
                observation=dict(path=observation.get('path'),
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
