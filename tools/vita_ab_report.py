#!/usr/bin/env python3
"""Compare console runs using their logs and adjacent downloaded JSON artifacts.

No quantile is inferred from window means or p95. Native route mode deliberately
disables frame_cpu/character spans; run fullbench separately to obtain those.
"""
import argparse
import json
import math
from pathlib import Path
import re
import sys


def number(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def quantiles(values):
    values = sorted(v for v in values if number(v) and v >= 0)
    n = len(values)
    return (values[n // 2], values[n * 90 // 100], n) if n else (None, None, 0)


def timing(entry):
    if not isinstance(entry, dict):
        return "—"
    raw = entry.get("frame_values_ms")
    if isinstance(raw, list):
        median, p90, n = quantiles(raw)
    else:
        stats = entry.get("per_frame", entry)
        median = stats.get("median_ms", stats.get("median_ms_rounded"))
        p90 = stats.get("p90_ms")
        n = stats.get("samples", 0)
    fmt = lambda v: f"{v:.3f}" if number(v) else "—"
    return f"{fmt(median)} / {fmt(p90)} (n={n})" if n else "—"


def ratio(numerator, denominator):
    if not number(numerator) or not number(denominator) or denominator <= 0:
        return "—"
    if numerator < 0 or numerator > denominator:
        return "INVALID counters"
    return f"{100 * numerator / denominator:.1f}%"


def read_json(path):
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise ValueError(f"{path}: {error}") from error


def local_path(root, logged):
    # Relocate Vita paths instead of opening paths on the connected console.
    prefix = "ux0:data/alice/private/"
    if logged.startswith(prefix):
        relative = Path(logged[len(prefix):])
        if ".." in relative.parts:
            raise ValueError("parent traversal in artifact path")
        for candidate in (root / relative, root / "private" / relative):
            if candidate.exists():
                return candidate
        return root / relative
    path = Path(logged)
    return path if path.is_absolute() else root / path


def counter_total(counters, name):
    matches = [entry for key, entry in counters.items() if key.split("/")[-1] == name]
    if not matches:
        return None
    totals = [entry.get("total") for entry in matches]
    return sum(totals) if all(number(v) for v in totals) else None


def parse_run(log_path, root=None):
    root = root or log_path.parent
    raw = log_path.read_bytes()
    text = raw.decode("utf-16" if raw.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig", errors="replace")
    metrics = {}
    heaps = []
    markers = {}
    profiles = []
    route_started = route_saved = route_continue = failed = drift = False
    for line in text.splitlines():
        if "[lg/startup]" in line:
            content = line.split("[lg/startup]", 1)[1].strip()
            # WorkTimer rows carry their own distinct work= labels.
            match = re.search(r"\belapsed_ms=([\d.]+)", content)
            if match:
                label = content[:match.start()].strip()
                markers.setdefault(label, []).append(float(match[1]))
        if "[miniquad/vita] memory reason=" in line:
            match = re.search(r"\bnewlib_uordblks=(\d+)", line)
            if match:
                heaps.append(int(match[1]))
        if "[lg/profile] window " in line:
            window = json.loads(line.split("[lg/profile] window ", 1)[1])
            if number(window.get("heap_uordblks_bytes")):
                heaps.append(window["heap_uordblks_bytes"])
        if "[lg/profile] report " in line:
            profiles.append(local_path(root, line.split("[lg/profile] report ", 1)[1].strip()))
        if "[lg/route]" in line:
            route_started |= "[lg/route] start " in line
            drift |= "replay drift " in line
            failed |= "FAIL" in line
            if "PASS saved " in line:
                match = re.search(r"map=(\S+).*moving_s=([\d.]+) post_knife_cat_s=([\d.]+)", line)
                valid = bool(match and match[1] == "gvillage" and float(match[2]) >= 60 and float(match[3]) >= 60)
                route_saved |= valid
                failed |= not valid
            route_continue |= "PASS Continue map=gvillage " in line
            heaps.extend(int(v) for v in re.findall(r"(?:start|end|heap_start_bytes)=Some\((\d+)\)", line))
        failed |= any(token in line for token in ("Looking Glass:", "panicked at", "data abort", "Segmentation fault"))

    # Reports are cumulative: select the largest snapshot per map/mode, never
    # concatenate snapshots. Missing paths stay visible in the table.
    if not profiles:
        profiles = sorted(set(root.glob("frame_profile/*.json")) | set(root.glob("private/frame_profile/*.json")) | set(root.glob("profile/*.json")))
    selected = {}
    missing = []
    for path in dict.fromkeys(profiles):
        if not path.exists():
            missing.append(path.name)
            continue
        report = read_json(path)
        key = (report.get("map", path.stem), report.get("mode", "host"))
        if key not in selected or report.get("gameplay_frames", 0) >= selected[key].get("gameplay_frames", 0):
            selected[key] = report
    for (map_name, mode), report in selected.items():
        scope = f" [{map_name}/{mode}]" if len(selected) > 1 else ""
        if "frame_wall_ms" in report:
            metrics["frame wall ms median / p90" + scope] = timing(report["frame_wall_ms"])
        spans = report.get("spans", report)  # historical host name -> summary
        for name, entry in spans.items():
            if name in ("frame_cpu", "frame_characters", "skinning") or name.startswith(("characters_", "skinning_")):
                metrics[f"{name} ms median / p90" + scope] = timing(entry)
        heaps.extend(v for v in report.get("heap", {}).get("frame_end_bytes", []) if number(v))
        counters = report.get("counters", {})
        metrics["skin cull ratio" + scope] = ratio(counter_total(counters, "skinned_draws_culled"), counter_total(counters, "skinned_draws_total"))
        for label, requested, skipped in (
            ("GPU index skip ratio", "gpu_probe_upload_requests", "vita_fx_index_uploads_skipped"),
            ("GPU uniform skip ratio", "gpu_probe_uniform_requests", "vita_fx_uniform_sets_skipped"),
        ):
            requests = counter_total(counters, requested)
            # The upload probe counts one vertex and one index request per mesh.
            if label == "GPU index skip ratio" and number(requests):
                requests /= 2
            metrics[label + scope] = ratio(counter_total(counters, skipped), requests)

    route_path = root / ("route-continue-report.json" if route_continue and not route_saved else "route-report.json")
    if not route_path.exists():
        route_path = root / "private" / route_path.name
    if route_started and route_path.exists():
        report = read_json(route_path)
        windows = report.get("raw_windows", 0)
        run_id = report.get("run_id", "")
        if not isinstance(run_id, str) or not run_id or Path(run_id).name != run_id:
            raise ValueError(f"{route_path}: invalid run_id")
        if not isinstance(windows, int) or not 0 <= windows <= 100_000:
            raise ValueError(f"{route_path}: invalid raw_windows")
        values = []
        complete = True
        for i in range(1, windows + 1):
            path = route_path.parent / "route-frame-windows" / run_id / f"window-{i:05}.json"
            if not path.exists():
                complete = False
                break
            values.extend(row[1] for row in read_json(path))
        metrics["route wall ms median / p90"] = timing({"frame_values_ms": values}) if complete and values else timing(report.get("frame_wall_ms")) + " [raw windows missing]"
        # Route probe ratios describe identical REQUESTS, not actual skipped calls.
        mean = lambda name: report.get(name, {}).get("mean")
        metrics["route skin cull ratio"] = ratio(mean("skinned_draws_culled"), mean("skinned_draws_total"))
        metrics["GPU upload identical probe ratio"] = ratio(mean("gpu_probe_identical_uploads"), mean("gpu_probe_upload_requests"))
        metrics["GPU uniform identical probe ratio"] = ratio(mean("gpu_probe_identical_uniforms"), mean("gpu_probe_uniform_requests"))
        for name in ("scene_startup_seconds", "including_scene_startup_seconds", "save_cpu_ms"):
            if number(report.get(name)):
                metrics[name] = f"{report[name]:.3f}"
        heaps.extend(v for key, v in report.get("heap", {}).items() if key in ("start_bytes", "end_bytes") and number(v))
    metrics["newlib heap sampled high-water bytes"] = str(max(heaps)) if heaps else "—"
    metrics["route markers"] = "FAIL" if failed else "PASS saved" if route_saved else "PASS Continue" if route_continue else "FAIL missing PASS" if route_started else "MISSING"
    metrics["route drift marker"] = "DRIFT" if drift else "none logged"
    metrics["missing profile artifacts"] = ", ".join(missing) if missing else "none referenced"
    for label, values in markers.items():
        metrics[f"load: {label} elapsed_ms"] = ", ".join(f"{v:.3f}" for v in values)
    return metrics


def markdown(names, runs):
    escape = lambda s: str(s).replace("|", "\\|").replace("\n", " ")
    rows = ["| Metric | " + " | ".join(map(escape, names)) + " |", "|---|" + "---|" * len(names)]
    required = ["frame wall ms median / p90", "frame_cpu ms median / p90", "frame_characters ms median / p90", "skinning ms median / p90", "skin cull ratio", "GPU index skip ratio", "GPU uniform skip ratio", "newlib heap sampled high-water bytes", "route markers", "route drift marker"]
    keys = list(dict.fromkeys(required + sorted(set().union(*(r.keys() for r in runs)))))
    for key in keys:
        rows.append("| " + escape(key) + " | " + " | ".join(escape(run.get(key, "—")) for run in runs) + " |")
    return "\n".join(rows) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("logs", type=Path, nargs="+", help="one log per preset; artifacts adjacent by default")
    parser.add_argument("--artifact-root", type=Path, action="append", help="repeat once per log, in the same order")
    parser.add_argument("--label", action="append", help="repeat once per log, in the same order")
    args = parser.parse_args(argv)
    for option, values in (("--artifact-root", args.artifact_root), ("--label", args.label)):
        if values and len(values) != len(args.logs):
            parser.error(f"{option} must appear once per log")
    try:
        runs = [parse_run(path, args.artifact_root[i] if args.artifact_root else None) for i, path in enumerate(args.logs)]
    except (OSError, ValueError, TypeError, KeyError, IndexError) as error:
        parser.exit(2, f"vita_ab_report: {error}\n")
    sys.stdout.write(markdown(args.label or [p.stem for p in args.logs], runs))
    return 1 if any(run["route markers"].startswith("FAIL") for run in runs) else 0


if __name__ == "__main__":
    sys.exit(main())
