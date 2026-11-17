"""Host probe builds for the potears1 attribution, from this task's worktree.

The probe recipe is the RC14 one: release, locked, offline,
native_timing,vita_renderer,frame_total_only,sim_maps_probe, RUSTFLAGS
--cfg observer_free_B, into the shared host target directory. The shared target
serves several checkouts, so the build touches the crate root and requires a
fresh artifact record; a fresh-looking artifact is rejected rather than trusted.
"""
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent
SOURCE = Path('/Users/anezium/vita-work/lg-actor-gpu')
TARGET = Path(os.environ.get('WCHESS_HOST_TARGET_DIR', '/Users/anezium/vita-j0/cargo-target-host'))
FEATURES = os.environ.get('WCHESS_HOST_FEATURES', 'native_timing,vita_renderer,frame_total_only,sim_maps_probe')
LABEL = sys.argv[1] if len(sys.argv) > 1 else 'base'
BIN = ROOT / 'bin' / ('lg-host-' + LABEL)
RELEASE = SOURCE / 'configs/PL-release.json'
LOCK = Path('/Users/anezium/vita-j0/mac-timing.lock')


def main():
    (ROOT/'bin').mkdir(exist_ok=True)
    (ROOT/'logs').mkdir(exist_ok=True)
    if shutil.disk_usage('/Users').free < 41 * 1024**3:
        raise SystemExit('disk reserve below 41 GiB')
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=SOURCE, text=True).strip()
    status = subprocess.check_output(['git', 'status', '--porcelain=v1'], cwd=SOURCE, text=True)
    release_sha = hashlib.sha256(RELEASE.read_bytes()).hexdigest()
    # main.rs is always touched: the shared target can otherwise answer a feature
    # set from another checkout, and the binding below must name this source.
    os.utime(SOURCE / 'src/main.rs')
    started = time.time()
    env = {
        'HOME': os.environ['HOME'], 'USER': os.environ.get('USER', ''),
        'PATH': '/Users/anezium/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin',
        'RUSTUP_TOOLCHAIN': 'nightly-2026-10-11-aarch64-apple-darwin',
        'CARGO_TARGET_DIR': str(TARGET), 'CARGO_NET_OFFLINE': 'true',
        'RUSTFLAGS': '--cfg observer_free_B',
    }
    log = ROOT / 'logs' / ('build-host-' + LABEL + '.log')
    with log.open('w') as out:
        result = subprocess.run([
            'cargo', 'build', '--release', '--offline', '--locked',
            '--message-format=json-render-diagnostics',
            '--features', FEATURES,
        ], cwd=SOURCE, env=env, stdout=out, stderr=subprocess.STDOUT)
    if result.returncode != 0:
        raise SystemExit('build failed; see ' + str(log))
    artifacts = []
    for line in log.read_text(errors='replace').splitlines():
        try:
            message = json.loads(line)
        except ValueError:
            continue
        if message.get('reason') == 'compiler-artifact' and \
                message.get('target', {}).get('name') == 'looking-glass':
            artifacts.append(dict(kind=message['target']['kind'], fresh=message.get('fresh'),
                                  executable=message.get('executable')))
    if not artifacts:
        raise SystemExit('no looking-glass artifact record in ' + str(log))
    record = artifacts[-1]
    if record['fresh'] is not False:
        raise SystemExit('cargo served a fresh-looking artifact; the shared target is untrusted here')
    built = Path(record['executable'])
    if built != TARGET / 'release/looking-glass':
        raise SystemExit('unexpected artifact ' + str(built))
    if built.stat().st_mtime < started - 1:
        raise SystemExit('cargo served a binary older than this build')
    shutil.copy2(built, BIN)
    sha = hashlib.sha256(BIN.read_bytes()).hexdigest()
    binding = dict(label=LABEL, head=head, features=FEATURES, dirty=status.strip().splitlines(),
                   release_sha256=release_sha, binary_sha256=sha,
                   bytes=BIN.stat().st_size, fresh=record['fresh'],
                   artifact=str(built), compiler_log=str(log))
    (ROOT / 'logs' / ('build-host-' + LABEL + '-binding.json')).write_text(
        json.dumps(binding, indent=1) + '\n')
    print(json.dumps(binding, indent=1))
    with (ROOT / 'bin' / 'BINARIES.txt').open('a') as handle:
        handle.write('%s head %s sha256 %s\n' % (LABEL, head, sha))


if __name__ == '__main__':
    main()
