"""Mac-only parameterized ON/OFF oracle for one renderer flag. No device transport.

It keeps every host_exact.py map comparison (tools/skin_exact_r9, left unchanged for r9
provenance) and toggles only the named flag. Prerequisites are written explicitly in both
arms. The environment is rebuilt without any LOOKING_GLASS_* variable, then only the probe
variables below are set; the flag reaches the non-test binary only through its JSON config.

Per map and arm it checks the printed effective config (flag and every prerequisite), that
no serial fallback was logged, 180 SIM_MAPS_FRAME rows, geometry == gpu_geometry for every
retained draw (actual GL buffer readback), and zero census serial fallbacks. Across arms it
compares actor_skin, state_sha256, selected_lights_bits, joint_alpha_order, blend_world_order,
uploaded packets apart from gpu_slot, the intermediate image witnesses and the final PNG, plus
the per-frame vita_fx_dropped_layers counter. With --baseline-binary it also runs a reference
binary that predates the flag (its config omits the flag key) and requires the candidate OFF
arm to equal it on the same fields, including gpu_slot and every reference counter.

Witness: --witness serial_vertices_drop requires the census serial vertices (measured rows
60..179) to drop in the ON arm on at least one map, and on every --witness-map. A run with no
drop anywhere is FAIL (vacuous).
"""
from pathlib import Path
import argparse, hashlib, json, os, subprocess, sys

p = argparse.ArgumentParser()
p.add_argument('output', type=Path)
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--baseline-binary', type=Path)
p.add_argument('--flag', required=True)
p.add_argument('--prereq', default='{}', help='JSON object written in every arm')
p.add_argument('--maps', nargs='+', required=True)
p.add_argument('--witness', choices=['serial_vertices_drop'], required=True)
p.add_argument('--witness-map', action='append', default=[])
p.add_argument('--data', default='/Users/anezium/vita-work/alice-data/Alice1/bin/base')
args = p.parse_args()
R = Path(__file__).resolve().parents[2]
O = args.output.resolve()
(O / 'proof').mkdir(parents=True, exist_ok=True)
base = json.loads((R / 'tools/lever_matrix_r4/configs/PL-baseline.json').read_text())
prereq = json.loads(args.prereq)
assert args.flag not in prereq
sha = lambda path: hashlib.sha256(Path(path).read_bytes()).hexdigest()
DT = {'pandemonium': '0.04184', 'hedge3': '0.06'}
result = dict(flag=args.flag, prerequisites=prereq, base_config=str(R / 'tools/lever_matrix_r4/configs/PL-baseline.json'),
              base_config_sha256=sha(R / 'tools/lever_matrix_r4/configs/PL-baseline.json'),
              binary=str(args.binary), binary_sha256=sha(args.binary),
              baseline_binary=str(args.baseline_binary) if args.baseline_binary else None,
              baseline_binary_sha256=sha(args.baseline_binary) if args.baseline_binary else None,
              gpu_observer='glGetBufferSubData on the bound vertex attribute and element buffers before every retained draw',
              elapsed_counters_not_compared=['skin_worker_service_us', 'skin_collect_wait_us', 'skin_gather_us', 'fx_worker_wait_us', 'fx_worker_max_service_us'],
              uniforms_not_compared='sha256 of live f32 bits; a repeated 0548549 jlair1 run changed frame 0 packet 0 while geometry matched',
              maps={}, status='INCOMPLETE')
out_json = O / 'proof' / f'HOST-FLAG-EXACT-{args.flag}.json'

def write():
    out_json.write_text(json.dumps(result, indent=1) + '\n')

