"""Exercise the production logger through merged OS pipe and file descriptors."""
from pathlib import Path
import os, subprocess, tempfile

ROOT = Path(__file__).resolve().parents[1]
source = '''#[path = "SOURCE"] mod logging;
fn main() {
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let threads = (0..8).map(|writer| {
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            for sequence in 0..500 {
                let stream = if writer % 2 == 0 { logging::Stream::Out } else { logging::Stream::Err };
                logging::line(stream, format_args!("writer={writer} sequence={sequence} payload={} END", "x".repeat(128)));
            }
        })
    }).collect::<Vec<_>>();
    for thread in threads { thread.join().unwrap(); }
}
'''.replace('SOURCE', str(ROOT/'src/logging.rs'))
expected = {f'writer={writer} sequence={sequence} payload={"x"*128} END'
            for writer in range(8) for sequence in range(500)}
def check(data):
    text = data.decode('utf-8'); lines = text.splitlines()
    assert len(lines) == 4000 and set(lines) == expected and text.endswith('\n')
with tempfile.TemporaryDirectory(prefix='alice-atomic-logging-') as temp:
    temp = Path(temp); (temp/'main.rs').write_text(source)
    subprocess.run(['rustc', '--edition=2021', '-O', str(temp/'main.rs'), '-o', str(temp/'logger')],
                   env=dict(os.environ, RUSTUP_TOOLCHAIN='stable'), check=True)
    check(subprocess.check_output([str(temp/'logger')], stderr=subprocess.STDOUT, timeout=30))
    with (temp/'file.log').open('wb') as output:
        subprocess.run([str(temp/'logger')], stdout=output, stderr=subprocess.STDOUT, check=True, timeout=30)
    check((temp/'file.log').read_bytes())
print('PASS: production stdout/stderr logger: 4000 concurrent whole lines through pipe; 4000 through file')
