"""Adversarial checks of actual scheduled Rust Recorder export, not synthetic timing."""
import copy,hashlib,json,os,subprocess,sys,unittest
import native_timing_schedule_check as schedule
from native_timing_overlay import display_lines
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
class ReceiptBounds(unittest.TestCase):
    """Synthetic structural receipts isolate bounds; these tags are UNATTESTED."""
    def setUp(self):
        state=dict(environment_bits=0,pickup_bits=0,physics_ticks=0,pose_bits=[0]*5)
        token=dict(ordinal=1,viewer_seq=1,submitted_us=1_000_000,generation=1,operation=2,auth=list(range(16)))
        self.p=dict(schema='alice-native-window-v2',expected_map='tower2',ready_offset_us=100_000_000,
            ready_us=0,generation=1,start_us=100_000_000,end_us=130_000_000,review_deadline_us=90_000_000,
            review_ack_us=61_000_000,reviewed_token=copy.deepcopy(token),tokens=[token],token_total=1,token_max_age_us=60_000_000,
            acquired_begin_us=88_000_000,seal_request_us=132_000_000,start_state=state,end_state=state,fault=None,
            token_mac_kat=True,token_authentication='HMAC-SHA256/128; per-process random key withheld; native receipt UNATTESTED',
            physical_image_time='UNKNOWN; submitted token is not camera exposure time',campaign_qualified=False)
        self.m=dict(schedule_token_size=56,schedule_storage_bytes=128*56,viewer_seq=4,capture_stop_us=132_000_000)
        self.frames=[dict(begin_operation=2,viewer_seq=3,begin_us=88_000_000),dict(end_us=132_000_000)]
    def validate(self):schedule.validate(self.p,self.m,dict(map='tower2',ready_us=0),1,100_000_000,self.frames,100_000_000,130_000_000)
    def reject(self,reason):
        with self.assertRaisesRegex(c.Invalid,reason):self.validate()
    def second(self):
        token=copy.deepcopy(self.p['tokens'][0]);token.update(ordinal=2,viewer_seq=2,submitted_us=2_000_000,auth=[16]*16)
        self.p['tokens'].append(token);self.p['token_total']=2
        return token
    def test_exact_sixty_second_age(self):self.validate()
    def test_age_above_sixty_seconds(self):
        self.p['review_ack_us']+=1;self.reject('missing/late/unbound')
    def test_ack_at_deadline(self):
        for token in [self.p['tokens'][0],self.p['reviewed_token']]:token['submitted_us']=30_000_000
        self.p['review_ack_us']=90_000_000;self.validate()
    def test_ack_after_deadline(self):
        for token in [self.p['tokens'][0],self.p['reviewed_token']]:token['submitted_us']=30_000_001
        self.p['review_ack_us']=90_000_001;self.reject('missing/late/unbound')
    def test_ordinal_above_issued_total(self):
        self.p['reviewed_token']['ordinal']=2;self.reject('missing/late/unbound')
    def test_wrong_generation(self):
        self.p['reviewed_token']['generation']=2;self.reject('missing/late/unbound')
    def test_wrong_operation(self):
        self.p['reviewed_token']['operation']=3;self.reject('missing/late/unbound')
    def test_short_and_long_tags(self):
        for length in [0,15,17]:
            with self.subTest(length=length):
                self.p['reviewed_token']['auth']=[0]*length;self.reject('array type/length')
    def test_ring_tag_wrong_length(self):
        self.p['tokens'][0]['auth']=[0]*15;self.reject('array type/length')
    def test_tag_octet_range(self):
        self.p['reviewed_token']['auth'][0]=256;self.reject('invalid unsigned')
    def test_tag_reused_for_two_frames(self):
        self.second()['auth']=self.p['tokens'][0]['auth'][:];self.reject('reused authentication tag')
    def test_ordinal_reused_for_two_frames(self):
        self.second()['ordinal']=1;self.reject('duplicate token')
    def test_reversed_frame_chronology(self):
        self.second()['viewer_seq']=0;self.reject('token sequence')
    def test_frame_reused_for_two_tokens(self):
        self.second()['viewer_seq']=1;self.reject('reused frame reference')
    def test_same_ordinal_bound_to_a_second_frame(self):
        self.p['reviewed_token']['viewer_seq']=2;self.reject('conflicts with retained')
    def test_evicted_review_tag_reused_for_another_frame(self):
        # Full wrapped ring; reviewed ordinal 1 has been evicted and cannot be
        # authenticated by Python, but observable reuse still must reject.
        first=copy.deepcopy(self.p['reviewed_token'])
        self.p['tokens']=[dict(first,ordinal=i,viewer_seq=i,auth=list(i.to_bytes(16,'little'))) for i in range(129,257)]
        self.p['token_total']=256;self.m['viewer_seq']=300;self.frames[0]['viewer_seq']=299
        self.p['tokens'][0]['auth']=first['auth'][:];self.reject('reused reviewed reference')
    def test_evicted_review_reference_is_structurally_allowed(self):
        self.p['tokens']=[dict(self.p['reviewed_token'],ordinal=i,viewer_seq=i,auth=list(i.to_bytes(16,'little'))) for i in range(129,257)]
        self.p['token_total']=256;self.m['viewer_seq']=300;self.frames[0]['viewer_seq']=299;self.validate()

class ReviewRequest(unittest.TestCase):
    def invoke(self,auth):
        root=Path(os.environ['ALICE_TIMING_TEST_OUTPUT'])/'review-request-cases'/self._testMethodName
        root.mkdir(parents=True,exist_ok=True)
        output=root/'request.json'
        argv=[sys.executable,str(Path(__file__).with_name('native_timing_review_request.py')),
            '--run-id','07'*16,'--token','1','2','1000000','1','0','--auth',auth,'--start-us','40000000',
            '--checks',*[line.rsplit(' ck=',1)[1] for line in display_lines('07'*16,[1,2,1000000,1,0],'01'*16,40000000)],'--output',str(output)]
        result=subprocess.run(argv,capture_output=True,text=True)
        attempt=len(list(root.glob('command-*.json')))+1
        with (root/f'command-{attempt}.json').open('x') as log:
            json.dump(dict(argv=argv,exit=result.returncode,stdout=result.stdout,stderr=result.stderr),log)
        return result,output
    def test_exact_format_and_exclusive_output(self):
        result,output=self.invoke('01'*16);self.assertEqual(result.returncode,0,result.stderr)
        request=json.loads(output.read_text());self.assertEqual(request['token']['auth'],[1]*16)
        self.assertEqual(set(request),{'run_id','token'})
        original=output.read_bytes();result,_=self.invoke('01'*16)
        self.assertNotEqual(result.returncode,0);self.assertEqual(output.read_bytes(),original)
    def test_short_tag(self):
        result,output=self.invoke('01'*15);self.assertEqual(result.returncode,2);self.assertFalse(output.exists())
    def test_long_tag(self):
        result,output=self.invoke('01'*17);self.assertEqual(result.returncode,2);self.assertFalse(output.exists())
    def test_non_hex_tag(self):
        result,output=self.invoke('z'*32);self.assertEqual(result.returncode,2);self.assertFalse(output.exists())
if __name__=='__main__':unittest.main()
