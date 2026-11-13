# Play tapes: record a play, replay it as a benchmark

`src/replay` records what a player does into one text file (a tape) and replays it later, so fights
and explosions cost the same work on every build and their frame times can be compared.

## Why a replay is exact

- The world advances once per viewer frame by that frame's time, clamped to 50 ms. Alice's body
  advances in fixed 1/120 s ticks fed from the same unclamped time (`FixedClock`).
- Random numbers come from per-actor generators seeded from spawn data (ants, imps, cards, chess,
  clockwork, snarks, battles, particles). No global or OS random source feeds the simulation.
- Input reaches the simulation only through macroquad's key, mouse, wheel and char calls and, on the
  Vita, one SceCtrl stick read per frame. The in-engine cinematic skip timer read the wall clock; it
  now reads tape time while a tape runs.

So a step is exact when it sees the same frame time and the same input reads. A recording stores
both for every step. A replay installs them instead of the pad and the clock: the simulation follows
the recording whatever the render speed. Measured on the host (see the A2 report): five replays of a
4000-step fight, a save start, a death and retry take and a take ended with Start matched every
check of their recording; 1% more incoming damage and 1% faster ants each failed the check at the
first step they touched.

What is not covered: modal menus (Start ends a recording), the opening film (a tape never starts
with it), quick load from a slot (ends a recording), a recording made on another platform, and any
code change. A replay on a different build is the point of the tool: the check says whether the
simulation is still the same.

## Keys (vita_renderer.json)

| Key | Default | Use |
|---|---|---|
| `replay_record` | `""` | Tape file name to write, `<name>.lgtape`. One `#` becomes the first free 01..99. |
| `replay_start` | `""` | Needed to record: `save:<slot>` (auto, quick, slot1..slot4) or `chapter:<map>[$<entrance>][@<difficulty>]`. |
| `replay_play` | `""` | Tape file name to replay. Exclusive with `replay_record`. |
| `replay_check_every` | 30 | Steps between check values in a recording. |
| `replay_state_every` | 0 | Steps between full-state hashes (the host oracle's state set). About 1 ms each on the Mac. |
| `replay_stop_after` | 0 | End a recording and the game after this many steps (host tests). |

Tapes live next to the configs (`ux0:data/alice/private/` on the Vita, `private/` on the host).
The game never creates a directory and never replaces a tape.

A recording goes straight to its start, without the menu. Play, then press Start: the tape is
closed and the menu works as usual. A replay goes to the same start, plays the tape, prints a
summary and quits. It uses the recorded control preferences and writes no save.

If the tape cannot be opened at startup, the launch fails with the reason in the log. If a write
fails later, the recording ends there (`reason=error`, the steps written so far stay replayable)
and the game goes on with the live pad.

## Log lines

```
[lg/replay] record a2-01.lgtape start=chapter:potears2 ...
[lg/replay] check step=300 map=potears2 feet=x,y,z sanity=95.0 hostiles=14 digest=<16 hex> [state=<16 hex>] cost_us=..
[lg/replay] event step=705 death | alive | map:<name>
[lg/replay] recorded a2-01.lgtape steps=N reason=menu ...
[lg/replay] play a2-01.lgtape steps=N ...
[lg/replay] config renderer.<key> recorded=.. now=..        keys that differ from the recording
[lg/replay] check ... rec=match | rec=DIFF
[lg/replay] DIVERGED step=S recorded: ... replay: ...       once, at the first difference
[lg/replay] mark step=S <name>                               markers named after the recording
[lg/replay] done a2-01.lgtape steps=N/N ... result=PASS | DIVERGED | SHORT
```

Frame-time rows (`[miniquad/vita] 10 frames, ...`) are unchanged.

## File

UTF-8 text, one record per line; see the comment at the top of `src/replay/tape.rs`. Marker lines
`M <step> <name>` may be appended after the recording (also after `END`); nothing else may change.

## Host test

`LOOKING_GLASS_REPLAY_SYNTH=1` makes a host recording play itself: it walks toward the nearest
hostile, turns, runs, jumps, attacks, presses Enter now and then (Retry), and presses Escape at the
step in `LOOKING_GLASS_REPLAY_SYNTH_MENU`. Use `replay_stop_after` to end it.
`LOOKING_GLASS_REPLAY_FAIL_AT=<step>` makes a host recording fail its write at that step.
