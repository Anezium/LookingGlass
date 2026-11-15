#!/usr/bin/env python3
"""Build the release VPK with third-party notices, licence texts and a source offer.

The Vita release VPK used to carry only eboot.bin, param.sfo and the LiveArea
assets. This tool repacks it with the same VitaSDK commands and flags the
release used, adding:

  THIRD_PARTY_NOTICES-vita.txt
  SOURCE-OFFER.txt
  licenses/GPL-3.0.txt
  licenses/LGPL-3.0.txt
  licenses/LGPL-2.1.txt
  licenses/GCC-RUNTIME-EXCEPTION-3.1.txt
  licenses/MPL-2.0.txt
  licenses/MIT.txt
  licenses/Apache-2.0.txt
  licenses/LookingGlass-LICENSE-MIT.txt
  licenses/newlib-COPYING.NEWLIB.txt
  licenses/newlib-COPYING.LIBGLOSS.txt
  licenses/newlib-vita-glue-NOTICE.txt
  ... and the per-library licence files used by the notices

It changes packaging only. The ELF is an input; the tool hashes it before and
after and never writes it. The eboot.bin and param.sfo inside the VPK are
checked byte-for-byte against the inputs.

Usage: tools/vita_package.py --sfo param.sfo --eboot eboot.bin --elf ELF \\
           --head SHA --source-bundle NAME [--source-bundle-sha256 HEX] \\
           --out looking-glass.vpk
"""
import argparse
import hashlib
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile

# (name in the VPK under licenses/, file in tools/vita_licenses/)
LICENCES = [
    ("GPL-3.0.txt", "GPL-3.0.txt"),
    ("LGPL-3.0.txt", "vitaGL-COPYING.LESSER"),
    ("LGPL-2.1.txt", "SceShaccCgExt-LICENSE"),
    ("GCC-RUNTIME-EXCEPTION-3.1.txt", "GCC-RUNTIME-EXCEPTION-3.1.txt"),
    ("MPL-2.0.txt", "symphonia-LICENSE"),
    ("MIT.txt", "MIT.txt"),
    ("Apache-2.0.txt", "Apache-2.0.txt"),
    ("LookingGlass-LICENSE-MIT.txt", "LookingGlass-LICENSE-MIT.txt"),
    ("newlib-COPYING.NEWLIB.txt", "newlib-COPYING.NEWLIB"),
    ("newlib-COPYING.LIBGLOSS.txt", "newlib-COPYING.LIBGLOSS"),
    ("newlib-vita-glue-NOTICE.txt", "newlib-vita-glue-NOTICE.txt"),
    ("vita-headers-LICENSE.md", "vita-headers-LICENSE.md"),
    ("math-neon-LICENSE.txt", "math-neon-LICENSE"),
    ("taiHEN-LICENSE.txt", "taiHEN-LICENSE"),
    ("pthread-embedded-COPYING.txt", "pthread-embedded-COPYING"),
    ("pthread-embedded-COPYING.LIB.txt", "pthread-embedded-COPYING.LIB"),
    ("pthread-embedded-COPYING.vita.txt", "pthread-embedded-COPYING.vita"),
    ("vitaShaRK-LICENSE.txt", "vitaShaRK-LICENSE"),
]
LIVEAREA = [
    "sce_sys/icon0.png",
    "sce_sys/livearea/contents/bg.png",
    "sce_sys/livearea/contents/startup.png",
    "sce_sys/livearea/contents/template.xml",
]
MPL_CRATES = ("symphonia", "symphonia-bundle-mp3", "symphonia-core", "symphonia-metadata")
REQUIRED_MEMBERS = ["eboot.bin", "sce_sys/param.sfo",
                    "THIRD_PARTY_NOTICES-vita.txt", "SOURCE-OFFER.txt"]
REQUIRED_MEMBERS += ["licenses/" + name for name, _ in LICENCES]
REQUIRED_MEMBERS += LIVEAREA


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def fail(message):
    raise ValueError(message)


def mpl_checksums(cargo_lock):
    """The four MPL-2.0 crate versions and checksums from Cargo.lock."""
    text = Path(cargo_lock).read_text(encoding="utf-8")
    found = {}
    for block in text.split("[[package]]"):
        name = re.search(r'^name = "([^"]+)"$', block, re.M)
        version = re.search(r'^version = "([^"]+)"$', block, re.M)
        checksum = re.search(r'^checksum = "([0-9a-f]{64})"$', block, re.M)
        if name and name.group(1) in MPL_CRATES and version and checksum:
            found[name.group(1)] = (version.group(1), checksum.group(1))
    missing = [name for name in MPL_CRATES if name not in found]
    if missing:
        fail("Cargo.lock does not pin " + ", ".join(missing))
    for name, (version, _) in found.items():
        if version != "0.5.5":
            fail("Cargo.lock pins %s %s, expected 0.5.5" % (name, version))
    return found


