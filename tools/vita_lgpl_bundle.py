#!/usr/bin/env python3
"""Assemble the LGPL source and relink bundle for the Alice Vita release.

The bundle carries the exact SceShaccCgExt source at the pinned revision, the
LGPL-2.1 text, copies of every object file and archive the release link used
(except the SDK and the release library itself), the exact linker command, the
release link map, a README and SHA-256 sums. It contains no game data and no
binary under another licence.

The object copies come from a linker wrapper run during the release build:
rustc deletes its temporary codegen objects after a successful link.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile

ARCHIVE = "libSceShaccCgExt.a"
HEAD_RE = re.compile(r"^head ([0-9a-f]{40})$", re.M)
ELF_HASH_RE = re.compile(r"^([0-9a-f]{64})  looking-glass\.elf$", re.M)
FORBIDDEN_SUFFIXES = (".pk3", ".vpk", ".self", ".velf", ".suprx", ".sfo",
                      ".tga", ".wav", ".ogg", ".vag", ".at3")


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def fail(message):
    raise ValueError(message)


def is_under(path, root):
    try:
        path.relative_to(root)
        return True
    except ValueError:
        return False


def absolute(path):
    return Path(os.path.abspath(os.path.expanduser(str(path))))


def load_pin(repo):
    folder = repo / "tools/vita_licenses"
    pin = json.loads((folder / "SceShaccCgExt-release.json").read_text(encoding="utf-8"))
    if pin["archive"] != ARCHIVE or pin["license"] != "LGPL-2.1":
        fail("unexpected SceShaccCgExt release pin")
    if sha256(folder / pin["license_file"]) != pin["license_sha256"]:
        fail("the pinned SceShaccCgExt licence text does not match")
    return pin


def read_capture(capture):
    count_file = capture / "count"
    if not count_file.is_file() or count_file.read_text(encoding="utf-8").strip() != "1":
        fail("the linker capture must record exactly one link")
    unknown = capture / "unknown.txt"
    if unknown.is_file() and unknown.read_text(encoding="utf-8").strip():
        fail("the linker capture contains inputs outside the target and SDK: "
             + unknown.read_text(encoding="utf-8").strip())
    argv_file = capture / "argv-1.txt"
    if not argv_file.is_file():
        fail("the linker capture has no argv-1.txt")
    if (capture / "response-1.txt").is_file():
        fail("the linker used a response file; expand it before assembling the bundle")
    argv = argv_file.read_text(encoding="utf-8").splitlines()
    if not argv:
        fail("the recorded linker command is empty")
    return argv


def classify(argv, target_root, lib_dir, sdk, release_build, capture):
    """Map the recorded argv to bundle placeholders and list required copies."""
    mapped = []
    copies = []
    empty_dirs = []
    count = {"lib": 0, "sdk": 0, "output": 0, "map": 0, "library_flag": 0}
    lib_index = None
    sdk_index = None
    i = 0
    while i < len(argv):
        arg = argv[i]
        if arg == "-L":
            i += 1
            if i >= len(argv):
                fail("-L without a directory in the recorded link")
            value = absolute(argv[i])
            if value == lib_dir:
                mapped.extend(["-L", "@BUNDLE@/lib"])
                count["lib"] += 1
                lib_index = len(mapped) - 1
            elif value == sdk / "arm-vita-eabi/lib":
                mapped.extend(["-L", "@SDK@/arm-vita-eabi/lib"])
                count["sdk"] += 1
                sdk_index = len(mapped) - 1
            elif is_under(value, target_root):
                relative = value.relative_to(target_root)
                mapped.extend(["-L", "@BUNDLE@/objects/" + relative.as_posix()])
                empty_dirs.append(relative)
            else:
                fail("unexpected linker search directory: " + argv[i])
        elif arg == "-o":
            i += 1
            if i >= len(argv):
                fail("-o without a path in the recorded link")
            output = absolute(argv[i])
            if not is_under(output, target_root):
                fail("the recorded link output is outside the Cargo target: " + argv[i])
            mapped.extend(["-o", "@OUT@"])
            count["output"] += 1
        elif arg.startswith("-Wl,-Map="):
            original = absolute(arg[len("-Wl,-Map="):])
            if not is_under(original, release_build):
                fail("the recorded link map is outside the release build: " + str(original))
            mapped.append("-Wl,-Map=@BUNDLE@/output/relink.map")
            count["map"] += 1
        elif arg.startswith("-"):
            if arg.startswith("@"):
                fail("the recorded link uses a response file")
            if arg == "-lSceShaccCgExt":
                count["library_flag"] += 1
            mapped.append(arg)
        else:
            value = absolute(arg)
            if not value.is_absolute():
                fail("relative path in the recorded link: " + arg)
            if is_under(value, target_root):
                relative = value.relative_to(target_root)
                source = capture / "objects" / relative
                if not source.is_file() or source.stat().st_size == 0:
                    fail("the linker capture is missing an input object: " + relative.as_posix())
                copies.append(relative)
                mapped.append("@BUNDLE@/objects/" + relative.as_posix())
            elif is_under(value, sdk):
                mapped.append("@SDK@/" + value.relative_to(sdk).as_posix())
            else:
                fail("the recorded link reads a path that is neither target, SDK nor release library: "
                     + arg)
        i += 1
    if count["lib"] != 1:
        fail("the release link must search the release library directory exactly once")
    if count["sdk"] != 1:
        fail("the release link must search the SDK library directory exactly once")
    if count["library_flag"] != 1:
        fail("the release link must link SceShaccCgExt exactly once")
    if count["output"] != 1 or count["map"] != 1:
        fail("the release link must name one output and one link map")
    if lib_index > sdk_index:
        fail("the SDK library directory is searched before the release library")
    if not copies:
        fail("the release link has no object files to ship")
    return mapped, sorted(set(copies)), sorted(set(empty_dirs))


def copy_objects(capture, copies, bundle):
    total = 0
    for relative in copies:
        source = capture / "objects" / relative
        if source.is_symlink():
            fail("refusing a symlink in the linker capture: " + relative.as_posix())
        destination = bundle / "objects" / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
        total += source.stat().st_size
    return total


def extract_source(source_repo, revision, destination):
    head = subprocess.run(["git", "-C", str(source_repo), "rev-parse", "HEAD"],
                          check=True, stdout=subprocess.PIPE, text=True).stdout.strip()
    if head != revision:
        fail("SceShaccCgExt source is at " + head + ", not " + revision)
    status = subprocess.run(["git", "-C", str(source_repo), "status", "--porcelain=v1",
                             "--untracked-files=all"],
                            check=True, stdout=subprocess.PIPE, text=True).stdout
    if status.strip():
        fail("SceShaccCgExt source checkout is not clean")
    data = subprocess.run(["git", "-C", str(source_repo), "archive", "--format=tar", revision],
                          check=True, stdout=subprocess.PIPE).stdout
    destination.mkdir(parents=True)
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        for member in archive.getmembers():
            name = member.name
            if name.startswith("/") or ".." in Path(name).parts:
                fail("unsafe path in the SceShaccCgExt archive: " + name)
            if member.issym() or member.islnk():
                fail("link in the SceShaccCgExt archive: " + name)
            archive.extract(member, destination)


def write_readme(bundle, facts):
    text = README_TEMPLATE
    for key, value in facts.items():
        text = text.replace("@@%s@@" % key.upper(), str(value))
    (bundle / "README.md").write_text(text, encoding="utf-8")


def write_relink(bundle):
    path = bundle / "link/relink.sh"
    path.write_text(RELINK_TEMPLATE, encoding="utf-8")
    path.chmod(0o755)


def write_sums(bundle):
    lines = []
    for path in sorted(bundle.rglob("*")):
        if not path.is_file():
            continue
        relative = path.relative_to(bundle).as_posix()
        if relative == "SHA256SUMS":
            continue
        lines.append("%s  %s" % (sha256(path), relative))
    (bundle / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")
    return lines


def check_bundle(bundle, mapped, forbidden_names):
    for token in mapped:
        for placeholder in ("@BUNDLE@/",):
            if token.startswith(placeholder):
                path = bundle / token[len(placeholder):]
                if not path.exists():
                    fail("the bundle is missing a path named by the link: " + token)
    for path in bundle.rglob("*"):
        if path.is_file() and path.suffix.lower() in FORBIDDEN_SUFFIXES:
            fail("the bundle contains a forbidden binary or game-data file: "
                 + path.relative_to(bundle).as_posix())
    for name in forbidden_names:
        if (bundle / name).exists():
            fail("the bundle contains " + name)


def make_tarball(bundle):
    tarball = bundle.parent / (bundle.name + ".tar.gz")
    if tarball.exists():
        tarball.unlink()
    subprocess.run(["tar", "-czf", str(tarball), "-C", str(bundle.parent), bundle.name],
                   check=True)
    return tarball


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release-build", required=True, type=Path,
                        help="directory written by tools/vita_release_build.sh")
    parser.add_argument("--capture", required=True, type=Path,
                        help="linker wrapper capture directory with argv-1.txt and objects/")
    parser.add_argument("--target-root", required=True, type=Path,
                        help="Cargo target directory used by the release build")
    parser.add_argument("--lib-dir", required=True, type=Path,
                        help="directory that held the pinned SceShaccCgExt archive")
    parser.add_argument("--shacccg-source", required=True, type=Path,
                        help="clean SceShaccCgExt checkout at the pinned revision")
    parser.add_argument("--out", required=True, type=Path,
                        help="bundle directory to create")
    args = parser.parse_args()

    repo = Path(__file__).resolve().parent.parent
    try:
        pin = load_pin(repo)
        release = absolute(args.release_build)
        capture = absolute(args.capture)
        target_root = absolute(args.target_root)
        lib_dir = absolute(args.lib_dir)
        source = absolute(args.shacccg_source)
        bundle = absolute(args.out)
        for path in (release, capture, target_root, lib_dir, source):
            if not path.is_dir():
                fail("not a directory: " + str(path))
        if bundle.exists() and any(bundle.iterdir()):
            fail("output directory is not empty: " + str(bundle))

        build_text = (release / "BUILD.txt").read_text(encoding="utf-8")
        head = HEAD_RE.search(build_text)
        if not head:
            fail("release BUILD.txt has no head")
        head = head.group(1)
        elf_hashes = ELF_HASH_RE.findall(build_text)
        if len(set(elf_hashes)) != 1:
            fail("release BUILD.txt does not name one looking-glass.elf hash")
        elf_hash = elf_hashes[0]
        if sha256(release / "looking-glass.elf") != elf_hash:
            fail("release looking-glass.elf does not match BUILD.txt")
        selection = json.loads((release / "library-selection.json").read_text(encoding="utf-8"))
        if selection["revision"] != pin["revision"] or selection["license"] != pin["license"]:
            fail("release library selection does not match the source pin")
        if selection["archive_sha256"] != pin["archive_sha256"]:
            fail("release library archive does not match the pinned bytes")
        if not selection.get("members"):
            fail("release library selection has no contributing members")
        recorded = absolute(selection["archive"].replace("~", str(Path.home()), 1))
        if recorded.parent != lib_dir:
            fail("--lib-dir does not match the directory recorded in library-selection.json")
        if (lib_dir / ARCHIVE).is_file() and sha256(lib_dir / ARCHIVE) != pin["archive_sha256"]:
            fail("the release library archive no longer matches its pin")

        argv = read_capture(capture)
        mapped, copies, empty_dirs = classify(argv, target_root, lib_dir,
                                             absolute(os.environ.get("VITASDK", "")),
                                             release, capture)

        bundle.mkdir(parents=True, exist_ok=True)
        (bundle / "link").mkdir()
        (bundle / "licenses").mkdir()
        (bundle / "lib").mkdir()
        (bundle / "output").mkdir()
        copied_bytes = copy_objects(capture, copies, bundle)
        for relative in empty_dirs:
            (bundle / "objects" / relative).mkdir(parents=True, exist_ok=True)
        if not (bundle / "objects").is_dir():
            fail("no object code was copied")

        extract_source(source, pin["revision"], bundle / "SceShaccCgExt-source")
        licence_source = bundle / "SceShaccCgExt-source/LICENSE"
        if sha256(licence_source) != pin["license_sha256"]:
            fail("the SceShaccCgExt source licence text does not match the pin")
        shutil.copy2(licence_source, bundle / "licenses/LGPL-2.1.txt")

        for name, destination in (("link.map", "link/link.map"),
                                  ("BUILD.txt", "link/BUILD.txt"),
                                  ("library-selection.json", "link/library-selection.json"),
                                  ("libraries.txt", "link/libraries.txt")):
            if (release / name).is_file():
                shutil.copy2(release / name, bundle / destination)
        notices = release / "THIRD_PARTY_NOTICES-vita.txt"
        if notices.is_file():
            shutil.copy2(notices, bundle / "licenses/THIRD_PARTY_NOTICES-vita.txt")
        (bundle / "link/args.txt").write_text("\n".join(mapped) + "\n", encoding="utf-8")
        (bundle / "link/release-elf.sha256").write_text(elf_hash + "\n", encoding="utf-8")
        (bundle / "link/pinned-library.sha256").write_text(pin["archive_sha256"] + "\n",
                                                           encoding="utf-8")
        members = []
        for entry in selection["members"]:
            if not entry.get("member") or not entry.get("sha256"):
                fail("release library selection has an incomplete member entry")
            members.append("%s  %s" % (entry["sha256"], entry["member"]))
        (bundle / "link/library-members.txt").write_text("\n".join(members) + "\n",
                                                         encoding="utf-8")
        (bundle / "link/link-map.sha256").write_text(
            sha256(bundle / "link/link.map") + "  link.map\n", encoding="utf-8")
        inputs = []
        for relative in copies:
            path = bundle / "objects" / relative
            inputs.append("%s  %10d  %s" % (sha256(path), path.stat().st_size,
                                            relative.as_posix()))
        (bundle / "link/inputs.txt").write_text("\n".join(inputs) + "\n", encoding="utf-8")
        write_relink(bundle)
        write_readme(bundle, {
            "head": head,
            "elf": elf_hash,
            "library": pin["archive_sha256"],
            "revision": pin["revision"],
            "objects": len(copies),
            "bytes": copied_bytes,
        })
        check_bundle(bundle, mapped, ("eboot.bin", "looking-glass.vpk", "looking-glass.self"))
        sums = write_sums(bundle)
        tarball = make_tarball(bundle)
        print("PASS bundle %s" % bundle)
        print("PASS files %d, sha256 entries %d" % (sum(1 for p in bundle.rglob("*") if p.is_file()), len(sums)))
        print("PASS tarball %s %s %d bytes" % (tarball, sha256(tarball), tarball.stat().st_size))
        print("release head %s" % head)
        print("release elf sha256 %s" % elf_hash)
        print("library sha256 %s" % pin["archive_sha256"])
        return 0
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError) as error:
        print("FAIL LGPL bundle: " + str(error), file=sys.stderr)
        return 1


README_TEMPLATE = """# Alice (Looking Glass Vita port) source and relink bundle

