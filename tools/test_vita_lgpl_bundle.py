#!/usr/bin/env python3
"""Fail-closed controls for the LGPL source and relink bundle."""
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile

TOOLS = Path(__file__).resolve().parent
REPO = TOOLS.parent
SPEC = importlib.util.spec_from_file_location("lgpl_bundle", TOOLS / "vita_lgpl_bundle.py")
BUNDLE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUNDLE)
PACKAGE_SPEC = importlib.util.spec_from_file_location("vita_package", TOOLS / "vita_package.py")
PACKAGE = importlib.util.module_from_spec(PACKAGE_SPEC)
PACKAGE_SPEC.loader.exec_module(PACKAGE)

LIBRARY_FLAGS = ["-lvitaGL", "-lvitashark", "-lSceShaccCgExt", "-lpthread"]


def digest(data):
    return hashlib.sha256(data).hexdigest()


class ClassifyControls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.target = self.root / "target"
        self.out = self.target / "armv7-sony-vita-newlibeabihf/release/build/looking-glass/abc/out"
        self.out.mkdir(parents=True)
        self.raw = self.out / "rustcXYZ/raw-dylibs"
        self.raw.mkdir(parents=True)
        self.sdk = self.root / "sdk"
        (self.sdk / "arm-vita-eabi/lib").mkdir(parents=True)
        self.lib = self.root / "isolated-lib"
        self.lib.mkdir()
        (self.lib / BUNDLE.ARCHIVE).write_bytes(b"release library fixture")
        self.release = self.root / "release"
        self.release.mkdir()
        (self.release / "link.map").write_text("release map fixture")
        self.capture = self.root / "capture"
        self.objects = [
            self.out / "looking_glass.core.0.rcgu.o",
            self.out / "looking_glass.zip.1.rcgu.o",
            self.out / "rustcXYZ/libcpal-1.rlib",
        ]
        for path in self.objects:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"object bytes " + path.name.encode())
            copy = self.capture / "objects" / path.relative_to(self.target)
            copy.parent.mkdir(parents=True, exist_ok=True)
            copy.write_bytes(path.read_bytes())
        self.compiler = self.target / "armv7-sony-vita-newlibeabihf/release/build/compiler_builtins/def/out/libcompiler_builtins-1.rlib"
        self.compiler.parent.mkdir(parents=True)
        self.compiler.write_bytes(b"compiler builtins fixture")
        copy = self.capture / "objects" / self.compiler.relative_to(self.target)
        copy.parent.mkdir(parents=True, exist_ok=True)
        copy.write_bytes(self.compiler.read_bytes())
        self.argv = [
            "-Wl,-q",
            str(self.objects[0]),
            "-Wl,-Bstatic",
            str(self.objects[2]),
            str(self.compiler),
            "-lvitaGL",
            "-lvitashark",
            "-lSceShaccCgExt",
            "-lpthread",
            "-L", str(self.raw),
            "-L", str(self.lib),
            "-L", str(self.sdk / "arm-vita-eabi/lib"),
            "-o", str(self.out / "looking_glass.elf"),
            "-Wl,-Map=" + str(self.release / "link.map"),
        ]

    def classify(self, argv=None):
        return BUNDLE.classify(argv or self.argv, self.target, self.lib, self.sdk,
                               self.release, self.capture, LIBRARY_FLAGS)

    def test_exact_command_is_rewritten_and_inputs_listed(self):
        mapped, copies, dirs = self.classify()
        self.assertEqual(mapped[0], "-Wl,-q")
        self.assertEqual(mapped[1], "@BUNDLE@/objects/armv7-sony-vita-newlibeabihf/release/build/looking-glass/abc/out/looking_glass.core.0.rcgu.o")
        self.assertIn("@BUNDLE@/lib", mapped)
        self.assertIn("@SDK@/arm-vita-eabi/lib", mapped)
        self.assertIn("@OUT@", mapped)
        self.assertIn("-Wl,-Map=@BUNDLE@/output/relink.map", mapped)
        self.assertEqual(len(mapped), len(self.argv))
        self.assertEqual(len(copies), 3)
        self.assertIn(Path("armv7-sony-vita-newlibeabihf/release/build/looking-glass/abc/out/rustcXYZ/raw-dylibs"), dirs)

    def test_missing_captured_object_is_refused(self):
        (self.capture / "objects" / self.objects[0].relative_to(self.target)).unlink()
        with self.assertRaisesRegex(ValueError, "missing an input object"):
            self.classify()

    def test_unknown_path_is_refused(self):
        argv = list(self.argv) + [str(self.root / "elsewhere" / "toolchain.o")]
        with self.assertRaisesRegex(ValueError, "neither target, SDK nor release library"):
            self.classify(argv)

    def test_sdk_before_isolated_library_is_refused(self):
        argv = ["-lvitaGL", "-lvitashark", "-lSceShaccCgExt", "-lpthread",
                "-L", str(self.sdk / "arm-vita-eabi/lib"),
                "-L", str(self.lib), "-o", str(self.out / "a.elf"),
                "-Wl,-Map=" + str(self.release / "link.map")]
        with self.assertRaisesRegex(ValueError, "searched before the release library"):
            self.classify(argv)

    def test_every_lgpl_flag_is_required(self):
        for flag in LIBRARY_FLAGS:
            argv = [argument for argument in self.argv if argument != flag]
            with self.assertRaisesRegex(ValueError, "must name " + flag):
                self.classify(argv)

    def test_one_output_one_map_are_required(self):
        argv = []
        skip = False
        for argument in self.argv:
            if argument == "-o":
                skip = True
                continue
            if skip:
                skip = False
                continue
            argv.append(argument)
        with self.assertRaisesRegex(ValueError, "one output and one link map"):
            self.classify(argv)


