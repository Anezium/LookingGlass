#!/usr/bin/env python3
"""Parameterized host ON/OFF oracle for one new runtime flag. Mac-only, no device transport.

tools/skin_exact_r9/host_exact.py stays unchanged for r9 provenance; it toggles
skin_actual_consumers. This oracle toggles exactly one named flag. Each arm writes
dict(base, **prerequisites, <flag>=arm) to private/vita_renderer.json, so the prerequisites
are explicit and identical in both arms. The environment is rebuilt without any
LOOKING_GLASS_* variable, then only the probe variables below (plus explicit --probe-env
LOOKING_GLASS_* variables) are set; LOOKING_GLASS_TEST_* switches never reach this binary.

Checks, per map:
- the printed effective config of each arm carries the flag value and every prerequisite;
- the workload moved: every --witness counter is > 0 summed over the ON arm's frames and 0 in
  OFF, every --removed counter is 0 in ON and > 0 in OFF; all-zero witnesses are FAIL (vacuous);
- every host_exact.py comparison: actor_skin, state_sha256, selected_lights_bits,
  joint_alpha_order, blend_world_order, submitted packets apart from gpu_slot, geometry ==
  gpu_geometry for every packet, no serial fallback, frame count, intermediate image witnesses
  and the final PNG;
- additionally: npc_decisions, frame/map/active, every other counter (counters named by
  --counter-prefix are the witness and are excluded), and the census serial/worker vertices.

With --control-binary (a build of the base commit, which does not know the flag), a third run
writes dict(base, **prerequisites) without the flag key and must match the OFF arm on every
comparison above: the flag-off path is baseline-equivalent.
"""
from pathlib import Path
import argparse, hashlib, json, os, subprocess

DATA = '/Users/anezium/vita-work/alice-data/Alice1/bin/base'
DT = {'pandemonium': '0.04184', 'hedge3': '0.06'}
FRAME_KEYS = ['actor_skin', 'state_sha256', 'selected_lights_bits', 'joint_alpha_order', 'blend_world_order']
EXTRA_KEYS = ['npc_decisions', 'frame', 'map', 'active']


def sha(data): return hashlib.sha256(data).hexdigest()


def canon(value): return sha(json.dumps(value, sort_keys=True, separators=(',', ':')).encode())


