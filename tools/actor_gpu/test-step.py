"""Offline end-to-end tests for the gpu_mask ablation package.

The driver runs as a subprocess against a fake kit that speaks the same CLI and
state.json vocabulary as rc1kit.py. Only the kit is fake: config reconstruction, tape
hash/step/check gates, launch attribution, ordered spike window, post-exit replay
re-gating, config PASS, quit outcome and assert_transaction are exercised on the real
driver code. No socket, console, FTP, capture device or Terminal.app is used.

The real rc1kit.py is edited never; one compatibility test reads its pure parsers offline.
"""
import contextlib
import hashlib
import importlib.util
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from types import SimpleNamespace as Args

PKG = Path(__file__).resolve().parent
ASSERTIONS = [0]
for _name in list(dir(unittest.TestCase)):
    if _name.startswith('assert') and callable(getattr(unittest.TestCase, _name)):
        _orig = getattr(unittest.TestCase, _name)

        def _wrapped(self, *a, __orig=_orig, **kw):
            ASSERTIONS[0] += 1
            return __orig(self, *a, **kw)

        setattr(unittest.TestCase, _name, _wrapped)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write_json(path, data):
    Path(path).write_text(json.dumps(data, indent=2, sort_keys=True) + '\n')


def load_module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def tape_bytes(arm):
    data = b'LGTAPE 1\n'
    data += b''.join(b'F %d\n' % i for i in range(int(arm['steps'])))
    data += b''.join(b'C %d\n' % i for i in range(int(arm['checks'])))
    return data


class Fixture:
    def __init__(self, tmp):
        self.root = Path(tmp).resolve()
        self.pkg = self.root / 'gpu-ablation'
        self.pkg.mkdir()
        for name in ('actor-gpu-step.py', 'analyze.py', 'prepare.py'):
            shutil.copyfile(PKG / name, self.pkg / name)
        shutil.copytree(PKG / 'console-configs', self.pkg / 'console-configs')
        shutil.copytree(PKG / 'base', self.pkg / 'base')
        (self.pkg / 'tapes').mkdir()
        (self.pkg / 'logs').mkdir()
        (self.root / 'rc1-session').mkdir()
        self.fake_path = self.root / 'rc1-session' / 'rc1kit.py'
        shutil.copyfile(PKG / 'tests' / 'fake-kit.py', self.fake_path)
        self.fake = load_module(self.fake_path, 'fake_kit_%d' % time.time_ns())
        self.plan = json.loads((PKG / 'arm-plan.json').read_text())
        self.arms = [dict(arm) for arm in self.plan]
        hashes = {}
        for arm in self.arms:
            data = tape_bytes(arm)
            path = self.pkg / 'tapes' / arm['tape']
            path.write_bytes(data)
            hashes[arm['tape']] = hashlib.sha256(data).hexdigest()
            arm['tape_sha256'] = hashes[arm['tape']]
        self.selfpath = self.root / 'fake-self'
        self.selfpath.write_bytes(b'offline synthetic RC15 SELF')
        self.log = self.pkg / 'logs/arms.log'
        self.log.write_text('')
        write_json(self.pkg / 'backup.json', dict(files=[], absent=['ux0:data/alice/private/vita_sim.json']))
        write_json(self.pkg / 'READY.json', dict(
            package='actor-gpu', kit_path=str(self.fake_path), kit_sha256=sha(self.fake_path),
            self_path=str(self.selfpath), self_sha256=sha(self.selfpath),
            base_path=str(self.pkg / 'base/PL-rc15-release-play.json'),
            base_sha256=sha(self.pkg / 'base/PL-rc15-release-play.json'),
            arms=self.arms,
            tapes=[dict(name=n, sha256=h, required=True) for n, h in sorted(hashes.items())]))
        self.sess = self.fake.Sess(self.pkg)
        self.sess.save(dict(game='unknown', backup=True, console_written=False, installed=None,
                            config=None, pending=None,
                            log=dict(path=str(self.log), generation='fixture', pid=123)))
        with contextlib.redirect_stdout(io.StringIO()):
            self.fake.cmd_observe(self.sess, Args())
            observation = self.sess.state()['observation']
            self.fake.cmd_closed(self.sess, Args(observed=observation['path']))
            self.fake.cmd_install(self.sess, Args(eboot=str(self.selfpath),
                                                  config=str(self.pkg / self.arms[0]['config'])))

    def run(self, mode=None):
        env = dict(os.environ)
        if mode is not None:
            env['FAKE_MODE'] = json.dumps(mode)
        return subprocess.run([sys.executable, str(self.pkg / 'actor-gpu-step.py'), '--step'],
                              capture_output=True, text=True, env=env, timeout=180)

    def results(self):
        path = self.pkg / 'actor-gpu-step-results.json'
        return json.loads(path.read_text()) if path.exists() else []

    def ack(self):
        state = self.sess.state()
        with contextlib.redirect_stdout(io.StringIO()):
            self.fake.cmd_closed(self.sess, Args(observed=state['observation']['path']))

    def launches(self):
        return [l for l in self.log.read_text().splitlines() if ' | kit | launch ' in l]

    def quits(self):
        return [l for l in self.log.read_text().splitlines() if ' outcome=' in l]


