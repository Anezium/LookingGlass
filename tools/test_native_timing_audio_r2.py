"""Adversarial predicates for the strict producer checker, using real Rust exports.
Each case edits an ACTUAL Rust export (audio-straddle) and pins ONE predicate by its exact reason."""
import copy,hashlib,json,math,os,struct,unittest
from pathlib import Path
import native_timing_check as check
def bits(v):return struct.unpack('<Q',struct.pack('<d',v))[0]
def renumber(a):
    for i,b in enumerate(a['blocks'],1):b['seq']=i
class ProducerR2(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root=Path(os.environ['ALICE_TIMING_TEST_OUTPUT']);cls.name='07'*16+'-0000000000000001'
        cls.base=cls.root/'audio-straddle'/cls.name
        cls.m=json.loads((cls.base/'manifest.json').read_bytes());cls.d=json.loads((cls.base/'records.json').read_bytes())
    def run_edit(self,edit):
        m,d=copy.deepcopy(self.m),copy.deepcopy(self.d);edit(m,d);path=self.root/os.environ.get('R2_CASES','audio-r2-cases')/self._testMethodName/self.name;path.mkdir(parents=True,exist_ok=False)
        raw=json.dumps(d,separators=(',',':')).encode();m.update(records_bytes=len(raw),records_sha256=hashlib.sha256(raw).hexdigest())
        (path/'records.json').write_bytes(raw);(path/'manifest.json').write_text(json.dumps(m))
        return check.check_capture(path,1,20_000_000,allow_test_clock=True)
    def rejects(self,edit,reason):
        with self.assertRaises(check.Invalid) as e:self.run_edit(edit)
        self.assertEqual(str(e.exception),reason)
    # --- multi-block windows: rebuild fence k with explicit totals, keeping everything else exact
    def window(self,d,k,values):
        a=d['audio_producer'];f=a['fences'];fe=f[k];span=fe['end_us']-fe['begin_us'];n=len(values)
        i0=fe['first_block']-1;step=span//(n+1)
        new=[dict(seq=0,begin_us=fe['begin_us']+1+j*step,end_us=fe['begin_us']+(j+1)*step,total_ms_bits=bits(v)) for j,v in enumerate(values)]
        a['blocks'][i0:i0+fe['n']]=new;renumber(a);delta=n-fe['n']
        for j,g in enumerate(f):
            if j==k:g.update(n=n,first_block=i0+1,last_block=i0+n)
            elif j>k and g['n']:g['first_block']+=delta;g['last_block']+=delta
        a['receipt']['blocks_produced']+=delta
        o=sorted(values);q=[o[n//2],o[(n-1)*95//100],o[n-1]];fe['quantile_ms_bits']=[bits(v) for v in q]
        if k<len(a['legacy']):a['legacy'][k]['window'].update(blocks=n,mix=q)
        return q
    def actual(self, name):return check.check_capture(self.root/name/self.name,1,20_000_000,allow_test_clock=True)
    def test_twenty_block_window_uses_n_minus_one_p95_index(self):
        r=self.actual('audio-n20-index')['producer_observations']
        self.assertEqual(r['N'],[20]*11);self.assertEqual(r['p95_ms'],[5.5]*11)
    def test_p95_above_8_with_p50_below_fails(self):
        with self.assertRaises(check.Invalid) as e:self.actual('audio-p95-fail')
        self.assertEqual(str(e.exception),'producer intersecting window p95 exceeds 8ms')
    def test_max_above_8_with_p95_below_passes(self):
        r=self.actual('audio-max-pass');self.assertTrue(r['native_audio_data_complete'])
        self.assertEqual(r['producer_observations']['p95_ms'],[1.0]*11)
    def test_twentyone_block_window_index(self):
        r=self.actual('audio-n21-index')['producer_observations']
        self.assertEqual(r['N'],[21]*11);self.assertEqual(r['p95_ms'],[4.75]*11)
    def test_signed_zero_uses_total_cmp_order_on_actual_export(self):
        r=self.actual('audio-signed-zero');self.assertTrue(r['native_audio_data_complete'])
        a=json.loads((self.root/'audio-signed-zero'/self.name/'records.json').read_bytes())['audio_producer']
        self.assertEqual([b['total_ms_bits'] for b in a['blocks'][:4]],[0,2**63,0,2**63])
        self.assertEqual(a['fences'][8]['quantile_ms_bits'],[0,0,0])
    def test_checker_recomputes_quantile_bits_for_multiblock(self):
        def edit(m,d):
            q=self.window(d,8,[1.0+0.25*j for j in range(20)]);fe=d['audio_producer']['fences'][8];fe['quantile_ms_bits'][1]=bits(q[2]);d['audio_producer']['legacy'][8]['window']['mix'][1]=q[2]
        self.rejects(edit,'producer quantile bits')
    def test_later_legacy_mix_must_equal_native_bits(self):
        def edit(m,d):
            self.window(d,8,[1.0+0.25*j for j in range(20)]);d['audio_producer']['legacy'][8]['window']['mix'][0]=1.1
        self.rejects(edit,'producer legacy quantile bits')
    # --- coverage of W by the WHOLE journal
    def test_journal_starting_after_S_cannot_cover(self):
        def edit(m,d):
            a=d['audio_producer'];r=a['receipt'];k=8;f=a['fences'];cut=f[k]['first_block']-1
            a['blocks']=a['blocks'][cut:];renumber(a);a['fences']=f[k:];a['legacy']=a['legacy'][k:]
            for i,fe in enumerate(a['fences'],1):
                fe['seq']=i;fe['flags']=(1 if i==1 else 0)|(2 if fe['legacy_id']==0 else 0)|(4 if fe['n']==0 else 0)
                if fe['n']:fe['first_block']-=cut;fe['last_block']-=cut
                if fe['legacy_id']:fe['legacy_id']=i
            for i,l in enumerate(a['legacy'],1):l['seq']=i;l['fence_id']=i
            r.update(blocks_produced=len(a['blocks']),fences_produced=len(a['fences']),legacy_produced=len(a['legacy']),start_consumed_us=a['fences'][0]['begin_us'],attached_us=min(r['attached_us'],a['fences'][0]['begin_us']))
            r['legacy_emitted']=sum(l['emitted'] for l in a['legacy']);r['legacy_queue_dropped']=r['legacy_produced']-r['legacy_emitted']
        self.rejects(edit,'producer window coverage')
    def test_journal_sealed_before_T_cannot_cover(self):
        def edit(m,d):
            a=d['audio_producer'];r=a['receipt'];keep=[f for f in a['fences'] if f['end_us']<=40_000_000];last=keep[-1]
            a['fences']=keep;a['blocks']=a['blocks'][:last['last_block']];a['legacy']=a['legacy'][:len(keep)]
            # the kept last fence becomes the final partial
            last.update(legacy_id=0,flags=last['flags']|2);a['legacy']=a['legacy'][:len(keep)-1]
            a['legacy'][-1]['post_us']=min(a['legacy'][-1]['post_us'],last['end_us'])
            r.update(final_us=last['end_us'],final_partial=True,no_residual_interval=False,blocks_produced=len(a['blocks']),fences_produced=len(keep),legacy_produced=len(a['legacy']))
            r['legacy_emitted']=sum(l['emitted'] for l in a['legacy']);r['legacy_queue_dropped']=r['legacy_produced']-r['legacy_emitted']
            a['seal_request'].update(pre_us=last['end_us']-2,post_us=last['end_us']-1)
        self.rejects(edit,'producer window coverage')
    # --- block membership edges
    def test_block_completed_exactly_at_fence_begin_is_rejected(self):
        def edit(m,d):
            a=d['audio_producer'];fe=a['fences'][0];b=a['blocks'][fe['first_block']-1];b['end_us']=fe['begin_us'];b['begin_us']=fe['begin_us']-5
        self.rejects(edit,'producer block membership')
    def test_block_completed_exactly_at_fence_end_belongs_to_it(self):
        def edit(m,d):
            a=d['audio_producer'];fe=a['fences'][8];b=a['blocks'][fe['last_block']-1];b['end_us']=fe['end_us']
        self.assertTrue(self.run_edit(edit)['native_audio_data_complete'])
    def test_block_after_final_fence_rejected(self):
        def edit(m,d):
            a=d['audio_producer'];r=a['receipt'];b=dict(a['blocks'][-1]);b.update(seq=len(a['blocks'])+1,begin_us=r['final_us']+1,end_us=r['final_us']+2);a['blocks'].append(b);r['blocks_produced']+=1
        self.rejects(edit,'producer final endpoints/membership')
    def test_first_fence_must_begin_at_start_consumption(self):
        self.rejects(lambda m,d:d['audio_producer']['fences'][0].update(begin_us=d['audio_producer']['receipt']['start_consumed_us']+1),'producer final endpoints/membership')
    # --- lifecycle / ordering / policy / storage / bounded-wait
    def test_start_request_after_consumption(self):
        self.rejects(lambda m,d:d['audio_producer'].update(start_request_us=d['audio_producer']['receipt']['start_consumed_us']+1),'producer activation order')
    def test_worker_entry_before_spawn_request(self):
        def edit(m,d):
            l=d['audio_producer']['lifecycle'];e=l['worker_birth']['entry']['post_us'];l['worker_spawn_request'].update(pre_us=e+1,post_us=e+2)
        self.rejects(edit,'producer creation/entry/binding order')
    def test_policy_not_applied(self):self.rejects(lambda m,d:d['audio_producer']['lifecycle']['worker_birth'].update(applied=False),'producer policy rejected')
    def test_policy_exit_code(self):self.rejects(lambda m,d:d['audio_producer']['lifecycle'].update(worker_exit=2,producer_loop_closed=True),'producer exit failure')
    def test_loop_closed_without_exit(self):self.rejects(lambda m,d:d['audio_producer']['lifecycle'].update(producer_loop_closed=True),'producer loop/thread lifetime')
    def test_storage_receipt(self):
        def edit(m,d):d['audio_producer']['storage']['reserved_bytes']+=8;m['audio_producer_storage_bytes']+=8;m['allocated_storage_bytes']+=8
        self.rejects(edit,'producer storage receipt')
    def test_wait_limit(self):self.rejects(lambda m,d:d['audio_producer'].update(wait_limit_us=2_000_001),'producer seal request/wait')
    def test_repeat_seal(self):self.rejects(lambda m,d:d['audio_producer'].update(repeat_seal=True),'producer seal request/wait')
    def test_seal_after_final(self):self.rejects(lambda m,d:d['audio_producer']['seal_request'].update(pre_us=d['audio_producer']['receipt']['final_us']+1,post_us=d['audio_producer']['receipt']['final_us']+2),'producer seal chronology')
    def test_sealed_before_start_reason(self):self.rejects(lambda m,d:d['audio_producer']['receipt'].update(reason='SealedBeforeStart'),'producer seal before start/abort')
    def test_delivered_status_requires_terminal(self):self.rejects(lambda m,d:d['audio_producer'].update(terminal=2),'producer completion unavailable')
    def test_audio_and_worker_identity_must_differ(self):self.rejects(lambda m,d:d['audio_producer'].update(worker_id=d['audio_producer']['audio_id']),'producer identity')
    def test_bool_in_legacy_float(self):self.rejects(lambda m,d:d['audio_producer']['legacy'][3]['window'].update(gap_max_ms=True),'producer nonfinite legacy value')
    def test_legacy_rows_overlap(self):self.rejects(lambda m,d:d['audio_producer']['legacy'][3].update(post_us=d['audio_producer']['legacy'][4]['pre_us']+1),'producer legacy order')
    def test_legacy_emitted_sum(self):
        def edit(m,d):
            a=d['audio_producer'];a['legacy'][2]['emitted']=False
        self.rejects(edit,'producer legacy receipt')
    def test_counter_lifetime_association_must_match_raw_section(self):
        def edit(m,d):
            d['audio_producer']['counter_lifetimes']=[[1]*16,None]
        self.rejects(edit,'producer counter association')
    def test_phase1_only_capture_is_not_audio_data_complete(self):
        r=check.check_capture(self.root/'exact'/self.name,1,20_000_000,allow_test_clock=True)
        self.assertFalse(r['native_audio_data_complete']);self.assertIsNone(r['producer_observations'])
    # --- raw-zero gate presentation when the journal exists without raw counters (actual optional capture sections)
    def test_producer_without_raw_counters_reports_raw_gate_unknown(self):
        def edit(m,d):
            raw=d.pop('raw_counters');n=m.pop('raw_counter_storage_bytes');m['allocated_storage_bytes']-=n
            d['audio_producer']['counter_lifetimes']=[None,None];m['phase2_unimplemented']=check.gaps(True,False)
        r=self.run_edit(edit)
        self.assertTrue(r['native_audio_data_complete'])
        for f in ('PcmStarvedBlocks','MusicUnderrunFrames'):
            self.assertEqual(r['raw_counter_guards'][f],dict(guard='UNKNOWN',reason='raw counters not captured; raw-zero gate unavailable'))
        self.assertFalse(r['audio_qualified'])
    def test_zero_width_fence_is_rejected_even_outside_selected_window(self):
        def edit(m,d):
            a=d['audio_producer'];f=a['fences'][0]
            a['blocks'].pop(0);renumber(a);a['receipt']['blocks_produced']-=1
            f.update(end_us=f['begin_us'],n=0,first_block=0,last_block=0,flags=5,quantile_ms_bits=[0,0,0])
            a['fences'][1]['begin_us']=f['end_us']
            for later in a['fences'][1:]:later['first_block']-=1;later['last_block']-=1
            a['legacy'][0].update(pre_us=f['end_us']);a['legacy'][0]['window'].update(blocks=0,mix=[0.,0.,0.])
        self.rejects(edit,'producer fence adjacency')
    def test_legacy_bracket_must_start_at_its_fence_cut(self):
        self.rejects(lambda m,d:d['audio_producer']['legacy'][3].update(pre_us=d['audio_producer']['legacy'][3]['pre_us']+1),'producer legacy association/bracket')
    def test_unjoined_thread_return_is_unknown_and_join_proves_return(self):
        self.assertEqual(self.actual('audio-straddle')['producer_observations']['thread_returned'],'UNKNOWN')
        def edit(m,d):d['audio_producer']['lifecycle'].update(thread_join=1,worker_exit=1,producer_loop_closed=True)
        self.assertIs(self.run_edit(edit)['producer_observations']['thread_returned'],True)
    def test_actual_optional_sections_determine_missing_capture_stages(self):
        for name,producer,raw in [('exact',False,False),('raw-zero',False,True),('audio-no-raw',True,False),('audio-straddle',True,True)]:
            r=self.actual(name)
            self.assertEqual('audio-producer-fences' not in r['phase2_unimplemented'],producer)
            self.assertEqual('raw-counter-brackets' not in r['phase2_unimplemented'],raw)
            self.assertFalse(r['audio_qualified']);self.assertFalse(r['method_qualified']);self.assertFalse(r['campaign_qualified'])
        r=self.actual('audio-no-raw')
        for f in ('PcmStarvedBlocks','MusicUnderrunFrames'):
            self.assertEqual(r['raw_counter_guards'][f],dict(guard='UNKNOWN',reason='raw counters not captured; raw-zero gate unavailable'))
    def test_legacy_later_N_must_match_even_when_quantiles_unchanged(self):
        self.rejects(lambda m,d:d['audio_producer']['legacy'][8]['window'].update(blocks=2),'producer legacy block count')
    # --- GENUINE worker journal (Rust test genuine_worker_journal_is_written_for_the_strict_checker)
    def test_genuine_worker_journal_validates_structurally(self):
        audio=check.sibling('native_timing_audio_check')
        a=json.loads((self.root/'genuine-worker'/'owner.json').read_bytes());f=a['fences']
        m=dict(run_id=a['run_id'],clock_id=a['clock_id'],audio_producer_storage_bytes=a['storage']['reserved_bytes'])
        r=audio.validate(a,m,f[1]['begin_us']+1,f[-2]['end_us']-1,None)
        self.assertTrue(r['structurally_complete']);self.assertGreaterEqual(len(r['selected_fence_ids']),3)
        self.assertTrue(all(l['window']['blocks']==f[i]['n'] for i,l in enumerate(a['legacy']) if i>0))
        with self.assertRaises(check.Invalid) as e:audio.validate(a,m,f[0]['begin_us']-1,f[-1]['end_us'],None)
        self.assertEqual(str(e.exception),'producer window coverage')
if __name__=='__main__':unittest.main(verbosity=2)
