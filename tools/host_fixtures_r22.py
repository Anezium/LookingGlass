#!/usr/bin/env python3
"""Run the two host semantic fixtures in both r22 lever arms and compare them.

Each process is one arm, because the levers come from private/vita_renderer.json at
startup. A pass needs every nominal assertion to hold, ON and OFF to match in serialized
state on every tick, and every negative control to fail for its own expected reason.
"""
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

FIXTURES = {
    "potears2-sight": {
        "flag": "--potears2-sight-fixture",
        "negatives": {
            "no-cover": "still sees Alice in cover",
            "short-cover": "memory did not expire behind cover",
            "no-reexpose": "did not reacquire Alice",
        },
    },
    "potears2-sight-30hz": {
        "flag": "--potears2-sight-fixture-30hz",
        "negatives": {
            "no-cover": "still sees Alice in cover",
            "short-cover": "memory did not expire behind cover",
            "no-reexpose": "did not reacquire Alice",
        },
    },
    "hatter1-door": {
        "flag": "--hatter1-door-fixture",
        "negatives": {
            "no-use": "did not hold door",
            "no-turn": "Out of step-away sequence",
            "no-occluder": "Behind geometry: prompt",
        },
    },
}
ARMS = {"on": "PL-r22-human.json", "off": "PL-r22-human-levers-off.json"}
LEVERS = ["sim_trigger_facts_reuse", "sim_sight_query_reuse", "sim_hatter_door_candidates",
          "actor_publication_occurrences", "actor_deferred_layers"]


def run(binary, data, configs, out, fixture, arm, variant, label="", probe=False):
    cwd = out / fixture / f"{arm}{label}" / variant
    if cwd.exists():
        shutil.rmtree(cwd)
    (cwd / "private").mkdir(parents=True)
    config = configs / ARMS[arm]
    shutil.copyfile(config, cwd / "private/vita_renderer.json")
    env = dict(os.environ, LOOKING_GLASS_FIXTURE_VARIANT=variant)
    env.pop("LOOKING_GLASS_SIM_MAPS_PROBE", None)
    if probe:
        env["LOOKING_GLASS_SIM_MAPS_PROBE"] = "1"
    with (cwd / "run.log").open("wb") as log:
        code = subprocess.run([str(binary), "--data", str(data), FIXTURES[fixture]["flag"]], cwd=cwd, env=env,
                              stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, timeout=300).returncode
    stem = cwd / "private/host-fixtures" / f"{fixture}-{variant}"
    report = json.loads(Path(f"{stem}.json").read_text())
    report["exit"] = code
    report["final_bytes_sha256"] = hashlib.sha256(Path(f"{stem}-final.json").read_bytes()).hexdigest()
    report["config_sha256"] = hashlib.sha256(config.read_bytes()).hexdigest()
    return report


