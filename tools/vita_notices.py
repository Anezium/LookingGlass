#!/usr/bin/env python3
"""Write THIRD_PARTY_NOTICES-vita.txt for the PS Vita build.

Sources, in the order they appear in the file:
  1. the modified forks (macroquad, miniquad, cpal) with the files changed from upstream,
     as the Apache-2.0 change notice;
  2. the C libraries linked statically, with the licence texts kept in tools/vita_licenses/;
  3. every Rust package `cargo metadata` resolves for armv7-sony-vita-newlibeabihf with the
     release features, with the licence files shipped in each package;
  4. the Rust standard library and its dependencies, built from source by cargo-vita.

With --link-map, every archive in the linker map must belong to a known C library, so a new
library cannot reach the binary without a notice. No path of the build machine is written.

Usage: tools/vita_notices.py [--link-map FILE] [--output FILE]
"""
import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

TARGET = "armv7-sony-vita-newlibeabihf"
TOOLCHAIN = "nightly-2026-10-11"
FEATURES = "native_timing,vita_renderer,frame_total_only"
BUILD_STD = ["std", "panic_unwind"]
LICENCE_NAME = re.compile(r"^(licen[cs]e|copying|notice|unlicense|copyright)", re.I)

# Fork, upstream base commit and version, from each fork's history.
FORKS = {
    "macroquad": ("not-fl3/macroquad", "2c60f75", "0.4.14"),
    "miniquad": ("not-fl3/miniquad", "cc6f8d1", "0.4.8"),
    "cpal": ("RustAudio/cpal", "7b21251", "0.15.3"),
}

# Licence texts for packages that ship none, taken from their repositories (see vita_licenses/SOURCES.txt).
EXTRA_TEXTS = {
    "symphonia": ["symphonia-LICENSE"],
    "symphonia-bundle-mp3": ["symphonia-LICENSE"],
    "symphonia-core": ["symphonia-LICENSE"],
    "symphonia-metadata": ["symphonia-LICENSE"],
    "dasp_sample": ["dasp-LICENSE-MIT", "dasp-LICENSE-APACHE"],
}

