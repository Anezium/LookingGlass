#!/usr/bin/env python3
"""Host ON/OFF oracle for one new runtime flag. Mac only. No device transport.

tools/skin_exact_r9/host_exact.py stays unchanged and still toggles skin_actual_consumers.
This oracle toggles exactly one flag. Each arm writes dict(base, **prerequisites, <flag>=arm)
into private/vita_renderer.json. The environment is rebuilt with no LOOKING_GLASS_* variable,
then only the probe variables below are set. LOOKING_GLASS_TEST_* never reaches the binary.

Checks, per map:
- the printed effective config carries the flag and every prerequisite;
- workload counters named by --witness are 0 in OFF; on each --require-positive map they are
  > 0 in ON. A run whose witnesses are 0 on every map is FAIL (vacuous). Other maps may have
  a zero ON witness when that map has no batched work; OFF is still 0;
- the host_exact comparisons: actor_skin, state_sha256, selected_lights_bits,
  joint_alpha_order, blend_world_order, submitted packets apart from gpu_slot,
  geometry == gpu_geometry, no serial fallback, frame count, intermediate images,
  the final PNG, and the census serial/worker vertex totals;
- every other probe counter matches. Counters with --counter-prefix are the witness and
  are excluded from that equality. cpu_ms and spans_ms are timings, not compared.
"""
from pathlib import Path
import argparse, hashlib, json, os, subprocess, sys

DATA = '/Users/anezium/vita-work/alice-data/Alice1/bin/base'
DT = {'pandemonium': '0.04184', 'hedge3': '0.06'}
FRAME_KEYS = ['actor_skin', 'state_sha256', 'selected_lights_bits', 'joint_alpha_order', 'blend_world_order']


def sha(data):
    return hashlib.sha256(data).hexdigest()


def canon(value):
    return sha(json.dumps(value, sort_keys=True, separators=(',', ':')).encode())


def packets(frame):
    return [{k: v for k, v in p.items() if k != 'gpu_slot'} for p in frame['submitted_packets']]