class GpuAblation(unittest.TestCase):
    def test_arm_plan_matches_rc15_base_and_skips_impossible_classes(self):
        base = json.loads((PKG / 'base/PL-rc15-release-play.json').read_text())
        plan = json.loads((PKG / 'arm-plan.json').read_text())
        self.assertEqual(len(plan), 8)
        self.assertEqual([a['map'] for a in plan], ['wchess1']*4+['centipede1']*4)
        for arm in plan:
            config=json.loads((PKG/arm['config']).read_text())
            expected=dict(base); expected.update(arm['overrides'])
            self.assertEqual(config,expected)
            self.assertEqual(sha(PKG/arm['config']),arm['config_sha256'])
            self.assertEqual(config['gpu_mask'],[])
            self.assertEqual(config['world_gxm'],'live')
            self.assertEqual(arm['frames'],481)
            self.assertEqual(config['render_scale'],0.75)
            for key in ['diag_skip_fx','diag_skip_live_actors','diag_skip_alpha_world']:
                self.assertFalse(config.get(key,False))

    def test_scene_delta_pairs_the_same_frame(self):
        analyzer=load_module(PKG/'analyze.py','actor_gpu_analyzer_pairing')
        self.assertEqual(analyzer.paired_scene_deltas([
            {'sim_split':'scene:10.0','gxm_end':'1.0'},
            {'sim_split':'scene:1.0','gxm_end':'10.0'}]),[9.0,9.0])

    def test_full_slot_end_to_end_and_analysis(self):
        with tempfile.TemporaryDirectory() as tmp:
            fx = Fixture(tmp)
            for index, arm in enumerate(fx.arms):
                p = fx.run()
                self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
                self.assertIn('STEP COMPLETE (%d/8)' % (index + 1), p.stdout)
                results = fx.results()
                self.assertEqual([r['arm'] for r in results], [a['name'] for a in fx.arms[:index + 1]])
                result = results[-1]
                self.assertEqual(result['mask'], arm['mask'])
                self.assertEqual(result['world_gxm'], arm['world_gxm'])
                self.assertEqual(result['window'], arm['window'])
                self.assertEqual(result['spike_frames'], 481)
                self.assertEqual((result['config'], result['exit'], result['quit']),
                                 ('PASS', 'VERIFIED', 'not-running'))
                self.assertIn('result=PASS', result['replay'])
                self.assertIn('diff=0', result['replay'])
                self.assertIn('event_mismatches=0', result['replay'])
                armlog = Path(result['arm_log']).read_text()
                self.assertEqual(armlog.count('[lg/spike] frame='), 481)
                self.assertEqual(armlog.count('[lg/replay] done '), 1)
                self.assertEqual(result['replay'], next(
                    l for l in armlog.splitlines() if '[lg/replay] done ' in l))
                self.assertFalse((fx.pkg / 'actor-gpu-step-active').exists())
                fx.ack()
            self.assertEqual(len(fx.launches()), 8)
            self.assertEqual(len(fx.quits()), 8)
            p = fx.run()
            self.assertNotEqual(p.returncode, 0)
            self.assertIn('all arms complete', p.stdout + p.stderr)
            self.assertEqual(len(fx.launches()), 8)
            analysis = subprocess.run([sys.executable, str(fx.pkg / 'analyze.py')],
                                      capture_output=True, text=True, timeout=60)
            self.assertEqual(analysis.returncode, 0, analysis.stderr)
            rows = json.loads((fx.pkg / 'analysis.json').read_text())['arms']
            self.assertEqual(len(rows), 8)
            by_name = {r['arm']: r for r in rows}
            for map in ['wchess1','centipede1']:
                self.assertEqual(by_name[map+'-control']['delta_gxm_end_p95_vs_control'],0)
            for row in rows:
                self.assertEqual(set(row['frames']), {'850','962','1047','1048'} if row['map']=='wchess1' else {'850'})
            self.assertIn('spike frames', analysis.stdout)
            self.assertIn('dEnd95', analysis.stdout)

    def test_real_kit_parses_the_tagged_log_and_config_shape(self):
        # Pure offline readers from the real kit, never edited and never contacted.
        real_path = PKG.parent / 'rc1-session' / 'rc1kit.py'
        if not real_path.is_file():
            self.skipTest('real kit not present on this machine')
        real = load_module(real_path, 'real_kit_compat_%d' % time.time_ns())
        with tempfile.TemporaryDirectory() as tmp:
            log = Path(tmp) / 'compat.log'
            append = lambda text: log.write_text(log.read_text() + text + '\n') if log.exists() \
                else log.write_text(text + '\n')
            append('2026-11-17T00:00:00.000+00:00 | kit | launch ALICELG01 id=abc '
                   '(SELF 4d7569e5bca9, config 01a74d02402f)')
            append('2026-11-17T00:00:00.100+00:00 | kit | connection open id=1 fixture')
            append('2026-11-17T00:00:00.200+00:00 | vita | [conn=1] [lg/spike] frame=1 '
                   'ms=10.00 sim=1.00 render=2.00 hud=1.00 present=1.00 gxm_end=3.00 '
                   'sim_split=scene:3.00 gxm_end_in=scene:3.00/1')
            append('2026-11-17T00:00:00.300+00:00 | vita | [conn=1] [lg/replay] done '
                   'rw-20261116-wchess1-01.lgtape steps=6375/6375 reason=tape-end checks=213 '
                   'match=213 diff=0 first_diff=- event_mismatches=0 seconds=1.0 result=PASS')
            append('2026-11-17T00:00:00.400+00:00 | kit | connection closed id=1 fixture')
            rows = list(real.parse_log(log, connections=True))
            self.assertTrue(real.launch_matches(rows, 'abc'))
            lines = real.launch_lines(rows)
            vita = [r[3] for r in lines if r[2] == 'vita' and r[4] is not None]
            self.assertEqual(len(vita), 2)
            self.assertIn('[lg/spike] frame=1', vita[0])
            self.assertIn('[lg/replay] done', vita[1])
        for arm in json.loads((PKG / 'arm-plan.json').read_text()):
            config = json.loads((PKG / arm['config']).read_text())
            self.assertEqual(real.compare_config(config, config), [])

    def test_wrong_mask_or_world_gxm_stops_before_launch(self):
        with tempfile.TemporaryDirectory() as tmp:
            fx = Fixture(tmp)
            arm = fx.arms[0]
            config = json.loads((fx.pkg / arm['config']).read_text())
            config['gpu_mask'] = ['actors']
            (fx.pkg / arm['config']).write_text(json.dumps(config, indent=2, sort_keys=True) + '\n')
            arm['config_sha256'] = sha(fx.pkg / arm['config'])
            ready = json.loads((fx.pkg / 'READY.json').read_text())
            ready['arms'] = fx.arms
            write_json(fx.pkg / 'READY.json', ready)
            p = fx.run()
            self.assertNotEqual(p.returncode, 0)
            self.assertIn('gpu_mask', p.stdout + p.stderr)
            self.assertEqual(len(fx.launches()), 0)
            self.assertFalse((fx.pkg / 'actor-gpu-step-active').exists())
            self.assertEqual(fx.results(), [])

    def test_tampered_tape_stops_before_launch(self):
        with tempfile.TemporaryDirectory() as tmp:
            fx = Fixture(tmp)
            with open(fx.pkg / 'tapes' / fx.arms[0]['tape'], 'ab') as f:
                f.write(b'F 999999\n')
            p = fx.run()
            self.assertNotEqual(p.returncode, 0)
            self.assertIn('tape hash differs from READY', p.stdout + p.stderr)
            self.assertEqual(len(fx.launches()), 0)

    def test_missing_spike_frame_stops_after_wait_exit(self):
        with tempfile.TemporaryDirectory() as tmp:
            fx = Fixture(tmp)
            p = fx.run({'omit': fx.arms[0]['window'][0] + 5})
            self.assertNotEqual(p.returncode, 0)
            self.assertIn('incomplete/duplicate spike window', p.stdout + p.stderr)
            active = (fx.pkg / 'actor-gpu-step-active').read_text()
            self.assertIn('FAILED', active)
            self.assertEqual(fx.results(), [])
            self.assertEqual(len(fx.quits()), 1)
            self.assertEqual(fx.sess.state()['game'], 'awaiting-root-observation')

    def test_nonzero_replay_diff_stops_before_wait_exit(self):
        with tempfile.TemporaryDirectory() as tmp:
            fx = Fixture(tmp)
            p = fx.run({'diff': 1})
            self.assertNotEqual(p.returncode, 0)
            self.assertIn('replay done diff', p.stdout + p.stderr)
            self.assertEqual(fx.results(), [])
            self.assertEqual(len(fx.quits()), 0)
            self.assertIn('FAILED', (fx.pkg / 'actor-gpu-step-active').read_text())

    def test_duplicate_done_after_wait_exit_stops_on_final_log(self):
        with tempfile.TemporaryDirectory() as tmp:
            fx = Fixture(tmp)
            p = fx.run({'duplicate_done': True})
            self.assertNotEqual(p.returncode, 0)
            self.assertIn('ambiguous replay-done count 2', p.stdout + p.stderr)
            self.assertEqual(fx.results(), [])
            self.assertEqual(len(fx.quits()), 1)
            self.assertIn('FAILED', (fx.pkg / 'actor-gpu-step-active').read_text())

    def test_dropped_and_missing_gxm_rows_stop(self):
        for mode, needle in (({'drop': True}, 'dropped spike rows'),
                             ({'no_gxm': True}, 'lacks gxm_end/gxm_end_in')):
            with tempfile.TemporaryDirectory() as tmp:
                fx = Fixture(tmp)
                p = fx.run(mode)
                self.assertNotEqual(p.returncode, 0)
                self.assertIn(needle, p.stdout + p.stderr, p.stdout + p.stderr)
                self.assertEqual(fx.results(), [])
                self.assertIn('FAILED', (fx.pkg / 'actor-gpu-step-active').read_text())

    def test_shader_fallback_cannot_receive_credit(self):
        with tempfile.TemporaryDirectory() as tmp:
            fx=Fixture(tmp)
            result=fx.run({'shader_failed':True})
            self.assertNotEqual(result.returncode,0)
            self.assertIn('corpse opaque program failed',result.stdout+result.stderr)
            self.assertEqual(fx.results(),[])
            self.assertIn('FAILED',(fx.pkg/'actor-gpu-step-active').read_text())

    def test_nonfinite_and_event_mismatch_cannot_receive_credit(self):
        for mode in ({'nonfinite':True},{'events':True}):
            with tempfile.TemporaryDirectory() as tmp:
                fx=Fixture(tmp);result=fx.run(mode)
                self.assertNotEqual(result.returncode,0)
                self.assertEqual(fx.results(),[])
                self.assertIn('FAILED',(fx.pkg/'actor-gpu-step-active').read_text())

    def test_killed_quit_stops(self):
        with tempfile.TemporaryDirectory() as tmp:
            fx = Fixture(tmp)
            p = fx.run({'killed': True})
            self.assertNotEqual(p.returncode, 0)
            self.assertIn("quit outcome 'killed'", p.stdout + p.stderr)
            self.assertEqual(fx.results(), [])
            self.assertIn('FAILED', (fx.pkg / 'actor-gpu-step-active').read_text())

    def test_foreign_line_after_observation_stops_before_launch(self):
        with tempfile.TemporaryDirectory() as tmp:
            fx = Fixture(tmp)
            self.assertEqual(fx.run().returncode, 0)
            self.assertEqual(len(fx.results()), 1)
            fx.ack()
            p = fx.run({'foreign_after_config': True})
            self.assertNotEqual(p.returncode, 0)
            self.assertIn('observation invalid before launch', p.stdout + p.stderr)
            self.assertEqual(len(fx.launches()), 1)
            self.assertEqual(len(fx.results()), 1)
            self.assertIn('FAILED', (fx.pkg / 'actor-gpu-step-active').read_text())


def main():
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(GpuAblation)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    receipt = dict(schema='gpu-ablation-tests-2', tests=result.testsRun,
                   assertions=ASSERTIONS[0], failures=len(result.failures),
                   errors=len(result.errors), skipped=len(result.skipped),
                   result='OK' if result.wasSuccessful() else 'FAIL',
                   arms_end_to_end=8, spike_rows_per_arm=481,
                   kit='tests/fake-kit.py; one compatibility test reads the real kit log/config parsers offline; rc1kit.py never edited or contacted',
                   console_contact=False, ftp=False, ports='none', terminal_app=False)
    (PKG / 'TEST-RESULTS.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt))
    return 0 if result.wasSuccessful() else 1


if __name__ == '__main__':
    sys.exit(main())
