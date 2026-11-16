#!/usr/bin/env python3
"""Assemble the LGPL source and relink bundle for the Alice Vita release.

The bundle carries, for every LGPL library the release links statically:

- the exact source tree at the linked revision, exported from a public clone,
  without the upstream sample media that is not needed to build or relink;
- the licence files of that revision;
- the exact object code and archives the release link read (except the library
  archives themselves and the SDK archives), the exact linker command, the
  release link map, a relink script and a packaging script.

It contains no game data and no binary under another licence. The object copies
come from a linker wrapper run during the release build: rustc deletes its
temporary codegen objects after a successful link.
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
import tempfile

ARCHIVE = "libSceShaccCgExt.a"
LIBRARY_ORDER = ["libvitaGL.a", "libvitashark.a", "libSceShaccCgExt.a", "libpthread.a"]
HEAD_RE = re.compile(r"^head ([0-9a-f]{40})$", re.M)
ELF_HASH_RE = re.compile(r"^([0-9a-f]{64})  looking-glass\.elf$", re.M)
SELF_HASH_RE = re.compile(r"^([0-9a-f]{64})  eboot\.bin$", re.M)
VITAGL_LINE_RE = re.compile(
    r"^linked libvitaGL\.a sha256=([0-9a-f]{64}) package=(\S+)", re.M)
VITAGL_RECIPE_RE = re.compile(r"^vitagl-recipe make (.+?) \(documented", re.M)
VITAGL_REVISION = "dca4b9d143290d78ec043131a19be36c78cdc7b5"
VDPM_PACKAGES = "taihen vitaShaRK libmathneon"
FORBIDDEN_SUFFIXES = (".pk3", ".vpk", ".self", ".velf", ".suprx", ".sfo",
                      ".tga", ".wav", ".ogg", ".vag", ".at3")
ASSET_FILES = ("sce_sys/icon0.png",
               "sce_sys/livearea/contents/bg.png",
               "sce_sys/livearea/contents/startup.png",
               "sce_sys/livearea/contents/template.xml")
# Licence texts the VPK carries, mirroring tools/vita_package.py. The LGPL and
# GPL texts come from the source trees below; these fill the remaining entries.
PACKAGE_LICENCES = [
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
]


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


def normalise_member(name):
    """Compare a rebuilt CMake object name with the release archive name."""
    for suffix in (".c.obj", ".cc.obj", ".cpp.obj", ".cxx.obj"):
        if name.endswith(suffix):
            return name[:-len(suffix)] + ".o"
    if name.endswith(".obj"):
        return name[:-4] + ".o"
    return name


def read_archive_members(path):
    """Read a GNU/BSD ar archive into {member name: bytes}."""
    data = Path(path).read_bytes()
    if data[:8] == b"!<thin>\n":
        fail("thin archives are not supported: " + str(path))
    if data[:8] != b"!<arch>\n":
        fail("not an ar archive: " + str(path))
    members = {}
    long_names = b""
    index = 8
    while index + 60 <= len(data):
        header = data[index:index + 60]
        name = header[:16].decode("latin-1").rstrip()
        try:
            size = int(header[48:58].decode("latin-1").strip())
        except ValueError:
            fail("malformed ar header in " + str(path))
        body = data[index + 60:index + 60 + size]
        if len(body) != size:
            fail("truncated ar member in " + str(path))
        if name == "//":
            long_names = body
        elif name.startswith("#1/"):
            length = int(name[3:])
            name = body[:length].decode("utf-8", "replace").rstrip("\0")
            members[name] = body[length:]
        elif name not in ("/", "/SYM64/"):
            if name.startswith("/") and name[1:].isdigit():
                offset = int(name[1:])
                end = long_names.find(b"/\n", offset)
                if end < 0:
                    fail("bad long-name reference in " + str(path))
                name = long_names[offset:end].decode("utf-8", "replace")
            else:
                name = name.rstrip("/")
            if not name:
                fail("empty ar member name in " + str(path))
            members[name] = body
        index += 60 + size + (size & 1)
    if not members:
        fail("empty ar archive: " + str(path))
    return members


def strip_debug(data, objcopy, remove_comment=False):
    with tempfile.TemporaryDirectory() as temp:
        source = Path(temp) / "member.o"
        target = Path(temp) / "member.stripped.o"
        source.write_bytes(data)
        command = [str(objcopy), "--strip-debug"]
        if remove_comment:
            command += ["-R", ".comment", "-R", ".note"]
        command += [str(source), str(target)]
        subprocess.run(command, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        return target.read_bytes()


def load_pins(repo):
    """Load the four library pins; SceShaccCgExt keeps its release-tooling pin."""
    folder = repo / "tools/vita_licenses"
    pin = json.loads((folder / "SceShaccCgExt-release.json").read_text(encoding="utf-8"))
    if pin["archive"] != ARCHIVE or pin["license"] != "LGPL-2.1":
        fail("unexpected SceShaccCgExt release pin")
    if sha256(folder / pin["license_file"]) != pin["license_sha256"]:
        fail("the pinned SceShaccCgExt licence text does not match")
    if sha256(Path(repo) / "tools" / pin["patch"]) != pin["patch_sha256"]:
        fail("the SceShaccCgExt patch does not match its pin")
    data = json.loads((folder / "lgpl-libraries.json").read_text(encoding="utf-8"))
    libraries = {}
    for entry in data["libraries"]:
        if entry["archive"] in libraries or entry["archive"] == ARCHIVE:
            fail("duplicate LGPL library pin: " + entry["archive"])
        libraries[entry["archive"]] = entry
    libraries[ARCHIVE] = {
        "name": "SceShaccCgExt",
        "archive": ARCHIVE,
        "source": pin["source"],
        "revision": pin["revision"],
        "license": pin["license"],
        "license_files": [{"path": "LICENSE", "sha256": pin["license_sha256"]}],
        "bundle_licenses": [{"source": "LICENSE", "name": "LGPL-2.1.txt"}],
        "archive_sha256": pin["archive_sha256"],
        "identity": "raw",
        "members_total": 0,
        "members": [],
    }
    for archive in LIBRARY_ORDER:
        if archive not in libraries:
            fail("missing pin for " + archive)
    return libraries, pin


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


def classify(argv, target_root, lib_dir, sdk, release_build, capture, library_flags):
    """Map the recorded argv to bundle placeholders and list required copies."""
    mapped = []
    copies = []
    empty_dirs = []
    count = {"lib": 0, "sdk": 0, "output": 0, "map": 0}
    flags = {flag: 0 for flag in library_flags}
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
            if arg in flags:
                flags[arg] += 1
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
    for flag, seen in flags.items():
        if seen != 1:
            fail("the release link must name %s exactly once" % flag)
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


def check_source(source_repo, revision, name):
    head = subprocess.run(["git", "-C", str(source_repo), "rev-parse", "HEAD"],
                          check=True, stdout=subprocess.PIPE, text=True).stdout.strip()
    if head != revision:
        fail(name + " source is at " + head + ", not " + revision)
    status = subprocess.run(["git", "-C", str(source_repo), "status", "--porcelain=v1",
                             "--untracked-files=all"],
                            check=True, stdout=subprocess.PIPE, text=True).stdout
    if status.strip():
        fail(name + " source checkout is not clean")


def extract_source(source_repo, revision, destination, name):
    check_source(source_repo, revision, name)
    data = subprocess.run(["git", "-C", str(source_repo), "archive", "--format=tar", revision],
                          check=True, stdout=subprocess.PIPE).stdout
    destination.mkdir(parents=True)
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        for member in archive.getmembers():
            member_name = member.name
            if member_name.startswith("/") or ".." in Path(member_name).parts:
                fail("unsafe path in the " + name + " archive: " + member_name)
            if member.issym() or member.islnk():
                fail("link in the " + name + " archive: " + member_name)
            if Path(member_name).parts[:1] == ("samples",):
                # Upstream sample media (Elephants Dream, skyboxes, Bunny, Boing Ball)
                # is not needed to build or relink the library and is not redistributed.
                continue
            if name == "SceShaccCgExt" and member_name == "include/sce_intrinsics.h":
                continue
            archive.extract(member, destination)

    if name == "SceShaccCgExt":
        subprocess.run([sys.executable, str(Path(__file__).resolve().parent / "vita_shacccg_no_prelude.py"),
                        "--apply", str(destination)], check=True)
        if (destination / "include/sce_intrinsics.h").exists():
            fail("excluded intrinsic header survived source export")
    if (destination / "samples").exists():
        fail("excluded sample media survived source export: " + name)


def library_member_records(pin, archive_path, objcopy, selection_members=None):
    """Return the member identity records for one library pin."""
    members = read_archive_members(archive_path)
    records = []
    if pin["archive"] == ARCHIVE:
        if not selection_members:
            fail("the release selection has no SceShaccCgExt members")
        for entry in selection_members:
            name = entry["member"]
            if name not in members:
                fail("pinned SceShaccCgExt member is absent from the release archive: " + name)
            data = members[name]
            if entry.get("bytes") not in (None, len(data)):
                fail("pinned SceShaccCgExt member size does not match: " + name)
            if entry["sha256"] != hashlib.sha256(data).hexdigest():
                fail("pinned SceShaccCgExt member bytes do not match: " + name)
            records.append({"member": name, "bytes": len(data), "sha256": entry["sha256"],
                            "sha256_stripped": hashlib.sha256(
                                strip_debug(data, objcopy)).hexdigest()})
        pin["members_total"] = len(members)
        return records
    remove_comment = pin.get("identity") == "stripped-comment"
    ident = pin.get("identity", "raw")
    available = {normalise_member(name): name for name in members}
    for entry in pin["members"]:
        name = entry["member"]
        actual = available.get(normalise_member(name))
        if actual is None:
            fail("pinned %s member is absent from the release archive: %s"
                 % (pin["name"], name))
        data = members[actual]
        if entry.get("bytes") not in (None, len(data)):
            fail("pinned %s member size does not match: %s" % (pin["name"], name))
        if entry.get("sha256") and entry["sha256"] != hashlib.sha256(data).hexdigest():
            fail("pinned %s member bytes do not match: %s" % (pin["name"], name))
        stripped = hashlib.sha256(strip_debug(data, objcopy, remove_comment)).hexdigest()
        if entry.get("sha256_stripped") and entry["sha256_stripped"] != stripped:
            fail("pinned %s stripped member bytes do not match: %s" % (pin["name"], name))
        records.append({"member": name, "bytes": len(data),
                        "sha256": hashlib.sha256(data).hexdigest(),
                        "sha256_stripped": stripped,
                        "identity": ident})
    pin["members_total"] = len(members)
    return records


def copy_assets(repo, bundle):
    lines = []
    for relative in ASSET_FILES:
        source = repo / "vita" / relative
        if not source.is_file():
            fail("missing LiveArea asset: " + relative)
        destination = bundle / "assets" / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
        lines.append("%s  %s" % (sha256(destination), relative))
    (bundle / "link/assets.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_readme(bundle, facts):
    text = README_TEMPLATE
    for key, value in facts.items():
        text = text.replace("@@%s@@" % key.upper(), str(value))
    (bundle / "README.md").write_text(text, encoding="utf-8")


def read_vitagl_record(build_text):
    """Return the vitaGL archive hash, package and make flags the release used."""
    archive = VITAGL_LINE_RE.search(build_text)
    recipe = VITAGL_RECIPE_RE.search(build_text)
    if not archive or not recipe:
        fail("release BUILD.txt does not record the vitaGL archive and recipe")
    return {"archive": archive.group(1), "package": archive.group(2),
            "flags": recipe.group(1).strip()}


def append_build_recipe(path, facts):
    text = BUILD_RECIPE_TEMPLATE
    for key, value in facts.items():
        text = text.replace("@@%s@@" % key.upper(), str(value))
    with open(path, "a", encoding="utf-8") as stream:
        stream.write(text)


def write_scripts(bundle):
    for name, text in (("relink.sh", RELINK_TEMPLATE), ("package.sh", PACKAGE_TEMPLATE)):
        path = bundle / "link" / name
        path.write_text(text, encoding="utf-8")
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
    env = dict(os.environ, COPYFILE_DISABLE="1")  # no AppleDouble members on macOS
    subprocess.run(["tar", "-czf", str(tarball), "-C", str(bundle.parent), bundle.name],
                   check=True, env=env)
    return tarball


def write_source_offer(bundle, facts):
    text = SOURCE_OFFER_TEMPLATE
    for key, value in facts.items():
        text = text.replace("@@%s@@" % key.upper(), str(value))
    (bundle / "SOURCE-OFFER.txt").write_text(text, encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release-build", required=True, type=Path,
                        help="directory written by tools/vita_release_build.sh")
    parser.add_argument("--capture", required=True, type=Path,
                        help="linker wrapper capture directory with argv-1.txt and objects/")
    parser.add_argument("--target-root", required=True, type=Path,
                        help="Cargo target directory prefix used by the release build (may be deleted)")
    parser.add_argument("--lib-dir", required=True, type=Path,
                        help="directory that held the pinned SceShaccCgExt archive")
    parser.add_argument("--shacccg-source", required=True, type=Path,
                        help="clean SceShaccCgExt checkout at the pinned revision")
    parser.add_argument("--vitagl-source", required=True, type=Path,
                        help="clean vitaGL checkout at the pinned revision")
    parser.add_argument("--vitashark-source", required=True, type=Path,
                        help="clean vitaShaRK checkout at the pinned revision")
    parser.add_argument("--pthread-source", required=True, type=Path,
                        help="clean pthread-embedded checkout at the pinned revision")
    parser.add_argument("--sdk", type=Path,
                        help="Vita SDK with the release archives (default: $VITASDK)")
    parser.add_argument("--out", required=True, type=Path,
                        help="bundle directory to create")
    args = parser.parse_args()

    repo = Path(__file__).resolve().parent.parent
    try:
        libraries, pin = load_pins(repo)
        release = absolute(args.release_build)
        capture = absolute(args.capture)
        target_root = absolute(args.target_root)
        lib_dir = absolute(args.lib_dir)
        sources = {
            "SceShaccCgExt": absolute(args.shacccg_source),
            "vitaGL": absolute(args.vitagl_source),
            "vitaShaRK": absolute(args.vitashark_source),
            "pthread-embedded": absolute(args.pthread_source),
        }
        sdk = absolute(args.sdk or os.environ.get("VITASDK", ""))
        bundle = absolute(args.out)
        for path in (release, capture, lib_dir, *sources.values()):
            if not path.is_dir():
                fail("not a directory: " + str(path))
        if not sdk.is_dir():
            fail("--sdk or VITASDK must name the Vita SDK")
        if not target_root.is_absolute():
            fail("--target-root must be an absolute path")
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
        self_hashes = SELF_HASH_RE.findall(build_text)
        if len(set(self_hashes)) != 1:
            fail("release BUILD.txt does not name one eboot.bin hash")
        self_hash = self_hashes[0]
        vitagl = read_vitagl_record(build_text)
        selection = json.loads((release / "library-selection.json").read_text(encoding="utf-8"))
        shacccg_pin = libraries[ARCHIVE]
        if selection["revision"] != shacccg_pin["revision"] or selection["license"] != shacccg_pin["license"]:
            fail("release library selection does not match the source pin")
        if selection["archive_sha256"] != shacccg_pin["archive_sha256"]:
            fail("release library archive does not match the pinned bytes")
        if not selection.get("members"):
            fail("release library selection has no contributing members")
        recorded = absolute(selection["archive"].replace("~", str(Path.home()), 1))
        if recorded.parent != lib_dir:
            fail("--lib-dir does not match the directory recorded in library-selection.json")
        if (lib_dir / ARCHIVE).is_file() and sha256(lib_dir / ARCHIVE) != shacccg_pin["archive_sha256"]:
            fail("the release library archive no longer matches its pin")

        sdk_archives = {}
        for archive in ("libvitaGL.a", "libvitashark.a", "libpthread.a"):
            path = sdk / "arm-vita-eabi/lib" / archive
            if not path.is_file():
                fail("the SDK is missing " + str(path))
            sdk_archives[archive] = path
        sdk_archives[ARCHIVE] = lib_dir / ARCHIVE
        if not sdk_archives[ARCHIVE].is_file():
            fail("missing pinned SceShaccCgExt archive: " + str(sdk_archives[ARCHIVE]))

        objcopy = sdk / "bin/arm-vita-eabi-objcopy"
        if not objcopy.is_file():
            fail("the SDK is missing " + str(objcopy))
        member_records = {}
        for archive in LIBRARY_ORDER:
            member_records[archive] = library_member_records(
                libraries[archive], sdk_archives[archive], objcopy, selection["members"]
                if archive == ARCHIVE else None)

        argv = read_capture(capture)
        library_flags = ["-lvitaGL", "-lvitashark", "-lSceShaccCgExt", "-lpthread"]
        mapped, copies, empty_dirs = classify(argv, target_root, lib_dir, sdk, release,
                                              capture, library_flags)

        bundle.mkdir(parents=True, exist_ok=True)
        (bundle / "link").mkdir()
        (bundle / "licenses").mkdir()
        (bundle / "lib").mkdir()
        (bundle / "output").mkdir()
        (bundle / "sources").mkdir()
        copied_bytes = copy_objects(capture, copies, bundle)
        for relative in empty_dirs:
            (bundle / "objects" / relative).mkdir(parents=True, exist_ok=True)
        if not (bundle / "objects").is_dir():
            fail("no object code was copied")

        shutil.copy2(repo / "tools" / pin["patch"], bundle / "SceShaccCgExt-no-prelude.patch")
        licence_texts = {}
        source_lines = []
        for archive in LIBRARY_ORDER:
            library = libraries[archive]
            destination = bundle / "sources" / library["name"]
            extract_source(sources[library["name"]], library["revision"], destination,
                           library["name"])
            tree = subprocess.run(["git", "-C", str(sources[library["name"]]), "rev-parse",
                                   library["revision"] + "^{tree}"],
                                  check=True, stdout=subprocess.PIPE, text=True).stdout.strip()
            files = sum(1 for path in destination.rglob("*") if path.is_file())
            source_lines.append("%s  %s  upstream_tree=%s  files=%d"
                                % (library["name"], library["revision"], tree, files))
            if library["name"] == "SceShaccCgExt":
                source_lines[-1] += "  patch_sha256=" + pin["patch_sha256"]
            for entry in library["license_files"]:
                path = destination / entry["path"]
                if not path.is_file():
                    fail("%s source has no %s" % (library["name"], entry["path"]))
                if sha256(path) != entry["sha256"]:
                    fail("%s %s does not match its pin" % (library["name"], entry["path"]))
            for mapping in library["bundle_licenses"]:
                path = destination / mapping["source"]
                if not path.is_file():
                    fail("%s source has no %s" % (library["name"], mapping["source"]))
                target = bundle / "licenses" / mapping["name"]
                data = path.read_bytes()
                if target.exists() and target.read_bytes() != data:
                    fail("two licence files disagree on " + mapping["name"])
                target.write_bytes(data)
                licence_texts[mapping["name"]] = hashlib.sha256(data).hexdigest()
        if not (bundle / "licenses/LGPL-2.1.txt").is_file():
            fail("the bundle has no LGPL-2.1 text")
        if not (bundle / "licenses/LGPL-3.0.txt").is_file():
            fail("the bundle has no LGPL-3.0 text")
        for name, source_name in PACKAGE_LICENCES:
            source = repo / "tools/vita_licenses" / source_name
            if not source.is_file():
                fail("missing licence text: " + str(source))
            target = bundle / "licenses" / name
            if target.exists() and target.read_bytes() != source.read_bytes():
                fail("two licence files disagree on " + name)
            if not target.exists():
                shutil.copy2(source, target)
        (bundle / "link/sources.txt").write_text("\n".join(source_lines) + "\n",
                                                 encoding="utf-8")
        credits = repo / "CREDITS.txt"
        if not credits.is_file():
            fail("missing CREDITS.txt")
        shutil.copy2(credits, bundle / "CREDITS.txt")
        copy_assets(repo, bundle)

        for name, destination in (("link.map", "link/link.map"),
                                  ("BUILD.txt", "link/BUILD.txt"),
                                  ("library-selection.json", "link/library-selection.json"),
                                  ("libraries.txt", "link/libraries.txt")):
            if (release / name).is_file():
                shutil.copy2(release / name, bundle / destination)
        notices = repo / "THIRD_PARTY_NOTICES-vita.txt"
        if not notices.is_file():
            notices = release / "THIRD_PARTY_NOTICES-vita.txt"
        if notices.is_file():
            shutil.copy2(notices, bundle / "licenses/THIRD_PARTY_NOTICES-vita.txt")
        (bundle / "link/args.txt").write_text("\n".join(mapped) + "\n", encoding="utf-8")
        (bundle / "link/release-elf.sha256").write_text(elf_hash + "\n", encoding="utf-8")
        (bundle / "link/release-self.sha256").write_text(self_hash + "\n", encoding="utf-8")
        (bundle / "link/pinned-library.sha256").write_text(shacccg_pin["archive_sha256"] + "\n",
                                                           encoding="utf-8")
        members = []
        for archive in LIBRARY_ORDER:
            default_identity = "raw" if archive == ARCHIVE else libraries[archive]["identity"]
            for record in member_records[archive]:
                identity = record.get("identity", default_identity)
                member_hash = record["sha256"] if identity == "raw" else record["sha256_stripped"]
                members.append("%s  %s  %s  %s" % (archive, record["member"], member_hash,
                                                   identity))
        (bundle / "link/library-members.txt").write_text("\n".join(members) + "\n",
                                                         encoding="utf-8")
        library_lines = []
        for archive in LIBRARY_ORDER:
            library = libraries[archive]
            library_lines.append("%s  %s  %s  %s  members=%d/%d  archive_sha256=%s"
                                 % (archive, library["name"], library["revision"],
                                    library["license"], len(member_records[archive]),
                                    library["members_total"], sha256(sdk_archives[archive])))
        (bundle / "link/libraries-linked.txt").write_text("\n".join(library_lines) + "\n",
                                                          encoding="utf-8")
        (bundle / "link/link-map.sha256").write_text(
            sha256(bundle / "link/link.map") + "  link.map\n", encoding="utf-8")
        inputs = []
        for relative in copies:
            path = bundle / "objects" / relative
            inputs.append("%s  %10d  %s" % (sha256(path), path.stat().st_size,
                                            relative.as_posix()))
        (bundle / "link/inputs.txt").write_text("\n".join(inputs) + "\n", encoding="utf-8")
        facts = {
            "head": head,
            "elf": elf_hash,
            "self": self_hash,
            "bundle_name": bundle.name + ".tar.gz",
            "library": shacccg_pin["archive_sha256"],
            "revision": shacccg_pin["revision"],
            "objects": len(copies),
            "bytes": copied_bytes,
            "vitagl_revision": VITAGL_REVISION,
            "vitagl_short": VITAGL_REVISION[:7],
            "vitagl_flags": vitagl["flags"],
            "vitagl_archive": vitagl["archive"],
            "vitagl_package": vitagl["package"],
            "vitashark_revision": libraries["libvitashark.a"]["revision"],
            "pthread_revision": libraries["libpthread.a"]["revision"],
            "vdpm_packages": VDPM_PACKAGES,
        }
        write_source_offer(bundle, facts)
        write_scripts(bundle)
        append_build_recipe(bundle / "link/BUILD.txt", facts)
        write_readme(bundle, facts)
        check_bundle(bundle, mapped, ("eboot.bin", "looking-glass.vpk", "looking-glass.self"))
        sums = write_sums(bundle)
        tarball = make_tarball(bundle)
        print("PASS bundle %s" % bundle)
        print("PASS files %d, sha256 entries %d" % (sum(1 for p in bundle.rglob("*") if p.is_file()), len(sums)))
        print("PASS tarball %s %s %d bytes" % (tarball, sha256(tarball), tarball.stat().st_size))
        print("release head %s" % head)
        print("release elf sha256 %s" % elf_hash)
        print("release eboot.bin sha256 %s" % self_hash)
        for archive in LIBRARY_ORDER:
            print("library %s members %d/%d sha256 %s"
                  % (archive, len(member_records[archive]), libraries[archive]["members_total"],
                     sha256(sdk_archives[archive])))
        return 0
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError, tarfile.TarError) as error:
        print("FAIL LGPL bundle: " + str(error), file=sys.stderr)
        return 1


README_TEMPLATE = """# Alice (Looking Glass Vita port) source and relink bundle

