#!/usr/bin/env python3
"""Fail-closed library selection and release-wrapper regression controls."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

TOOLS = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("release_libraries", TOOLS / "vita_release_libraries.py")
LIB = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LIB)
SHELL = "/opt/homebrew/bin/bash" if Path("/opt/homebrew/bin/bash").exists() else shutil.which("bash")


class SelectionControls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.good = self.root / "isolated archive" / LIB.ARCHIVE
        self.sdk = self.root / "sdk" / LIB.ARCHIVE
        self.good.parent.mkdir()
        self.sdk.parent.mkdir()
        self.good.write_bytes(b"reviewed isolated LGPL archive fixture")
        self.sdk.write_bytes(b"old SDK GPL archive fixture")
        self.pin = {"archive_sha256": LIB.sha256(self.good), "known_sdk_gpl_sha256": LIB.sha256(self.sdk)}

    def test_explicit_selection_and_known_sdk_rejection(self):
        with self.assertRaisesRegex(ValueError, "requires --lib-dir"):
            LIB.select_library(None, self.pin)
        with self.assertRaisesRegex(ValueError, "known SDK GPL"):
            LIB.select_library(self.sdk.parent, self.pin)
        self.assertEqual(LIB.select_library(self.good.parent, self.pin), self.good.resolve())

    def test_unknown_bytes_are_rejected(self):
        self.good.write_bytes(b"unreviewed archive")
        with self.assertRaisesRegex(ValueError, "does not match"):
            LIB.select_library(self.good.parent, self.pin)

    def test_actual_member_selection_and_spaces(self):
        text = f"LOAD {self.good}\n{self.good}(shacccg_ext.c.obj)\n .text 0x810000 0x20 {self.good}(shacccg_ext_oop.cpp.obj)\n"
        self.assertEqual(LIB.verify_link_map(text, self.good, self.pin),
                         ["shacccg_ext.c.obj", "shacccg_ext_oop.cpp.obj"])

    def test_old_sdk_resolution_rejected_even_after_good_preflight(self):
        selected = LIB.select_library(self.good.parent, self.pin)
        with self.assertRaisesRegex(ValueError, "different.*archive"):
            LIB.verify_link_map(f"{self.sdk}(shacccg_ext.c.obj)\n", selected, self.pin)
        with self.assertRaisesRegex(ValueError, "different.*archive"):
            LIB.verify_link_map(f"LOAD {self.sdk}\n{self.good}(shacccg_ext.c.obj)\n", selected, self.pin)

    def test_no_member_relative_reference_and_changed_archive_rejected(self):
        with self.assertRaisesRegex(ValueError, "no contributing"):
            LIB.verify_link_map(f"LOAD {self.good}\n", self.good, self.pin)
        with self.assertRaisesRegex(ValueError, "unrecognised"):
            LIB.verify_link_map(f"{LIB.ARCHIVE}(shacccg_ext.c.obj)\n", self.good, self.pin)
        self.good.write_bytes(b"changed during native link")
        with self.assertRaisesRegex(ValueError, "changed during"):
            LIB.verify_link_map(f"{self.good}(shacccg_ext.c.obj)\n", self.good, self.pin)

    def test_pinned_license_text_is_verified(self):
        folder = self.root / "tools/vita_licenses"
        folder.mkdir(parents=True)
        (folder / "LICENSE").write_text("changed licence text")
        pin = dict(archive=LIB.ARCHIVE, license="LGPL-2.1", license_file="LICENSE", license_sha256="0" * 64)
        (folder / "SceShaccCgExt-release.json").write_text(json.dumps(pin))
        with self.assertRaisesRegex(ValueError, "licence text"):
            LIB.load_pin(self.root)


class WrapperControls(unittest.TestCase):
    """Exercise the production shell wrapper with a fake build, not a second wrapper model.

    Toolchain stubs only create inert test files in a temporary target. No Cargo
    build, Git mutation, global SDK write or device connection occurs.
    """
    def run_wrapper(self, checker_status):
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary)
            binaries = folder / "bin"
            binaries.mkdir()
            repo = TOOLS.parent
            stubs = {
                "git": f"#!/bin/sh\ncase \"$*\" in *--show-toplevel*) echo '{repo}';; *--porcelain*) :;; *) echo fixture-head;; esac\n",
                "rustup": "#!/bin/sh\ncase \"$*\" in *component*) echo rust-src;; *) echo rustc-fixture;; esac\n",
                "cargo": "#!/bin/sh\nif [ \"$1\" = install ]; then echo 'cargo-vita v0.2.2:'; exit 0; fi\n" +
                    "r=\"$CARGO_TARGET_DIR/armv7-sony-vita-newlibeabihf/release\"\nmkdir -p \"$r\"\n" +
                    "for n in elf self sfo vpk; do echo fixture > \"$r/looking-glass.$n\"; done\necho fixture-map > \"$LG_VITA_LINK_MAP\"\n",
                # Image validation has separate real-package controls; this inert
                # wrapper fixture tests only checker propagation/private-mode flow.
                "python3": "#!/bin/sh\ncase \"$*\" in *vita_livearea.py*) exit 0;; esac\ncase \"$2\" in *vita_release_check.py) echo 'checker fixture without FAIL marker'; exit " +
                    str(checker_status) + ";; *) exec '" + sys.executable + "' \"$@\";; esac\n",
            }
            for name, contents in stubs.items():
                path = binaries / name
                path.write_text(contents)
                path.chmod(0o755)
            sdk = folder / "sdk"
            sdk.mkdir()
            (sdk / "version_info.txt").write_text("fixture SDK\nfixture component\n")
            out = folder / "out"
            env = dict(os.environ, PATH=str(binaries) + os.pathsep + "/usr/bin:/bin", VITASDK=str(sdk),
                       CARGO_TARGET_DIR=str(folder / "target"))
            result = subprocess.run([SHELL, str(TOOLS / "vita_release_build.sh"), "--private-test", "--out", str(out)],
                                    env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=30)
            if checker_status:
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn("release checks failed", result.stdout)
            else:
                self.assertEqual(result.returncode, 0, result.stdout)
                self.assertTrue((out / "PRIVATE-TEST.txt").exists())
                self.assertIn("mode private-test", (out / "BUILD.txt").read_text())
                self.assertFalse((out / "THIRD_PARTY_NOTICES-vita.txt").exists())

    def test_checker_crash_cannot_be_hidden_by_tee(self):
        self.run_wrapper(37)

    def test_private_test_mode_is_explicit_and_has_no_release_notice_claim(self):
        self.run_wrapper(0)


if __name__ == "__main__":
    unittest.main()