class CaptureControls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.capture = Path(self.temp.name)
        (self.capture / "count").write_text("1\n")
        (self.capture / "argv-1.txt").write_text("-Wl,-q\n")

    def test_one_link_is_required(self):
        (self.capture / "count").write_text("2\n")
        with self.assertRaisesRegex(ValueError, "exactly one link"):
            BUNDLE.read_capture(self.capture)

    def test_unknown_inputs_are_refused(self):
        (self.capture / "unknown.txt").write_text("/opt/other/toolchain.o\n")
        with self.assertRaisesRegex(ValueError, "outside the target and SDK"):
            BUNDLE.read_capture(self.capture)

    def test_response_file_is_refused(self):
        (self.capture / "response-1.txt").write_text("@args\n")
        with self.assertRaisesRegex(ValueError, "response file"):
            BUNDLE.read_capture(self.capture)


class RelinkControls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bundle = self.root / "bundle"
        for name in ("lib", "link", "objects", "output"):
            (self.bundle / name).mkdir(parents=True)
        (self.bundle / "objects/a.o").write_bytes(b"object")
        self.members = {
            "libSceShaccCgExt.a": {"shacccg_ext.c.obj": b"sce member"},
            "libvitaGL.a": {"buffers.o": b"vitaGL buffers",
                            "splashscreen.o": b"vitaGL splash path bytes"},
            "libvitashark.a": {"vitashark.o": b"vitaShaRK"},
            "libpthread.a": {"pthread_mutex_lock.c.obj": b"pthread lock"},
        }
        self.pin_lines = [
            "libSceShaccCgExt.a  shacccg_ext.c.obj  %s  raw" % digest(b"sce member"),
            "libvitaGL.a  buffers.o  %s  stripped" % digest(b"vitaGL buffersstripped"),
            "libvitashark.a  vitashark.o  %s  stripped" % digest(b"vitaShaRKstripped"),
            "libpthread.a  pthread_mutex_lock.o  %s  stripped-comment" % digest(b"pthread lockstripped"),
            "libvitaGL.a  splashscreen.o  %s  stripped" % digest(b"vitaGL splash path bytesstripped"),
        ]
        self.write_member_manifest(self.pin_lines)
        self.sdk = self.root / "sdk"
        (self.sdk / "arm-vita-eabi/lib").mkdir(parents=True)
        self.linker = self.sdk / "bin/arm-vita-eabi-gcc"
        self.linker.parent.mkdir()
        self.linker.write_text(
            "#!/bin/sh\n"
            "out=\n prev=\n dirs=\n"
            "for a in \"$@\"; do\n"
            "  [ \"$prev\" = -L ] && dirs=\"$dirs $a\"\n"
            "  [ \"$prev\" = -o ] && out=$a\n"
            "  prev=$a\n"
            "done\n"
            ": > \"$MAP\"\n"
            "for d in $dirs; do\n"
            "  for f in \"$d\"/*.a; do\n"
            "    [ -f \"$f\" ] && printf 'LOAD %s\\n' \"$f\" >> \"$MAP\"\n"
            "  done\n"
            "done\n"
            "[ -n \"${FAKE_LINK_FAIL:-}\" ] && exit 1\n"
            "printf '%s' \"${FAKE_ELF:-release elf}\" > \"$out\"\n")
        self.linker.chmod(0o755)
        self.fake_ar = self.root / "fake-ar"
        self.fake_ar.mkdir()
        for archive, members in self.members.items():
            folder = self.fake_ar / archive
            folder.mkdir()
            for member, data in members.items():
                (folder / member).write_bytes(data)
        self.archiver = self.sdk / "bin/arm-vita-eabi-ar"
        self.archiver.write_text(
            "#!/bin/sh\n"
            "case $1 in\n"
            "  t) ls \"$FAKE_AR/$(basename \"$2\")\" ;;\n"
            "  p) [ -f \"$FAKE_AR/$(basename \"$2\")/$3\" ] || exit 1; cat \"$FAKE_AR/$(basename \"$2\")/$3\" ;;\n"
            "  *) exit 2 ;;\n"
            "esac\n")
        self.archiver.chmod(0o755)
        self.objcopy = self.sdk / "bin/arm-vita-eabi-objcopy"
        self.objcopy.write_text(
            "#!%s\n"
            "import sys\n"
            "args = sys.argv[1:]\n"
            "data = open(args[-2], 'rb').read() + b'stripped'\n"
            "open(args[-1], 'wb').write(data)\n" % sys.executable)
        self.objcopy.chmod(0o755)
        self.map = self.bundle / "output/relink.map"
        (self.bundle / "link/args.txt").write_text(
            "-Wl,-q\n@BUNDLE@/objects/a.o\n-L\n@BUNDLE@/lib\n-L\n@SDK@/arm-vita-eabi/lib\n"
            "-o\n@OUT@\n-Wl,-Map=@BUNDLE@/output/relink.map\n")
        (self.bundle / "link/release-elf.sha256").write_text(digest(b"release elf") + "\n")
        (self.bundle / "link/relink.sh").write_text(BUNDLE.RELINK_TEMPLATE)
        (self.bundle / "link/relink.sh").chmod(0o755)

    def write_member_manifest(self, lines):
        (self.bundle / "link/library-members.txt").write_text("\n".join(lines) + "\n")

    def supply(self, *archives):
        for archive in archives:
            (self.bundle / "lib" / archive).write_bytes(b"fixture " + archive.encode())

    def run_relink(self, *arguments, env=None):
        return subprocess.run(["/bin/sh", str(self.bundle / "link/relink.sh"), *arguments],
                              cwd=self.root, text=True, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE,
                              env={**{"PATH": "/usr/bin:/bin", "MAP": str(self.map),
                                      "VITASDK": str(self.sdk), "FAKE_AR": str(self.fake_ar)},
                                   **(env or {})})

    def test_default_mode_passes_for_the_release_elf(self):
        self.supply("libSceShaccCgExt.a")
        result = self.run_relink()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("members of libSceShaccCgExt.a match the pinned objects", result.stdout)
        self.assertIn("result byte-identical to the release ELF", result.stdout)
        self.assertIn("PASS the relink reproduces the release ELF", result.stdout)
        self.assertTrue((self.bundle / "output/looking-glass.elf").is_file())

    def test_strict_mode_passes_for_the_release_elf(self):
        self.supply("libSceShaccCgExt.a")
        result = self.run_relink("--strict")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("PASS the relink reproduces the release ELF", result.stdout)

    def test_multi_library_relink_checks_each_archive(self):
        self.supply("libSceShaccCgExt.a", "libvitaGL.a", "libpthread.a")
        result = self.run_relink()
        self.assertEqual(result.returncode, 0, result.stderr)
        for archive in ("libSceShaccCgExt.a", "libvitaGL.a", "libpthread.a"):
            self.assertIn("members of %s match the pinned objects" % archive, result.stdout)
        self.assertIn("PASS the relink reproduces the release ELF", result.stdout)

    def test_pthread_member_names_are_normalised(self):
        # CMake writes pthread_mutex_lock.c.obj; the release archive has .o.
        self.supply("libpthread.a")
        result = self.run_relink()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("members of libpthread.a match the pinned objects", result.stdout)

    def test_splashscreen_member_bytes_are_compared(self):
        # The exported-source build passes VGL_GIT_HASH, so splashscreen.o is
        # an ordinary stripped member of the pin, not a presence-only exception.
        self.supply("libvitaGL.a")
        result = self.run_relink()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("members of libvitaGL.a match the pinned objects", result.stdout)
        self.assertNotIn("presence checked", result.stdout)
        (self.fake_ar / "libvitaGL.a/splashscreen.o").write_bytes(b"different splash bytes")
        result = self.run_relink("--strict", env={"FAKE_ELF": "different elf"})
        self.assertEqual(result.returncode, 1)
        self.assertIn("splashscreen.o of libvitaGL.a differs from the pinned object", result.stdout)
        self.assertIn("archive members differ", result.stderr)

    def test_default_mode_warns_when_members_match_but_elf_differs(self):
        self.supply("libSceShaccCgExt.a")
        result = self.run_relink(env={"FAKE_ELF": "different elf"})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("WARNING the link succeeded", result.stdout)

    def test_strict_mode_fails_when_members_match_but_elf_differs(self):
        self.supply("libSceShaccCgExt.a")
        result = self.run_relink("--strict", env={"FAKE_ELF": "different elf"})
        self.assertEqual(result.returncode, 1)
        self.assertIn("FAIL strict mode", result.stderr)

    def test_strict_environment_variable_is_honoured(self):
        self.supply("libSceShaccCgExt.a")
        result = self.run_relink(env={"FAKE_ELF": "different elf", "LG_RELINK_STRICT": "1"})
        self.assertEqual(result.returncode, 1)
        self.assertIn("FAIL strict mode", result.stderr)

    def test_link_failure_fails_in_both_modes(self):
        self.supply("libSceShaccCgExt.a")
        for arguments in ((), ("--strict",)):
            result = self.run_relink(*arguments, env={"FAKE_LINK_FAIL": "1"})
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertNotIn("relinked ELF", result.stdout)

    def test_modified_library_reports_a_modified_relink(self):
        self.supply("libSceShaccCgExt.a")
        lines = list(self.pin_lines)
        lines[0] = "libSceShaccCgExt.a  shacccg_ext.c.obj  %s  raw" % ("0" * 64)
        self.write_member_manifest(lines)
        result = self.run_relink(env={"FAKE_ELF": "modified elf"})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("modified relink", result.stdout)
        self.assertIn("members of libSceShaccCgExt.a differ", result.stdout)

    def test_modified_library_fails_strict_mode(self):
        self.supply("libSceShaccCgExt.a")
        lines = list(self.pin_lines)
        lines[0] = "libSceShaccCgExt.a  shacccg_ext.c.obj  %s  raw" % ("0" * 64)
        self.write_member_manifest(lines)
        result = self.run_relink("--strict", env={"FAKE_ELF": "modified elf"})
        self.assertEqual(result.returncode, 1)
        self.assertIn("archive members differ", result.stderr)

    def test_one_mismatched_library_fails_strict_mode(self):
        self.supply("libSceShaccCgExt.a", "libvitaGL.a")
        lines = list(self.pin_lines)
        lines[1] = "libvitaGL.a  buffers.o  %s  stripped" % ("0" * 64)
        self.write_member_manifest(lines)
        result = self.run_relink("--strict", env={"FAKE_ELF": "modified elf"})
        self.assertEqual(result.returncode, 1)
        self.assertIn("members of libvitaGL.a differ", result.stdout)
        self.assertIn("members of libSceShaccCgExt.a match", result.stdout)

    def test_archive_file_hash_is_not_the_identity(self):
        # A rebuild of the same source can change the archive file bytes (ar
        # stores timestamps) while its member objects stay identical.
        self.supply("libSceShaccCgExt.a")
        (self.bundle / "lib/libSceShaccCgExt.a").write_bytes(b"rebuilt archive with different metadata")
        result = self.run_relink()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("PASS the relink reproduces the release ELF", result.stdout)

    def test_missing_library_is_refused(self):
        result = self.run_relink()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing a library archive", result.stderr)

    def test_sdk_archive_loaded_alongside_bundle_archive_is_refused(self):
        self.supply("libvitaGL.a")
        (self.sdk / "arm-vita-eabi/lib/libvitaGL.a").write_bytes(b"sdk archive")
        result = self.run_relink()
        self.assertEqual(result.returncode, 1)
        self.assertIn("also loaded the SDK libvitaGL.a", result.stderr)

    def test_two_outputs_are_refused(self):
        result = self.run_relink("one.elf", "two.elf")
        self.assertEqual(result.returncode, 2)
        self.assertIn("usage", result.stderr)


