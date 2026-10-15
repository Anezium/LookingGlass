#!/usr/bin/env python3
"""Run functional coverage on macOS, preserving raw evidence and shared timing ownership."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time


def utc():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("manifest", type=Path)
    ap.add_argument("--binary", required=True, type=Path)
    ap.add_argument("--data", required=True, type=Path)
    ap.add_argument("--label", required=True)
    args = ap.parse_args()
    manifest = args.manifest.resolve()
    root = manifest.parent
    lock = Path.home() / "vita-j0/mac-timing.lock"
    work = root / "mac-runs" / args.label
    work.mkdir(parents=True, exist_ok=False)
    private = work / "private"
    private.mkdir()
    for name in ("vita_renderer.json", "vita_sim.json"):
        shutil.copy2(root / "configs" / name, private / name)
    env = dict(os.environ, CARGO_BUILD_JOBS="4", LOOKING_GLASS_RENDER_METRICS="1")
    for key in ("LIBGL_ALWAYS_SOFTWARE", "GALLIUM_DRIVER", "LP_NUM_THREADS",
                "LOOKING_GLASS_ROUTE", "LOOKING_GLASS_ACTOR_REPLAY"):
        env.pop(key, None)
    identity = dict(track="coverage", start_utc=utc(), pid=os.getpid())
    owner = json.dumps(identity) + "\n"
    deadline = time.monotonic() + 3600
    while True:
        if time.monotonic() >= deadline:
            raise RuntimeError("shared lock/load wait exceeded one hour")
        load = list(os.getloadavg())
        if lock.exists():
            with (work / "wait.jsonl").open("a") as f:
                f.write(json.dumps(dict(utc=utc(), load=load, lock=lock.read_text() if lock.exists() else None)) + "\n")
            print("WAIT shared timing lock or load", load, flush=True)
            time.sleep(120)
            continue
        try:
            with lock.open("x") as f:
                f.write(owner)
            break
        except FileExistsError:
            continue
    process = None
    report = dict(identity, manifest_sha256=sha(manifest), binary_sha256=sha(args.binary),
                  configs={p.name: sha(p) for p in private.iterdir()},
                  host_timings_are_acceptance=False, timing_qualified=True,
                  load_limit=3, samples=[], command=[str(args.binary.resolve()), "--data",
                  str(args.data.resolve()), "--no-audio", "--coverage-survey", str(manifest)])
    try:
        # Claim ownership before letting an existing build's load decay. Peers
        # finish their in-flight step and see the lock before starting another.
        while max(os.getloadavg()) > 3:
            if time.monotonic() >= deadline:
                raise RuntimeError("host load qualification exceeded one hour")
            with (work / "wait.jsonl").open("a") as f:
                f.write(json.dumps(dict(utc=utc(), load=list(os.getloadavg()))) + "\n")
            print("WAIT host load under coverage lock", list(os.getloadavg()), flush=True)
            time.sleep(30)
        with (work / "run.log").open("w") as log:
            process = subprocess.Popen(report["command"], cwd=work, env=env,
                                       stdout=log, stderr=subprocess.STDOUT)
            end = time.monotonic() + 7230
            while True:
                load = list(os.getloadavg())
                report["samples"].append(dict(utc=utc(), load=load))
                if max(load) > 3:
                    report["timing_qualified"] = False
                status = process.poll()
                if status is not None:
                    report["exit_code"] = status
                    break
                if time.monotonic() > end:
                    raise RuntimeError("external campaign deadline")
                time.sleep(1)
        # CLI success alone is not fixture success. Preserve the authoritative cells.
        outputs = [line.split("output=", 1)[1].split()[0]
                   for line in (work / "run.log").read_text().splitlines()
                   if "[lg/coverage] done output=" in line]
        if outputs:
            report["output"] = outputs[-1]
            report["results"] = json.loads((Path(outputs[-1]) / "results.json").read_text())
        report["completed_utc"] = utc()
        report["timing_disposition"] = ("qualified host mechanics only" if report["timing_qualified"]
                                         else "rejected for timing; raw functional evidence retained")
    finally:
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        (work / "campaign.json").write_text(json.dumps(report, indent=2) + "\n")
        if lock.exists() and lock.read_text() == owner:
            lock.unlink()
    print(json.dumps({k: v for k, v in report.items() if k != "samples"}), flush=True)
    if report.get("exit_code") != 0 or "results" not in report:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
