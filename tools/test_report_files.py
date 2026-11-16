"""Report-name compatibility against fixtures shaped like preserved evidence."""
from pathlib import Path
import hashlib
import json
import subprocess
import sys
import tempfile
import unittest

import report_files as r

ROOT = Path(__file__).resolve().parents[1]


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


class ReportNameTests(unittest.TestCase):
    def test_canonical_names_and_neutral_prefixed_names(self):
        self.assertEqual(r.canonical_report_name("REPORT.md"), "REPORT.md")
        self.assertEqual(r.canonical_report_name("REPORT-R11D.md"), "REPORT-R11D.md")
        self.assertEqual(r.canonical_report_name("TEAM-REPORT.md"), "REPORT.md")
        self.assertEqual(r.canonical_report_name("TEAM-REPORT-R11B.md"), "REPORT-R11B.md")
        for name in ["README.md", "AMENDMENT-R11.md", "CONSOLE-BRIEF.md",
                     "REPORTING.md", "REPORTER.md"]:
            self.assertIsNone(r.canonical_report_name(name), name)
            self.assertFalse(r.is_report_file(name), name)

    def test_resolve_accepts_canonical_and_historical_prefix(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            write(root / "TEAM-REPORT.md", "old report")
            write(root / "TEAM-REPORT-R11C.md", "old stage report")
            self.assertEqual(r.resolve(root, "REPORT.md").name, "TEAM-REPORT.md")
            self.assertEqual(r.resolve(root, "REPORT-R11C.md").name, "TEAM-REPORT-R11C.md")
            self.assertEqual(r.report_path(root).read_text(), "old report")

    def test_canonical_name_wins_and_ambiguity_is_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            write(root / "REPORT.md", "canonical")
            write(root / "TEAM-REPORT.md", "legacy")
            self.assertEqual(r.report_path(root).name, "REPORT.md")
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            write(root / "TEAM-REPORT-R11D.md", "one")
            write(root / "OTHER-REPORT-R11D.md", "two")
            with self.assertRaisesRegex(ValueError, "ambiguous"):
                r.report_path(root, "-R11D")

    def test_missing_report_returns_the_canonical_path(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            self.assertEqual(r.report_path(root).name, "REPORT.md")
            self.assertFalse(r.report_path(root).exists())

    def test_copy_documents_renames_reports_and_preserves_the_rest(self):
        with tempfile.TemporaryDirectory() as temp:
            old = Path(temp) / "old"
            new = Path(temp) / "new"
            new.mkdir()
            write(old / "TEAM-REPORT.md", "report")
            write(old / "TEAM-REPORT-R11B.md", "r11b")
            write(old / "CONSOLE-BRIEF.md", "brief")
            write(old / "AMENDMENT-R11B.md", "amendment")
            copied = r.copy_documents(old, new)
            self.assertEqual(copied, [
                ("AMENDMENT-R11B.md", "AMENDMENT-R11B.md"),
                ("CONSOLE-BRIEF.md", "CONSOLE-BRIEF.md"),
                ("TEAM-REPORT-R11B.md", "REPORT-R11B.md"),
                ("TEAM-REPORT.md", "REPORT.md"),
            ])
            self.assertEqual((new / "REPORT.md").read_text(), "report")
            self.assertEqual((new / "REPORT-R11B.md").read_text(), "r11b")
            self.assertFalse(any(p.name.startswith("TEAM-REPORT") for p in new.iterdir()))
            self.assertEqual(r.resolve(new, "REPORT-R11B.md").read_text(), "r11b")

    def test_copy_documents_rejects_two_sources_for_one_target(self):
        with tempfile.TemporaryDirectory() as temp:
            old = Path(temp) / "old"
            new = Path(temp) / "new"
            new.mkdir()
            write(old / "TEAM-REPORT.md", "one")
            write(old / "REPORT.md", "two")
            with self.assertRaisesRegex(ValueError, "duplicate"):
                r.copy_documents(old, new)

    def test_manifest_filter_excludes_a_legacy_report_shape(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            write(root / "LEGACY-REPORT.md", "report")
            write(root / "ARTIFACT-SHA256.json", "{}")
            write(root / "proof" / "RESULT.json", "{}")
            names = {p.name for p in sorted(root.rglob("*"))
                     if p.is_file() and not r.is_report_file(p.name, "")
                     and p.name != "ARTIFACT-SHA256.json"}
            self.assertEqual(names, {"RESULT.json"})

    def test_original_only_exclusion_keeps_the_staged_reports(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for name in ["TEAM-REPORT.md", "TEAM-REPORT-R11B.md",
                         "TEAM-REPORT-R11C.md", "TEAM-REPORT-R11D.md",
                         "PROOF.json"]:
                write(root / name, name)
            kept = {p.name for p in root.iterdir()
                    if not r.is_report_file(p.name, "")}
            self.assertEqual(kept, {"TEAM-REPORT-R11B.md", "TEAM-REPORT-R11C.md",
                                    "TEAM-REPORT-R11D.md", "PROOF.json"})

    def test_r11d_only_exclusion_keeps_the_earlier_reports(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            for name in ["TEAM-REPORT.md", "TEAM-REPORT-R11B.md",
                         "TEAM-REPORT-R11C.md", "TEAM-REPORT-R11D.md",
                         "PROOF.json"]:
                write(root / name, name)
            kept = {p.name for p in root.iterdir()
                    if not r.is_report_file(p.name, "-R11D")}
            self.assertEqual(kept, {"TEAM-REPORT.md", "TEAM-REPORT-R11B.md",
                                    "TEAM-REPORT-R11C.md", "PROOF.json"})
            self.assertEqual(r.report_path(root, "-R11D").name, "TEAM-REPORT-R11D.md")
            self.assertEqual(r.report_path(root).name, "TEAM-REPORT.md")

    def test_verify_seal_script_accepts_a_preserved_r11_layout(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            names = ["TEAM-REPORT.md", "TEAM-REPORT-R11B.md",
                     "TEAM-REPORT-R11C.md", "TEAM-REPORT-R11D.md",
                     "CONSOLE-PACKAGE.zip"]
            for name in names:
                write(root / name, "content of " + name)
            inventory = {name: {"bytes": (root / name).stat().st_size,
                                "sha256": hashlib.sha256((root / name).read_bytes()).hexdigest()}
                         for name in names if name != "TEAM-REPORT-R11D.md"}
            write(root / "ARTIFACT-SHA256.json", json.dumps(inventory))
            sealed = ["ARTIFACT-SHA256.json", "TEAM-REPORT-R11D.md", "CONSOLE-PACKAGE.zip"]
            seal = {"files": {name: {"bytes": (root / name).stat().st_size,
                                     "sha256": hashlib.sha256((root / name).read_bytes()).hexdigest()}
                              for name in sealed}}
            write(root / "SEAL-R11D.json", json.dumps(seal))
            result = subprocess.run(
                [sys.executable, str(ROOT / "tools/transition_mem_r11/verify_seal.py"), str(root)],
                capture_output=True, text=True,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("PASS detached seal and", result.stdout)

    def test_setup_script_copies_a_preserved_legacy_layout(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            old = root / "alice-transition-mem-r11c"
            out = root / "alice-transition-mem-r11d"
            write(old / "TEAM-REPORT.md", "old report\n")
            write(old / "TEAM-REPORT-R11B.md", "old r11b\n")
            write(old / "TEAM-REPORT-R11C.md", "old r11c\n")
            write(old / "CONSOLE-BRIEF.md", "brief\n")
            result = subprocess.run(
                [sys.executable, str(ROOT / "tools/transition_mem_r11/setup_r11d.py"), str(out)],
                capture_output=True, text=True,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertTrue((out / "REPORT.md").read_text().startswith("> R11d correction"))
            self.assertIn("old report", (out / "REPORT.md").read_text())
            self.assertEqual((out / "REPORT-R11B.md").read_text().splitlines()[-1], "old r11b")
            self.assertEqual((out / "REPORT-R11C.md").read_text().splitlines()[-1], "old r11c")
            self.assertFalse(any("TEAM-REPORT" in p.name for p in out.iterdir()))
            self.assertEqual((out / "CONSOLE-BRIEF.md").read_text(), "brief\n")
            self.assertEqual(json.loads((out / "proof/E2-FIXED-PLAN.json").read_text())["cycles_per_operation"], 30)


if __name__ == "__main__":
    unittest.main()


class RootOnlyExclusionTest(unittest.TestCase):
    """Only root-level seal artifacts leave the R11 inventory; nested copies stay hashed."""

    def test_nested_seal_artifacts_stay_in_inventory(self):
        import tempfile
        from report_files import is_report_file
        with tempfile.TemporaryDirectory() as tmp:
            O = Path(tmp)
            (O / "proof").mkdir()
            for rel in ["ARTIFACT-SHA256.json", "SEAL-R11D.json", "REPORT-R11D.md",
                        "proof/ARTIFACT-SHA256.json", "proof/SEAL-R11D.json", "proof/REPORT-R11D.md",
                        "REPORT.md"]:
                (O / rel).write_text("x")
            kept = sorted(str(p.relative_to(O)) for p in O.rglob("*") if p.is_file() and not (
                p.parent == O and (is_report_file(p.name, "-R11D") or p.name in ["ARTIFACT-SHA256.json", "SEAL-R11D.json"])))
            self.assertEqual(kept, ["REPORT.md", "proof/ARTIFACT-SHA256.json",
                                    "proof/REPORT-R11D.md", "proof/SEAL-R11D.json"])
