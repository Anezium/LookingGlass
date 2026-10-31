#!/usr/bin/env python3
"""Host ON/OFF oracle for one runtime flag on the diagnostic twin.

tools/skin_exact_r9/host_exact.py stays unchanged. This oracle toggles exactly one
named flag. Each arm writes dict(base, **prerequisites, <flag>=arm) plus
phase_census=true. The environment is rebuilt without any LOOKING_GLASS_* variable,
then only the probe variables below (and explicit --probe-env LOOKING_GLASS_* values)
are set. LOOKING_GLASS_TEST_* switches never reach this binary.

The diagnostic config record is one line:

    [lg/vita-renderer] config <json> effective_log_frame_time_every=<cadence>

`report_cadence` is not part of that record.

Elapsed fields are an explicit allowlist. A name ending in _us or _ns is still
compared unless it is on that list. Exclusion means "not part of deterministic
equality". It is not a finding that the flag cannot affect those times.

Census rows drop only the named elapsed arrays. Counts, rejections, actors and
query columns stay in the comparison. A witness that is zero on both arms is
FAIL (vacuous).
"""
from pathlib import Path
import argparse, hashlib, json, os, subprocess, sys

DATA = '/Users/anezium/vita-work/alice-data/Alice1/bin/base'
DT = {'pandemonium': '0.04184', 'hedge3': '0.06'}
# Reviewed elapsed counters from the production part A reparse. Not a suffix rule.
TIMING_COUNTERS = (
    'fx_worker_max_service_us',
    'fx_worker_wait_us',
    'skin_collect_wait_us',
    'skin_gather_us',
    'skin_worker_service_us',
    'vita_fx_upload_cpu_us',
    'vita_fx_upload_gl_us',
)
TIMING_FRAME_FIELDS = ('cpu_ms', 'spans_ms')
CENSUS_ELAPSED = (
    'frame_total_ns',
    'bracket_union_ns',
    'unbracketed_residual_ns',
    'inclusive_ns',
    'exclusive_ns',
    'static_traversal_ns',
    'sweep_ns',
    'acoustics_world_sweep_ns',
    'worker_skin_ns',
)
CONFIG_PREFIX = '[lg/vita-renderer] config '


def sha(data):
    return hashlib.sha256(data).hexdigest()


def canon(value):
    return sha(json.dumps(value, sort_keys=True, separators=(',', ':')).encode())


def parse_config_record(line):
    if not line.startswith(CONFIG_PREFIX) or 'report_cadence' in line:
        raise AssertionError(line)
    payload = line[len(CONFIG_PREFIX):]
    printed, end = json.JSONDecoder().raw_decode(payload)
    suffix = payload[end:].strip()
    key = 'effective_log_frame_time_every='
    if not suffix.startswith(key) or suffix[len(key):] == '':
        raise AssertionError(suffix)
    return printed, suffix[len(key):]


def semantic_counters(counters, witness_prefix):
    return {k: v for k, v in counters.items()
            if k not in TIMING_COUNTERS and not (witness_prefix and k.startswith(witness_prefix))}


def frame_body(frame):
    return {k: v for k, v in frame.items() if k not in TIMING_FRAME_FIELDS and k not in ('counters', 'submitted_packets')}


def packets(frame):
    return [{k: v for k, v in packet.items() if k != 'gpu_slot'} for packet in frame['submitted_packets']]


def strip_census(doc):
    rows = [{k: v for k, v in row.items() if k not in CENSUS_ELAPSED} for row in doc['rows']]
    return {k: (rows if k == 'rows' else v) for k, v in doc.items()}


def self_test():
    printed, cadence = parse_config_record(
        '[lg/vita-renderer] config {"tls_fast_path":false,"nested":{"a":1}} effective_log_frame_time_every=off')
    assert printed == {'tls_fast_path': False, 'nested': {'a': 1}} and cadence == 'off'
    try:
        parse_config_record('[lg/vita-renderer] config {} report_cadence effective_log_frame_time_every=off')
    except AssertionError:
        pass
    else:
        raise AssertionError('report_cadence must not parse as the diagnostic record')
    counters = semantic_counters({
        'skin_worker_service_us': 9,
        'mystery_us': 4,
        'trigger_fires': 2,
        'tls_fast_thread_lookups_skipped': 7,
    }, 'tls_fast_')
    assert counters == {'mystery_us': 4, 'trigger_fires': 2}, counters
    assert set(TIMING_COUNTERS) == {
        'fx_worker_max_service_us', 'fx_worker_wait_us', 'skin_collect_wait_us', 'skin_gather_us',
        'skin_worker_service_us', 'vita_fx_upload_cpu_us', 'vita_fx_upload_gl_us',
    }
    row = strip_census({'schema': 6, 'rows': [{'calls': [1], 'inclusive_ns': [9], 'actors': [{'serial_vertices': 3}]}]})
    assert row['rows'] == [{'calls': [1], 'actors': [{'serial_vertices': 3}]}]
    assert 'inclusive_ns' not in row['rows'][0]
    print('SELF-TEST PASS')


