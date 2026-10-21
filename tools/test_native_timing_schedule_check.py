"""Adversarial checks of actual scheduled Rust Recorder export, not synthetic timing."""
import copy,hashlib,json,os,unittest
from pathlib import Path
import native_timing_check as c
class ActualSchedule(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root=Path(os.environ['ALICE_TIMING_TEST_OUTPUT'])
        cls.original=cls.root/'scheduled'/('07'*16+'-0000000000000001')
        cls.m=json.loads((cls.original/'manifest.json').read_bytes());cls.d=json.loads((cls.original/'records.json').read_bytes())
    def test_actual_window_and_global_viewer_sequence(self):
        result=c.check_capture(self.original,1,40_000_000,allow_test_clock=True)
        self.assertEqual(result['N'],3000);self.assertGreater(result['selected_viewer_seq'][0],1)
        self.assertFalse(result['campaign_qualified']);self.assertFalse(result['method_qualified'])
    def altered(self,edit,reason):
        m,d=copy.deepcopy(self.m),copy.deepcopy(self.d);edit(m,d)
        p=self.root/'schedule-check-cases'/self._testMethodName/self.original.name;p.mkdir(parents=True,exist_ok=False)
        raw=json.dumps(d).encode();m['records_bytes']=len(raw);m['records_sha256']=hashlib.sha256(raw).hexdigest()
        (p/'records.json').write_bytes(raw);(p/'manifest.json').write_text(json.dumps(m))
        with self.assertRaisesRegex(c.Invalid,reason):c.check_capture(p,1,40_000_000,allow_test_clock=True)
    def test_late_review(self):self.altered(lambda m,d:d['schedule'].update(review_ack_us=30_000_001),'missing/late/unbound')
    def test_unbound_token(self):self.altered(lambda m,d:d['schedule']['reviewed_token'].update(ordinal=999),'missing/late/unbound')
    def test_hidden_failure(self):self.altered(lambda m,d:d['schedule'].update(fault='Interrupted'),'native schedule rejected')
    def test_changed_offset(self):self.altered(lambda m,d:d['schedule'].update(ready_offset_us=40_000_001),'residence-age identity')
    def test_early_stop(self):self.altered(lambda m,d:d['schedule'].update(seal_request_us=70_000_000),'scheduled automatic seal')
    def test_token_mutation(self):self.altered(lambda m,d:d['schedule']['tokens'][0].update(submitted_us=40_000_000),'token chronology')
    def test_memory_underreport(self):self.altered(lambda m,d:m.update(schedule_storage_bytes=0),'allocation budget')
    def test_extra_field(self):self.altered(lambda m,d:d['schedule'].update(approved=True),'missing/extra')
    def test_no_test_clock_relaxation(self):
        with self.assertRaisesRegex(c.Invalid,'clock domain'):c.check_capture(self.original,1,40_000_000)
if __name__=='__main__':unittest.main()
