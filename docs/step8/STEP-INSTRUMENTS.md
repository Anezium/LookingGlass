# Step instrumentation ownership (I4)

Serial execution keeps the original thread and scopes. W0 runs the sites below on its blocked worker.
Worker timing and profiling accumulators belong to that thread. I5 owns publishing those diagnostics.
Functional replay configuration and ordered mover rows must be available on their execution thread.
Frame boundaries, render instrumentation, and final probe serialization stay on the owner.

| Site | Call | W0 or I5 handling |
|---|---|---|
| `step.rs:102` | `crate::frame_profile::span(name));` | Worker-local harvest |
| `step.rs:107` | `crate::phase_census::span(region));` | Worker-local harvest |
| `step.rs:339` | `scopes.frame("frame_interaction_search");` | Worker-local harvest |
| `step.rs:341` | `crate::spike_log::sim(crate::spike_log::Sim::Search);` | Worker-local harvest |
| `step.rs:344` | `scopes.frame("frame_interaction_search");` | Worker-local harvest |
| `step.rs:345` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::InteractionSearch);` | Worker-local harvest |
| `step.rs:380` | `scopes.census(crate::phase_census::Region::Objects);` | Worker-local harvest |
| `step.rs:383` | `scopes.frame("frame_interactions");` | Worker-local harvest |
| `step.rs:385` | `crate::spike_log::sim(crate::spike_log::Sim::Interactions);` | Worker-local harvest |
| `step.rs:388` | `scopes.frame("frame_interactions");` | Worker-local harvest |
| `step.rs:389` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::Interactions);` | Worker-local harvest |
| `step.rs:413` | `scopes.census(crate::phase_census::Region::Movers);` | Worker-local harvest |
| `step.rs:416` | `scopes.frame("frame_movers");` | Worker-local harvest |
| `step.rs:418` | `crate::spike_log::sim(crate::spike_log::Sim::Movers);` | Worker-local harvest |
| `step.rs:421` | `scopes.frame("frame_movers");` | Worker-local harvest |
| `step.rs:422` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::None);` | Worker-local harvest |
| `step.rs:423` | `crate::phase_census::set_owner(crate::phase_census::Owner::Other);` | Worker-local harvest |
| `step.rs:437` | `crate::sim_maps::timer("movers_companions_ns");` | Worker-local harvest |
| `step.rs:443` | `crate::sim_maps::timer("movers_targets_ns");` | Worker-local harvest |
| `step.rs:448` | `crate::sim_maps::count("movers_target_rows", mover_targets.len() as u64);` | Worker-local harvest |
| `step.rs:452` | `crate::sim_maps::probe::mover_replay_enabled() {` | Execute on worker using sampled frame and probe options |
| `step.rs:454` | `crate::sim_maps::probe::mover_ride(s.moving_tops(), &mut sim.player);` | Execute on worker using sampled frame and probe options |
| `step.rs:458` | `crate::sim_maps::timer("r26_movers_ns");` | Worker-local harvest |
| `step.rs:467` | `crate::sim_maps::probe::mover_replay_enabled() {` | Execute on worker using sampled frame and probe options |
| `step.rs:468` | `crate::sim_maps::probe::record_movers(` | Queue ordered row for owner serialization |
| `step.rs:474` | `scopes.census(crate::phase_census::Region::PlayerMovement);` | Worker-local harvest |
| `step.rs:477` | `scopes.frame("frame_movement");` | Worker-local harvest |
| `step.rs:479` | `crate::spike_log::sim(crate::spike_log::Sim::Movement);` | Worker-local harvest |
| `step.rs:482` | `scopes.frame("frame_movement");` | Worker-local harvest |
| `step.rs:483` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::Movement);` | Worker-local harvest |
| `step.rs:484` | `crate::phase_census::set_owner(crate::phase_census::Owner::Player);` | Worker-local harvest |
| `step.rs:487` | `crate::phase_census::span(crate::phase_census::Region::PlayerPrepare);` | Worker-local harvest |
| `step.rs:488` | `crate::phase_census::count_under(` | Worker-local harvest |
| `step.rs:545` | `crate::phase_census::span(` | Worker-local harvest |
| `step.rs:548` | `crate::phase_census::count_under(` | Worker-local harvest |
| `step.rs:741` | `scopes.census(crate::phase_census::Region::StoryWeapons);` | Worker-local harvest |
| `step.rs:744` | `scopes.frame("frame_story");` | Worker-local harvest |
| `step.rs:746` | `crate::spike_log::sim(crate::spike_log::Sim::Story);` | Worker-local harvest |
| `step.rs:749` | `scopes.frame("frame_story");` | Worker-local harvest |
| `step.rs:750` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::None);` | Worker-local harvest |
| `step.rs:751` | `crate::phase_census::set_owner(crate::phase_census::Owner::Other);` | Worker-local harvest |
| `step.rs:951` | `scopes.census(crate::phase_census::Region::Particles);` | Worker-local harvest |
| `step.rs:955` | `scopes.frame("frame_particles");` | Worker-local harvest |
| `step.rs:957` | `crate::spike_log::sim(crate::spike_log::Sim::Particles);` | Worker-local harvest |
| `step.rs:960` | `scopes.frame("frame_particles");` | Worker-local harvest |
| `step.rs:961` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::None);` | Worker-local harvest |
| `step.rs:962` | `crate::phase_census::set_owner(crate::phase_census::Owner::Other);` | Worker-local harvest |
| `step.rs:969` | `crate::frame_profile::span("steam_animate");` | Worker-local harvest |
| `step.rs:982` | `crate::frame_profile::span("steam_sync");` | Worker-local harvest |
| `step.rs:988` | `crate::frame_profile::span("village_particle_bindings");` | Worker-local harvest |
| `step.rs:999` | `scopes.census(crate::phase_census::Region::StoryWeapons);` | Worker-local harvest |
| `step.rs:1000` | `crate::spike_log::sim(crate::spike_log::Sim::Combat);` | Worker-local harvest |
| `step.rs:1003` | `scopes.frame("frame_combat");` | Worker-local harvest |
| `step.rs:1004` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::Combat);` | Worker-local harvest |
| `step.rs:1086` | `scopes.frame("frame_combat");` | Worker-local harvest |
| `step.rs:1088` | `crate::spike_log::sim(crate::spike_log::Sim::Encounters);` | Worker-local harvest |
| `step.rs:1091` | `scopes.frame("frame_combat");` | Worker-local harvest |
| `step.rs:1092` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::Combat);` | Worker-local harvest |
| `step.rs:1130` | `crate::spike_log::sim(crate::spike_log::Sim::Levels);` | Worker-local harvest |
| `step.rs:1175` | `crate::spike_log::sim(crate::spike_log::Sim::Npcs);` | Worker-local harvest |
| `step.rs:1201` | `crate::spike_log::sim(crate::spike_log::Sim::CombatRest);` | Worker-local harvest |
| `step.rs:1246` | `crate::spike_log::sim(crate::spike_log::Sim::Alice);` | Worker-local harvest |
| `step.rs:1249` | `scopes.frame("frame_alice");` | Worker-local harvest |
| `step.rs:1250` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::Alice);` | Worker-local harvest |
| `step.rs:1447` | `scopes.frame("frame_audio");` | Worker-local harvest |
| `step.rs:1449` | `crate::spike_log::sim(crate::spike_log::Sim::Audio);` | Worker-local harvest |
| `step.rs:1452` | `scopes.frame("frame_audio");` | Worker-local harvest |
| `step.rs:1453` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::None);` | Worker-local harvest |
| `step.rs:1454` | `crate::phase_census::set_owner(crate::phase_census::Owner::Other);` | Worker-local harvest |
| `step.rs:1483` | `scopes.census(crate::phase_census::Region::CameraScene);` | Worker-local harvest |
| `step.rs:1486` | `scopes.frame("frame_camera");` | Worker-local harvest |
| `step.rs:1488` | `crate::spike_log::sim(crate::spike_log::Sim::Camera);` | Worker-local harvest |
| `step.rs:1491` | `scopes.frame("frame_camera");` | Worker-local harvest |
| `step.rs:1492` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::Camera);` | Worker-local harvest |
| `step.rs:1493` | `crate::phase_census::set_owner(crate::phase_census::Owner::Camera);` | Worker-local harvest |
| `step.rs:1495` | `crate::phase_census::span(crate::phase_census::Region::CameraScripted);` | Worker-local harvest |
| `step.rs:1571` | `crate::phase_census::span(crate::phase_census::Region::CameraFollow);` | Worker-local harvest |
| `step.rs:1591` | `crate::phase_census::span(crate::phase_census::Region::CameraHandoff);` | Worker-local harvest |
| `step.rs:1657` | `crate::sim_maps::probe::moving_view(frame, camera, target);` | Execute on worker using sampled frame and probe options |
| `step.rs:1671` | `scopes.frame("frame_audio_mix");` | Worker-local harvest |
| `step.rs:1673` | `crate::spike_log::sim(crate::spike_log::Sim::AudioMix);` | Worker-local harvest |
| `step.rs:1676` | `scopes.frame("frame_audio_mix");` | Worker-local harvest |
| `step.rs:1677` | `crate::frame_profile::set_owner(crate::frame_profile::Owner::None);` | Worker-local harvest |
| `step.rs:1678` | `crate::phase_census::set_owner(crate::phase_census::Owner::Other);` | Worker-local harvest |
