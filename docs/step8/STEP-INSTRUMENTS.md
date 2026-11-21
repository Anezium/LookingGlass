# Step instrumentation ownership (I5)

Serial execution keeps the original thread and scopes. W0 runs the sites below on its blocked worker.
Worker spike, census and frame-profile scopes begin and end on the worker when their diagnostics are active in the timing window. The reply carries completed, stamped rows. W0 also returns the host probe accumulators, including NPC decision digests and mover rows.
Functional probe switches are process-wide. The worker installs the stamped frame number without touching GL.
Frame boundaries, render instrumentation, and final probe serialization stay on the owner.

| Direct site | Call | W0 handling |
|---|---|---|
| `step.rs at 7caf763:102` | `crate::frame_profile::span(name));` | Worker-local harvest |
| `step.rs at 7caf763:107` | `crate::phase_census::span(region));` | Worker-local harvest |
| `step.rs at 7caf763:339` | `scopes.frame("frame_interaction_search");` | Worker-local harvest |
| `step.rs at 7caf763:341` | `crate::spike_log::sim(crate::spike_log::Sim::Search);` | Worker-local harvest |
| `step.rs at 7caf763:344` | `scopes.frame("frame_interaction_search");` | Worker-local harvest |
| `step.rs at 7caf763:345` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::InteractionSearch);` | Worker-local harvest |
| `step.rs at 7caf763:380` | `scopes.census(crate::phase_census::Region::Objects);` | Worker-local harvest |
| `step.rs at 7caf763:383` | `scopes.frame("frame_interactions");` | Worker-local harvest |
| `step.rs at 7caf763:385` | `crate::spike_log::sim(crate::spike_log::Sim::Interactions);` | Worker-local harvest |
| `step.rs at 7caf763:388` | `scopes.frame("frame_interactions");` | Worker-local harvest |
| `step.rs at 7caf763:389` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::Interactions);` | Worker-local harvest |
| `step.rs at 7caf763:413` | `scopes.census(crate::phase_census::Region::Movers);` | Worker-local harvest |
| `step.rs at 7caf763:416` | `scopes.frame("frame_movers");` | Worker-local harvest |
| `step.rs at 7caf763:418` | `crate::spike_log::sim(crate::spike_log::Sim::Movers);` | Worker-local harvest |
| `step.rs at 7caf763:421` | `scopes.frame("frame_movers");` | Worker-local harvest |
| `step.rs at 7caf763:422` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::None);` | Worker-local harvest |
| `step.rs at 7caf763:423` | `crate::phase_census::set_owner(crate::phase_census::Owner::Other);` | Worker-local harvest |
| `step.rs at 7caf763:437` | `crate::sim_maps::timer("movers_companions_ns");` | Worker-local harvest |
| `step.rs at 7caf763:443` | `crate::sim_maps::timer("movers_targets_ns");` | Worker-local harvest |
| `step.rs at 7caf763:448` | `crate::sim_maps::count("movers_target_rows", mover_targets.len() as u64);` | Worker-local harvest |
| `step.rs at 7caf763:452` | `crate::sim_maps::probe::mover_replay_enabled() {` | Execute on worker using sampled frame and probe options |
| `step.rs at 7caf763:454` | `crate::sim_maps::probe::mover_ride(s.moving_tops(), &mut sim.player);` | Execute on worker using sampled frame and probe options |
| `step.rs at 7caf763:458` | `crate::sim_maps::timer("r26_movers_ns");` | Worker-local harvest |
| `step.rs at 7caf763:467` | `crate::sim_maps::probe::mover_replay_enabled() {` | Execute on worker using sampled frame and probe options |
| `step.rs at 7caf763:468` | `crate::sim_maps::probe::record_movers(` | Return ordered row for owner serialization |
| `step.rs at 7caf763:474` | `scopes.census(crate::phase_census::Region::PlayerMovement);` | Worker-local harvest |
| `step.rs at 7caf763:477` | `scopes.frame("frame_movement");` | Worker-local harvest |
| `step.rs at 7caf763:479` | `crate::spike_log::sim(crate::spike_log::Sim::Movement);` | Worker-local harvest |
| `step.rs at 7caf763:482` | `scopes.frame("frame_movement");` | Worker-local harvest |
| `step.rs at 7caf763:483` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::Movement);` | Worker-local harvest |
| `step.rs at 7caf763:484` | `crate::phase_census::set_owner(crate::phase_census::Owner::Player);` | Worker-local harvest |
| `step.rs at 7caf763:487` | `crate::phase_census::span(crate::phase_census::Region::PlayerPrepare);` | Worker-local harvest |
| `step.rs at 7caf763:488` | `crate::phase_census::count_under(` | Worker-local harvest |
| `step.rs at 7caf763:545` | `crate::phase_census::span(` | Worker-local harvest |
| `step.rs at 7caf763:548` | `crate::phase_census::count_under(` | Worker-local harvest |
| `step.rs at 7caf763:741` | `scopes.census(crate::phase_census::Region::StoryWeapons);` | Worker-local harvest |
| `step.rs at 7caf763:744` | `scopes.frame("frame_story");` | Worker-local harvest |
| `step.rs at 7caf763:746` | `crate::spike_log::sim(crate::spike_log::Sim::Story);` | Worker-local harvest |
| `step.rs at 7caf763:749` | `scopes.frame("frame_story");` | Worker-local harvest |
| `step.rs at 7caf763:750` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::None);` | Worker-local harvest |
| `step.rs at 7caf763:751` | `crate::phase_census::set_owner(crate::phase_census::Owner::Other);` | Worker-local harvest |
| `step.rs at 7caf763:951` | `scopes.census(crate::phase_census::Region::Particles);` | Worker-local harvest |
| `step.rs at 7caf763:955` | `scopes.frame("frame_particles");` | Worker-local harvest |
| `step.rs at 7caf763:957` | `crate::spike_log::sim(crate::spike_log::Sim::Particles);` | Worker-local harvest |
| `step.rs at 7caf763:960` | `scopes.frame("frame_particles");` | Worker-local harvest |
| `step.rs at 7caf763:961` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::None);` | Worker-local harvest |
| `step.rs at 7caf763:962` | `crate::phase_census::set_owner(crate::phase_census::Owner::Other);` | Worker-local harvest |
| `step.rs at 7caf763:969` | `crate::frame_profile::span("steam_animate");` | Worker-local harvest |
| `step.rs at 7caf763:982` | `crate::frame_profile::span("steam_sync");` | Worker-local harvest |
| `step.rs at 7caf763:988` | `crate::frame_profile::span("village_particle_bindings");` | Worker-local harvest |
| `step.rs at 7caf763:999` | `scopes.census(crate::phase_census::Region::StoryWeapons);` | Worker-local harvest |
| `step.rs at 7caf763:1000` | `crate::spike_log::sim(crate::spike_log::Sim::Combat);` | Worker-local harvest |
| `step.rs at 7caf763:1003` | `scopes.frame("frame_combat");` | Worker-local harvest |
| `step.rs at 7caf763:1004` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::Combat);` | Worker-local harvest |
| `step.rs at 7caf763:1086` | `scopes.frame("frame_combat");` | Worker-local harvest |
| `step.rs at 7caf763:1088` | `crate::spike_log::sim(crate::spike_log::Sim::Encounters);` | Worker-local harvest |
| `step.rs at 7caf763:1091` | `scopes.frame("frame_combat");` | Worker-local harvest |
| `step.rs at 7caf763:1092` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::Combat);` | Worker-local harvest |
| `step.rs at 7caf763:1130` | `crate::spike_log::sim(crate::spike_log::Sim::Levels);` | Worker-local harvest |
| `step.rs at 7caf763:1175` | `crate::spike_log::sim(crate::spike_log::Sim::Npcs);` | Worker-local harvest |
| `step.rs at 7caf763:1201` | `crate::spike_log::sim(crate::spike_log::Sim::CombatRest);` | Worker-local harvest |
| `step.rs at 7caf763:1246` | `crate::spike_log::sim(crate::spike_log::Sim::Alice);` | Worker-local harvest |
| `step.rs at 7caf763:1249` | `scopes.frame("frame_alice");` | Worker-local harvest |
| `step.rs at 7caf763:1250` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::Alice);` | Worker-local harvest |
| `step.rs at 7caf763:1447` | `scopes.frame("frame_audio");` | Worker-local harvest |
| `step.rs at 7caf763:1449` | `crate::spike_log::sim(crate::spike_log::Sim::Audio);` | Worker-local harvest |
| `step.rs at 7caf763:1452` | `scopes.frame("frame_audio");` | Worker-local harvest |
| `step.rs at 7caf763:1453` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::None);` | Worker-local harvest |
| `step.rs at 7caf763:1454` | `crate::phase_census::set_owner(crate::phase_census::Owner::Other);` | Worker-local harvest |
| `step.rs at 7caf763:1483` | `scopes.census(crate::phase_census::Region::CameraScene);` | Worker-local harvest |
| `step.rs at 7caf763:1486` | `scopes.frame("frame_camera");` | Worker-local harvest |
| `step.rs at 7caf763:1488` | `crate::spike_log::sim(crate::spike_log::Sim::Camera);` | Worker-local harvest |
| `step.rs at 7caf763:1491` | `scopes.frame("frame_camera");` | Worker-local harvest |
| `step.rs at 7caf763:1492` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::Camera);` | Worker-local harvest |
| `step.rs at 7caf763:1493` | `crate::phase_census::set_owner(crate::phase_census::Owner::Camera);` | Worker-local harvest |
| `step.rs at 7caf763:1495` | `crate::phase_census::span(crate::phase_census::Region::CameraScripted);` | Worker-local harvest |
| `step.rs at 7caf763:1571` | `crate::phase_census::span(crate::phase_census::Region::CameraFollow);` | Worker-local harvest |
| `step.rs at 7caf763:1591` | `crate::phase_census::span(crate::phase_census::Region::CameraHandoff);` | Worker-local harvest |
| `step.rs at 7caf763:1657` | `crate::sim_maps::probe::moving_view(frame, camera, target);` | Execute on worker using sampled frame and probe options |
| `step.rs at 7caf763:1671` | `scopes.frame("frame_audio_mix");` | Worker-local harvest |
| `step.rs at 7caf763:1673` | `crate::spike_log::sim(crate::spike_log::Sim::AudioMix);` | Worker-local harvest |
| `step.rs at 7caf763:1676` | `scopes.frame("frame_audio_mix");` | Worker-local harvest |
| `step.rs at 7caf763:1677` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::None);` | Worker-local harvest |
| `step.rs at 7caf763:1678` | `crate::phase_census::set_owner(crate::phase_census::Owner::Other);` | Worker-local harvest |