class LibraryPinControls(unittest.TestCase):
    def test_the_four_lgpl_libraries_are_pinned(self):
        data = json.loads((TOOLS / "vita_licenses/lgpl-libraries.json").read_text(encoding="utf-8"))
        pins = {entry["archive"]: entry for entry in data["libraries"]}
        shacccg = json.loads((TOOLS / "vita_licenses/SceShaccCgExt-release.json").read_text(encoding="utf-8"))
        self.assertEqual(set(BUNDLE.LIBRARY_ORDER),
                         set(pins) | {shacccg["archive"]})
        expected = {"libvitaGL.a": 30, "libvitashark.a": 1, "libpthread.a": 54}
        for archive, count in expected.items():
            self.assertEqual(len(pins[archive]["members"]), count)
            self.assertRegex(pins[archive]["revision"], r"^[0-9a-f]{40}$")
            self.assertTrue(pins[archive]["license_files"])
        self.assertEqual(pins["libvitashark.a"]["tag"], "v.1.7")
        self.assertEqual(pins["libpthread.a"]["identity"], "stripped-comment")
        self.assertEqual(pins["libvitaGL.a"]["identity"], "stripped")
        self.assertNotIn("path_dependent", pins["libvitaGL.a"])
        splash = next(member for member in pins["libvitaGL.a"]["members"]
                      if member["member"] == "splashscreen.o")
        self.assertEqual(splash["sha256_stripped"],
                         "8d1f0fdd907cd0c88a8dad7024a4d8adb219a99de1f74a16ebedade2062184f9")
        self.assertEqual(pins["libvitaGL.a"]["revision_macro"], "VGL_GIT_HASH")
        self.assertIn("VGL_GIT_HASH", pins["libvitaGL.a"]["revision_macro_note"])

    def test_every_pinned_member_has_both_hashes(self):
        data = json.loads((TOOLS / "vita_licenses/lgpl-libraries.json").read_text(encoding="utf-8"))
        for entry in data["libraries"]:
            for member in entry["members"]:
                self.assertRegex(member["sha256"], r"^[0-9a-f]{64}$")
                self.assertRegex(member["sha256_stripped"], r"^[0-9a-f]{64}$")
                self.assertGreater(member["bytes"], 0)


