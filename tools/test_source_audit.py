"""Exact-path source pins: the release allowlist admits no game data.

The pins are reviewed files whose path or extension falls outside the general
source rules. A changed byte must fail until a human updates the pin; a binary
is admitted only as LiveArea art at its reviewed dimensions.
"""
from pathlib import Path
import hashlib
import struct
import subprocess
import sys
import unittest
from unittest.mock import patch

import check_source as c

GAME_DATA_SUFFIXES = {
    ".pk3", ".pk4", ".bsp", ".tga", ".dds", ".wav", ".ogg", ".mp3", ".bik",
    ".roq", ".mpx", ".sav", ".sav0", ".elf", ".self", ".vpk", ".suprx",
}


class PinnedSourceTests(unittest.TestCase):
    def test_every_pin_is_an_exact_reviewed_file(self):
        for path, digest in c.PINNED_FILES.items():
            self.assertNotIn("*", path)
            self.assertNotIn("?", path)
            self.assertFalse(path.endswith("/"), path)
            data = (c.ROOT / path).read_bytes()
            self.assertEqual(hashlib.sha256(data).hexdigest(), digest, path)
            self.assertEqual(c.audit_data(path, data), [], path)

    def test_no_game_data_or_binary_is_pinned(self):
        self.assertEqual(set(c.LIVEAREA_ART), {
            "vita/sce_sys/icon0.png",
            "vita/sce_sys/livearea/contents/bg.png",
            "vita/sce_sys/livearea/contents/startup.png",
        })
        for path in c.PINNED_FILES:
            self.assertNotIn(Path(path).suffix.lower(), GAME_DATA_SUFFIXES, path)
            data = (c.ROOT / path).read_bytes()
            if path in c.LIVEAREA_ART:
                continue
            self.assertNotIn(b"\0", data, path)

    def test_livearea_art_is_png_at_the_reviewed_size(self):
        for path, size in c.LIVEAREA_ART.items():
            data = (c.ROOT / path).read_bytes()
            self.assertTrue(data.startswith(b"\x89PNG\r\n\x1a\n"), path)
            self.assertEqual(struct.unpack(">II", data[16:24]), size, path)

    def test_changed_pin_is_rejected(self):
        path = "THIRD_PARTY_NOTICES-vita.txt"
        data = (c.ROOT / path).read_bytes()
        errors = c.audit_data(path, data + b"\n")
        self.assertTrue(any("Unreviewed pinned source content" in e for e in errors), errors)

    def test_changed_or_resized_art_is_rejected(self):
        path = "vita/sce_sys/icon0.png"
        data = (c.ROOT / path).read_bytes()
        self.assertTrue(any("Unreviewed pinned source content" in e
                            for e in c.audit_data(path, data[:-1])))
        fake = b"\x89PNG\r\n\x1a\n" + b"\0\0\0\rIHDR" + struct.pack(">II", 512, 512)
        errors = c.audit_data(path, fake)
        self.assertTrue(any("Unexpected LiveArea art size" in e for e in errors), errors)

    def test_unlisted_paths_and_game_data_are_rejected(self):
        for path in ["vita/sce_sys/extra.png", "configs/extra.pk3", "src/map.bsp",
                     "docs/media/pk3/data.txt", "vita/sce_sys/icon0.jpg"]:
            self.assertFalse(c.allowed(path), path)
        self.assertTrue(c.audit_data("src/texture.tga", b"\0" * 16))
        self.assertTrue(c.audit_data("src/binary.rs", b"\0binary"))

    def test_rc15_release_play_config_requires_and_then_accepts_an_exact_pin(self):
        path = "configs/PL-rc15-release-play.json"
        data = b'{\n  "log_frame_time_every": 0\n}\n'
        self.assertFalse(c.allowed(path))
        errors = c.audit_data(path, data)
        self.assertTrue(any("Not an approved source path" in e for e in errors), errors)
        with patch.dict(c.PINNED_FILES, {path: hashlib.sha256(data).hexdigest()}):
            self.assertEqual(c.audit_data(path, data), [])
            self.assertTrue(any("Unreviewed pinned source content" in e
                                for e in c.audit_data(path, data + b"\n")))

    def test_worktree_audit_is_green(self):
        result = subprocess.run(
            [sys.executable, str(c.ROOT / "tools/check_source.py"), "--worktree"],
            cwd=c.ROOT, capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("audit passed", result.stdout)


if __name__ == "__main__":
    unittest.main()
