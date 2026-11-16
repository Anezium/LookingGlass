"""Strict host oracle for the potears1 simulation candidates.

A reference run and one run per arm, per tape. Every frame's full simulation
state, ordered submitted packets, world packets, mover transforms/collider bits/
collision answers, presented image hash and replay summary are compared with no
tolerance and no per-actor exception. Only observational clocks and counters are
removed from the compared rows.

The reference binary is the task base commit built by build_host.py base; each arm
is the candidate binary with exactly one switch flipped in its renderer config.
Nothing here contacts the console.
"""
import argparse
import gzip
import hashlib
import json
import os
import shutil
import select
import subprocess
import sys
import time
from pathlib import Path

P = Path(__file__).resolve().parent
SOURCE = Path('/Users/anezium/vita-work/lg-wchess1')
DATA = Path('/Users/anezium/vita-work/alice-data/Alice1/bin/base')
TAPES = Path('/Users/anezium/vita-j0/plan-2026-11-12/a5-heavy/tapes')
LOCK = Path('/Users/anezium/vita-j0/mac-timing.lock')
HOST = P / 'host'
BINS = P / 'bin'

# Window is the 481-frame spike segment of each tape used by the console package.
TAPE_ROWS = {
    'a2-04.lgtape': (1000, 1480),
    'a4-wchess1-01.lgtape': (700, 1180),
    'rw-20261116-wchess1-01.lgtape': (796, 1966),
    'a4-skool1-01.lgtape': (700, 1180),
    'a4-potears1-01.lgtape': (330, 810),
}
# label -> (binary label, renderer-config overrides, extra environment). `base` is the
# reference binary from the untouched base commit; every other arm is the candidate
# binary with exactly one switch flipped.
ARMS = {
    'base': ('base', {}, {}),
    'arm-off': ('arm', {}, {}),
    'alpha-once': ('arm', {'world_alpha_parameter_once': True}, {}),
    'skin-reuse': ('arm', {'actor_skin_output_reuse': True}, {}),
    'combined': ('arm', {'world_alpha_parameter_once': True, 'actor_skin_output_reuse': True}, {}),
}
KEEP = ['frame', 'map', 'active', 'state_sha256', 'joint_alpha_order', 'blend_world_order',
        'world_packets', 'gl_trace', 'selected_lights_bits', 'world_viewport',
        'submitted_packets', 'renderer_drops', 'npc_decisions', 'mover_replay']
DROP = ['cpu_ms', 'spans_ms', 'counters', 'render']