This bundle carries the source, licence and relink material for the four GNU
LGPL libraries that the PlayStation Vita release of the Looking Glass port of
American McGee's Alice links statically:

| Library | Revision | Licence |
| --- | --- | --- |
| vitaGL | `@@VITAGL_REVISION@@` | LGPL-3.0-or-later |
| vitaShaRK | `@@VITASHARK_REVISION@@` (tag v.1.7) | LGPL-3.0-or-later |
| pthread-embedded (VitaSDK fork) | `@@PTHREAD_REVISION@@` | LGPL-2.1; the Vita platform layer is MIT |
| SceShaccCgExt | `@@REVISION@@` | LGPL-2.1 |

- Release build head: `@@HEAD@@`
- Release ELF `looking-glass.elf` SHA-256: `@@ELF@@`
- Release `eboot.bin` SHA-256: `@@SELF@@`
- Link inputs shipped: @@OBJECTS@@ files, @@BYTES@@ bytes
- Bundle tarball: `@@BUNDLE_NAME@@`

## Contents

- `sources/vitaGL/`, `sources/vitaShaRK/`, `sources/pthread-embedded/`,
  `sources/SceShaccCgExt/` - the complete source tree of each library at the
  revision the release linked, exported from Git, without the upstream
  `samples/` media that is not needed to build or relink. Each tree carries its
  upstream licence files. SceShaccCgExt omits the optional intrinsic prelude and
  its load (modified 2026-11-15); see `SceShaccCgExt-no-prelude.patch`.
