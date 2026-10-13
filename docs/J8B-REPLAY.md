# Tick-1062 New Game drift: host investigation

The milestone-5 Vita3K tape and tick-state capture are absent from this host; the
planning decisions log contains the outcome, not those inputs. The exact reported
position-only drift at tick 1062 is therefore not reproduced or fixed here.

A newer, complete New Game recording is available in
`night/j8-host/record-final/route-inputs.json` (8,916 samples, entry and knife_cat
gates). An ignored source copy under `private/j8b/trace-source/` adds bounded
pre-control JSON tracing through tape tick 1200. It records the ordinary player's
state, reference feet/yaw, controls, status and alignment count. It does not edit
pose, clocks, scenes, inputs or game data. Production replay sources are untouched.
Both diagnostic runs play the full opening film; their explicit tick-1200 stop is
not a route PASS. The windows use 960x544. Trace I/O affects frame cadence, so these
measurements describe host replay behavior, not Vita performance.

With default heading recovery, reference positions match exactly through tick
594. The first different pre-control state is tick 595, hallway-4:

```text
reference feet: [-4968.0, 5424.0, -119.96875]
actual feet:    [-4934.2353515625, 5402.9970703125, -119.96875]
actual velocity: [-233.896240234375, 207.87289428710938, 0]
reference yaw: 3.1415929794311523
actual yaw:    4.71238899230957
position distance: 39.7639854588 units
```

The reference has reached a scripted marker while the replay still moves with
the preceding yaw. `viewer.rs:2096` checks interactions once per render frame;
`viewer.rs:2128` advances village cinematics once before the physics batch.
`movement.rs:803` can then execute several 120 Hz steps. The recorded scene
handoff and input tick can consequently occur on different batch boundaries.
`village/cinema.rs:520` places Alice at a talk marker and resets velocity; this
is ordinary scene behavior, not a probe intervention.

There is an additional replay effect: `route_replay/mod.rs:463` returns neutral
controls during heading recovery without advancing its tape tick, while the
fixed clock still ticks physics. A trace before that recovery can expose a
difference earlier than the existing `>8`-unit drift log, which is after recovery.
The default diagnostic logs its first threshold marker at tick 891, not 1062.

With `route_world_wish=true`, there are no alignment ticks (1,201 trace rows versus
1,207 with default recovery). The first mismatch instead occurs at tick 594:
the replay has already reached `[-4968, 5424, -119.96875]`, velocity zero and
yaw 3.141593, while the recording is still moving at
`[-4932.2861328125, 5401.2646484375, -119.96875]`, yaw 4.712389. The 42.336468-unit
error reflects the same scene handoff occurring one tape tick earlier. Removing
camera waits therefore does not make frame-driven scene/trigger boundaries equal.

At tick 1062, both host replays are climbing with zero velocity and the recorded
yaw. Position error is only 0.019043 units with heading recovery and 0.031250 units
with world-wish recovery, below the existing eight-unit drift threshold. These
measurements cannot explain the original milestone-5 threshold marker without its
actual tape/frame grouping. Raw logs, `trace-results.json` and
`replay-analysis.log` are under `private/j8b/`.

Independent real-data headless village runs both complete 17,790 ticks, 6 jumps,
2 throws, zero combat damage and 100 sanity. Their 103 navigation/combat/end-check
trace lines are identical, SHA-256
`32e88c5c1b3a2b3aa61d53f145e1b7229ad39eb4a4d11efe1f9e878f0eca5925`.
This supports repeatability of that host path; it does not prove cross-architecture
identity or frame-independent cinematic timing. All existing movement/frame-rate
unit tests pass in the 770-test feature suite.

No gameplay fix is justified by these observations. Making scene/trigger timing
fully deterministic requires coordinating it with the physics timeline, not
changing a float comparison or overriding saved pose. That is broader than an
exact small repair and may change ordinary progression timing.

For the original Vita3K case, retain its exact tape and initial save/configuration,
then capture frame grouping, fixed-clock tick, route tick, scene beat/time,
scripted-control handoffs and player feet/velocity/grounded/yaw around ticks
1020–1080. Compare the first state difference before the eight-unit marker, using
the same input tick and script phase. The newer host tape cannot substitute for
that reference. Evidence and helper scripts remain in `private/j8b/`.
