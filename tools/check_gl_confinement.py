#!/usr/bin/env python3
"""Compile negative Send/Sync checks against the actual renderer ownership marker."""
from pathlib import Path
import os, subprocess, tempfile
root = Path(__file__).resolve().parent.parent
rustc = os.environ.get('RUSTC', 'rustc')
with tempfile.TemporaryDirectory(prefix='lg-gl-thread-') as directory:
    for trait in ['Send', 'Sync']:
        code = f'''#[path = "{root / 'src/game_thread.rs'}"] mod game_thread;
fn requires<T: {trait}>() {{}}
fn main() {{ requires::<game_thread::GameThread>(); }}
'''
        source = Path(directory) / f'{trait}.rs'
        source.write_text(code)
        result = subprocess.run([rustc, '--edition=2021', str(source), '-o', str(Path(directory) / trait)], capture_output=True, text=True)
        assert result.returncode != 0 and 'E0277' in result.stderr and 'Rc<()>' in result.stderr, result.stderr
        print(f'PASS GameThread cannot implement {trait}')
print('PASS 2 compile-fail confinement checks')
