#!/usr/bin/env python3
"""Save-free all-map retained inventory and deterministic paired entry captures.
All output belongs to --evidence; archives under --data are opened read-only by the game.
"""
import argparse, hashlib, json, math, os, shutil, subprocess, time
from pathlib import Path
from PIL import Image, ImageChops

p = argparse.ArgumentParser()
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--data', type=Path, required=True)
p.add_argument('--evidence', type=Path, required=True)
p.add_argument('--visits', type=Path, required=True)
p.add_argument('--config', type=Path, required=True)
p.add_argument('--sim', type=Path, required=True)
p.add_argument('--reuse-captures', action='store_true', help='Reuse hash-verified cells from the same binary/config after a driver interruption')
a = p.parse_args()
E = a.evidence.resolve()
E.mkdir(parents=True, exist_ok=True)
visits = json.loads(a.visits.read_text())
assert len(visits) == 39 and len({v['map'] for v in visits}) == 36
cfg = json.loads(a.config.read_text())
previous_cells = {}
if a.reuse_captures:
    previous = json.loads((E/'IMAGE-DIFFERENCES.json').read_text())
    assert previous['binary_sha256'] == hashlib.sha256(a.binary.read_bytes()).hexdigest()
    for visit in previous['visits']:
        for quality, comparison in visit['comparisons'].items():
            for renderer in ['legacy', 'vita']:
                record = comparison['legacy' if renderer == 'legacy' else 'retained']
                previous_cells[(visit['label'], quality, renderer)] = record


def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def check_lock():
    lock = Path.home()/'vita-j0/mac-timing.lock'
    # Wait cooperatively; no new process starts while another track owns timing.
    for _ in range(360):
        if not lock.exists(): return
        print('Timing lock active; waiting:', lock.read_text().strip(), flush=True)
        time.sleep(30)
    raise RuntimeError('Mac timing lock still active after 3 hours')