- `licenses/` - the full licence texts shipped in the package: GPL-3.0,
  LGPL-3.0, LGPL-2.1, the GCC Runtime Library Exception 3.1, MPL-2.0, MIT,
  Apache-2.0, the newlib BSD texts, the newlib Vita glue notice, the
  per-library texts of vitaShaRK and pthread-embedded, and
  `THIRD_PARTY_NOTICES-vita.txt` for the whole link.
- `objects/` - every object file and static archive that the release link read
  from the Cargo target directory. These are the port's own compiled code,
  including the Rust standard library and third-party Rust crates that form the
  work that uses the libraries. They are the exact bytes of the release link;
  `link/inputs.txt` lists each file with its size and SHA-256.
- `link/args.txt` - the exact linker command, one argument per line, with three
  placeholders: `@BUNDLE@` is this directory, `@SDK@` is `$VITASDK`, and
  `@OUT@` is the output ELF.
- `link/relink.sh` - runs that command against any archives placed in `lib/`.
  The default mode is a functional relink; `--strict` requires byte identity
  with the release ELF. See "Relink modes and exit codes".
- `link/package.sh` - turns a relinked ELF into `eboot.bin`, `param.sfo` and a
  VPK that carries the notices and licence texts. See "Package and install".
- `link/link.map` - the release linker map, with the build machine's home
  directory shortened to `~`.
