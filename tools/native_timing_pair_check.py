#!/usr/bin/env python3
"""Assess one frozen standing-pose AA/AB pair. Offline only; never qualification.

All frame/fence populations are whole, ordered, and independently validated.
External receipts establish consistency, not authenticity of the evidence author.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sys

import native_timing_check as c

SCHEMA = 'alice-native-pair-v1'
SPEC_SCHEMA = 'alice-native-pair-spec-v1'
EVIDENCE_SCHEMA = 'alice-native-pair-evidence-v1'
CLOCKS = ('environment_bits', 'pickup_bits', 'physics_ticks')
ARTIFACTS = ('source', 'build', 'game_config', 'recorder_config', 'save')
LIMITS = dict(sim_age_seconds=0.050, physics_ticks=6, median_ratio_upper=1.010,
              p95_ratio_upper=1.020, max_extra_us=5000, producer_p95_ms=8.0,
              slow_frame_us=100000)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def digest(value):
    c.need(type(value) is str and re.fullmatch('[0-9a-f]{64}', value) is not None,
           'invalid SHA-256')


def choice(*values):
    def verify(value):
        c.need(type(value) is str and value in values, 'unexpected choice')
    return verify


def positive(value):
    c.uint(value)
    c.need(value > 0, 'expected positive integer')


def absolute_path(value):
    c.text(value)
    c.need(Path(value).is_absolute(), 'expected absolute path')
    c.need('\x00' not in value, 'invalid path')
    try:
        value.encode('utf-8')
    except UnicodeError as error:
        raise c.Invalid('invalid UTF-8 path') from error


POLICY = dict(route=c.text, intro=c.text, skip=c.text, input=c.literal('neutral-standing'))
EXPECTED = {k + '_sha256': c.nullable(digest) for k in ARTIFACTS}
ARM = dict(directory=absolute_path, generation=positive, age_offset_us=c.uint,
           expected=lambda x: c.obj(x, EXPECTED))
REF = dict(path=absolute_path, sha256=digest)


def validate_spec(spec):
    c.obj(spec, dict(schema=c.literal(SPEC_SCHEMA), pair_id=c.text,
        experiment_kind=choice('AA', 'AB'), planned_order=choice('AB', 'BA'),
        route=lambda x: c.obj(x, dict(map=c.text, entry=c.nullable(c.text), reason=c.text,
            input_save_sha256=c.nullable(digest), policy=lambda y: c.obj(y, POLICY))),
        window_us=c.literal(30_000_000),
        limits=lambda x: c.obj(x, {k: c.literal(v) for k, v in LIMITS.items()}),
        treatment=lambda x: c.obj(x, dict(key=c.literal('sim_hatter_door_candidates'),
            A=c.boolean, B=c.boolean)),
        arms=lambda x: c.obj(x, dict(A=lambda y: c.obj(y, ARM), B=lambda y: c.obj(y, ARM)))))
    for arm in spec['arms'].values():
        c.need(20_000_000 <= arm['age_offset_us'] <= 600_000_000, 'offset outside schedule contract')
    c.need(spec['route']['reason'] in ('Fresh', 'StartSave', 'Load', 'Retry', 'ChapterOrConsole', 'Natural'),
           'install reason')
    return spec


def bounded(path, maximum):
    with Path(path).open('rb') as handle:
        raw = handle.read(maximum + 1)
    c.need(len(raw) <= maximum, 'oversize evidence/spec file')
    return raw


def read_json(path, maximum=256*1024):
    raw = bounded(path, maximum)
    return c.load(raw), sha(raw)


def capture_metrics(validated):
    durations = sorted(f['end_us'] - f['begin_us'] for f in validated.population)
    n = len(durations)
    c.need(n > 0, 'empty frame population')
    median = (durations[(n-1)//2] + durations[n//2]) / 2
    count = sum(value > 100_000 for value in durations)
    return dict(N=n, small_sample=n < 120, median_us=median,
                p95_us=durations[(95*n + 99)//100 - 1], max_us=durations[-1],
                above_100ms_count=count, above_100ms_rate=count/n,
                sum_full_frame_us=sum(durations),
                selected_viewer_seq=[f['viewer_seq'] for f in validated.population],
                producer=validated.producer,
                producer_gate=c.sibling('native_timing_audio_check').assess_producer_gate(validated.producer),
                raw_counter_guards=validated.raw_guards)


def decoded(state):
    return dict(bits=state, environment_seconds=c.f32(state['environment_bits']),
                pickup_seconds=c.f32(state['pickup_bits']), physics_ticks=state['physics_ticks'])


def state_values(state):
    return [c.f32(state[k]) if k != 'physics_ticks' else state[k] for k in CLOCKS]


def exact_json(a, b):
    if type(a) is not type(b):
        return False
    if type(a) is dict:
        return a.keys() == b.keys() and all(exact_json(a[k], b[k]) for k in a)
    if type(a) is list:
        return len(a) == len(b) and all(exact_json(x, y) for x, y in zip(a, b))
    return a == b


def boundary_observations(validated, edge):
    """Observed envelope only. No interpolation or scheduler start_state retiming.

    before is sampled at viewer begin; after is sampled before the swap, whereas
    end_us is the NEXT begin. An upper endpoint is sound only when after equals
    the next frame's before. Require that continuity explicitly.
    """
    c.need(edge in ('S', 'T'), 'unknown boundary edge')
    point = validated.start_us if edge == 'S' else validated.end_us
    frames = validated.records['frames']
    for i, frame in enumerate(frames):
        if frame['begin_us'] == point:
            states, ages, seqs = [frame['before']], [point], [frame['viewer_seq']]
            break
        if frame['begin_us'] < point < frame['end_us']:
            if i+1 == len(frames) or frame['after'] is None or frame['after'] != frames[i+1]['before']:
                return dict(edge=edge, status='UNKNOWN', native_us=point,
                            reason='no continuous observed upper endpoint')
            states = [frame['before'], frames[i+1]['before']]
            ages = [frame['begin_us'], frame['end_us']]
            seqs = [frame['viewer_seq'], frames[i+1]['viewer_seq']]
            break
    else:
        return dict(edge=edge, status='UNKNOWN', native_us=point, reason='missing observed boundary')
    values = [state_values(state) for state in states]
    return dict(edge=edge, status='OBSERVED', native_us=point,
        ready_relative_us=[age-validated.world['ready_us'] for age in ages],
        sample_native_us=ages, viewer_seq=seqs, native_width_us=ages[-1]-ages[0],
        observations=[decoded(state) for state in states],
        ranges={key: [min(v[i] for v in values), max(v[i] for v in values)]
                for i, key in enumerate(CLOCKS)},
        widths={key: max(v[i] for v in values)-min(v[i] for v in values)
                for i, key in enumerate(CLOCKS)})


class Assessment:
    def __init__(self):
        self.diagnostics = []
        self.checks = []

    def test(self, gate, code, field, value, limit, ok, unknown=False):
        status = 'PASS' if ok else ('UNKNOWN' if unknown else 'FAIL')
        row = dict(gate=gate, code=code, field=field, value=value, limit=limit, status=status)
        self.checks.append(row)
        if not ok:
            self.diagnostics.append(dict(row, message=f'{code}: {field}={value!r}; limit={limit!r}'))
        return ok

    def gate(self, name):
        states = [row['status'] for row in self.checks if row['gate'] == name]
        return 'FAIL' if 'FAIL' in states else ('UNKNOWN' if not states or 'UNKNOWN' in states else 'PASS')


def interval_difference(a, b):
    return max(abs(a[0]-b[1]), abs(a[1]-b[0]))


def missing_evidence(error):
    return isinstance(error, OSError) or any(word in str(error) for word in (
        'missing', 'unavailable'))


def state_contract(a, b, spec, assessment):
    out = {}
    baseline_a, baseline_b = a.world['baseline'], b.world['baseline']
    assessment.test('pair', 'PAIR_BASELINE_MISMATCH', 'ready.baseline', [baseline_a, baseline_b],
                    'exact equality', baseline_a == baseline_b)
    poses = []
    for label, v in (('A', a), ('B', b)):
        previous = state_values(v.world['baseline'])
        rows = [f for f in v.records['frames'] if f['begin_generation'] == f['end_generation'] == v.generation
                and f['begin_us'] < v.end_us]
        regression = None
        for frame in rows:
            for position in ('before', 'after'):
                state = frame[position]
                if state is None:
                    continue
                values = state_values(state)
                if any(x < y for x, y in zip(values, previous)):
                    regression = dict(seq=frame['viewer_seq'], observation=position,
                                      previous=previous, current=values)
                    break
                previous = values
            if regression is not None:
                break
        assessment.test('pair', 'PAIR_STATE_REGRESSION', f'{label}.clocks/ticks', regression,
                        'monotonic from ready', regression is None)
        pose = v.continuous[0]['before']['pose_bits']
        drift = next((dict(seq=f['viewer_seq'], position=p, bits=f[p]['pose_bits'])
                      for f in v.continuous for p in ('before', 'after')
                      if f[p] is None or f[p]['pose_bits'] != pose), None)
        assessment.test('pair', 'PAIR_POSE_BITS_MISMATCH', f'{label}.settle/window.pose', drift,
                        pose, drift is None)
        poses.append(pose)
        out[label] = dict(ready=decoded(v.world['baseline']),
                          S=boundary_observations(v, 'S'), T=boundary_observations(v, 'T'))
        for edge in ('S', 'T'):
            observed = out[label][edge]
            if observed['status'] == 'OBSERVED':
                bits = [row['bits']['pose_bits'] for row in observed['observations']]
                assessment.test('pair', 'PAIR_POSE_BITS_MISMATCH', f'{label}.{edge}.pose_bits', bits,
                                pose, all(value == pose for value in bits))
    assessment.test('pair', 'PAIR_POSE_BITS_MISMATCH', 'A/B.settled_pose_bits', poses,
                    'exact equality', poses[0] == poses[1])
    for edge in ('S', 'T'):
        for label in ('A', 'B'):
            assessment.test('pair', 'PAIR_STATE_BRACKET_UNKNOWN', f'{label}.{edge}', out[label][edge],
                            'sound observed bracket', out[label][edge]['status'] == 'OBSERVED', unknown=True)
    if any(out[label][edge]['status'] != 'OBSERVED' for label in ('A', 'B') for edge in ('S', 'T')):
        return out
    for label, baseline in (('A', baseline_a), ('B', baseline_b)):
        origin = state_values(baseline)
        for edge in ('S', 'T'):
            point = out[label][edge]
            point['baseline_relative_ranges'] = {
                key: [value-origin[i] for value in point['ranges'][key]] for i, key in enumerate(CLOCKS)}
        out[label]['S_to_T_ranges'] = {
            key: [out[label]['T']['ranges'][key][0]-out[label]['S']['ranges'][key][1],
                  out[label]['T']['ranges'][key][1]-out[label]['S']['ranges'][key][0]] for key in CLOCKS}
    for edge in ('S', 'T', 'S_to_T'):
        for key in CLOCKS:
            ranges = [(out[label][edge]['baseline_relative_ranges'][key] if edge != 'S_to_T'
                       else out[label]['S_to_T_ranges'][key]) for label in ('A', 'B')]
            worst = interval_difference(*ranges)
            ticks = key == 'physics_ticks'
            limit = spec['limits']['physics_ticks' if ticks else 'sim_age_seconds']
            assessment.test('pair', 'PAIR_PHYSICS_TICK_TOLERANCE' if ticks else 'PAIR_SIM_AGE_TOLERANCE',
                            f'{edge}.{key}.worst_case_difference', dict(ranges=ranges, delta=worst),
                            limit, worst <= limit)
    return out


def validate_evidence(evidence):
    c.obj(evidence, dict(schema=c.literal(EVIDENCE_SCHEMA), spec_sha256=digest,
        freeze=lambda x: c.obj(x, REF),
        arms=lambda x: c.obj(x, {label: lambda y: c.obj(y, dict(
            deployment=lambda z: c.obj(z, REF), review=lambda z: c.obj(z, REF),
            quiet=lambda z: c.obj(z, REF))) for label in ('A', 'B')})))


def receipt(ref, maximum=256*1024):
    raw = bounded(ref['path'], maximum)
    c.need(sha(raw) == ref['sha256'], 'evidence receipt hash mismatch')
    return c.load(raw)


def file_hash(ref, maximum=256*1024*1024):
    # Streaming bound for original executable/readback bytes; never load a SELF.
    digest_value = hashlib.sha256()
    size = 0
    with Path(ref['path']).open('rb') as handle:
        while True:
            block = handle.read(64*1024)
            if not block:
                break
            size += len(block)
            c.need(size <= maximum, 'oversize artifact file')
            digest_value.update(block)
    c.need(digest_value.hexdigest() == ref['sha256'], 'artifact/readback hash mismatch')
    return digest_value.hexdigest()


def export_directory(path):
    """A preregistered arm directory may contain exactly one native export.

    Random native run IDs need not be known when freezing the spec. No recursive
    search, alternate-window selection, or choice between multiple captures.
    """
    directory = Path(path)
    if (directory/'manifest.json').is_file():
        return directory
    with os.scandir(directory) as entries:
        children = []
        for entry in entries:
            c.need(len(children) < 2, 'arm directory must contain exactly one original export')
            children.append(Path(entry.path))
    c.need(len(children) == 1 and children[0].is_dir(), 'arm directory must contain exactly one original export')
    return children[0]


def external_contract(captures, spec, evidence, spec_hash, assessment):
    """Join bounded independent receipts. PASS means consistent external evidence.

    Host chronology is never a native timestamp. Quiet coverage uses two
    explicit host/native correlation brackets from the independent observation
    log, and monotonic elapsed times only, separately for each process.
    """
    configs, deployments = {}, {}
    try:
        validate_evidence(evidence)
        c.need(evidence['spec_sha256'] == spec_hash, 'evidence spec hash mismatch')
        freeze = receipt(evidence['freeze'])
        c.obj(freeze, dict(schema=c.literal('alice-pair-freeze-v1'), pair_id=c.text,
                          spec_sha256=digest, frozen_host_us=c.uint, host_clock_id=c.text))
        c.need(freeze['pair_id'] == spec['pair_id'] and freeze['spec_sha256'] == spec_hash,
               'freeze identity mismatch')
    except (c.Invalid, OSError, OverflowError) as error:
        assessment.test('external', 'PAIR_EXTERNAL_EVIDENCE_UNKNOWN', 'evidence/freeze', str(error),
                        'complete joined independent receipts', False, unknown=missing_evidence(error))
        assessment.test('pair', 'PAIR_EXTERNAL_EVIDENCE_UNKNOWN', 'arm/config/save associations',
                        'not established', 'joined independent receipts', False, unknown=True)
        return
    for label, v in captures.items():
        refs = evidence['arms'][label]
        try:
            run = bytes(v.manifest['run_id']).hex()
            deploy = receipt(refs['deployment'])
            artifact = dict(upload=lambda x: c.obj(x, REF), readback=lambda x: c.obj(x, REF))
            c.obj(deploy, dict(schema=c.literal('alice-pair-deployment-v1'), pair_id=c.text,
                run_id=c.text, manifest_sha256=digest, records_sha256=digest,
                host_start_us=c.uint, host_clock_id=c.text, policy=lambda x: c.obj(x, POLICY),
                files=lambda x: c.obj(x, {k: c.nullable(lambda y: c.obj(y, artifact)) for k in ARTIFACTS})))
            c.need(deploy['pair_id'] == spec['pair_id'] and deploy['run_id'] == run and
                   deploy['manifest_sha256'] == v.manifest_sha256 and
                   deploy['records_sha256'] == v.manifest['records_sha256'], 'stale/mismatched deployment join')
            c.need(freeze['frozen_host_us'] < deploy['host_start_us'], 'spec not frozen before run')
            c.need(deploy['host_clock_id'] == freeze['host_clock_id'], 'deployment/freeze host clock mismatch')
            c.need(deploy['policy'] == spec['route']['policy'], 'effective policy mismatch')
            deployments[label] = deploy
            for key in ARTIFACTS:
                files = deploy['files'][key]
                expected = spec['arms'][label]['expected'][key+'_sha256']
                if key == 'save' and spec['route']['input_save_sha256'] is None and spec['route']['reason'] == 'ChapterOrConsole':
                    c.need(files is None and expected is None, 'chapter input must be explicitly absent')
                    continue
                c.need(files is not None and expected is not None, f'{key} upload/readback association missing')
                for ref in files.values():
                    c.need(file_hash(ref) == expected, f'{key} expected identity mismatch')
                if key == 'game_config':
                    configs[label] = receipt(files['readback'])
                    c.need(type(configs[label]) is dict, 'game config must be object')
                meta = v.manifest['metadata'].get(key+'_sha256' if key != 'save' else 'startup_save_sha256')
                c.need(meta is None or bytes(meta).hex() == expected, f'{key} native metadata mismatch')
            if spec['route']['input_save_sha256'] is not None:
                c.need(deploy['files']['save'] is not None and
                       deploy['files']['save']['readback']['sha256'] == spec['route']['input_save_sha256'],
                       'save readback association mismatch')
            review = receipt(refs['review'])
            c.obj(review, dict(schema=c.literal('alice-pair-review-v1'), pair_id=c.text, run_id=c.text,
                image=lambda x: c.obj(x, REF), request=lambda x: c.obj(x, artifact),
                token=lambda x: c.obj(x, c.sibling('native_timing_schedule_check').TOKEN),
                request_id=c.text, image_request_id=c.text, inspector=c.text,
                host_clock_id=c.text,
                image_return_host_us=c.uint, inspection_host_us=c.uint, action_host_us=c.uint,
                upload_host_us=c.uint, readback_host_us=c.uint, rename_host_us=c.uint,
                ftp_closed_host_us=c.uint))
            schedule = v.records.get('schedule')
            c.need(schedule is not None, 'native review schedule missing')
            c.need(review['host_clock_id'] == deploy['host_clock_id'], 'review host clock mismatch')
            c.need(review['pair_id'] == spec['pair_id'] and review['run_id'] == run and
                   review['token'] == schedule['reviewed_token'], 'review/native token join mismatch')
            file_hash(review['image'], 32*1024*1024)
            request_hashes = [file_hash(ref, 1024) for ref in review['request'].values()]
            c.need(request_hashes[0] == request_hashes[1], 'review upload/readback gap')
            request = receipt(review['request']['readback'], 1024)
            c.obj(request, dict(run_id=c.array(c.octet, 16), token=lambda x:
                c.obj(x, c.sibling('native_timing_schedule_check').TOKEN)))
            token = review['token']
            expected_request = dict(run_id=v.manifest['run_id'], token=token)
            c.need(request == expected_request, 'review request/native token mismatch')
            times = [deploy['host_start_us']] + [review[k] for k in (
                'image_return_host_us', 'inspection_host_us', 'action_host_us', 'upload_host_us',
                'readback_host_us', 'rename_host_us', 'ftp_closed_host_us')]
            c.need(all(x < y for x, y in zip(times, times[1:])) and
                   review['request_id'] != review['image_request_id'], 'image return/inspection/separate action chronology')
            quiet = receipt(refs['quiet'])
            anchor = dict(host_pre_us=c.uint, host_post_us=c.uint, native_us=c.uint,
                          observation=lambda x: c.obj(x, REF))
            c.obj(quiet, dict(schema=c.literal('alice-pair-quiet-v1'), run_id=c.text,
                host_clock_id=c.text, before=lambda x: c.obj(x, anchor), after=lambda x: c.obj(x, anchor),
                log_begin_host_us=c.uint, log_end_host_us=c.uint,
                ports=c.array(positive), requests=c.array(lambda x: c.obj(x, dict(
                    port=positive, begin_host_us=c.uint, end_host_us=c.uint)))))
            c.need(quiet['run_id'] == run and quiet['ports'] == [21, 5000, 5001], 'quiet run/port coverage')
            c.need(quiet['host_clock_id'] == deploy['host_clock_id'], 'quiet host clock mismatch')
            for name in ('before', 'after'):
                anchor_value = quiet[name]
                c.need(anchor_value['host_pre_us'] <= anchor_value['host_post_us'], 'quiet anchor order')
                observed = receipt(anchor_value['observation'])
                c.obj(observed, dict(schema=c.literal('alice-pair-clock-observation-v1'),
                    run_id=c.text, native_us=c.uint, host_pre_us=c.uint, host_post_us=c.uint,
                    host_clock_id=c.text))
                c.need(observed == dict(schema='alice-pair-clock-observation-v1', run_id=run,
                    host_clock_id=quiet['host_clock_id'], **{k: anchor_value[k] for k in (
                        'native_us', 'host_pre_us', 'host_post_us')}), 'quiet anchor evidence mismatch')
            before, after = quiet['before'], quiet['after']
            c.need(schedule['review_ack_us'] <= before['native_us'] <= v.start_us-10_000_000 and after['native_us'] >= v.end_us and
                   before['host_post_us'] < after['host_pre_us'], 'quiet native window containment')
            # No cross-launch/UTC clock joining. Cover the entire conservative
            # host superset between the outer ends of these observation brackets.
            left, right = before['host_pre_us'], after['host_post_us']
            c.need(review['ftp_closed_host_us'] <= left and quiet['log_begin_host_us'] <= left and
                   quiet['log_end_host_us'] >= right, 'quiet log/FTP closure coverage')
            for request in quiet['requests']:
                c.need(request['begin_host_us'] <= request['end_host_us'] and request['port'] in quiet['ports'] and
                       quiet['log_begin_host_us'] <= request['begin_host_us'] <= request['end_host_us'] <= quiet['log_end_host_us'],
                       'quiet request log bounds')
                c.need(request['end_host_us'] < left or request['begin_host_us'] > right,
                       'quiet-port request overlaps settle/window superset')
            assessment.test('external', 'PAIR_EXTERNAL_EVIDENCE_UNKNOWN', label+'.receipts', 'joined',
                            'complete evidence consistency', True)
        except (c.Invalid, OSError, OverflowError) as error:
            if 'save' in str(error):
                assessment.test('pair', 'PAIR_SAVE_ASSOCIATION_UNKNOWN', label+'.save.readback', str(error),
                                spec['route']['input_save_sha256'], False, unknown='missing' in str(error))
            assessment.test('external', 'PAIR_EXTERNAL_EVIDENCE_UNKNOWN', label+'.receipts', str(error),
                            'complete joined independent receipts', False,
                            unknown=missing_evidence(error))
    if len(deployments) == 2:
        times = [deployments[label]['host_start_us'] for label in spec['planned_order']]
        assessment.test('external', 'PAIR_EXTERNAL_EVIDENCE_UNKNOWN', 'planned_order', times,
                        spec['planned_order'], times[0] < times[1])
    if len(configs) == 2:
        aa, bb = configs['A'], configs['B']
        key = spec['treatment']['key']
        differences = sorted(k for k in aa.keys() | bb.keys()
                             if k not in aa or k not in bb or not exact_json(aa[k], bb[k]))
        treatment_ok = (type(aa.get(key)) is bool and type(bb.get(key)) is bool and
                        aa[key] == spec['treatment']['A'] and bb[key] == spec['treatment']['B'])
        expected_difference = [] if spec['experiment_kind'] == 'AA' else [key]
        assessment.test('pair', 'PAIR_TREATMENT_MISMATCH', 'parsed_config.differences', differences,
                        expected_difference, treatment_ok and differences == expected_difference)
    else:
        assessment.test('pair', 'PAIR_TREATMENT_MISMATCH', 'parsed_config', sorted(configs),
                        ['A', 'B'], False, unknown=True)


def assess_frame_performance(ma, mb, spec, a):
    """Per-pair margins only; repeated-pair confidence bounds remain unresolved."""
    result = {}
    for field, limit in (('median_us', spec['limits']['median_ratio_upper']),
                         ('p95_us', spec['limits']['p95_ratio_upper'])):
        ratio = mb[field]/ma[field]
        result[field+'_ratio'] = ratio
        a.test('performance', 'PAIR_FRAME_RATIO_MARGIN', field+'.B/A', ratio, limit, ratio <= limit)
    for field, extra in (('max_us', spec['limits']['max_extra_us']), ('above_100ms_count', 0)):
        a.test('performance', 'PAIR_FRAME_EXTREME_MARGIN', field, mb[field], ma[field]+extra,
               mb[field] <= ma[field]+extra)
    a.test('performance', 'PAIR_FRAME_EXTREME_MARGIN', 'above_100ms_rate', mb['above_100ms_rate'],
           ma['above_100ms_rate'], mb['above_100ms_count']*ma['N'] <= ma['above_100ms_count']*mb['N'])
    return result


def compare_pair(reference, candidate, spec, evidence, *, allow_test_clock=False, spec_sha256=None):
    """Validate ORIGINAL directories separately, then assess the frozen pair."""
    validate_spec(spec)
    spec_hash = spec_sha256 or sha(json.dumps(spec, sort_keys=True, separators=(',', ':')).encode())
    a = Assessment()
    result = dict(schema=SCHEMA, spec_sha256=spec_hash, pair_id=spec['pair_id'],
                  experiment_kind=spec['experiment_kind'], planned_order=spec['planned_order'],
                  method_qualified=False, campaign_qualified=False, candidate_benefit_established=False,
                  provenance='UNATTESTED; receipt consistency is not independent authenticity',
                  unresolved_qualifications=['unobserved dynamic state UNKNOWN', 'physical exposure time UNKNOWN',
                    'route activity/listening', 'native RAM/stack and heap acceptance',
                    'method repeatability and paired variance', '13-map campaign and functional coverage',
                    'external receipt authenticity UNATTESTED', 'repeated-pair Student-t bounds unavailable from one pair'])
    captures = {}
    for label, directory in (('A', reference), ('B', candidate)):
        arm = spec['arms'][label]
        try:
            selected_directory = export_directory(directory)
            v = c.validate_capture(selected_directory, arm['generation'], arm['age_offset_us'],
                                   allow_test_clock=allow_test_clock)
            captures[label] = v
            a.test('pair', 'PAIR_CAPTURE_PATH_MISMATCH', label+'.directory', str(Path(directory).resolve()),
                   str(Path(arm['directory']).resolve()), Path(directory).resolve() == Path(arm['directory']).resolve())
        except (c.Invalid, OSError, OverflowError) as error:
            a.test('pair', 'CAPTURE_INVALID', label, str(error), 'valid original export', False)
    result['captures'] = {label: dict(run_id=bytes(v.manifest['run_id']).hex(),
        capture_id=v.manifest['capture_id'], metadata=v.manifest['metadata'], world=v.world,
        metrics=capture_metrics(v)) for label, v in captures.items()}
    if len(captures) == 2:
        av, bv = captures['A'], captures['B']
        a.test('pair', 'PAIR_CAPTURE_REUSED', 'run_id', [v.manifest['capture_id'] for v in (av, bv)],
               'distinct process runs', av.manifest['run_id'] != bv.manifest['run_id'])
        route = spec['route']
        for label, v in captures.items():
            for key in ('map', 'entry', 'reason'):
                a.test('pair', 'PAIR_MAP_ENTRY_MISMATCH', label+'.'+key, v.world[key],
                       route[key], v.world[key] == route[key])
            native_save = v.world['save_sha256']
            save_expected = route['input_save_sha256']
            a.test('pair', 'PAIR_SAVE_ASSOCIATION_UNKNOWN', label+'.native_save_sha256', native_save,
                   save_expected, native_save is None or bytes(native_save).hex() == save_expected)
            a.test('pair', 'PAIR_SAVE_ASSOCIATION_UNKNOWN', label+'.input_save_sha256', save_expected,
                   'known immutable save for save routes', save_expected is not None or route['reason'] == 'ChapterOrConsole',
                   unknown=True)
            for key in ARTIFACTS:
                meta_key = key+'_sha256' if key != 'save' else 'startup_save_sha256'
                native = v.manifest['metadata'].get(meta_key)
                expected = spec['arms'][label]['expected'][key+'_sha256']
                if native is not None:
                    a.test('pair', 'PAIR_INSTRUMENT_MISMATCH' if key in ('source', 'build', 'recorder_config')
                           else ('PAIR_TREATMENT_MISMATCH' if key == 'game_config' else 'PAIR_SAVE_ASSOCIATION_UNKNOWN'),
                           label+'.metadata.'+meta_key, bytes(native).hex(), expected,
                           bytes(native).hex() == expected)
        offsets = [v.start_us-v.world['ready_us'] for v in (av, bv)]
        a.test('pair', 'PAIR_OFFSET_MISMATCH', 'S-ready/T-ready', offsets,
               'equal offset and 30000000 us duration', offsets[0] == offsets[1] and
               all(v.end_us-v.start_us == spec['window_us'] for v in (av, bv)))
        for key in ('source', 'build', 'recorder_config'):
            expected = [spec['arms'][label]['expected'][key+'_sha256'] for label in ('A', 'B')]
            a.test('pair', 'PAIR_INSTRUMENT_MISMATCH', key, expected, 'same known identity',
                   expected[0] is not None and expected[0] == expected[1], unknown=expected[0] is None or expected[1] is None)
        treatment = spec['treatment']
        treatment_ok = ((treatment['A'] == treatment['B']) if spec['experiment_kind'] == 'AA'
                        else (treatment['A'] is False and treatment['B'] is True))
        a.test('pair', 'PAIR_TREATMENT_MISMATCH', 'experiment_kind/treatment', treatment,
               spec['experiment_kind'], treatment_ok)
        config_hashes = [spec['arms'][label]['expected']['game_config_sha256'] for label in ('A', 'B')]
        a.test('pair', 'PAIR_TREATMENT_MISMATCH', 'game_config_sha256', config_hashes,
               'identical AA bytes / distinct AB bytes', all(value is not None for value in config_hashes) and
               ((config_hashes[0] == config_hashes[1]) == (spec['experiment_kind'] == 'AA')),
               unknown=any(value is None for value in config_hashes))
        result['boundaries'] = state_contract(av, bv, spec, a)
        external_contract(captures, spec, evidence, spec_hash, a)
        ma, mb = (result['captures'][label]['metrics'] for label in ('A', 'B'))
        result['performance'] = assess_frame_performance(ma, mb, spec, a)
        for label in ('A', 'B'):
            metrics = result['captures'][label]['metrics']
            gate = metrics['producer_gate']
            a.test('audio', 'AUDIO_PRODUCER_P95_GT_8MS', label+'.producer', gate,
                   8.0, gate['gate'] == 'PASS', unknown=gate['gate'] == 'UNKNOWN')
            guards = metrics['raw_counter_guards'] or {}
            for key in ('PcmStarvedBlocks', 'MusicUnderrunFrames'):
                guard = guards.get(key, dict(guard='UNKNOWN'))
                a.test('audio', 'RAW_COUNTER_NONZERO_OR_UNKNOWN', label+'.'+key, guard,
                       'ZERO_ON_CONTAINING_SUPERSET delta=0', guard.get('guard') == 'ZERO_ON_CONTAINING_SUPERSET'
                       and guard.get('delta') == 0, unknown=guard.get('guard') == 'UNKNOWN')
    result.update(pair_equivalence=a.gate('pair'), performance_gate=a.gate('performance'),
                  audio_gate=a.gate('audio'), external_evidence_gate=a.gate('external'),
                  checks=a.checks, diagnostics=a.diagnostics)
    result['all_requested_gates_pass'] = all(result[key] == 'PASS' for key in (
        'pair_equivalence', 'performance_gate', 'audio_gate', 'external_evidence_gate'))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--spec', type=Path, required=True)
    parser.add_argument('--evidence', type=Path, required=True)
    parser.add_argument('--allow-test-clock', action='store_true')
    args = parser.parse_args()
    spec, spec_hash = None, None
    try:
        spec, spec_hash = read_json(args.spec)
        validate_spec(spec)
        evidence, _ = read_json(args.evidence)
        result = compare_pair(spec['arms']['A']['directory'], spec['arms']['B']['directory'],
                              spec, evidence, spec_sha256=spec_hash, allow_test_clock=args.allow_test_clock)
    except (c.Invalid, OSError, OverflowError) as error:
        result = dict(schema=SCHEMA, spec_sha256=spec_hash,
                      pair_id=spec.get('pair_id') if type(spec) is dict else None,
                      method_qualified=False, campaign_qualified=False,
                      pair_equivalence='UNKNOWN', performance_gate='UNKNOWN', audio_gate='UNKNOWN',
                      external_evidence_gate='UNKNOWN', all_requested_gates_pass=False,
                      diagnostics=[dict(code='PAIR_INPUT_INVALID', message=str(error))])
    print(json.dumps(result, indent=2, allow_nan=False))
    return 0 if result['all_requested_gates_pass'] else 2


if __name__ == '__main__':
    sys.exit(main())