# C libraries in the link. Archives match the linker map by file name.
C_LIBRARIES = [
    {
        "name": "vitaGL",
        "archives": ["libvitaGL.a"],
        "licence": "LGPL-3.0",
        "source": "https://github.com/Rinnegatamante/vitaGL at dca4b9d, built with "
                  "make NO_TEX_COMBINER=1 HAVE_UNFLIPPED_FBOS=1 SINGLE_THREADED_GC=1 HAVE_CUSTOM_HEAP=1",
        "files": ["vitaGL-COPYING.LESSER", "vitaGL-COPYING"],
    },
    {
        "name": "vitaShaRK",
        "archives": ["libvitashark.a"],
        "licence": "LGPL-3.0",
        "source": "https://github.com/Rinnegatamante/vitaShaRK tag v.1.7 (df24065), vdpm package vitaShaRK 1.7-1",
        "files": ["vitaShaRK-LICENSE"],
    },
    {
        "name": "SceShaccCgExt",
        "archives": ["libSceShaccCgExt.a"],
        "licence": "LGPL-2.1",
        "source": "https://github.com/bythos14/SceShaccCgExt at d3c83ef, the first revision under LGPL-2.1",
        "files": ["SceShaccCgExt-LICENSE"],
    },
    {
        "name": "math-neon",
        "archives": ["libmathneon.a"],
        "licence": "MIT",
        "source": "https://github.com/Rinnegatamante/math-neon at 0faab81, vdpm package libmathneon 0.0.0.r11.g0faab81-1. "
                  "The repository has no licence file; the text below is the header of source/math_neon.h",
        "files": ["math-neon-LICENSE"],
    },
    {
        "name": "taiHEN user stubs",
        "archives": ["libtaihen_stub.a"],
        "licence": "MIT",
        "source": "https://github.com/yifanlu/taiHEN release v0.11, vdpm package taihen 0.11-1",
        "files": ["taiHEN-LICENSE"],
    },
    {
        "name": "vita-headers (system module import stubs)",
        "archives": ["*_stub.a"],
        "licence": "MIT",
        "source": "https://github.com/vitasdk/vita-headers at ebc8f4f (VitaSDK build of 2026-08-25)",
        "files": ["vita-headers-LICENSE.md"],
    },
    {
        "name": "newlib (VitaSDK fork)",
        "archives": ["libc.a", "libm.a", "libg.a", "crt0.o"],
        "licence": "several permissive licences, listed per source file in COPYING.NEWLIB and COPYING.LIBGLOSS",
        "source": "https://github.com/vitasdk/newlib at 64aa7aa (VitaSDK build of 2026-08-25). "
                  "The PS Vita glue in newlib/libc/sys/vita (crt0, sbrk, threading, file system) carries no "
                  "licence header and is not listed in COPYING.NEWLIB",
        "files": ["newlib-COPYING.NEWLIB", "newlib-COPYING.LIBGLOSS"],
    },
    {
        "name": "pthread-embedded (VitaSDK fork)",
        "archives": ["libpthread.a"],
        "licence": "LGPL-2.1; the PS Vita platform layer is MIT",
        "source": "https://github.com/vitasdk/pthread-embedded at 11d2e57 (VitaSDK build of 2026-08-25)",
        "files": ["pthread-embedded-COPYING", "pthread-embedded-COPYING.LIB", "pthread-embedded-COPYING.vita"],
    },
    {
        "name": "GCC runtime libraries (libgcc, libstdc++, libsupc++)",
        "archives": ["libgcc.a", "libstdc++.a", "libsupc++.a", "libgcc_eh.a", "crtbegin.o", "crtend.o", "crti.o", "crtn.o"],
        "licence": "GPL-3.0 with the GCC Runtime Library Exception 3.1",
        "source": "arm-vita-eabi-gcc 15.2.0 from vita-toolchain c527abc (VitaSDK build of 2026-08-25). "
                  "The exception allows these libraries in a program under any licence",
        "files": ["gcc-COPYING.RUNTIME", "vitaGL-COPYING"],
    },
]


def run(*args, cwd=None):
    return subprocess.run(args, cwd=cwd, check=True, capture_output=True, text=True).stdout


def metadata(workspace=None):
    """cargo metadata for the program, or for the standard library workspace. The latter runs
    inside that workspace so its own cargo configuration (vendored sources) applies."""
    command = ["rustup", "run", TOOLCHAIN, "cargo", "metadata", "--format-version", "1", "--locked", "--offline",
               "--filter-platform", TARGET]
    if workspace is None:
        command += ["--features", FEATURES]
    return json.loads(run(*command, cwd=workspace))


def reachable(meta, roots):
    """Packages reachable from the roots through normal and build edges, and whether any
    path reaches them as linked code (normal edges from a linked package, no proc-macro)."""
    nodes = {node["id"]: node for node in meta["resolve"]["nodes"]}
    packages = {pkg["id"]: pkg for pkg in meta["packages"]}
    proc_macro = {pid for pid, pkg in packages.items()
                  if any("proc-macro" in target["kind"] for target in pkg["targets"])}
    linked, seen = set(), set()
    stack = [(root, True) for root in roots]
    while stack:
        pid, is_linked = stack.pop()
        is_linked = is_linked and pid not in proc_macro
        if (pid, is_linked) in seen:
            continue
        seen.add((pid, is_linked))
        if is_linked:
            linked.add(pid)
        for dep in nodes[pid]["deps"]:
            kinds = {kind["kind"] for kind in dep["dep_kinds"]}
            if None in kinds:
                stack.append((dep["pkg"], is_linked))
            if "build" in kinds:
                stack.append((dep["pkg"], False))
    every = {pid for pid, _ in seen}
    return [packages[pid] for pid in every], linked