- `link/BUILD.txt`, `link/library-selection.json`, `link/libraries.txt`,
  `link/libraries-linked.txt`, `link/sources.txt` - release build facts:
  toolchain, flags, SDK components, the archives and members the linker
  actually loaded, and the Git tree identity of each source export.
- `link/release-elf.sha256`, `link/release-self.sha256`,
  `link/link-map.sha256`, `link/pinned-library.sha256`,
  `link/library-members.txt`, `link/inputs.txt`, `link/assets.txt` - the
  identities used by `relink.sh` and `package.sh`. The member list is the
  deciding identity: a rebuild of the same source can change the archive file
  hash because `ar` records member timestamps, but its object bytes stay the
  same. For vitaGL, vitaShaRK and pthread-embedded the check uses the member
  bytes after `objcopy --strip-debug`, because their build records debug paths
  and the release link passes `-Wl,--strip-debug`; for SceShaccCgExt it uses the
  raw member bytes, as the release tooling pins them.
- `assets/sce_sys/` - the original LiveArea artwork of this port, needed by
  `link/package.sh`. It is project artwork, not game data.
- `CREDITS.txt` - the attribution shipped at the VPK root for the linked files
  that state no licence (the newlib Vita glue and the red-black tree copied into
  SceShaccCgExt). It is an accepted risk recorded by the owner on 2026-11-15,
  not a licence grant; see its text and `licenses/THIRD_PARTY_NOTICES-vita.txt`.
- `SOURCE-OFFER.txt` - the written offer and pointers for the source and relink
  material.
- `SHA256SUMS` - SHA-256 of every file in this bundle except itself.
- `lib/` - empty on purpose. Put any rebuilt archive to relink against here:
  `libvitaGL.a`, `libvitashark.a`, `libSceShaccCgExt.a` or `libpthread.a`.
  One, several or all four may be present.