This bundle satisfies the source and relink part of LGPL-2.1 section 6 for the
statically linked SceShaccCgExt library used by the PlayStation Vita release of
the Looking Glass port of American McGee's Alice.

- Release head: `@@HEAD@@`
- Release ELF `looking-glass.elf` SHA-256: `@@ELF@@`
- SceShaccCgExt revision: `@@REVISION@@`
- Pinned `libSceShaccCgExt.a` SHA-256: `@@LIBRARY@@`
- Link inputs shipped: @@OBJECTS@@ files, @@BYTES@@ bytes

## Contents

- `SceShaccCgExt-source/` - the complete source tree at `@@REVISION@@`, exported
  from Git. Its `LICENSE` is the LGPL-2.1 text.
- `licenses/LGPL-2.1.txt` - the same LGPL-2.1 text, hash-pinned by the release
  tooling.
- `licenses/THIRD_PARTY_NOTICES-vita.txt` - the notices generated for this
  link, including every other linked library and Rust package.
- `objects/` - every object file and static archive that the release link read
  from the Cargo target directory. These are the port's own compiled code,
  including the Rust standard library and third-party Rust crates that form the
  work that uses the library. They are the exact bytes of the release link;
  `link/inputs.txt` lists each file with its size and SHA-256.
- `link/args.txt` - the exact linker command, one argument per line, with three
  placeholders: `@BUNDLE@` is this directory, `@SDK@` is `$VITASDK`, and
  `@OUT@` is the output ELF.
