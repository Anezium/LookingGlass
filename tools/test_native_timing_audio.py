"""Adversarial checks of real Rust producer journal exports."""
import copy,hashlib,json,math,os,struct,subprocess,sys,unittest
from pathlib import Path
import native_timing_check as check

def bits(v):return struct.unpack('<Q',struct.pack('<d',v))[0]
class ProducerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root=Path(os.environ['ALICE_TIMING_TEST_OUTPUT']);cls.name='07'*16+'-0000000000000001';cls.base=cls.root/'audio-straddle'/cls.name
        cls.m=json.loads((cls.base/'manifest.json').read_bytes());cls.d=json.loads((cls.base/'records.json').read_bytes())
    def checked(self,p):return check.check_capture(p,1,20_000_000,allow_test_clock=True)
    def modified(self,edit,reason=None):
        m,d=copy.deepcopy(self.m),copy.deepcopy(self.d);edit(m,d);path=self.root/'audio-cases'/self._testMethodName/self.name;path.mkdir(parents=True,exist_ok=False)
        raw=json.dumps(d,separators=(',',':')).encode();m.update(records_bytes=len(raw),records_sha256=hashlib.sha256(raw).hexdigest());(path/'records.json').write_bytes(raw);(path/'manifest.json').write_text(json.dumps(m))
        if reason is None:return self.checked(path)
        with self.assertRaises(check.Invalid) as e:self.checked(path)
        self.assertEqual(str(e.exception),reason)
    def test_actual_full_export_uses_both_whole_boundary_windows(self):
        r=self.checked(self.base);self.assertTrue(r['native_audio_data_complete']);self.assertEqual(r['producer_observations']['selected_fence_ids'],list(range(7,18)));self.assertEqual(r['producer_observations']['boundary_fence_ids'],[7,17]);self.assertFalse(r['audio_qualified']);self.assertFalse(r['campaign_qualified']);self.assertFalse(r['method_qualified'])
    def test_exact_edges_exclude_zero_overlap(self):
        r=self.checked(self.root/'audio-exact'/self.name);self.assertEqual(r['producer_observations']['selected_fence_ids'],list(range(20,50)));self.assertEqual(r['producer_observations']['boundary_fence_ids'],[])
    def high(self,m,d,index,value):
        a=d['audio_producer'];a['blocks'][index]['total_ms_bits']=bits(value);a['fences'][index]['quantile_ms_bits']=[bits(value)]*3;a['legacy'][index]['window']['mix']=[value]*3
    def test_left_boundary_p95(self):self.modified(lambda m,d:self.high(m,d,6,8.01),'producer intersecting window p95 exceeds 8ms')
    def test_right_boundary_p95(self):self.modified(lambda m,d:self.high(m,d,16,8.01),'producer intersecting window p95 exceeds 8ms')
    def test_exact_8ms(self):self.assertTrue(self.modified(lambda m,d:self.high(m,d,6,8.0))['native_audio_data_complete'])
    def test_next_float_above_8ms(self):self.modified(lambda m,d:self.high(m,d,6,math.nextafter(8.,math.inf)),'producer intersecting window p95 exceeds 8ms')
    def test_first_partial_legacy_n2_keeps_original_percentile_convention(self):
        def edit(m,d):d['audio_producer']['legacy'][0]['window'].update(blocks=2,mix=[2.,1.,2.])
        self.assertTrue(self.modified(edit)['native_audio_data_complete'])
    def test_legacy_n_mismatch(self):self.modified(lambda m,d:d['audio_producer']['legacy'][5]['window'].update(blocks=2),'producer legacy block count')
    def test_legacy_music_units(self):self.modified(lambda m,d:d['audio_producer']['legacy'][0]['window'].update(music=[0.,0.,0.,-1.,-1.,0.]),'producer legacy music units')
    def test_empty_intersecting_window_never_passes_as_zero(self):
        def edit(m,d):
            a=d['audio_producer'];a['blocks'].pop(6);a['receipt']['blocks_produced']-=1
            for i,b in enumerate(a['blocks'],1):b['seq']=i
            for i,f in enumerate(a['fences']):
                if i==6:f.update(n=0,first_block=0,last_block=0,quantile_ms_bits=[0,0,0],flags=4)
                elif i>6:f['first_block']-=1;f['last_block']-=1
            a['legacy'][6]['window'].update(blocks=0,mix=[0.,0.,0.])
        self.modified(edit,'producer empty intersecting fence')
    def test_crossing_block_keeps_whole_duration_by_completion(self):
        r=self.modified(lambda m,d:d['audio_producer']['blocks'][7].update(begin_us=21_999_999))
        self.assertTrue(r['native_audio_data_complete'])
    def test_huge_integer_in_float_field_fails_controlled(self):self.modified(lambda m,d:d['audio_producer']['legacy'][0]['window'].update(log_ms=2**2000),'producer nonfinite legacy value')
    def test_missing_complete_bundle(self):self.modified(lambda m,d:d['audio_producer'].update(status='Timeout'),'producer completion unavailable')
    def test_ack_missing(self):self.modified(lambda m,d:d['audio_producer'].update(terminal=0),'producer completion unavailable')
    def test_final_endpoint_missing(self):self.modified(lambda m,d:d['audio_producer']['receipt'].update(final_us=60_000_001),'producer final endpoints/membership')
    def test_partial_receipt(self):self.modified(lambda m,d:d['audio_producer']['receipt'].update(final_partial=False),'producer final partial receipt')
    def test_fence_gap(self):self.modified(lambda m,d:d['audio_producer']['fences'][7].update(begin_us=22_000_001),'producer fence adjacency')
    def test_fence_sequence(self):self.modified(lambda m,d:d['audio_producer']['fences'][7].update(seq=10),'producer fence adjacency')
    def test_block_id(self):self.modified(lambda m,d:d['audio_producer']['blocks'][5].update(seq=9),'producer block order')
    def test_block_order(self):self.modified(lambda m,d:d['audio_producer']['blocks'][5].update(begin_us=20_000_000),'producer block order')
    def test_block_nan(self):self.modified(lambda m,d:d['audio_producer']['blocks'][5].update(total_ms_bits=bits(float('nan'))),'producer invalid total_ms bits')
    def test_negative_units(self):self.modified(lambda m,d:d['audio_producer']['blocks'][5].update(total_ms_bits=bits(-1.)),'producer invalid total_ms bits')
    def test_quantile_bits(self):self.modified(lambda m,d:d['audio_producer']['fences'][5]['quantile_ms_bits'].__setitem__(1,bits(5.)),'producer quantile bits')
    def test_n(self):self.modified(lambda m,d:d['audio_producer']['fences'][5].update(n=2),'producer fence block IDs/N')
    def test_loss(self):self.modified(lambda m,d:d['audio_producer']['receipt'].update(flags=8),'producer fault/loss')
    def test_count(self):self.modified(lambda m,d:d['audio_producer']['receipt'].update(blocks_produced=9000),'producer count/loss/saturation')
    def test_legacy_id(self):self.modified(lambda m,d:d['audio_producer']['legacy'][0].update(fence_id=9),'producer legacy association/bracket')
    def test_legacy_drop_count(self):self.modified(lambda m,d:d['audio_producer']['receipt'].update(legacy_queue_dropped=1),'producer legacy outcome counts')
    def test_legacy_print_drop_does_not_erase_independent_evidence(self):
        def edit(m,d):
            a=d['audio_producer'];a['legacy'][-1]['emitted']=False;a['receipt']['legacy_emitted']-=1;a['receipt']['legacy_queue_dropped']+=1
        self.assertEqual(self.modified(edit)['producer_observations']['legacy_queue_drops'],1)
    def test_counter_association(self):self.modified(lambda m,d:d['audio_producer']['counter_lifetimes'].__setitem__(0,[9]*16),'producer counter association')
    def test_lifetime(self):self.modified(lambda m,d:d['audio_producer'].update(worker_id=0),'producer identity')
    def test_completed_identity(self):self.modified(lambda m,d:d['audio_producer']['completed_identity'].update(worker_id=900),'producer completed identity')
    def test_birth_clock(self):self.modified(lambda m,d:d['audio_producer']['lifecycle']['audio_birth']['creation'].update(fault='Backward'),'producer birth clock')
    def test_wrong_clock(self):self.modified(lambda m,d:d['audio_producer'].update(clock_id='UTC'),'producer identity')
    def test_worker_panic(self):self.modified(lambda m,d:d['audio_producer']['lifecycle'].update(worker_exit=3),'producer exit failure')
    def test_extra(self):self.modified(lambda m,d:d['audio_producer'].update(accepted=True),'missing/extra object fields')
    def test_boolean_id(self):self.modified(lambda m,d:d['audio_producer']['blocks'][0].update(seq=True),'invalid unsigned integer')
    def test_storage(self):self.modified(lambda m,d:d['audio_producer']['storage'].update(fixed_bytes=0),'producer storage dimensions')
    def test_cli_isolated_imports(self):
        r=subprocess.run([sys.executable,'-I','-B',str(Path(check.__file__)),str(self.base),'--generation','1','--age-offset-us','20000000','--allow-test-clock'],capture_output=True,text=True);self.assertEqual(r.returncode,0,r.stderr);self.assertTrue(json.loads(r.stdout)['native_audio_data_complete'])
if __name__=='__main__':unittest.main(verbosity=2)