class DocumentationControls(unittest.TestCase):
    def test_relink_and_readme_document_both_modes(self):
        for text in (BUNDLE.RELINK_TEMPLATE, BUNDLE.README_TEMPLATE):
            self.assertIn("--strict", text)
            self.assertIn("LG_RELINK_STRICT", text)
        self.assertIn("functional relink", BUNDLE.README_TEMPLATE)
        self.assertIn("byte-identical", BUNDLE.RELINK_TEMPLATE)

    def test_readme_documents_all_four_build_steps_and_packaging(self):
        for token in ("@@VITAGL_REVISION@@", "@@VITASHARK_REVISION@@",
                      "@@PTHREAD_REVISION@@", "@@REVISION@@"):
            self.assertIn(token, BUNDLE.README_TEMPLATE)
        for token in ("Step 3: build vitaGL", "Step 4: build vitaShaRK",
                      "Step 5: build pthread-embedded", "Step 6: build SceShaccCgExt",
                      "vita-elf-create", "vita-make-fself", "vita-mksfoex",
                      "vita-pack-vpk", "vitaShaRK", "pthread-embedded"):
            self.assertIn(token, BUNDLE.README_TEMPLATE)
        self.assertIn("link/package.sh", BUNDLE.README_TEMPLATE)

    def test_relink_template_checks_members_per_library(self):
        for token in ("libvitaGL.a", "libvitashark.a", "libSceShaccCgExt.a",
                      "libpthread.a", "library-members.txt", "normalise_member",
                      "also loaded the SDK", "stripped-comment"):
            self.assertIn(token, BUNDLE.RELINK_TEMPLATE)
        self.assertNotIn("presence checked", BUNDLE.RELINK_TEMPLATE)

    def test_public_recipe_names_the_full_revisions_and_commands(self):
        facts = {
            "vitagl_revision": BUNDLE.VITAGL_REVISION,
            "vitagl_short": BUNDLE.VITAGL_REVISION[:7],
            "vitashark_revision": "d" * 40,
            "pthread_revision": "e" * 40,
            "vitagl_flags": "NO_TEX_COMBINER=1",
            "vitagl_archive": "c" * 64,
            "vitagl_package": "vitaGL-1-1",
            "vdpm_packages": BUNDLE.VDPM_PACKAGES,
            "revision": "b" * 40,
            "head": "a" * 40,
            "elf": "f" * 64,
            "self": "1" * 64,
            "bundle_name": "bundle.tar.gz",
            "library": "2" * 64,
            "objects": 3,
            "bytes": 4,
        }
        for template in (BUNDLE.README_TEMPLATE, BUNDLE.BUILD_RECIPE_TEMPLATE,
                         BUNDLE.SOURCE_OFFER_TEMPLATE):
            text = template
            for key, value in facts.items():
                text = text.replace("@@%s@@" % key.upper(), str(value))
            self.assertNotIn("@@", text)
            self.assertIn(BUNDLE.VITAGL_REVISION, text)
        recipe = BUNDLE.BUILD_RECIPE_TEMPLATE
        for key, value in facts.items():
            recipe = recipe.replace("@@%s@@" % key.upper(), str(value))
        self.assertIn("bootstrap-vitasdk.sh", recipe)
        self.assertIn("make install", recipe)
        self.assertIn(BUNDLE.VDPM_PACKAGES, recipe)
        self.assertIn("CMake", recipe)

    def test_readme_builds_exported_vitagl_with_the_revision_macro(self):
        text = BUNDLE.README_TEMPLATE
        self.assertIn("sources/vitaGL", text)
        self.assertIn("-DVGL_GIT_HASH='\\\"@@VITAGL_SHORT@@\\\"'", text)
        for macro in ("-DDISABLE_TEXTURE_COMBINER", "-DHAVE_UNFLIPPED_FBOS",
                      "-DHAVE_SINGLE_THREADED_GC", "-DHAVE_CUSTOM_HEAP"):
            self.assertIn(macro, text)
        self.assertIn("make VGL_GIT_HASH=...` has no effect", text)
        self.assertNotIn("__FILE__", text)

    def test_notices_name_sceaudio_records_and_quad_rand_copyright_gap(self):
        text = (REPO / "THIRD_PARTY_NOTICES-vita.txt").read_text(encoding="utf-8")
        for symbol in ("sceAudioOutOpenPort", "sceAudioOutOutput",
                       "sceAudioOutReleasePort", "sceAudioOutSetVolume"):
            self.assertIn(symbol, text)
        self.assertIn("The fork also declares four SceAudio import records in "
                      "src/host/vita/mod.rs", text)
        self.assertIn("quad-rand 0.2.3 ships no licence file and no copyright line", text)

    def test_notices_ship_the_newlib_vita_glue_notice(self):
        text = (REPO / "THIRD_PARTY_NOTICES-vita.txt").read_text(encoding="utf-8")
        self.assertIn("--- licenses/newlib-vita-glue-NOTICE.txt ---", text)
        self.assertIn("Objects compiled from newlib/libc/sys/vita in this link:", text)
        self.assertIn("io.c, mlock.c, sbrk.c, syscalls.c and threading.c carry no "
                      "copyright or licence line", text)
        self.assertIn("crt0.o", text)

    def test_vitagl_record_is_read_from_the_release_build(self):
        text = ("linked libvitaGL.a sha256=" + "a" * 64
                + " package=vitaGL-1-1 not-the-package-file path=~/vitasdk/lib/libvitaGL.a\n"
                + "vitagl-recipe make A=1 B=2 (documented; the archive does not record its flags)\n")
        record = BUNDLE.read_vitagl_record(text)
        self.assertEqual(record["archive"], "a" * 64)
        self.assertEqual(record["package"], "vitaGL-1-1")
        self.assertEqual(record["flags"], "A=1 B=2")

    def test_missing_vitagl_record_is_refused(self):
        with self.assertRaisesRegex(ValueError, "does not record the vitaGL"):
            BUNDLE.read_vitagl_record("head " + "0" * 40 + "\n")

    def test_build_recipe_is_appended(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "BUILD.txt"
            path.write_text("release record\n")
            BUNDLE.append_build_recipe(path, {
                "revision": "b" * 40,
                "vitagl_revision": BUNDLE.VITAGL_REVISION,
                "vitagl_short": BUNDLE.VITAGL_REVISION[:7],
                "vitashark_revision": "d" * 40,
                "pthread_revision": "e" * 40,
                "vitagl_flags": "A=1 B=2",
                "vitagl_archive": "c" * 64,
                "vdpm_packages": BUNDLE.VDPM_PACKAGES,
            })
            text = path.read_text(encoding="utf-8")
        self.assertTrue(text.startswith("release record\n"))
        self.assertIn("## Public rebuild recipe", text)
        self.assertIn(BUNDLE.VITAGL_REVISION, text)
        self.assertIn("A=1 B=2", text)
        self.assertIn("c" * 64, text)


class PackagingControls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.sdk = self.root / "sdk"
        (self.sdk / "bin").mkdir(parents=True)
        self.packer = self.sdk / "bin/vita-pack-vpk"
        self.packer.write_text("""#!%s
import sys, zipfile
args = sys.argv[1:]
out = args[-1]
sfo = eboot = None
adds = []
i = 0
while i < len(args) - 1:
    if args[i] == '-s':
        sfo = args[i + 1]; i += 2
    elif args[i] == '-b':
        eboot = args[i + 1]; i += 2
    elif args[i] == '--add':
        adds.append(args[i + 1]); i += 2
    else:
        i += 1
with zipfile.ZipFile(out, 'w') as archive:
    archive.write(eboot, 'eboot.bin')
    archive.write(sfo, 'sce_sys/param.sfo')
    for item in adds:
        source, name = item.split('=', 1)
        archive.write(source, name)
""" % sys.executable)
        self.packer.chmod(0o755)
        self.eboot = self.root / "eboot.bin"
        self.eboot.write_bytes(b"self bytes")
        self.sfo = self.root / "param.sfo"
        self.sfo.write_bytes(b"sfo bytes")
        self.elf = self.root / "looking-glass.elf"
        self.elf.write_bytes(b"elf bytes")
        self.notices = self.root / "THIRD_PARTY_NOTICES-vita.txt"
        self.notices.write_text("test notices\n")
        self.assets = self.root / "vita"
        for relative in PACKAGE.LIVEAREA:
            path = self.assets / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"asset " + relative.encode())
        self.facts = {
            "head": "a" * 40,
            "elf": digest(self.elf.read_bytes()),
            "bundle": "alice-lgpl-test.tar.gz",
            "bundle_sha256": "b" * 64,
            "vitagl": "1" * 40,
            "vitashark": "2" * 40,
            "pthread": "3" * 40,
            "shacccg": "4" * 40,
        }

    def build(self):
        checksums = PACKAGE.mpl_checksums(REPO / "Cargo.lock")
        stage = self.root / "stage"
        extras = PACKAGE.stage(REPO, stage, self.notices, checksums, self.facts)
        assets = PACKAGE.collect_assets(self.assets, REPO)
        extras = assets + extras
        vpk = self.root / "looking-glass.vpk"
        PACKAGE.build_vpk(self.sdk, self.sfo, self.eboot, extras, vpk)
        names = PACKAGE.verify_vpk(vpk, self.eboot, self.sfo, extras)
        return vpk, names

    def test_vpk_contains_notices_licences_and_source_offer(self):
        vpk, names = self.build()
        for name in ("THIRD_PARTY_NOTICES-vita.txt", "SOURCE-OFFER.txt",
                     "licenses/GPL-3.0.txt", "licenses/LGPL-3.0.txt",
                     "licenses/LGPL-2.1.txt",
                     "licenses/GCC-RUNTIME-EXCEPTION-3.1.txt",
                     "licenses/MPL-2.0.txt", "licenses/MIT.txt",
                     "licenses/Apache-2.0.txt",
                     "licenses/newlib-vita-glue-NOTICE.txt"):
            self.assertIn(name, names)
        with zipfile.ZipFile(vpk) as archive:
            self.assertEqual(archive.read("eboot.bin"), b"self bytes")
            self.assertEqual(archive.read("sce_sys/param.sfo"), b"sfo bytes")
            offer = archive.read("SOURCE-OFFER.txt").decode()
        self.assertIn("alice-lgpl-test.tar.gz", offer)
        self.assertIn("crates.io", offer)
        self.assertIn("5773a4c030a19d9bfaa090f49746ff35c75dfddfa700df7a5939d5e076a57039", offer)

    def test_notices_are_the_generated_text(self):
        vpk, _ = self.build()
        with zipfile.ZipFile(vpk) as archive:
            self.assertEqual(archive.read("THIRD_PARTY_NOTICES-vita.txt"), b"test notices\n")

    def test_gpl3_and_rle_are_under_their_right_names(self):
        vpk, _ = self.build()
        with zipfile.ZipFile(vpk) as archive:
            gpl = archive.read("licenses/GPL-3.0.txt").decode()
            rle = archive.read("licenses/GCC-RUNTIME-EXCEPTION-3.1.txt").decode()
        self.assertIn("GNU GENERAL PUBLIC LICENSE", gpl)
        self.assertIn("Version 3, 29 June 2007", gpl)
        self.assertIn("GCC RUNTIME LIBRARY EXCEPTION", rle)
        self.assertIn("Version 3.1", rle)

    def test_missing_notices_are_refused(self):
        self.notices.unlink()
        with self.assertRaises(OSError):
            self.build()

    def test_missing_licence_is_refused(self):
        with tempfile.TemporaryDirectory() as temp:
            repo = Path(temp)
            shutil.copytree(REPO / "tools/vita_licenses", repo / "tools/vita_licenses")
            (repo / "tools/vita_licenses/SceShaccCgExt-LICENSE").unlink()
            checksums = PACKAGE.mpl_checksums(REPO / "Cargo.lock")
            with self.assertRaises(ValueError):
                PACKAGE.stage(repo, self.root / "stage2", self.notices, checksums, self.facts)

    def test_bundle_and_package_licence_lists_agree(self):
        package_names = {name for name, _ in PACKAGE.LICENCES}
        bundle_names = {"GPL-3.0.txt", "LGPL-3.0.txt", "LGPL-2.1.txt",
                        "pthread-embedded-COPYING.txt",
                        "pthread-embedded-COPYING.LIB.txt",
                        "pthread-embedded-COPYING.vita.txt",
                        "vitaShaRK-LICENSE.txt"}
        bundle_names |= {name for name, _ in BUNDLE.PACKAGE_LICENCES}
        self.assertEqual(package_names, bundle_names)

    def test_cargo_lock_mpl_checksums(self):
        checksums = PACKAGE.mpl_checksums(REPO / "Cargo.lock")
        self.assertEqual(set(checksums), set(PACKAGE.MPL_CRATES))
        for _, (version, checksum) in checksums.items():
            self.assertEqual(version, "0.5.5")
            self.assertRegex(checksum, r"^[0-9a-f]{64}$")


if __name__ == "__main__":
    unittest.main()