## W0 intervals and counters

Private `diag_sim_timing` and the host probe switch `LOOKING_GLASS_SIM_TIMING` enable timing independently of snapshot acceptance. If supplied, the spike frame window selects zero-based viewer IDs for these rows. All clocks are monotonic wall clocks. Each row carries frame ID, tape step, visit, generation, lane and thread. The owner buffers completed rows until the loop closes. Runtimes that exceed the buffer report the dropped count.

| Row | Lane | Boundaries |
|---|---|---|
| `head` | Owner | Loop entry through the hop decision, including capture and prologue |
| `submit`, `join` | Owner | Channel send and blocking receive. Both identify cut A or B. |
| `worker` | Worker | Request callback entry through reply preparation. Its endpoints are worker start and end. |
| `step_a`, `step_b` | Executing lane | The simulation function and its scoped observations. The worker B envelope also includes acoustic capture and optional replay seal. A returned `Due` leaves cut B on the owner. |
| `audio_apply` | Owner | Main post-step audio apply, including capture when a host control requests it |
| `present` | Owner | Framework yield and return for the stamped image, including any pacing wait |
| `present_without_yield` | Owner | Terminal capture/tail boundary when the loop exits without a framework yield |

The owner logs one exclusive W0 outcome per viewer frame. W0 dispatch failures are also emitted as a separate set because one frame can fail several conditions. The stage-10 census records hop eligibility, dispatch and snapshot acceptance independently. The snapshot's original blockers still govern acceptance. Worker seals are counted at consumption and mid-step returns are counted at their first join. The host-only `LOOKING_GLASS_SIM_FORCE_CHECKPOINT` control selects a frame and exercises the normal checkpoint continuation after cut A.

