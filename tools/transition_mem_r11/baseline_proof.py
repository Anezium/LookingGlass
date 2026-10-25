"""Frame-one census and exact-size heap histogram from an isolated source archive."""
from pathlib import Path
import argparse, hashlib, io, json, os, shutil, subprocess, tarfile, tempfile

p = argparse.ArgumentParser()
p.add_argument('output', type=Path)
p.add_argument('--ref', required=True)
p.add_argument('--label', required=True)
a = p.parse_args()
R = Path(__file__).resolve().parents[2]
O = a.output.resolve() / 'baseline' / a.label
O.mkdir(parents=True, exist_ok=True)
env = dict(os.environ, PATH='/Users/anezium/.cargo/bin:/opt/homebrew/bin:' + os.environ['PATH'],
           RUSTUP_TOOLCHAIN='stable', CARGO_TARGET_DIR=str(a.output.resolve() / 'target-diagnostic'),
           RUSTFLAGS='--cfg observer_free_B', CARGO_INCREMENTAL='0', CARGO_NET_OFFLINE='true',
           PYTHONDONTWRITEBYTECODE='1')
with tempfile.TemporaryDirectory(prefix='r11d-baseline-') as temp:
    source = Path(temp)
    archive = subprocess.check_output(['git', 'archive', a.ref], cwd=R)
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        tar.extractall(source, filter='data')
    path = source / 'src/kernel_alloc.rs'
    text = path.read_text()
    marker = '    fn live_alloc(n: usize) {'
    assert marker in text
    text = text.replace(marker, '''    // Fixed storage: histogram observation itself never allocates.
    static SIZE_KEYS: [std::sync::atomic::AtomicUsize; 16384] = [const { std::sync::atomic::AtomicUsize::new(0) }; 16384];
    static SIZE_COUNTS: [std::sync::atomic::AtomicIsize; 16384] = [const { std::sync::atomic::AtomicIsize::new(0) }; 16384];
    fn record_size(n: usize, delta: isize) {
        let mut i = n % SIZE_KEYS.len();
        loop {
            let key = SIZE_KEYS[i].load(Relaxed);
            if key == n || (key == 0 && SIZE_KEYS[i].compare_exchange(0, n, Relaxed, Relaxed).is_ok()) {
                SIZE_COUNTS[i].fetch_add(delta, Relaxed); return;
            }
            i = (i + 1) % SIZE_KEYS.len();
        }
    }
    pub fn live_sizes() -> Vec<(usize, isize)> {
        let snapshot: [(usize, isize); 16384] = std::array::from_fn(|i| (SIZE_KEYS[i].load(Relaxed), SIZE_COUNTS[i].load(Relaxed)));
        let mut result: Vec<_> = snapshot.into_iter().filter(|s| s.1 != 0).collect();
        result.sort_unstable(); result
    }
    fn live_alloc(n: usize) {
        record_size(n, 1);''', 1)
    text = text.replace('    fn live_free(n: usize) {', '    fn live_free(n: usize) { record_size(n, -1);', 1)
    text += '\n#[cfg(not(target_os="vita"))]\npub(crate) use imp::live_sizes;\n'
    path.write_text(text)
    path = source / 'src/viewer.rs'
    text = path.read_text()
    marker = '            use sha2::{Digest,Sha256};\n            let value = game_snapshot!();'
    assert marker in text
    text = text.replace(marker, '''            if frame == 1 {
                let live = crate::kernel_alloc::live_bytes();
                let sizes = crate::kernel_alloc::live_sizes();
                println!("BASELINE_HEAP {}", serde_json::json!({"live": live, "sizes": sizes}));
            }
''' + marker, 1)
    path.write_text(text)
    path = source / 'src/main.rs'
    text = path.read_text()
    marker = '    macroquad::Window::from_config(viewer::config(), async move {'
    assert marker in text
    before, after = text.rsplit(marker, 1)
    text = before + '''    fn future_size<A, B, F>(_: fn(A, B) -> F) -> usize { std::mem::size_of::<F>() }
    eprintln!("BASELINE_RUN_FUTURE_BYTES {}", future_size(viewer::run));
''' + marker + after
    path.write_text(text)
    with (O / 'build.log').open('w') as log:
        subprocess.run(['cargo', 'build', '--release', '--offline', '--locked', '--features',
                        'native_timing,frame_total_only,sim_maps_probe'], cwd=source, env=env,
                       stdout=log, stderr=subprocess.STDOUT, check=True)
    binary = O / 'diagnostic-bin'
    shutil.copy2(Path(env['CARGO_TARGET_DIR']) / 'release/looking-glass', binary)

base = json.loads((R / 'tools/lever_matrix_r4/configs/PL-baseline.json').read_text())
base.update(phase_census=False, profile_capture=False, audio_music_thread=False, story_film_bypass=True)
rows = []
for name in ['hedge3', 'pandemonium']:
    for on in [False, True]:
        run = O / f'{name}-{int(on)}'
        (run / 'private/saves').mkdir(parents=True, exist_ok=True)
        (run / 'private/vita_renderer.json').write_text(json.dumps(dict(base,
            audio_static_cache=on, collision_sah=on, skin_actual_consumers=on)))
        process_env = {k: v for k, v in env.items() if not k.startswith('LOOKING_GLASS_')}
        # An empty plan enables frame-one sampling without scheduling a transition.
        process_env.update(LOOKING_GLASS_TRANSITION_CENSUS='1', LOOKING_GLASS_TRANSITION_PLAN='[]',
                           LOOKING_GLASS_SIM_MAPS_PROBE='1', LOOKING_GLASS_SIM_MAPS_DT='0.05',
                           LOOKING_GLASS_SAH_SILENT_AUDIO='1')
        with (run / 'run.log').open('w') as log:
            subprocess.run([str(binary), '--data', '/Users/anezium/vita-work/alice-data/Alice1/bin/base',
                            '--map', name, '--frames', '3', '--save-dir', str(run / 'private/saves')],
                           cwd=run, env=process_env, stdout=log, stderr=subprocess.STDOUT,
                           check=True, timeout=180)
        lines = (run / 'run.log').read_text().splitlines()
        heap = next(json.loads(s.split(' ', 1)[1]) for s in lines if s.startswith('BASELINE_HEAP '))
        future = next(int(s.split(' ', 1)[1]) for s in lines if s.startswith('BASELINE_RUN_FUTURE_BYTES '))
        assert sum(size * count for size, count in heap['sizes']) == heap['live']
        rows.append(dict(map=name, levers=on, run_future_bytes=future, **heap))
        print(a.label, name, on, 'live', heap['live'], 'future', future, flush=True)
(O / 'BASELINE.json').write_text(json.dumps(dict(status='PASS', revision=a.ref,
    diagnostic_binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), rows=rows), indent=2) + '\n')
