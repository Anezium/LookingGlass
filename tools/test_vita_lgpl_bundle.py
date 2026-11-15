#!/usr/bin/env python3
"""Fail-closed controls for the LGPL source and relink bundle."""
import hashlib
import importlib.util
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

TOOLS = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("lgpl_bundle", TOOLS / "vita_lgpl_bundle.py")
BUNDLE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUNDLE)


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
            "-lSceShaccCgExt",
            "-L", str(self.raw),
            "-L", str(self.lib),
            "-L", str(self.sdk / "arm-vita-eabi/lib"),
            "-o", str(self.out / "looking_glass.elf"),
            "-Wl,-Map=" + str(self.release / "link.map"),
        ]

    def test_exact_command_is_rewritten_and_inputs_listed(self):
        mapped, copies, dirs = BUNDLE.classify(self.argv, self.target, self.lib,
                                               self.sdk, self.release, self.capture)
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
            BUNDLE.classify(self.argv, self.target, self.lib, self.sdk, self.release, self.capture)

    def test_unknown_path_is_refused(self):
        argv = list(self.argv) + [str(self.root / "elsewhere" / "toolchain.o")]
        with self.assertRaisesRegex(ValueError, "neither target, SDK nor release library"):
            BUNDLE.classify(argv, self.target, self.lib, self.sdk, self.release, self.capture)

    def test_sdk_before_isolated_library_is_refused(self):
        argv = ["-lSceShaccCgExt", "-L", str(self.sdk / "arm-vita-eabi/lib"),
                "-L", str(self.lib), "-o", str(self.out / "a.elf"),
                "-Wl,-Map=" + str(self.release / "link.map")]
        with self.assertRaisesRegex(ValueError, "searched before the release library"):
            BUNDLE.classify(argv, self.target, self.lib, self.sdk, self.release, self.capture)

    def test_one_output_one_map_one_library_are_required(self):
        argv = [str(self.objects[0]), "-L", str(self.lib),
                "-L", str(self.sdk / "arm-vita-eabi/lib"),
                "-Wl,-Map=" + str(self.release / "link.map")]
        with self.assertRaisesRegex(ValueError, "link SceShaccCgExt"):
            BUNDLE.classify(argv, self.target, self.lib, self.sdk, self.release, self.capture)


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
        (self.bundle / "lib").mkdir(parents=True)
        (self.bundle / "link").mkdir()
        (self.bundle / "objects").mkdir()
        (self.bundle / "objects/a.o").write_bytes(b"object")
        self.library = self.bundle / "lib" / BUNDLE.ARCHIVE
        self.library.write_bytes(b"library")
        self.sdk = self.root / "sdk"
        (self.sdk / "arm-vita-eabi/lib").mkdir(parents=True)
        self.linker = self.sdk / "bin/arm-vita-eabi-gcc"
        self.linker.parent.mkdir()
        self.linker.write_text(
            "#!/bin/sh\n"
            "out=\n prev=\n rec=\n"
            "for a in \"$@\"; do\n"
            "  [ \"$prev\" = -L ] && [ -f \"$a/libSceShaccCgExt.a\" ] && rec=$a\n"
            "  [ \"$prev\" = -o ] && out=$a\n"
            "  prev=$a\n"
            "done\n"
            "printf 'LOAD %s\\n' \"$rec/libSceShaccCgExt.a\" > \"$MAP\"\n"
            "printf 'release elf' > \"$out\"\n")
        self.linker.chmod(0o755)
        self.map = self.bundle / "output/relink.map"
        self.map.parent.mkdir()
        (self.bundle / "link/args.txt").write_text(
            "-Wl,-q\n@BUNDLE@/objects/a.o\n-L\n@BUNDLE@/lib\n-L\n@SDK@/arm-vita-eabi/lib\n"
            "-o\n@OUT@\n-Wl,-Map=@BUNDLE@/output/relink.map\n")
        (self.bundle / "link/release-elf.sha256").write_text(digest(b"release elf") + "\n")
        (self.bundle / "link/pinned-library.sha256").write_text(digest(b"library") + "\n")
        (self.bundle / "link/relink.sh").write_text(BUNDLE.RELINK_TEMPLATE)
        (self.bundle / "link/relink.sh").chmod(0o755)

    def run_relink(self, *arguments, env=None):
        return subprocess.run(["/bin/sh", str(self.bundle / "link/relink.sh"), *arguments],
                              cwd=self.root, text=True, stdout=subprocess.PIPE,
                              stderr=subprocess.PIPE,
                              env={**{"PATH": "/usr/bin:/bin", "MAP": str(self.map),
                                      "VITASDK": str(self.sdk)}, **(env or {})})

    def test_placeholders_resolve_and_release_elf_is_checked(self):
        result = self.run_relink()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("PASS the relink reproduces the release ELF", result.stdout)
        self.assertTrue((self.bundle / "output/looking-glass.elf").is_file())

    def test_modified_library_reports_a_modified_relink(self):
        (self.bundle / "link/pinned-library.sha256").write_text("0" * 64 + "\n")
        result = self.run_relink()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("modified relink", result.stdout)

    def test_missing_library_is_refused(self):
        self.library.unlink()
        result = self.run_relink()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("missing", result.stderr)


if __name__ == "__main__":
    unittest.main()