## Callee modules

These routes are reachable from the step inventory above. Conditional diagnostics remain bound to their normal enable switches. Existing render, live bank, audio playback and GPU counters stay on their owning paths.

| Callee or route | Instruments and execution |
|---|---|
| `collision.rs` world, static and dynamic sweeps | `query_sweep`, `query_category`, static traversal/repeat and publication census. `DynamicSweep::sweep`, geometry and clipping feed render-independent hot frame counters plus sim-map counts. Worker census and frame-profile rows carry these separately from owner audio rays. Host SAH records are part of the returned probe. |
| `collision/model_shape.rs` placement, candidate, bevel and full-hull builders | Host sim-map candidate and hull work counts. Vita `collider_lazy_build` and `LazyHullBuilds` run where the initializer runs. In overlap mode each racing initializer attempt counts as work, including one whose result is discarded. `found_published` counts cold callers that found a published result without building, and measures elapsed call time. |
| `skeletal.rs` to `character/skinning/prepared.rs` | Prepared sampling opens the pose census span and consults the execution thread's skin phase. Sim sampling stays on the step lane. Art gather, bank publication and GL submission stay with the owner. |
| `player/`, `interaction.rs` and `interaction/` | Movement query ownership, fixed ticks, trigger and object counters, mover timings, and solid publication brackets. Step hooks run in cut A; save/load acknowledgement and modal paths stay with the owner. |
| `npc.rs`, `npc/profile.rs`, NPC controllers and combat | Per-kind update splits, prepared animation, scoped NPC counters, first-use probes, query categories and branch decisions. The worker returns phase/spike/stat rows and lends back the host decision digest. Rendering NPC art and allocating skin instances remain owner transactions. |
| `combat/navigation.rs` and `flight.rs` | Ground detour and airborne route searches report expansion and clearance work through spike and census rows on the execution lane. |
| `particles.rs`, steam, and `character.rs` | Puffs, constrained sweeps, particle publication, character animation and character-specific frame-profile timers. Simulation-side samples travel with their cut. Render effects and attached draw submission use owner counters. |
| `audio.rs`, `audio/static_cache.rs`, and `viewer/step.rs` audio intent | The step records intents and captures the immutable acoustic view. Commands, acoustic sweeps, the mixer and device apply stay on the owner. Host audio evidence remains an independent parity channel. |
| Replay capture and seal generation | State capture is worker-local on a free W0 frame, and consumption is checked on the owner. Pixel hashes, presentation probes, F12 and input/tape control use the owner's presentation or capture stamp. |