def main():
    p = argparse.ArgumentParser()
    p.add_argument('output', type=Path)
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--flag', required=True)
    p.add_argument('--maps', nargs='+', required=True)
    p.add_argument('--prerequisites', required=True, help='JSON object written into both arms')
    p.add_argument('--witness', action='append', required=True, help='counter 0 in OFF; > 0 in ON on required maps')
    p.add_argument('--require-positive', action='append', required=True, help='map whose ON witness must be > 0')
    p.add_argument('--counter-prefix', required=True)
    p.add_argument('--frames', type=int, default=180)
    p.add_argument('--label', default='host-flag-exact')
    args = p.parse_args()
    root = Path(__file__).resolve().parents[2]
    out = args.output.resolve()
    base = json.loads((root / 'tools/lever_matrix_r4/configs/PL-baseline.json').read_text())
    prerequisites = json.loads(args.prerequisites)
    assert isinstance(prerequisites, dict) and prerequisites, prerequisites
    assert args.flag not in prerequisites and args.flag not in base, 'the flag is supplied only by the arm'
    required = set(args.require_positive)
    assert required <= set(args.maps), (sorted(required - set(args.maps)))
    receipt_path = out / 'proof' / f'{args.label}.json'
    result = dict(
        label=args.label, binary=str(args.binary), binary_sha256=sha(args.binary.read_bytes()),
        flag=args.flag, prerequisites=prerequisites, witness=args.witness,
        require_positive=sorted(required), counter_prefix=args.counter_prefix,
        frames=args.frames, maps=[], status='RUNNING',
        oracle_sha256=sha(Path(__file__).read_bytes()))
    positive = {name: 0 for name in args.witness}
    for m in args.maps:
        arms = []
        for arm in (False, True):
            d = out / 'host' / args.label / f'{m}-{args.flag}-{int(arm)}'
            (d / 'private').mkdir(parents=True, exist_ok=False)
            config = dict(base, **prerequisites, **{args.flag: arm}, phase_census=True)
            (d / 'private/vita_renderer.json').write_text(json.dumps(config))
            env = {k: v for k, v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
            env.update(
                LOOKING_GLASS_SIM_MAPS_PROBE='1',
                LOOKING_GLASS_SIM_MAPS_DT=DT.get(m, '0.05'),
                LOOKING_GLASS_SKIN_PROOF='1',
                LOOKING_GLASS_SKIN_MOVING_VIEW='1',
                LOOKING_GLASS_SKIN_DEATHS='1')
            assert 'LOOKING_GLASS_SIM_MAPS_COUNTERS' not in env
            assert not any(k.startswith('LOOKING_GLASS_TEST_') for k in env)
            cmd = [str(args.binary), '--data', DATA, '--no-audio', '--save-dir', str(d / 'private/saves'),
                   '--map', m, '--frames', str(args.frames), '--capture', str(d / 'final.png')]
            with (d / 'run.log').open('w') as f:
                proc = subprocess.run(cmd, cwd=d, env=env, stdout=f, stderr=subprocess.STDOUT, timeout=1800)
            assert proc.returncode == 0, (m, arm, proc.returncode, str(d / 'run.log'))
            text = (d / 'run.log').read_text()
            lines = text.splitlines()
            configs = [json.loads(line.split('[lg/vita-renderer] config ', 1)[1])
                       for line in lines if '[lg/vita-renderer] config ' in line]
            assert len(configs) == 1, (m, arm, len(configs))
            printed = configs[0]
            assert printed[args.flag] is arm, (m, arm, 'printed flag', printed.get(args.flag))
            for key, value in prerequisites.items():
                assert printed[key] == value, (m, arm, 'prerequisite', key, printed.get(key), value)
            assert printed['phase_census'] is True, (m, arm, 'phase_census')
            single = [line for line in lines if line.startswith('[lg/vita-renderer] actor_single_prepare active=')]
            assert single == ['[lg/vita-renderer] actor_single_prepare active=true'], (m, arm, single)
            fallback = [line for line in lines if '[lg/skin] serial fallback' in line or 'afterimage consumer must be declared' in line]
            assert not fallback, (m, arm, fallback[:3])
            frames = [json.loads(line.split(' ', 1)[1]) for line in lines if line.startswith('SIM_MAPS_FRAME ')]
            images = [json.loads(line.split(' ', 1)[1]) for line in lines if line.startswith('SKIN_EXACT_IMAGE ')]
            assert len(frames) == args.frames, (m, arm, len(frames))
            assert all(all(packet['geometry'] == packet['gpu_geometry'] for packet in frame['submitted_packets']) for frame in frames), (m, arm, 'gpu readback')
            census = json.loads((d / f'{m}-census-{args.frames}.json').read_text())
            if 'serial_fallbacks' in census:
                assert census['serial_fallbacks'] == 0, (m, arm)
            rows = census['rows'][60:]
            counters = {}
            for frame in frames:
                for key, value in frame['counters'].items():
                    counters[key] = counters.get(key, 0) + value
            arms.append(dict(
                arm=arm, frames=frames, images=images, counters=counters,
                serial_vertices=sum(actor['serial_vertices'] for row in rows for actor in row['actors']),
                worker_vertices=sum(actor['worker_vertices'] for row in rows for actor in row['actors']),
                final_png_sha256=sha((d / 'final.png').read_bytes()),
                run_log_sha256=sha((d / 'run.log').read_bytes()),
                printed_flag=printed[args.flag],
                printed_prerequisites={key: printed[key] for key in prerequisites},
                gpu_draw_readbacks=sum(len(frame['submitted_packets']) for frame in frames)))
            print('DONE', args.label, m, arm, len(frames), 'GPU draws', arms[-1]['gpu_draw_readbacks'], flush=True)
        off, on = arms
        for n, (a, b) in enumerate(zip(off['frames'], on['frames'])):
            for key in FRAME_KEYS:
                assert a[key] == b[key], (m, n, key)
            assert packets(a) == packets(b), (m, n, 'uploaded GPU packets')
            other = lambda frame: {k: v for k, v in frame['counters'].items() if not k.startswith(args.counter_prefix)}
            assert other(a) == other(b), (m, n, 'workload counters', other(a), other(b))
        assert off['images'] == on['images'], (m, 'intermediate pixels')
        assert off['final_png_sha256'] == on['final_png_sha256'], (m, 'final image')
        assert (off['serial_vertices'], off['worker_vertices']) == (on['serial_vertices'], on['worker_vertices']), (m, 'census vertices')
        witness = {}
        for name in args.witness:
            pair = [off['counters'].get(name, 0), on['counters'].get(name, 0)]
            witness[name] = pair
            assert pair[0] == 0, (m, 'OFF witness', name, pair[0])
            if m in required:
                assert pair[1] > 0, (m, 'vacuous ON witness', name, pair[1])
            positive[name] += pair[1]
        prefixed = {k: [off['counters'].get(k, 0), on['counters'].get(k, 0)]
                    for k in sorted(set(off['counters']) | set(on['counters'])) if k.startswith(args.counter_prefix)}
        row = dict(
            map=m, dt=DT.get(m, '0.05'), frames=args.frames, witness_off_on=witness,
            prefixed_counters_off_on=prefixed, required_positive=m in required,
            frame_digest=[canon([{k: frame[k] for k in FRAME_KEYS} for frame in arm['frames']]) for arm in arms],
            packet_digest=[canon([packets(frame) for frame in arm['frames']]) for arm in arms],
            image_digest=[canon(arm['images']) for arm in arms], image_witnesses=len(off['images']),
            final_png_sha256=[arm['final_png_sha256'] for arm in arms],
            run_log_sha256=[arm['run_log_sha256'] for arm in arms],
            gpu_draw_readbacks=[arm['gpu_draw_readbacks'] for arm in arms],
            census_serial_worker_vertices=[[arm['serial_vertices'], arm['worker_vertices']] for arm in arms],
            printed=[dict(flag=arm['printed_flag'], prerequisites=arm['printed_prerequisites']) for arm in arms],
            status='PASS')
        result['maps'].append(row)
        receipt_path.parent.mkdir(parents=True, exist_ok=True)
        receipt_path.write_text(json.dumps(result, indent=1) + '\n')
        print('PASS', args.label, m, 'witness', json.dumps(witness), flush=True)
    assert all(v > 0 for v in positive.values()), ('vacuous oracle', positive)
    result['witness_on_total'] = positive
    result['status'] = 'PASS'
    receipt_path.write_text(json.dumps(result, indent=1) + '\n')
    print('PASS', args.label, 'witness totals', json.dumps(positive), flush=True)


if __name__ == '__main__':
    try:
        main()
    except Exception as exc:
        print('FAIL', exc, file=sys.stderr)
        raise