def licence_files(package):
    directory = Path(package["manifest_path"]).parent
    names = sorted(p.name for p in directory.iterdir() if p.is_file() and LICENCE_NAME.match(p.name))
    if package.get("license_file"):
        extra = Path(package["license_file"]).name
        if (directory / extra).is_file() and extra not in names:
            names.append(extra)
    return directory, names


def source_of(package):
    source = package.get("source") or ""
    if source.startswith("git+"):
        return source[4:].split("#")[0]
    return package.get("repository") or "https://crates.io/crates/" + package["name"]


def package_block(package, linked, out, texts):
    key = f"{package['name']}-{package['version']}"
    out.append(key)
    out.append(f"Declared license: {package.get('license') or 'none declared'}")
    out.append(f"Source: {source_of(package)}")
    if not linked:
        out.append("Used at build time only (build script or procedural macro); not linked into the program.")
    directory, names = licence_files(package)
    extra = EXTRA_TEXTS.get(package["name"], []) if not names else []
    if extra:
        out.append("The package ships no licence file; the text below comes from its source repository.")
    elif not names:
        out.append("The package ships no licence file and its repository has none. The standard text of its "
                   "declared licence is in the licence files of other packages listed here.")
    out.append("")
    for name in names:
        out.append(f"--- licenses/{key}/{name} ---")
        out.append((directory / name).read_text(encoding="utf-8", errors="replace").rstrip("\n"))
        out.append("")
    for name in extra:
        out.append(f"--- licenses/{name} ---")
        out.append((texts / name).read_text(encoding="utf-8").rstrip("\n"))
        out.append("")
    out.append("")


def fork_block(package, out):
    upstream, base, version = FORKS[package["name"]]
    directory = Path(package["manifest_path"]).parent
    revision = run("git", "rev-parse", "--short=7", "HEAD", cwd=directory).strip()
    changes = run("git", "diff", "--name-status", "--no-renames", base, "HEAD", cwd=directory).splitlines()
    out.append(f"{package['name']} {package['version']}: fork {source_of(package)} at {revision}")
    out.append(f"Based on {upstream} {version} (commit {base}). Declared license: {package.get('license')}.")
    out.append("The PS Vita port changed the files below (A added, M modified, D deleted). Each changed "
               "file is in the fork at the revision above; its history records every change.")
    out.extend("  " + line.replace("\t", " ") for line in changes)
    out.append("")


def archives_in_map(path):
    text = Path(path).read_text(encoding="utf-8", errors="replace")
    found = set(re.findall(r"([A-Za-z0-9_+.\-]+\.a)\(", text))
    found |= set(re.findall(r"/(crt[a-z0-9]+\.o)\b", text))
    return found