def source_offer(facts, cargo_checksums):
    lines = [
        "Source and relink offer for the Looking Glass PS Vita package",
        "",
        "This package contains the third-party notices in",
        "THIRD_PARTY_NOTICES-vita.txt and the full licence texts under licenses/.",
        "The release build head is %s and the linked ELF hashes %s."
        % (facts["head"], facts["elf"]),
        "",
        "The corresponding source for the four GNU LGPL libraries linked into",
        "eboot.bin, and the material needed to relink this build against modified",
        "versions of them, are in the source bundle published with this release:",
        "",
        "  " + facts["bundle"],
    ]
    if facts.get("bundle_sha256"):
        lines += ["", "  SHA-256 " + facts["bundle_sha256"]]
    lines += [
        "",
        "It carries, from public repositories at the linked revisions:",
        "",
        "  vitaGL            %s  (LGPL-3.0-or-later)" % facts["vitagl"],
        "  vitaShaRK         %s  (LGPL-3.0-or-later, tag v.1.7)" % facts["vitashark"],
        "  pthread-embedded  %s  (LGPL-2.1, MIT Vita layer)" % facts["pthread"],
        "  SceShaccCgExt     %s  (LGPL-2.1)" % facts["shacccg"],
        "",
        "The bundle also carries the complete object code and rlibs of this build,",
        "the exact linker command (link/args.txt), the release link map,",
        "relink.sh, package.sh and the LiveArea packaging assets. It is published",
        "with the release and is available on written request.",
        "",
        "Written offer: for at least three years from the date this package is",
        "distributed, the maintainer will provide the same source and relink material",
        "to any third party on request through",
        "",
        "  https://github.com/Anezium/LookingGlass/issues",
        "",
        "at no charge beyond the cost of the physical medium and delivery. Identify",
        "the release by its build head or by the ELF SHA-256 above.",
        "",
        "MPL-2.0 source: the four symphonia 0.5.5 crates (symphonia,",
        "symphonia-bundle-mp3, symphonia-core and symphonia-metadata) are linked",
        "unmodified. Their Source Code Form is the crates.io release archive,",
        "available at https://crates.io/api/v1/crates/<crate>/0.5.5/download.",
        "Cargo.lock pins these SHA-256 values:",
        "",
    ]
    for name in MPL_CRATES:
        lines.append("  %-20s %s" % (name, cargo_checksums[name][1]))
    lines += ["", "Original game data and Sony firmware are not included and are not offered.", ""]
    return "\n".join(lines)


def collect_assets(assets, repo):
    source = Path(assets)
    if not source.is_dir():
        fail("missing assets directory: " + str(source))
    entries = []
    for relative in LIVEAREA:
        path = source / relative
        if not path.is_file():
            fail("missing LiveArea asset: " + str(path))
        entries.append((path, relative))
    return entries


def verify_vpk(vpk, eboot, sfo, extras):
    with zipfile.ZipFile(vpk) as archive:
        names = sorted(archive.namelist())
        if len(names) != len(set(names)):
            fail("duplicate member names in " + str(vpk))
        missing = [name for name in REQUIRED_MEMBERS if name not in names]
        if missing:
            fail("the VPK is missing: " + ", ".join(missing))
        if archive.read("eboot.bin") != Path(eboot).read_bytes():
            fail("the VPK eboot.bin differs from the input")
        if archive.read("sce_sys/param.sfo") != Path(sfo).read_bytes():
            fail("the VPK param.sfo differs from the input")
        for source, name in extras:
            if archive.read(name) != Path(source).read_bytes():
                fail("the VPK " + name + " differs from the staged file")
        reserved = ("eboot.bin", "sce_sys/param.sfo")
        for name in reserved:
            if names.count(name) != 1:
                fail("the VPK must carry one " + name)
        print("INFO vpk members=%s" % names)
        print("INFO vpk eboot.bin sha256=%s" % hashlib.sha256(archive.read("eboot.bin")).hexdigest())
    return names


