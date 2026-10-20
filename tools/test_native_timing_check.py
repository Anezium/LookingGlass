"""Adversarial checker tests consume actual Rust Recorder/writer fixtures.
Run cargo tests with ALICE_TIMING_TEST_OUTPUT first, then this unittest module.
All new test evidence stays in that directory; no fixtures are removed/rewritten.
"""
import copy
import hashlib
import json
import os
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

    def modified(self, edit, expected=None, raw=None):
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
        if expected:
            self.assertIn(expected, str(result.exception))

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

    def test_gap(self): self.modified(lambda m,d:d['frames'][2500].update(begin_us=25_000_001))
    def test_sequence_gap(self): self.modified(lambda m,d:d['frames'][2500].update(viewer_seq=2502))
    def test_duplicate(self): self.modified(lambda m,d:d['frames'].__setitem__(2500,d['frames'][2499]))
    def test_reorder(self): self.modified(lambda m,d:d['frames'].__setitem__(slice(2500,2502),d['frames'][2500:2502][::-1]))
    def test_backward(self): self.modified(lambda m,d:d['frames'][2500].update(end_us=24_999_999))
    def test_missing_field(self): self.modified(lambda m,d:d['frames'][2500].pop('before'))
    def test_wrong_bool_type(self): self.modified(lambda m,d:d['frames'][2500].update(viewer_seq=True))
    def test_wrong_float_type(self): self.modified(lambda m,d:d['frames'][2500].update(begin_us=25_000_000.0))
    def test_null_clock(self): self.modified(lambda m,d:d['frames'][2500].update(begin_us=None))
    def test_cpu_outside(self): self.modified(lambda m,d:d['frames'][2500].update(cpu_end_us=25_020_001))
    def test_missing_cpu(self): self.modified(lambda m,d:d['frames'][2500].update(cpu_end_us=None))
    def test_missing_sim(self): self.modified(lambda m,d:d['frames'][2500].update(simulation=None))
    def test_sim_nan(self): self.modified(lambda m,d:d['frames'][2500]['simulation'].update(dt_bits=0x7fc00000))
    def test_sim_dt_cap(self): self.modified(lambda m,d:d['frames'][2500]['simulation'].update(dt_bits=0x3f800000))
    def test_physics_backwards(self): self.modified(lambda m,d:d['frames'][2500]['after'].update(physics_ticks=0))
    def test_cinema(self): self.modified(lambda m,d:d['frames'][2500].update(reasons=4))
    def test_modal(self): self.modified(lambda m,d:d['frames'][2500].update(reasons=16))
    def test_pause_in_settle(self): self.modified(lambda m,d:d['frames'][1500].update(reasons=8))
    def test_hidden_operation(self): self.modified(lambda m,d:d['frames'][2500].update(end_operation=2))
    def test_generation_crossing(self): self.modified(lambda m,d:d['frames'][2500].update(end_generation=2))
    def test_same_map_generation_change(self):
        def edit(m,d):
            w=copy.deepcopy(d['worlds'][0]);w.update(generation=2,ready_us=25_000_001,reason='Load')
            d['worlds'].append(w);m.update(world_count=2,generation_total=2)
            d['frames'][2500].update(end_generation=2,reasons=32)
        self.modified(edit)
    def test_clock_lifetime_utc_claim(self): self.modified(lambda m,d:m.update(clock_lifetime='UTC'))
    def test_missing_first_playable(self): self.modified(lambda m,d:d['events'][2].update(kind='Status'))
    def test_deep_malformed_json(self): self.modified(lambda m,d:None,raw=b'['*2000+b']'*2000)
    def test_unknown_identity(self): self.modified(lambda m,d:m.update(run_id=None))
    def test_clock_domain(self): self.modified(lambda m,d:d.update(clock_id='UTC'))
    def test_clock_fault(self): self.modified(lambda m,d:m.update(clock_fault='ReadFailed'))
    def test_arithmetic_overflow(self): self.modified(lambda m,d:m.update(arithmetic_overflow=True))
    def test_loss(self): self.modified(lambda m,d:m.update(lost_frames=1))
    def test_event_loss(self): self.modified(lambda m,d:m.update(lost_events=1))
    def test_saturation(self): self.modified(lambda m,d:m.update(frame_count=6144))
    def test_budget(self): self.modified(lambda m,d:m.update(allocated_storage_bytes=2*1024*1024))
    def test_missing_final_pending(self): self.modified(lambda m,d:m.update(pending_count=1))
    def test_missing_right_guard(self):
        def edit(m,d):
            d['frames']=d['frames'][:5000];m.update(frame_count=5000,completed=5000,viewer_seq=5000,last_viewer_seq=5000)
            d['events']=[e for e in d['events'] if e['viewer_seq']<=5000];m.update(event_count=len(d['events']),event_total=len(d['events']))
        self.modified(edit)
    def test_missing_left_guard(self): self.modified(lambda m,d:d['frames'].__delitem__(slice(0,1500)))
    def test_event_reorder(self): self.modified(lambda m,d:d['events'].__setitem__(slice(0,2),d['events'][0:2][::-1]))
    def test_milestone_hidden_inside_window(self):
        def edit(m,d):
            d['events'][-1].update(native_us=25_000_000,kind='ModalEnter')
        self.modified(edit)
    def test_missing_source_identity_stays_unknown(self):
        result=checked(self.exact,1,20_000_000)
        self.assertIn('UNKNOWN',result['metadata_assurance'])
        self.assertFalse(result['campaign_qualified'])
    def test_unknown_extra_field(self): self.modified(lambda m,d:d.update(qualification=True))
    def test_duplicate_json_key(self):
        self.modified(lambda m,d:None, raw=b'{"schema":"alice-native-phase1-v1","schema":"alice-native-phase1-v1"}')
    def test_partial_json(self): self.modified(lambda m,d:None,raw=b'{"schema":')
    def test_hash_mismatch(self):
        path=self.root/'checker-cases'/self._testMethodName/self.capture_name;path.mkdir(parents=True)
        (path/'manifest.json').write_text(json.dumps(self.manifest));(path/'records.json').write_bytes(b'{}')
        with self.assertRaises(check.Invalid):checked(path,1,20_000_000)
    def test_actual_repeated_export_rejected(self):
        with self.assertRaisesRegex(check.Invalid,'repeated cumulative'):
            checked(self.root/'repeated'/('07'*16+'-0000000000000002'),1,20_000_000)
    def test_actual_partial_writer_exports_rejected(self):
        for index in range(3):
            with self.subTest(index=index), self.assertRaises((OSError,check.Invalid)):
                checked(self.root/f'failure-{index}'/self.capture_name,1,20_000_000)
    def test_actual_unfinalized_pending_rejected(self):
        with self.assertRaisesRegex(check.Invalid,'pending'):
            checked(self.root/'pending'/self.capture_name,1,20_000_000)
    def test_window_overflow(self):
        with self.assertRaises(check.Invalid):checked(self.exact,1,2**64-1)

if __name__=='__main__':unittest.main(verbosity=2)
