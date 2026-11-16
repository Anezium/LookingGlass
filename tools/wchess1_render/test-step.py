"""Offline potears1 arm tests with the real round-3 kit gates.

Only external transport is mocked: FTP, vitacompanion, capture and receiver
liveness. Config swap, launch attribution, wait-exit, closed, check-config and the
mutation guard are the real kit implementations. No socket, capture device or
console is used. Negatives pin the no-write observation rule, the not-running exit
requirement, the lever polarity check and every spike-window gate.
"""
import contextlib
import hashlib
import importlib.util
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace as Args

PACKAGE = Path(__file__).resolve().parent
sys.path.insert(0, str(PACKAGE.parent / 'rc1-session' / 'tests'))
import test_hardening as h  # noqa: E402

k = h.k
ROOT = h.ROOT
row = h.row
vita = h.vita
block = h.block
mock = h.mock

KILLED = 'Quit.'
NOT_RUNNING = 'Error: cannot quit the app. Is the TITLEID correct?'


class FakeFtp:
    """External transport only: accepts a write when the observation is still current."""

    def __init__(self, sess):
        self.sess = sess
        self.writes = []

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False

    def write(self, command, local, remote, expected):
        k.mutation_guard(self.sess, command)
        assert k.sha256_file(Path(local)) == expected, local
        self.writes.append((command, str(local), remote, expected))