## What is not in this bundle

- No game data, no pack, texture, sound or movie.
- No C library binary under another licence. The other archives in the link are
  the VitaSDK ones (newlib, libstdc++, libgcc, the Sce stub archives, taiHEN
  and math-neon), rebuildable from public sources. The recipe below builds the
  four LGPL libraries from the source trees in this bundle and the rest from
  public vdpm packages. `link/BUILD.txt` records the exact SDK components and
  archive SHA-256 values the release used.
- The Rust object code in `objects/` includes third-party crates; their licence
  texts are in `licenses/THIRD_PARTY_NOTICES-vita.txt`.

## Build the third-party dependencies from public sources

The link reads four LGPL libraries plus the SDK archives. All four LGPL
libraries are in `sources/`, so a relink needs only a public Vita SDK and a C
toolchain. The SDK archives come from public vdpm packages.

Prerequisites (host):

- macOS 13 or later (arm64 or x86_64) or 64-bit Linux. vdpm publishes clients
  for `arm64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-linux-gnu`,
  `aarch64-linux-gnu`, `x86_64-linux-musl`, `aarch64-linux-musl`, FreeBSD and
  Windows x86_64.
- `git`, `curl`, `tar`, `make`, a POSIX shell and Python 3. On Debian or
  Ubuntu: `sudo apt install build-essential git curl python3 cmake`. On
  macOS, `xcode-select --install` provides git, make and tar; `curl` and
  `python3` are part of the system.
- CMake 3.10 or newer (`sources/SceShaccCgExt/CMakeLists.txt` requires 3.10).
  CMake 4 also needs `-DCMAKE_POLICY_VERSION_MINIMUM=3.5` for the
  SceShaccCgExt build below. The release used CMake 4.4.4.
- Network access to github.com and vitasdk.org, and about 2 GB free.

Run the commands of steps 1 to 6 in order, in one shell, starting in the
extracted bundle directory. Steps 1 and 3 to 5 work in subshells and clone into
`$BUNDLE/deps`, so the shell stays in the bundle directory for step 6.

### Step 1: install a clean VitaSDK with vdpm

```sh
BUNDLE=$(pwd)   # the extracted bundle directory
mkdir -p "$BUNDLE/deps"
(cd "$BUNDLE/deps" && git clone https://github.com/vitasdk/vdpm && cd vdpm &&
  VITASDK_CHANNEL=2026.08 ./bootstrap-vitasdk.sh --install-dir "$HOME/vitasdk-clean")
export VITASDK=$HOME/vitasdk-clean
export PATH=$VITASDK/bin:$PATH
```

The bootstrap installs the newest supported SDK series as vdpm packages. The
release used series `2026.08`; `link/BUILD.txt` records its exact `sdk-build`
and `sdk-component` revisions. Drop `VITASDK_CHANNEL` to take the series that
is supported today. A newer series still links, but its archives differ and
the ELF will not be byte-identical.

### Step 2: install the vdpm packages the link needs

```sh
"$VITASDK/bin/vdpm" install @@VDPM_PACKAGES@@
```

`vdpm install` asks `Proceed with installation? [Y/n]` and needs a `Y`. In a
non-interactive script, feed it with `< <(yes Y)` (bash).

`taihen` supplies `libtaihen_stub.a`, `vitaShaRK` supplies `libvitashark.a`,
and `libmathneon` supplies `libmathneon.a`. Steps 4 and 5 rebuild vitaShaRK and
pthread-embedded from the source trees in this bundle and install them over the
packaged archives; that is the source path the licence asks for. The `vitaGL`
package is not installed on purpose: step 3 replaces it with the release
revision.

### Step 3: build vitaGL at the release revision

```sh
(cd "$BUNDLE/deps" && cp -R "$BUNDLE/sources/vitaGL" vitaGL && cd vitaGL &&
  make clean &&
  make CFLAGS="-g -Wl,-q -O3 -ffast-math -mtune=cortex-a9 -mfpu=neon \\
    -Wno-incompatible-pointer-types -Wno-stringop-overflow -mfp16-format=ieee \\
    -DVGL_GIT_HASH='\\"@@VITAGL_SHORT@@\\"' -Isource \\
    -DDISABLE_TEXTURE_COMBINER -DHAVE_UNFLIPPED_FBOS \\
    -DHAVE_SINGLE_THREADED_GC -DHAVE_CUSTOM_HEAP" &&
  make install)      # libvitaGL.a and source/vitaGL.h into $VITASDK
```

The release linked `libvitaGL.a` SHA-256 `@@VITAGL_ARCHIVE@@`; the installed
vdpm package was `@@VITAGL_PACKAGE@@`, but the release used a local build of
the revision above. The archive does not record its compile flags, so the
flags above are the documented ones from `link/BUILD.txt`. The release's make
flags were `@@VITAGL_FLAGS@@`; the `CFLAGS` above carries their four
equivalent `-D` macros. `make install` needs `VITASDK` exported.

The revision has to be passed explicitly. `Makefile:16` fills
`VGL_GIT_HASH` from `git rev-parse --short HEAD`, and
`source/splashscreen.c:6266` renders `"#" VGL_GIT_HASH` into the splash
string. The exported tree has no `.git`, and this Makefile has no
`VGL_GIT_HASH` make variable (`make VGL_GIT_HASH=...` has no effect), so
`CFLAGS` names the revision as `-DVGL_GIT_HASH='"@@VITAGL_SHORT@@"'`. The
command-line `CFLAGS` also overrides the Makefile's feature `+=` lines, so the
four feature macros are spelled out in the same value. Without this, the only
difference is that `.rodata.str1.4` starts with `#` instead of
`#@@VITAGL_SHORT@@`.

The Makefile compiles with `-g`, and `ar` stores member timestamps, so the
archive file hash depends on the directory and on when it was built. A rebuild
therefore has a different file SHA-256 although the compiled code is the same.
The release link passes `-Wl,--strip-debug`, and `relink.sh` compares member
bytes after `objcopy --strip-debug`. Measured on the release host with the
command above, all 30 members of the exported-source build, including
`splashscreen.o`, are identical after stripping. Relinking those members with
the three other release archives reproduces the release ELF, `@@ELF@@`, byte
for byte. A build from a different SDK series still produces different bytes
for the other SDK archives and is not claimed to be identical.

### Step 4: build vitaShaRK at the release revision

```sh
(cd "$BUNDLE/deps" && git clone https://github.com/Rinnegatamante/vitaShaRK && cd vitaShaRK &&
  git checkout --detach @@VITASHARK_REVISION@@ &&
  test "$(git rev-parse HEAD)" = @@VITASHARK_REVISION@@ &&
  make &&
  make install)      # libvitashark.a and source/vitashark.h into $VITASDK
```

The release linked `libvitashark.a` from vdpm package `vitaShaRK-1.7-1`, built
from tag `v.1.7` at the revision above. A source build has a different archive
file hash because `ar` records timestamps; the single member `vitashark.o` is
identical after `objcopy --strip-debug` (measured on the release host).

### Step 5: build pthread-embedded at the release revision