def same(a, b):
    return (a["state_stream_sha256"] == b["state_stream_sha256"] and a["ticks"] == b["ticks"]
            and a["final_bytes_sha256"] == b["final_bytes_sha256"] and a["events"] == b["events"])


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--binary", required=True, type=Path)
    p.add_argument("--probe-binary", type=Path)
    p.add_argument("--data", required=True, type=Path)
    p.add_argument("--configs", required=True, type=Path)
    p.add_argument("--out", required=True, type=Path)
    a = p.parse_args()
    args = (a.data, a.configs, a.out)
    failures = []

    def expect(condition, message):
        print(("ok   " if condition else "FAIL ") + message, flush=True)
        if not condition:
            failures.append(message)

    if a.probe_binary and a.probe_binary.read_bytes() == a.binary.read_bytes():
        sys.exit("The probe binary is the plain binary: a failed probe build would make its checks vacuous")
    summary = {}
    for fixture, spec in FIXTURES.items():
        on = run(a.binary, *args, fixture, "on", "nominal")
        off = run(a.binary, *args, fixture, "off", "nominal")
        again = run(a.binary, *args, fixture, "on", "nominal", label="-repeat")
        expect(on["exit"] == 0 and on["passed"], f"{fixture} ON nominal passes: {on['failure']}")
        expect(off["exit"] == 0 and off["passed"], f"{fixture} OFF nominal passes: {off['failure']}")
        expect(all(on["levers"][k] is True for k in LEVERS), f"{fixture} ON arm has all five levers on")
        expect(all(off["levers"][k] is False for k in LEVERS), f"{fixture} OFF arm has all five levers off")
        expect(same(on, off), f"{fixture} ON and OFF match on every tick")
        expect(same(on, again), f"{fixture} ON repeats in a fresh process")
        shifted = run(a.binary, *args, fixture, "on", "timing-shift")
        expect(shifted["exit"] == 0, f"{fixture} timing-shift passes its own assertions: {shifted['failure']}")
        expect(not same(on, shifted), f"{fixture} comparison catches a one-tick input shift")
        result = {"ticks": on["ticks"], "stream_sha256": on["state_stream_sha256"],
                  "final_sha256": on["final_bytes_sha256"],
                  "off_stream_sha256": off["state_stream_sha256"], "off_final_sha256": off["final_bytes_sha256"],
                  "repeat_stream_sha256": again["state_stream_sha256"],
                  "timing_shift_stream_sha256": shifted["state_stream_sha256"],
                  "config_sha256": {"on": on["config_sha256"], "off": off["config_sha256"]},
                  "summary": on["summary"], "events": on["events"], "negatives": {}}
        for variant, reason in spec["negatives"].items():
            for arm in ARMS:
                bad = run(a.binary, *args, fixture, arm, variant)
                failed = bad["exit"] != 0 and not bad["passed"] and reason in (bad["failure"] or "")
                expect(failed, f"{fixture} {variant} {arm} fails for its reason: {bad['failure']}")
                result["negatives"][f"{variant}/{arm}"] = {"exit": bad["exit"], "failure": bad["failure"], "ticks": bad["ticks"]}
        if a.probe_binary:
            pon = run(a.probe_binary, *args, fixture, "on", "nominal", label="-probe", probe=True)
            poff = run(a.probe_binary, *args, fixture, "off", "nominal", label="-probe", probe=True)
            expect(pon["counters"] != {}, f"{fixture} probe build reports counters")
            expect(same(on, pon) and same(off, poff), f"{fixture} probe build matches the plain build")
            result["counters"] = {"on": pon["counters"], "off": poff["counters"]}
            if fixture.startswith("potears2-sight"):
                expect(pon["counters"].get("sight_reuse_misses", 0) > 0, f"{fixture} ON ran the sight reuse scope")
                expect(poff["counters"].get("sight_reuse_misses", 0) == 0
                       and poff["counters"].get("sight_reuse_hits", 0) == 0, f"{fixture} OFF never entered the scope")
            if fixture == "potears2-sight-30hz":
                expect(pon["counters"].get("sight_reuse_hits", 0) > 0, f"{fixture} ON reused sight traces")
            if fixture == "hatter1-door":
                # The fixture itself calls door_pick_mode in both modes at four points, so
                # OFF is not exactly zero. The lever's effect is the trace count.
                on_c, off_c = pon["counters"], poff["counters"]
                considered = off_c["hatter_door_considered"]
                expect(on_c["hatter_door_considered"] == considered, "hatter1 arms considered the same doors")
                expect(off_c["hatter_door_trace_pairs_deferred"] * 100 < considered, "hatter1 OFF traced eagerly")
                expect(on_c["hatter_door_world_traces"] * 10 < off_c["hatter_door_world_traces"], "hatter1 ON deferred door traces")
        summary[fixture] = result
    summary["failures"] = failures
    a.out.mkdir(parents=True, exist_ok=True)
    (a.out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print("ALL PASS" if not failures else f"{len(failures)} FAILURES")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
