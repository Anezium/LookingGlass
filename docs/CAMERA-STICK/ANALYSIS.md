# Vita right-stick analysis (r3f 0bd6241)

## Existing path

`src/main.rs` keeps miniquad 6f89180's `right_stick_raw_speed=8` default.
Its Vita `input.rs` normalizes `(byte-128)/127`, clamps to [-1,1], applies
an independent per-axis 0.25 dead zone without rescaling, and emits
`raw_mouse_motion(rx*8, ry*8)` once per rendered frame. Intercepted system
input is zeroed. Macroquad 0.4.14 accumulates this as mouse position while
captured; Vita DPI is 1. `src/look.rs` differences successive positions and
applies 0.0025 radians/count times `Preferences.sensitivity`.

Thus full-axis turn = 8 * 0.0025 * sensitivity radians/frame.
Both yaw and pitch have these **unclamped instantaneous** speeds:

| FPS | Default sensitivity 1 | Maximum sensitivity 3 |
| --- | --- | --- |
| 30 | 34.377 degrees/s | 103.132 degrees/s |
| 40 | 45.837 degrees/s | 137.510 degrees/s |

These are source-derived steady-state rates, not console measurements. The
first captured frame is discarded. At 25–38 ms/frame default speed varies
from 45.837 to 30.156 degrees/s. There is no angular smoothing or acceleration.
Mouse yaw wraps modulo tau and pitch clamps to +/-1.5 radians (85.944 degrees).
`camera::Follow` smooths vertical pivot/obstacle arm return, not aim angles.

`Preferences::apply_vita_preset` remaps physical Vita buttons to gameplay
keys and leaves sensitivity untouched. Both mouse sensitivity and controller
sensitivity validate 0.2–3.0; menus adjust by 0.1 or continuously by slider.
`invert_mouse` affects this old stick path. `pad_sensitivity`, `invert_pad`,
and radial `pad_deadzone` (default .24, range .1–.4) currently affect only the
XInput path, not Vita right-stick look. Vita's direct SDK read only uses the
left stick for movement. `viewer.rs` separately applies XInput look at
2.5 rad/s yaw and 2 rad/s pitch times pad sensitivity using simulation dt;
that axis is zero on Vita because read_pad is Windows-only.

## Change specification

Read both Vita sticks in the existing SDK sample. Disable backend right-stick
mouse emulation to prevent double input; retain touch/mouse and desktop paths.
Use radial dead zone and rescale remaining magnitude to [0,1], then magnitude
exponent 1.7 for fine aim. At sensitivity 1, full-axis yaw/pitch are 210/135
degrees/s (180-degree yaw in 0.857 s). The existing controller setting 0.2–3
maps monotonically through 60/40, 210/135, 420/270 degrees/s at min/default/max.
These defaults follow the requested console third-person tuning range; no
specific game's undocumented tuning is claimed. No temporal acceleration:
immediate release/reversal and curved low-deflection aim. Multiply velocity
by render elapsed seconds, capped at .1 s only to bound post-stall jumps.
At both 30 and 40 FPS default = 210/135, maximum = 420/270 degrees/s.
Preserve mouse inversion for touch and controller inversion for stick.
Expose stick sensitivity/inversion in the handheld Game menu too, so the
familiar sensitivity slider controls the real Vita camera. Do not migrate or
rewrite saved settings. Modal/focus boundaries require stick neutral again.
Log measured applied yaw delta / elapsed time while fully deflected horizontally.

Replay/simulation code, physics dt, dependency pins, rendering/audio presets,
assets and package metadata must remain unchanged. Verify neutral-input frame
hashes against the archived r3f gate and rerun the existing host suites.
