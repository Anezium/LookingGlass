#!/usr/bin/env python3
"""Exercise the actual Vita observer bodies with deterministic host ABI adapters.

Run with the project's declared Python and RUSTUP_TOOLCHAIN; pass a new output
directory. This does not emulate ARM, libc, or unwinding across an extern C ABI.
The unchanged full native ELF validator remains the code-generation regression.
"""
from pathlib import Path
import argparse, hashlib, json, os, subprocess

R = Path(__file__).resolve().parents[1]

def block(text, start):
    begin = text.index(start)
    opening = text.index('{', begin)
    depth = 1
    end = opening + 1
    # Selected bodies contain no brace-bearing string literals or comments.
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    return text[begin:end]

def native(text):
    text = text.replace('#[cfg(target_os="vita")]', '').replace('#[cfg(target_os = "vita")]', '')
    host = '#[cfg(not(target_os="vita"))]'
    host_spaced = '#[cfg(not(target_os = "vita"))]'
    for marker in [host, host_spaced]:
        if marker in text:
            tail = text.index(marker)
            rest = text[tail+len(marker):].lstrip()
            body = block(rest, '{') if rest.startswith('{') else rest[:rest.index(';')+1]
            text = text[:tail] + rest[len(body):]
    assert '#[cfg(' not in text, text
    return text

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    a.output.mkdir(parents=True, exist_ok=False)
    sources = {n: (R/'src'/n).read_text(encoding='utf-8') for n in
               ['kernel_alloc.rs', 'allocation_census.rs', 'transition_memory.rs']}
    k = sources['kernel_alloc.rs']
    k = block(k[k.index('#[cfg(target_os = "vita")]'):], 'mod imp')
    k = k.replace('mod imp {', 'mod kernel_alloc {', 1)
    c = sources['allocation_census.rs']
    owner = block(c, 'mod owner_lock')
    peak = block(c, 'pub(crate) mod peak')
    peak = peak[:-1] + '''
    pub fn reset_for_test() {
        OWNER.release(); BASE.store(0,Relaxed); GROWTH.store(0,Relaxed);
        CALLS.store(0,Relaxed); LOWER.store(0,Relaxed); UPPER.store(0,Relaxed);
        FALLBACKS.store(0,Relaxed);
    }
    pub fn state_for_test()->[usize;4] {
        [BASE.load(Relaxed),GROWTH.load(Relaxed),CALLS.load(Relaxed),UPPER.load(Relaxed)]
    }
}'''
    census = 'mod allocation_census {\nuse std::sync::atomic::Ordering::Relaxed;\n'
    census += 'pub fn enabled()->bool { crate::ENABLED.load(Relaxed) }\n'
    census += native(block(c, 'pub(crate) fn used()')) + '\n' + owner + '\n' + peak + '\n}'
    t = sources['transition_memory.rs']
    transition = '''mod transition_memory {
use std::sync::atomic::{AtomicBool,AtomicUsize,Ordering::Relaxed};
static ACTIVE:AtomicBool=AtomicBool::new(false);
static PEAK:AtomicUsize=AtomicUsize::new(0);
static PEAK_ARENA:AtomicUsize=AtomicUsize::new(0);
static ALLOCATIONS:AtomicUsize=AtomicUsize::new(0);
'''
    transition += block(t, 'fn heap_sample()') + '\n'
    transition += native(block(t, 'pub(crate) fn observe()')) + '\n'
    transition += block(t, 'pub(crate) fn allocation(size: usize)') + '\n'
    transition += '''pub fn reset_for_test(active:bool) {
ACTIVE.store(active,Relaxed);PEAK.store(0,Relaxed);PEAK_ARENA.store(0,Relaxed);ALLOCATIONS.store(0,Relaxed);
}
pub fn state_for_test()->[usize;3] {[PEAK.load(Relaxed),PEAK_ARENA.load(Relaxed),ALLOCATIONS.load(Relaxed)]}
}'''
    template = (R/'tests/native_allocator_observer.rs.in').read_text(encoding='utf-8')
    harness = a.output/'native_allocator_observer.rs'
    harness.write_text(k+'\n'+census+'\n'+transition+'\n'+template, encoding='utf-8')
    env = dict(os.environ, CARGO_NET_OFFLINE='true')
    command = ['rustc', '--edition=2021', '--test', str(harness), '-o', str(a.output/'tests')]
    runs = []
    for tag, cmd in [('compile', command), ('run', [str(a.output/'tests'), '--test-threads=1', '--nocapture'])]:
        with (a.output/(tag+'.log')).open('x', encoding='utf-8') as log:
            r = subprocess.run(cmd, env=env, stdout=log, stderr=subprocess.STDOUT)
        runs.append(dict(argv=cmd, exit_code=r.returncode))
        if r.returncode: break
    receipt = dict(status='PASS' if all(r['exit_code']==0 for r in runs) and len(runs)==2 else 'FAIL',
        sources={n:hashlib.sha256((R/'src'/n).read_bytes()).hexdigest() for n in sources},
        harness_sha256=hashlib.sha256(harness.read_bytes()).hexdigest(), runs=runs,
        adapters='Host thread ID, matching payloads, errno sentinel, and _mallinfo_r return; native observer bodies retained. Private-state reset/read access added only to generated test module.',
        limitations='Host usize width, mock payload/OS, no real libc or ARM unwind; full ELF gate required separately.')
    (a.output/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(receipt,indent=2))
    return 0 if receipt['status']=='PASS' else 1

if __name__ == '__main__': raise SystemExit(main())