class Potears1Step(unittest.TestCase):
    def load(self):
        spec = importlib.util.spec_from_file_location('potears1', PACKAGE / 'wchess1-step.py')
        a = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(a)
        return a

    def fixture(self, tmp):
        ready = json.loads((PACKAGE / 'TEST-MANIFEST.json').read_text())
        s = k.Sess(tmp)
        log = s.log_path('arms')
        log.touch()
        (s.root / 'console-configs').symlink_to(PACKAGE / 'console-configs',
                                                target_is_directory=True)
        (s.root / 'tapes').mkdir()
        arms = []
        for original in ready['arms']:
            arm = dict(original)
            data = ('LGTAPE 1\n%s\n' % arm['tape']).encode()
            (s.root / 'tapes' / arm['tape']).write_bytes(data)
            arm['tape_sha256'] = hashlib.sha256(data).hexdigest()
            arms.append(arm)
        selfpath = s.root / 'fake-self'
        selfpath.write_bytes(b'offline synthetic binary')
        k.write_json(s.root / 'READY.json', dict(
            kit_sha256=k.sha256_file(ROOT / 'rc1kit.py'),
            self_path=str(selfpath), self_sha256=k.sha256_file(selfpath), arms=arms))
        k.write_json(s.root / 'backup.json', dict(files=[], absent=[k.SIM]))
        state = s.state()
        state.update(backup=True, pending=None, installed=None, config=None,
                     log=dict(path=str(log), generation='fixture', pid=123))
        s.save(state)
        return s, log, selfpath, arms

    def bootstrap(self, s, selfpath, first_config):
        with mock.patch.object(k, 'receiver_alive', return_value=123), \
                mock.patch.object(k, 'capture_frame', side_effect=lambda p: p.write_bytes(b'PNG')), \
                mock.patch.object(k, 'lock_is_free', return_value=True), \
                mock.patch.object(k, 'FtpSession', FakeFtp), \
                contextlib.redirect_stdout(io.StringIO()):
            k.cmd_observe(s, Args())
            k.cmd_closed(s, Args(observed=s.state()['observation']['path']))
            before = Path(s.state()['log']['path']).read_bytes()
            k.cmd_install(s, Args(eboot=str(selfpath),
                                  config=str(PACKAGE / first_config)))
        after = Path(s.state()['log']['path']).read_bytes()
        suffix = after[len(before):]
        self.assertRegex(suffix, rb'^[^\n]+ \| kit \| install SELF [0-9a-f]{12} config '
                                 rb'[0-9a-f]{12} \(a4-control\.json\)\n$')
        proof = s.state()['verified_exit']['transaction']
        self.assertEqual((proof['log_size'], proof['log_sha256']),
                         (len(after), k.sha256_file(Path(s.state()['log']['path']))))

    def game_text(self, s, arm, mode):
        config = json.loads(s.stored(s.state()['config']['sha256']).read_text())
        first, last = arm['window']
        text = row('kit', 'connection open id=1 fixture') + block(config)
        text += vita('[lg/replay] play %s steps=1200 start=chapter:potears1 recorded_on=vita '
                     'package=offline checks_every=600 ended=true truncated=false'
                     % arm['tape'])
        # Real replays list every renderer key absent from the recording; informational.
        text += vita('[lg/replay] config renderer.vita_sim recorded=absent now=""')
        for frame in range(first, last + 1):
            if mode.get('omit') == frame:
                continue
            split = '' if mode.get('no_split') else \
                ' sim_split=input:0.80,interactions:0.47,movers:7.26,movement:0.83,npcs:11.64'
            npc = '' if mode.get('no_npc') else \
                ' npc=wildlife:3/1.69/0.37/1.25/0.07/0.00,plant:1/0.05/0.03/0.00/0.00/0.02,' \
                'snark:18/8.45/7.79/0.22/0.00/0.41,ant:3/1.15/0.81/0.28/0.00/0.05'
            text += vita('[lg/spike] frame=%d ms=51.14 sim=27.59 render=19.96 hud=1.24 '
                         'present=2.35%s%s' % (frame, split, npc))
        text += vita('[lg/spike] flushed at=replay-end lines=%d dropped=%d bytes=1 ms=1.0'
                     % (last - first + 1, 1 if mode.get('drop') else 0))
        result = 'FAIL' if mode.get('result') == 'FAIL' else 'PASS'
        text += vita('[lg/replay] done %s steps=%s/%s reason=tape-end checks=%s match=%s '
                     'diff=%d first_diff=- event_mismatches=%d seconds=1.0 result=%s'
                     % (arm['tape'], arm['steps'], arm['steps'], arm['checks'], arm['checks'], 1 if mode.get('diff') else 0,
                        2 if mode.get('events') else 0, result))
        text += vita('[lg/transition-memory] {"event":"end","kind":"quit","success":true}')
        text += row('kit', 'connection closed id=1 fixture')
        return text

    def harness(self, s, log, arms, mode):
        calls, outcomes = [], []

        def done_line(arm, diff='0', result='PASS'):
            return vita('[lg/replay] done %s steps=%s/%s reason=tape-end checks=%s match=%s '
                        'diff=%s first_diff=- event_mismatches=0 seconds=1.0 result=%s'
                        % (arm['tape'], arm['steps'], arm['steps'], arm['checks'],
                           arm['checks'], diff, result))

        def dispatcher(argv, **kwargs):
            args = tuple(argv[4:])
            calls.append(args)
            if args[0] == 'wait-exit':
                arm = next(x for x in arms
                           if x['config_sha256'] == s.state()['config']['sha256'])
                if mode.get('fatal_before_wait_exit'):
                    k.append_line(log, vita('[lg/replay] event mismatch step=1400 '
                                            'actor=alice-after-capture'))
                if mode.get('panicked_before_wait_exit'):
                    k.append_line(log, vita("[lg/replay] panicked at src/game.rs:42:9: "
                                            "index out of bounds: the len is 3 but the index is 4"))
                if mode.get('duplicate_done_before_wait_exit'):
                    k.append_line(log, done_line(arm, diff='1', result='DIVERGED'))
            out, err = io.StringIO(), io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                rc = k.main(['--session', str(s.root), *args])
            if args[0] == 'config' and mode.get('foreign_after_config'):
                k.append_line(log, row('vita', '[conn=1] foreign line after the observation'))
            if args[:2] == ('log', 'check-config') and mode.get('log_change_after_exit'):
                k.append_line(log, row('vita', '[conn=1] foreign line after the exit quorum'))
            if args[0] == 'wait-exit' and rc == 0:
                outcomes.append((s.state().get('observation') or {}).get('quit_outcome'))
            return Args(returncode=rc or 0, stdout=out.getvalue(), stderr=err.getvalue())

        def vita_cmd(line):
            if line.startswith('launch '):
                digest = s.state()['config']['sha256']
                arm = next(x for x in arms if x['config_sha256'] == digest)
                k.append_line(log, self.game_text(s, arm, mode))
                return 'ok'
            if line.startswith('quit '):
                return KILLED if mode.get('reply') == 'killed' else NOT_RUNNING
            raise AssertionError(line)

        return calls, outcomes, dispatcher, vita_cmd

    def run_ctx(self, a, s, log, arms, mode):
        calls, outcomes, dispatcher, vita_cmd = self.harness(s, log, arms, mode)
        stack = contextlib.ExitStack()
        stack.enter_context(mock.patch.object(a, 'SESSION', s.root))
        stack.enter_context(mock.patch.object(a, 'LOG', log))
        stack.enter_context(mock.patch.object(a, 'MANIFEST', s.root / 'READY.json'))
        stack.enter_context(mock.patch.object(a, 'ACTIVE', s.root / 'wchess1-step-active'))
        stack.enter_context(mock.patch.object(a, 'RESULTS', s.root / 'wchess1-step-results.json'))
        stack.enter_context(mock.patch.object(a, 'kit', k))
        stack.enter_context(mock.patch.object(k, 'receiver_alive', return_value=123))
        stack.enter_context(mock.patch.object(k, 'lock_is_free', return_value=True))
        stack.enter_context(mock.patch.object(k, 'EXIT_QUIET_SECONDS', 0))
        stack.enter_context(mock.patch.object(k, 'vita_cmd', side_effect=vita_cmd))
        stack.enter_context(mock.patch.object(k, 'capture_frame',
                                              side_effect=lambda p: p.write_bytes(b'PNG')))
        stack.enter_context(mock.patch.object(k, 'FtpSession', FakeFtp))
        stack.enter_context(mock.patch.object(a.subprocess, 'run', side_effect=dispatcher))
        stack.enter_context(mock.patch('sys.argv', ['wchess1-step.py', '--step']))
        stack.enter_context(contextlib.redirect_stdout(io.StringIO()))
        return stack, calls, outcomes

    def test_observation_install_and_every_arm(self):
        a = self.load()
        with tempfile.TemporaryDirectory() as tmp:
            s, log, selfpath, arms = self.fixture(tmp)
            self.bootstrap(s, selfpath, arms[0]['config'])
            stack, calls, outcomes = self.run_ctx(a, s, log, arms, {})
            with stack:
                for index, arm in enumerate(arms):
                    a.main()
                    self.assertEqual(s.state()['game'], 'awaiting-root-observation')
                    results = json.loads((s.root / 'wchess1-step-results.json').read_text())
                    self.assertEqual([r['arm'] for r in results], [x['name'] for x in arms[:index + 1]])
                    result = results[-1]
                    self.assertEqual(result['spike_frames'], 481)
                    self.assertEqual(result['window'], arm['window'])
                    self.assertEqual(result['lever'], arm['lever'])
                    self.assertEqual((result['config'], result['exit'], result['quit']),
                                     ('PASS', 'VERIFIED', 'not-running'))
                    self.assertIn('result=PASS', result['replay'])
                    self.assertIn('diff=0', result['replay'])
                    self.assertIn('event_mismatches=0', result['replay'])
                    armlog = (s.root / 'logs' / ('arm-' + arm['name'] + '.log')).read_text()
                    self.assertEqual(armlog.count('[lg/spike] frame='), 481)
                    done_lines = [l for l in armlog.splitlines() if '[lg/replay] done ' in l]
                    self.assertEqual(len(done_lines), 1)
                    self.assertEqual(result['replay'], done_lines[0])
                    self.assertFalse((s.root / 'wchess1-step-active').exists())
                    before = len(calls)
                    with self.assertRaises(k.Stop):
                        a.main()
                    self.assertEqual(len(calls), before)
                    k.cmd_closed(s, Args(observed=s.state()['observation']['path']))
                before = len(calls)
                with self.assertRaises(SystemExit):
                    a.main()
                self.assertEqual(len(calls), before)
            self.assertEqual(outcomes, ['not-running'] * len(arms))
            self.assertEqual(sum(c[0] == 'launch' for c in calls), len(arms))
            self.assertEqual(sum(c[0] == 'config' for c in calls), len(arms) - 1)
            self.assertEqual(sum(c[0] == 'wait-exit' for c in calls), len(arms))
            self.assertEqual(sum(c[:2] == ('log', 'check-config') for c in calls), len(arms))
            self.assertFalse(any(c[0] in ('closed', 'restore', 'mark') for c in calls))

    def test_wrong_lever_polarity_stops_before_launch(self):
        a = self.load()
        with tempfile.TemporaryDirectory() as tmp:
            s, log, selfpath, arms = self.fixture(tmp)
            self.bootstrap(s, selfpath, arms[0]['config'])
            manifest = json.loads((s.root / 'READY.json').read_text())
            control = next(x for x in manifest['arms'] if x['lever'] == 'control')
            arm = json.loads((PACKAGE / control['config']).read_text())
            arm[control['levers'][0]] = True
            path = s.root / 'console-configs' / 'wrong.json'
            path.write_text(json.dumps(arm))
            control['config'] = 'console-configs/wrong.json'
            control['config_sha256'] = k.sha256_file(path)
            k.write_json(s.root / 'READY.json', manifest)
            stack, calls, outcomes = self.run_ctx(a, s, log, manifest['arms'], {})
            with stack:
                with self.assertRaises(SystemExit) as stop:
                    a.main()
                self.assertIn('lever', str(stop.exception))
                self.assertEqual(sum(c[0] == 'launch' for c in calls), 0)
                # A pre-launch config refusal writes no step marker: nothing ran and
                # root can simply re-install the packaged config and retry.
                self.assertFalse((s.root / 'wchess1-step-active').exists())
                self.assertFalse((s.root / 'wchess1-step-results.json').exists())

    def test_foreign_line_after_observation_stops_before_launch(self):
        a = self.load()
        with tempfile.TemporaryDirectory() as tmp:
            s, log, selfpath, arms = self.fixture(tmp)
            self.bootstrap(s, selfpath, arms[0]['config'])
            stack, calls, outcomes = self.run_ctx(
                a, s, log, arms, {'foreign_after_config': True})
            with stack:
                a.main()
                self.assertEqual(len(json.loads(
                    (s.root / 'wchess1-step-results.json').read_text())), 1)
                k.cmd_closed(s, Args(observed=s.state()['observation']['path']))
                before = sum(c[0] == 'launch' for c in calls)
                with self.assertRaises(SystemExit) as stop:
                    a.main()
                self.assertIn('observation invalid before launch', str(stop.exception))
                self.assertEqual(sum(c[0] == 'launch' for c in calls), before)
                active = (s.root / 'wchess1-step-active').read_text()
                self.assertIn('FAILED', active)
                self.assertIn('observation invalid before launch', active)
                self.assertEqual(len(json.loads(
                    (s.root / 'wchess1-step-results.json').read_text())), 1)
                self.assertFalse((s.root / 'logs' / ('arm-' + arms[1]['name'] + '.log')).exists())
            self.assertEqual(outcomes, ['not-running'])

    def test_killed_after_replay_latches_and_never_completes(self):
        a = self.load()
        with tempfile.TemporaryDirectory() as tmp:
            s, log, selfpath, arms = self.fixture(tmp)
            self.bootstrap(s, selfpath, arms[0]['config'])
            mode = {'reply': 'killed'}
            stack, calls, outcomes = self.run_ctx(a, s, log, arms, mode)
            with stack:
                with self.assertRaises(SystemExit) as stop:
                    a.main()
                self.assertIn('quit outcome', str(stop.exception))
                self.assertEqual(s.state()['game'], 'awaiting-root-observation')
                self.assertFalse((s.root / 'wchess1-step-results.json').exists())
                active = (s.root / 'wchess1-step-active').read_text()
                self.assertIn('FAILED', active)
                self.assertIn("quit outcome 'killed'", active)
                self.assertFalse((s.root / 'logs' / ('arm-' + arms[0]['name'] + '.log')).exists())
                self.assertEqual(sum(c[0] == 'wait-exit' for c in calls), 1)
                self.assertFalse(any(c[:2] == ('log', 'check-config') for c in calls))
                k.cmd_closed(s, Args(observed=s.state()['observation']['path']))
                before = len(calls)
                with self.assertRaises(SystemExit):
                    a.main()
                self.assertEqual(len(calls), before)
                # Root recovery: remove the latched marker; the measured reply completes one step.
                (s.root / 'wchess1-step-active').unlink()
                mode['reply'] = 'not-running'
                a.main()
                results = json.loads((s.root / 'wchess1-step-results.json').read_text())
                self.assertEqual([r['arm'] for r in results], [arms[0]['name']])
                self.assertEqual(results[0]['quit'], 'not-running')
            self.assertEqual(outcomes, ['killed', 'not-running'])

    def test_incomplete_window_and_dropped_spike_latch(self):
        for mode, needle in (({'omit': 705}, 'incomplete/duplicate spike window'),
                             ({'drop': True}, 'dropped spike rows')):
            a = self.load()
            with tempfile.TemporaryDirectory() as tmp:
                s, log, selfpath, arms = self.fixture(tmp)
                self.bootstrap(s, selfpath, arms[0]['config'])
                stack, calls, outcomes = self.run_ctx(a, s, log, arms, mode)
                with stack:
                    with self.assertRaises(SystemExit) as stop:
                        a.main()
                    self.assertIn(needle, str(stop.exception))
                    self.assertFalse((s.root / 'wchess1-step-results.json').exists())
                    self.assertIn('FAILED', (s.root / 'wchess1-step-active').read_text())
                    self.assertFalse((s.root / 'logs' / ('arm-' + arms[0]['name'] + '.log')).exists())
                    self.assertEqual(sum(c[0] == 'wait-exit' for c in calls), 1)
                    self.assertEqual(sum(c[:2] == ('log', 'check-config') for c in calls), 1)

    def test_nonfinite_negative_or_missing_outer_metrics_latch(self):
        a=self.load()
        arm=json.loads((PACKAGE/'TEST-MANIFEST.json').read_text())['arms'][0]
        for value in ('nan','inf','-1','bad'):
            text='\n'.join('[lg/spike] frame=%d ms=%s sim=1 render=1 hud=1 present=1 sim_split=x npc=x'
                % (f,value) for f in range(*[arm['window'][0],arm['window'][1]+1]))
            text+='\n[lg/spike] flushed dropped=0'
            with self.assertRaises(SystemExit) as caught:a.validate_spikes(text,arm)
            self.assertIn('invalid outer spike metrics',str(caught.exception))

    def test_missing_split_or_snark_attribution_latches(self):
        for mode, needle in (({'no_split': True}, 'missing sim split'),
                             ({'no_npc': True}, 'missing NPC attribution')):
            a = self.load()
            with tempfile.TemporaryDirectory() as tmp:
                s, log, selfpath, arms = self.fixture(tmp)
                self.bootstrap(s, selfpath, arms[0]['config'])
                stack, calls, outcomes = self.run_ctx(a, s, log, arms, mode)
                with stack:
                    with self.assertRaises(SystemExit) as stop:
                        a.main()
                    self.assertIn(needle, str(stop.exception))
                    self.assertFalse((s.root / 'wchess1-step-results.json').exists())
                    self.assertIn('FAILED', (s.root / 'wchess1-step-active').read_text())
                    self.assertFalse((s.root / 'logs' / ('arm-' + arms[0]['name'] + '.log')).exists())

    def test_diff_and_event_mismatch_latch(self):
        for mode, needle in (({'diff': True}, 'replay done diff='),
                             ({'events': True}, 'replay done event_mismatches='),
                             ({'result': 'FAIL'}, 'replay done result=')):
            a = self.load()
            with tempfile.TemporaryDirectory() as tmp:
                s, log, selfpath, arms = self.fixture(tmp)
                self.bootstrap(s, selfpath, arms[0]['config'])
                stack, calls, outcomes = self.run_ctx(a, s, log, arms, mode)
                with stack:
                    with self.assertRaises(SystemExit) as stop:
                        a.main()
                    self.assertIn(needle, str(stop.exception))
                    self.assertFalse((s.root / 'wchess1-step-results.json').exists())
                    self.assertIn('FAILED', (s.root / 'wchess1-step-active').read_text())

    def test_post_exit_fatal_line_latches(self):
        """A fatal line that appears after the pre-wait check is still in the frozen log."""
        a = self.load()
        with tempfile.TemporaryDirectory() as tmp:
            s, log, selfpath, arms = self.fixture(tmp)
            self.bootstrap(s, selfpath, arms[0]['config'])
            stack, calls, outcomes = self.run_ctx(
                a, s, log, arms, {'panicked_before_wait_exit': True})
            with stack:
                with self.assertRaises(SystemExit) as stop:
                    a.main()
                self.assertIn('first failure line', str(stop.exception))
                self.assertIn('panicked at', str(stop.exception))
                self.assertFalse((s.root / 'wchess1-step-results.json').exists())
                self.assertIn('FAILED', (s.root / 'wchess1-step-active').read_text())
                self.assertEqual(sum(c[0] == 'wait-exit' for c in calls), 1)
                self.assertEqual(s.state()['game'], 'awaiting-root-observation')

    def test_post_exit_duplicate_done_latches(self):
        """A second replay-done after the pre-wait check is ambiguous, not success."""
        a = self.load()
        with tempfile.TemporaryDirectory() as tmp:
            s, log, selfpath, arms = self.fixture(tmp)
            self.bootstrap(s, selfpath, arms[0]['config'])
            stack, calls, outcomes = self.run_ctx(
                a, s, log, arms, {'duplicate_done_before_wait_exit': True})
            with stack:
                with self.assertRaises(SystemExit) as stop:
                    a.main()
                self.assertIn('ambiguous replay-done count 2', str(stop.exception))
                self.assertFalse((s.root / 'wchess1-step-results.json').exists())
                self.assertIn('FAILED', (s.root / 'wchess1-step-active').read_text())
                self.assertEqual(sum(c[0] == 'wait-exit' for c in calls), 1)

    def test_stale_observation_recovered_by_wait_exit(self):
        """closed and recover both refuse after a post-capture log change; wait-exit recovers."""
        a = self.load()
        with tempfile.TemporaryDirectory() as tmp:
            s, log, selfpath, arms = self.fixture(tmp)
            self.bootstrap(s, selfpath, arms[0]['config'])
            stack, calls, outcomes = self.run_ctx(
                a, s, log, arms, {'log_change_after_exit': True})
            with stack:
                with self.assertRaises(SystemExit) as stop:
                    a.main()
                self.assertIn('observation transaction changed', str(stop.exception))
                stale = s.state()['observation']['path']
                with self.assertRaises(k.Stop):
                    k.cmd_closed(s, Args(observed=stale))
                with self.assertRaises(k.Stop) as refused:
                    k.cmd_recover(s, Args())
                self.assertIn('Nothing to recover', str(refused.exception))
                # The pinned kit takes a new transaction when the exit proofs still hold.
                with mock.patch.object(k, 'EXIT_QUIET_SECONDS', 0):
                    self.assertEqual(k.cmd_wait_exit(s, Args(timeout=0)), 0)
                fresh = s.state()['observation']['path']
                self.assertNotEqual(fresh, stale)
                k.cmd_closed(s, Args(observed=fresh))
                self.assertEqual(s.state()['game'], 'closed')
                records = [json.loads(line) for line in
                           (s.root / 'session.jsonl').read_text().splitlines() if line.strip()]
                # Bootstrap observation, the failed step's stale capture, then the recovery.
                self.assertEqual(len([r for r in records if r.get('command') == 'observation']), 3)
                observed = [r.get('path') for r in records if r.get('command') == 'observation']
                self.assertIn(stale, observed)
                self.assertIn(fresh, observed)
                self.assertTrue(Path(stale).is_file())
                self.assertIn('FAILED', (s.root / 'wchess1-step-active').read_text())

    def test_failed_second_wait_exit_then_recover(self):
        """If the fresh wait-exit fails, it clears the pending observation and recover applies."""
        a = self.load()
        with tempfile.TemporaryDirectory() as tmp:
            s, log, selfpath, arms = self.fixture(tmp)
            self.bootstrap(s, selfpath, arms[0]['config'])
            stack, calls, outcomes = self.run_ctx(
                a, s, log, arms, {'log_change_after_exit': True})
            with stack:
                with self.assertRaises(SystemExit) as stop:
                    a.main()
                self.assertIn('observation transaction changed', str(stop.exception))
                stale = s.state()['observation']['path']
                with self.assertRaises(k.Stop):
                    k.cmd_closed(s, Args(observed=stale))
                # Timeout zero with the real quiet interval (the harness patches it to zero
                # for driver speed): the second wait-exit latches.
                with mock.patch.object(k, 'EXIT_QUIET_SECONDS', 6.0):
                    self.assertEqual(k.cmd_wait_exit(s, Args(timeout=0)), 4)
                self.assertTrue(s.state().get('exit_failed'))
                self.assertNotIn('observation', s.state())
                self.assertEqual(k.cmd_recover(s, Args()), 0)
                fresh = s.state()['observation']['path']
                self.assertNotEqual(fresh, stale)
                k.cmd_closed(s, Args(observed=fresh))
                self.assertEqual(s.state()['game'], 'closed')
                self.assertTrue(Path(stale).is_file())
                self.assertIn('FAILED', (s.root / 'wchess1-step-active').read_text())


if __name__ == '__main__':
    unittest.main(verbosity=2)
