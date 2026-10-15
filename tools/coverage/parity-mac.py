#!/usr/bin/env python3
"""Compare the 109 existing frozen renderer fixtures on native Mac OpenGL."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
from PIL import Image, ImageChops


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--evidence", required=True, type=Path)
    ap.add_argument("--baseline-binary", required=True, type=Path)
    ap.add_argument("--binary", required=True, type=Path)
    ap.add_argument("--data", required=True, type=Path)
    ap.add_argument("--label", required=True)
    args = ap.parse_args()
    root = args.evidence.resolve()
    output = root / "mac-parity" / args.label
    output.mkdir(parents=True, exist_ok=False)
    lock = Path.home() / "vita-j0/mac-timing.lock"
    owner = json.dumps(dict(track="coverage-parity", pid=os.getpid(),
                           start_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                           purpose="frozen image equality; no timing claim")) + "\n"
    deadline = time.monotonic() + 3600
    while True:
        try:
            with lock.open("x") as f:
                f.write(owner)
            break
        except FileExistsError:
            if time.monotonic() >= deadline:
                raise RuntimeError("shared timing lock wait exceeded one hour")
            print("WAIT shared timing lock", flush=True)
            time.sleep(120)
    report = dict(baseline_commit="86792fe186736125219de139ee1ddec44dc356fb",
                  baseline_binary_sha256=sha(args.baseline_binary), binary_sha256=sha(args.binary),
                  console_access=False, timing_claim=False, suites={})
    env = dict(os.environ, LOOKING_GLASS_HOST_CLOCK_STEP="0", LOOKING_GLASS_RENDER_METRICS="1")
    for key in ("LIBGL_ALWAYS_SOFTWARE", "GALLIUM_DRIVER", "LP_NUM_THREADS", "LOOKING_GLASS_ROUTE", "LOOKING_GLASS_ACTOR_REPLAY"):
        env.pop(key, None)
    try:
        for suite, flag, subdir, count in (
                ("reference", "renderer-host-check", "renderer-host", 15),
                ("alpha", "vita-alpha-check", "retained-alpha", 22),
                ("scenes", "skin-overlap-render-check", "skin-overlap", 72)):
            captures = {}
            for arm, binary in (("pkg2", args.baseline_binary), ("coverage", args.binary)):
                work = output / (suite + "-" + arm)
                (work / "private/saves").mkdir(parents=True)
                shutil.copy2(root / "fixtures/v0-reference.json", work / "private/saves/auto.json")
                for name in ("vita_renderer.json", "vita_sim.json"):
                    shutil.copy2(root / "configs" / name, work / "private" / name)
                command = [str(binary.resolve()), "--data", str(args.data.resolve()), "--no-audio", "--" + flag]
                with (work / "run.log").open("w") as log:
                    log.write(repr(command) + "\n")
                    log.flush()
                    status = subprocess.run(command, cwd=work, env=env, stdout=log,
                                            stderr=subprocess.STDOUT, timeout=1200).returncode
                assert status == 0, (suite, arm, status)
                assert "invalid config" not in (work / "run.log").read_text().lower()
                paths = {p.name: p for p in (work / "private" / subdir).glob("*.png")}
                assert len(paths) == count, (suite, arm, len(paths), count)
                captures[arm] = paths
            assert set(captures["pkg2"]) == set(captures["coverage"])
            comparisons = {}
            for name, baseline in sorted(captures["pkg2"].items()):
                current = captures["coverage"][name]
                with Image.open(baseline) as a, Image.open(current) as b:
                    assert a.size == b.size, name
                    extrema = b.convert("RGB").getextrema()
                    nonuniform = any(lo != hi for lo, hi in extrema)
                    if suite != "alpha":
                        assert nonuniform, (suite, name, "blank renderer proof")
                    equal = baseline.read_bytes() == current.read_bytes()
                    pixels_equal = not any(channel.getbbox() for channel in ImageChops.difference(a.convert("RGBA"), b.convert("RGBA")).split())
                    comparisons[name] = dict(byte_identical=equal, rgba_equal=pixels_equal,
                                             baseline_sha256=sha(baseline), sha256=sha(current),
                                             size=list(b.size), nonuniform=nonuniform)
            report["suites"][suite] = dict(captures=count, comparison=comparisons,
                                           byte_identical=all(x["byte_identical"] for x in comparisons.values()),
                                           rgba_equal=all(x["rgba_equal"] for x in comparisons.values()))
            (output / "RESULT.json").write_text(json.dumps(report, indent=2) + "\n")
            assert report["suites"][suite]["byte_identical"] and report["suites"][suite]["rgba_equal"], suite
            print("PASS", suite, count, "byte-identical, RGBA equal", flush=True)
        assert sum(s["captures"] for s in report["suites"].values()) == 109
    finally:
        if lock.exists() and lock.read_text() == owner:
            lock.unlink()


if __name__ == "__main__":
    main()