- `link/relink.sh` - runs that command against `lib/libSceShaccCgExt.a`.
- `link/link.map` - the release linker map, with the build machine's home
  directory shortened to `~`.
- `link/BUILD.txt`, `link/library-selection.json`, `link/libraries.txt` -
  release build facts: toolchain, flags, SDK components, and the archive and
  members the linker actually loaded.
- `link/release-elf.sha256`, `link/link-map.sha256`,
  `link/pinned-library.sha256`, `link/library-members.txt`,
  `link/inputs.txt` - the identities used by `relink.sh`. The member list is
  the deciding identity: a rebuild of the same source can change the archive
  file hash because `ar` records member timestamps, but its object bytes stay
  the same and the relink stays byte-identical.
- `SHA256SUMS` - SHA-256 of every file in this bundle except itself.
- `lib/` - empty on purpose. Put the SceShaccCgExt archive to relink against
  here as `lib/libSceShaccCgExt.a`.

## What is not in this bundle

- No game data, no pack, texture, sound or movie.
- No binary under another licence. The C archives other than SceShaccCgExt are
  part of the VitaSDK and are not redistributed here: `libvitaGL.a`,
  `libvitashark.a`, `libmathneon.a`, newlib, pthread-embedded, taiHEN and the
  Sce stub archives. `link/BUILD.txt` records the exact SDK components and
  archive SHA-256 values the release used. A byte-identical relink needs the
  same archives; a functional relink works with compatible ones.

