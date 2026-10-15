# Vita playable reference assembly

R2 review fixes are documented in `VITA-CONTROLS.md` and the local
`~/vita-j0/atoz/reference-build-r2/PROTOCOL.md`. R2 retains the pkg-2 renderer
and pins except for CPAL: BGM at 44.1 kHz, 512 frames (11.610 ms), priority 128
on CPU1 (0x20000), 1 MiB callback stack, explicit stereo volume and kernel policy
readback. Software deadline estimates log nonzero windows once per second.
The retained mono PCM cache is capped at 24 MiB (6,291,456 samples); music and
voices remain compressed streams. Heap/cache logs cover every growth and level
load. Console audio, heap and latency measurements remain pending.

The sections below record the original r1 build; their hashes/test counts apply
only to that historical package. R2 evidence and hashes live in its separate root.


Base: pkg-2 runtime `86792fe186736125219de139ee1ddec44dc356fb`.
The reference adds only the pinned CPAL Vita output fork, a handheld preference
preset, Circle menu-back handling and on-demand normal-play save thumbnails.
The renderer, miniquad `6f891804`, vitaGL archive/header pins, packaging build
script, 192 MiB newlib and 8 MiB main/game thread stacks stay at pkg-2.

See `VITA-CONTROLS.md` for the physical layout and host keyboard emulation.
Fresh Vita preferences receive the preset; untouched older default bindings
migrate when loaded. Custom layouts survive and may require an explicit reset.

`PL-def` retains `save_preview: "throttled"`. In normal Full mode that now means
capture only on explicit save/autosave/finale checkpoint, preparation for modal
save menus, pause/inventory/chapter boundaries and quit. Captures occur after
world/first-person presentation and before HUD/menu/cursor, before the matching
save is written. Removing the empty-frame bootstrap also avoids a launch-only
capture when no save needs it. Play across frame 120/240 has no periodic request.
The explicit `every_frame` diagnostic and historical diagnostic modes retain
their old behavior; render/full benches and route modes still suppress captures.
Menu saves/new-game/quit keep the existing frozen clean-frame handshake.

The CPAL fork is pinned at `66bd87675f2f289f57db7ec5309b636e44618037`.
The Vita backend opens the BGM port at fixed 48 kHz stereo on its own worker;
startup logs identify the selected output device and default stream configuration.
A host audio probe proves samples reached a host device, not Vita audibility.

Local evidence and the console-owner protocol are under
`~/vita-j0/atoz/reference-build/`. Host checks and parity are functional evidence.
Console audio, physical reachability, save-preview behavior, audio cost, the
five-minute soak, film soundtrack/synchronization and whole-campaign progression
require the owner's protocol before any console acceptance claim.

## Verified host evidence

Final runtime source: `de20f2a`. Full feature tests: 844 passed; default tests:
780 passed; zero failures and two inherited ignored asset cases in each.
The 15-image J20 saved-pose/HUD parity set is byte-identical to pkg-2.
The native save-preview fixture passed disk reopen, overwrite, failed-write
integrity, matching backup, malformed/legacy image fallback and menu presentation.
Two world thumbnails and the save-menu association were visually inspected.

Fresh direct-village host emulation ran 241 active frames with no save_preview
span at all. The explicit every_frame witness produced 211 recorded capture
samples after the existing report's 30-sample exclusion. These are capture-policy
and functional checks, not host or console performance acceptance. The initial
saved-game attempt was paused by design and rejected by the active-frame check;
it is preserved separately. No runtime change was needed for that harness fix.

`Audio device opened: Haut-parleurs MacBook Air / 48000 Hz / 2 channels`. The real host output probe consumed 1,135,546
stereo mix frames, resolved 81 sound requests and passed 14 measured intervals,
including mute/pause silence, resume, positional effects and crossfades. Its
25.75-second app-only mix capture is retained locally. No human listening or
Vita audio-quality claim follows from this host-device test.

## Verified native Mac package

Built at runtime source `de20f2a0afb833093a86e947bb9ee8bfa25b9e10` with native `~/vitasdk`,
nightly-2026-10-11, cargo-vita 0.2.2, four Cargo jobs and `-neon`.
The pinned vitaGL archive/header match the J24 pipeline. Linked metadata margin:
1479192 bytes (minimum 3348). ELF process-parameter values confirm
192 MiB newlib and an 8 MiB main-thread stack; the unchanged pkg-2 entry point
retains the separate 8 MiB game-thread stack. Imports, deterministic regenerated
VELF, ZIP CRC and embedded SELF/SFO identity pass. Only eboot.bin and SFO are packaged.

VPK SHA-256: `70a981a745b3547000b45218f41f4bc2ec950a0ac23fcc7ca7ff412e9b9dff1f`.
SELF SHA-256: `20fc7399b11735fd2f324e49ec7418b04da58f854b9b65e0ce16d4c18597bba5`.

The documentation handoff commit does not change the tested or packaged runtime.
`PL-def`, `vita-sim-all`, the optional preferences preset, package, verification
logs, checksum manifests and console-owner protocol remain in the local evidence
root above. No console install, run, listening test or timing acceptance occurred.
