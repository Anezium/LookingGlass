#!/usr/bin/env python3
"""Build and verify A-stock on the Mac with the pinned pkg-2 SDK inputs."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import zipfile


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--evidence", required=True, type=Path)
    ap.add_argument("--package-name", default="A-stock-mac")
    args = ap.parse_args()
    root = args.evidence.resolve()
    tree = Path(__file__).resolve().parents[2]
    sdk = Path.home() / "vitasdk"
    assert not (Path.home() / "vita-j0/mac-timing.lock").exists(), "another timing campaign owns the Mac"
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=tree, text=True).strip()
    dirty = subprocess.check_output(["git", "status", "--porcelain"], cwd=tree, text=True).splitlines()
    assert all(line == " M docs/RCHESS1.md" for line in dirty), dirty
    ref = json.loads((root / "A-stock/VERIFICATION.json").read_text())
    inputs = {tree / "Cargo.lock": ref["cargo_lock_sha256"],
              sdk / "arm-vita-eabi/lib/libvitaGL.a": ref["vitagl_library_sha256"],
              sdk / "arm-vita-eabi/include/vitaGL.h": ref["vitagl_header_sha256"]}
    for path, expected in inputs.items():
        assert sha(path) == expected, str(path)
    package = root / args.package_name
    package.mkdir(exist_ok=False)
    target = root / "target-mac-vita"
    env = dict(os.environ, PATH=str(Path.home() / ".cargo/bin") + ":" + str(sdk / "bin") + ":" + os.environ["PATH"],
               VITASDK=str(sdk), CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS="4",
               RUSTUP_TOOLCHAIN="nightly-2026-10-11", RUSTFLAGS="-C target-feature=-neon")
    source = dict(source_commit=revision, host="Mac", cargo_build_jobs=4,
                  target_dir=str(target), toolchain="nightly-2026-10-11",
                  allowed_dirty_file="docs/RCHESS1.md (clone CRLF artifact)", console_access=False)
    (package / "BUILD-SOURCE.json").write_text(json.dumps(source, indent=2) + "\n")

    def run(name, command):
        start = time.monotonic()
        with (package / (name + ".log")).open("w") as log:
            log.write(repr(command) + "\n")
            log.flush()
            status = subprocess.run(command, cwd=tree, env=env, stdout=log, stderr=subprocess.STDOUT).returncode
        assert status == 0, (name, status)
        return dict(duration_seconds=time.monotonic() - start, exit_code=status)

    result = run("build", ["cargo", "vita", "build", "vpk", "--release", "--locked", "--features", "vita_renderer"])
    effective = "-C target-feature=-neon --cfg mio_unsupported_force_poll_poll --cfg mio_unsupported_force_waker_pipe"
    assert effective in (package / "build.log").read_text(), "cargo-vita polling cfg proof missing"
    release = target / "armv7-sony-vita-newlibeabihf/release"
    for suffix in ("elf", "self", "vpk", "velf", "sfo"):
        shutil.copy2(release / ("looking-glass." + suffix), package / ("looking-glass." + suffix))
    elf = package / "looking-glass.elf"
    run("elf-margin", ["python3", str(root / "verify-elf-margin.py"), str(elf)])
    spec = importlib.util.spec_from_file_location("margin", root / "verify-elf-margin.py")
    margin = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(margin)
    gap = margin.verify(elf)
    run("imports", [str(sdk / "bin/vita-elf-create"), "-vvv", str(elf), str(package / "looking-glass.verified.velf")])
    assert "unresolved" not in (package / "imports.log").read_text().lower()
    assert sha(package / "looking-glass.velf") == sha(package / "looking-glass.verified.velf")
    with zipfile.ZipFile(package / "looking-glass.vpk") as archive:
        assert archive.testzip() is None
        assert set(archive.namelist()) == {"eboot.bin", "sce_sys/param.sfo"}
        assert archive.read("eboot.bin") == (package / "looking-glass.self").read_bytes()
        assert archive.read("sce_sys/param.sfo") == (package / "looking-glass.sfo").read_bytes()
        entries = {name: hashlib.sha256(archive.read(name)).hexdigest() for name in archive.namelist()}
    assert subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=tree, text=True).strip() == revision
    assert not subprocess.check_output(["git", "diff", "HEAD", "--", "src", "Cargo.lock", "Cargo.toml", "build.rs", ".cargo"], cwd=tree)
    for path, expected in inputs.items():
        assert sha(path) == expected, str(path)
    hashes = {p.name: sha(p) for p in package.glob("looking-glass.*")}
    result.update(source, variant="A-stock", effective_rustflags=effective, elf_margin_bytes=gap,
                  minimum_margin_bytes=3348, imports_pass=True, regenerated_velf_equal=True,
                  zip_crc_pass=True, packaged_self_sfo_equal=True, package_entries=entries,
                  hashes=hashes, cargo_lock_sha256=ref["cargo_lock_sha256"],
                  vitagl_library_sha256=ref["vitagl_library_sha256"], vitagl_header_sha256=ref["vitagl_header_sha256"])
    (package / "VERIFICATION.json").write_text(json.dumps(result, indent=2) + "\n")
    (package / "SHA256SUMS.txt").write_text("".join(f"{digest}  {name}\n" for name, digest in sorted(hashes.items())))
    print("PASS Mac A-stock margin/imports/package; VPK", hashes["looking-glass.vpk"], flush=True)


if __name__ == "__main__":
    main()
