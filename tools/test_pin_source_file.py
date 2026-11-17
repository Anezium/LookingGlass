"""Exact-pin insertion: update one path, never grant a wildcard."""
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from pin_source_file import add_pin, pin_line, checked_path

ROOT = Path(__file__).resolve().parents[1]

SAMPLE = '''PINNED_FILES = {
    # Release documents.
    "CREDITS.txt": "00",
    # Reviewed runtime and measurement configurations, one entry per preset.
    "configs/old.json": "1111111111111111111111111111111111111111111111111111111111111111",
    # Tooling, licence texts and test fixtures.
    "tools/x.py": "2222222222222222222222222222222222222222222222222222222222222222",
}
LIVEAREA_ART = {}
'''


class PinTests(unittest.TestCase):
    def test_adds_a_config_before_the_tools_group(self):
        digest = "a" * 64
        text, action = add_pin(SAMPLE, "configs/PL-rc15-release-play.json", digest)
        self.assertEqual(action, "added")
        self.assertIn(pin_line("configs/PL-rc15-release-play.json", digest), text)
        self.assertLess(text.index("PL-rc15"), text.index("# Tooling"))

    def test_updates_existing_and_reports_unchanged(self):
        digest = "b" * 64
        text, action = add_pin(SAMPLE, "configs/old.json", digest)
        self.assertEqual(action, "updated")
        self.assertIn(pin_line("configs/old.json", digest), text)
        self.assertNotIn("1" * 64, text)
        same, action = add_pin(text, "configs/old.json", digest)
        self.assertEqual((same, action), (text, "unchanged"))

    def test_falls_back_to_the_end_of_the_pinned_block(self):
        digest = "c" * 64
        text, action = add_pin(SAMPLE, "vita/extra.xml", digest)
        self.assertEqual(action, "added")
        self.assertLess(text.index("vita/extra.xml"), text.index("LIVEAREA_ART"))
        self.assertLess(text.index("tools/x.py"), text.index("vita/extra.xml"))

    def test_rejects_non_sha256_and_game_data(self):
        with self.assertRaisesRegex(ValueError, "SHA-256"):
            pin_line("configs/x.json", "not-a-digest")
        with self.assertRaisesRegex(ValueError, "game-data"):
            checked_path("configs/PL-release.pk3")

    def test_checked_path_is_repository_relative(self):
        with self.assertRaisesRegex(ValueError, "repository-relative"):
            checked_path("/etc/hosts")
        with self.assertRaisesRegex(ValueError, "repository-relative"):
            checked_path("../outside.json")

    def test_pin_command_turns_a_failing_audit_fixture_green(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            (root / "tools").mkdir()
            for name in ["check_source.py", "pin_source_file.py"]:
                shutil.copyfile(ROOT / "tools" / name, root / "tools" / name)
            fixture = root / "configs/PL-rc15-release-play.json"
            fixture.parent.mkdir()
            fixture.write_text('{\n  "log_frame_time_every": 0\n}\n', encoding="utf-8")
            audit = [sys.executable, "tools/check_source.py", "--worktree"]
            before = subprocess.run(audit, cwd=root, capture_output=True, text=True)
            self.assertEqual(before.returncode, 1, before.stdout + before.stderr)
            self.assertIn("Not an approved source path: configs/PL-rc15-release-play.json",
                          before.stdout)
            pin = subprocess.run(
                [sys.executable, "tools/pin_source_file.py", "configs/PL-rc15-release-play.json"],
                cwd=root, capture_output=True, text=True,
            )
            self.assertEqual(pin.returncode, 0, pin.stdout + pin.stderr)
            after = subprocess.run(audit, cwd=root, capture_output=True, text=True)
            self.assertEqual(after.returncode, 0, after.stdout + after.stderr)
            self.assertIn("audit passed", after.stdout)


if __name__ == "__main__":
    unittest.main()