def covered(archive):
    for library in C_LIBRARIES:
        for pattern in library["archives"]:
            if pattern.startswith("*") and archive.endswith(pattern[1:]) or archive == pattern:
                return True
    # Rust rlibs and the program's own objects are covered by the Rust sections.
    return archive.endswith(".rlib") or archive.startswith("liblooking_glass")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--link-map")
    parser.add_argument("--output", default="THIRD_PARTY_NOTICES-vita.txt")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    texts = repo / "tools" / "vita_licenses"
    os.chdir(repo)

    linked_archives = set()
    if args.link_map:
        linked_archives = archives_in_map(args.link_map)
        missing = sorted(a for a in linked_archives if not covered(a))
        if missing:
            print("archives in the link map without a notice entry: " + ", ".join(missing), file=sys.stderr)
            return 1

    meta = metadata()
    root = meta["resolve"]["root"]
    packages, linked = reachable(meta, [root])
    packages = sorted((p for p in packages if p["id"] != root), key=lambda p: (p["name"], p["version"]))

    sysroot = Path(run("rustup", "run", TOOLCHAIN, "rustc", "--print", "sysroot").strip())
    library = sysroot / "lib" / "rustlib" / "src" / "rust" / "library"
    std_meta = metadata(library)
    std_roots = [p["id"] for p in std_meta["packages"] if p["name"] in BUILD_STD and p.get("source") is None]
    std_packages, std_linked = reachable(std_meta, std_roots)
    std_packages = sorted(std_packages, key=lambda p: (p["name"], p["version"]))
    rustc = run("rustup", "run", TOOLCHAIN, "rustc", "--version").strip()

    out = [
        "Third-party notices for the Looking Glass PS Vita build",
        "",
        "Generated by tools/vita_notices.py. Original game data is not included. Sony's shader compiler",
        "(libshacccg.suprx) is loaded at run time from the user's console and is not distributed.",
        "",
        "Looking Glass itself is under LICENSE-MIT; see LEGAL.md. The C libraries under the GNU LGPL are",
        "linked statically. The complete source of this build, the revisions of every library below and the",
        "build script (tools/vita_release_build.sh) are published with each release, so the program can be",
        "relinked against modified versions of those libraries.",
        "",
        "",
        "=== 1. Modified forks (Apache-2.0 section 4(b) change notices) ===",
        "",
    ]
    for package in packages:
        if package["name"] in FORKS and (package.get("source") or "").startswith("git+"):
            fork_block(package, out)
    out += ["", "=== 2. C libraries linked statically ===", ""]
    for entry in C_LIBRARIES:
        out.append(entry["name"])
        out.append(f"License: {entry['licence']}")
        out.append(f"Source: {entry['source']}")
        if linked_archives:
            exact = {a for e in C_LIBRARIES for a in e["archives"] if not a.startswith("*")}
            used = sorted(a for a in linked_archives if a in entry["archives"] or a not in exact and any(
                p.startswith("*") and a.endswith(p[1:]) for p in entry["archives"]))
            out.append(f"Archives in the link: {', '.join(used) or 'none'}")
        else:
            out.append(f"Archives: {', '.join(entry['archives'])}")
        out.append("")
        for name in entry["files"]:
            out.append(f"--- licenses/{name} ---")
            out.append((texts / name).read_text(encoding="utf-8").rstrip("\n"))
            out.append("")
        out.append("")
    out += ["", f"=== 3. Rust packages for {TARGET} (features {FEATURES}) ===", ""]
    for package in packages:
        package_block(package, package["id"] in linked, out, texts)
    out += ["", f"=== 4. Rust standard library ({rustc}), built from source with build-std ===", ""]
    for package in std_packages:
        if package.get("source") is None:
            out.append(f"{package['name']} (part of the Rust distribution)")
            out.append(f"Declared license: {package.get('license')}")
            out.append("")
            continue
        package_block(package, package["id"] in std_linked, out, texts)
    out += ["--- licenses/rust/Apache-2.0.txt ---",
            (sysroot / "share/doc/rust/licenses/Apache-2.0.txt").read_text(encoding="utf-8").rstrip("\n"), "",
            "--- licenses/rust/MIT.txt ---",
            (sysroot / "share/doc/rust/licenses/MIT.txt").read_text(encoding="utf-8").rstrip("\n"), "",
            "--- licenses/rust/LLVM-exception.txt ---",
            (sysroot / "share/doc/rust/licenses/LLVM-exception.txt").read_text(encoding="utf-8").rstrip("\n"), ""]

    text = "\n".join(out) + "\n"
    for private in {str(Path.home()), str(repo)}:
        if private in text:
            print("refusing to write: a build machine path reached the notices", file=sys.stderr)
            return 1
    Path(args.output).write_text(text, encoding="utf-8")
    print(f"{args.output}: {len(packages)} Rust packages, {len(std_packages)} standard library packages, "
          f"{len(C_LIBRARIES)} C library entries, {text.count(chr(10))} lines")
    return 0


if __name__ == "__main__":
    sys.exit(main())
