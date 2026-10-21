"""Pair adversaries derived from real Rust writer exports in fresh owned paths.

Synthetic edits/readback receipts are TEST evidence, never native attestation.
Run the feature suite with ALICE_TIMING_TEST_OUTPUT first. No device/network IO.
"""
import copy
from dataclasses import replace
import hashlib
import json
import math
import os
from pathlib import Path
import struct
import subprocess
import sys
import unittest

import native_timing_check as c
import native_timing_pair_check as p


def bits(value):
    return struct.unpack('<I', struct.pack('<f', value))[0]


def compact(value):
    return json.dumps(value, separators=(',', ':')).encode()


def change_run(value, run):
    if isinstance(value, dict):
        for key in value:
            if key == 'run_id' and value[key] is not None:
                value[key] = [run]*16
            else:
                change_run(value[key], run)
    elif isinstance(value, list):
        for item in value:
            change_run(item, run)


def artifact(root, name, value):
    path = root/name
    path.parent.mkdir(parents=True, exist_ok=True)
    raw = value if type(value) is bytes else compact(value)
    path.write_bytes(raw)
    return dict(path=str(path), sha256=p.sha(raw))


def write_capture(root, name, m, d):
    path = root/name/(bytes(m['run_id']).hex()+'-0000000000000001')
    path.mkdir(parents=True, exist_ok=False)
    payload = compact(d)
    m.update(records_bytes=len(payload), records_sha256=p.sha(payload), capture_id=path.name)
    (path/'records.json').write_bytes(payload)
    (path/'manifest.json').write_bytes(compact(m))
    return path


class PairTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.fixture_root = Path(os.environ['ALICE_TIMING_TEST_OUTPUT'])
        cls.root = cls.fixture_root/os.environ.get('PAIR_TEST_CASES', 'pair-cases')
        cls.root.mkdir(exist_ok=False)
        name = '07'*16+'-0000000000000001'
        original = cls.fixture_root/'audio-exact'/name
        cls.m = json.loads((original/'manifest.json').read_bytes())
        cls.d = json.loads((original/'records.json').read_bytes())
        # Actual recorder populations plus the actual scheduler receipt shape.
        # Synthetic shortened schedule combines them for the external join tests.
        scheduled = cls.fixture_root/'scheduled'/name
        sm = json.loads((scheduled/'manifest.json').read_bytes())
        sd = json.loads((scheduled/'records.json').read_bytes())
        cls.d['frames'] = [f for f in cls.d['frames'] if 8_000_000 <= f['begin_us'] < 52_000_000]
        cls.d['events'] = [e for e in cls.d['events'] if e['native_us'] < 8_000_000]
        m, d = cls.m, cls.d
        # The writer's audio fixture deliberately repeats a static before state
        # and tick=1 after. Give the pair TEST copy cumulative, continuous ticks.
        for frame in d['frames']:
            frame['before']['physics_ticks'] = frame['begin_us']//10_000
            frame['after']['physics_ticks'] = frame['end_us']//10_000
        m.update(schedule_storage_bytes=sm['schedule_storage_bytes'], schedule_token_size=sm['schedule_token_size'])
        m['allocated_storage_bytes'] += m['schedule_storage_bytes']
        m.update(frame_count=len(d['frames']), completed=len(d['frames']),
                 event_count=len(d['events']), event_total=len(d['events']),
                 first_viewer_seq=d['frames'][0]['viewer_seq'], last_viewer_seq=d['frames'][-1]['viewer_seq'],
                 viewer_seq=d['frames'][-1]['viewer_seq']+1, capture_stop_viewer_seq=d['frames'][-1]['viewer_seq']+1,
                 capture_stop_us=52_000_000, phase2_unimplemented=c.gaps(True, True, True))
        d['schedule'] = copy.deepcopy(sd['schedule'])
        d['schedule'].update(ready_offset_us=20_000_000, start_us=20_000_000, end_us=50_000_000,
            review_deadline_us=10_000_000, acquired_begin_us=8_000_000, seal_request_us=52_000_000,
            start_state=copy.deepcopy(next(f['before'] for f in d['frames'] if f['begin_us'] == 20_000_000)),
            end_state=copy.deepcopy(next(f['before'] for f in d['frames'] if f['begin_us'] == 50_000_000)))
        shared = cls.root/'shared'
        shared.mkdir()
        shared_files = {key: artifact(shared, key+'.bin', ('TEST '+key).encode())
                        for key in ('source', 'build', 'recorder_config', 'save')}
        cls.spec = dict(schema=p.SPEC_SCHEMA, pair_id='gate0-test-pair', experiment_kind='AB', planned_order='AB',
            route=dict(map='tower2', entry=None, reason='Fresh', input_save_sha256=shared_files['save']['sha256'],
                policy=dict(route='test fixture', intro='ordinary', skip='none', input='neutral-standing')),
            window_us=30_000_000, limits=copy.deepcopy(p.LIMITS),
            treatment=dict(key='sim_hatter_door_candidates', A=False, B=True), arms={})
        cls.evidence = dict(schema=p.EVIDENCE_SCHEMA, spec_sha256='0'*64, freeze={}, arms={})
        cls.captures = {}
        for label, run in (('A', 7), ('B', 8)):
            mm, dd = copy.deepcopy(m), copy.deepcopy(d)
            change_run(mm, run); change_run(dd, run)
            config = artifact(shared, label+'-config.json', dict(sim_hatter_door_candidates=label == 'B', volume=1))
            files = dict(shared_files, game_config=config)
            mm['metadata']['recorder_config_sha256'] = list(bytes.fromhex(files['recorder_config']['sha256']))
            path = write_capture(cls.root, label, mm, dd)
            cls.captures[label] = path
            cls.spec['arms'][label] = dict(directory=str(path), generation=1, age_offset_us=20_000_000,
                expected={key+'_sha256': ref['sha256'] for key, ref in files.items()})
            start = 100_000_000 if label == 'A' else 300_000_000
            deploy = dict(schema='alice-pair-deployment-v1', pair_id=cls.spec['pair_id'], run_id=bytes([run]*16).hex(),
                host_clock_id='test-monotonic-session',
                manifest_sha256=p.sha((path/'manifest.json').read_bytes()), records_sha256=mm['records_sha256'],
                host_start_us=start, policy=cls.spec['route']['policy'],
                files={key: dict(upload=ref, readback=ref) for key, ref in files.items()})
            token = dd['schedule']['reviewed_token']
            request = artifact(shared, label+'-request.json', dict(run_id=[run]*16, token=token))
            review = dict(schema='alice-pair-review-v1', pair_id=cls.spec['pair_id'], run_id=deploy['run_id'],
                host_clock_id='test-monotonic-session',
                image=artifact(shared, label+'-image.png', b'TEST image bytes'),
                request=dict(upload=request, readback=request), token=token,
                request_id='separate-action', image_request_id='image-return', inspector='test reviewer',
                **{key: start+(i+1)*100_000 for i, key in enumerate((
                    'image_return_host_us', 'inspection_host_us', 'action_host_us', 'upload_host_us',
                    'readback_host_us', 'rename_host_us', 'ftp_closed_host_us'))})
            quiet = dict(schema='alice-pair-quiet-v1', run_id=deploy['run_id'], host_clock_id='test-monotonic-session',
                log_begin_host_us=start, log_end_host_us=start+60_000_000,
                ports=[21,5000,5001], requests=[])
            for edge, host, native in (('before', start+8_000_000, 8_000_000), ('after', start+52_000_000, 52_000_000)):
                observation = dict(schema='alice-pair-clock-observation-v1', run_id=deploy['run_id'],
                    native_us=native, host_pre_us=host, host_post_us=host+100, host_clock_id=quiet['host_clock_id'])
                quiet[edge] = dict(native_us=native, host_pre_us=host, host_post_us=host+100,
                    observation=artifact(shared, label+'-'+edge+'.json', observation))
            cls.evidence['arms'][label] = {key: artifact(shared, label+'-'+key+'.json', value)
                for key, value in (('deployment', deploy), ('review', review), ('quiet', quiet))}
        cls.spec_hash = p.sha(json.dumps(cls.spec, sort_keys=True, separators=(',', ':')).encode())
        cls.evidence['spec_sha256'] = cls.spec_hash
        cls.evidence['freeze'] = artifact(shared, 'freeze.json', dict(schema='alice-pair-freeze-v1',
            pair_id=cls.spec['pair_id'], spec_sha256=cls.spec_hash, frozen_host_us=1,host_clock_id='test-monotonic-session'))

    def setUp(self):
        self.specification = copy.deepcopy(self.spec)
        self.proof = copy.deepcopy(self.evidence)
        self.paths = dict(self.captures)
        self.case = self.root/self._testMethodName
        self.case.mkdir()

    def assess(self):
        # A real changed spec must be frozen again; tests refresh its independent
        # receipt so unrelated mismatches are not hidden by a spec-hash error.
        spec_hash = p.sha(json.dumps(self.specification, sort_keys=True, separators=(',', ':')).encode())
        self.proof['spec_sha256'] = spec_hash
        freeze = p.receipt(self.proof['freeze'])
        freeze['spec_sha256'] = spec_hash
        self.proof['freeze'] = artifact(self.case, 'freeze.json', freeze)
        return p.compare_pair(self.paths['A'], self.paths['B'], self.specification, self.proof, allow_test_clock=True)

    def edit_capture(self, edit, label='B'):
        path = self.paths[label]
        m = json.loads((path/'manifest.json').read_bytes())
        d = json.loads((path/'records.json').read_bytes())
        edit(m, d)
        serial = len(list(self.case.glob(label+'-capture-*')))
        path = write_capture(self.case, label+'-capture-'+str(serial), m, d)
        self.paths[label] = path
        self.specification['arms'][label]['directory'] = str(path)
        deploy = p.receipt(self.proof['arms'][label]['deployment'])
        deploy.update(manifest_sha256=p.sha((path/'manifest.json').read_bytes()), records_sha256=m['records_sha256'])
        self.proof['arms'][label]['deployment'] = artifact(self.case, label+'-deployment.json', deploy)

    def edit_receipt(self, key, edit, label='B'):
        row = p.receipt(self.proof['arms'][label][key]); edit(row)
        self.proof['arms'][label][key] = artifact(self.case, label+'-'+key+'.json', row)

    def rejected(self, code, status='FAIL', gate='pair_equivalence'):
        result = self.assess()
        self.assertEqual(result[gate], status, result['diagnostics'])
        self.assertIn(code, [d['code'] for d in result['diagnostics']], result['diagnostics'])
        self.assertFalse(result['all_requested_gates_pass'])
        return result

    def clocks(self, env=None, pickup=None, ticks=None):
        def edit(m, d):
            for f in d['frames']:
                for position in ('before', 'after'):
                    state = f[position]
                    if env is not None: state['environment_bits'] = env
                    if pickup is not None: state['pickup_bits'] = pickup
                    if ticks is not None: state['physics_ticks'] += ticks
        self.edit_capture(edit)

    def test_positive_pair_never_qualifies_method_or_campaign(self):
        r = self.assess()
        self.assertTrue(r['all_requested_gates_pass'], r['diagnostics'])
        self.assertEqual(r['pair_equivalence'], 'PASS')
        self.assertEqual(r['audio_gate'], 'PASS')
        self.assertFalse(r['method_qualified']); self.assertFalse(r['campaign_qualified'])
        self.assertFalse(r['candidate_benefit_established'])
        self.assertIn('UNATTESTED', r['provenance'])
        self.assertEqual(r['captures']['A']['metrics']['N'], 3000)

    @staticmethod
    def shift_frames(m, d):
        for f in d['frames']:
            for key in ('begin_us','end_us','cpu_begin_us','cpu_end_us'):
                if f[key] is not None:f[key]+=1
        m['capture_stop_us']+=1
        d['schedule']['acquired_begin_us']+=1;d['schedule']['seal_request_us']+=1

    def test_positive_straddling_pair_uses_worst_case_brackets(self):
        self.edit_capture(self.shift_frames,'A');self.edit_capture(self.shift_frames,'B')
        r=self.assess();self.assertTrue(r['all_requested_gates_pass'],r['diagnostics'])
        self.assertEqual(r['boundaries']['A']['S']['native_width_us'],10000)
        self.assertEqual(r['captures']['A']['metrics']['N'],3001)
        self.assertEqual(r['captures']['A']['metrics']['sum_full_frame_us'],30_010_000)

    def test_prefrozen_parent_directories_resolve_one_capture(self):
        for label in ('A','B'):
            self.paths[label]=self.paths[label].parent
            self.specification['arms'][label]['directory']=str(self.paths[label])
        r=self.assess();self.assertTrue(r['all_requested_gates_pass'],r['diagnostics'])

    def test_parent_with_multiple_captures_rejects(self):
        parent=self.case/'multiple';parent.mkdir()
        (parent/'one').mkdir();(parent/'two').mkdir()
        self.paths['B']=parent;self.specification['arms']['B']['directory']=str(parent)
        r=self.rejected('CAPTURE_INVALID')
        self.assertIn('exactly one original export',str(r['diagnostics']))

    def test_wide_clock_brackets_fail_even_when_identical(self):
        def edit(m,d):
            self.shift_frames(m,d)
            for f in d['frames']:
                for pos,age in (('before',f['begin_us']),('after',f['end_us'])):
                    f[pos]['environment_bits']=bits(5 if age<20_000_000 else 5.1)
        self.edit_capture(edit,'A');self.edit_capture(edit,'B')
        r=self.rejected('PAIR_SIM_AGE_TOLERANCE')
        self.assertGreater(r['boundaries']['A']['S']['widths']['environment_bits'],.05)

    def test_unsound_bracket_pair_unknown(self):
        def edit(m,d):
            self.shift_frames(m,d)
            f=next(f for f in d['frames'] if f['begin_us']<20_000_000<f['end_us'])
            f['after']['physics_ticks']-=1
        self.edit_capture(edit)
        self.rejected('PAIR_STATE_BRACKET_UNKNOWN','UNKNOWN')

    def test_last_f32_within_clock_limit_passes(self):
        self.clocks(env=bits(3.55))
        r=self.assess();self.assertEqual(r['pair_equivalence'],'PASS',r['diagnostics'])

    def test_one_f32_beyond_clock_limit_fails(self):
        self.clocks(env=bits(3.55)+1);self.rejected('PAIR_SIM_AGE_TOLERANCE')

    def test_S_to_T_advance_is_checked_independently(self):
        def reference(m,d):
            for f in d['frames']:
                for pos in ('before','after'):f[pos]['environment_bits']=bits(10)
        def candidate(m,d):
            for f in d['frames']:
                for pos,age in (('before',f['begin_us']),('after',f['end_us'])):
                    f[pos]['environment_bits']=bits(9.96 if age<35_000_000 else 10.04)
        self.edit_capture(reference,'A');self.edit_capture(candidate,'B')
        r=self.rejected('PAIR_SIM_AGE_TOLERANCE')
        failures=[d['field'] for d in r['diagnostics'] if d['code']=='PAIR_SIM_AGE_TOLERANCE']
        self.assertEqual(failures,['S_to_T.environment_bits.worst_case_difference'])

    def test_swapped_planned_order_preserves_measurements(self):
        self.specification['planned_order'] = 'BA'
        self.edit_receipt('deployment', lambda d: d.update(host_start_us=50_000_000))
        r = self.assess()
        self.assertTrue(r['all_requested_gates_pass'], r['diagnostics'])
        self.assertEqual(r['performance']['median_us_ratio'], 1)

    def test_self_pair_rejects_reused_capture(self):
        self.paths['B'] = self.paths['A']
        self.specification['arms']['B']['directory'] = str(self.paths['A'])
        self.rejected('PAIR_CAPTURE_REUSED')

    def test_map_mismatch(self):
        def edit(m,d):d['worlds'][0]['map']='other';d['schedule']['expected_map']='other'
        self.edit_capture(edit);self.rejected('PAIR_MAP_ENTRY_MISMATCH')

    def test_entry_null_is_distinct_from_string(self):
        self.edit_capture(lambda m,d:d['worlds'][0].update(entry='first'))
        self.rejected('PAIR_MAP_ENTRY_MISMATCH')

    def test_install_reason_mismatch(self):
        self.edit_capture(lambda m,d:d['worlds'][0].update(reason='Load'))
        self.rejected('PAIR_MAP_ENTRY_MISMATCH')

    def test_save_mismatch(self):
        self.edit_capture(lambda m,d:d['worlds'][0].update(save_sha256=[0]*32))
        self.rejected('PAIR_SAVE_ASSOCIATION_UNKNOWN')

    def test_both_null_save_associations_cannot_pass(self):
        self.specification['route']['input_save_sha256']=None
        self.rejected('PAIR_SAVE_ASSOCIATION_UNKNOWN', 'UNKNOWN')

    def test_missing_evidence_is_unknown(self):
        self.proof = {}
        # direct call deliberately omits all independent receipts
        r=p.compare_pair(self.paths['A'],self.paths['B'],self.specification,{},allow_test_clock=True)
        self.assertEqual(r['external_evidence_gate'],'UNKNOWN')
        self.assertFalse(r['all_requested_gates_pass'])
        self.assertIn('PAIR_EXTERNAL_EVIDENCE_UNKNOWN',[d['code'] for d in r['diagnostics']])

    def test_single_approved_boolean_is_not_evidence(self):
        r=p.compare_pair(self.paths['A'],self.paths['B'],self.specification,{'approved':True},allow_test_clock=True)
        self.assertEqual(r['external_evidence_gate'],'UNKNOWN');self.assertFalse(r['method_qualified'])

    def test_build_instrument_mismatch(self):
        self.specification['arms']['B']['expected']['build_sha256']='0'*64
        self.rejected('PAIR_INSTRUMENT_MISMATCH')

    def test_native_instrument_mismatch(self):
        self.edit_capture(lambda m,d:m['metadata'].update(source_sha256=[0]*32))
        self.rejected('PAIR_INSTRUMENT_MISMATCH')

    def test_native_config_mismatch(self):
        self.edit_capture(lambda m,d:m['metadata'].update(game_config_sha256=[0]*32))
        self.rejected('PAIR_TREATMENT_MISMATCH')

    def test_effective_policy_mismatch(self):
        self.edit_receipt('deployment',lambda d:d['policy'].update(skip='different'))
        self.rejected('PAIR_EXTERNAL_EVIDENCE_UNKNOWN',gate='external_evidence_gate')

    def config(self, value):
        ref=artifact(self.case,'changed-config.json',value)
        self.specification['arms']['B']['expected']['game_config_sha256']=ref['sha256']
        self.edit_receipt('deployment',lambda d:d['files'].update(game_config=dict(upload=ref,readback=ref)))

    def test_additional_config_difference(self):
        self.config(dict(sim_hatter_door_candidates=True,volume=2))
        self.rejected('PAIR_TREATMENT_MISMATCH')

    def test_config_bool_is_not_integer(self):
        self.config(dict(sim_hatter_door_candidates=1,volume=1))
        self.rejected('PAIR_TREATMENT_MISMATCH')

    def test_AB_requires_treatment_difference(self):
        self.config(dict(sim_hatter_door_candidates=False,volume=1))
        self.rejected('PAIR_TREATMENT_MISMATCH')

    def test_AA_same_config_is_repeatability_only(self):
        self.specification.update(experiment_kind='AA')
        self.specification['treatment']['B']=False
        self.config(dict(sim_hatter_door_candidates=False,volume=1))
        r=self.assess();self.assertTrue(r['all_requested_gates_pass'],r['diagnostics'])
        self.assertFalse(r['candidate_benefit_established'])

    def test_AA_rejects_treatment_difference(self):
        self.specification.update(experiment_kind='AA')
        self.rejected('PAIR_TREATMENT_MISMATCH')

    def test_offset_mismatch(self):
        self.specification['arms']['B']['age_offset_us']=21_000_000
        def edit(m,d):
            d.pop('schedule');n=m.pop('schedule_storage_bytes');m.pop('schedule_token_size')
            m['allocated_storage_bytes']-=n;m['phase2_unimplemented']=c.gaps(True,True)
            for i,f in enumerate(d['frames'],1):f['viewer_seq']=i
            m.update(first_viewer_seq=1,last_viewer_seq=len(d['frames']),viewer_seq=len(d['frames'])+1,
                     capture_stop_viewer_seq=len(d['frames'])+1)
        self.edit_capture(edit)
        self.rejected('PAIR_OFFSET_MISMATCH')

    def test_exact_baseline_mismatch(self):
        self.edit_capture(lambda m,d:d['worlds'][0]['baseline'].update(environment_bits=bits(3.0)))
        self.rejected('PAIR_BASELINE_MISMATCH')

    def test_signed_zero_pose_bits_are_distinct(self):
        def reference(m,d):
            for f in d['frames']:
                for position in ('before','after'):f[position]['pose_bits'][4]=0
        def edit(m,d):
            for f in d['frames']:
                for position in ('before','after'):f[position]['pose_bits'][4]=0x80000000
        self.edit_capture(reference,'A');self.edit_capture(edit);self.rejected('PAIR_POSE_BITS_MISMATCH')

    def test_interior_pose_drift(self):
        self.edit_capture(lambda m,d:d['frames'][1500]['after']['pose_bits'].__setitem__(0,0))
        self.rejected('PAIR_POSE_BITS_MISMATCH')

    def test_pose_drift_in_settle(self):
        self.edit_capture(lambda m,d:d['frames'][500]['before']['pose_bits'].__setitem__(0,0))
        self.rejected('PAIR_POSE_BITS_MISMATCH')

    def test_exact_T_pose_is_checked(self):
        def edit(m,d):
            next(f for f in d['frames'] if f['begin_us']==50_000_000)['before']['pose_bits'][0]=0
        self.edit_capture(edit);self.rejected('PAIR_POSE_BITS_MISMATCH')

    def test_equal_wall_age_mismatched_environment_age(self):
        self.clocks(env=bits(3.75));self.rejected('PAIR_SIM_AGE_TOLERANCE')

    def test_pickup_age_independently_checked(self):
        self.clocks(pickup=bits(7.5));self.rejected('PAIR_SIM_AGE_TOLERANCE')

    def test_six_physics_ticks_pass(self):
        self.clocks(ticks=6);r=self.assess();self.assertEqual(r['pair_equivalence'],'PASS',r['diagnostics'])

    def test_seven_physics_ticks_fail(self):
        self.clocks(ticks=7);self.rejected('PAIR_PHYSICS_TICK_TOLERANCE')

    def test_clock_regression(self):
        self.edit_capture(lambda m,d:d['frames'][2000]['before'].update(environment_bits=bits(3.0)))
        self.rejected('PAIR_STATE_REGRESSION')

    def test_between_frame_tick_regression(self):
        def edit(m,d):
            for position in ('before','after'):d['frames'][2000][position]['physics_ticks']=0
        self.edit_capture(edit);self.rejected('PAIR_STATE_REGRESSION')

    def test_nan_state_is_capture_invalid(self):
        self.edit_capture(lambda m,d:d['frames'][2000]['before'].update(environment_bits=0x7fc00000))
        r=self.rejected('CAPTURE_INVALID')
        self.assertIn('nonfinite simulation/state observation',str(r['diagnostics']))

    def test_infinite_state_is_capture_invalid(self):
        self.edit_capture(lambda m,d:d['frames'][2000]['before'].update(pickup_bits=0x7f800000))
        self.rejected('CAPTURE_INVALID')

    def test_sequence_loss_stays_structural(self):
        self.edit_capture(lambda m,d:d['frames'][2000].update(viewer_seq=9))
        self.rejected('CAPTURE_INVALID')

    def test_loss_stays_structural(self):
        self.edit_capture(lambda m,d:m.update(lost_frames=1));self.rejected('CAPTURE_INVALID')

    def test_reference_audio_failure_retains_metrics(self):
        self.edit_capture(lambda m,d:self.high_audio(d,19,8.01),label='A')
        r=self.rejected('AUDIO_PRODUCER_P95_GT_8MS',gate='audio_gate')
        self.assertEqual(r['captures']['A']['metrics']['producer']['p95_ms'][0],8.01)
        self.assertEqual(r['pair_equivalence'],'PASS')

    @staticmethod
    def high_audio(d,index,value):
        value_bits=struct.unpack('<Q',struct.pack('<d',value))[0]
        a=d['audio_producer'];a['blocks'][index]['total_ms_bits']=value_bits
        a['fences'][index]['quantile_ms_bits']=[value_bits]*3
        a['legacy'][index]['window']['mix']=[value]*3

    def test_candidate_audio_fail_cannot_be_excused(self):
        self.edit_capture(lambda m,d:self.high_audio(d,19,8.01))
        self.rejected('AUDIO_PRODUCER_P95_GT_8MS',gate='audio_gate')

    def audio_boundary(self,index):
        def shift(m,d):
            self.shift_frames(m,d)
            for key in ('ready_offset_us','start_us','end_us','review_deadline_us'):d['schedule'][key]+=1
        for label in ('A','B'):
            self.edit_capture(shift,label)
            self.specification['arms'][label]['age_offset_us']+=1
        self.edit_capture(lambda m,d:self.high_audio(d,index,8.01))
        r=self.rejected('AUDIO_PRODUCER_P95_GT_8MS',gate='audio_gate')
        self.assertIn(index+1,r['captures']['B']['metrics']['producer']['boundary_fence_ids'])
        self.assertIn(index+1,r['captures']['B']['metrics']['producer_gate']['failing_fence_ids'])

    def test_left_boundary_fence_kept(self):self.audio_boundary(19)

    def test_right_boundary_fence_kept(self):self.audio_boundary(49)

    def test_eight_ms_audio_passes(self):
        self.edit_capture(lambda m,d:self.high_audio(d,19,8.0))
        r=self.assess();self.assertEqual(r['audio_gate'],'PASS',r['diagnostics'])

    def test_audio_loss_remains_capture_invalid(self):
        self.edit_capture(lambda m,d:d['audio_producer']['receipt'].update(blocks_lost=1))
        self.rejected('CAPTURE_INVALID')

    def test_nonzero_raw_pcm_surrounding_delta(self):
        def edit(m,d):
            snaps=[r for r in d['raw_counters']['snapshots'] if r['field']=='PcmStarvedBlocks']
            snaps[-1]['value']+=1
        self.edit_capture(edit);self.rejected('RAW_COUNTER_NONZERO_OR_UNKNOWN',gate='audio_gate')

    def test_nonzero_raw_music_surrounding_delta(self):
        def edit(m,d):
            snaps=[r for r in d['raw_counters']['snapshots'] if r['field']=='MusicUnderrunFrames']
            snaps[-1]['value']+=1
        self.edit_capture(edit);self.rejected('RAW_COUNTER_NONZERO_OR_UNKNOWN',gate='audio_gate')

    def test_absent_raw_route_unknown(self):
        def edit(m,d):
            d['raw_counters']['music']=None
            d['raw_counters']['snapshots']=[r for r in d['raw_counters']['snapshots'] if r['field']!='MusicUnderrunFrames']
            for i,row in enumerate(d['raw_counters']['snapshots'],1):row['ordinal']=i
            d['raw_counters']['attempted']=len(d['raw_counters']['snapshots'])
            d['audio_producer']['counter_lifetimes'][1]=None
        self.edit_capture(edit);self.rejected('RAW_COUNTER_NONZERO_OR_UNKNOWN','UNKNOWN',gate='audio_gate')

    def test_busy_counter_rejects_structurally(self):
        self.edit_capture(lambda m,d:d['raw_counters']['snapshots'][0].update(busy=True))
        self.rejected('CAPTURE_INVALID')

    def test_counter_wrap_rejects_structurally(self):
        self.edit_capture(lambda m,d:d['raw_counters']['snapshots'][0].update(wrapped=True))
        self.rejected('CAPTURE_INVALID')

    def test_flush_subtraction_is_forbidden(self):
        self.edit_capture(lambda m,d:d['raw_counters'].update(flush_subset=0))
        self.rejected('CAPTURE_INVALID')

    def test_missing_review_chronology(self):
        self.edit_receipt('review',lambda d:d.pop('inspection_host_us'))
        self.rejected('PAIR_EXTERNAL_EVIDENCE_UNKNOWN','UNKNOWN',gate='external_evidence_gate')

    def test_action_before_image_return(self):
        self.edit_receipt('review',lambda d:d.update(action_host_us=d['image_return_host_us']-1))
        self.rejected('PAIR_EXTERNAL_EVIDENCE_UNKNOWN',gate='external_evidence_gate')

    def test_host_clock_mismatch_rejects_chronology(self):
        self.edit_receipt('review',lambda d:d.update(host_clock_id='different-host-session'))
        self.rejected('PAIR_EXTERNAL_EVIDENCE_UNKNOWN',gate='external_evidence_gate')

    def test_spec_freeze_after_start_rejects(self):
        freeze=p.receipt(self.proof['freeze']);freeze['frozen_host_us']=400_000_000
        self.proof['freeze']=artifact(self.case,'late-freeze.json',freeze)
        self.rejected('PAIR_EXTERNAL_EVIDENCE_UNKNOWN',gate='external_evidence_gate')

    def test_missing_save_readback_association_unknown(self):
        self.edit_receipt('deployment',lambda d:d['files'].update(save=None))
        self.rejected('PAIR_SAVE_ASSOCIATION_UNKNOWN','UNKNOWN')

    def test_same_request_is_not_separate_action(self):
        self.edit_receipt('review',lambda d:d.update(request_id=d['image_request_id']))
        self.rejected('PAIR_EXTERNAL_EVIDENCE_UNKNOWN',gate='external_evidence_gate')

    def test_request_upload_readback_gap(self):
        bad=artifact(self.case,'bad-request.json',{'bad':True})
        self.edit_receipt('review',lambda d:d['request'].update(readback=bad))
        self.rejected('PAIR_EXTERNAL_EVIDENCE_UNKNOWN',gate='external_evidence_gate')

    def test_review_request_bool_is_not_integer(self):
        row=p.receipt(self.proof['arms']['B']['review'])
        request=p.receipt(row['request']['readback']);request['token']['ordinal']=True
        ref=artifact(self.case,'bool-request.json',request)
        self.edit_receipt('review',lambda d:d.update(request=dict(upload=ref,readback=ref)))
        self.rejected('PAIR_EXTERNAL_EVIDENCE_UNKNOWN',gate='external_evidence_gate')

    def test_stale_deployment_run(self):
        self.edit_receipt('deployment',lambda d:d.update(run_id='09'*16))
        self.rejected('PAIR_EXTERNAL_EVIDENCE_UNKNOWN',gate='external_evidence_gate')

    def test_stale_deployment_manifest(self):
        self.edit_receipt('deployment',lambda d:d.update(manifest_sha256='0'*64))
        self.rejected('PAIR_EXTERNAL_EVIDENCE_UNKNOWN',gate='external_evidence_gate')

    def test_quiet_overlap(self):
        self.edit_receipt('quiet',lambda d:d['requests'].append(dict(port=5001,begin_host_us=320_000_000,end_host_us=320_000_001)))
        self.rejected('PAIR_EXTERNAL_EVIDENCE_UNKNOWN',gate='external_evidence_gate')

    def test_quiet_port_omission(self):
        self.edit_receipt('quiet',lambda d:d.update(ports=[21,5001]))
        self.rejected('PAIR_EXTERNAL_EVIDENCE_UNKNOWN',gate='external_evidence_gate')

    def test_forged_native_host_anchor(self):
        self.edit_receipt('quiet',lambda d:d['before'].update(native_us=10_000_000))
        self.rejected('PAIR_EXTERNAL_EVIDENCE_UNKNOWN',gate='external_evidence_gate')

    def test_receipt_bytes_hash_mismatch(self):
        self.proof['arms']['B']['review']['sha256']='0'*64
        self.rejected('PAIR_EXTERNAL_EVIDENCE_UNKNOWN',gate='external_evidence_gate')

    def test_spec_unknown_field_and_bool_integer_rejected(self):
        for edit in (lambda s:s.update(approved=True),lambda s:s['arms']['B'].update(generation=True),
                     lambda s:s['arms']['B'].update(directory='/tmp/\x00'),
                     lambda s:s['arms']['B'].update(directory='/tmp/\ud800')):
            s=copy.deepcopy(self.specification);edit(s)
            with self.assertRaises(c.Invalid):p.validate_spec(s)

    def test_spec_limits_cannot_be_widened(self):
        self.specification['limits']['sim_age_seconds']=0.051
        with self.assertRaises(c.Invalid):p.validate_spec(self.specification)

    def test_cli_json_on_rejection_and_success(self):
        spec=artifact(self.case,'spec.json',self.specification)
        # CLI hashes the original bytes rather than canonicalizing the spec.
        self.proof['spec_sha256']=spec['sha256']
        self.proof['freeze']=artifact(self.case,'freeze.json',dict(schema='alice-pair-freeze-v1',
            pair_id=self.specification['pair_id'],spec_sha256=spec['sha256'],frozen_host_us=1,host_clock_id='test-monotonic-session'))
        evidence=artifact(self.case,'evidence.json',self.proof)
        command=[sys.executable,'-B',p.__file__,'--spec',spec['path'],'--evidence',evidence['path'],'--allow-test-clock']
        good=subprocess.run(command,capture_output=True,text=True)
        self.assertEqual(good.returncode,0,good.stdout+good.stderr)
        self.assertTrue(json.loads(good.stdout)['all_requested_gates_pass'])
        Path(evidence['path']).write_bytes(b'{"approved":true,"approved":false}')
        bad=subprocess.run(command,capture_output=True,text=True)
        self.assertEqual(bad.returncode,2,bad.stderr)
        self.assertEqual(json.loads(bad.stdout)['diagnostics'][0]['message'],'duplicate JSON key')
        self.assertEqual(json.loads(bad.stdout)['spec_sha256'],spec['sha256'])


class ObservationAndMetricTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root=Path(os.environ['ALICE_TIMING_TEST_OUTPUT']);cls.name='07'*16+'-0000000000000001'

    def v(self,name):return c.validate_capture(self.root/name/self.name,1,20_000_000,allow_test_clock=True)

    def test_actual_whole_long_boundary_frame_kept(self):
        metrics=p.capture_metrics(self.v('long'))
        self.assertEqual((metrics['N'],metrics['max_us'],metrics['median_us'],metrics['p95_us']),
                         (1,60_000_000,60_000_000,60_000_000))
        self.assertEqual(metrics['above_100ms_count'],1)

    def test_actual_exact_and_straddling_boundary_brackets(self):
        exact=p.boundary_observations(self.v('exact'),'S')
        self.assertEqual(exact['native_width_us'],0)
        v=self.v('straddle')
        for i,f in enumerate(v.records['frames']):
            f['before']['physics_ticks']=i;f['after']['physics_ticks']=i+1
        bracket=p.boundary_observations(v,'S')
        self.assertEqual(bracket['sample_native_us'],[18_000_000,21_000_000])
        self.assertEqual(bracket['native_width_us'],3_000_000)

    def test_unsound_upper_endpoint_unknown(self):
        v=self.v('straddle');v.records['frames'][6]['after']['physics_ticks']+=1
        self.assertEqual(p.boundary_observations(v,'S')['status'],'UNKNOWN')

    def test_absent_boundary_unknown(self):
        v=self.v('straddle');v=replace(v,start_us=100_000_000)
        self.assertEqual(p.boundary_observations(v,'S')['status'],'UNKNOWN')

    def test_worst_case_not_center_or_interpolation(self):
        self.assertEqual(p.interval_difference([1,1.05],[1.04,1.09]),0.09000000000000008)
        self.assertGreater(p.interval_difference([0,0.1],[0,0.1]),0.050)

    def test_fifty_ms_exact_and_one_f32_above(self):
        # 0.05 is not exactly representable by f32. Two exactly representable
        # endpoints can approach it from either side; the predicate uses the
        # declared real-valued limit without rounding it to f32 or adding epsilon.
        self.assertLessEqual(p.interval_difference([0,0],[0,0.050]),p.LIMITS['sim_age_seconds'])
        self.assertGreater(p.interval_difference([0,0],[0,math.nextafter(.050,math.inf)]),p.LIMITS['sim_age_seconds'])
        below=bits(.05)-1;above=bits(.05)
        self.assertLess(c.f32(below),.05);self.assertGreater(c.f32(above),.05)

    def test_hand_calculated_odd_even_quantiles_and_slow_ties(self):
        v=self.v('exact')
        for values, expected in (([1,3,100000],(3,100000,0)),
                                 ([1,3,100000,100001],(50001.5,100001,1)),
                                 (list(range(1,21)),(10.5,19,0))):
            population=[dict(viewer_seq=i,begin_us=0,end_us=value) for i,value in enumerate(values)]
            r=p.capture_metrics(replace(v,population=population))
            self.assertEqual((r['median_us'],r['p95_us'],r['above_100ms_count']),expected)
            self.assertEqual(r['N'],len(values));self.assertTrue(r['small_sample'])

    def test_audio_structure_retains_fail_but_old_validate_raises(self):
        v=self.v('audio-p95-fail')
        self.assertEqual(p.capture_metrics(v)['producer_gate']['gate'],'FAIL')
        with self.assertRaisesRegex(c.Invalid,'producer intersecting window p95 exceeds 8ms'):
            c.check_capture(v.directory,1,20_000_000,allow_test_clock=True)

    def test_bounded_loader_ignores_brackets_in_strings(self):
        raw=json.dumps({'text':'['*10000+'"'+']'*10000}).encode()
        self.assertEqual(c.load(raw)['text'].count('['),10000)
        with self.assertRaisesRegex(c.Invalid,'malformed JSON'):c.load(b'['*65+b']'*65)

    def test_nested_config_types_are_exact(self):
        self.assertFalse(p.exact_json({'nested':[True]}, {'nested':[1]}))
        self.assertFalse(p.exact_json({'nested':1.0}, {'nested':1}))

    def test_performance_margins_exact_then_above(self):
        base=dict(N=100,median_us=10000,p95_us=10000,max_us=100000,
                  above_100ms_count=0,above_100ms_rate=0)
        candidate=dict(base,median_us=10100,p95_us=10200,max_us=105000)
        a=p.Assessment();p.assess_frame_performance(base,candidate,dict(limits=p.LIMITS),a)
        self.assertEqual(a.gate('performance'),'PASS')
        for key,value in (('median_us',10101),('p95_us',10201),('max_us',105001),
                          ('above_100ms_count',1)):
            with self.subTest(key=key):
                modified=dict(candidate,**{key:value})
                a=p.Assessment();p.assess_frame_performance(base,modified,dict(limits=p.LIMITS),a)
                self.assertEqual(a.gate('performance'),'FAIL')

    def test_slow_frame_count_and_rate_are_independent(self):
        base=dict(N=100,median_us=10000,p95_us=10000,max_us=110000,
                  above_100ms_count=2,above_100ms_rate=.02)
        for candidate in (dict(base,N=50,above_100ms_count=2,above_100ms_rate=.04),
                          dict(base,N=200,above_100ms_count=3,above_100ms_rate=.015)):
            a=p.Assessment();p.assess_frame_performance(base,candidate,dict(limits=p.LIMITS),a)
            self.assertEqual(a.gate('performance'),'FAIL')


if __name__=='__main__':unittest.main(verbosity=2)