def run_cell(v, quality, renderer, failure=False):
    name = v['label']
    cell = E/('fallback-fortress1' if failure else f'images/{name}/{quality}/{renderer}')
    private = cell/'private'
    private.mkdir(parents=True, exist_ok=True)
    config = dict(cfg, renderer=renderer, retained_maps=['*'], legacy_maps=[])
    # Single preparation requires the retained actor bridge; legacy is its control.
    config['actor_single_prepare'] = renderer == 'vita' and cfg.get('actor_single_prepare', True)
    if quality == 'full': config.update(quality='full', render_scale=1.0)
    previous = previous_cells.get((name, quality, renderer)) if not failure else None
    if previous is not None:
        assert sha(E/previous['image']) == previous['image_sha256']
        assert sha(private/'vita_renderer.json') == previous['config_sha256']
        assert json.loads((private/'vita_renderer.json').read_text()) == config
        assert (private/'vita_sim.json').read_bytes() == a.sim.read_bytes()
        return previous
    (private/'vita_renderer.json').write_text(json.dumps(config, indent=2)+'\n')
    shutil.copy2(a.sim, private/'vita_sim.json')
    capture = cell/'entry.png'
    cmd = [str(a.binary.resolve()), '--data', str(a.data.resolve()), '--map', v['map'],
           '--allmaps-entry-check', '--no-audio', '--frames', '3', '--capture', str(capture)]
    if v['entry']: cmd += ['--entry', v['entry']]
    env = {k: val for k, val in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
    env['LOOKING_GLASS_HOST_CLOCK_STEP'] = '0'
    if failure: env['LOOKING_GLASS_RETAINED_FAIL'] = 'after-upload'
    check_lock()
    with (cell/'run.log').open('w') as log:
        result = subprocess.run(cmd, cwd=cell, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=300)
    text = (cell/'run.log').read_text()
    assert result.returncode == 0, (cell, result.returncode, text[-2000:])
    assert 'invalid config' not in text
    record = json.loads((private/'allmaps-entry.json').read_text())
    failures = record['report']['construction_failures']
    assert record['renderer'] == ('legacy' if failure or failures else renderer), cell
    assert failures == 1 if failure else failures in (0, 1), cell
    assert Image.open(capture).size == (960, 544)
    record.update(image=str(capture.relative_to(E)), image_sha256=sha(capture), config_sha256=sha(private/'vita_renderer.json'))
    return record

def difference(x, y, path):
    left, right = Image.open(E/x['image']).convert('RGB'), Image.open(E/y['image']).convert('RGB')
    diff = ImageChops.difference(left, right)
    diff.save(path)
    hist = diff.histogram()
    pixels = left.width*left.height
    absolute = sum((i % 256)*n for i,n in enumerate(hist))
    square = sum((i % 256)**2*n for i,n in enumerate(hist))
    channels = diff.split()
    mask = ImageChops.lighter(ImageChops.lighter(channels[0], channels[1]), channels[2])
    changed = pixels-mask.histogram()[0]
    return dict(changed_pixels=changed, total_pixels=pixels, changed_fraction=changed/pixels,
                mean_absolute_rgb=absolute/(pixels*3), rms_rgb=math.sqrt(square/(pixels*3)),
                max_rgb=max(i%256 for i,n in enumerate(hist) if n), diff_png=str(path.relative_to(E)))

result = dict(binary_sha256=sha(a.binary), data=str(a.data), seed=0x416c696365,
              material_time=1.0, dimensions=[960,544], console_access=False, performance_claim=False,
              scope='world and ambient props at initial chapter gameplay spawn; no cast/HUD/opening cinematic replay',
              visits=[])
for v in visits:
    row = {k:v[k] for k in ['map','entry','chapter_index','label']}
    row['comparisons'] = {}
    for quality in ['full','low']:
        legacy = run_cell(v, quality, 'legacy')
        retained = run_cell(v, quality, 'vita')
        for field in ['spawn_eye','player_feet','yaw','transforms','material_time','seed']:
            assert legacy[field] == retained[field], (v, field)
        assert legacy['report']['view'] == retained['report']['view']
        row['comparisons'][quality] = dict(legacy=legacy, retained=retained,
            difference=difference(legacy, retained, E/f"images/{v['label']}/{quality}/difference.png"))
    result['visits'].append(row)
    (E/'IMAGE-DIFFERENCES.json').write_text(json.dumps(result, indent=2)+'\n')
    print('PASS paired captures',v['label'],flush=True)

maps = []
seen = set()
for row in result['visits']:
    if row['map'] in seen: continue
    seen.add(row['map'])
    r = row['comparisons']['low']['retained']
    inventory = r['report']['inventory']
    if inventory is None:
        maps.append(dict(map=row['map'], retained_construction_failed=True, log=f"images/{row['label']}/low/vita/run.log"))
        continue
    assert inventory['groups'] > 0
    maps.append(dict(map=row['map'], **inventory, portal_candidate_surfaces=r['portal_candidate_surfaces'], authored_sky_surfaces=r['authored_sky_surfaces']))
inv = dict(binary_sha256=result['binary_sha256'], settings='installed PL-def + retained_maps [*], vita-sim-all',
           host_abi='aarch64 Mac; sizes and capacities are host measurements; explicit vita PVS projection uses 32-bit indices',
           construction_failures=[r['map'] for r in maps if r.get('retained_construction_failed')], map_count=36, visit_count=39, maps=maps,
           visits=[{k:v[k] for k in ['map','entry','chapter_index','label']} for v in visits])
(E/'INVENTORY.json').write_text(json.dumps(inv,indent=2)+'\n')
lines = ['# Retained host inventory', '', 'Bytes are buffer payload/capacity, not total process RAM. Host ABI; PVS32 is the Vita index-width projection.', '',
         '| Map | Geometry | PVS32 incl. unknown | Uniform staging | Buffer peak | Groups | Drop / deform / ambient | Sky / ropes |',
         '|---|---:|---:|---:|---:|---:|---:|---:|']
for r in maps:
    if r.get('retained_construction_failed'):
        lines.append(f"| {r['map']} | FAILED; see log | | | | | | |"); continue
    lines.append(f"| {r['map']} | {r['gpu_geometry_bytes']} | {r['vita_cluster_list_payload_including_unknown_bytes']} | {r['uniform_staging_capacity_bytes']} | {r['construction_buffer_peak_bytes']} | {r['groups']} | {r['dropped_layers']} / {r['frozen_deforms']} / {r['frozen_ambient_models']} | {r['skipped_sky_batches']} / {r['frozen_ropes']} |")
(E/'INVENTORY.md').write_text('\n'.join(lines)+'\n')
fallback = run_cell(next(v for v in visits if v['map']=='fortress1'), 'low', 'vita', True)
assert fallback['legacy_portals_loaded'] > 0
legacy_portal_map = next(r for r in result['visits'] if r['map']=='fortress1')['comparisons']['low']['legacy']
assert fallback['image_sha256'] == legacy_portal_map['image_sha256'], 'fallback legacy PNG differs'
fallback['legacy_image_byte_identical'] = True
(E/'FALLBACK.json').write_text(json.dumps(fallback,indent=2)+'\n')
print('PASS all 36 maps / 39 visits, 156 captures; injected fallback restores Fortress portals', flush=True)
