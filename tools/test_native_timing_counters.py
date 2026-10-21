#!/usr/bin/env python3
"""Adversarial tests on actual production Recorder counter exports, offline."""
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest
import native_timing_check as check
import native_timing_counter_check as counters

class ActualCounterTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root=Path(os.environ['ALICE_TIMING_TEST_OUTPUT']);cls.name='07'*16+'-0000000000000001'
        cls.base=cls.root/'raw-zero'/cls.name
        cls.m=json.loads((cls.base/'manifest.json').read_bytes())
        cls.d=json.loads((cls.base/'records.json').read_bytes())
    def checked(self,path):return check.check_capture(path,1,20_000_000,allow_test_clock=True)
    def modified(self,edit,reason):
        m,d=copy.deepcopy(self.m),copy.deepcopy(self.d);edit(m,d)
        path=self.root/'counter-cases'/self._testMethodName/self.name;path.mkdir(parents=True,exist_ok=False)
        raw=json.dumps(d,separators=(',',':')).encode();m.update(records_bytes=len(raw),records_sha256=hashlib.sha256(raw).hexdigest())
        (path/'records.json').write_bytes(raw);(path/'manifest.json').write_text(json.dumps(m))
        with self.assertRaises(check.Invalid) as caught:self.checked(path)
        self.assertEqual(str(caught.exception),reason)
    def test_zero_proves_only_containing_superset(self):
        r=self.checked(self.base)
        self.assertEqual(r['phase2a_status'],'PARTIAL_COUNTERS_ONLY_UNQUALIFIED')
        self.assertFalse(r['audio_qualified']);self.assertFalse(r['campaign_qualified'])
        for row in r['raw_counter_guards'].values():self.assertEqual(row['guard'],'ZERO_ON_CONTAINING_SUPERSET')
        self.assertIn('audio-producer-fences',r['phase2_unimplemented'])
    def test_positive_surrounding_delta_fails_exact_inside_count_unknown(self):
        r=self.checked(self.root/'raw-positive'/self.name)
        pcm=r['raw_counter_guards']['PcmStarvedBlocks']
        self.assertEqual((pcm['guard'],pcm['delta'],pcm['positive_inside_window']),('FAIL',1,'UNKNOWN'))
        self.assertFalse(r['audio_qualified']);self.assertFalse(r['campaign_qualified'])
    def test_missing_route_explicit_unknown(self):
        r=self.checked(self.root/'raw-missing'/self.name)
        self.assertEqual(r['raw_counter_guards']['MusicUnderrunFrames']['guard'],'UNKNOWN')
    def test_phase1_optional_extension_absence_explicit(self):
        r=self.checked(self.root/'exact'/self.name)
        self.assertEqual(r['phase2a_status'],'UNIMPLEMENTED');self.assertIsNone(r['raw_counter_guards'])
        self.assertFalse(r['audio_qualified'])
    def test_wrap(self):self.modified(lambda m,d:d['raw_counters']['snapshots'][-1].update(wrapped=True),'counter concurrent read/wrap fault')
    def test_busy(self):self.modified(lambda m,d:d['raw_counters']['snapshots'][-1].update(busy=True),'counter concurrent read/wrap fault')
    def test_missing_bracket(self):self.modified(lambda m,d:d['raw_counters']['snapshots'][-1].update(pre_us=None),'counter missing/backward bracket')
    def test_backwards_bracket(self):self.modified(lambda m,d:d['raw_counters']['snapshots'][-1].update(post_us=0),'counter missing/backward bracket')
    def test_wrong_clock(self):self.modified(lambda m,d:d['raw_counters']['pcm'].update(clock_id='UTC'),'counter clock domain')
    def test_missing_lifetime(self):self.modified(lambda m,d:d['raw_counters']['pcm'].update(lifetime_id=None),'counter lifetime change/unknown')
    def test_lifetime_change(self):self.modified(lambda m,d:d['raw_counters']['snapshots'][-1].update(lifetime_id=[4]*16),'counter lifetime change/unknown')
    def test_width(self):self.modified(lambda m,d:d['raw_counters']['pcm'].update(width_bits=64),'unexpected constant')
    def test_float(self):self.modified(lambda m,d:d['raw_counters']['snapshots'][-1].update(value=0.0),'invalid unsigned integer')
    def test_bool(self):self.modified(lambda m,d:d['raw_counters']['snapshots'][-1].update(value=True),'invalid unsigned integer')
    def test_unknown_key(self):self.modified(lambda m,d:d['raw_counters']['snapshots'][-1].update(filtered=0),'missing/extra object fields')
    def test_extra_subset_not_subtracted(self):self.modified(lambda m,d:d['raw_counters'].update(flush_subset=1),'unexpected constant')
    def test_drop_last(self):self.modified(lambda m,d:d['raw_counters']['snapshots'].pop(),'counter snapshot loss/saturation')
    def test_loss(self):self.modified(lambda m,d:d['raw_counters'].update(lost=1),'counter snapshot loss/saturation')
    def test_reorder(self):self.modified(lambda m,d:d['raw_counters']['snapshots'].__setitem__(slice(0,2),d['raw_counters']['snapshots'][0:2][::-1]),'counter snapshot identity/order')
    def test_unknown_field(self):self.modified(lambda m,d:d['raw_counters']['snapshots'][0].update(field='PcmAfterFlush'),'counter snapshot identity/order')
    def test_reset(self):self.modified(lambda m,d:d['raw_counters']['snapshots'][0].update(value=1),'counter reset/wrap')
    def test_dimensions(self):self.modified(lambda m,d:d['raw_counters'].update(snapshot_size=4096),'counter storage dimensions')
    def test_storage_receipt(self):self.modified(lambda m,d:d['raw_counters'].update(shared_bytes=1),'counter storage receipt')
    def test_binding_before_publication(self):self.modified(lambda m,d:d['raw_counters']['pcm'].update(activation_post_us=5),'counter missing/backward bracket')
    def test_zero_bracket_edges(self):
        rows=self.d['raw_counters']['snapshots'];before=copy.deepcopy(rows[0]);after=copy.deepcopy(rows[2])
        before.update(pre_us=19_999_999,post_us=20_000_000);after.update(pre_us=50_000_000,post_us=50_000_001)
        self.assertEqual(counters.zero_proof(before,after,20_000_000,50_000_000)['guard'],'ZERO_ON_CONTAINING_SUPERSET')
        before['post_us']+=1;self.assertEqual(counters.zero_proof(before,after,20_000_000,50_000_000)['guard'],'UNKNOWN')
        before['post_us']-=1;after['pre_us']-=1;self.assertEqual(counters.zero_proof(before,after,20_000_000,50_000_000)['guard'],'UNKNOWN')
    def test_zero_proof_rejects_cross_field_read(self):
        before,after=map(copy.deepcopy,(self.d['raw_counters']['snapshots'][0],self.d['raw_counters']['snapshots'][2]))
        after['field']='MusicUnderrunFrames'
        with self.assertRaises(check.Invalid) as caught:counters.zero_proof(before,after,20_000_000,50_000_000)
        self.assertEqual(str(caught.exception),'counter proof field/order')
    def test_zero_proof_rejects_invalid_window(self):
        before,after=self.d['raw_counters']['snapshots'][0],self.d['raw_counters']['snapshots'][2]
        with self.assertRaises(check.Invalid) as caught:counters.zero_proof(before,after,50_000_000,20_000_000)
        self.assertEqual(str(caught.exception),'counter proof window')
    def test_zero_proof_direct_api_rejects_bool_value(self):
        before,after=map(copy.deepcopy,(self.d['raw_counters']['snapshots'][0],self.d['raw_counters']['snapshots'][2]))
        after['value']=True
        with self.assertRaises(check.Invalid) as caught:counters.zero_proof(before,after,20_000_000,50_000_000)
        self.assertEqual(str(caught.exception),'invalid unsigned integer')
    def test_parseable_orphan_rejected_by_checker_and_cli(self):
        path=self.root/'parseable-orphan'/self.name
        json.loads((path/'records.json').read_bytes())
        with self.assertRaises(FileNotFoundError) as caught:self.checked(path)
        self.assertEqual(Path(caught.exception.filename),path/'manifest.json')
        result=subprocess.run([sys.executable,'-B',str(Path(check.__file__)),str(path),'--generation','1','--age-offset-us','20000000','--allow-test-clock'],capture_output=True,text=True)
        self.assertEqual(result.returncode,2);self.assertIn('manifest.json',result.stderr)
    def test_cli_counter_extension(self):
        result=subprocess.run([sys.executable,'-B',str(Path(check.__file__)),str(self.base),'--generation','1','--age-offset-us','20000000','--allow-test-clock'],capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertEqual(json.loads(result.stdout)['phase2a_status'],'PARTIAL_COUNTERS_ONLY_UNQUALIFIED')
if __name__=='__main__':unittest.main(verbosity=2)
