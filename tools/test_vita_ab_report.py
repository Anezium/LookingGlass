"""Synthetic fixtures use frame_profile.rs, route_replay/mod.rs and Miniquad's
native/vita/mod.rs output formats; no console/game data is needed.
"""
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest

import vita_ab_report as ab


class ReportTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.log = self.root / "defaults.log"

    def write(self, name, value):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value), encoding="utf-8")

    def parse(self, text):
        self.log.write_text(text, encoding="utf-8")
        return ab.parse_run(self.log)

    def test_native_raw_frames_and_cumulative_snapshots(self):
        for n in (10, 20):
            self.write(f"frame_profile/gvillage-fullbench-{n}.json", {
                "map": "gvillage", "mode": "fullbench", "gameplay_frames": n,
                "frame_wall_ms": {"frame_values_ms": list(range(n))},
                "spans": {"frame_cpu": {"frame_values_ms": [2] * n}, "characters_pose": {"frame_values_ms": [0.1] * n}, "skinning": {"frame_values_ms": [0.2] * n}},
                "counters": {"actors/skinned_draws_total": {"total": 100}, "actors/skinned_draws_culled": {"total": 25}, "vita_fx_index_uploads_skipped": {"total": 30}, "gpu_probe_upload_requests": {"total": 100}, "vita_fx_uniform_sets_skipped": {"total": 80}, "gpu_probe_uniform_requests": {"total": 100}},
                "heap": {"frame_end_bytes": [1000, 2000, 1500]},
            })
        r = self.parse("[lg/profile] report ux0:data/alice/private/frame_profile/gvillage-fullbench-10.json\n[lg/profile] report ux0:data/alice/private/frame_profile/gvillage-fullbench-20.json\n")
        self.assertEqual(r["frame wall ms median / p90"], "10.000 / 18.000 (n=20)")
        self.assertEqual(r["characters_pose ms median / p90"], "0.100 / 0.100 (n=20)")
        self.assertEqual(r["skin cull ratio"], "25.0%")
        self.assertEqual(r["GPU index skip ratio"], "60.0%")
        self.assertEqual(r["GPU uniform skip ratio"], "80.0%")
        self.assertEqual(r["newlib heap sampled high-water bytes"], "2000")

    def test_window_means_are_not_frame_quantiles(self):
        r = self.parse('[lg/profile] window {"mode":"fullbench","map":"gvillage","gameplay_frames_end":10,"frame_cpu_mean_ms":9.5,"heap_uordblks_bytes":1234,"counts":{}}\n')
        self.assertNotIn("frame_cpu ms median / p90", r)
        self.assertEqual(r["newlib heap sampled high-water bytes"], "1234")

    def test_minquad_heap_startup_and_route_options(self):
        r = self.parse('[miniquad/vita] memory reason=tick time_us=12 kernel_rc=0 user=10 cdram=10 phycont=10 newlib_arena=100 newlib_uordblks=90 newlib_fordblks=10 vgl_VRAM=20 vgl_RAM=30 vgl_PHYCONT=40 vgl_BUDGET=50 vgl_EXTERNAL=0\n[lg/startup] map_load textures elapsed_ms=31.000 total_ms=42.000\n[lg/startup] map_load textures work=texture_upload calls=4 elapsed_ms=20.000\n[lg/route] ready map=gvillage heap_start_bytes=Some(200)\n[lg/route] heap end start=Some(200) end=Some(150)\n')
        self.assertEqual(r["newlib heap sampled high-water bytes"], "200")
        self.assertEqual(r["load: map_load textures elapsed_ms"], "31.000")
        self.assertEqual(r["load: map_load textures work=texture_upload calls=4 elapsed_ms"], "20.000")

    def route(self, windows=True):
        self.write("route-report.json", {"run_id": "replay-123", "raw_windows": 2, "frame_wall_ms": {"samples": 4, "median_ms_rounded": 20, "p95_ms_rounded": 99}, "skinned_draws_total": {"mean": 10}, "skinned_draws_culled": {"mean": 2}, "gpu_probe_upload_requests": {"mean": 10}, "gpu_probe_identical_uploads": {"mean": 5}, "heap": {"start_bytes": 100, "end_bytes": 150}})
        if windows:
            self.write("route-frame-windows/replay-123/window-00001.json", [["walk", 10], ["scene-wait", 20]])
            self.write("route-frame-windows/replay-123/window-00002.json", [["walk", 30], ["save-or-continue", 40]])
        return self.parse("[lg/route] start mode=replay save_dir=route-saves\n[lg/route] PASS saved map=gvillage feet=Vec3(1, 2, 3) moving_s=61.000 post_knife_cat_s=60.000\n")

    def test_route_exact_windows_and_probe_not_skip(self):
        r = self.route()
        self.assertEqual(r["route wall ms median / p90"], "30.000 / 40.000 (n=4)")
        self.assertEqual(r["route markers"], "PASS saved")
        self.assertEqual(r["route skin cull ratio"], "20.0%")
        self.assertEqual(r["GPU upload identical probe ratio"], "50.0%")
        self.assertNotIn("GPU index skip ratio", r)

    def test_missing_window_keeps_p90_unknown(self):
        r = self.route(False)
        self.assertEqual(r["route wall ms median / p90"], "20.000 / — (n=4) [raw windows missing]")

    def test_p95_never_substitutes_for_p90(self):
        self.assertEqual(ab.timing({"samples": 5, "median_ms": 2, "p95_ms": 7}), "2.000 / — (n=5)")

    def test_missing_pass_and_short_route_fail(self):
        self.assertEqual(self.parse("[lg/route] start mode=replay\n")["route markers"], "FAIL missing PASS")
        self.assertEqual(self.parse("[lg/route] PASS saved map=gvillage moving_s=61.000 post_knife_cat_s=59.000\n")["route markers"], "FAIL")

    def test_crash_after_pass_still_fails(self):
        r = self.parse("[lg/route] PASS saved map=gvillage moving_s=61.000 post_knife_cat_s=60.000\nSegmentation fault (core dumped)\n")
        self.assertEqual(r["route markers"], "FAIL")

    def test_continue_and_drift_are_separate(self):
        r = self.parse("[lg/route] replay drift tick=1062 segment=x expected=Some([1, 2, 3]) actual=Vec3(1, 2, 4)\n[lg/route] PASS Continue map=gvillage feet=Vec3(1, 2, 3)\n")
        self.assertEqual(r["route markers"], "PASS Continue")
        self.assertEqual(r["route drift marker"], "DRIFT")

    def test_no_ratio_when_requests_are_zero(self):
        self.assertEqual(ab.ratio(0, 0), "—")
        self.assertEqual(ab.ratio(2, 1), "INVALID counters")

    def test_missing_report_path_visible(self):
        r = self.parse("[lg/profile] report ux0:data/alice/private/frame_profile/gvillage-120.json\n")
        self.assertEqual(r["missing profile artifacts"], "gvillage-120.json")

    def test_utf16_console_capture(self):
        self.log.write_bytes("[lg/route] start mode=replay\n".encode("utf-16"))
        self.assertEqual(ab.parse_run(self.log)["route markers"], "FAIL missing PASS")

    def test_markdown_and_cli_multiple_runs(self):
        self.log.write_text("[lg/route] start mode=replay\n")
        other = self.root / "exact.log"
        other.write_text("[lg/route] PASS Continue map=gvillage feet=Vec3(1, 2, 3)\n")
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            code = ab.main([str(self.log), str(other), "--label", "defaults|A", "--label", "exact"])
        self.assertEqual(code, 1)
        self.assertTrue(output.getvalue().startswith("| Metric | defaults\\|A | exact |"))
        self.assertIn("FAIL missing PASS | PASS Continue", output.getvalue())

    def test_bad_artifact_reports_path_and_nonzero_exit(self):
        self.log.write_text("[lg/profile] report ux0:data/alice/private/frame_profile/broken.json\n")
        path = self.root / "frame_profile/broken.json"
        path.parent.mkdir()
        path.write_text("{bad")
        with contextlib.redirect_stderr(io.StringIO()) as err, self.assertRaises(SystemExit) as raised:
            ab.main([str(self.log)])
        self.assertEqual(raised.exception.code, 2)
        self.assertIn("broken.json", err.getvalue())


if __name__ == "__main__":
    unittest.main()
