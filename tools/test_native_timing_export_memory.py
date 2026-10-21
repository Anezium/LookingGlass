#!/usr/bin/env python3
"""Automated HOST-only allocation bound using production modules and locked deps.

Outputs must be new owned paths. Reuses the existing Cargo cache; never removes
artifacts. Cargo's compiler-artifact receipts select dependencies (no newest-file
heuristic). This is requested heap + actual macOS probe-thread stack, not native
RAM, allocator bookkeeping, OS thread runtime, or the application's viewer stack.
"""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import subprocess

ROOT=Path(__file__).resolve().parents[1]

def main():
    p=argparse.ArgumentParser()
    p.add_argument('--output',required=True,type=Path)
    args=p.parse_args();out=args.output.resolve();out.mkdir(parents=True,exist_ok=False)
    # Fail closed when a production static is added without updating the ledger.
    # Standalone probes, host fixtures and cfg(test) modules are not production.
    expected={'mod.rs':['SESSION_CLAIM'],'operations.rs':['ENABLED','EPOCH','WRAPPED'],
        'clock.rs':['ORIGIN'],'audio_producer.rs':['UNIVERSE']}
    actual={}
    for source in (ROOT/'src/native_timing').rglob('*.rs'):
        if source.name in ('tests.rs','export_memory_probe.rs','host_fixture.rs'):continue
        names=re.findall(r'\bstatic\s+(\w+)\s*:',source.read_text())
        if names:actual[str(source.relative_to(ROOT/'src/native_timing'))]=names
    assert actual==expected,('unaccounted production static',actual)
    # The function-local host clock static is private. Verify its declared type
    # matches the size_of reservation, without modifying any clock code.
    clock=(ROOT/'src/native_timing/clock.rs').read_text()
    assert 'static ORIGIN: OnceLock<Instant> = OnceLock::new();' in clock
    assert 'use std::{sync::OnceLock, time::Instant};' in clock
    (out/'static-inventory.json').write_text(json.dumps(dict(production=actual,
        auxiliary=['operations::{ENABLED,EPOCH,WRAPPED}','two existing raw OnceLock slots'],
        already_in_producer_bootstrap=['SESSION_CLAIM','UNIVERSE','clock::ORIGIN (host only; conservative reserve on native)'],
        sizes='Rust size_of_val/size_of constants checked in compiled probe',
        excluded='probe allocator and cfg(test) statics; linker padding and allocator/OS overhead unmeasured'),indent=2)+'\n')
    cargo=Path.home()/'.cargo/bin/cargo';rustc=Path.home()/'.cargo/bin/rustc'
    locked=hashlib.sha256((ROOT/'Cargo.lock').read_bytes()).hexdigest()
    command=[str(cargo),'build','--locked','--offline','--features','native_timing','--message-format=json']
    result=subprocess.run(command,cwd=ROOT,capture_output=True,text=True)
    (out/'cargo.jsonl').write_text(result.stdout);(out/'cargo.log').write_text(result.stderr)
    commands=[dict(argv=command,exit=result.returncode)]
    if result.returncode: raise RuntimeError(result.stderr)
    artifacts={};parents=set()
    for line in result.stdout.splitlines():
        row=json.loads(line)
        if row.get('reason')!='compiler-artifact':continue
        for path in map(Path,row['filenames']):
            parents.add(path.parent)
            if path.suffix=='.rlib':artifacts[row['target']['name']]=path
    for opt in (0,2):
        binary=out/f'export-memory-o{opt}'
        command=[str(rustc),'--edition=2021','--cfg','feature="native_timing"','-C',f'opt-level={opt}',str(ROOT/'src/native_timing/export_memory_probe.rs'),'-o',str(binary)]
        for parent in sorted(parents):command+=['-L','dependency='+str(parent)]
        for name in ('serde','serde_json','anyhow','sha2'):
            lib=artifacts[name];command+=['--extern',name+'='+str(lib)]
            if lib.with_suffix('.rmeta').exists():command+=['--extern',name+'='+str(lib.with_suffix('.rmeta'))]
        result=subprocess.run(command,cwd=ROOT,capture_output=True,text=True)
        (out/f'compile-o{opt}.log').write_text(result.stdout+result.stderr)
        commands.append(dict(argv=command,exit=result.returncode))
        (out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
        if result.returncode:raise RuntimeError(result.stderr)
        command=[str(binary),str(out/f'capture-o{opt}')]
        result=subprocess.run(command,cwd=ROOT,capture_output=True,text=True)
        (out/f'run-o{opt}.log').write_text(result.stdout+result.stderr)
        commands.append(dict(argv=command,exit=result.returncode))
        (out/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
        if result.returncode:raise RuntimeError(result.stdout+result.stderr)
        print(result.stdout,end='')
    assert locked==hashlib.sha256((ROOT/'Cargo.lock').read_bytes()).hexdigest()
    (out/'source.json').write_text(json.dumps(dict(lock_sha256=locked,production_modules={str(f.relative_to(ROOT)):hashlib.sha256(f.read_bytes()).hexdigest() for f in (ROOT/'src/native_timing').rglob('*.rs')},scope='HOST requested allocation bound only'),indent=2)+'\n')
if __name__=='__main__':main()
