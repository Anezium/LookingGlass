# Step 8 shared-state inventory

Source review at 3e90ce9. Reachability includes the free simulation step, conditional owner fences and the presentation tail. Rows name the route and owner. A test-only or startup row identifies its separate entry point. The audit counts every source declaration, including foreign symbols, immutable tables and macro templates, so opaque Rust types require review too. Locations describe this revision and can move.

## Shared collision data

`AcousticView` owns the end-of-step solid publications read by audio. Static geometry and dynamic buckets use shared Arcs. The settled, dynamic and weapon lists are cloned in publication order, keeping the same shape placements. Capture follows the final Pool of Tears dynamic publication. The owner completes static audio rays with the captured lists. Live lighting and shadow queries still use `World`; free snapshot render consumes captured light selection, shadow geometry and liquid tint.

`Collider::lazy` shares `Arc<Placed>` with simulation. Audio completion calls `hulls` on admitted settled supports. Dynamic queries call `trace_query` and may initialize complete hulls, or initialize `Placed::candidates` and individual triangle bevel hulls when `sim_triangle_candidates` is on. Weapon obstacles are eager boxes. Movement and NPC sweeps can reach the same placements. Camera liquid capture uses separate liquid colliders. Consider first-touch costs at visit entry and for newly published placements in step 8.

School2 clones share Template.eager and .last_lazy (src/collision/model_shape.rs:185–186). These are sim-owned placement caches behind the two approved mutexes. Preserve cache identity and pose keys across clones.

## Source declarations

The step 7c owner-fence analysis supplies the routes for graphics caches, probe rows, clock guards, recovery, transition loads and skin gather. In particular, keep CROQUET distinct from graphics publication, harvest sim probe rows before resetting the worker frame, and forbid owner ID allocation during an in-flight step. Macro headers below group the slots immediately following them.

