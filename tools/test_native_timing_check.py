"""Adversarial checker tests consume actual Rust Recorder/writer fixtures.
Run cargo tests with ALICE_TIMING_TEST_OUTPUT first, then this unittest module.
All new test evidence stays in that directory; no fixtures are removed/rewritten.
"""
import copy
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path
import unittest
import native_timing_check as check


def checked(*args):
    return check.check_capture(*args, allow_test_clock=True)


class ActualCaptureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root = Path(os.environ['ALICE_TIMING_TEST_OUTPUT'])
        cls.capture_name = '07'*16 + '-0000000000000001'
        cls.exact = cls.root/'exact'/cls.capture_name
        cls.manifest = json.loads((cls.exact/'manifest.json').read_bytes())
        cls.records = json.loads((cls.exact/'records.json').read_bytes())

    def modified(self, edit, expected, raw=None):
        m, d = copy.deepcopy(self.manifest), copy.deepcopy(self.records)
        edit(m, d)
        path = self.root/'checker-cases'/self._testMethodName/self.capture_name
        path.mkdir(parents=True, exist_ok=False)
        payload = json.dumps(d).encode() if raw is None else raw
        m['records_bytes'] = len(payload)
        m['records_sha256'] = hashlib.sha256(payload).hexdigest()
        (path/'records.json').write_bytes(payload)
        (path/'manifest.json').write_text(json.dumps(m))
        with self.assertRaises(check.Invalid) as result:
            checked(path, 1, 20_000_000)
        self.assertEqual(expected, str(result.exception))

    def test_injected_clock_rejected_by_default(self):
        with self.assertRaisesRegex(check.Invalid,'injected test clocks'):
            check.check_capture(self.exact,1,20_000_000)

    def test_exact_actual_native_recorder(self):
        r = checked(self.exact, 1, 20_000_000)
        self.assertEqual(r['N'], 3000)
        self.assertEqual(r['sum_full_frame_us'],30_000_000)
        self.assertEqual(r['boundary_viewer_seq'], [])
        self.assertFalse(r['campaign_qualified'])
        self.assertFalse(r['small_sample'])
        self.assertEqual(r['clock_id'],'host-injected-fake-us')
        self.assertEqual(r['provenance'],'UNATTESTED; self-consistent hash only')

    def test_straddles_whole_actual_intervals(self):
        r = checked(self.root/'straddle'/self.capture_name,1,20_000_000)
        self.assertEqual(r['selected_viewer_seq'],list(range(7,18)))
        self.assertEqual(r['boundary_viewer_seq'],[7,17])
        self.assertEqual(r['sum_full_frame_us'],33_000_000)
        self.assertEqual((r['left_overhang_us'],r['right_overhang_us']),(2_000_000,1_000_000))
        self.assertTrue(r['small_sample'])

    def test_one_long_frame_small_no_extension_no_clipping(self):
        r=checked(self.root/'long'/self.capture_name,1,20_000_000)
        self.assertEqual(r['N'],1)
        self.assertEqual(r['sum_full_frame_us'],60_000_000)
        self.assertEqual(r['boundary_viewer_seq'],[3])
        self.assertEqual((r['left_overhang_us'],r['right_overhang_us']),(10_000_000,20_000_000))
        self.assertTrue(r['small_sample'])

    def test_gap(self): self.modified(lambda m,d:d['frames'][2500].update(begin_us=25_000_001), expected='hidden interval gap/discontinuity')
    def test_sequence_gap(self): self.modified(lambda m,d:d['frames'][2500].update(viewer_seq=2502), expected='frame sequence/clock/duration')
    def test_duplicate(self): self.modified(lambda m,d:d['frames'].__setitem__(2500,d['frames'][2499]), expected='frame sequence/clock/duration')
    def test_reorder(self): self.modified(lambda m,d:d['frames'].__setitem__(slice(2500,2502),d['frames'][2500:2502][::-1]), expected='frame sequence/clock/duration')
    def test_backward(self): self.modified(lambda m,d:d['frames'][2500].update(end_us=24_999_999), expected='frame sequence/clock/duration')
    def test_missing_field(self): self.modified(lambda m,d:d['frames'][2500].pop('before'), expected='missing/extra object fields')
    def test_wrong_bool_type(self): self.modified(lambda m,d:d['frames'][2500].update(viewer_seq=True), expected='invalid unsigned integer')
    def test_wrong_float_type(self): self.modified(lambda m,d:d['frames'][2500].update(begin_us=25_000_000.0), expected='invalid unsigned integer')
    def test_null_clock(self): self.modified(lambda m,d:d['frames'][2500].update(begin_us=None), expected='frame sequence/clock/duration')
    def test_cpu_outside(self): self.modified(lambda m,d:d['frames'][2500].update(cpu_end_us=25_020_001), expected='CPU outside whole viewer interval')
    def test_missing_cpu(self): self.modified(lambda m,d:d['frames'][2500].update(cpu_end_us=None), expected='missing gameplay observation')
    def test_missing_sim(self): self.modified(lambda m,d:d['frames'][2500].update(simulation=None), expected='missing gameplay observation')
    def test_sim_nan(self): self.modified(lambda m,d:d['frames'][2500]['simulation'].update(dt_bits=0x7fc00000), expected='nonfinite simulation/state observation')
    def test_sim_dt_cap(self): self.modified(lambda m,d:d['frames'][2500]['simulation'].update(dt_bits=0x3f800000), expected='simulation dt observation')
    def test_physics_backwards(self): self.modified(lambda m,d:d['frames'][2500]['after'].update(physics_ticks=0), expected='physics counter backwards')
    def test_cinema(self): self.modified(lambda m,d:d['frames'][2500].update(reasons=4), expected='non-gameplay/generation/operation during settle/window')
    def test_modal(self): self.modified(lambda m,d:d['frames'][2500].update(reasons=16), expected='non-gameplay/generation/operation during settle/window')
    def test_pause_in_settle(self): self.modified(lambda m,d:d['frames'][1500].update(reasons=8), expected='non-gameplay/generation/operation during settle/window')
    def test_hidden_operation(self):
        def edit(m,d):
            m['operation_total']=2
            d['frames'][2500]['end_operation']=2
            for frame in d['frames'][2501:]: frame.update(begin_operation=2,end_operation=2)
            d['events'][-1]['operation']=2
        self.modified(edit, expected='unmarked operation crossing')
    def second_generation(self,m,d,marked):
        w=copy.deepcopy(d['worlds'][0]);w.update(generation=2,ready_us=25_000_001,reason='Load')
        d['worlds'].append(w);m.update(world_count=2,generation_total=2)
        d['frames'][2500].update(end_generation=2,reasons=32 if marked else 0)
        for frame in d['frames'][2501:]: frame.update(begin_generation=2,end_generation=2)
        event=copy.deepcopy(d['events'][0]);event.update(ordinal=4,native_us=25_000_001,viewer_seq=2501,generation=2,operation=1)
        d['events'].insert(-1,event)
        d['events'][-1].update(ordinal=5,generation=2)
        m.update(event_count=5,event_total=5)

    def test_generation_crossing(self):
        self.modified(lambda m,d:self.second_generation(m,d,False), expected='unmarked install crossing')
    def test_same_map_generation_change(self):
        self.modified(lambda m,d:self.second_generation(m,d,True), expected='non-gameplay/generation/operation during settle/window')
    def test_clock_lifetime_utc_claim(self): self.modified(lambda m,d:m.update(clock_lifetime='UTC'), expected='unexpected constant')
    def test_missing_first_playable(self): self.modified(lambda m,d:d['events'][2].update(kind='Status'), expected='missing/late/duplicate first-playable epoch')
    def test_deep_malformed_json(self): self.modified(lambda m,d:None,raw=b'['*2000+b']'*2000, expected='malformed JSON')
    def test_unknown_identity(self): self.modified(lambda m,d:m.update(run_id=None), expected='UNKNOWN run/world identity')
    def test_clock_domain(self): self.modified(lambda m,d:d.update(clock_id='UTC'), expected='payload identity')
    def test_clock_fault(self): self.modified(lambda m,d:m.update(clock_fault='ReadFailed'), expected='clock/arithmetic failure')
    def test_arithmetic_overflow(self): self.modified(lambda m,d:m.update(arithmetic_overflow=True), expected='clock/arithmetic failure')
    def test_loss(self): self.modified(lambda m,d:m.update(lost_frames=1), expected='record loss')
    def test_event_loss(self): self.modified(lambda m,d:m.update(lost_events=1), expected='record loss')
    def test_saturation(self): self.modified(lambda m,d:m.update(frame_count=6144), expected='count/saturation/loss')
    def test_budget(self): self.modified(lambda m,d:m.update(allocated_storage_bytes=2*1024*1024), expected='allocation budget')
    def test_missing_final_pending(self): self.modified(lambda m,d:m.update(pending_count=1), expected='unresolved pending interval')
    def test_missing_right_guard(self):
        def edit(m,d):
            d['frames']=d['frames'][:5000];m.update(frame_count=5000,completed=5000,viewer_seq=5001,last_viewer_seq=5000)
            d['events']=[e for e in d['events'] if e['viewer_seq']<=5000];m.update(event_count=len(d['events']),event_total=len(d['events']),capture_stop_viewer_seq=5001,capture_stop_us=d['frames'][-1]['end_us'])
        self.modified(edit, expected='missing complete guards around settle/window')
    def test_missing_left_guard(self):
        def edit(m,d):
            for frame in d['frames']:
                for key in ('begin_us','end_us','cpu_begin_us','cpu_end_us'):
                    if frame[key] is not None: frame[key]+=15_000_000
            m['capture_stop_us']+=15_000_000
        self.modified(edit, expected='missing complete guards around settle/window')
    def test_event_reorder(self): self.modified(lambda m,d:d['events'].__setitem__(slice(0,2),d['events'][0:2][::-1]), expected='event sequence/clock/kind')
    def test_milestone_hidden_inside_window(self):
        def edit(m,d):
            d['events'][-1].update(native_us=25_000_000,kind='ModalEnter')
        self.modified(edit, expected='milestone/operation inside settle/window')
    def test_missing_source_identity_stays_unknown(self):
        result=checked(self.exact,1,20_000_000)
        self.assertIn('UNKNOWN',result['metadata_assurance'])
        self.assertFalse(result['campaign_qualified'])
    def test_unknown_extra_field(self): self.modified(lambda m,d:d.update(qualification=True), expected='missing/extra object fields')
    def test_duplicate_json_key(self):
        self.modified(lambda m,d:None, raw=b'{"schema":"alice-native-phase1-v1","schema":"alice-native-phase1-v1"}', expected='duplicate JSON key')
    def test_partial_json(self): self.modified(lambda m,d:None,raw=b'{"schema":', expected='malformed JSON')
    def test_hash_mismatch(self):
        path=self.root/'checker-cases'/self._testMethodName/self.capture_name;path.mkdir(parents=True)
        (path/'manifest.json').write_text(json.dumps(self.manifest));(path/'records.json').write_bytes(b'{}')
        with self.assertRaisesRegex(check.Invalid,'^payload hash/length$'):checked(path,1,20_000_000)
    def test_actual_repeated_export_rejected(self):
        with self.assertRaisesRegex(check.Invalid,'repeated cumulative'):
            checked(self.root/'repeated'/('07'*16+'-0000000000000002'),1,20_000_000)
    def test_actual_partial_writer_exports_rejected(self):
        for index in range(3):
            with self.subTest(index=index):
                path=self.root/f'failure-{index}'/self.capture_name
                if index==2:
                    with self.assertRaisesRegex(check.Invalid,'^malformed JSON$'): checked(path,1,20_000_000)
                else:
                    with self.assertRaises(FileNotFoundError) as caught: checked(path,1,20_000_000)
                    self.assertEqual(Path(caught.exception.filename),path/'manifest.json')
    def test_actual_unfinalized_pending_rejected(self):
        with self.assertRaisesRegex(check.Invalid,'pending'):
            checked(self.root/'pending'/self.capture_name,1,20_000_000)
    def test_window_overflow(self):
        with self.assertRaisesRegex(check.Invalid,'^window arithmetic/settle$'):checked(self.exact,1,2**64-1)

    def test_actual_recovery_settle_and_window_rejected_for_operation(self):
        for name in ('recovery-settle','recovery-window'):
            with self.subTest(name=name), self.assertRaisesRegex(check.Invalid,'^non-gameplay/generation/operation during settle/window$'):
                checked(self.root/name/self.capture_name,1,20_000_000)

    def test_actual_buffer_failures_have_no_manifest_and_retries_reject(self):
        for name in ('Write','Flush','Sync','ZeroWrite'):
            base=self.root/f'buffer-failure-{name}'
            with self.subTest(name=name), self.assertRaises(FileNotFoundError) as caught:
                checked(base/self.capture_name,1,20_000_000)
            self.assertEqual(Path(caught.exception.filename),base/self.capture_name/'manifest.json')
            with self.subTest(retry=name), self.assertRaisesRegex(check.Invalid,'^repeated cumulative export$'):
                checked(base/('07'*16+'-0000000000000002'),1,20_000_000)

    def test_actual_postseal_loss_rejected(self):
        with self.assertRaisesRegex(check.Invalid,'^record loss$'):
            checked(self.root/'postseal-loss'/self.capture_name,1,20_000_000)

    def test_forged_clock_domain_can_only_be_structural_unattested(self):
        # Explicit threat-model regression: rehashing is not provenance proof.
        for domain in ('host-shared-Instant-session-us','vita-newlib-clock4-process-us'):
            m,d=copy.deepcopy(self.manifest),copy.deepcopy(self.records)
            m['clock_id']=d['clock_id']=domain
            path=self.root/'checker-cases'/self._testMethodName/domain/self.capture_name
            path.mkdir(parents=True,exist_ok=False)
            raw=json.dumps(d).encode();m.update(records_bytes=len(raw),records_sha256=hashlib.sha256(raw).hexdigest())
            (path/'records.json').write_bytes(raw);(path/'manifest.json').write_text(json.dumps(m))
            result=check.check_capture(path,1,20_000_000)
            self.assertEqual(result['clock_id'],domain)
            self.assertEqual(result['provenance'],'UNATTESTED; self-consistent hash only')
            self.assertFalse(result['campaign_qualified'])
            self.assertFalse(result['test_clock'])

    def test_cli_reports_domain_and_unattested_assurance(self):
        args=[sys.executable,'-B',str(Path(check.__file__)),str(self.exact),'--generation','1','--age-offset-us','20000000']
        rejected=subprocess.run(args,capture_output=True,text=True)
        self.assertEqual(rejected.returncode,2)
        self.assertEqual(rejected.stderr.strip(),'UNQUALIFIED phase1 records: clock domain; injected test clocks require explicit opt-in')
        accepted=subprocess.run(args+['--allow-test-clock'],capture_output=True,text=True)
        self.assertEqual(accepted.returncode,0,accepted.stderr)
        result=json.loads(accepted.stdout)
        self.assertEqual(result['clock_id'],'host-injected-fake-us')
        self.assertEqual(result['provenance'],'UNATTESTED; self-consistent hash only')
        self.assertTrue(result['test_clock'])
        self.assertFalse(result['campaign_qualified'])

if __name__=='__main__':unittest.main(verbosity=2)
