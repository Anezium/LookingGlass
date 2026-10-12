# Milestone 5 work log

Branch vita-renderer-j5 starts at 061faaa. Milestone 4 stays unchanged.

## Actor preparation

Runtime actor_layer_cache caches immutable layer state by surface and mesh identity.
Only camera/normal-independent layers with full entity opacity use this path;
animated image/gain state refreshes when its clock changes. Camera-dependent UV,
dot-alpha, fading and TAN effects retain exact preparation. Appearance remains a
live packet input. actor_shared_geometry shares identical layer attributes and
uploads once per shared geometry, with individual texture bindings at draw time.
actor_batching isolates adjacent single-pass actor grouping by complete pipeline,
texture and uniform state. Multi-layer actors and world packets remain barriers;
alpha order is preserved. actor_frustum_cull reuses sim9's safe ambient-actor checks
without changing simulation, contacts, progression or saves.

Host reference-save A/B: 2400x1296, zero differing pixels across the entire image.
The last-frame A/B counters show 45 layer-cache hits, zero dynamic state refreshes
and 64/55 direct dynamic draws with actor batching disabled/enabled. This draw
comparison isolates the knob; adjacent actor grouping already existed in milestone
4, so it is not a measured milestone-4 performance gain. Host tests: 741 passed,
zero failed, one pre-existing ignored. Evidence remains outside Git under
~/vita-j0/village-retained-world/jalon-5/host/.

## Startup timeout under investigation

Pilot P3 on milestone 4 logged only vita-sim configuration between 19:04 and 19:19.
Code order: sim configuration -> PK3 mount/index in Assets::open -> renderer JSON
read in viewer::config -> native GL initialization -> viewer UI -> route JSON and
validation -> corpus identity/save read -> scene build -> first scaled framebuffer.
Thus route-inputs.json, slot1, render-scale and the splash thread are downstream
of the missing renderer-config line. Assets::open has no application mutex and
its mounting algorithm is unchanged from milestone 3.1; milestone 4 only memoized
the later fingerprint calculation. The renderer config line is emitted after its
file read, parse and validation, so those steps are also possible timeout sites.
No native rebuild until the pilot's P0 outcome or an obvious code cause.