def main(argv):
    if argv == ['--self-test']:
        self_test()
        return 0
    p = argparse.ArgumentParser()
    p.add_argument('output', type=Path)
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--flag', required=True)
    p.add_argument('--maps', nargs='+', required=True)
    p.add_argument('--prerequisites', default='{}')
    p.add_argument('--witness', action='append', default=[])
    p.add_argument('--removed', action='append', default=[])
    p.add_argument('--counter-prefix', default='tls_fast_')
    p.add_argument('--probe-env', action='append', default=[])
    p.add_argument('--frames', type=int, default=180)
    p.add_argument('--label', default='host-flag-exact')
    p.add_argument('--control-binary', type=Path, default=None)
    args = p.parse_args(argv)
    root = Path(__file__).resolve().parents[2]
    out = args.output.resolve()
    base = json.loads((root / 'tools/lever_matrix_r4/configs/PL-baseline.json').read_text())
    prerequisites = json.loads(args.prerequisites)
    assert args.flag not in prerequisites and args.flag not in base, 'the flag must come only from the arm'
    assert args.witness, 'a workload witness is mandatory'
    extra = dict(kv.split('=', 1) for kv in args.probe_env)
    assert all(k.startswith('LOOKING_GLASS_') for k in extra), extra
    receipt_path = out / 'proof' / f'{args.label}.json'
    result = dict(
        label=args.label, binary=str(args.binary), binary_sha256=sha(args.binary.read_bytes()),
        flag=args.flag, prerequisites=prerequisites, probe_env=extra, witness=args.witness,
        removed=args.removed, counter_prefix=args.counter_prefix,
        timing_counters=list(TIMING_COUNTERS), timing_frame_fields=list(TIMING_FRAME_FIELDS),
        census_elapsed=list(CENSUS_ELAPSED), maps=[], status='RUNNING',
        config_record='single line with effective_log_frame_time_every; report_cadence absent',
        control_binary=str(args.control_binary) if args.control_binary else None,
        control_binary_sha256=sha(args.control_binary.read_bytes()) if args.control_binary else None)
    for name in args.maps:
        arms = []
        runs = [(False, args.binary), (True, args.binary)]
        if args.control_binary:
            runs.append((None, args.control_binary))
        for arm, binary in runs:
            tag = 'control' if arm is None else str(int(arm))
            directory = out / 'host' / args.label / f'{name}-{args.flag}-{tag}'
            (directory / 'private').mkdir(parents=True, exist_ok=False)
            flag = {} if arm is None else {args.flag: arm}
            config = dict(base, **prerequisites, **flag, phase_census=True)
            (directory / 'private/vita_renderer.json').write_text(json.dumps(config))
            env = {k: v for k, v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
            env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1', LOOKING_GLASS_SIM_MAPS_DT=DT.get(name, '0.05'),
                       LOOKING_GLASS_SKIN_PROOF='1', LOOKING_GLASS_SKIN_MOVING_VIEW='1',
                       LOOKING_GLASS_SKIN_DEATHS='1', **extra)
            cmd = [str(binary), '--data', DATA, '--no-audio', '--save-dir', str(directory / 'private/saves'),
                   '--map', name, '--frames', str(args.frames), '--capture', str(directory / 'final.png')]
            with (directory / 'run.log').open('w') as handle:
                proc = subprocess.run(cmd, cwd=directory, env=env, stdout=handle, stderr=subprocess.STDOUT, timeout=1800)
            assert proc.returncode == 0, (name, arm, proc.returncode, str(directory / 'run.log'))
            text = (directory / 'run.log').read_text()
            lines = text.splitlines()
            records = [parse_config_record(line) for line in lines if CONFIG_PREFIX in line]
            assert len(records) == 1, (name, arm, len(records))
            printed, cadence = records[0]
            if arm is None:
                assert args.flag not in printed, (name, 'control binary must predate the flag')
            else:
                assert printed[args.flag] is arm, (name, arm, 'printed flag')
            for key, value in prerequisites.items():
                assert printed[key] == value, (name, arm, 'prerequisite', key, printed[key], value)
            assert printed['phase_census'] is True, (name, arm)
            want_cadence = 'off' if not printed.get('log_frame_time_every') else str(printed['log_frame_time_every'])
            assert cadence == want_cadence, (name, arm, cadence, want_cadence)
            single = [line for line in lines if line.startswith('[lg/vita-renderer] actor_single_prepare active=')]
            assert single == ['[lg/vita-renderer] actor_single_prepare active=true'], (name, arm, single)
            fallback = [line for line in lines if '[lg/skin] serial fallback' in line or 'afterimage consumer must be declared' in line]
            assert not fallback, (name, arm, fallback[:3])
            frames = [json.loads(line.split(' ', 1)[1]) for line in lines if line.startswith('SIM_MAPS_FRAME ')]
            images = [json.loads(line.split(' ', 1)[1]) for line in lines if line.startswith('SKIN_EXACT_IMAGE ')]
            assert len(frames) == args.frames, (name, arm, len(frames))
            assert all(packet['geometry'] == packet['gpu_geometry'] for frame in frames for packet in frame['submitted_packets']), (name, arm, 'gpu readback')
            census_path = directory / f'{name}-census-{args.frames}.json'
            census = json.loads(census_path.read_text())
            assert census.get('schema') == 6, (name, arm, census.get('schema'))
            if 'serial_fallbacks' in census:
                assert census['serial_fallbacks'] == 0, (name, arm)
            measured = census['rows'][60:]
            counters = {}
            for frame in frames:
                for key, value in frame['counters'].items():
                    counters[key] = counters.get(key, 0) + value
            png = (directory / 'final.png').read_bytes()
            assert len(png) > 0, (name, arm, 'empty final image')
            arms.append(dict(
                arm=arm, frames=frames, images=images, counters=counters, census=strip_census(census),
                serial_vertices=sum(actor['serial_vertices'] for row in measured for actor in row['actors']),
                worker_vertices=sum(actor['worker_vertices'] for row in measured for actor in row['actors']),
                final_png_bytes=len(png), final_png_sha256=sha(png),
                run_log_sha256=sha((directory / 'run.log').read_bytes()),
                printed_flag=printed.get(args.flag),
                printed_config_sha256=canon(printed),
                printed_prerequisites={key: printed[key] for key in prerequisites},
                cadence=cadence,
                gpu_draw_readbacks=sum(len(frame['submitted_packets']) for frame in frames)))
            print('DONE', args.label, name, arm, len(frames), 'GPU draws', arms[-1]['gpu_draw_readbacks'], flush=True)
        off = arms[0]
        for other in arms[1:]:
            other_name = 'ON' if other is arms[1] else 'control'
            assert off['printed_config_sha256'] != other['printed_config_sha256'] or other['arm'] is None
            if other['arm'] is not None:
                assert off['cadence'] == other['cadence']
            for index, (left, right) in enumerate(zip(off['frames'], other['frames'])):
                assert frame_body(left) == frame_body(right), (name, other_name, index, 'frame fields')
                assert packets(left) == packets(right), (name, other_name, index, 'uploaded GPU packets')
                assert semantic_counters(left['counters'], args.counter_prefix) == semantic_counters(right['counters'], args.counter_prefix), (name, other_name, index, 'workload counters')
            assert off['images'] == other['images'], (name, other_name, 'intermediate pixels')
            assert off['final_png_sha256'] == other['final_png_sha256'], (name, other_name, 'final image')
            assert off['census'] == other['census'], (name, other_name, 'census counts')
            assert (off['serial_vertices'], off['worker_vertices']) == (other['serial_vertices'], other['worker_vertices']), (name, other_name, 'census vertices')
        witness = {key: [off['counters'].get(key, 0), arms[1]['counters'].get(key, 0)] for key in args.witness}
        removed = {key: [off['counters'].get(key, 0), arms[1]['counters'].get(key, 0)] for key in args.removed}
        for key, (left, right) in witness.items():
            assert left == 0 and right > 0, (name, 'witness', key, left, right)
        for key, (left, right) in removed.items():
            assert left > 0 and right == 0, (name, 'removed', key, left, right)
        on = arms[1]
        row = dict(
            map=name, dt=DT.get(name, '0.05'), frames=args.frames, witness_off_on=witness, removed_off_on=removed,
            prefixed_counters_off_on={key: [off['counters'].get(key, 0), on['counters'].get(key, 0)]
                                      for key in sorted(set(off['counters']) | set(on['counters']))
                                      if args.counter_prefix and key.startswith(args.counter_prefix)},
            frame_digest=[canon([frame_body(frame) for frame in item['frames']]) for item in arms],
            packet_digest=[canon([packets(frame) for frame in item['frames']]) for item in arms],
            census_digest=[canon(item['census']) for item in arms],
            image_digest=[canon(item['images']) for item in arms], image_witnesses=len(off['images']),
            timing_counters_not_compared='Explicit elapsed-time allowlist. These sums are recorded and excluded from deterministic equality. Exclusion is not a causal claim.',
            timing_counter_sums=[{key: item['counters'].get(key, 0) for key in TIMING_COUNTERS} for item in arms],
            final_png_bytes=[item['final_png_bytes'] for item in arms],
            final_png_sha256=[item['final_png_sha256'] for item in arms],
            run_log_sha256=[item['run_log_sha256'] for item in arms],
            gpu_draw_readbacks=[item['gpu_draw_readbacks'] for item in arms],
            census_serial_worker_vertices=[[item['serial_vertices'], item['worker_vertices']] for item in arms],
            printed=[dict(flag=item['printed_flag'], prerequisites=item['printed_prerequisites'], cadence=item['cadence']) for item in arms],
            control_matches_off=args.control_binary is not None, status='PASS')
        result['maps'].append(row)
        receipt_path.parent.mkdir(parents=True, exist_ok=True)
        receipt_path.write_text(json.dumps(result, indent=1) + '\n')
        print('PASS', args.label, name, 'witness', json.dumps(witness), 'removed', json.dumps(removed), flush=True)
    result['status'] = 'PASS'
    receipt_path.write_text(json.dumps(result, indent=1) + '\n')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