def run(m, tag, binary, config):
    d = O / 'host' / f'{m}-{tag}'
    if not ((d / 'run.log').is_file() and (d / f'{m}-census-180.json').is_file()):
        (d / 'private').mkdir(parents=True, exist_ok=False)
        (d / 'private/vita_renderer.json').write_text(json.dumps(config))
        env = {k: v for k, v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
        env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1', LOOKING_GLASS_SIM_MAPS_DT=DT.get(m, '0.05'), LOOKING_GLASS_SIM_MAPS_COUNTERS='1',
                   LOOKING_GLASS_SKIN_PROOF='1', LOOKING_GLASS_SKIN_MOVING_VIEW='1', LOOKING_GLASS_SKIN_DEATHS='1')
        cmd = ['/usr/bin/time', '-l', '-o', str(d / 'rusage.txt'), str(binary), '--data', args.data, '--no-audio',
               '--save-dir', str(d / 'private/saves'), '--map', m, '--frames', '180', '--capture', str(d / 'final.png')]
        with (d / 'run.log').open('w') as f:
            proc = subprocess.run(cmd, cwd=d, env=env, stdout=f, stderr=subprocess.STDOUT, timeout=1800)
        assert proc.returncode == 0, (m, tag, proc.returncode, str(d / 'run.log'))
    lines = (d / 'run.log').read_text().splitlines()
    configs = [json.loads(l.split('[lg/vita-renderer] config ', 1)[1]) for l in lines if l.startswith('[lg/vita-renderer] config ')]
    assert len(configs) == 1, (m, tag, 'one effective config record')
    effective = configs[0]
    for k, v in prereq.items():
        assert effective[k] == v, (m, tag, k, effective[k], v)
    if args.flag in config:
        assert effective[args.flag] is config[args.flag], (m, tag, args.flag)
    else:
        assert args.flag not in effective, (m, tag, 'reference binary must predate the flag')
    assert '[lg/vita-renderer] actor_single_prepare active=true' in lines, (m, tag, 'actor_single_prepare inactive')
    fallback = [l for l in lines if '[lg/skin] serial fallback' in l or 'afterimage consumer must be declared' in l]
    assert not fallback, (m, tag, fallback[:3])
    frames = [json.loads(l.split(' ', 1)[1]) for l in lines if l.startswith('SIM_MAPS_FRAME ')]
    images = [json.loads(l.split(' ', 1)[1]) for l in lines if l.startswith('SKIN_EXACT_IMAGE ')]
    assert len(frames) == 180, (m, tag, len(frames))
    assert all(all(q['geometry'] == q['gpu_geometry'] for q in f['submitted_packets']) for f in frames), (m, tag, 'GPU readback')
    census = json.loads((d / f'{m}-census-180.json').read_text())
    assert census['serial_fallbacks'] == 0, (m, tag)
    rows = census['rows'][60:]
    rss = [l.split()[0] for l in (d / 'rusage.txt').read_text().splitlines() if 'maximum resident set size' in l]
    counters = [f['counters'] for f in frames]
    measured = counters[60:]
    def mean(name): return sum(c.get(name, 0) for c in measured) / len(measured)
    def peak(name): return max(c.get(name, 0) for c in counters)
    row = dict(map=m, arm=tag, run_dir=str(d), config_sha256=sha(d / 'private/vita_renderer.json'), log_sha256=sha(d / 'run.log'),
               serial_vertices=sum(a['serial_vertices'] for r in rows for a in r['actors']) / 120,
               worker_vertices=sum(a['worker_vertices'] for r in rows for a in r['actors']) / 120,
               final_png_sha256=sha(d / 'final.png'), gpu_draw_readbacks=sum(len(f['submitted_packets']) for f in frames),
               max_rss_bytes=int(rss[0]) if rss else None,
               counters_measured_mean={k: mean(k) for k in sorted({k for c in measured for k in c})},
               counters_peak={k: peak(k) for k in ['actor_publication_bank_bytes', 'actor_publication_bank_count', 'actor_deferred_surfaces',
                                                    'actor_deferred_capacity_bytes', 'actor_deferred_layer_copy_bytes', 'actor_layer_copy_bytes']})
    print('DONE', m, tag, 'serial', row['serial_vertices'], 'worker', row['worker_vertices'], 'draws', row['gpu_draw_readbacks'], flush=True)
    return row, frames, images

def compare(m, a, b, label, full):
    (ra, fa, ia), (rb, fb, ib) = a, b
    for n, (x, y) in enumerate(zip(fa, fb)):
        for k in ['actor_skin', 'state_sha256', 'selected_lights_bits', 'joint_alpha_order', 'blend_world_order']:
            assert x[k] == y[k], (m, label, n, k)
        # uniforms is a hash of live float bits. A second run of the 0548549 binary on jlair1
        # changed frame 0 packet 0 while geometry stayed equal, so the hash is not a stable output.
        packets = lambda f: [{k: v for k, v in q.items() if k not in ('gpu_slot', 'uniforms')} for q in f['submitted_packets']]
        assert packets(x) == packets(y), (m, label, n, 'uploaded GPU packets')
        assert x['counters'].get('vita_fx_dropped_layers', 0) == y['counters'].get('vita_fx_dropped_layers', 0), (m, label, n, 'dropped layers')
        if full:
            with_slot = lambda f: [{k: v for k, v in q.items() if k != 'uniforms'} for q in f['submitted_packets']]
            assert with_slot(x) == with_slot(y), (m, label, n, 'packets including gpu_slot')
            # Worker service microseconds are a clock reading. Two builds of the same draws
            # do not land on the same number, so they are not an output comparison.
            elapsed = {'skin_worker_service_us', 'skin_collect_wait_us', 'skin_gather_us',
                       'fx_worker_wait_us', 'fx_worker_max_service_us'}
            assert all(y['counters'].get(k) == v for k, v in x['counters'].items() if k not in elapsed), (m, label, n, 'reference counters')
    assert len(fa) == len(fb) == 180
    assert ia == ib, (m, label, 'intermediate pixels')
    assert ra['final_png_sha256'] == rb['final_png_sha256'], (m, label, 'final image')
    print('PASS', m, label, flush=True)

for m in args.maps:
    entry = result['maps'][m] = {}
    off = run(m, 'off', args.binary, dict(base, **prereq, **{args.flag: False}))
    on = run(m, 'on', args.binary, dict(base, **prereq, **{args.flag: True}))
    entry['off'], entry['on'] = off[0], on[0]
    compare(m, off, on, 'off-vs-on', False)
    entry['off_vs_on'] = 'PASS'
    if args.baseline_binary:
        ref = run(m, 'baseline', args.baseline_binary, dict(base, **prereq))
        entry['baseline'] = ref[0]
        compare(m, ref, off, 'baseline-vs-off', True)
        entry['baseline_vs_off'] = 'PASS'
    entry['witness_serial_vertex_drop'] = off[0]['serial_vertices'] - on[0]['serial_vertices']
    write()

drops = {m: e['witness_serial_vertex_drop'] for m, e in result['maps'].items()}
result['witness'] = dict(kind=args.witness, serial_vertex_drop_per_map=drops, required_maps=args.witness_map)
if not any(v > 0 for v in drops.values()):
    result['status'] = 'FAIL (vacuous: no map moved serial vertices to the workers)'
    write(); sys.exit(1)
missing = [m for m in args.witness_map if not drops.get(m, 0) > 0]
if missing:
    result['status'] = f'FAIL (no witness on required maps {missing})'
    write(); sys.exit(1)
result['status'] = 'PASS'
write()
print('PASS', json.dumps(drops), flush=True)