## Relink against the unmodified library (proof of the kit)

1. Install the VitaSDK components listed in `link/BUILD.txt`, or use the SDK
   used for the release. Export its root as `VITASDK`.
2. Build the supplied source at `@@REVISION@@`:

   ```
   cmake SceShaccCgExt-source -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \\
     -DCMAKE_C_FLAGS="-std=gnu11 -Wno-error=implicit-function-declaration -Wno-error=int-conversion -Wno-error=incompatible-pointer-types" \\
     -DCMAKE_CXX_FLAGS="-std=gnu++11 -Wno-error=implicit-function-declaration -Wno-error=int-conversion -Wno-error=incompatible-pointer-types"
   make
   ```

   The resulting `libSceShaccCgExt.a` can have a different file SHA-256 from
   the pinned `@@LIBRARY@@` because `ar` stores member timestamps. What matters
   is the member object bytes: `link/library-members.txt` holds their SHA-256,
   and `relink.sh` compares those. With this source and toolchain the members
   equal the pinned ones and the relink is byte-identical.
3. `mkdir -p lib && cp libSceShaccCgExt.a lib/`
4. `VITASDK=/path/to/vitasdk ./link/relink.sh output/looking-glass.elf`
5. The script prints the ELF SHA-256. With the pinned member objects it must be
   `@@ELF@@`, byte for byte the release ELF. The script fails if the SDK
   archive is loaded instead or if the expected bytes do not follow.