def main():
    p = argparse.ArgumentParser()
    p.add_argument('output', type=Path)
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--flag', required=True)
    p.add_argument('--maps', nargs='+', required=True)
    p.add_argument('--prerequisites', default='{}', help='JSON object written into both arms')
    p.add_argument('--witness', action='append', default=[], help='counter > 0 in ON, 0 in OFF')
    p.add_argument('--removed', action='append', default=[], help='counter 0 in ON, > 0 in OFF')
    p.add_argument('--counter-prefix', default=None, help='witness counters excluded from the counter equality')
    p.add_argument('--probe-env', action='append', default=[], help='extra LOOKING_GLASS_*=value probe variable')
    p.add_argument('--frames', type=int, default=180)
    p.add_argument('--label', default='host-flag-exact')
    p.add_argument('--control-binary', type=Path, default=None)
    args = p.parse_args()
    R = Path(__file__).resolve().parents[2]
    O = args.output.resolve()
    base = json.loads((R / 'tools/lever_matrix_r4/configs/PL-baseline.json').read_text())
    prerequisites = json.loads(args.prerequisites)
    assert args.flag not in prerequisites and args.flag not in base, 'the flag must come only from the arm'
    assert args.witness, 'a workload witness is mandatory'
    extra = dict(kv.split('=', 1) for kv in args.probe_env)
    assert all(k.startswith('LOOKING_GLASS_') for k in extra), extra
    receipt_path = O / 'proof' / f'{args.label}.json'
    result = dict(label=args.label, binary=str(args.binary), binary_sha256=sha(args.binary.read_bytes()),
                  flag=args.flag, prerequisites=prerequisites, probe_env=extra, witness=args.witness,
                  removed=args.removed, counter_prefix=args.counter_prefix, maps=[], status='RUNNING',
                  control_binary=str(args.control_binary) if args.control_binary else None,
                  control_binary_sha256=sha(args.control_binary.read_bytes()) if args.control_binary else None)
    for m in args.maps:
        arms = []
        runs = [(False, args.binary), (True, args.binary)] + ([(None, args.control_binary)] if args.control_binary else [])
        for arm, binary in runs:
            d = O / 'host' / args.label / f'{m}-{args.flag}-{"control" if arm is None else int(arm)}'
            (d / 'private').mkdir(parents=True, exist_ok=False)
            flag = {} if arm is None else {args.flag: arm}
            config = dict(base, **prerequisites, **flag, phase_census=True)
            (d / 'private/vita_renderer.json').write_text(json.dumps(config))
            env = {k: v for k, v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
            env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1', LOOKING_GLASS_SIM_MAPS_DT=DT.get(m, '0.05'),
                       LOOKING_GLASS_SKIN_PROOF='1', LOOKING_GLASS_SKIN_MOVING_VIEW='1', LOOKING_GLASS_SKIN_DEATHS='1', **extra)
            cmd = [str(binary), '--data', DATA, '--no-audio', '--save-dir', str(d / 'private/saves'),
                   '--map', m, '--frames', str(args.frames), '--capture', str(d / 'final.png')]
            with (d / 'run.log').open('w') as f:
                proc = subprocess.run(cmd, cwd=d, env=env, stdout=f, stderr=subprocess.STDOUT, timeout=1800)
            assert proc.returncode == 0, (m, arm, proc.returncode, str(d / 'run.log'))
            t = (d / 'run.log').read_text()
            lines = t.splitlines()
            configs=[json.loads(l.split('[lg/vita-renderer] config ',1)[1]) for l in t.splitlines() if '[lg/vita-renderer] config ' in l]
            assert len(configs) == 1, (m, arm, len(configs))
            printed = configs[0]
            if arm is None:
                assert args.flag not in printed, (m, 'control binary must predate the flag')
            else:
                assert printed[args.flag] is arm, (m, arm, 'printed flag')
            for k, v in prerequisites.items():
                assert printed[k] == v, (m, arm, 'prerequisite', k, printed[k], v)
            single = [l for l in lines if l.startswith('[lg/vita-renderer] actor_single_prepare active=')]
            assert single == ['[lg/vita-renderer] actor_single_prepare active=true'], (m, arm, single)
            fallback = [l for l in lines if '[lg/skin] serial fallback' in l or 'afterimage consumer must be declared' in l]
            assert not fallback, (m, arm, fallback[:3])
            frames = [json.loads(l.split(' ', 1)[1]) for l in lines if l.startswith('SIM_MAPS_FRAME ')]
            images = [json.loads(l.split(' ', 1)[1]) for l in lines if l.startswith('SKIN_EXACT_IMAGE ')]
            assert len(frames) == args.frames, (m, arm, len(frames))
            assert all(all(p['geometry'] == p['gpu_geometry'] for p in f['submitted_packets']) for f in frames), (m, arm, 'gpu readback')
            census = json.loads((d / f'{m}-census-{args.frames}.json').read_text())
            if 'serial_fallbacks' in census:
                assert census['serial_fallbacks'] == 0, (m, arm)
            rs = census['rows'][60:]
            counters = {}
            for f in frames:
                for k, v in f['counters'].items():
                    counters[k] = counters.get(k, 0) + v
            arms.append(dict(arm=arm, frames=frames, images=images, counters=counters,
                             serial_vertices=sum(a['serial_vertices'] for r in rs for a in r['actors']),
                             worker_vertices=sum(a['worker_vertices'] for r in rs for a in r['actors']),
                             final_png_sha256=sha((d / 'final.png').read_bytes()),
                             run_log_sha256=sha((d / 'run.log').read_bytes()),
                             printed_flag=printed.get(args.flag), printed_prerequisites={k: printed[k] for k in prerequisites},
                             gpu_draw_readbacks=sum(len(f['submitted_packets']) for f in frames)))
            print('DONE', args.label, m, arm, len(frames), 'GPU draws', arms[-1]['gpu_draw_readbacks'], flush=True)
        off, on = arms[:2]
        for other_arm in arms[1:]:
            name = 'ON' if other_arm is on else 'control'
            for n, (a, b) in enumerate(zip(off['frames'], other_arm['frames'])):
                for k in FRAME_KEYS + EXTRA_KEYS:
                    assert a[k] == b[k], (m, name, n, k)
                packets = lambda f: [{k: v for k, v in p.items() if k != 'gpu_slot'} for p in f['submitted_packets']]
                assert packets(a) == packets(b), (m, name, n, 'uploaded GPU packets')
                other = lambda f: {k: v for k, v in f['counters'].items() if not (args.counter_prefix and k.startswith(args.counter_prefix))}
                assert other(a) == other(b), (m, name, n, 'workload counters')
            assert off['images'] == other_arm['images'], (m, name, 'intermediate pixels')
            assert off['final_png_sha256'] == other_arm['final_png_sha256'], (m, name, 'final image')
            assert (off['serial_vertices'], off['worker_vertices']) == (other_arm['serial_vertices'], other_arm['worker_vertices']), (m, name, 'census vertices')
        if len(arms) == 3:
            assert not any(args.counter_prefix and k.startswith(args.counter_prefix) for k in arms[2]['counters']), (m, 'control has no witness counters')
        witness = {name: [off['counters'].get(name, 0), on['counters'].get(name, 0)] for name in args.witness}
        removed = {name: [off['counters'].get(name, 0), on['counters'].get(name, 0)] for name in args.removed}
        for name, (a, b) in witness.items():
            assert a == 0 and b > 0, (m, 'witness', name, a, b)
        for name, (a, b) in removed.items():
            assert a > 0 and b == 0, (m, 'removed', name, a, b)
        row = dict(map=m, dt=DT.get(m, '0.05'), frames=args.frames, witness_off_on=witness, removed_off_on=removed,
                   prefixed_counters_off_on={k: [off['counters'].get(k, 0), on['counters'].get(k, 0)]
                                             for k in sorted(set(off['counters']) | set(on['counters']))
                                             if args.counter_prefix and k.startswith(args.counter_prefix)},
                   frame_digest=[canon([{k: f[k] for k in FRAME_KEYS + EXTRA_KEYS} for f in x['frames']]) for x in arms],
                   packet_digest=[canon([[{k: v for k, v in p.items() if k != 'gpu_slot'} for p in f['submitted_packets']] for f in x['frames']]) for x in arms],
                   image_digest=[canon(x['images']) for x in arms], image_witnesses=len(off['images']),
                   final_png_sha256=[x['final_png_sha256'] for x in arms], run_log_sha256=[x['run_log_sha256'] for x in arms],
                   gpu_draw_readbacks=[x['gpu_draw_readbacks'] for x in arms],
                   census_serial_worker_vertices=[[x['serial_vertices'], x['worker_vertices']] for x in arms],
                   printed=[dict(flag=x['printed_flag'], prerequisites=x['printed_prerequisites']) for x in arms],
                   control_matches_off=len(arms) == 3, status='PASS')
        result['maps'].append(row)
        receipt_path.parent.mkdir(parents=True, exist_ok=True)
        receipt_path.write_text(json.dumps(result, indent=1) + '\n')
        print('PASS', args.label, m, 'witness', json.dumps(witness), 'removed', json.dumps(removed), flush=True)
    result['status'] = 'PASS'
    receipt_path.write_text(json.dumps(result, indent=1) + '\n')


if __name__ == '__main__':
    main()
