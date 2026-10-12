# Milestone 5.1 scaled-target orientation fix

Console milestone 5 showed the whole scaled 3D image inverted, with native HUD
orientation correct (pilot report feat/memory-192 029c856; Heap-low-heapwatch.png).
The pinned Mac-built VitaGL library uses HAVE_UNFLIPPED_FBOS. The final scaled
copy therefore needs the opposite sampled V from the working desktop convention.

Only the Vita upscale fragment shader samples vec2(uv.x, 1.-uv.y). Existing copy
quad geometry and the desktop shader stay unchanged. render_scale=1 never creates
or presents a scaled target, so the native-resolution path is unaffected. The
copy remains one opaque draw before HUD. Target creation logs copy_v_inverted=true
on Vita; host false. No SDK/pin updates, simulation changes or film changes.

One native build; verify imports, packed SELF, SHA-256 and ELF RX/RW margin before
writing PACKAGE-READY under ~/vita-j0/village-retained-world/jalon-5.1/. Hardware
scaled orientation must be judged by the pilot; host images are not evidence.

Pilot priority: playable-low + vita-sim-all at the reference pose, visual capture
with asymmetric upright landmarks/Alice/HUD, then playable-low-native control.
Run Arrival/Continue if the scaled view is upright; New Game/Continue remains an
optional functional check already validated on milestone 5. Accept post_knife_cat_s
>=60.000, not exact string equality. Position drift beyond 8 units remains a
warning; PASS tests progression/save/input duration, not exact trajectory equality.