## Relink against a modified library

1. Modify `SceShaccCgExt-source/` as you wish and build it as above.
2. Put the new archive at `lib/libSceShaccCgExt.a`.
3. Run `./link/relink.sh output/modified.elf`. The output is a new ELF that
   contains your modified library. Its hash will differ from the release; that
   is expected and is the point of the bundle.
4. Package the ELF into a SELF/VPK with the usual Vita tools if you want to run
   it. This bundle covers the link, not the signing or packaging steps.

## How the object code was preserved

Rust removes its temporary codegen objects after a successful link. The release
build therefore ran its linker through a wrapper that copied every target-side
input before invoking the real linker. `link/inputs.txt` is the list of those
copies; `link/args.txt` is the exact argument list. The bundle was checked by
relinking from an extracted copy of the tarball and comparing the ELF with the
release hash above.
"""


RELINK_TEMPLATE = """#!/bin/sh
# Relink the Looking Glass Vita ELF from the object code in this bundle.
#
# Usage: VITASDK=/path/to/vitasdk ./link/relink.sh [OUTPUT_ELF]
#
# The archive under lib/libSceShaccCgExt.a is linked instead of the SDK one.
# With the pinned archive the output is the release ELF byte for byte.
set -eu
if [ "$#" -gt 1 ]; then
    echo "usage: $0 [OUTPUT_ELF]" >&2
    exit 2
