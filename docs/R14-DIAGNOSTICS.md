# r14b host/native diagnostic amendments

The renderer config at `ux0:data/alice/private/vita_renderer.json` accepts
`log_frame_time_every`: omitted = 10, positive u32 = cadence, 0/null = off.
A bad value rejects only this key, logs `invalid config key` with the rejected
value and effective fallback 10, and preserves the other valid renderer flags.
Unknown fields and other invalid flags retain the strict parser's rejection.
The config line prints the serialized flags and effective cadence (`off` or N).

**Package compatibility:** r13e rejects any config containing this key and falls
back to all defaults. Never stage such a config with an r13e SELF. Stage the key
only with the matching r14/r14b SELF, check for `invalid config` after launch,
and verify every intended flag and the effective cadence on the config line.
This checkout's VPK contains only SELF/SFO; it does not stage runtime configs.

Miniquad remains pinned to 41882db. Report-off also removes its three phase
clock reads per frame, so the report A/B changes those reads as well as printing.
Raw presentation wall capture remains independent of report cadence. The raw
wall CSV is flushed before either census export; a census CSV failure cannot
discard that wall file.

Schema 5 preserves all 51 schema-4 indexes and appends `actor_npc_overlays`
(52 regions). Electric/frozen entry and NPC visibility restoration use this
leaf under `actor_consumers_publication`; fallback skin/layers remain separate.
`actor_npc_overlay_calls` counts those entries under actor_gather, including
early exits; it is not a count of rendered overlay meshes. Attachments retain
their own leaf. The original serial-reason bit 128 is unchanged. Eleven
publication rejection reasons still count terminal failed prepare predicates.

World leaves publish integer ns and calls from the **existing** `Totals<3>`
reads, once inside WorldSubmit. Inactive Totals publish nothing. There are no
added census clock reads per world draw. Leaf calls remain available even
though they no longer contribute to `added_brackets`.

Finish API and per-packet uniform leaves accumulate in fixed local arrays and
publish once under `joint_upload_cpu`, without per-item TLS/RefCell spans.
Buffer updates share one interval per upload (including existing local counters).
Outside frame_total_only, that interval reuses the existing MetricInstant pair.
In the requested frame_total_only build MetricInstant is inert, so API intervals
need clocks; uniform preparation has no existing pair to reuse. Buffer create
and delete intervals also need clocks. **`joint_upload_cpu` exclusive and
`joint_uniform_prep` are upper bounds including observer overhead.** Upload
requests/hits count eligible passes; bytes count actual buffer_update payloads.
Merge/pack includes synchronous worker service/wait. These are CPU wall times,
including preemption and API waits; no GPU duration is inferred.

`added_brackets + reused_brackets == sum(calls[30..52])` per frame. Reused
intervals are world leaves plus buffer-update intervals when their old clock is
active. Both fields are exported in JSON and CSV. A synthetic frame with 507
world draws and one call to every other diagnostic leaf has 19 added brackets
and 1,521 reused intervals. The equivalent historical r14 frame had 1,539 added
brackets before the separate overlay leaf (1,521 from world alone).

Every survey arm prints one clock/span calibration line **before** window_begin
and the initial wall timestamp. It uses fixed arrays, 1,024 samples of 16
operations (16,384 repetitions per operation), median and p95 in ns/operation.
Empty brackets exercise the gated-off span/drop path, not active bookkeeping.
Clock results include the amortized outer read pair, with no subtraction.
Host numbers are not Vita calibration or a runtime performance claim.

The census has fixed storage initialized before the window; memory growth is
bounded by region/counter columns, not draw count. The ON-arm heap gate remains
necessary on the console. All 180 rows require exact integer closure:
`sum(exclusive_ns) + unbracketed_residual_ns == frame_total_ns`.
The offline validator retains schemas 3 and 4 and validates schema 5 names,
parents, counters, and reused/added accounting. Run all three closure fixtures.

## r14c correction (first r15 commit)

The r14b frame_total_only package had no world leaves: light_profile::full()
is always false with that feature. The old test_active fixture did not prove
the shipped path. Totals::new now starts when the census OR full profiler is
active. The prepared world loop shares adjacent boundaries: four reads per
draw with census on and zero with census off in frame_total_only. Publication
marks intervals reused only if the full profiler owns its clocks. Thus 507
draws contribute 1,521 added intervals and 2,028 added reads, zero reused
world intervals. The previous "1,521 reused" and "zero added world reads"
claims above describe the broken r14b fixture, not this corrected build.
The non-GL regression uses production Totals::new and lap, in the exact
native_timing,vita_renderer,frame_total_only feature configuration.