```sh
(cd "$BUNDLE/deps" && git clone https://github.com/vitasdk/pthread-embedded && cd pthread-embedded &&
  git checkout --detach @@PTHREAD_REVISION@@ &&
  test "$(git rev-parse HEAD)" = @@PTHREAD_REVISION@@ &&
  make -C platform/vita CC=arm-vita-eabi-gcc CXX=arm-vita-eabi-g++ \\
    AR=arm-vita-eabi-ar GLOBAL_CFLAGS=-Wno-error=int-conversion &&
  "$VITASDK/bin/arm-vita-eabi-objcopy" --strip-debug -R .comment -R .note \\
    platform/vita/libpthread.a &&
  cp platform/vita/libpthread.a "$VITASDK/arm-vita-eabi/lib/")
```

This is the VitaSDK build recipe: the `platform/vita/Makefile` names the 134
archive members, and `GLOBAL_CFLAGS=-Wno-error=int-conversion` is the flag the
VitaSDK buildscripts pass. The `objcopy --strip-debug -R .comment -R .note`
step is what VitaSDK applies to every target object and archive; without it the
archive carries `.comment` and debug sections that the release archive does not.
After it, the 54 members this link loads are byte-identical to the release
archive (measured on the release host). `relink.sh` applies the same stripping
when it compares members, so a rebuild that skips the last `objcopy` still
matches.

### Step 6: build SceShaccCgExt from the source in this bundle

```sh
cd "$BUNDLE"
cmake sources/SceShaccCgExt -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \\
  -DCMAKE_C_FLAGS="-std=gnu11 -Wno-error=implicit-function-declaration -Wno-error=int-conversion -Wno-error=incompatible-pointer-types" \\
  -DCMAKE_CXX_FLAGS="-std=gnu++11 -Wno-error=implicit-function-declaration -Wno-error=int-conversion -Wno-error=incompatible-pointer-types"
make
mkdir -p lib
cp libSceShaccCgExt.a lib/
```

The archive file hash can differ from the pinned `@@LIBRARY@@` because `ar`
stores member timestamps. The member object bytes decide identity:
`link/library-members.txt` holds their SHA-256 and `relink.sh` compares those.

With the pinned release archives and GCC 15.2.0 the relink reproduces the
release ELF byte for byte, as the proof below shows. The full source recipe in
steps 3 to 6 rebuilds each library from public source. The vitaGL step passes
`VGL_GIT_HASH` explicitly, so the exported-source build reproduces all 30
vitaGL members, including `splashscreen.o`. `link/BUILD.txt` names the
archives that produced the release. Across SDK series the other archives still
link but their bytes differ, so this bundle does not claim byte identity from
a rebuild under a different SDK series.

## Relink modes and exit codes

`link/relink.sh` has two modes:

- Default: functional relink. A successful link exits 0. The script prints the
  ELF SHA-256 and states whether it is byte-identical to the release ELF or
  differs. A difference is a warning, not a failure, because other SDK
  archives are allowed to differ.
- Strict: release reproduction. `--strict` or `LG_RELINK_STRICT=1` exits 1
  unless the output is byte-identical to the release ELF.

Place a rebuilt archive at `lib/libvitaGL.a`, `lib/libvitashark.a`,
`lib/libSceShaccCgExt.a` or `lib/libpthread.a`. Any combination works: each
present archive replaces the SDK one for its library, and each is checked
against its own pinned member list. A link that does not load a supplied
archive, or that also loads the SDK archive of the same name, exits non-zero in
both modes. A failed link or a missing archive exits non-zero in both modes.
The script does not claim byte identity when it did not check it.

## Relink against the unmodified libraries

1. Complete steps 1 to 6 above, or use the SDK and archives that produced the
   release (`link/BUILD.txt`).
2. `VITASDK=/path/to/vitasdk ./link/relink.sh output/looking-glass.elf`
3. With the pinned member objects and the release SDK the output is
   `@@ELF@@`, byte for byte the release ELF. The script prints `PASS` and
   exits 0.
4. `VITASDK=/path/to/vitasdk ./link/relink.sh --strict output/strict.elf`
   additionally fails on any difference.

## Relink against a modified library

1. Modify a source tree in `sources/` as you wish, or use your own copy.
2. Build it as in steps 3 to 6 and put the new archive in `lib/` under its
   release name.
3. Run `./link/relink.sh output/modified.elf`. The output is a new ELF that
   contains your modified library. Its hash differs from the release; the
   script says so and exits 0 because the link succeeded. `--strict` exits 1,
   which is the point of the mode split.
4. Package and install it as below.

## Package and install (LGPL-3.0 section 4(e) installation information)

`link/package.sh` turns an ELF into the installable package with the same
VitaSDK tools and flags the release used:

```sh
VITASDK=/path/to/vitasdk ./link/package.sh output/looking-glass.elf
```

It runs, in this order:

1. `vita-elf-create output/looking-glass.elf output/looking-glass.velf`
2. `vita-make-fself -s output/looking-glass.velf output/eboot.bin`
3. `vita-mksfoex -d ATTRIBUTE2=12 -s APP_VER=01.00 -s TITLE_ID=ALICELG01
   "American McGee's Alice" output/param.sfo`
4. `vita-pack-vpk -s output/param.sfo -b output/eboot.bin` with the four
   LiveArea assets from `assets/`, plus `THIRD_PARTY_NOTICES-vita.txt`,
   `CREDITS.txt`, `SOURCE-OFFER.txt` and every file in `licenses/` at the VPK
   root and under `licenses/`, producing `output/looking-glass.vpk`.

These are the exact commands and flags from the release build log and
`link/BUILD.txt`. The script prints the ELF, `eboot.bin` and VPK SHA-256
values. With the release ELF it prints the release `eboot.bin` hash
`@@SELF@@`; a modified library produces a different one, which is expected.

To install: copy `output/looking-glass.vpk` to the Vita (VitaShell over USB or
FTP is the usual route) and open it in VitaShell to install it as
`ALICELG01`. The game data is not part of the package; players copy their own
files to `ux0:data/alice/base/`. The shader compiler `libshacccg.suprx` is
loaded from the console and is not distributed. Installing a relinked package
replaces the installed application; uninstall the old one first if VitaShell
asks. This bundle does not test the installation on a console.

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
# Usage: VITASDK=/path/to/vitasdk ./link/relink.sh [--strict] [OUTPUT_ELF]
#
# Default mode is a functional relink: the link must succeed and the script
# states whether the ELF equals the release ELF, but a difference is not an
# error. --strict (or LG_RELINK_STRICT=1) is release reproduction: the script
# exits 1 unless the output is byte-identical to the release ELF.
#
# Put any rebuilt LGPL archive in lib/: libvitaGL.a, libvitashark.a,
# libSceShaccCgExt.a or libpthread.a. One, several or all four may be present;
# each present archive replaces the SDK one and is checked against its own
# pinned member list.
set -eu
strict=0
output=
for argument in "$@"; do
    case $argument in
        --strict) strict=1 ;;
        -h|--help)
            echo "usage: VITASDK=/path/to/vitasdk $0 [--strict] [OUTPUT_ELF]"
            exit 0
            ;;
        -*)
            echo "unknown option: $argument" >&2
            exit 2
            ;;
        *)
            if [ -n "$output" ]; then
                echo "usage: $0 [--strict] [OUTPUT_ELF]" >&2
                exit 2
            fi
            output=$argument
            ;;
    esac
done
case ${LG_RELINK_STRICT:-} in
    ''|0|no|false) ;;
    *) strict=1 ;;