fi
here=$(cd "$(dirname "$0")/.." && pwd)
: "${VITASDK:?set VITASDK to the Vita SDK root}"
sdk=$VITASDK
linker=$sdk/bin/arm-vita-eabi-gcc
[ -x "$linker" ] || { echo "missing $linker" >&2; exit 1; }
archiver=$sdk/bin/arm-vita-eabi-ar
[ -x "$archiver" ] || { echo "missing $archiver" >&2; exit 1; }
library=$here/lib/libSceShaccCgExt.a
[ -f "$library" ] || {
    echo "missing $library; put the SceShaccCgExt archive to relink against there" >&2
    exit 1
}
output=${1:-$here/output/looking-glass.elf}
case $output in
    /*) ;;
    *) output=$(pwd)/$output ;;
esac
mkdir -p "$(dirname "$output")" "$here/output"

hash_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

set --
while IFS= read -r line; do
    case $line in
        @BUNDLE@/*) line=$here/${line#@BUNDLE@/} ;;
        -Wl,-Map=@BUNDLE@/*) line=-Wl,-Map=$here/${line#-Wl,-Map=@BUNDLE@/} ;;
        @SDK@/*) line=$sdk/${line#@SDK@/} ;;
        @OUT@) line=$output ;;
        *) ;;
    esac
    set -- "$@" "$line"
done < "$here/link/args.txt"

"$linker" "$@"

map=$here/output/relink.map
grep -F "$library" "$map" >/dev/null 2>&1 || {
    echo "the link did not load $library" >&2
    exit 1
}
if grep -F "$sdk/arm-vita-eabi/lib/libSceShaccCgExt.a" "$map" >/dev/null 2>&1; then
    echo "the link also loaded the SDK SceShaccCgExt archive" >&2
    exit 1
fi

actual=$(hash_file "$output")
echo "relinked ELF $output"
echo "sha256 $actual"

# Decide whether the supplied archive carries the pinned member objects. The
# archive file hash is not decisive: ar stores member timestamps, so a rebuild
# of the same source can hash differently while its objects are identical.
pinned_members=true
if [ -s "$here/link/library-members.txt" ]; then
    member_file=$here/output/.member
    while read -r expected member; do
        [ -n "$member" ] || continue
        if ! "$archiver" p "$library" "$member" > "$member_file" 2>/dev/null; then
            pinned_members=false
            break
        fi
        if [ "$(hash_file "$member_file")" != "$expected" ]; then
            pinned_members=false
            break
        fi
    done < "$here/link/library-members.txt"
    rm -f "$member_file"
else
    pinned_members=false
fi

echo "archive file sha256 $(hash_file "$library")"
if $pinned_members; then
    expected=$(cat "$here/link/release-elf.sha256")
    if [ "$actual" != "$expected" ]; then
        echo "FAIL the relink does not reproduce the release ELF ($expected)" >&2
        exit 1
    fi
    echo "PASS the relink reproduces the release ELF"
else
    echo "the contributing member objects differ from the pinned ones; this is a modified relink, not the release ELF"
fi
"""


if __name__ == "__main__":
    sys.exit(main())
