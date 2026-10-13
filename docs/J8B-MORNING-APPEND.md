## Host-side console A/B report (milestone 8b)

Append this section to the existing `jalon-8/README-test.md`. It is staged here
because the implementation brief restricts writes to the `sol-j8b` worktree.

Collect one console log per preset. Preserve separate directories for each run;
download `frame_profile/`, `route-report.json`, and the matching
`route-frame-windows/<run_id>/` files from `ux0:data/alice/private/` into the same
directory as that log. Leave artifact names unchanged. Then, from the repository:

```sh
python3 tools/vita_ab_report.py \
  captures/defaults/console.log captures/exact/console.log \
  captures/lean/console.log captures/gpu/console.log \
  --label defaults --label exact --label lean --label gpu > captures/ab-report.md
python3 -m unittest discover -s tools -p test_vita_ab_report.py
```

For logs stored elsewhere, repeat `--artifact-root PATH` once per log, in the same
order. Both a directly downloaded private directory and a `private/` subdirectory
are recognized. Vita report paths in logs are relocated to those local directories.

The single Markdown table uses upper medians `sorted[n/2]` and p90
`sorted[floor(n*0.90)]`, with sample counts. It takes the largest cumulative
profiler snapshot per map/mode, retaining distinct populations as separate rows.
Characters/skinning spans use **per-frame** samples on Vita; historical host
summary medians retain their original invocation population and cannot supply p90.
Route wall quantiles include scene/camera waits and the final save interval, as the
route report does. Every exported window must be present for exact route quantiles.
No p90 is inferred from p95 or ten-frame mean CPU windows. `—` means unavailable.

Native route mode disables the cumulative CPU profiler. For frame_cpu and
characters_* measurements, collect a separate fullbench run with `frame_profile/`
enabled and download its reports. Log-only runs still report startup markers,
sampled newlib heap high-water and route marker checks. Heap high-water is the
maximum observed allocated bytes, not a claim about unobserved peaks or GPU heaps.

GPU **skip** ratios require native profiler counters and `gpu_state_probe=true`:
index skips / (vertex+index request count / 2), and uniform skips / uniform
requests. The probe adds overhead; turn it off for the primary timing comparison.
Route JSON exports only identical-request probes, shown in separate probe rows;
these are not actual skipped calls. A zero/missing denominator gives `—`.

Exit 0 means parsed with no failed route marker; `MISSING` is not an acceptance
pass. Exit 1 means a started route lacks PASS, a saved route is too short/wrong
map, or a logged failure/crash occurred (including after PASS). Exit 2 means input
or artifact parsing failed. `PASS saved` checks gvillage and both moving durations
>=60 seconds; the game checks alive/Blade/knife_cat before emitting this marker.
`PASS Continue` and `DRIFT` remain separate. Verify effective configuration and
clean process exit in the original logs as part of the existing morning criteria.
