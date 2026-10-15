# J24 other probe

Instrumentation only, based on vita-j23-probe 7d416f0 (runtime 3b850c7).
Seven exclusive children split the old other span: world_opaque_prepare,
world_opaque_submit, power_art, world_particles_alpha, effects_first_person,
present_preview and hud_tail. Existing finish/upload/submission children remain
exclusive; other is the residual. No rendering, gameplay or scheduling change.

The light CSV preserves its first 241 J23 columns and appends 35 category fields
(exclusive_ns, calls, copied_bytes, compared_bytes, logical_draws) plus four raw
counters: actor_prep_world_opaque_submit_gl_draw_calls, _gl_uniform_calls,
_gl_bind_calls and actor_prep_present_preview_captures. Total: 280 columns.
Raw GL calls are observed at driver entry points only while opaque submit owns
the thread: glDrawElementsInstanced, sampler/float/matrix uniforms, texture/buffer
binds. Sky is prepare, not loop submission. Counters are neither API requests
nor durations. Preview counts actual grab_screen calls after a valid-size check.
Recording remains fixed-size and allocation-free, with one same-frame snapshot.
The probe-disabled arm bypasses all new clocks and counter state; the game and GL
calls retain their arguments/order. Disabled CSV placeholders remain unavailable.

Mac-only verification and package evidence are under
/Users/anezium/vita-j0/village-retained-world/jalon-24-other/.
PROTOCOL.md retains J23's three alternating normal-play pairs and frames 61-180,
adds the seven categories and reconstructed old other per-frame sum, and reports
raw counts including capture-bearing rows without deleting primary stalls.
No console access or optimization/performance result is claimed.