| Source | Declaration | Class | Reachability and step 8 action |
|---|---|---|---|
| `src/allocation_census.rs:7` | `ENABLED` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 32. |
| `src/allocation_census.rs:15` | `STATE` | state | Allocation scopes push/pop a per-thread stack; close scopes before returning stamped deltas. See line 56. |
| `src/allocation_census.rs:15` | `thread_local!` | counter | TLS block. Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. |
| `src/allocation_census.rs:16` | `NET` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 24. |
| `src/allocation_census.rs:17` | `CALLS` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 24. |
| `src/allocation_census.rs:18` | `BASELINE` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 44. |
| `src/allocation_census.rs:19` | `OPEN` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 62. |
| `src/allocation_census.rs:20` | `THREADS` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 61. |
| `src/allocation_census.rs:21` | `ANOMALIES` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 22. |
| `src/allocation_census.rs:165` | `OWNER` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 186. |
| `src/allocation_census.rs:166` | `BASE` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 172. |
| `src/allocation_census.rs:167` | `GROWTH` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 198. |
| `src/allocation_census.rs:168` | `CALLS` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 195. |
| `src/allocation_census.rs:169` | `LOWER` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 172. |
| `src/allocation_census.rs:170` | `UPPER` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 172. |
| `src/allocation_census.rs:171` | `FALLBACKS` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 174. |
| `src/audio/block.rs:31` | `TEST` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 33. |
| `src/audio/block.rs:99` | `COST` | diagnostic | Audio preparation feeds apply/mix/decode. Retain control writes on the audio owner and collect proof in producer order. See line 101. |
| `src/audio/block.rs:99` | `thread_local!` | diagnostic | TLS block. Audio preparation feeds apply/mix/decode. Retain control writes on the audio owner and collect proof in producer order. |
| `src/audio/music.rs:34` | `RAW_UNDERRUNS` | counter | Decoder and playback count streams and underruns. Keep atomic counters with their existing producers. See line 127. |
| `src/audio/music.rs:35` | `STATS` | counter | Decoder and playback count streams and underruns. Keep atomic counters with their existing producers. See line 118. |
| `src/audio/music.rs:170` | `JOBS` | state | Music decode enqueues on the existing worker. Keep sender and decode order with audio. See line 177. |
| `src/audio/music.rs:313` | `TEST_COUNTER_LOCK` | state | No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/audio/proof.rs:16` | `thread_local!` | diagnostic | TLS block. Audio preparation feeds apply/mix/decode. Retain control writes on the audio owner and collect proof in producer order. |
| `src/audio/proof.rs:17` | `MIX` | diagnostic | Audio preparation feeds apply/mix/decode. Retain control writes on the audio owner and collect proof in producer order. See line 30. |
| `src/audio/proof.rs:21` | `ON` | cache | Audio preparation feeds apply/mix/decode. Retain control writes on the audio owner and collect proof in producer order. See line 22. |
| `src/audio/queue.rs:54` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/audio/queue.rs:56` | `PUSH_OBSERVER` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 61. |
| `src/audio/sink.rs:12` | `API_MAX_US` | diagnostic | Audio preparation feeds apply/mix/decode. Retain control writes on the audio owner and collect proof in producer order. See line 21. |
| `src/audio.rs:77` | `GAME_STAGE` | diagnostic | Audio preparation feeds apply/mix/decode. Retain control writes on the audio owner and collect proof in producer order. See line 83. |
| `src/audio.rs:78` | `GAME_STAGE_AT` | diagnostic | Audio preparation feeds apply/mix/decode. Retain control writes on the audio owner and collect proof in producer order. See line 82. |
| `src/audio.rs:79` | `GAME_ORIGIN` | cache | Audio preparation feeds apply/mix/decode. Retain control writes on the audio owner and collect proof in producer order. See line 81. |
| `src/audio.rs:1099` | `SAH_MIXER` | diagnostic | Audio preparation feeds apply/mix/decode. Retain control writes on the audio owner and collect proof in producer order. See line 1177. |
| `src/audio.rs:1099` | `thread_local!` | diagnostic | TLS block. Audio preparation feeds apply/mix/decode. Retain control writes on the audio owner and collect proof in producer order. |
| `src/campaign.rs:216` | `KEYS` | cache | Campaign progression, story dispatch or save/restore reads authored visit data. Share immutable tables. See line 217. |
| `src/campaign_chain/graph.rs:118` | `ROWS` | state | Campaign progression, story dispatch or save/restore reads authored visit data. Share immutable tables. See line 754. |
| `src/campaign_route.rs:38` | `LEGS` | state | Campaign progression, story dispatch or save/restore reads authored visit data. Share immutable tables. See line 172. |
| `src/character/skinning/census.rs:6` | `FRAMES` | cache | Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. See line 7. |
| `src/character/skinning/census.rs:15` | `thread_local!` | cache | TLS block. Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. |
| `src/character/skinning/census.rs:16` | `CURRENT` | cache | Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. See line 20. |
| `src/character/skinning/census.rs:17` | `NEXT` | cache | Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. See line 30. |
| `src/character/skinning/census.rs:18` | `GROUPS` | cache | Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. See line 21. |
| `src/character/skinning/cull_tests.rs:26` | `KEEP` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 58. |
| `src/character/skinning/cull_tests.rs:26` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/character/skinning/identity.rs:5` | `thread_local!` | counter | TLS block. NPC or skin-instance construction allocates an ID. Use atomic IDs at integration, assert no concurrent owner allocation, compare exact serial IDs. |
| `src/character/skinning/identity.rs:6` | `NEXT` | counter | NPC or skin-instance construction allocates an ID. Use atomic IDs at integration, assert no concurrent owner allocation, compare exact serial IDs. See line 15. |
| `src/character/skinning/identity.rs:7` | `NEXT_INSTANCE` | counter | NPC or skin-instance construction allocates an ID. Use atomic IDs at integration, assert no concurrent owner allocation, compare exact serial IDs. See line 11. |
| `src/character/skinning/identity.rs:8` | `CURRENT` | state | Draw installs ActorId scope. Retain TLS on drawing thread. See line 20. |
| `src/character/skinning/overlap.rs:30` | `thread_local!` | cache | TLS block. Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. |
| `src/character/skinning/overlap.rs:31` | `PHASE` | cache | Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. See line 35. |
| `src/character/skinning/overlap.rs:32` | `STORE` | cache | Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. See line 62. |
| `src/character/skinning/overlap.rs:77` | `STREAM_TEST` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 1360. |
| `src/character/skinning/overlap.rs:77` | `thread_local!` | counter | TLS block. Skin dispatch counts serial fallback. Preserve on gather owner. |
| `src/character/skinning/overlap.rs:78` | `SERIAL_FALLBACKS` | counter | Skin dispatch counts serial fallback. Preserve on gather owner. See line 80. |
| `src/character/skinning/overlap.rs:305` | `PANIC_INPUT` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 310. |
| `src/character/skinning/overlap.rs:1799` | `SURFACES_KEEP` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 1813. |
| `src/character/skinning/overlap.rs:1799` | `thread_local!` | cache | TLS block. Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. |
| `src/character/skinning/prepared.rs:112` | `thread_local!` | cache | TLS block. Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. |
| `src/character/skinning/prepared.rs:113` | `POSES` | cache | Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. See line 118. |
| `src/character/skinning/prepared.rs:114` | `PALETTES` | cache | Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. See line 119. |
| `src/character/skinning/prepared.rs:115` | `SAMPLES` | cache | Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. See line 120. |
| `src/character/skinning.rs:23` | `thread_local!` | cache | TLS block. Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. |
| `src/character/skinning.rs:24` | `PALETTE` | cache | Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. See line 34. |
| `src/character/skinning.rs:52` | `MUTABLE_SKIN` | cache | Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. See line 53. |
| `src/character/skinning.rs:52` | `thread_local!` | cache | TLS block. Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. |
| `src/character/skinning.rs:61` | `PRESERVE_SKIN` | cache | Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. See line 70. |
| `src/character/skinning.rs:61` | `thread_local!` | cache | TLS block. Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. |
| `src/character/skinning.rs:92` | `CULL_OWES_BUDGET` | cache | Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. See line 102. |
| `src/character/skinning.rs:92` | `thread_local!` | cache | TLS block. Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. |
| `src/character/skinning.rs:187` | `WORKERS` | cache | Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. See line 199. |
| `src/character/skinning.rs:187` | `thread_local!` | cache | TLS block. Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. |
| `src/character/skinning.rs:458` | `BENCH` | cache | Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. See line 460. |
| `src/character/skinning.rs:458` | `thread_local!` | cache | TLS block. Actor draw enters skin gather, prepare and collect. Keep scratch, workers and draw scopes with the gather owner. |
| `src/character.rs:2100` | `thread_local!` | cache | TLS block. Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. |
| `src/character.rs:2104` | `SKIN_MATERIAL` | cache | Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. See line 2107. |
| `src/cinematic.rs:179` | `NEXT` | counter | Film entry constructs FilmTransaction. Serialize transaction IDs in the fenced cinematic path. See line 180. |
| `src/collision/sah.rs:4` | `BUILDS` | counter | World construction builds static SAH. Retain build count with world loader. See line 5. |
| `src/collision/sah_probe.rs:4` | `thread_local!` | diagnostic | TLS block. Player/NPC/camera sweeps record the SAH corpus. Carry query owner, stamp and ordered rows back from sim. |
| `src/collision/sah_probe.rs:5` | `ACTIVE` | diagnostic | Player/NPC/camera sweeps record the SAH corpus. Carry query owner, stamp and ordered rows back from sim. See line 11. |
| `src/collision/sah_probe.rs:6` | `OWNER` | diagnostic | Player/NPC/camera sweeps record the SAH corpus. Carry query owner, stamp and ordered rows back from sim. See line 10. |
| `src/collision/sah_probe.rs:7` | `QUERIES` | diagnostic | Player/NPC/camera sweeps record the SAH corpus. Carry query owner, stamp and ordered rows back from sim. See line 11. |
| `src/collision.rs:685` | `ON` | cache | World construction during transition builds or reuses geometry. Keep build/cache accounting with the fenced loader. See line 686. |
| `src/collision.rs:691` | `ON` | cache | World construction during transition builds or reuses geometry. Keep build/cache accounting with the fenced loader. See line 692. |
| `src/collision.rs:695` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/collision.rs:696` | `SAH_OVERRIDE` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 700. |
| `src/collision.rs:711` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/collision.rs:712` | `SHARE_OVERRIDE` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 716. |
| `src/collision.rs:726` | `thread_local!` | cache | TLS block. World construction during transition builds or reuses geometry. Keep build/cache accounting with the fenced loader. |
| `src/collision.rs:729` | `LIVE_GEOMETRY` | cache | World construction during transition builds or reuses geometry. Keep build/cache accounting with the fenced loader. See line 758. |
| `src/collision.rs:739` | `R13_GEOMETRIES` | counter | Static geometry build/drop updates live allocation counters. Preserve atomic lifetime accounting. See line 794. |
| `src/collision.rs:740` | `R13_GEOMETRY_BYTES` | counter | Static geometry build/drop updates live allocation counters. Preserve atomic lifetime accounting. See line 793. |
| `src/collision.rs:2375` | `LOG` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 2377. |
| `src/collision.rs:2375` | `thread_local!` | cache | TLS block. World construction during transition builds or reuses geometry. Keep build/cache accounting with the fenced loader. |
| `src/dismember.rs:53` | `RECIPES` | cache | Combat severs read validated recipes. Share the initialized immutable recipe table. See line 54. |
| `src/electric.rs:24` | `CACHE` | cache | Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. See line 31. |
| `src/electric.rs:24` | `thread_local!` | cache | TLS block. Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. |
| `src/frame_phases.rs:29` | `TRACE` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 36. |
| `src/frame_phases.rs:29` | `thread_local!` | diagnostic | TLS block. Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. |
| `src/frame_profile.rs:9` | `ROUTE_PUFFS` | counter | Route or timing instrumentation aggregates draws and sim events. Join stamped deltas on owner. See line 23. |
| `src/frame_profile.rs:11` | `ROUTE_FX_DRAWS` | counter | Route or timing instrumentation aggregates draws and sim events. Join stamped deltas on owner. See line 25. |
| `src/frame_profile.rs:13` | `ROUTE_SKIN_TOTAL` | counter | Route or timing instrumentation aggregates draws and sim events. Join stamped deltas on owner. See line 27. |
| `src/frame_profile.rs:15` | `ROUTE_SKIN_CULLED` | counter | Route or timing instrumentation aggregates draws and sim events. Join stamped deltas on owner. See line 27. |
| `src/frame_profile.rs:17` | `ROUTE_GPU_STATES` | counter | Route or timing instrumentation aggregates draws and sim events. Join stamped deltas on owner. See line 20. |
| `src/frame_profile.rs:44` | `CREATIONS` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 46. |
| `src/frame_profile.rs:44` | `thread_local!` | diagnostic | TLS block. Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. |
| `src/frame_profile.rs:55` | `TRANSITION_FRAME` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 57. |
| `src/frame_profile.rs:55` | `thread_local!` | diagnostic | TLS block. Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. |
| `src/frame_profile.rs:66` | `NATIVE_MEASURED` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 68. |
| `src/frame_profile.rs:66` | `thread_local!` | diagnostic | TLS block. Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. |
| `src/frame_profile.rs:81` | `KEEP` | cache | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 82. |
| `src/frame_profile.rs:234` | `SAMPLES` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 325. |
| `src/frame_profile.rs:234` | `thread_local!` | diagnostic | TLS block. Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. |
| `src/frame_profile.rs:236` | `CHARACTER_DEPTH` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 239. |
| `src/frame_profile.rs:236` | `thread_local!` | diagnostic | TLS block. Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. |
| `src/frame_profile.rs:248` | `ON` | cache | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 249. |
| `src/frame_profile.rs:339` | `OBJECT_TOTALS` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 342. |
| `src/frame_profile.rs:339` | `thread_local!` | diagnostic | TLS block. Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. |
| `src/frame_profile.rs:549` | `HOT_RECORDING` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 574. |
| `src/frame_profile.rs:550` | `OWNER` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 554. |
| `src/frame_profile.rs:551` | `HOT_COUNTS` | counter | Route or timing instrumentation aggregates draws and sim events. Join stamped deltas on owner. See line 578. |
| `src/frame_profile.rs:777` | `STATE` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 806. |
| `src/frame_profile.rs:777` | `thread_local!` | diagnostic | TLS block. Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. |
| `src/frame_total_profile.rs:145` | `RING` | diagnostic | Frame end or render-pass timing. Keep graphics hooks and frame completion on the owner. See line 149. |
| `src/frozen.rs:11` | `CACHE` | cache | Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. See line 17. |
| `src/frozen.rs:11` | `thread_local!` | cache | TLS block. Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. |
| `src/game_thread.rs:6` | `OWNER` | cache | Renderer handle creation/check uses the registered graphics thread. Enforce the same owner. See line 9. |
| `src/gpu_timing.rs:68` | `MASK` | diagnostic | Frame end or render-pass timing. Keep graphics hooks and frame completion on the owner. See line 69. |
| `src/gpu_timing.rs:212` | `ARMED` | diagnostic | Frame end or render-pass timing. Keep graphics hooks and frame completion on the owner. See line 219. |
| `src/gpu_timing.rs:213` | `LOG` | diagnostic | Frame end or render-pass timing. Keep graphics hooks and frame completion on the owner. See line 215. |
| `src/gpu_timing.rs:223` | `SPIKE` | diagnostic | Frame end or render-pass timing. Keep graphics hooks and frame completion on the owner. See line 227. |
| `src/gpu_timing.rs:224` | `SPIKE_BEGIN_US` | diagnostic | Frame end or render-pass timing. Keep graphics hooks and frame completion on the owner. See line 232. |
| `src/gpu_timing.rs:225` | `SPIKE_END_US` | diagnostic | Frame end or render-pass timing. Keep graphics hooks and frame completion on the owner. See line 232. |
| `src/gpu_timing.rs:226` | `SPIKE_END_CALLS` | diagnostic | Frame end or render-pass timing. Keep graphics hooks and frame completion on the owner. See line 232. |
| `src/gpu_timing.rs:241` | `query_fence` | diagnostic | Frame end or render-pass timing. Keep graphics hooks and frame completion on the owner. See line 265. |
| `src/gpu_timing.rs:242` | `gxm_render_target` | diagnostic | Frame end or render-pass timing. Keep graphics hooks and frame completion on the owner. See line 283. |
| `src/gpu_timing.rs:268` | `VERTEX` | diagnostic | Frame end or render-pass timing. Keep graphics hooks and frame completion on the owner. See line 302. |
| `src/hud.rs:544` | `ENABLED` | cache | Owner HUD tracing reads the cached host switch. Share its immutable value. See line 545. |
| `src/input.rs:134` | `KEYS` | cache | No free-step poll. Owner capture resolves scripted binding fixtures into StepInput. See line 135. |
| `src/interaction/hooks.rs:681` | `REG` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 696. |
| `src/interaction.rs:17` | `FACT_BUILDS` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 582. |
| `src/interaction.rs:17` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/kernel_alloc.rs:9` | `ACTIVE` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 11. |
| `src/kernel_alloc.rs:9` | `CALLS` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 13. |
| `src/kernel_alloc.rs:9` | `thread_local!` | counter | TLS block. Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. |
| `src/kernel_alloc.rs:20` | `HEAP` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 21. |
| `src/kernel_alloc.rs:20` | `thread_local!` | counter | TLS block. Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. |
| `src/kernel_alloc.rs:32` | `LIVE` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 38. |
| `src/kernel_alloc.rs:33` | `CHUNKS` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 39. |
| `src/kernel_alloc.rs:34` | `HIGH` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 40. |
| `src/kernel_alloc.rs:35` | `CHUNK_HIGH` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 40. |
| `src/kernel_alloc.rs:50` | `ALLOCATOR` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. |
| `src/kernel_alloc.rs:81` | `THREAD` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 90. |
| `src/kernel_alloc.rs:82` | `CALLS` | counter | Step and load allocations enter allocator/scope hooks. Keep thread scopes local, join deltas by stamp. Exclude kernel_alloc under overlap P0. See line 92. |
| `src/level.rs:597` | `POOL` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 622. |
| `src/level.rs:598` | `TEMPLE` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 624. |
| `src/level.rs:599` | `QUIET` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 623. |
| `src/levels/centipede1.rs:581` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/centipede2/data.rs:88` | `N` | cache | Fenced level load interns sound paths. Share the locked name pool and immutable resulting keys. See line 90. |
| `src/levels/centipede2.rs:427` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/facade.rs:497` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/funhouse/motion.rs:533` | `LOGS` | counter | Mover obstruction tracing increments the print limit. Count in sim order and emit the stamped diagnostic on owner. See line 535. |
| `src/levels/funhouse.rs:611` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/garden1.rs:576` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/garden2.rs:769` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/garden3.rs:592` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/garden4.rs:729` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/grounds1.rs:4` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/grounds2.rs:416` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/hatter1.rs:659` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/hatter2.rs:38` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/hedge1.rs:282` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/hedge2.rs:569` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/hedge3.rs:287` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/jlair1.rs:469` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/jlair2.rs:4` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/keep.rs:709` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/mod.rs:94` | `LEVELS` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. See line 264. |
| `src/levels/mod.rs:417` | `FIRST` | state | No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/levels/mod.rs:465` | `WOODS` | state | No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/levels/mod.rs:471` | `WOODS_RETURN` | state | No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/levels/mod.rs:477` | `WOODS_CASES` | state | No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/levels/potears2.rs:710` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/qlair/data.rs:219` | `NAMES` | cache | Fenced level load interns sound paths. Share the locked name pool and immutable resulting keys. See line 221. |
| `src/levels/qlair.rs:729` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/rchess1.rs:55` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/reservations.rs:136` | `RESERVATIONS` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. See line 384. |
| `src/levels/reservations.rs:712` | `GOOD` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 807. |
| `src/levels/reservations.rs:747` | `ANOTHER` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 807. |
| `src/levels/reservations.rs:751` | `UNKNOWN` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 812. |
| `src/levels/reservations.rs:752` | `LEGACY` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 813. |
| `src/levels/reservations.rs:753` | `WRONG_VISIT` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 814. |
| `src/levels/reservations.rs:757` | `WRONG_BASE` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 815. |
| `src/levels/reservations.rs:762` | `WRONG_FLAG` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 816. |
| `src/levels/reservations.rs:771` | `WRONG_CASE` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 817. |
| `src/levels/reservations.rs:781` | `WRONG_CASE_VISIT` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 818. |
| `src/levels/reservations.rs:791` | `WRONG_BEAT` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 819. |
| `src/levels/synthetic.rs:212` | `REGISTRATION` | state | No. Synthetic registration belongs to checker fixtures. See line 301. |
| `src/levels/tower1.rs:252` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/tower2.rs:434` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/tower3.rs:323` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/utemple.rs:615` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/wchess1/graph.rs:18` | `BISHOP` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/wchess1/graph.rs:660` | `KNIGHT` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/wchess1.rs:819` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/wchess2.rs:532` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/levels/wforest.rs:689` | `REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. See line 738. |
| `src/levels/wforest.rs:734` | `RETURN_REGISTRATION` | state | Visit dispatch resolves immutable authored registration, graph or reservation tables. Share read-only. |
| `src/light_profile.rs:24` | `LIGHT` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 31. |
| `src/light_profile.rs:25` | `FULL` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 30. |
| `src/light_profile.rs:94` | `HOST` | cache | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 95. |
| `src/light_profile.rs:247` | `RING` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 249. |
| `src/light_profile.rs:247` | `thread_local!` | diagnostic | TLS block. Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. |
| `src/lighting.rs:16` | `thread_local!` | state | TLS block. Draw publishes selected lights, shadows and flares. Keep material and draw scopes on owner. |
| `src/lighting.rs:17` | `LIGHTS` | state | Draw publishes selected lights, shadows and flares. Keep material and draw scopes on owner. See line 99. |
| `src/lighting.rs:18` | `SHADOW` | state | Draw publishes selected lights, shadows and flares. Keep material and draw scopes on owner. See line 194. |
| `src/lighting.rs:19` | `FLARE` | state | Draw publishes selected lights, shadows and flares. Keep material and draw scopes on owner. See line 212. |
| `src/lighting.rs:20` | `CROQUET` | state | Croquet projectile update reads authored lights in weapons.rs. Carry immutable visit lights into sim. See line 198. |
| `src/loading_screen.rs:28` | `thread_local!` | state | TLS block. Death retry or chapter transition enters loading and recovery. Fence all control mutations and yields on owner. |
| `src/loading_screen.rs:29` | `ACTIVE` | state | Death retry or chapter transition enters loading and recovery. Fence all control mutations and yields on owner. See line 57. |
| `src/loading_screen.rs:30` | `SUPPRESS` | state | Death retry or chapter transition enters loading and recovery. Fence all control mutations and yields on owner. See line 95. |
| `src/loading_screen.rs:42` | `TEST_ENABLED` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 44. |
| `src/loading_screen.rs:42` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/loading_screen.rs:49` | `TEST_NO_SWAP` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 51. |
| `src/loading_screen.rs:49` | `thread_local!` | state | TLS block. Death retry or chapter transition enters loading and recovery. Fence all control mutations and yields on owner. |
| `src/logging.rs:44` | `LOGGER` | state | Engine output uses the stdio logger. Buffer worker and head messages in step order for owner emission. See line 49. |
| `src/loot_art.rs:185` | `thread_local!` | cache | TLS block. Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. |
| `src/loot_art.rs:187` | `MATERIALS` | cache | Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. See line 190. |
| `src/main.rs:207` | `SCE_USER_MAIN_THREAD_STACK_SIZE` | state | No. Vita loader reads process stack/heap constants at startup. Leave at process scope. |
| `src/main.rs:213` | `NEWLIB_HEAP_SIZE_USER` | state | No. Vita loader reads process stack/heap constants at startup. Leave at process scope. |
| `src/native_timing/audio_producer/tests.rs:474` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/native_timing/audio_producer/tests.rs:476` | `COLLECT_GAP` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 489. |
| `src/native_timing/audio_producer.rs:104` | `UNIVERSE` | diagnostic | Timing-instrumented PCM producer publishes measurements. Keep producer writes and owner reads in their timing domains. See line 105. |
| `src/native_timing/clock.rs:64` | `ORIGIN` | cache | Timing hooks read the process clock origin. Share the monotonic origin. See line 65. |
| `src/native_timing/export_memory_probe.rs:21` | `ON` | counter | No production viewer path. Export-memory probe uses its allocator only in the diagnostic harness. See line 32. |
| `src/native_timing/export_memory_probe.rs:22` | `CURRENT` | counter | No production viewer path. Export-memory probe uses its allocator only in the diagnostic harness. See line 36. |
| `src/native_timing/export_memory_probe.rs:23` | `PEAK` | counter | No production viewer path. Export-memory probe uses its allocator only in the diagnostic harness. See line 37. |
| `src/native_timing/export_memory_probe.rs:24` | `ALLOCS` | counter | No production viewer path. Export-memory probe uses its allocator only in the diagnostic harness. See line 35. |
| `src/native_timing/export_memory_probe.rs:59` | `ALLOCATOR` | counter | No production viewer path. Export-memory probe uses its allocator only in the diagnostic harness. |
| `src/native_timing/mod.rs:571` | `SESSION_CLAIM` | state | No free-step acquisition. Timing session starts at owner setup. Retain exclusive recorder claim. See line 572. |
| `src/native_timing/operations.rs:3` | `ENABLED` | counter | Instrumented operations advance the timing epoch. Preserve atomic ordering and collect at owner boundaries. |
| `src/native_timing/operations.rs:4` | `EPOCH` | counter | Instrumented operations advance the timing epoch. Preserve atomic ordering and collect at owner boundaries. |
| `src/native_timing/operations.rs:5` | `WRAPPED` | counter | Instrumented operations advance the timing epoch. Preserve atomic ordering and collect at owner boundaries. |
| `src/native_timing/overlay.rs:12` | `GLYPHS` | state | Timing overlay draw reads constant glyphs. Share read-only. See line 128. |
| `src/npc/attack_fx.rs:15` | `CACHE` | cache | Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. See line 52. |
| `src/npc/attack_fx.rs:15` | `thread_local!` | cache | TLS block. Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. |
| `src/npc/profile.rs:11` | `ACTIVE` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 104. |
| `src/npc/profile.rs:88` | `STATE` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 95. |
| `src/npc/profile.rs:88` | `thread_local!` | diagnostic | TLS block. Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. |
| `src/npc.rs:2443` | `REPORTED` | diagnostic | World-free NPC draw reports skipped actors once. Keep fallback and diagnostic on graphics owner. See line 2444. |
| `src/particles.rs:1641` | `thread_local!` | cache | TLS block. Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. |
| `src/particles.rs:1642` | `BURST_MATERIAL` | cache | Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. See line 1652. |
| `src/particles.rs:1755` | `thread_local!` | cache | TLS block. Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. |
| `src/particles.rs:1756` | `PARTICLE_MATERIALS` | cache | Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. See line 1759. |
| `src/pass_clock.rs:23` | `ON` | diagnostic | Frame end or render-pass timing. Keep graphics hooks and frame completion on the owner. See line 44. |
| `src/pass_clock.rs:24` | `FRAME` | diagnostic | Frame end or render-pass timing. Keep graphics hooks and frame completion on the owner. See line 44. |
| `src/pass_clock.rs:25` | `US` | diagnostic | Frame end or render-pass timing. Keep graphics hooks and frame completion on the owner. See line 61. |
| `src/pass_clock.rs:26` | `DRAWS` | diagnostic | Frame end or render-pass timing. Keep graphics hooks and frame completion on the owner. See line 66. |
| `src/pass_clock.rs:35` | `START` | cache | Frame end or render-pass timing. Keep graphics hooks and frame completion on the owner. See line 36. |
| `src/phase_census.rs:183` | `thread_local!` | diagnostic | TLS block. Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. |
| `src/phase_census.rs:184` | `ACTIVE` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 194. |
| `src/phase_census.rs:185` | `STATE` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 203. |
| `src/phase_census.rs:191` | `EVER_ARMED` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 194. |
| `src/r13_survey.rs:3` | `CENSUS` | diagnostic | Survey census reads the armed switch. Force the survey path serial. See line 4. |
| `src/registry_check.rs:211` | `SKIPS` | diagnostic | No. Offline registry-check probe is absent from runtime registrations. Keep in checker. See line 224. |
| `src/registry_check.rs:238` | `PROBE` | diagnostic | No. Offline registry-check probe is absent from runtime registrations. Keep in checker. See line 253. |
| `src/render/sky_stats.rs:12` | `thread_local!` | state | TLS block. World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render/sky_stats.rs:13` | `QUERY` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 27. |
| `src/render/sky_stats.rs:14` | `FRAME` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 43. |
| `src/render/sky_stats.rs:17` | `ON` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 18. |
| `src/render/sky_stats.rs:47` | `thread_local!` | state | TLS block. World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render/sky_stats.rs:48` | `PHASES` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 70. |
| `src/render/sky_stats.rs:49` | `VERTICES` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 65. |
| `src/render/sky_stats.rs:52` | `SUBMITTED` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 57. |
| `src/render/vita_renderer/depth_sentinel.rs:14` | `PROBE` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 60. |
| `src/render/vita_renderer/depth_sentinel.rs:14` | `thread_local!` | state | TLS block. World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render/vita_renderer/depth_sentinel.rs:69` | `ONCE` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 70. |
| `src/render/vita_renderer/gxm.rs:135` | `CURRENT` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 143. |
| `src/render/vita_renderer/gxm.rs:357` | `gxm_context` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 429. |
| `src/render/vita_renderer/gxm.rs:358` | `dirty_shader_vert_unifs` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 443. |
| `src/render/vita_renderer/gxm.rs:359` | `dirty_shader_frag_unifs` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 443. |
| `src/render/vita_renderer/gxm.rs:360` | `vgl_def_vert_buf` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 435. |
| `src/render/vita_renderer/gxm.rs:361` | `vgl_def_frag_buf` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 436. |
| `src/render/vita_renderer/gxm.rs:378` | `ARMED` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 386. |
| `src/render/vita_renderer/gxm.rs:380` | `ERRORS` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 381. |
| `src/render/vita_renderer/gxm.rs:384` | `CAPTURED` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 385. |
| `src/render/vita_renderer/scale.rs:17` | `TARGET` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 28. |
| `src/render/vita_renderer/scale.rs:17` | `thread_local!` | state | TLS block. World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render/vita_renderer/scale.rs:208` | `ONCE` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 209. |
| `src/render/vita_renderer/scale.rs:232` | `OPAQUE` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 234. |
| `src/render/vita_renderer/scale.rs:233` | `ALPHA` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 234. |
| `src/render/vita_renderer/water_probe.rs:60` | `ON` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 61. |
| `src/render/vita_renderer/water_probe.rs:65` | `FIRST` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 67. |
| `src/render/vita_renderer/water_probe.rs:66` | `LAST` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 68. |
| `src/render/vita_renderer/water_probe.rs:72` | `thread_local!` | state | TLS block. World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render/vita_renderer/water_probe.rs:73` | `FRAME` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 79. |
| `src/render/vita_renderer/water_probe.rs:74` | `START` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 80. |
| `src/render/vita_renderer.rs:949` | `ON` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 950. |
| `src/render/vita_renderer.rs:955` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/render/vita_renderer.rs:957` | `TEST_CONFIG` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 965. |
| `src/render/vita_renderer.rs:975` | `CONFIG` | cache | Both render and sim lever accessors read the process configuration. Share initialized config. See line 976. |
| `src/render/vita_renderer.rs:980` | `CONFIG` | cache | Both render and sim lever accessors read the process configuration. Share initialized config. See line 981. |
| `src/render/vita_renderer.rs:1105` | `CONSTRUCTION_FAILURES` | counter | Level material setup increments construction failures. Keep on graphics owner. See line 1107. |
| `src/render/vita_renderer.rs:1113` | `INVENTORY` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 1142. |
| `src/render/vita_renderer.rs:1126` | `VIEW` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 1128. |
| `src/render.rs:637` | `thread_local!` | state | TLS block. World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render.rs:638` | `ACTIVE_WORLD_MATERIALS` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 647. |
| `src/render.rs:644` | `WORLD_MATERIALS` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 649. |
| `src/render_direct.rs:24` | `thread_local!` | state | TLS block. World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:27` | `FIRST_DEFERRED` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 49. |
| `src/render_direct.rs:32` | `LIB` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 33. |
| `src/render_direct.rs:44` | `REAL` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 45. |
| `src/render_direct.rs:62` | `ONCE` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 63. |
| `src/render_direct.rs:95` | `LIB` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 96. |
| `src/render_direct.rs:112` | `REAL` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 113. |
| `src/render_direct.rs:124` | `glUseProgram::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:125` | `glActiveTexture::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:126` | `glBindTexture::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:127` | `glBindBuffer::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:128` | `glBindFramebuffer::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:129` | `glDepthMask::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:130` | `glDepthFunc::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:131` | `glCullFace::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:132` | `glFrontFace::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:133` | `glEnable::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:134` | `glDisable::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:135` | `glColorMask::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:136` | `glBlendFuncSeparate::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:137` | `glBlendEquationSeparate::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:138` | `glViewport::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:139` | `glScissor::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:140` | `glClear::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:141` | `glUniform1i::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:142` | `glUniform1fv::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:143` | `glUniform2fv::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:144` | `glUniform3fv::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:145` | `glUniform4fv::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:146` | `glUniformMatrix4fv::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:147` | `glBufferData::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:148` | `glBufferSubData::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:149` | `glVertexAttribPointer::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:150` | `glEnableVertexAttribArray::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:151` | `glDisableVertexAttribArray::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:152` | `glVertexAttribDivisor::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:153` | `glDrawElementsInstanced::REAL` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_direct.rs:156` | `ONCE` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 157. |
| `src/render_fx/actor_cache.rs:7` | `PASS_STORAGE` | cache | Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. See line 9. |
| `src/render_fx/actor_cache.rs:7` | `thread_local!` | cache | TLS block. Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. |
| `src/render_fx/actor_publication.rs:532` | `thread_local!` | cache | TLS block. Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. |
| `src/render_fx/actor_publication.rs:533` | `ACTIVE` | cache | Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. See line 538. |
| `src/render_fx/actor_publication.rs:535` | `DEFERRED` | cache | Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. See line 656. |
| `src/render_fx/actor_publication.rs:536` | `STATE` | cache | Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. See line 591. |
| `src/render_fx/actor_publication.rs:540` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/render_fx/actor_publication.rs:541` | `DEFERRED_TEST` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 549. |
| `src/render_fx/actor_publication.rs:542` | `DEFERRED_COMMITS` | cache | Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. See line 616. |
| `src/render_fx/actor_publication.rs:566` | `UV_REUSE_TEST` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 570. |
| `src/render_fx/actor_publication.rs:566` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/render_fx/actor_publication.rs:595` | `OCCURRENCES_TEST` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 599. |
| `src/render_fx/actor_publication.rs:595` | `thread_local!` | cache | TLS block. Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. |
| `src/render_fx/actor_publication.rs:950` | `thread_local!` | cache | TLS block. Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. |
| `src/render_fx/actor_publication.rs:951` | `ELIGIBILITY` | cache | Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. See line 955. |
| `src/render_fx/actor_vertices.rs:12` | `NEXT` | counter | Actor revision construction allocates a producer identity. Serialize allocation against owner construction. See line 13. |
| `src/render_fx/corpse_opaque.rs:7` | `OPAQUE` | cache | Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. See line 12. |
| `src/render_fx/corpse_opaque.rs:7` | `thread_local!` | cache | TLS block. Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. |
| `src/render_fx/corpse_opaque.rs:52` | `ONLY` | cache | Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. See line 55. |
| `src/render_fx/corpse_opaque.rs:52` | `thread_local!` | cache | TLS block. Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. |
| `src/render_fx/corpse_opaque.rs:53` | `NOT` | cache | Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. See line 56. |
| `src/render_fx/corpse_triangles.rs:8` | `ANT` | cache | Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. See line 10. |
| `src/render_fx/corpse_triangles.rs:8` | `thread_local!` | cache | TLS block. Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. |
| `src/render_fx/prepared.rs:7` | `NEXT` | counter | Prepared geometry creation assigns identity. Keep numbering in owner publication order. See line 8. |
| `src/render_fx/prepared.rs:7` | `thread_local!` | cache | TLS block. Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. |
| `src/render_fx/retained/fill_cuts.rs:17` | `thread_local!` | cache | TLS block. Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. |
| `src/render_fx/retained/fill_cuts.rs:19` | `ZERO_ALPHA` | cache | Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. See line 24. |
| `src/render_fx/retained.rs:22` | `HOST_COUNTS` | cache | Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. See line 25. |
| `src/render_fx/retained.rs:22` | `thread_local!` | cache | TLS block. Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. |
| `src/render_fx.rs:108` | `thread_local!` | cache | TLS block. Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. |
| `src/render_fx.rs:109` | `FRAME` | cache | Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. See line 126. |
| `src/render_fx.rs:110` | `SKIN_OPACITY` | cache | Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. See line 986. |
| `src/render_fx.rs:113` | `TEST_SURFACE` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 118. |
| `src/render_fx.rs:1191` | `thread_local!` | cache | TLS block. Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. |
| `src/render_fx.rs:1192` | `ACTOR_MAX_LAYERS` | cache | Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. See line 1197. |
| `src/render_fx.rs:1193` | `SURFACES` | cache | Actor gather, publish or submit enters render-fx storage. Keep graphics cache lifetimes and publication on owner. See line 1202. |
| `src/render_fx_routing_controls.rs:6` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/render_fx_routing_controls.rs:7` | `ENABLED` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 17. |
| `src/render_fx_routing_controls.rs:8` | `IMMEDIATE` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 19. |
| `src/render_snapshot.rs:165` | `NEXT` | counter | Visit creation allocates RenderVisit. Allocate on owner during fenced load. See line 166. |
| `src/render_snapshot.rs:281` | `thread_local!` | state | TLS block. Snapshot rendering installs the handoff pose stamp. Transfer pose and stamp with the frame output. |
| `src/render_snapshot.rs:282` | `HANDOFF_RENDER_STAMP` | state | Snapshot rendering installs the handoff pose stamp. Transfer pose and stamp with the frame output. See line 285. |
| `src/render_tail/vgl_scenes.rs:18` | `MODE` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 19. |
| `src/render_tail/vgl_scenes.rs:48` | `STATE` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 98. |
| `src/render_tail/vgl_scenes.rs:48` | `thread_local!` | state | TLS block. World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail/vgl_scenes.rs:60` | `$slot` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail/vgl_scenes.rs:64` | `BIND` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 157. |
| `src/render_tail/vgl_scenes.rs:64` | `CLEAR` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 89. |
| `src/render_tail/vgl_scenes.rs:64` | `ELEMENTS` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 179. |
| `src/render_tail/vgl_scenes.rs:65` | `ARRAYS` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 180. |
| `src/render_tail/vgl_scenes.rs:65` | `ELEMENTS_I` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 181. |
| `src/render_tail/vgl_scenes.rs:66` | `ARRAYS_I` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 182. |
| `src/render_tail/vgl_scenes.rs:66` | `FINISH` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 195. |
| `src/render_tail/vgl_scenes.rs:66` | `FLUSH` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 188. |
| `src/render_tail/vgl_scenes.rs:67` | `DELETE_PROGRAM` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 173. |
| `src/render_tail/vgl_scenes.rs:67` | `USE_PROGRAM` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 161. |
| `src/render_tail/vgl_scenes.rs:68` | `READ` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 200. |
| `src/render_tail/vgl_scenes.rs:69` | `BLIT` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 204. |
| `src/render_tail/vgl_scenes.rs:71` | `GET_INTEGER` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 82. |
| `src/render_tail/vgl_scenes.rs:72` | `GET_FLOAT` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 82. |
| `src/render_tail/vgl_scenes.rs:73` | `IS_ENABLED` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 82. |
| `src/render_tail/vgl_scenes.rs:74` | `ENABLE` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 92. |
| `src/render_tail/vgl_scenes.rs:75` | `DISABLE` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 86. |
| `src/render_tail/vgl_scenes.rs:76` | `DEPTH_MASK` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 87. |
| `src/render_tail/vgl_scenes.rs:77` | `CLEAR_DEPTH` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 88. |
| `src/render_tail/vgl_scenes.rs:213` | `LIB` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 214. |
| `src/render_tail/vgl_scenes.rs:221` | `ONCE` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 222. |
| `src/render_tail.rs:25` | `SUBMIT` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 29. |
| `src/render_tail.rs:25` | `thread_local!` | state | TLS block. World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:39` | `STATE` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 42. |
| `src/render_tail.rs:39` | `thread_local!` | state | TLS block. World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:114` | `DISPATCH` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 120. |
| `src/render_tail.rs:115` | `DIRECT` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 124. |
| `src/render_tail.rs:149` | `LIB` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 150. |
| `src/render_tail.rs:158` | `FN` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 159. |
| `src/render_tail.rs:166` | `glBindTexture::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:166` | `glDepthMask::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:166` | `glUseProgram::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:167` | `glCullFace::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:167` | `glDepthFunc::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:167` | `glDisable::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:167` | `glEnable::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:167` | `glFrontFace::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:168` | `glBlendEquationSeparate::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:168` | `glBlendFuncSeparate::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:169` | `glScissor::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:169` | `glViewport::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:170` | `glUniform1fv::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:170` | `glUniform1i::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:170` | `glUniform2fv::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:171` | `glUniform3fv::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:171` | `glUniform4fv::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:172` | `glUniformMatrix4fv::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:173` | `glBufferData::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:173` | `glBufferSubData::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:174` | `glDrawElementsInstanced::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:175` | `glDeleteProgram::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:175` | `glLinkProgram::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:176` | `glActiveTexture::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:176` | `glBindBuffer::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:177` | `glVertexAttribPointer::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:178` | `glDisableVertexAttribArray::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:178` | `glEnableVertexAttribArray::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:178` | `glVertexAttribDivisor::FN` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. |
| `src/render_tail.rs:257` | `HOST` | cache | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 258. |
| `src/render_tail.rs:267` | `LAST` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 268. |
| `src/render_tail.rs:294` | `ONCE` | state | World/HUD submit, graphics setup or swap instrumentation. Keep GL dispatch, render targets and mutable view state on graphics owner. See line 295. |
| `src/replay/input.rs:41` | `SORTED` | cache | Owner capture resolves sorted replay key order. Share immutable key ordering. See line 42. |
| `src/replay/input.rs:95` | `MODE` | state | Owner begins the tape pulse and captures input. Pass latched data to worker, enter the frame clock on each consuming thread. See line 182. |
| `src/replay/input.rs:97` | `DT` | state | Owner replay installs physical input atoms before capture. Worker reads the latched StepInput. See line 221. |
| `src/replay/input.rs:98` | `KEYS_DOWN` | state | Owner replay installs physical input atoms before capture. Worker reads the latched StepInput. See line 222. |
| `src/replay/input.rs:99` | `KEYS_PRESSED` | state | Owner replay installs physical input atoms before capture. Worker reads the latched StepInput. See line 223. |
| `src/replay/input.rs:100` | `KEYS_RELEASED` | state | Owner replay installs physical input atoms before capture. Worker reads the latched StepInput. See line 224. |
| `src/replay/input.rs:101` | `MOUSE_BITS` | state | Owner replay installs physical input atoms before capture. Worker reads the latched StepInput. See line 225. |
| `src/replay/input.rs:102` | `MOUSE_POS` | state | Owner replay installs physical input atoms before capture. Worker reads the latched StepInput. See line 229. |
| `src/replay/input.rs:103` | `WHEEL` | state | Owner replay installs physical input atoms before capture. Worker reads the latched StepInput. See line 230. |
| `src/replay/input.rs:104` | `QUIT` | state | Owner replay installs physical input atoms before capture. Worker reads the latched StepInput. See line 231. |
| `src/replay/input.rs:106` | `PAD` | state | Owner replay installs physical input atoms before capture. Worker reads the latched StepInput. See line 212. |
| `src/replay/input.rs:107` | `CHARS` | state | Owner replay installs physical input atoms before capture. Worker reads the latched StepInput. See line 217. |
| `src/replay/input.rs:109` | `CLOCK` | state | Owner begins the tape pulse and captures input. Pass latched data to worker, enter the frame clock on each consuming thread. See line 210. |
| `src/replay/input.rs:110` | `ORIGIN` | state | Owner begins the tape pulse and captures input. Pass latched data to worker, enter the frame clock on each consuming thread. See line 211. |
| `src/replay/input.rs:119` | `thread_local!` | state | TLS block. StepGuard/ClockView guard in-step replay reads. Enter and suspend per consuming thread. |
| `src/replay/input.rs:120` | `CLOCK_VIEW` | state | StepGuard/ClockView guard in-step replay reads. Enter and suspend per consuming thread. See line 162. |
| `src/replay/input.rs:121` | `IN_STEP` | state | StepGuard/ClockView guard in-step replay reads. Enter and suspend per consuming thread. See line 132. |
| `src/replay/input.rs:375` | `CROSS` | cache | Owner begins the tape pulse and captures input. Pass latched data to worker, enter the frame clock on each consuming thread. See line 376. |
| `src/replay_capture.rs:19` | `PLAN` | cache | Owner replay capture reads the shot plan. Keep frame selection and screenshots with presentation. See line 20. |
| `src/route.rs:30` | `ON` | cache | Route/combat tracing reads its enable switch. Route mode stays serial. See line 31. |
| `src/shader_cache/mock.rs:9` | `MOCK` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 12. |
| `src/shader_cache/mock.rs:9` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/shader_cache/mock.rs:11` | `glsl_sema_mode` | state | No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/shader_cache/native.rs:29` | `glsl_sema_mode` | cache | Material build and program lookup use graphics context. Keep shader cache and naming scope on owner. See line 134. |
| `src/shader_cache/native.rs:48` | `STATE` | cache | Material build and program lookup use graphics context. Keep shader cache and naming scope on owner. See line 108. |
| `src/shader_cache/native.rs:48` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/shader_cache/native.rs:50` | `TEST_ROOT` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 56. |
| `src/shader_cache/native.rs:50` | `thread_local!` | cache | TLS block. Material build and program lookup use graphics context. Keep shader cache and naming scope on owner. |
| `src/shader_cache.rs:13` | `NAME` | cache | Material build and program lookup use graphics context. Keep shader cache and naming scope on owner. |
| `src/shader_cache.rs:13` | `thread_local!` | cache | TLS block. Material build and program lookup use graphics context. Keep shader cache and naming scope on owner. |
| `src/sim_maps/probe.rs:4` | `thread_local!` | counter | TLS block. Host frame/probe sequencing and counts. Stamp sim deltas, preserve submission order on owner. |
| `src/sim_maps/probe.rs:5` | `FRAME_NO` | counter | Host frame/probe sequencing and counts. Stamp sim deltas, preserve submission order on owner. See line 98. |
| `src/sim_maps/probe.rs:6` | `PACKET_NO` | counter | Host frame/probe sequencing and counts. Stamp sim deltas, preserve submission order on owner. See line 98. |
| `src/sim_maps/probe.rs:7` | `ALLOC_TRACKED` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 400. |
| `src/sim_maps/probe.rs:8` | `PREPARE_ALLOC` | counter | Host frame/probe sequencing and counts. Stamp sim deltas, preserve submission order on owner. See line 400. |
| `src/sim_maps/probe.rs:9` | `SKINS` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 114. |
| `src/sim_maps/probe.rs:10` | `TEXTURES` | cache | GPU resource and program labels attach to submissions. Keep lookup with graphics owner. See line 327. |
| `src/sim_maps/probe.rs:11` | `PACKETS` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 115. |
| `src/sim_maps/probe.rs:12` | `JOINT_ORDER` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 116. |
| `src/sim_maps/probe.rs:13` | `WORLD_ORDER` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 117. |
| `src/sim_maps/probe.rs:14` | `WORLD_PACKETS` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 118. |
| `src/sim_maps/probe.rs:15` | `WORLD_GEOMETRY` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 184. |
| `src/sim_maps/probe.rs:16` | `WORLD_RANGES` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 186. |
| `src/sim_maps/probe.rs:17` | `GL_TRACE` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 119. |
| `src/sim_maps/probe.rs:18` | `COUNTS` | counter | Host frame/probe sequencing and counts. Stamp sim deltas, preserve submission order on owner. See line 81. |
| `src/sim_maps/probe.rs:19` | `SPANS` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 121. |
| `src/sim_maps/probe.rs:20` | `START` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 126. |
| `src/sim_maps/probe.rs:21` | `PROJECTION` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 123. |
| `src/sim_maps/probe.rs:22` | `MOVER_REPLAY` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 152. |
| `src/sim_maps/probe.rs:23` | `NPC_ACTOR` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 34. |
| `src/sim_maps/probe.rs:24` | `NPC_DECISIONS` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 42. |
| `src/sim_maps/probe.rs:26` | `WORLD_VIEWPORT` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 109. |
| `src/sim_maps/probe.rs:29` | `ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 30. |
| `src/sim_maps/probe.rs:56` | `ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 57. |
| `src/sim_maps/probe.rs:63` | `ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 64. |
| `src/sim_maps/probe.rs:70` | `ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 71. |
| `src/sim_maps/probe.rs:75` | `DT` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 76. |
| `src/sim_maps/probe.rs:79` | `COUNTS_ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 80. |
| `src/sim_maps/probe.rs:87` | `COUNTS_ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 88. |
| `src/sim_maps/probe.rs:179` | `ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 180. |
| `src/sim_maps/probe.rs:209` | `ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 210. |
| `src/sim_maps/probe.rs:234` | `ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 235. |
| `src/sim_maps/probe.rs:239` | `ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 240. |
| `src/sim_maps/probe.rs:252` | `ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 253. |
| `src/sim_maps/probe.rs:256` | `ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 257. |
| `src/sim_maps/probe.rs:280` | `ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 281. |
| `src/sim_maps/probe.rs:337` | `BITS` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 338. |
| `src/sim_maps/probe.rs:352` | `VIEW` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 353. |
| `src/sim_maps/probe.rs:364` | `VIEWS` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 365. |
| `src/sim_maps/probe.rs:377` | `FRAMES` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 378. |
| `src/sim_maps/probe.rs:389` | `ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 390. |
| `src/sim_maps/probe.rs:398` | `ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 399. |
| `src/sim_maps/probe.rs:412` | `ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 413. |
| `src/sim_maps/probe.rs:415` | `OTHER` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 416. |
| `src/sim_maps/probe.rs:528` | `thread_local!` | diagnostic | TLS block. Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. |
| `src/sim_maps/probe.rs:529` | `ARMED` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 560. |
| `src/sim_maps/probe.rs:530` | `MARK` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 614. |
| `src/sim_maps/probe.rs:531` | `PENDING` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 669. |
| `src/sim_maps/probe.rs:532` | `SEQUENCE` | counter | Host frame/probe sequencing and counts. Stamp sim deltas, preserve submission order on owner. See line 564. |
| `src/sim_maps/probe.rs:533` | `SAMPLE_FRAME` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 573. |
| `src/sim_maps/probe.rs:534` | `PROGRAMS` | cache | GPU resource and program labels attach to submissions. Keep lookup with graphics owner. See line 580. |
| `src/sim_maps/probe.rs:538` | `TARGET` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 539. |
| `src/sim_maps/probe.rs:544` | `ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 545. |
| `src/sim_maps/probe.rs:551` | `ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 552. |
| `src/sim_maps/probe.rs:556` | `ON` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 557. |
| `src/sim_maps/probe.rs:565` | `LAST` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 566. |
| `src/sim_maps/probe.rs:568` | `FRAMES` | cache | Host fixture/probe accessor caches an immutable option. Read the initialized setting on both sides. See line 569. |
| `src/sim_maps/probe.rs:589` | `thread_local!` | cache | TLS block. GPU resource and program labels attach to submissions. Keep lookup with graphics owner. |
| `src/sim_maps/probe.rs:590` | `LABELS` | cache | GPU resource and program labels attach to submissions. Keep lookup with graphics owner. See line 598. |
| `src/sim_maps/probe.rs:591` | `PROP_SCOPE` | diagnostic | Host probe tracks sim and render per frame. Return stamped worker rows/deltas and keep GL labels and packet order with owner. See line 597. |
| `src/sim_maps.rs:63` | `TEST_SWITCHES` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 68. |
| `src/sim_maps.rs:63` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/sim_maps.rs:69` | `DEFAULTS` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 70. |
| `src/sim_maps.rs:124` | `R2_TEST` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 127. |
| `src/sim_maps.rs:124` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/sim_maps.rs:157` | `TRIGGER_FACTS_TEST` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 161. |
| `src/sim_maps.rs:157` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/sim_maps.rs:166` | `FX_OBSERVE_MOVE_TEST` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 170. |
| `src/sim_maps.rs:166` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/sim_maps.rs:179` | `MOVER_ADVANCE_REUSE_TEST` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 183. |
| `src/sim_maps.rs:179` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/sim_maps.rs:205` | `SEEN` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 209. |
| `src/sim_maps.rs:206` | `LOCAL` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 208. |
| `src/sim_maps.rs:206` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/sim_maps.rs:223` | `FX_ATTACHED_PERSIST_TEST` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 227. |
| `src/sim_maps.rs:223` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/sim_maps.rs:296` | `SIGHT_TEST` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 300. |
| `src/sim_maps.rs:296` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/sim_maps.rs:315` | `SWEEP_ANY_TEST` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 319. |
| `src/sim_maps.rs:315` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/sim_maps.rs:334` | `SWEEP_BLOCKED_TEST` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 338. |
| `src/sim_maps.rs:334` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/sim_maps.rs:357` | `SWEEP_MEMO_TEST` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 361. |
| `src/sim_maps.rs:357` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/sim_maps.rs:405` | `ARM_TEST` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 411. |
| `src/sim_maps.rs:405` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/sim_maps.rs:434` | `THINK_TEST` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 437. |
| `src/sim_maps.rs:434` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
| `src/skin_worker.rs:145` | `SPIN_US` | state | Existing skin worker reads its spin policy. Keep with skin dispatch and worker configuration. See line 146. |
| `src/skin_worker.rs:477` | `APPLIED` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 478. |
| `src/skin_worker.rs:477` | `thread_local!` | state | TLS block. Existing skin worker reads its spin policy. Keep with skin dispatch and worker configuration. |
| `src/spike_log.rs:85` | `FRAME` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 224. |
| `src/spike_log.rs:85` | `thread_local!` | diagnostic | TLS block. Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. |
| `src/spike_log.rs:94` | `OUT` | diagnostic | Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. See line 240. |
| `src/spike_log.rs:94` | `thread_local!` | diagnostic | TLS block. Step movement, NPC, camera or presentation instrumentation. Stamp samples on their measuring thread and join at the owner tail. |
| `src/startup.rs:6` | `STATE` | state | Death retry or chapter transition enters loading and recovery. Fence all control mutations and yields on owner. See line 24. |
| `src/startup.rs:6` | `thread_local!` | state | TLS block. Death retry or chapter transition enters loading and recovery. Fence all control mutations and yields on owner. |
| `src/story/registry.rs:59` | `LEGACY` | state | Campaign progression, story dispatch or save/restore reads authored visit data. Share immutable tables. See line 345. |
| `src/story/speakers.rs:9` | `SPEAKERS` | state | Campaign progression, story dispatch or save/restore reads authored visit data. Share immutable tables. See line 120. |
| `src/texture.rs:48` | `thread_local!` | cache | TLS block. Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. |
| `src/texture.rs:49` | `SHARED` | cache | Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. See line 185. |
| `src/texture.rs:51` | `MATERIAL_IMAGES` | cache | Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. See line 168. |
| `src/texture.rs:53` | `SHARE_WINDOW` | cache | Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. See line 66. |
| `src/texture.rs:113` | `TEST_SHARE` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 115. |
| `src/texture.rs:113` | `thread_local!` | cache | TLS block. Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. |
| `src/texture.rs:180` | `NEXT_UPLOAD` | cache | Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. See line 181. |
| `src/texture.rs:180` | `thread_local!` | cache | TLS block. Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. |
| `src/texture.rs:402` | `REDUCED` | cache | Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. |
| `src/texture_cache.rs:17` | `fast_texture_compression` | cache | Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. |
| `src/texture_cache.rs:26` | `MOCK` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 29. |
| `src/texture_cache.rs:26` | `thread_local!` | cache | TLS block. Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. |
| `src/texture_cache.rs:28` | `fast_texture_compression` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 74. |
| `src/texture_cache.rs:46` | `TEST_ROOT` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 48. |
| `src/texture_cache.rs:46` | `thread_local!` | cache | TLS block. Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. |
| `src/tls_count.rs:31` | `MODE` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 71. |
| `src/tls_count.rs:32` | `MAIN_HEAD` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 104. |
| `src/tls_count.rs:33` | `MAIN_LOOKUPS` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 78. |
| `src/tls_count.rs:34` | `MAIN_STEPS` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 79. |
| `src/tls_count.rs:35` | `MAIN_MISSES` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 80. |
| `src/tls_count.rs:36` | `MAIN_MAX_STEPS` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 86. |
| `src/tls_count.rs:38` | `MAIN_HISTOGRAM` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 85. |
| `src/tls_count.rs:39` | `OTHER_LOOKUPS` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 81. |
| `src/tls_count.rs:40` | `OTHER_STEPS` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 82. |
| `src/tls_count.rs:100` | `SITE_TABLE` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 107. |
| `src/tls_count.rs:101` | `SITE_OVERFLOW` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 117. |
| `src/tls_count.rs:222` | `FAST_TABLE` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 301. |
| `src/tls_count.rs:223` | `FAST_STACK_LOW` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 296. |
| `src/tls_count.rs:224` | `FAST_STACK_SIZE` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 296. |
| `src/tls_count.rs:225` | `FAST_OVERFLOW` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 252. |
| `src/tls_count.rs:226` | `FAST_UNSET` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 249. |
| `src/tls_count.rs:227` | `FAST_CHECKS` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 262. |
| `src/tls_count.rs:228` | `FAST_MISMATCHES` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 263. |
| `src/tls_count.rs:229` | `FAST_STARTED` | counter | Instrumented Vita TLS access enters the pthread wrapper on any thread. Keep main-only fast-table writes gated by stack identity and atomics for other threads. See line 359. |
| `src/tls_count.rs:388` | `TEST_LOCK` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 397. |
| `src/transition_memory.rs:5` | `ACTIVE` | state | Death retry or chapter transition enters loading and recovery. Fence all control mutations and yields on owner. See line 47. |
| `src/transition_memory.rs:6` | `PEAK` | state | Death retry or chapter transition enters loading and recovery. Fence all control mutations and yields on owner. See line 52. |
| `src/transition_memory.rs:7` | `PEAK_ARENA` | state | Death retry or chapter transition enters loading and recovery. Fence all control mutations and yields on owner. See line 53. |
| `src/transition_memory.rs:8` | `ALLOCATIONS` | state | Death retry or chapter transition enters loading and recovery. Fence all control mutations and yields on owner. See line 59. |
| `src/transition_memory.rs:70` | `thread_local!` | state | TLS block. Death retry or chapter transition enters loading and recovery. Fence all control mutations and yields on owner. |
| `src/transition_memory.rs:71` | `CURRENT` | state | Death retry or chapter transition enters loading and recovery. Fence all control mutations and yields on owner. See line 79. |
| `src/transition_memory.rs:168` | `LAST_PLAN_FRAME` | state | Death retry or chapter transition enters loading and recovery. Fence all control mutations and yields on owner. See line 171. |
| `src/transition_memory.rs:177` | `FIRED` | state | Death retry or chapter transition enters loading and recovery. Fence all control mutations and yields on owner. See line 178. |
| `src/transition_memory.rs:189` | `FAILED` | state | Death retry or chapter transition enters loading and recovery. Fence all control mutations and yields on owner. See line 195. |
| `src/transition_memory.rs:208` | `FAILED` | state | Death retry or chapter transition enters loading and recovery. Fence all control mutations and yields on owner. See line 210. |
| `src/transition_memory.rs:219` | `thread_local!` | state | TLS block. Recovery injection is owner-local. Capture once, peek on hop, consume with ui on the fenced path. |
| `src/transition_memory.rs:220` | `RECOVERY_KEY` | state | Recovery injection is owner-local. Capture once, peek on hop, consume with ui on the fenced path. See line 224. |
| `src/ui.rs:198` | `SHARED` | cache | Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. See line 201. |
| `src/ui.rs:198` | `thread_local!` | cache | TLS block. Level art load, draw or HUD lookup uses graphics resources. Keep creation, upload and release on the owner. |
| `src/vita_sim.rs:74` | `CONFIG` | cache | Step and renderer read configured simulation levers. Share initialized configuration. See line 76. |
| `src/weapons.rs:891` | `PROP_REUSE_TEST` | state | No. Test fixture or override. Keep scoped to its test thread or harness. See line 3969. |
| `src/weapons.rs:891` | `thread_local!` | state | TLS block. No. Test fixture or override. Keep scoped to its test thread or harness. |