def normalize(row):
    """Keep all payload fields and non-observer counters, including unknown fields."""
    row = json.loads(json.dumps(row))
    for key in ['cpu_ms', 'spans_ms']:
        row.pop(key, None)
    row['counters'] = {k:v for k,v in row.get('counters', {}).items()
        if not k.endswith(('_us','_ns','_ms'))
        and not k.startswith(('actor_mem_', 'actor_publication_mem_'))
        and k not in ('dynamic_colliders_tested','dynamic_colliders_rejected',
                      'skin_output_reused_vertices','world_alpha_parameter_once_hits')}
    for metrics in row.get('render', {}).values():
        for key in list(metrics):
            if key.endswith('_ns'): metrics.pop(key)
    for packet in row.get('submitted_packets', []):
        packet.pop('gpu_slot', None)
    return row


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def acquire(task):
    deadline = time.monotonic() + max(0, json.loads((P/'lock-wait.json').read_text())['deadline_epoch'] - time.time())
    while True:
        try:
            fd = os.open(LOCK, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        except FileExistsError:
            if time.monotonic() >= deadline:
                raise TimeoutError('host timing lock wait exceeded an hour')
            print('waiting for host timing lock', flush=True)
            try:
                watch = os.open(LOCK, os.O_RDONLY)
            except FileNotFoundError:
                continue
            try:
                queue = select.kqueue()
                try:
                    queue.control([select.kevent(watch, filter=select.KQ_FILTER_VNODE,
                        flags=select.KQ_EV_ADD | select.KQ_EV_ONESHOT,
                        fflags=select.KQ_NOTE_DELETE | select.KQ_NOTE_RENAME)], 1, 60)
                finally:
                    queue.close()
            finally:
                os.close(watch)
            continue
        os.write(fd, json.dumps(dict(pid=os.getpid(), task=task)).encode())
        os.close(fd)
        return


def release():
    try:
        if json.loads(LOCK.read_text()).get('pid') == os.getpid():
            LOCK.unlink()
    except (OSError, ValueError):
        pass


def run(label, tape, binary, overrides, env_extra=None, decisions=False, force=False):
    env_extra = env_extra or {}
    first, last = TAPE_ROWS[tape]
    out = HOST / (tape + '-' + label)
    if out.exists() and not force:
        cached = out / 'oracle.json'
        if cached.is_file() and json.loads(cached.read_text()).get('binary_sha256') == \
                sha(binary) and json.loads(cached.read_text()).get('overrides') == overrides and \
                json.loads(cached.read_text()).get('env_extra') == env_extra and \
                json.loads(cached.read_text()).get('decisions') == decisions:
            print('cached', out.name, flush=True)
            return json.loads(cached.read_text())
    if out.exists():
        raise SystemExit('refusing to replace existing evidence: ' + str(out))
    if shutil.disk_usage('/Users').free < 41 * 1024**3:
        raise SystemExit('disk reserve below 41 GiB')
    private = out / 'private'
    private.mkdir(parents=True)
    config = json.loads((SOURCE / 'configs/PL-release.json').read_text())
    config.update(replay_play=tape, spike_log_ms=0.001,
                  spike_log_first_frame=first, spike_log_last_frame=last)
    config.update(overrides)
    (private / 'vita_renderer.json').write_text(json.dumps(config))
    shutil.copy2(SOURCE / 'configs/vita-sim/vita-sim-release.json', private / 'vita_sim.json')
    os.symlink((Path('/Users/anezium/vita-j0/plan-2026-11-12/release-week/play-record/tapes') if tape.startswith('rw-') else TAPES) / tape, private / tape)
    env = {k: os.environ[k] for k in ['HOME', 'USER', 'TMPDIR'] if k in os.environ}
    env.update(PATH='/usr/bin:/bin', LANG='C', LOOKING_GLASS_REPLAY_CROSS='1',
               LOOKING_GLASS_VITA_CONTROLS='1', LOOKING_GLASS_SIM_MAPS_PROBE='1',
               LOOKING_GLASS_SKIN_PROOF='1', LOOKING_GLASS_WORLD_PACKETS='1',
               LOOKING_GLASS_PROBE_KEEP_WINDOW='1', LOOKING_GLASS_SHOT_FRAMES='0',
               LOOKING_GLASS_MOVER_REPLAY='1')
    if decisions:
        env['LOOKING_GLASS_NPC_DECISIONS'] = '1'
    env.update(env_extra)
    signatures, images, presentation, done, events = [], [], [], [], []
    coverage = {k:0 for k in ('skin_output_reused_vertices','world_alpha_parameter_once_hits')}
    started = time.time()
    with (out / 'stderr.log').open('w') as err, \
            gzip.open(out / 'stdout.log.gz', 'wt', encoding='utf-8') as log:
        proc = subprocess.Popen(
            [str(binary), '--data', str(DATA), '--save-dir', str(private / 'saves')],
            cwd=out, env=env, stdout=subprocess.PIPE, stderr=err, text=True)
        for line in proc.stdout:
            log.write(line)
            if line.startswith('SIM_MAPS_FRAME '):
                row = json.loads(line.split(' ', 1)[1])
                for key in coverage: coverage[key] += row.get('counters',{}).get(key,0)
                row = normalize(row)
                signatures.append([row['frame'], hashlib.sha256(json.dumps(
                    row,
                    sort_keys=True, separators=(',', ':')).encode()).hexdigest()])
            elif line.startswith('SKIN_EXACT_IMAGE '):
                images.append(json.loads(line.split(' ', 1)[1]))
            elif line.startswith('SKIN_EXACT_PRESENTATION '):
                presentation.append(json.loads(line.split(' ', 1)[1]))
            elif '[lg/replay] event ' in line:
                events.append(line.strip())
            elif '[lg/replay] done ' in line:
                done.append(line.strip())
        code = proc.wait()
    if code != 0 or len(done) != 1:
        raise SystemExit('run failed rc=%s done=%d: %s' % (code, len(done), out))
    fields = dict(__import__('re').findall(r'(\w+)=([^\s]+)', done[0]))
    # The native tape may diverge from the host control on this map (a2-04 and
    # potears1 both do at step 600 in the base commit); every candidate must
    # reproduce the base run's exact replay summary and every-frame payload.
    if fields['reason'] != 'tape-end' or \
            fields['result'] not in ('PASS', 'DIVERGED'):
        raise SystemExit('replay gate failed: ' + done[0])
    step, total = map(int, fields['steps'].split('/'))
    if step != total:
        raise SystemExit('replay did not finish the tape: ' + done[0])
    if not signatures or len(images) != len(signatures):
        raise SystemExit('frame/image count mismatch %d/%d' % (len(signatures), len(images)))
    result = dict(label=label, tape=tape, overrides=overrides, env_extra=env_extra,
                  binary=str(binary),
                  binary_sha256=sha(binary), frames=len(signatures),
                  replay={k: v for k, v in fields.items() if k != 'seconds'},
                  done=done[0], decisions=decisions,
                  signatures=signatures, images=images, presentation=presentation, events=events, coverage=coverage,
                  elapsed_s=round(time.time() - started, 1))
    (out / 'oracle.json').write_text(json.dumps(result) + '\n')
    print('%s %s frames=%d %.1fs' % (label, tape, len(signatures), result['elapsed_s']),
          flush=True)
    return result


def compare(reference, arm):
    for key in ['frames', 'signatures', 'images', 'presentation', 'events', 'replay']:
        if reference[key] != arm[key]:
            if key == 'signatures':
                for i, (a, b) in enumerate(zip(reference[key], arm[key])):
                    if a != b:
                        raise SystemExit('%s differs from %s at signature row %d: %r != %r'
                                         % (arm['label'], reference['label'], i, a, b))
            raise SystemExit('%s differs from %s: %s' % (arm['label'], reference['label'], key))
    return dict(tape=arm['tape'], reference=reference['label'], candidate=arm['label'],
                frames=arm['frames'], exact=True, coverage=arm.get('coverage',{}),
                replay=arm['done'].split('seconds=')[0].strip(),
                arm_binary_sha256=arm['binary_sha256'])


def main():
    parser = argparse.ArgumentParser(description='potears1 strict host oracle')
    parser.add_argument('--tape', action='append', choices=sorted(TAPE_ROWS))
    parser.add_argument('--arm', action='append', choices=sorted(ARMS))
    parser.add_argument('--force', action='store_true')
    args = parser.parse_args()
    tapes = args.tape or list(TAPE_ROWS)
    arms = args.arm or list(ARMS)
    if 'base' not in arms:
        arms = ['base'] + arms
    if not (BINS / 'lg-host-base').is_file():
        raise SystemExit('missing bin/lg-host-base')
    rows = []
    for tape in tapes:
        reference = run('base', tape, BINS / 'lg-host-base', {},
                        decisions=('potears1' in tape), force=args.force)
        for label in arms:
            if label == 'base':
                continue
            binary_label, overrides, env_extra = ARMS[label]
            arm = run(label, tape, BINS / ('lg-host-' + binary_label), overrides,
                      env_extra=env_extra, decisions=('potears1' in tape),
                      force=args.force)
            rows.append(compare(reference, arm))
    (P / 'host-proof.json').write_text(json.dumps(rows, indent=2) + '\n')
    print(json.dumps(rows, indent=2))


if __name__ == '__main__':
    acquire('potears1 host oracle ' + ' '.join(sys.argv[1:]))
    try:
        main()
    finally:
        release()
