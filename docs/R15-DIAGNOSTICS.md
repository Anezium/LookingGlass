# r15 census diagnostics (schema 6)

Host/native qualification only. Gameplay, cadence, collision results, RNG,
particle population, order and render calls are unchanged. All diagnostic state
is gated by the existing census arm. No allocator wrapper is modified.

The first r15 commit fixes r14b's empty world leaves under frame_total_only.
Production Totals starts for the census even though light_profile::full() is
false. A prepared world draw has one start and three shared laps: four added
clock reads, three added intervals, zero reused intervals in this feature set.
Census off has zero world clock reads. Sky and joint-alpha Totals retain the
old full-profiler gate because they have no world-census publication. Outside
frame_total_only, full-profile-owned world intervals can be marked reused.
The 507-draw fixture contributes 1,521 added intervals and 2,028 reads. The
r14b claim of 1,521 reused intervals was produced by a test-only activation;
that helper is removed. Calibration remains outside the survey window.

Schema 6 appends 15 regions to schema 5 (67 total) and 24 counters (32 total).
Indexes 0..51 and counters 0..7 retain their meaning. Parent masks use two
unsigned 64-bit words, parent_masks and parent_masks_hi. Combine as
lo | (hi << 64); frame_total is bit 67. CSV appends the high-word region group
and the flattened NPC category counters. No precision-losing JSON floats or
128-bit JSON number support is needed. Every row still requires exact integer
sum(exclusive_ns) + unbracketed_residual_ns == frame_total_ns.

* Centipede: npc_generic_update is Npcs::update; npc_levels_step is the complete
  registered-controller/Pool step (Flora on centipede1). Both are children of
  npc_think_movement. Ant enabled update calls and 120-Hz substeps are counted
  once per update. Navigation walk/plan and Flora spawn-wave entries are counts.
* NPC queries: 3 controllers (generic/levels/other) x 5 categories
  (gravity_recoil/navigation_detour/sight_visibility/spawn_wave/other), exported
  as 15 rows of 12 integers. Existing outer sweep and static traversal clocks
  feed sweep_ns/static_ns; these nested views never add to frame coverage.
  Static columns include queries, nodes, tested/clipped hulls, planes,
  certificates and fallbacks. Cached npc_sight hits count logical queries but
  have no sweep/traversal time. Unclassified controllers and projectiles stay
  explicit in other. Shared recoil/Guard gravity, Ant gravity/sight, NPC sight,
  Detour and Flora spawn probes are classified without clocks in their loops.
* Repeat opportunities use exact endpoint/extent bits, world and immutable
  geometry identities, API mask (full/geometry/camera), ignored obstacle and a
  conservative global solid-publication epoch, within one attempted frame.
  Constructors, dynamic-copy/dynamic/support/weapon/camera publications bump the
  epoch even for identical geometry. The 1,024-entry, 16-probe table cannot
  produce hash false positives: full keys are compared. Evictions lose repeats
  and are exported. Cross-category repeats are credited at the repeated call.
  This sizes an opportunity; it never reuses a trace or proves safe savings.
* FX: fx_world_particles encloses the retained world-puff preparation and
  registration, excluding scene/world alpha preparation. fx_attached_sample_birth
  wraps one Attached::sample call, including birth-pose cache misses and puff
  reconstruction. New birth poses count calls to the existing birth_pose closure,
  not newly alive puffs; first/reverse samples may rebuild historical births.
  World new particles count successful births before the existing cap eviction.
  Live counts are populations observed at gather; multiple views/samplers sum.
  fx_quad_build wraps one set, fx_registration wraps a complete sort/register
  loop, and fx_worker_pair_wait wraps one synchronous CPU pair (service plus
  dispatch/join elapsed, including inline fallback). No clocks per puff, birth,
  quad or individual registration. Pair time is a child of quad build, not an
  independent frame budget or proof that workers were idle.
* Player: player_input_prepare covers the existing prepare/input block;
  player_map_hooks separates prepare_player, filter_level_controls and the
  controller-control predicate block. Existing fixed_physics supplies movement
  timing; player_fixed_ticks counts ticks. player_trigger_checks encloses the
  complete triggers call; visits, eligible checks, fires and event-fact builds
  count original work without changing mutation boundaries or caching facts.
  Audio, hints, inventory and recovery remain in the parent's exclusive budget.
* HUD: hud_drawing runs around hud_prompt_search, with nested hud_menus for
  chapter/inventory/console entries. Console counts include its closed early
  exit. Prompt toast rendering belongs to drawing. Search covers rope, talk,
  world prompt and owner selection. No search/range/visibility rule changes.
* Tail: tail_save and tail_profiler subdivide framework_tail. The r14 input,
  story, particles, camera, render/setup, first-person and backend-report roots
  remain. Residual outside these scopes is still unattributed, including
  uncovered framework/scheduling work; no claim about the utemple tail is made.

added_brackets + reused_brackets == sum(calls[30..67]). These are intervals,
not clock reads: adjacent world intervals share reads. NPC category timing
reuses already present query clocks and adds no category clocks. Other new
spans use two reads per coarse entry. The map fixtures include 507 artificial
world draws plus r15 path intervals: centipede1 4, utemple 10, wchess2 7,
hatter1 6 (each includes two tail spans). Their added totals are 1526, 1532,
1529 and 1528; reused totals are zero. The common 1,522 includes the world
intervals and the existing framework_tail parent. Counts depend on sampler/controller/view
entries in real play and must be read from the export, not inferred from these
fixtures. Elapsed values include observer bookkeeping/preemption; no automatic
bias correction or performance win is claimed.

Run the host suite with native_timing,vita_renderer,frame_total_only, followed
by tools/stack_r10/test_r15_diagnostics.py with ALICE_TIMING_TEST_OUTPUT pointing
at its fresh output directory. Tests exercise production gates/timers, real
Ant/attached-particle/player CPU work, and no-GL synthetic protocols for every
new leaf. They do not render a game frame. The validator checks work counters
against leaf calls (including world_draws==all three world-leaf calls), preserves
schemas 3/4/5, rejects missing-work mutants whose timing closure remains exact,
and pairs every new CSV column with JSON.

No console run, heap peak, visual/audio/gameplay or performance acceptance is
implied by host/native gates. The console owner must update their schema,
high-word parent masks, config suffix parsing and SELF identity to this r15
package. r14b's SELF must not be used for a world-leaf diagnostic session.
