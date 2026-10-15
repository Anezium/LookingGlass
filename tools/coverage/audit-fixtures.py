#!/usr/bin/env python3
"""Extract observed fixture facts; never turn failed cells into coverage."""
import argparse
import csv
import hashlib
import json
from pathlib import Path
from PIL import Image


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("root", type=Path)
    args = ap.parse_args()
    root = args.root
    manifest = json.loads((root / "effective-manifest.json").read_text())
    results = json.loads((root / "results.json").read_text())
    columns = ("guards_alive guards_active guards_in_view_cone guard_attacks guard_deaths "
               "guards_dismembered weapon_shots weapon_impacts level_alive level_active "
               "level_species level_parts level_effects skin_fallbacks skin_overflow "
               "world_visible_groups puffs_live skin_logical_draws").split()
    audit = dict(console_acceptance=False, cells=[])
    for result in results:
        prefix = result["scene"] + "-r" + str(result["repeat"])
        scene = next(s for s in manifest["scenes"] if s["id"] == result["scene"])
        cell = dict(result=result, family=scene["family"], expected=scene["expected"],
                    all_frames=0, timed_frames=0, epochs={}, states={})
        for path in sorted(root.glob(prefix + "*-light-*.csv")):
            epoch = path.name.split("-light-", 1)[0]
            observed = cell["epochs"].setdefault(epoch, dict(frames=0, timed_frames=0, maxima={}))
            with path.open() as f:
                for row in csv.DictReader(f):
                    cell["all_frames"] += 1
                    observed["frames"] += 1
                    if int(row["timed"]):
                        cell["timed_frames"] += 1
                        observed["timed_frames"] += 1
                        for name in columns:
                            observed["maxima"][name] = max(observed["maxima"].get(name, 0), int(row[name]))
        for which in ("initial", "final"):
            path = root / (prefix + "-" + which + "-state.json")
            if path.exists():
                state = json.loads(path.read_text())
                level = state["campaign"]["levels"][state["current"]]
                cell["states"][which] = dict(sha256=sha(path), current=state["current"],
                                               feet=state["player"]["feet"], view=state["view"],
                                               environment_clock=level["environment_clock"])
                if which == "initial" and scene["school_clubs"]:
                    actors = [a for a in level["npcs"]["actors"] if a.get("guard")]
                    guards = [dict(id=a["spawn"]["name"], model=a["spawn"]["model"],
                                   health=a["guard"]["health"], feet=a["guard"]["feet"],
                                   notarget=a["guard"]["notarget"])
                              for a in actors]
                    assert len(guards) == len(scene["school_clubs"])
                    assert len({g["id"] for g in guards}) == len(guards)
                    assert len({g["model"] for g in guards}) == 1
                    assert all(g["health"] > 0 and not g["notarget"] for g in guards)
                    cell["initial_guards"] = guards
        validation = root / (prefix + "-validation.jsonl")
        if validation.exists():
            records = [json.loads(line) for line in validation.read_text().splitlines()]
            cell["validation"] = dict(identity=records[0], last=records[-1])
        png = root / (prefix + "-verification.png")
        if png.exists():
            cell["verification_png_sha256"] = sha(png)
            with Image.open(png) as image:
                cell["verification_size"] = list(image.size)
                cell["verification_rgb_extrema"] = image.convert("RGB").getextrema()
                cell["verification_nonuniform"] = any(lo != hi for lo, hi in cell["verification_rgb_extrema"])
        epochs = root / (prefix + "-epochs.jsonl")
        cell["completed_intermediate_epochs"] = (
            [json.loads(line) for line in epochs.read_text().splitlines()] if epochs.exists() else [])
        if result["status"] == "complete":
            assert cell["timed_frames"] > 0, prefix + " missing timed rows"
            assert set(cell["states"]) == {"initial", "final"}, prefix + " missing state proof"
            assert "verification_png_sha256" in cell, prefix + " missing host verification image"
            assert cell["verification_nonuniform"], prefix + " uniform/blank verification image"
            assert len(cell["completed_intermediate_epochs"]) == scene["epochs"] - 1, prefix + " incomplete epochs"
        audit["cells"].append(cell)
    (root / "fixture-audit.json").write_text(json.dumps(audit, indent=2) + "\n")
    for cell in audit["cells"]:
        maxima = {key: max((epoch["maxima"].get(key, 0) for epoch in cell["epochs"].values()), default=0)
                  for key in columns}
        print(cell["result"]["scene"], cell["result"]["repeat"], cell["result"]["status"],
              "timed_frames=" + str(cell["timed_frames"]), json.dumps(maxima))


if __name__ == "__main__":
    main()