def build_vpk(sdk, sfo, eboot, extras, out):
    packer = Path(sdk) / "bin/vita-pack-vpk"
    if not packer.is_file():
        fail("missing " + str(packer))
    command = [str(packer), "-s", str(sfo), "-b", str(eboot)]
    for source, name in extras:
        command += ["--add", "%s=%s" % (source, name)]
    command.append(str(out))
    subprocess.run(command, check=True)
    return command


def stage(repo, stage_dir, notices, cargo_checksums, facts):
    stage_dir = Path(stage_dir)
    stage_dir.mkdir(parents=True, exist_ok=True)
    extras = []
    notice_target = stage_dir / "THIRD_PARTY_NOTICES-vita.txt"
    shutil.copy2(notices, notice_target)
    extras.append((notice_target, "THIRD_PARTY_NOTICES-vita.txt"))
    licence_dir = stage_dir / "licenses"
    licence_dir.mkdir(exist_ok=True)
    for name, source_name in LICENCES:
        source = Path(repo) / "tools/vita_licenses" / source_name
        if not source.is_file():
            fail("missing licence text: " + str(source))
        target = licence_dir / name
        shutil.copy2(source, target)
        extras.append((target, "licenses/" + name))
    offer = stage_dir / "SOURCE-OFFER.txt"
    offer.write_text(source_offer(facts, cargo_checksums), encoding="utf-8")
    extras.append((offer, "SOURCE-OFFER.txt"))
    return extras


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vitasdk", type=Path,
                        help="Vita SDK containing bin/vita-pack-vpk (default: $VITASDK)")
    parser.add_argument("--sfo", required=True, type=Path)
    parser.add_argument("--eboot", required=True, type=Path)
    parser.add_argument("--elf", required=True, type=Path,
                        help="ELF the SELF was made from; hashed, never written")
    parser.add_argument("--head", required=True,
                        help="release build head named in the source offer")
    parser.add_argument("--assets", type=Path,
                        help="directory holding sce_sys/ (default: repo/vita)")
    parser.add_argument("--notices", type=Path,
                        help="generated THIRD_PARTY_NOTICES-vita.txt")
    parser.add_argument("--cargo-lock", type=Path)
    parser.add_argument("--source-bundle", required=True,
                        help="source bundle file name, e.g. alice-lgpl-9aab7cb-r4.tar.gz")
    parser.add_argument("--source-bundle-sha256", default="")
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--stage", type=Path,
                        help="keep the staged notices and licences in this directory")
    args = parser.parse_args()

    repo = Path(__file__).resolve().parent.parent
    try:
        sdk = Path(args.vitasdk or os.environ.get("VITASDK", ""))
        if not sdk.is_dir():
            fail("--vitasdk or VITASDK must name the Vita SDK")
        if not args.elf.is_file():
            fail("missing ELF: " + str(args.elf))
        elf_before = sha256(args.elf)
        checksums = mpl_checksums(args.cargo_lock or repo / "Cargo.lock")
        facts = {
            "head": args.head,
            "elf": elf_before,
            "bundle": args.source_bundle,
            "bundle_sha256": args.source_bundle_sha256,
            "vitagl": "dca4b9d143290d78ec043131a19be36c78cdc7b5",
            "vitashark": "df24065e65098b2d1ac533760109ad4367573f28",
            "pthread": "11d2e5722d98c86f33c908fc47b2cf6e55205db5",
            "shacccg": "d3c83efceedeca3be863d0c1f19bb145daf1685f",
        }
        notices = args.notices or repo / "THIRD_PARTY_NOTICES-vita.txt"
        if not notices.is_file():
            fail("missing notices: " + str(notices))
        assets = collect_assets(args.assets or repo / "vita", repo)
        temporary = None
        if args.stage:
            stage_dir = args.stage
        else:
            temporary = tempfile.TemporaryDirectory()
            stage_dir = Path(temporary.name)
        extras = stage(repo, stage_dir, notices, checksums, facts)
        extras = assets + extras
        build_vpk(sdk, args.sfo, args.eboot, extras, args.out)
        names = verify_vpk(args.out, args.eboot, args.sfo, extras)
        if sha256(args.elf) != elf_before:
            fail("the ELF changed while packaging")
        print("PASS vpk %s (%d members)" % (args.out, len(names)))
        print("INFO elf %s sha256=%s unchanged" % (args.elf, elf_before))
        print("INFO vpk sha256=%s bytes=%d" % (sha256(args.out), args.out.stat().st_size))
        if temporary:
            temporary.cleanup()
        return 0
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError, zipfile.BadZipFile) as error:
        print("FAIL Vita package: " + str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
