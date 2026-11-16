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

--credits-output writes the short CREDITS.txt that the VPK carries at its root.

Usage: tools/vita_notices.py [--link-map FILE] [--output FILE] [--credits-output FILE]
"""
import argparse
import json
import os
import re
import subprocess
import sys
import textwrap
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

# Import records compiled into a fork rather than read from a separate archive.
# cpal's Vita host declares #[link(name = "SceAudio_stub")]; rustc bundles that
# static archive into libcpal-*.rlib, so the release link map shows these four
# members inside the rlib and no libSceAudio_stub.a archive of its own.
IMPORT_RECORDS = {
    "cpal": ("SceAudio", "src/host/vita/mod.rs",
             ["sceAudioOutOpenPort", "sceAudioOutOutput",
              "sceAudioOutReleasePort", "sceAudioOutSetVolume"]),
}

# Packages that ship no licence file: say exactly what is missing.
NO_LICENCE_NOTES = {
    "quad-rand": "quad-rand 0.2.3 ships no licence file and no copyright line. Its "
                 "repository (https://github.com/not-fl3/quad-rand) has no licence "
                 "file and no copyright line either; Cargo.toml declares MIT. The "
                 "standard MIT text is in the licence files of other packages "
                 "listed here.",
}

# libc.a members compiled from newlib/libc/sys/vita that the release link map
# loads. crt0.o is a separate object. Kept explicit so the notice names the
# measured objects; the map is checked when it is supplied.
VITA_GLUE_MEMBERS = [
    "closedir.o", "opendir.o", "readdir.o",
    "lib_a-clock.o", "lib_a-error.o", "lib_a-fs.o", "lib_a-getentropy.o",
    "lib_a-io.o", "lib_a-lcltime_r.o", "lib_a-mlock.o", "lib_a-sbrk.o",
    "lib_a-syscalls.o", "lib_a-sysconf.o", "lib_a-threading.o",
]

# Credits for the linked files that state no licence. Measured in the public
# history of https://github.com/vitasdk/newlib at 64aa7aa, the revision this
# release links: the commit that introduced each file and the authors git blame
# attributes its lines to at that revision. The owner decided on 2026-11-15 that
# no permission request would be posted for these files and that the release
# ships with the gap documented and the authors credited (LEGAL.md). These names
# are attribution only. They are not a licence and they do not claim one.
NEWLIB_GLUE_CREDITS = [
    ("crt0.c", "Ilya Zhuravlev (0681b290b, 2015-08-13)",
     "Francisco José García García, Ilya Zhuravlev, Sergi Granell, Ivan Epifanov, "
     "Glenn Anderson, Yifan Lu, Rémy F"),
    ("io.c", "Yifan Lu (95b2dc6df, 2016-06-23)",
     "Davee, Ivan Epifanov, Yifan Lu, Vladimir Atamanenko, Nikolay Arhipov"),
    ("mlock.c", "Ilya Zhuravlev (5bf33fd61, 2015-08-14)",
     "Ivan Epifanov, Francisco José García García, Ilya Zhuravlev, Sergi Granell"),
    ("sbrk.c", "Ilya Zhuravlev (c7ae9b785, 2015-08-13)",
     "Ilya Zhuravlev, Ivan Epifanov, Francisco José García García, Glenn Anderson, "
     "Sergi Granell, Rinnegatamante"),
    ("syscalls.c", "Ilya Zhuravlev (0681b290b, 2015-08-13)",
     "Ilya Zhuravlev, Ivan Epifanov, Davee, Jeffrey Pfau, frangarcj, Nikolay Arhipov, "
     "Vladimir Atamanenko, Yifan Lu, Sunguk Lee, Glenn Anderson, "
     "Francisco José García García"),
    ("threading.c", "Ilya Zhuravlev (0875ca1dd, 2015-08-14)",
     "Davee, Ilya Zhuravlev, Ivan Epifanov, Francisco José García García, "
     "Sergi Granell, TheOfficialFloW"),
]
NEWLIB_GLUE_CREDIT_NOTE = ('Ivan Epifanov also appears in the history as "Epifanov Ivan".')
NEWLIB_REVISION = "64aa7aa33d4f380451a1f100d19589226cdad334"
RBTREE_CREDIT = {
    "library": "SceShaccCgExt",
    "revision": "d3c83efceedeca3be863d0c1f19bb145daf1685f",
    "file": "src/shacccg_ext_oop.cpp",
    "url": "https://github.com/Dynmi/RedBlackTree",
    "author": "Haris Wang",
    "handle": "Dynmi",
    "upstream": "533e5d3ca8368682898f89a5574a96d0df8ec265",
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
        "source": "https://github.com/bythos14/SceShaccCgExt at d3c83ef, LGPL-2.1; modified 2026-11-15 to remove the optional intrinsic prelude and its load (tools/patches/SceShaccCgExt-no-prelude.patch)",
        "notes": ["Sony intrinsics prelude: resolved. The owner approved shipping SceShaccCgExt "
                  "without the optional intrinsic prelude on 2026-11-15. The upstream "
                  "include/sce_intrinsics.h is not built, linked, packaged or offered, and the "
                  "library no longer loads it. The console check of that date recorded 45 of 45 "
                  "shader-cache payloads byte-identical with and without it; the check is "
                  "retained in the release evidence, and the public project source is "
                  "https://github.com/Anezium/LookingGlass.",
                  "Red-black tree: " + RBTREE_CREDIT["file"] + " contains a red-black tree copied "
                  "from " + RBTREE_CREDIT["url"] + ", whose rbtree.h names its author as "
                  + RBTREE_CREDIT["author"] + " (GitHub: " + RBTREE_CREDIT["handle"] + "). The "
                  "upstream repository has no licence file and its files state no licence. The "
                  "author and repository are credited in the CREDITS section and in CREDITS.txt; "
                  "no licence is claimed for that code."],
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
        "licence": "the newlib core files carry the per-file licences listed in COPYING.NEWLIB and COPYING.LIBGLOSS; the Vita glue is partly MIT-noticed and partly unnotified, as stated in newlib-vita-glue-NOTICE.txt",
        "source": "https://github.com/vitasdk/newlib at 64aa7aa (VitaSDK build of 2026-08-25). "
                  "The PS Vita glue in newlib/libc/sys/vita is a fork addition, not upstream newlib. "
                  "Of the files whose objects this link loads, dirent.c, clock.c, error.c, fs.c, "
                  "getentropy.c, lcltime_r.c and sysconf.c carry an MIT-style notice (copyright "
                  "David \"Davee\" Morgan, Sunguk Lee, Martin Larralde and vitasdk); io.c, mlock.c, "
                  "sbrk.c, syscalls.c and threading.c carry no copyright or licence line; crt0.c "
                  "attributes code to PSPSDK (Copyright (c) 2005 Marcus R. Brown) without a licence "
                  "text. None of these files is named in COPYING.NEWLIB. Its preamble applies the "
                  "notices it lists to files in the newlib subdirectory unless a file states "
                  "otherwise; whether that covers the unnotified files is not established here",
        "notes": ["Credits: the authors of the linked files that state no licence are named in "
                  "the CREDITS section and in CREDITS.txt. That credit is an attribution "
                  "recorded after the owner decision of 2026-11-15; it is not a licence grant."],
        "files": ["newlib-COPYING.NEWLIB", "newlib-COPYING.LIBGLOSS",
                  "newlib-vita-glue-NOTICE.txt"],
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
        "licence": "GPL-3.0-or-later with the GCC Runtime Library Exception 3.1",
        "source": "arm-vita-eabi-gcc 15.2.0 from vita-toolchain c527abc (VitaSDK build of 2026-08-25). "
                  "The exception allows these libraries in a program under any licence",
        "files": ["GCC-RUNTIME-EXCEPTION-3.1.txt", "GPL-3.0.txt"],
    },
]

MPL_CRATES = ("symphonia", "symphonia-bundle-mp3", "symphonia-core", "symphonia-metadata")


def mpl_checksums(repo):
    """Return {crate: (version, checksum)} for the MPL-2.0 crates in Cargo.lock."""
    text = (Path(repo) / "Cargo.lock").read_text(encoding="utf-8")
    found = {}
    for block in text.split("[[package]]"):
        name = re.search(r'^name = "([^"]+)"$', block, re.M)
        version = re.search(r'^version = "([^"]+)"$', block, re.M)
        checksum = re.search(r'^checksum = "([0-9a-f]{64})"$', block, re.M)
        if name and name.group(1) in MPL_CRATES and version and checksum:
            found[name.group(1)] = (version.group(1), checksum.group(1))
    missing = [name for name in MPL_CRATES if name not in found]
    if missing:
        raise ValueError("Cargo.lock does not pin " + ", ".join(missing))
    return found


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
    if package["name"] in NO_LICENCE_NOTES:
        out.append(NO_LICENCE_NOTES[package["name"]])
    elif extra:
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
    if package["name"] in IMPORT_RECORDS:
        module, where, symbols = IMPORT_RECORDS[package["name"]]
        out.append("The fork also declares four %s import records in %s: %s."
                   % (module, where, ", ".join(symbols)))
        out.append("rustc bundles the VitaSDK %s_stub archive into the cpal rlib; the release link "
                   "map shows these four import objects inside libcpal-*.rlib, not as a separate "
                   "archive. The declarations follow VitaSDK's psp2/audioout.h, whose repository text "
                   "is covered by vita-headers-LICENSE.md." % module)
    out.append("")


def archives_in_map(path):
    text = Path(path).read_text(encoding="utf-8", errors="replace")
    found = set(re.findall(r"([A-Za-z0-9_+.\-]+\.a)\(", text))
    found |= set(re.findall(r"/(crt[a-z0-9]+\.o)\b", text))
    return found


def vita_glue_objects(path):
    """The newlib/libc/sys/vita objects the link map actually loads."""
    text = Path(path).read_text(encoding="utf-8", errors="replace")
    found = ["crt0.o"] if re.search(r"/crt0\.o\b", text) else []
    for member in VITA_GLUE_MEMBERS:
        if "libc.a(%s)" % member in text:
            found.append("libc.a(%s)" % member)
    return found


def covered(archive):
    for library in C_LIBRARIES:
        for pattern in library["archives"]:
            if pattern.startswith("*") and archive.endswith(pattern[1:]) or archive == pattern:
                return True
    # Rust rlibs and the program's own objects are covered by the Rust sections.
    return archive.endswith(".rlib") or archive.startswith("liblooking_glass")


def credits_section():
    """The CREDITS section of the notices: attribution for the two documented gaps."""
    lines = [
        "=== CREDITS: authors of linked files that state no licence ===",
        "",
    ]
    lines += textwrap.wrap(
        "The owner decided on 2026-11-15 that no permission request would be posted for "
        "the two gaps below. The release ships with each gap documented and the authors "
        "credited. That decision is an accepted risk, not a licence grant: nothing here "
        "asserts that permission exists for these files. See LEGAL.md and CREDITS.txt.",
        width=96)
    lines += ["", "newlib PS Vita port", ""]
    lines += textwrap.wrap(
        "The link loads objects compiled from six files in newlib/libc/sys/vita of "
        "https://github.com/vitasdk/newlib at " + NEWLIB_REVISION + " that state no "
        "licence: io.c, mlock.c, sbrk.c, syscalls.c and threading.c carry no copyright "
        "or licence line, and crt0.c carries only the PSPSDK attribution quoted below. "
        "Each file's introduction commit and the authors git blame attributes its lines "
        "to at " + NEWLIB_REVISION[:7] + " are listed here. They are credited as "
        "contributors, not as grantors of a licence.",
        width=96)
    lines.append("")
    for name, introduced, authors in NEWLIB_GLUE_CREDITS:
        lines.append("  " + name)
        lines.extend(textwrap.wrap("Introduced by " + introduced + ".",
                                   width=96, initial_indent="    ",
                                   subsequent_indent="      "))
        lines.extend(textwrap.wrap("Line authors at " + NEWLIB_REVISION[:7] + ": "
                                   + authors + ".",
                                   width=96, initial_indent="    ",
                                   subsequent_indent="      "))
    lines += [
        "",
        NEWLIB_GLUE_CREDIT_NOTE,
        "",
        "crt0.c also carries this attribution and no licence text:",
        "",
        '  Code below is based on the PSPSDK implementation',
        '  Copyright (c) 2005 Marcus R. Brown <mrbrown@ocgnet.org>',
        "",
        "red-black tree in SceShaccCgExt",
        "",
    ]
    lines += textwrap.wrap(
        RBTREE_CREDIT["library"] + " 1.0.1 at " + RBTREE_CREDIT["revision"]
        + " contains a red-black tree in " + RBTREE_CREDIT["file"] + " copied from "
        + RBTREE_CREDIT["url"] + ", whose rbtree.h names its author as "
        + RBTREE_CREDIT["author"] + " (GitHub: " + RBTREE_CREDIT["handle"] + "). The "
        "upstream repository has no licence file and no licence statement in its files "
        "(measured at " + RBTREE_CREDIT["upstream"] + ", 2021-04-13); the source comment "
        "links the repository and states the origin. " + RBTREE_CREDIT["author"]
        + " is credited as the author of that code. No licence is claimed for it.",
        width=96)
    lines.append("")
    return lines


def credits_text():
    """The short CREDITS.txt shipped at the VPK root, from the same facts as the section."""
    lines = [
        "Credits for third-party files that state no licence",
        "",
        "Looking Glass, PS Vita port. See LEGAL.md and THIRD_PARTY_NOTICES-vita.txt.",
        "",
        "newlib PS Vita port",
        "",
    ]
    lines += textwrap.wrap(
        "The release links objects from newlib/libc/sys/vita of the VitaSDK newlib fork "
        "at " + NEWLIB_REVISION + ". These linked files carry no copyright or licence "
        "statement and are not named in COPYING.NEWLIB:",
        width=96)
    lines += [
        "",
        "  io.c, mlock.c, sbrk.c, syscalls.c, threading.c",
        "",
        "crt0.c carries this PSPSDK attribution and no licence text:",
        "",
        '  Code below is based on the PSPSDK implementation',
        '  Copyright (c) 2005 Marcus R. Brown <mrbrown@ocgnet.org>',
        "",
    ]
    lines += textwrap.wrap(
        "Contributors recorded in the public Git history of "
        "https://github.com/vitasdk/newlib for these files:",
        width=96)
    lines += [
        "",
        "  Ilya Zhuravlev, Yifan Lu, Davee, Ivan Epifanov,",
        "  Francisco José García García, Sergi Granell, Vladimir Atamanenko,",
        "  Nikolay Arhipov, Glenn Anderson, Rinnegatamante, Jeffrey Pfau, frangarcj,",
        "  Sunguk Lee, TheOfficialFloW, Rémy F",
        "",
        "red-black tree in SceShaccCgExt",
        "",
    ]
    lines += textwrap.wrap(
        RBTREE_CREDIT["file"] + " of " + RBTREE_CREDIT["library"] + " at "
        + RBTREE_CREDIT["revision"] + " contains a red-black tree copied from "
        + RBTREE_CREDIT["url"] + ". The author named in its rbtree.h is "
        + RBTREE_CREDIT["author"] + " (GitHub: " + RBTREE_CREDIT["handle"] + "). The "
        "upstream repository has no licence file and states no licence.",
        width=96)
    lines += [""]
    lines += textwrap.wrap(
        "The owner decided on 2026-11-15 that no permission request would be posted for "
        "these two items and that the release ships with the gaps documented and the "
        "authors credited. That is an accepted risk, not a licence grant. No licence is "
        "claimed for the files named above.",
        width=96)
    lines.append("")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--link-map")
    parser.add_argument("--output", default="THIRD_PARTY_NOTICES-vita.txt")
    parser.add_argument("--credits-output",
                        help="also write the short CREDITS.txt shipped at the VPK root")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    texts = repo / "tools" / "vita_licenses"
    os.chdir(repo)

    linked_archives = set()
    vita_objects = []
    if args.link_map:
        linked_archives = archives_in_map(args.link_map)
        vita_objects = vita_glue_objects(args.link_map)
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
    mpl = mpl_checksums(repo)

    out = [
        "Third-party notices for the Looking Glass PS Vita build",
        "",
        "Generated by tools/vita_notices.py. Original game data is not included. Sony's shader compiler",
        "(libshacccg.suprx) is loaded at run time from the user's console and is not distributed.",
        "",
        "Looking Glass itself is under LICENSE-MIT; see LEGAL.md. The C libraries under the GNU LGPL are",
        "linked statically. Their corresponding source, the port's object code for this link, the exact",
        "linker command and a relink and packaging script are in the source bundle published with each",
        "release; this package carries SOURCE-OFFER.txt, which names the bundle and the written offer.",
        "That material is what lets a user relink the program against modified versions of the libraries.",
        "",
        "MPL-2.0 components: the four symphonia 0.5.5 crates are unmodified. Their Source Code Form is",
        "the crates.io release archive, available at",
        "https://crates.io/api/v1/crates/<crate>/0.5.5/download. Cargo.lock pins these SHA-256 values:",
    ]
    for name in MPL_CRATES:
        out.append("  %-20s %s" % (name, mpl[name][1]))
    out += [""]
    out += credits_section()
    out += [
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
        out.extend(entry.get("notes", []))
        if entry["name"].startswith("newlib") and vita_objects:
            out.append("Objects compiled from newlib/libc/sys/vita in this link:")
            out.extend("  " + name for name in vita_objects)
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
    credits = credits_text() if args.credits_output else ""
    for private in {str(Path.home()), str(repo)}:
        if private in text or private in credits:
            print("refusing to write: a build machine path reached the notices", file=sys.stderr)
            return 1
    Path(args.output).write_text(text, encoding="utf-8")
    if args.credits_output:
        Path(args.credits_output).write_text(credits, encoding="utf-8")
    print(f"{args.output}: {len(packages)} Rust packages, {len(std_packages)} standard library packages, "
          f"{len(C_LIBRARIES)} C library entries, {text.count(chr(10))} lines")
    if args.credits_output:
        print(f"{args.credits_output}: {credits.count(chr(10))} lines")
    return 0


if __name__ == "__main__":
    sys.exit(main())
