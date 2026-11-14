#!/usr/bin/env python3
"""Pin release library bytes and verify their actual archive/member selection.

Preflight requires --lib-dir containing the explicitly pinned LGPL archive. With
--link-map, verify every mentioned selection, require actual contributing members,
and record their hashes. This does not qualify every licence or the whole release.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ARCHIVE = "libSceShaccCgExt.a"
MEMBER = re.compile(r"((?:/|~/)[^()\n]*?/libSceShaccCgExt\.a)\(([^()]+)\)")
LOAD = re.compile(r"^\s*LOAD\s+(.*/libSceShaccCgExt\.a)\s*$", re.M)


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def load_pin(repo):
    folder = Path(repo) / "tools/vita_licenses"
    pin = json.loads((folder / "SceShaccCgExt-release.json").read_text(encoding="utf-8"))
    if pin["archive"] != ARCHIVE or pin["license"] != "LGPL-2.1":
        raise ValueError("unexpected release library pin")
    if sha256(folder / pin["license_file"]) != pin["license_sha256"]:
        raise ValueError("the pinned SceShaccCgExt licence text does not match")
    return pin


def select_library(lib_dir, pin):
    if not lib_dir:
        raise ValueError("release mode requires --lib-dir with the pinned LGPL SceShaccCgExt archive; "
                         "use --private-test only for a non-release compile check")
    selected = (Path(lib_dir).expanduser().resolve() / ARCHIVE).resolve(strict=True)
    digest = sha256(selected)
    if digest == pin["known_sdk_gpl_sha256"]:
        raise ValueError("refusing the known SDK GPL SceShaccCgExt archive in release mode")
    if digest != pin["archive_sha256"]:
        raise ValueError("SceShaccCgExt archive does not match the pinned LGPL build: " + digest)
    return selected


def selection_in_map(text):
    members = {(path.strip(), member) for path, member in MEMBER.findall(text)}
    loaded = {path.strip() for path in LOAD.findall(text)}
    # A relative or otherwise unrecognised archive reference must not disappear
    # silently from a release check. The pinned selection is an absolute path.
    for line in text.splitlines():
        if ARCHIVE not in line:
            continue
        if not MEMBER.search(line) and not LOAD.fullmatch(line):
            raise ValueError("unrecognised SceShaccCgExt link-map reference: " + line.strip())
    return loaded | {path for path, _ in members}, members


def verify_link_map(text, selected, pin):
    paths, members = selection_in_map(text)
    if not members:
        raise ValueError("link map has no contributing SceShaccCgExt archive members")
    if not paths:
        raise ValueError("link map has no SceShaccCgExt archive selection")
    selected = Path(selected).resolve(strict=True)
    for path in paths:
        actual = Path(path).expanduser().resolve(strict=True)
        if actual != selected:
            raise ValueError("link map selected a different SceShaccCgExt archive: " + path)
        # Check again after the link, not only before Cargo starts.
        if sha256(actual) != pin["archive_sha256"]:
            raise ValueError("selected SceShaccCgExt archive changed during the build")
    return sorted({member for _, member in members})


def member_hashes(archive, members, ar):
    result = []
    for member in members:
        data = subprocess.run([str(ar), "p", str(archive), member], check=True,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE).stdout
        if not data:
            raise ValueError("contributing archive member is empty or absent: " + member)
        result.append({"member": member, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()})
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lib-dir")
    parser.add_argument("--link-map", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    try:
        pin = load_pin(repo)
        selected = select_library(args.lib_dir, pin)
        receipt = {"mode": "release", "library": "SceShaccCgExt", "source": pin["source"],
                   "revision": pin["revision"], "license": pin["license"],
                   "license_sha256": pin["license_sha256"], "archive_sha256": sha256(selected),
                   "archive": str(selected).replace(str(Path.home()), "~", 1), "members": []}
        if args.link_map:
            members = verify_link_map(args.link_map.read_text(encoding="utf-8"), selected, pin)
            if not os.environ.get("VITASDK"):
                raise ValueError("VITASDK is required to hash the selected archive members")
            sdk = Path(os.environ["VITASDK"])
            receipt["members"] = member_hashes(selected, members, sdk / "bin/arm-vita-eabi-ar")
            receipt["link_map_sha256"] = sha256(args.link_map)
        if args.output:
            args.output.write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
        print("PASS SceShaccCgExt release archive " + receipt["archive_sha256"] +
              ("; contributing members " + ", ".join(x["member"] for x in receipt["members"])
               if args.link_map else "; preflight only"))
        return 0
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError) as error:
        print("FAIL release library selection: " + str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
