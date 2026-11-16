"""Vita release build of this task's worktree, RC14 recipe.

Release mode of tools/vita_release_build.sh with the RC14/9aab7cb flags: the
pinned no-prelude SceShaccCgExt archive, the RC14 target flags and a private target
tree cloned from the last surviving Vita build tree. HEAD is read before the build
and again before the source is handed to the script; the tree must be clean.

No console contact: this produces a local SELF/VPK only.
"""
import hashlib
import json
import os
import select
import shutil
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent
SOURCE = Path('/Users/anezium/vita-work/lg-wchess1')
LOCK = Path('/Users/anezium/vita-j0/mac-timing.lock')
TARGET = ROOT / 'target-vita'
SEED = Path('/Users/anezium/vita-work/lg-lever-matrix-r4/target-vita')
LIB = Path('/Users/anezium/vita-j0/plan-2026-11-12/rc14/lib-no-prelude')
VITASDK = Path('/Users/anezium/vitasdk')
PIN = '0e340441f6909c1169b6f33bcd6e0223ffa91c9d7ce5155f3b27eb1bdd0b38a5'


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def build_once():
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=SOURCE, text=True).strip()
    print('verified HEAD before build', head, flush=True)
    assert not subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=all'],
                                       cwd=SOURCE, text=True), 'the tree must be committed'
    assert shutil.disk_usage(ROOT).free >= 41 * 2**30, 'less than 41 GiB free'
    assert sha256(LIB / 'libSceShaccCgExt.a') == PIN
    out = ROOT / 'builds' / head[:7]
    assert not out.exists() or not any(out.iterdir()), 'output directory not empty'
    if not TARGET.exists():
        TARGET.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(['cp', '-cR', str(SEED), str(TARGET)], check=True)
    (ROOT / 'tmp').mkdir(exist_ok=True)
    capture = ROOT / 'capture-vita'
    if capture.exists():
        shutil.rmtree(capture)
    capture.mkdir()

    assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=SOURCE, text=True).strip() == head
    print('verified HEAD; build does not acquire timing lock', head, flush=True)
    env = dict(
        os.environ,
        VITASDK=str(VITASDK),
        CARGO_TARGET_DIR=str(TARGET),
        CARGO_BUILD_JOBS='4',
        CARGO_NET_OFFLINE='true',
        CARGO_TERM_COLOR='never',
        CARGO_TARGET_ARMV7_SONY_VITA_NEWLIBEABIHF_LINKER=str(
            SOURCE / 'tools/vita_lgpl_link_capture.sh'),
        LG_LINK_CAPTURE=str(capture),
        LG_REAL_LINKER=str(VITASDK / 'bin/arm-vita-eabi-gcc'),
        LG_TARGET_ROOT=str(TARGET),
        LG_VITASDK=str(VITASDK),
        LG_LIB_DIR=str(LIB),
        PATH='/Users/anezium/.cargo/bin:' + str(VITASDK / 'bin') +
             ':/opt/homebrew/bin:/usr/bin:/bin',
        TMPDIR=str(ROOT / 'tmp'),
    )
    subprocess.run(['/opt/homebrew/bin/bash', str(SOURCE / 'tools/vita_release_build.sh'),
                    '--out', str(out), '--lib-dir', str(LIB),
                    '--source-bundle', 'alice-lgpl-wchess1-render.tar.gz'],
                   cwd=SOURCE, env=env, check=True)
    receipt = dict(head=head, branch=subprocess.check_output(
        ['git', 'rev-parse', '--abbrev-ref', 'HEAD'], cwd=SOURCE, text=True).strip(),
        self_path=str(out / 'eboot.bin'), self_sha256=sha256(out / 'eboot.bin'),
        self_bytes=(out / 'eboot.bin').stat().st_size,
        elf_sha256=sha256(out / 'looking-glass.elf'),
        vpk_sha256=sha256(out / 'looking-glass.vpk'),
        lib_dir=str(LIB), lib_sha256=PIN, build_txt=str(out / 'BUILD.txt'))
    (out / 'BUILD-RECEIPT.json').write_text(json.dumps(receipt, indent=1) + '\n')
    print(json.dumps(receipt, indent=1))


def main():
    # Package-local writer exclusion only. This never uses mac-timing.lock.
    lock=ROOT/'native-build.lock'
    deadline=time.monotonic()+1800
    head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=SOURCE,text=True).strip()
    receipt_path=ROOT/'builds'/head[:7]/'BUILD-RECEIPT.json'
    while True:
        if receipt_path.is_file():
            receipt=json.loads(receipt_path.read_text())
            assert receipt['head']==head and sha256(receipt['self_path'])==receipt['self_sha256']
            print('reused completed native build receipt',str(receipt_path),flush=True)
            return
        try:
            fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600)
            os.write(fd,json.dumps(dict(pid=os.getpid(),head=head)).encode());os.close(fd)
            break
        except FileExistsError:
            assert time.monotonic()<deadline,'package-local native build wait exceeded 30 minutes'
            print('waiting for this package native writer; no timing lock',flush=True)
            time.sleep(60)
    try:build_once()
    finally:
        if lock.exists() and json.loads(lock.read_text()).get('pid')==os.getpid():lock.unlink()

if __name__ == '__main__':
    main()
