"""Persistent recovery screen, repeated Enter and save-preserving Escape, host only."""
from pathlib import Path
import hashlib, json, os, shutil, subprocess, sys
from PIL import Image

O = Path(sys.argv[1]).resolve()
B = Path(sys.argv[2]).resolve()
R = Path(__file__).resolve().parents[2]
base = json.loads((R / 'tools/lever_matrix_r4/configs/PL-baseline.json').read_text())
base.update(phase_census=False, profile_capture=False, audio_music_thread=False, story_film_bypass=True)
seed = O / 'host/seed-hedge3/private/saves/quick.json'
human = Path('/Users/anezium/vita-j0/resume-2026-10-24/alice-lever-matrix-r3/session-s5-world-static-merge/originals/data/alice/private/saves/auto.json')

def saves(directory):
    return {str(p.relative_to(directory)): dict(sha256=hashlib.sha256(p.read_bytes()).hexdigest(),
                                               bytes=p.stat().st_size, mtime_ns=p.stat().st_mtime_ns)
            for p in directory.rglob('*') if p.is_file()}

def run(on, action, mode):
    directory = O / 'persistent' / f'{action}-{mode}-{int(on)}'
    private = directory / 'private'
    save = private / 'saves'
    save.mkdir(parents=True, exist_ok=True)
    (private / 'vita_renderer.json').write_text(json.dumps(dict(base,
        audio_static_cache=on, collision_sah=on, skin_actual_consumers=on)))
    shutil.copyfile(seed, save / 'quick.json')
    shutil.copyfile(human, save / 'auto.json')
    # Include companion and unrelated bytes, and assert the entire tree survives.
    (save / 'auto.previous.json').write_bytes(seed.read_bytes())
    (save / 'untouched.bin').write_bytes(b'preserve unrelated save bytes\x00\xff')
    before = saves(save)
    env = {k: v for k, v in os.environ.items() if not k.startswith('LOOKING_GLASS_')}
    env.update(LOOKING_GLASS_SIM_MAPS_PROBE='1', LOOKING_GLASS_SIM_MAPS_DT='0.05',
               LOOKING_GLASS_SAH_SILENT_AUDIO='1', LOOKING_GLASS_SKIN_PROOF='1',
               LOOKING_GLASS_TRANSITION_ACTION=action, LOOKING_GLASS_TRANSITION_DESTINATION='pandemonium',
               LOOKING_GLASS_TRANSITION_FAIL_BUILD='scene')
    if mode == 'reference':
        env['LOOKING_GLASS_TRANSITION_LEGACY'] = '1'
    else:
        env.update(LOOKING_GLASS_TRANSITION_FAIL_RECOVERY='always', LOOKING_GLASS_RECOVERY_UI=mode,
                   LOOKING_GLASS_RECOVERY_CAPTURE=str(directory))
    argv = [str(B), '--data', '/Users/anezium/vita-work/alice-data/Alice1/bin/base',
            '--map', 'hedge3', '--save-dir', str(save)]
    if mode != 'quit':
        argv += ['--frames', '8', '--capture', str(directory / 'final.png')]
    # Quit deliberately uses the normal, writable-save path (no --frames).
    with (directory / 'run.log').open('w') as log:
        result = subprocess.run(argv, cwd=directory, env=env, stdout=log,
                                stderr=subprocess.STDOUT, timeout=90)
    lines = (directory / 'run.log').read_text().splitlines()
    assert result.returncode == 0, (directory, result.returncode)
    assert before == saves(save), (directory, 'save tree changed')
    records = [json.loads(s.split('] ', 1)[1]) for s in lines if s.startswith('[lg/transition-memory] ')]
    begins = [r for r in records if r['event'] == 'begin']
    ends = [r for r in records if r['event'] == 'end']
    identity = lambda r: (r['kind'], r['from'], r['to'])
    assert [identity(r) for r in begins] == [identity(r) for r in ends], 'open transition bracket'
    failed = [r for r in ends if r['kind'] not in ['startup-entry', 'quit']]
    assert len(failed) == 1 and not failed[0]['success'], failed
    events = [json.loads(s.split(' ', 1)[1]) for s in lines if s.startswith('TRANSITION_RECOVERY ')]
    if mode != 'reference':
        assert len(events) == (6 if mode == 'retry' else 3)
        assert events[0]['fault_armed'] == 'always'
        assert [e['action'] for e in events] == (['wait', 'wait', 'enter'] * 2 if mode == 'retry' else ['wait', 'wait', 'escape'])
        assert events[2]['fault_armed'] == 'always'
        if mode == 'retry':
            assert events[-1]['fault_armed'] is None
            assert 'outgoing rebuild failed attempt=3' in '\n'.join(lines)
        for attempt in ([2, 3] if mode == 'retry' else [2]):
            image = Image.open(directory / f'screen-{attempt}.png').convert('RGB')
            assert image.width >= 960 and image.height >= 544
            assert len(image.getcolors(image.width * image.height)) > 10, 'screen not drawn'
            # Both toast bands must contain visible glyphs, rather than merely
            # accepting a decorative menu backdrop as proof of the error screen.
            for top in [image.height-110, image.height-70]:
                band = image.crop((0, top-12, image.width, top+28))
                assert sum(count for count,pixel in band.getcolors(band.width*band.height) if max(pixel)>180) > 50
    states = [json.loads(s.split(' ', 1)[1]) for s in lines if s.startswith('TRANSITION_STATE ')]
    frames = [json.loads(s.split(' ', 1)[1]) for s in lines if s.startswith('SIM_MAPS_FRAME ')]
    return dict(directory=directory, states=states, frames=frames, events=events,
                failed=failed[0], save_hashes=before)

rows = []
for on in [False, True]:
    for action in ['chapter', 'natural-exit', 'retry', 'quick-load', 'native-load']:
        reference = run(on, action, 'reference')
        retry = run(on, action, 'retry')
        assert len(retry['states']) == 8 and reference['states'] == retry['states']
        for left, right in zip(reference['frames'], retry['frames']):
            packets = lambda f: [{k: v for k, v in p.items() if k != 'gpu_slot'} for p in f['submitted_packets']]
            assert packets(left) == packets(right)
        assert Image.open(reference['directory'] / 'final.png').tobytes() == Image.open(retry['directory'] / 'final.png').tobytes()
        quit_run = run(on, action, 'quit')
        assert len(quit_run['states']) == 2 and all(s['map'] == 'hedge3' for s in quit_run['states'])
        rows.append(dict(levers=on, path=action, screen_rendered=True, persistent_enter_stays_on_screen=True,
                         enter_after_fault_clear_exact_game_packets_pixels=True, escape_exit_code=0,
                         quit_normal_writable_save_mode=True, save_tree_byte_equal=True,
                         save_hashes=quit_run['save_hashes'], quit_bracket=quit_run['failed'],
                         retry_events=retry['events'], quit_events=quit_run['events']))
        (O / 'proof/PERSISTENT-FAILURE.json').write_text(json.dumps(dict(status='PASS',
            binary_sha256=hashlib.sha256(B.read_bytes()).hexdigest(), rows=rows,
            input_method='host-only one-shot key edges consumed by Input::ui',
            vita_mapping='backend Cross -> Enter; Circle -> Backspace; Input::ui(Escape) accepts Backspace under Vita controls'), indent=2) + '\n')
        print('PASS persistent recovery', on, action, flush=True)
