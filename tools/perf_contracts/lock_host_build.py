"""Build one host binary under mac-timing.lock and copy it before releasing the lock.

The receipt records the source revision and a manifest of the tracked build inputs, plus the
sha256 of the copied executable. Another shared-target build cannot replace that file first.
"""
import datetime, hashlib, json, os, subprocess, sys, time
from pathlib import Path

LOCK = Path('/Users/anezium/vita-j0/mac-timing.lock')
INPUTS = ['src', 'Cargo.toml', 'Cargo.lock', 'build.rs', '.cargo', 'configs/PL-def-r3.json']

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def git(repo, *args):
    return subprocess.check_output(['git', '--no-optional-locks', *args], cwd=repo, text=True)

def manifest(repo):
    listing = git(repo, 'ls-tree', '-r', 'HEAD', '--', *INPUTS)
    return hashlib.sha256(listing.encode()).hexdigest(), listing

def main():
    repo = Path(sys.argv[1]).resolve()
    copy_to = Path(sys.argv[2]).resolve()
    log_path = Path(sys.argv[3]).resolve()
    receipt_path = Path(sys.argv[4]).resolve()
    head = git(repo, 'rev-parse', 'HEAD').strip()
    dirty = [line for line in git(repo, 'status', '--porcelain=v1', '--untracked-files=all').splitlines() if line.strip()]
    assert dirty == [' M docs/RCHESS1.md'], dirty
    manifest_sha, _listing = manifest(repo)
    assert os.environ.get('RUSTFLAGS') == '--cfg observer_free_B', os.environ.get('RUSTFLAGS')
    target = os.environ['CARGO_TARGET_DIR']
    cmd = ['cargo', 'build', '--release', '--offline', '--locked', '--features',
           'native_timing,frame_total_only,sim_maps_probe']
    token = dict(pid=os.getpid(), task='alice-r20-deferred-layers-r2', tag='host-bin', head=head,
                 created_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
    waited = 0.0
    while True:
        try:
            fd = os.open(LOCK, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
            break
        except FileExistsError:
            time.sleep(20)
            waited += 20
            print('waiting for mac-timing.lock', waited, flush=True)
    identity = os.fstat(fd)
    os.write(fd, (json.dumps(token) + '\n').encode())
    os.fsync(fd)
    try:
        started = time.monotonic()
        log_path.parent.mkdir(parents=True, exist_ok=True)
        with log_path.open('x', encoding='utf-8') as handle:
            proc = subprocess.run(cmd, cwd=repo, stdout=handle, stderr=subprocess.STDOUT)
        seconds = round(time.monotonic() - started, 1)
        assert proc.returncode == 0, proc.returncode
        produced = Path(target) / 'release/looking-glass'
        assert produced.is_file(), produced
        copy_to.parent.mkdir(parents=True, exist_ok=True)
        copy_to.write_bytes(produced.read_bytes())
        executable_sha = sha(copy_to)
        assert sha(produced) == executable_sha
        assert git(repo, 'rev-parse', 'HEAD').strip() == head
    finally:
        assert LOCK.stat().st_ino == identity.st_ino and json.loads(LOCK.read_text()) == token
        LOCK.unlink()
        os.close(fd)
    toolchain = subprocess.check_output(['rustc', '+nightly-2026-10-11', '--version'], text=True).strip()
    receipt = dict(argv=cmd, cwd=str(repo), source_revision=head, source_manifest_sha256=manifest_sha,
                   executable_path=str(copy_to), executable_sha256=executable_sha,
                   toolchain='nightly-2026-10-11', rustc=toolchain,
                   cargo_lock_sha256=sha(repo / 'Cargo.lock'), cargo_toml_sha256=sha(repo / 'Cargo.toml'),
                   features='native_timing,frame_total_only,sim_maps_probe',
                   rustflags=os.environ.get('RUSTFLAGS', ''),
                   cargo_target_dir=target, exit_code=0, seconds=seconds, lock_wait_seconds=waited,
                   log=str(log_path), log_sha256=sha(log_path))
    receipt_path.parent.mkdir(parents=True, exist_ok=True)
    receipt_path.write_text(json.dumps(receipt, indent=1) + '\n')
    print(json.dumps(dict(head=head, executable_sha256=executable_sha, seconds=seconds), indent=1))

if __name__ == '__main__':
    main()
