# Vita camera human protocol

This package changes only camera input/menu wiring relative to r3f 0bd6241.
No console was contacted for implementation or host validation. Feel and actual
console rates remain pending a human test. Use the existing r3f renderer/audio
configs, game data, save and launch procedure. Preserve the installed SELF,
preferences and save first; install the supplied SELF using the established
manual SELF-only overwrite procedure from the r3f campaign. Keep the VPK as
an archive artifact. Restore the previous SELF and preferences to roll back.

1. Open Options > Game. Set **Stick Sensitivity** to 1.0 (28.57% of slider;
   or set `pad_sensitivity: 1.0` in preferences with the game stopped).
   Set Invert Look off and controller Stick Deadzone .24. The controller
   sensitivity/inversion page and Game page refer to the same settings.
   Separate Touch Sensitivity / Invert Touch rows expose sensitivity /
   invert_mouse on Vita. Host controls emulation keeps the original mouse
   Game rows. Existing saved
   pad_sensitivity/invert_pad values are respected; no settings are migrated.
2. On a safe flat area of gvillage, release both sticks after loading, keep
   the left stick neutral and hold right stick fully right for 5 seconds.
   Expect one 360-degree revolution in about 1.714 seconds, 180 degrees in
   .857 seconds. Repeat left; release should stop immediately. Pan while
   walking/running, then repeat in a slower area (25–38 ms frames).
3. Collect the game's existing log by the normal human capture workflow.
   Horizontal movement windows are logged every >=1 second, even below .99:
   `[lg/camera-stick] horizontal_window frames=30 seconds=1.0000 yaw_deg_s=-210.000 peak_abs_x=1.0000 full_deflection=true`
   `peak_abs_x` is the largest dead-zone-rescaled |x| in that window.
   Negative yaw means right; left is positive. With a steady stick at 1.0,
   expect 210 degrees/s (+/-2%) at both frame rates. `full_deflection=true`
   means every sampled |x| was >=.99. A worn stick below .99 still logs its
   peak and measured yaw; `full_deflection=false` is not a camera failure.
   For a steady horizontal rescaled value x, expected speed is
   `210 * abs(x)^1.7` at default sensitivity. A changing window's peak alone
   does not predict its average speed. Report the actual lines while holding
   one direction steadily (sign reversals cancel yaw). Diagonal samples with
   rescaled abs(y)>.05 are excluded. This measures wrapped applied yaw change
   divided by integrated render seconds; console behavior still needs this test.
4. Repeat at minimum .2 and maximum 3.0: expect yaw 60 and 420 degrees/s.
   Reset to 1.0 afterward. Full-axis pitch is 135 degrees/s by default
   (minimum 40, maximum 270), stopping at +/-85.944 degrees. Test pitch near
   the horizon before its clamp; diagonal maximum is shared radially.
5. Move just outside the centre slowly to check fine framing, then to half
   throw and diagonally; expect no centre drift, no dead-zone jump and
   slower precise aim. Try deadzone .1/.4 if needed. Toggle Invert Look:
   vertical direction must reverse, horizontal direction must stay the same.
6. Hold look while opening/closing pause/inventory and after loading. Camera
   must stay still until the right stick is released to centre. Confirm
   Cross/menu navigation and left-stick walking/running still work. Compare
   feel directly with the backed-up r3f build and report overshoot, precision,
   reversal/release latency, default/max usability and measured rate lines.

No temporal ramp is enabled. Magnitude uses exponent 1.7 after radial dead-zone
rescaling; elapsed time caps at .1 seconds only for long stalls. Mouse/desktop
look and simulation/replay inputs are unchanged.
