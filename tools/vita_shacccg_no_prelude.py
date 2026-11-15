#!/usr/bin/env python3
"""Export pinned upstream source without the optional intrinsic prelude.

The patch intentionally contains no copy of the removed header. Modified
2026-11-15: omit its load and CMake entry; delete the header from exported source.
"""
import argparse
from pathlib import Path
import subprocess

REVISION = 'd3c83efceedeca3be863d0c1f19bb145daf1685f'
PATCH = Path(__file__).resolve().parent / 'patches/SceShaccCgExt-no-prelude.patch'

def apply(destination):
    subprocess.run(['patch', '--batch', '-p1', '-i', str(PATCH)], cwd=destination, check=True)
    (destination / 'include/sce_intrinsics.h').unlink(missing_ok=True)

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--source', type=Path)
    p.add_argument('--out', type=Path)
    p.add_argument('--apply', type=Path)
    a = p.parse_args()
    if a.apply:
        apply(a.apply)
        return
    if not a.source or not a.out:
        p.error('--source and --out are required')
    head = subprocess.check_output(['git', '-C', str(a.source), 'rev-parse', 'HEAD'], text=True).strip()
    if head != REVISION:
        p.error('source revision differs from pin')
    if a.out.exists():
        p.error('output must not exist')
    # Read only tracked files from the pin; never copy .git, SDKs or build trees.
    names = subprocess.check_output(['git', '-C', str(a.source), 'ls-tree', '-r', '--name-only', REVISION], text=True).splitlines()
    for name in names:
        if name == 'include/sce_intrinsics.h':
            continue
        target = a.out / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(subprocess.check_output(['git', '-C', str(a.source), 'show', REVISION + ':' + name]))
    apply(a.out)

if __name__ == '__main__':
    main()