esac
here=$(cd "$(dirname "$0")/.." && pwd)
: "${VITASDK:?set VITASDK to the Vita SDK root}"
sdk=$VITASDK
linker=$sdk/bin/arm-vita-eabi-gcc
[ -x "$linker" ] || { echo "missing $linker" >&2; exit 1; }
archiver=$sdk/bin/arm-vita-eabi-ar
[ -x "$archiver" ] || { echo "missing $archiver" >&2; exit 1; }
objcopy=$sdk/bin/arm-vita-eabi-objcopy
output=${output:-$here/output/looking-glass.elf}
case $output in
    /*) ;;
    *) output=$(pwd)/$output ;;
esac

libraries="libvitaGL.a libvitashark.a libSceShaccCgExt.a libpthread.a"
supplied=
for archive in $libraries; do
    [ -f "$here/lib/$archive" ] && supplied="$supplied $archive"
done
[ -n "$supplied" ] || {
    echo "missing a library archive in $here/lib/;" >&2
    echo "put a rebuilt libvitaGL.a, libvitashark.a, libSceShaccCgExt.a or libpthread.a there" >&2
    exit 1
}
mkdir -p "$(dirname "$output")" "$here/output"

hash_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

normalise_member() {
    case $1 in
        *.c.obj) printf '%s.o\\n' "${1%.c.obj}" ;;
        *.cc.obj) printf '%s.o\\n' "${1%.cc.obj}" ;;
        *.cpp.obj) printf '%s.o\\n' "${1%.cpp.obj}" ;;
        *.cxx.obj) printf '%s.o\\n' "${1%.cxx.obj}" ;;
        *.obj) printf '%s.o\\n' "${1%.obj}" ;;
        *) printf '%s\\n' "$1" ;;
    esac
}

member_name() {
    archive=$1
    wanted=$2
    wanted=$(normalise_member "$wanted")
    "$archiver" t "$archive" | while IFS= read -r name; do
        [ "$(normalise_member "$name")" = "$wanted" ] && { printf '%s\\n' "$name"; break; }
    done
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
all_match=true
for archive in $supplied; do
    library=$here/lib/$archive
    grep -F "$library" "$map" >/dev/null 2>&1 || {
        echo "the link did not load $library" >&2
        exit 1
    }
    if grep -F "$sdk/arm-vita-eabi/lib/$archive" "$map" >/dev/null 2>&1; then
        echo "the link also loaded the SDK $archive" >&2
        exit 1
    fi
done

member_file=$here/output/.member
for archive in $supplied; do
    library=$here/lib/$archive
    match=true
    pinned=0
    while read -r pin_archive member member_hash identity; do
        [ "$pin_archive" = "$archive" ] || continue
        pinned=$((pinned + 1))
        actual=$(member_name "$library" "$member")
        if [ -z "$actual" ]; then
            match=false
            echo "missing member $member in $archive"
            continue
        fi
        if ! "$archiver" p "$library" "$actual" > "$member_file" 2>/dev/null; then
            match=false
            echo "cannot extract member $member from $archive"
            continue
        fi
        case $identity in
            stripped)
                [ -x "$objcopy" ] || { echo "missing $objcopy" >&2; exit 1; }
                "$objcopy" --strip-debug "$member_file" "$member_file.stripped" 2>/dev/null || {
                    match=false
                    echo "cannot strip member $member from $archive"
                    continue
                }
                ;;
            stripped-comment)
                # VitaSDK strips target objects with --strip-debug -R .comment -R .note.
                [ -x "$objcopy" ] || { echo "missing $objcopy" >&2; exit 1; }
                "$objcopy" --strip-debug -R .comment -R .note "$member_file" \\
                    "$member_file.stripped" 2>/dev/null || {
                    match=false
                    echo "cannot strip member $member from $archive"
                    continue
                }
                ;;
            *)
                cp "$member_file" "$member_file.stripped"
                ;;
        esac
        if [ -f "$member_file.stripped" ]; then
            actual_hash=$(hash_file "$member_file.stripped")
            rm -f "$member_file.stripped"
        else
            actual_hash=$(hash_file "$member_file")
        fi
        if [ "$actual_hash" != "$member_hash" ]; then
            match=false
            echo "member $member of $archive differs from the pinned object"
        fi
    done < "$here/link/library-members.txt"
    rm -f "$member_file"
    if [ "$pinned" -eq 0 ]; then
        match=false
        echo "no pinned members for $archive in link/library-members.txt"
    fi
    echo "archive file sha256 $(hash_file "$library")"
    if [ "$match" = true ]; then
        echo "members of $archive match the pinned objects"
    else
        echo "members of $archive differ from the pinned objects"
        all_match=false
    fi
done

actual=$(hash_file "$output")
expected=$(cat "$here/link/release-elf.sha256")
echo "relinked ELF $output"
echo "sha256 $actual"
echo "release sha256 $expected"

if [ "$actual" = "$expected" ]; then
    echo "result byte-identical to the release ELF"
    echo "PASS the relink reproduces the release ELF"
    exit 0
fi
echo "result different from the release ELF"
if [ "$strict" = 1 ]; then
    if [ "$all_match" = true ]; then
        echo "FAIL strict mode: the pinned members are present but the ELF differs;" >&2
        echo "     the other SDK archives in the link are not the release ones" >&2
    else
        echo "FAIL strict mode: one or more archive members differ from the pinned ones" >&2
    fi
    exit 1
fi
if [ "$all_match" = true ]; then
    echo "WARNING the link succeeded but the ELF differs from the release hash;"
    echo "        the other SDK archives in the link are not the release ones."
    echo "        This is a functional relink, not the release ELF."
else
    echo "NOTE the contributing member objects differ from the pinned ones;"
    echo "     this is a modified relink, not the release ELF."
fi
echo "use --strict to require byte identity with the release ELF"
exit 0
"""


PACKAGE_TEMPLATE = """#!/bin/sh
# Turn a relinked ELF into eboot.bin, param.sfo and a VPK that carries the
# third-party notices and licence texts.
#
# Usage: VITASDK=/path/to/vitasdk ./link/package.sh [ELF]
#
# Default input: output/looking-glass.elf. Outputs, in the same directory as
# the ELF: looking-glass.velf, eboot.bin, param.sfo and looking-glass.vpk.
# These are the exact VitaSDK commands and flags of the release build.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
: "${VITASDK:?set VITASDK to the Vita SDK root}"
sdk=$VITASDK
for tool in vita-elf-create vita-make-fself vita-mksfoex vita-pack-vpk; do
    [ -x "$sdk/bin/$tool" ] || { echo "missing $sdk/bin/$tool" >&2; exit 1; }
done
elf=${1:-$here/output/looking-glass.elf}
case $elf in
    /*) ;;
    *) elf=$(pwd)/$elf ;;
esac
[ -f "$elf" ] || { echo "missing ELF $elf" >&2; exit 1; }
out=$(dirname "$elf")
velf=$out/looking-glass.velf
eboot=$out/eboot.bin
sfo=$out/param.sfo
vpk=$out/looking-glass.vpk

hash_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

"$sdk/bin/vita-elf-create" "$elf" "$velf"
"$sdk/bin/vita-make-fself" -s "$velf" "$eboot"
"$sdk/bin/vita-mksfoex" -d ATTRIBUTE2=12 -s APP_VER=01.00 -s TITLE_ID=ALICELG01 \\
    "American McGee's Alice" "$sfo"
set -- -s "$sfo" -b "$eboot"
for asset in sce_sys/icon0.png sce_sys/livearea/contents/startup.png \\
             sce_sys/livearea/contents/template.xml sce_sys/livearea/contents/bg.png; do
    set -- "$@" --add "$here/assets/$asset=$asset"
done
set -- "$@" --add "$here/licenses/THIRD_PARTY_NOTICES-vita.txt=THIRD_PARTY_NOTICES-vita.txt"
[ -f "$here/CREDITS.txt" ] || { echo "missing $here/CREDITS.txt" >&2; exit 1; }
set -- "$@" --add "$here/CREDITS.txt=CREDITS.txt"
set -- "$@" --add "$here/SOURCE-OFFER.txt=SOURCE-OFFER.txt"
for licence in "$here"/licenses/*; do
    [ -f "$licence" ] || continue
    name=$(basename "$licence")
    [ "$name" = "THIRD_PARTY_NOTICES-vita.txt" ] && continue
    set -- "$@" --add "$licence=licenses/$name"
done
"$sdk/bin/vita-pack-vpk" "$@" "$vpk"

echo "ELF $elf"
echo "sha256 $elf $(hash_file "$elf")"
echo "eboot.bin $eboot"
echo "sha256 $eboot $(hash_file "$eboot")"
if [ -f "$here/link/release-self.sha256" ]; then
    release=$(cat "$here/link/release-self.sha256")
    echo "release eboot.bin sha256 $release"
    if [ "$(hash_file "$eboot")" = "$release" ]; then
        echo "result byte-identical to the release eboot.bin"
    else
        echo "result different from the release eboot.bin"
    fi
fi
echo "vpk $vpk"
echo "sha256 $vpk $(hash_file "$vpk")"
echo "install: copy the VPK to the Vita and open it in VitaShell (title ALICELG01)"
"""


SOURCE_OFFER_TEMPLATE = """Source and relink offer for the Looking Glass PS Vita package

This package contains the third-party notices in THIRD_PARTY_NOTICES-vita.txt
and the full licence texts under licenses/. The release build head is
@@HEAD@@ and the ELF it linked hashes @@ELF@@.

The corresponding source for the four GNU LGPL libraries linked into
eboot.bin, and the material needed to relink this build against modified
versions of them, are in the source bundle published with this release:

  @@BUNDLE_NAME@@

It carries, from public repositories at the linked revisions:

  vitaGL            @@VITAGL_REVISION@@  (LGPL-3.0-or-later)
  vitaShaRK         @@VITASHARK_REVISION@@  (LGPL-3.0-or-later, tag v.1.7)
  pthread-embedded  @@PTHREAD_REVISION@@  (LGPL-2.1, MIT Vita layer)
  SceShaccCgExt     @@REVISION@@  (LGPL-2.1)

The bundle also carries the complete object code and rlibs of this build, the
exact linker command (link/args.txt), the release link map, relink.sh,
package.sh and the LiveArea packaging assets. The bundle is published with the
release and is also available on written request.

Written offer: for at least three years from the date this package is
distributed, the maintainer will provide the same source and relink material
to any third party on request through

  https://github.com/Anezium/LookingGlass/issues

at no charge beyond the cost of the physical medium and delivery. Identify the
release by its build head @@HEAD@@ or by the ELF SHA-256 above.

MPL-2.0 source: the four symphonia 0.5.5 crates (symphonia,
symphonia-bundle-mp3, symphonia-core and symphonia-metadata) are linked
unmodified. Their Source Code Form is the crates.io release archive, available
at https://crates.io/api/v1/crates/<crate>/0.5.5/download. Cargo.lock pins these
SHA-256 values:

  symphonia           5773a4c030a19d9bfaa090f49746ff35c75dfddfa700df7a5939d5e076a57039
  symphonia-bundle-mp3 4872dd6bb56bf5eac799e3e957aa1981086c3e613b27e0ac23b176054f7c57ed
  symphonia-core      ea00cc4f79b7f6bb7ff87eddc065a1066f3a43fe1875979056672c9ef948c2af
  symphonia-metadata  36306ff42b9ffe6e5afc99d49e121e0bd62fe79b9db7b9681d48e29fa19e6b16

Original game data and Sony firmware are not included and are not offered.
"""


BUILD_RECIPE_TEMPLATE = """

## Public rebuild recipe

The lines above are the release record. This section is the executable recipe
that rebuilds the third-party inputs from public sources. The full version,
with prerequisites and each command, is in README.md under "Build the
third-party dependencies from public sources".

- Host: macOS 13+ (arm64 or x86_64) or 64-bit Linux with git, curl, tar, make,
  a POSIX shell, Python 3 and CMake 3.10 or newer; about 2 GB free; network
  access to github.com and vitasdk.org.
- Clean SDK: `git clone https://github.com/vitasdk/vdpm`, then
  `VITASDK_CHANNEL=2026.08 ./bootstrap-vitasdk.sh --install-dir <directory>`,
  then `export VITASDK=<directory>` and `export PATH=$VITASDK/bin:$PATH`.
- Packages: `"$VITASDK/bin/vdpm" install @@VDPM_PACKAGES@@`.
- vitaGL: build `sources/vitaGL` (the exported tree at
  @@VITAGL_REVISION@@) with the `make CFLAGS=...` command in README.md step 3,
  then `make install`. The release's make flags were `@@VITAGL_FLAGS@@`.
  The CFLAGS pass `-DVGL_GIT_HASH='"@@VITAGL_SHORT@@"'`
  because the Makefile otherwise reads the hash from Git, which an exported
  tree does not have, and because a command-line CFLAGS also overrides the
  feature flags (`-DDISABLE_TEXTURE_COMBINER -DHAVE_UNFLIPPED_FBOS
  -DHAVE_SINGLE_THREADED_GC -DHAVE_CUSTOM_HEAP`). That is a local build at the
  recorded revision, not the vdpm package file.
- vitaShaRK: `git clone https://github.com/Rinnegatamante/vitaShaRK`, then
  `git checkout --detach @@VITASHARK_REVISION@@`, then `make`, then
  `make install`.
- pthread-embedded: `git clone https://github.com/vitasdk/pthread-embedded`, then
  `git checkout --detach @@PTHREAD_REVISION@@`, then
  `make -C platform/vita ... GLOBAL_CFLAGS=-Wno-error=int-conversion`, then
  `objcopy --strip-debug -R .comment -R .note platform/vita/libpthread.a`, then
  copy it into `$VITASDK/arm-vita-eabi/lib/`.
- SceShaccCgExt: source in this bundle at `@@REVISION@@`; CMake recipe in
  README.md step 6, output `lib/libSceShaccCgExt.a`.
- Expected `libvitaGL.a` SHA-256: `@@VITAGL_ARCHIVE@@`. The archive file does
  not record its compile flags; the flags above are the documented ones.
- Rebuilds have different archive file hashes because `ar` stores member
  timestamps. `link/relink.sh` compares member bytes, stripped with
  `objcopy --strip-debug` (and additionally `-R .comment -R .note` for
  pthread-embedded). With the CFLAGS above, the exported-source vitaGL build
  matches all 30 pinned members, including `splashscreen.o`.
- No input here needs a proprietary SDK binary. The Sce stub archives and the
  C library come from the public vdpm SDK packages.
"""


if __name__ == "__main__":
    sys.exit(main())
