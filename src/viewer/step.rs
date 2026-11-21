use crate::{
    audio::step::{AudioOp, AudioStep},
    character::{self, ArtOp},
    collision::World,
    hud::HudOp,
    interaction::{self, Interactions},
    inventory::{self, Stats},
    movement::{Controls, FixedClock, Player},
    recovery::{self, Recovery},
    save::{self, Campaign},
};
use anyhow::Result;
use macroquad::prelude::{vec2, vec3, Color, KeyCode, Vec3};
use std::sync::Arc;

/// The owned gameplay state at the synchronous step boundary.
pub(super) struct SimState {
    pub(super) world: World,
    pub(super) interactions: Interactions,
    pub(super) alice: character::AliceSim,
    pub(super) npcs: crate::npc::NpcSim,
    pub(super) steam: crate::particles::SteamSim,
    pub(super) input: crate::input::Input,
    pub(super) weapon_buttons: crate::weapon_rules::Buttons,
    pub(super) pickup_feedback: inventory::PickupFeedback,
    pub(super) third_person: bool,
    pub(super) follow_camera: crate::camera::Follow,
    pub(super) camera_handoff: crate::camera::Handoff,
    pub(super) rail_camera: crate::camera::Follow,
    pub(super) clock: FixedClock,
    pub(super) recovery: Recovery,
    pub(super) retry: recovery::RetryPoint,
    pub(super) paused: bool,
    pub(super) post_game: bool,
    pub(super) skip_scene: crate::cinematic::Skip,
    pub(super) hints: crate::cheshire::Hints,
    pub(super) entry_spawn: Option<String>,
    pub(super) stats: Stats,
    pub(super) pickups: Vec<inventory::Pickup>,
    pub(super) pickup_clock: f32,
    pub(super) environment_clock: f32,
    pub(super) story: crate::story::Story,
    pub(super) pos: Vec3,
    pub(super) yaw: f32,
    pub(super) pitch: f32,
    pub(super) flying: bool,
    pub(super) spawn_landing: bool,
    pub(super) player: Player,
    pub(super) campaign: Campaign,
    pub(super) played: bool,
}

const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<SimState>();
    assert_send::<crate::input::StepInput>();
    assert_send::<(StepJob, StepAInput)>();
    assert_send::<(StepJob, StepBInput)>();
    assert_send::<StepView>();
};

/// Values the owner has already resolved for the current step.
pub(super) struct StepLocals {
    pub(super) simulate: bool,
    pub(super) overlay_before: bool,
    pub(super) menu: bool,
    pub(super) inventory_menu: bool,
    pub(super) console_input: bool,
    pub(super) focused: bool,
    pub(super) scripted_view: bool,
    pub(super) bench: bool,
    pub(super) elapsed: f32,
    pub(super) dt: f32,
    pub(super) world_dt: f32,
    pub(super) in_transport: bool,
    pub(super) summon_cat: bool,
    pub(super) power_before: Option<crate::powerups::Kind>,
    pub(super) advance_dialogue: bool,
    pub(super) pending_transition: Option<LevelExit>,
    pub(super) weapon_click: Option<bool>,
}

pub(super) type LevelExit = (String, Option<String>);

pub(super) enum Due {
    None,
    Checkpoint,
    Ending,
    Exit(LevelExit),
}

pub(super) struct StepScopes {
    #[cfg(any(target_os = "vita", feature = "sim_maps_probe"))]
    pub(super) frame: Option<crate::frame_profile::Span>,
    pub(super) census: Option<crate::phase_census::Timer>,
}

impl StepScopes {
    #[cfg(any(target_os = "vita", feature = "sim_maps_probe"))]
    fn frame(&mut self, name: &'static str) {
        drop(self.frame.take());
        self.frame = Some(crate::frame_profile::span(name));
    }

    fn census(&mut self, region: crate::phase_census::Region) {
        drop(self.census.take());
        self.census = Some(crate::phase_census::span(region));
    }
}

#[derive(Default)]
pub(super) struct ConsoleOps {
    pub(super) close: bool,
    pub(super) message: Option<&'static str>,
}

pub(super) struct StepJob {
    pub(super) stamp: crate::sim_pipeline::Stamp,
    pub(super) sim: Box<SimState>,
    pub(super) locals: StepLocals,
    pub(super) art_ops: Vec<ArtOp>,
    pub(super) hud_ops: Vec<HudOp>,
    pub(super) audio_step: AudioStep,
    pub(super) console: ConsoleOps,
    #[cfg(feature = "vita_renderer")]
    pub(super) route_control: Option<crate::route_replay::Control>,
}

macro_rules! capture_replay_probe {
    ($map:expr, $want_state:expr, $player:expr, $stats:expr, $interactions:expr, $npcs:expr, $story:expr, $hints:expr, $alice:expr, $environment_clock:expr, $pickup_clock:expr, $yaw:expr, $pitch:expr, $clock:expr) => {{
            let gather = std::time::Instant::now();
            let targets = $npcs.targets();
            let state = $want_state.then(|| serde_json::to_vec(&serde_json::json!({
                "player": $player, "stats": $stats, "interactions": $interactions.snapshot(),
                "npcs": $npcs.snapshot(), "story": $story.snapshot(), "hints": $hints.snapshot(),
                "character": $alice.snapshot(), "environment_clock": $environment_clock,
                "pickup_clock": $pickup_clock, "yaw": $yaw, "pitch": $pitch, "ticks": $clock.ticks,
            }))).transpose()?;
            crate::replay::OwnedProbe {
                map: $map.to_owned(), feet: $player.feet, velocity: $player.velocity,
                yaw: $yaw, pitch: $pitch, sanity: $stats.sanity(), will: $stats.will(), alive: $stats.alive(),
                weapon: $stats.selected(), ticks: $clock.ticks, environment_clock: $environment_clock, targets, state,
                cost_us: gather.elapsed().as_micros() as u64,
            }
    }};
}
pub(super) use capture_replay_probe;

impl StepJob {
    pub(super) fn replay_probe(&self, map: &str, want_state: bool) -> Result<crate::replay::OwnedProbe> {
        let sim = &self.sim;
        Ok(capture_replay_probe!(map, want_state, sim.player, sim.stats, sim.interactions,
            sim.npcs, sim.story, sim.hints, sim.alice, sim.environment_clock,
            sim.pickup_clock, sim.yaw, sim.pitch, sim.clock))
    }
}

pub(super) struct StepAInput {
    pub(super) gameplay: bool,
    pub(super) map: Arc<crate::bsp::Bsp>,
    pub(super) preferences: Arc<crate::preferences::Preferences>,
    pub(super) saves_writable: bool,
    pub(super) host_natural_exit: bool,
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    pub(super) force_checkpoint: bool,
}

pub(super) struct StepBInput {
    pub(super) story_active: bool,
    pub(super) dispatched: bool,
    pub(super) map_name: String,
    pub(super) preferences: Arc<crate::preferences::Preferences>,
    pub(super) aspect: f32,
    pub(super) handoff_pose: Option<crate::cinematic::ActorPose>,
    pub(super) loot_visit: String,
    pub(super) loot_before: Vec<crate::loot::Source>,
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    pub(super) frame: u64,
    #[cfg(target_os = "vita")]
    pub(super) bench_camera: Option<(Vec3, Vec3, bool)>,
}

pub(super) struct CameraOut {
    pub(super) position: Vec3,
    pub(super) target: Vec3,
    pub(super) up: Vec3,
    pub(super) fovy: f32,
}

pub(super) struct StepView {
    pub(super) stamp: crate::sim_pipeline::Stamp,
    pub(super) presentation: crate::presentation_input::PresentationInput,
    pub(super) camera_out: CameraOut,
    pub(super) dynamic: crate::render_snapshot::DynamicInput,
    pub(super) active: bool,
    pub(super) direction: Vec3,
    pub(super) targets: Vec<crate::combat::Target>,
    pub(super) pand_cinematic: bool,
    pub(super) scripted_camera: Option<crate::cinematic::Camera>,
    pub(super) show_alice: bool,
    #[cfg(target_os = "vita")]
    pub(super) gameplay_alice: bool,
}

// If the checkpoint is due, its acknowledgement can affect later tests. Resume
// here afterwards without retesting the checkpoint. The ending branch likewise
// leaves before the story or host exit is consumed.
pub(super) fn step_due(
    checkpoint_allowed: bool,
    story_active: bool,
    post_game: bool,
    interactions: &mut Interactions,
    story: &mut crate::story::Story,
    pending_transition: &mut Option<LevelExit>,
    host_natural_exit: bool,
) -> Due {
    if checkpoint_allowed
        && story_active
        && interactions
            .levels
            .iter()
            .any(|s| s.ctl.checkpoint_requested())
    {
        return Due::Checkpoint;
    }
    if story_active && interactions.levels.iter().any(|s| s.ctl.ending_ready()) && !post_game {
        return Due::Ending;
    }
    if let Some(exit) = pending_transition.take() {
        story.defer_exit(exit);
    }
    if story_active {
        *pending_transition = interactions.take_story_exit(story);
    }
    if host_natural_exit {
        *pending_transition = Some((crate::transition_memory::host_destination(), None));
    }
    match pending_transition.take() {
        Some(exit) => Due::Exit(exit),
        None => Due::None,
    }
}

/// Gameplay movement when its arm is active, followed by the common story suffix.
pub(super) fn step_a(mut job: StepJob, input: StepAInput, scopes: &mut StepScopes) -> (StepJob, Result<(bool, Due)>) {
    let StepJob {
        sim,
        locals: step,
        art_ops,
        hud_ops,
        audio_step,
        console,
        #[cfg(feature = "vita_renderer")]
        route_control,
        ..
    } = &mut job;
    let StepLocals {
        simulate,
        overlay_before,
        menu,
        inventory_menu,
        console_input,
        focused,
        scripted_view,
        bench,
        elapsed,
        dt,
        world_dt,
        in_transport,
        summon_cat,
        ..
    } = *step;
    let StepAInput {
        gameplay,
        map,
        preferences,
        saves_writable,
        host_natural_exit,
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        force_checkpoint,
    } = input;
    #[cfg(not(all(feature = "sim_maps_probe", not(target_os = "vita"))))]
    let force_checkpoint = false;
    let preferences = &preferences;
    let mut advance_dialogue = step.advance_dialogue;
    let mut pending_transition = step.pending_transition.take();
    macro_rules! pressed {
        ($key:expr) => {
            focused && !console_input && sim.input.key(preferences, $key, true)
        };
    }
    macro_rules! held {
        ($key:expr) => {
            focused && !console_input && sim.input.key(preferences, $key, false)
        };
    }
    let result = (|| -> Result<_> {
        if gameplay {
            if !scripted_view && !bench {
                #[cfg(target_os = "vita")]
                sim.input
                    .vita_look(preferences, elapsed, &mut sim.yaw, &mut sim.pitch);
                #[cfg(not(target_os = "vita"))]
                if crate::replay::input::cross() {
                    sim.input
                        .vita_look(preferences, elapsed, &mut sim.yaw, &mut sim.pitch);
                } else {
                    let look = sim.input.look(preferences);
                    sim.yaw -= look.x * dt * 2.5;
                    sim.pitch += look.y * dt * 2.;
                }
            }
            if !scripted_view && held!(KeyCode::Left) {
                sim.yaw += dt * 1.3;
            }
            if !scripted_view && held!(KeyCode::Right) {
                sim.yaw -= dt * 1.3;
            }
            if !scripted_view && held!(KeyCode::Up) {
                sim.pitch += dt;
            }
            if !scripted_view && held!(KeyCode::Down) {
                sim.pitch -= dt;
            }
            sim.pitch = sim.pitch.clamp(-1.5, 1.5);
            let forward = vec3(
                sim.yaw.cos() * sim.pitch.cos(),
                sim.yaw.sin() * sim.pitch.cos(),
                sim.pitch.sin(),
            );
            let right = vec3(sim.yaw.sin(), -sim.yaw.cos(), 0.);
            let mut movement = forward * sim.input.movement().y + right * sim.input.movement().x;
            if held!(KeyCode::W) {
                movement += forward;
            }
            if held!(KeyCode::S) {
                movement -= forward;
            }
            if held!(KeyCode::D) {
                movement += right;
            }
            if held!(KeyCode::A) {
                movement -= right;
            }
            if sim.flying {
                sim.player.release_rope();
            }
            if sim.flying && held!(KeyCode::E) {
                movement.z += 1.;
            }
            if sim.flying && held!(KeyCode::Q) {
                movement.z -= 1.;
            }
            let speed = if held!(KeyCode::LeftShift) {
                800.
            } else {
                220.
            };
            if sim.flying {
                sim.pos += movement.normalize_or_zero() * speed * dt;
                sim.clock.pause();
            } else {
                // Looking up/down must not change walking speed or produce flight.
                let aim = vec3(
                    sim.yaw.cos() * sim.pitch.cos(),
                    sim.yaw.sin() * sim.pitch.cos(),
                    sim.pitch.sin(),
                );
                use crate::interaction::friendly::{use_owner, UseOwner};
                let shared_rope = sim.player.rope_prompt(&sim.world).is_some();
                let world_rope = sim
                    .interactions
                    .pandemonium
                    .as_ref()
                    .is_some_and(|p| p.prompt(&sim.world, sim.player.eye()).is_some());
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                {
                    scopes.frame("frame_interaction_search");
                }
                crate::spike_log::sim(crate::spike_log::Sim::Search);
                #[cfg(target_os = "vita")]
                {
                    scopes.frame("frame_interaction_search");
                    crate::frame_profile::set_owner(crate::frame_profile::Owner::InteractionSearch);
                }
                let talk = sim.interactions.resolve_conversation(
                    &sim.world,
                    sim.player.eye(),
                    aim,
                    &sim.story,
                    |name| {
                        sim.npcs
                            .talk_target(name, &sim.world, sim.player.eye(), aim)
                    },
                );
                let owner = use_owner(
                    sim.story.busy(),
                    sim.interactions.scripted(),
                    shared_rope,
                    world_rope,
                    talk.is_some(),
                    sim.interactions
                        .prompt(&sim.world, sim.player.eye(), aim)
                        .is_some(),
                );
                let use_pressed = pressed!(KeyCode::E);
                advance_dialogue = use_pressed && owner == UseOwner::Dialogue;
                let mut events = interaction::Events::default();
                if use_pressed && owner == UseOwner::Talk {
                    if let Some(talk) = talk {
                        if let Some(result) =
                            sim.interactions.start_conversation(talk, &mut sim.story)
                        {
                            sim.npcs.begin_talk(talk.actor());
                            events.merge(result);
                        }
                    }
                }
                scopes.census(crate::phase_census::Region::Objects);
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                {
                    scopes.frame("frame_interactions");
                }
                crate::spike_log::sim(crate::spike_log::Sim::Interactions);
                #[cfg(target_os = "vita")]
                {
                    scopes.frame("frame_interactions");
                    crate::frame_profile::set_owner(crate::frame_profile::Owner::Interactions);
                }
                events.merge(sim.interactions.update(
                    world_dt,
                    &map,
                    &mut sim.world,
                    &sim.player,
                    aim,
                    use_pressed && owner == UseOwner::World,
                )?);
                if events.damage > 0. {
                    sim.stats.damage(events.damage);
                    hud_ops.push(HudOp::Announce("Hazard / Sanity lost".into()));
                }
                for event in events.story {
                    sim.story.trigger(&event);
                }
                pending_transition = events.transition.or(pending_transition.take());
                if let Some(message) = events.message {
                    hud_ops.push(HudOp::Announce(message.into()));
                }
                if let Some(sound) = events.sound {
                    audio_step.effect(sound, None);
                }
                scopes.census(crate::phase_census::Region::Movers);
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                {
                    scopes.frame("frame_movers");
                }
                crate::spike_log::sim(crate::spike_log::Sim::Movers);
                #[cfg(target_os = "vita")]
                {
                    scopes.frame("frame_movers");
                    crate::frame_profile::set_owner(crate::frame_profile::Owner::None);
                    crate::phase_census::set_owner(crate::phase_census::Owner::Other);
                }
                let was_cinematic = sim.interactions.scripted();
                let mover_dt = if sim
                    .interactions
                    .levels
                    .iter()
                    .any(|s| s.ctl.ignores_watch())
                {
                    dt
                } else {
                    world_dt
                };
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                let _movers_companions = crate::sim_maps::timer("movers_companions_ns");
                sim.interactions
                    .companions_for_movers(&sim.npcs.companions());
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                drop(_movers_companions);
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                let _movers_targets = crate::sim_maps::timer("movers_targets_ns");
                let mover_targets = sim.npcs.targets();
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                {
                    drop(_movers_targets);
                    crate::sim_maps::count("movers_target_rows", mover_targets.len() as u64);
                }
                sim.interactions.actors_for_movers(&mover_targets);
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                if crate::sim_maps::probe::mover_replay_enabled() {
                    if let Some(s) = &sim.interactions.school2 {
                        crate::sim_maps::probe::mover_ride(s.moving_tops(), &mut sim.player);
                    }
                }
                {
                    let _time = crate::sim_maps::timer("r26_movers_ns");
                    sim.interactions.advance_school(
                        mover_dt,
                        &map,
                        &mut sim.world,
                        &mut sim.player,
                    )?;
                }
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                if crate::sim_maps::probe::mover_replay_enabled() {
                    crate::sim_maps::probe::record_movers(
                        &sim.interactions.transforms(),
                        &sim.world,
                        &sim.player,
                    );
                }
                scopes.census(crate::phase_census::Region::PlayerMovement);
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                {
                    scopes.frame("frame_movement");
                }
                crate::spike_log::sim(crate::spike_log::Sim::Movement);
                #[cfg(target_os = "vita")]
                {
                    scopes.frame("frame_movement");
                    crate::frame_profile::set_owner(crate::frame_profile::Owner::Movement);
                    crate::phase_census::set_owner(crate::phase_census::Owner::Player);
                }
                let prepare_path =
                    crate::phase_census::span(crate::phase_census::Region::PlayerPrepare);
                crate::phase_census::count_under(
                    crate::phase_census::Region::PlayerMovement,
                    crate::phase_census::Counter::PlayerPrepare,
                    1,
                );
                if was_cinematic && !sim.interactions.scripted() {
                    sim.yaw = sim.player.script_facing;
                    sim.pitch = 0.;
                    sim.clock.pause();
                    sim.spawn_landing = true;
                }
                let before_feet = sim.player.feet;
                let before_water = sim.player.immersion;
                let before_drowning = sim.player.breath.hits;
                let before_air_warning = sim.player.breath.warning_age().is_some();
                sim.stats.prepare_player(&mut sim.player);
                let before_grounded = sim.player.grounded;
                // A supported spawn has already landed before its first tick.
                // Its first later fall must receive ordinary fall damage.
                if before_grounded {
                    sim.spawn_landing = false;
                }
                sim.interactions
                    .prepare_player(&mut sim.stats, &mut sim.player);
                let before_jumps = sim.player.jumps;
                let before_landings = sim.player.landings;
                let before_fall = sim.player.velocity.z;
                let flat = vec2(sim.yaw.cos(), sim.yaw.sin());
                let right = vec2(sim.yaw.sin(), -sim.yaw.cos());
                let mut wish = flat * sim.input.movement().y + right * sim.input.movement().x;
                if held!(KeyCode::W) {
                    wish += flat;
                }
                if held!(KeyCode::S) {
                    wish -= flat;
                }
                if held!(KeyCode::D) {
                    wish += right;
                }
                if held!(KeyCode::A) {
                    wish -= right;
                }
                let mut control = Controls {
                    use_pressed: use_pressed
                        && matches!(owner, UseOwner::SharedRope | UseOwner::Traversal),
                    wish: wish.clamp_length_max(1.),
                    swim: movement.clamp_length_max(1.),
                    rise: f32::from(held!(KeyCode::Space)) - f32::from(held!(KeyCode::LeftControl)),
                    jump: !overlay_before && pressed!(KeyCode::Space),
                    run: sim.input.running(preferences, held!(KeyCode::LeftShift)),
                };
                #[cfg(target_os = "vita")]
                if bench {
                    control = Controls::default();
                }
                sim.interactions.filter_level_controls(&mut control);
                drop(prepare_path);
                let mut control_hooks = Some(crate::phase_census::span(
                    crate::phase_census::Region::PlayerHooks,
                ));
                crate::phase_census::count_under(
                    crate::phase_census::Region::PlayerMovement,
                    crate::phase_census::Counter::PlayerHooks,
                    1,
                );
                let controlled = sim
                    .interactions
                    .pandemonium
                    .as_mut()
                    .is_some_and(|p| p.control(world_dt, &sim.world, &mut sim.player, control));
                if !controlled
                    && !scripted_view
                    && !sim.interactions.scripted()
                    && !sim
                        .interactions
                        .duchess
                        .as_ref()
                        .is_some_and(|d| d.controlled())
                    && !sim
                        .interactions
                        .school
                        .as_ref()
                        .is_some_and(|s| s.cinematic())
                    && !sim.interactions.levels_controlled()
                {
                    if sim.alice.ice_locked() {
                        control = Controls::default();
                        sim.player.velocity = Vec3::ZERO;
                    }
                    if sim.alice.attack_movement_locked() {
                        control = Controls::default();
                    }
                    drop(control_hooks.take());
                    sim.world.set_weapon_obstacles(sim.alice.ice_targets());
                    #[cfg(feature = "vita_renderer")]
                    if let Some(route) = route_control {
                        sim.clock
                            .advance_route(elapsed as f64, &sim.world, &mut sim.player, |p| {
                                route.control(&sim.world, p, &mut sim.yaw, &mut sim.pitch)
                            });
                    } else {
                        sim.clock
                            .advance(elapsed as f64, &sim.world, &mut sim.player, control);
                    }
                    #[cfg(not(feature = "vita_renderer"))]
                    sim.clock
                        .advance(elapsed as f64, &sim.world, &mut sim.player, control);
                }
                drop(control_hooks);
                sim.pos = sim.player.eye();
                audio_step.ops.push(AudioOp::Movement {
                    distance: (sim.player.feet - before_feet).truncate().length(),
                    grounded: sim.player.grounded
                        && before_grounded
                        && sim.player.immersion.level == 0
                        && !sim.input.running(preferences, held!(KeyCode::LeftShift)),
                    jumped: sim.player.jumps > before_jumps,
                    landed: sim.player.landings > before_landings && before_fall < -90.,
                });
                audio_step.ops.push(AudioOp::Water {
                    dt,
                    before: before_water.level,
                    now: sim.player.immersion.level,
                    speed: sim.player.velocity.length(),
                });
                if before_water.level < 2 && sim.player.immersion.level >= 2 {
                    hud_ops.push(HudOp::Announce(
                        "Swimming / Space rise / Ctrl dive / look and W to swim".into(),
                    ));
                }
                let liquid_damage = std::mem::take(&mut sim.player.liquid_damage);
                sim.stats.damage(liquid_damage);
                if sim.player.breath.hits != before_drowning {
                    audio_step.effect("sound/character/alice/choke1.wav", None);
                }
                if !before_air_warning
                    && sim.player.breath.warning_age().is_some()
                    && sim.player.immersion.level == 3
                    && sim.stats.alive()
                {
                    audio_step.effect("sound/character/alice/choke2.wav", None);
                }
                if liquid_damage > 0. && before_water.kind != sim.player.immersion.kind {
                    hud_ops.push(HudOp::Announce("Harmful liquid / Sanity lost".into()));
                }
                if sim.player.landings > before_landings {
                    let lost = if sim.spawn_landing {
                        0.
                    } else {
                        sim.stats.fall(sim.player.landing_speed)
                    };
                    sim.spawn_landing = false;
                    if lost > 0. {
                        hud_ops.push(HudOp::Announce(
                            format!("Hard landing / -{lost:.0} Sanity").into(),
                        ));
                    }
                }
                sim.hints.sync(&sim.interactions);
                sim.hints.observe(before_feet, sim.player.feet);
                let events = sim.interactions.triggers(dt, before_feet, sim.player.feet);
                for event in &events.story {
                    sim.story.trigger(event);
                }
                if events.damage > 0. {
                    sim.stats.damage(events.damage);
                    hud_ops.push(HudOp::Announce("Hazard / Sanity lost".into()));
                }
                if let Some(message) = events.message {
                    hud_ops.push(HudOp::Announce(message.into()));
                }
                if let Some(sound) = events.sound {
                    audio_step.effect(sound, None);
                }
                if sim.stats.alive() {
                    pending_transition = events.transition.or(pending_transition.take());
                    if let Some((feet, angle)) = events.teleport {
                        if sim.world.body_clear(feet) {
                            sim.player = Player::new(feet);
                            sim.yaw = angle;
                            sim.pos = sim.player.eye();
                            art_ops.push(ArtOp::RetireAliceBody);
                            sim.alice.clear_water();
                            art_ops.push(ArtOp::ResetWaterEmitter);
                            sync_water(sim, art_ops);
                            sim.alice.reset(&sim.player, sim.yaw);
                            art_ops.push(ArtOp::ClearAliceEffects);
                            sim.follow_camera.reset();
                            sim.camera_handoff.reset();
                            sim.clock.pause();
                            sim.spawn_landing = true;
                            sim.recovery.clear();
                            hud_ops.push(HudOp::Announce("Teleported".into()));
                        } else {
                            hud_ops
                                .push(HudOp::Announce("Teleport destination is obstructed".into()));
                        }
                    }
                }
                if !in_transport && recovery::outside_world(&sim.world, sim.player.feet) {
                    sim.stats.damage(10000.);
                    pending_transition = None;
                    hud_ops.push(HudOp::Announce(
                        "Lost beyond the level / Enter to retry".into(),
                    ));
                }
                if sim.stats.alive() {
                    let dice_before = sim.stats.copies(6);
                    let rage_before = sim.stats.powers.rage;
                    if let Some(s) = &mut sim.interactions.school {
                        let e = s.collect_secret(&mut sim.stats, sim.player.feet, &sim.world);
                        if let Some(m) = e.message {
                            hud_ops.push(HudOp::Announce(m.into()));
                        }
                        for event in e.story {
                            sim.story.trigger(&event);
                        }
                    }
                    for message in inventory::collect_with(
                        &mut sim.stats,
                        &sim.pickups,
                        sim.player.feet,
                        &sim.world,
                        |item| {
                            art_ops.push(ArtOp::PickupCollected {
                                item: item.clone(),
                                clock: sim.pickup_clock,
                            })
                        },
                    ) {
                        hud_ops.push(HudOp::Announce(message.into()));
                    }
                    if sim.stats.copies(6) > dice_before {
                        sim.story.trigger("dice_cat");
                    }
                    if sim.stats.powers.rage > rage_before {
                        audio_step.effect("sound/item/ragebox/ragebox_pickup.wav", None);
                    }
                    if sim.player.script_motion == 0 {
                        sim.recovery
                            .observe(&sim.world, &sim.interactions, &sim.player, sim.yaw);
                    }
                }
            }
        }
        let story_active = !sim.paused
            && !menu
            && !inventory_menu
            && !console_input
            && focused
            && !sim.flying
            && sim.stats.alive();
        if simulate {
            scopes.census(crate::phase_census::Region::StoryWeapons);
            #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
            {
                scopes.frame("frame_story");
            }
            crate::spike_log::sim(crate::spike_log::Sim::Story);
            #[cfg(target_os = "vita")]
            {
                scopes.frame("frame_story");
                crate::frame_profile::set_owner(crate::frame_profile::Owner::None);
                crate::phase_census::set_owner(crate::phase_census::Owner::Other);
            }
            sim.hints.sync(&sim.interactions);
            if (summon_cat || (story_active && pressed!(KeyCode::C))) && !in_transport {
                let outcome = if !sim.stats.alive() || sim.flying {
                    Err("Return to walking and retry before summoning Cheshire.")
                } else {
                    sim.hints.summon(
                        &mut sim.story,
                        &sim.world,
                        sim.player.feet,
                        sim.yaw,
                        sim.player.swimming,
                    )
                };
                match outcome {
                    Ok(()) => {
                        if sim.hints.visible() {
                            audio_step.effect("sound/character/cheshire_cat/appear.wav", None);
                        }
                        console.close = true;
                    }
                    Err(message) => {
                        hud_ops.push(HudOp::Announce(message.into()));
                        if summon_cat {
                            console.message = Some(message);
                        }
                    }
                }
            }
            sim.interactions
                .sync_pickups(&sim.stats, &mut sim.pickups, &mut sim.story);
            if let Some(b) = &mut sim.interactions.beyond {
                b.bind_rage_player(&mut sim.player, sim.alice.facing());
            }
            let story_ready = sim.interactions.prepare_story(&mut sim.story);
            if let Some(request) = sim.story.update(
                if story_active && story_ready && sim.hints.prepare_story(&sim.story) {
                    elapsed
                } else {
                    0.
                },
                advance_dialogue,
            ) {
                audio_step.ops.push(AudioOp::Voice(request));
            }
            sim.interactions.sync_cinematic_story(&sim.story);
            sim.hints.sync(&sim.interactions);
            if sim
                .hints
                .update(if story_active { dt } else { 0. }, sim.story.hint_active())
            {
                audio_step.effect("sound/character/cheshire_cat/disappear.wav", None);
            }
            for event in sim.story.take_completed() {
                let effects = sim.interactions.completed_dialogue(&event);
                if let Some(message) = effects.message {
                    hud_ops.push(HudOp::Announce(message.into()));
                }
                if let Some(sound) = effects.sound {
                    audio_step.effect(sound, None);
                }
            }
            if let Some(s) = &mut sim.interactions.school2 {
                s.summon(sim.alice.ally_target());
                s.notarget(
                    sim.stats.ignores_alice()
                        || sim
                            .interactions
                            .school
                            .as_ref()
                            .is_some_and(|s| s.cinematic()),
                );
                let (events, feedback) = s.update(
                    if story_active { world_dt } else { 0. },
                    &sim.world,
                    sim.player.feet,
                    sim.story.busy(),
                );
                sim.stats.school_items = s.quest.items.clone();
                for &hit in &feedback.summon_hits {
                    sim.alice.hit_summon(hit);
                }
                sim.stats.damage(feedback.damage);
                if feedback.will_drain > 0. {
                    sim.stats
                        .spend_will(feedback.will_drain.min(sim.stats.will()));
                }
                sim.player.knockback(feedback.impulse);
                for event in events.story {
                    sim.story.trigger(&event);
                }
                audio_step.feedback(feedback);
                if let Some(sound) = events.sound {
                    audio_step.effect(sound, None);
                }
                if let Some(message) = events.message {
                    hud_ops.push(HudOp::Announce(message.into()));
                }
            }
            if let Some(s) = &mut sim.interactions.school {
                s.sync_inventory(&mut sim.stats);
            }
            if let Some(d) = &mut sim.interactions.duchess {
                if story_active && sim.stats.alive() {
                    if let Some(target) = d.target() {
                        d.threatened(sim.alice.threatens(&sim.world, target));
                    }
                }
                let before_stage = d.state.stage;
                d.state.notarget = sim.stats.ignores_alice();
                d.state.opponents.summon = sim.alice.ally_target();
                let f = d.update(
                    if story_active { world_dt } else { 0. },
                    &sim.world,
                    &mut sim.player,
                    &mut sim.stats,
                    &mut sim.story,
                );
                for &hit in &f.summon_hits {
                    sim.alice.hit_summon(hit);
                }
                sim.stats.damage(f.damage);
                sim.player.knockback(f.impulse);
                audio_step.feedback(f);
                if before_stage == crate::duchess::Stage::Waiting && d.cinematic() {
                    sim.recovery.clear();
                }
                if in_transport && !d.cinematic() {
                    sim.yaw = sim.player.script_facing;
                    sim.pitch = 0.;
                    sim.clock.pause();
                    sim.spawn_landing = true;
                }
                sim.pos = sim.player.eye();
            }
        }
        // The host control reaches the same owner continuation as an
        // authored checkpoint without adding a pre-hop fence.
        let due = if simulate && force_checkpoint {
            Due::Checkpoint
        } else if simulate {
            step_due(
                saves_writable,
                story_active,
                sim.post_game,
                &mut sim.interactions,
                &mut sim.story,
                &mut pending_transition,
                host_natural_exit,
            )
        } else {
            Due::None
        };
        Ok((story_active, due))
    })();
    step.advance_dialogue = advance_dialogue;
    step.pending_transition = pending_transition;
    (job, result)
}

/// Particles, combat, camera and listener commands at their original serial cut.
pub(super) fn step_b(mut job: StepJob, input: StepBInput, scopes: &mut StepScopes) -> (StepJob, StepView) {
    let stamp = job.stamp;
    let StepJob {
        sim,
        locals: step,
        art_ops,
        hud_ops,
        audio_step,
        ..
    } = &mut job;
    let StepLocals {
        simulate,
        menu,
        inventory_menu,
        console_input,
        focused,
        elapsed,
        dt,
        world_dt,
        power_before,
        ..
    } = *step;
    let StepBInput {
        story_active,
        dispatched,
        map_name,
        preferences,
        aspect,
        handoff_pose,
        loot_visit,
        loot_before,
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        frame,
        #[cfg(target_os = "vita")]
        bench_camera,
    } = input;
    let preferences = &preferences;
    let loot_visit = loot_visit.as_str();
    let loot_before = loot_before.as_slice();
    macro_rules! held {
        ($key:expr) => {
            focused && !console_input && sim.input.key(preferences, $key, false)
        };
    }
    scopes.census(crate::phase_census::Region::Particles);
    if simulate {
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        {
            scopes.frame("frame_particles");
        }
        crate::spike_log::sim(crate::spike_log::Sim::Particles);
        #[cfg(target_os = "vita")]
        {
            scopes.frame("frame_particles");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::None);
            crate::phase_census::set_owner(crate::phase_census::Owner::Other);
        }
        if !sim.paused && !menu && !inventory_menu && !console_input && focused && sim.stats.alive()
        {
            sim.environment_clock += world_dt;
            {
                #[cfg(target_os = "vita")]
                let _hotspot = crate::frame_profile::span("steam_animate");
                sim.steam.animate(sim.environment_clock);
            }
            if sim
                .interactions
                .school
                .as_ref()
                .is_some_and(|s| s.theatre_finished())
            {
                sim.steam.stop();
            }
            {
                #[cfg(target_os = "vita")]
                let _hotspot = crate::frame_profile::span("steam_sync");
                sim.steam.sync(&sim.interactions.event_world);
            }
            if let Some(v) = &sim.interactions.village {
                {
                    #[cfg(target_os = "vita")]
                    let _hotspot = crate::frame_profile::span("village_particle_bindings");
                    v.place_particles(&mut sim.steam);
                }
            }
            if let Some(p) = &sim.interactions.pandemonium {
                p.gate_particles(&mut sim.steam);
            }
            sim.interactions.place_level_particles(&mut sim.steam);
            sim.steam.update_clocks(dt, world_dt, sim.pos, &sim.world);
        }
    } // simulation: particles
    scopes.census(crate::phase_census::Region::StoryWeapons);
    crate::spike_log::sim(crate::spike_log::Sim::Combat);
    #[cfg(target_os = "vita")]
    if simulate {
        scopes.frame("frame_combat");
        crate::frame_profile::set_owner(crate::frame_profile::Owner::Combat);
    }
    // Scene controllers may finish during movement, dialogue or boss updates.
    // Resolve the handoff once, after all three, before animation/aim/rendering.
    let still_scripted = sim.interactions.scripted()
        || sim
            .interactions
            .duchess
            .as_ref()
            .is_some_and(|d| d.cinematic())
        || sim
            .interactions
            .school
            .as_ref()
            .and_then(|s| s.return_visit.as_ref())
            .is_some_and(|s| s.camera().is_some());
    if simulate && sim.camera_handoff.was_scripted() && !still_scripted {
        if handoff_pose.is_some() {
            assert!(!dispatched, "dispatched step consumed a draw-produced handoff pose");
        }
        if !sim.third_person {
            sim.camera_handoff.skip();
        }
        sim.yaw = sim.player.script_facing;
        sim.pitch = 0.;
        sim.pos = sim.player.eye();
        sim.clock.pause();
        sim.spawn_landing = true;
        step.weapon_click = None;
        sim.weapon_buttons.block();
        sim.alice
            .resume_scene(&sim.player, sim.yaw, handoff_pose.as_ref());
    }
    let direction = vec3(
        sim.yaw.cos() * sim.pitch.cos(),
        sim.yaw.sin() * sim.pitch.cos(),
        sim.pitch.sin(),
    );
    let active = !sim.paused
        && !menu
        && !inventory_menu
        && !console_input
        && focused
        && !sim.flying
        && sim.stats.alive();
    if active && simulate {
        sim.played = true;
        sim.stats.update(dt);
        sim.pickup_clock += dt;
    }
    // Keep conversation participants safe while listening. Exploration continues;
    // enemies and attacks resume after the conversation.
    let pand_cinematic = sim
        .interactions
        .duchess
        .as_ref()
        .is_some_and(|d| d.cinematic())
        || sim.interactions.scripted();
    if simulate {
        if sim.story.busy()
            || pand_cinematic
            || sim
                .interactions
                .school
                .as_ref()
                .is_some_and(|s| s.cinematic())
        {
            step.weapon_click = None;
            sim.weapon_buttons.block();
            sim.alice.cancel_weapon_action();
            sim.alice.dismiss_summon();
        }
        if sim.flying {
            sim.alice.cancel_weapon_action();
        }
        if sim
            .interactions
            .levels
            .iter()
            .any(|s| s.ctl.dismiss_summons())
        {
            sim.alice.dismiss_summon();
        }
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        {
            scopes.frame("frame_combat");
        }
        crate::spike_log::sim(crate::spike_log::Sim::Encounters);
        #[cfg(target_os = "vita")]
        {
            scopes.frame("frame_combat");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::Combat);
        }
        sim.interactions.activate_enemies();
        let scripted_motion = sim.interactions.scripted();
        if let Some(s) = &mut sim.interactions.encounters {
            s.summon(sim.alice.ally_target());
            s.notarget(
                sim.stats.ignores_alice()
                    || sim
                        .interactions
                        .school
                        .as_ref()
                        .is_some_and(|s| s.cinematic()),
            );
            let f = s.update(
                if active
                    && !sim.story.busy()
                    && !sim
                        .interactions
                        .school
                        .as_ref()
                        .is_some_and(|s| s.cinematic())
                    && !scripted_motion
                {
                    world_dt
                } else {
                    0.
                },
                &sim.world,
                sim.player.eye(),
            );
            for &hit in &f.summon_hits {
                sim.alice.hit_summon(hit);
            }
            sim.stats.damage(f.damage);
            sim.player.knockback(f.impulse);
            audio_step.feedback(f);
        }
        crate::spike_log::sim(crate::spike_log::Sim::Levels);
        if sim.interactions.has_level_combat() {
            // Pool's scene-owned ants and registered controllers share the route's combat hook.
            let notarget = sim.stats.ignores_alice();
            let summon = sim.alice.ally_target();
            let dt = if active
                && !sim.story.busy()
                && !sim
                    .interactions
                    .school
                    .as_ref()
                    .is_some_and(|s| s.cinematic())
                && !scripted_motion
            {
                if sim
                    .interactions
                    .levels
                    .iter()
                    .any(|s| s.ctl.ignores_watch())
                {
                    dt
                } else {
                    world_dt
                }
            } else {
                0.
            };
            let threatens = |t: &crate::combat::Target| sim.alice.threatens(&sim.world, *t);
            let f = sim.interactions.levels_step(&mut crate::level::Combat {
                dt,
                world: &sim.world,
                player: &mut sim.player,
                stats: &mut sim.stats,
                story: &mut sim.story,
                notarget,
                summon,
                threatens: &threatens,
            });
            for &hit in &f.summon_hits {
                sim.alice.hit_summon(hit);
            }
            sim.stats.damage(f.damage);
            sim.player.knockback(f.impulse);
            audio_step.feedback(f);
        }
        crate::spike_log::sim(crate::spike_log::Sim::Npcs);
        sim.npcs.summon(sim.alice.ally_target());
        sim.npcs.activate_levels(&sim.interactions.levels);
        sim.npcs.notarget(sim.stats.ignores_alice());
        if active && !sim.story.busy() && !sim.interactions.scripted() && world_dt > 0. {
            sim.npcs
                .chess_threats(|t| sim.alice.threatens(&sim.world, *t));
        }
        sim.npcs.level_patrols(&sim.interactions.levels);
        let feedback = sim.npcs.update(
            if active
                && !sim.story.busy()
                && !sim
                    .interactions
                    .school
                    .as_ref()
                    .is_some_and(|s| s.cinematic())
                && !sim.interactions.scripted()
            {
                world_dt
            } else {
                0.
            },
            &sim.world,
            sim.player.eye(),
        );
        crate::spike_log::sim(crate::spike_log::Sim::CombatRest);
        sim.alice
            .story_pose(&sim.story, if story_active { dt } else { 0. });
        sim.npcs.story_pose(
            &sim.story,
            if active { world_dt } else { 0. },
            &sim.world,
            sim.player.eye(),
        );
        if let Some(v) = &sim.interactions.village {
            v.cinema.apply_npc_sim(&mut sim.npcs);
        }
        for &hit in &feedback.summon_hits {
            sim.alice.hit_summon(hit);
        }
        sim.stats.damage(feedback.damage);
        if feedback.will_drain > 0. {
            sim.stats
                .spend_will(feedback.will_drain.min(sim.stats.will()));
        }
        sim.player.knockback(feedback.impulse);
        if feedback.damage > 0. {
            println!(
                "Guard struck Alice: {} damage, {} sanity",
                feedback.damage,
                sim.stats.sanity()
            );
            hud_ops.push(HudOp::Announce("Card guard struck you".into()));
        }
        audio_step.feedback(feedback);
        if sim.player.script_motion > 0
            || sim.player.ledge.is_some()
            || sim.interactions.level_blocks_weapons()
        {
            step.weapon_click = None;
            sim.weapon_buttons.block();
        }
        if !sim.stats.alive() {
            if sim.alice.die() {
                art_ops.push(ArtOp::ClearAliceEffects);
            }
        }
    } // simulation: combat, encounters and NPC poses
    let death_animation =
        !sim.stats.alive() && focused && !sim.paused && !menu && !inventory_menu && !console_input;
    crate::spike_log::sim(crate::spike_log::Sim::Alice);
    #[cfg(target_os = "vita")]
    if simulate {
        scopes.frame("frame_alice");
        crate::frame_profile::set_owner(crate::frame_profile::Owner::Alice);
    }
    let mut targets = if sim.story.busy() {
        Vec::new()
    } else {
        sim.npcs.targets()
    };
    if !sim.story.busy() {
        if sim.stats.equipped() == Some(2) || sim.alice.croquet_contacts() {
            targets.extend(sim.npcs.mallet_contacts());
        }
        if let Some(t) = sim.interactions.duchess.as_ref().and_then(|d| d.target()) {
            targets.push(t);
        }
        if let Some(summon) = sim.alice.summon_target() {
            targets.push(summon);
        }
        targets.extend(sim.interactions.shot_targets());
        if let Some(s) = &sim.interactions.encounters {
            targets.extend(s.targets());
        }
        if let Some(s) = &sim.interactions.school2 {
            targets.extend(s.targets());
        }
        targets.extend(sim.interactions.levels_targets());
    }
    targets.extend(sim.alice.ice_targets());
    if simulate {
        sim.alice.world_time(dt, world_dt);
        sim.alice.notarget(sim.stats.ignores_alice());
        sync_water(sim, art_ops);
        let weapon_sounds = sim.alice.update_funded(
            if (active && !pand_cinematic) || death_animation {
                dt
            } else {
                0.
            },
            &sim.player,
            sim.input.running(preferences, held!(KeyCode::LeftShift)),
            character::WeaponInput {
                dice: sim.stats.copies(6),
                first_person: !sim.third_person && !sim.flying,
                selected: sim.stats.equipped().unwrap_or(crate::weapons::UNARMED),
                click: step.weapon_click,
                aim: direction,
            },
            &crate::combat::Context {
                world: &sim.world,
                targets: &targets,
            },
            Some(&mut sim.stats),
        );
        if let Some(message) = sim.alice.take_weapon_notice() {
            hud_ops.push(HudOp::Announce(message.into()));
        }
        for mut hit in sim.alice.take_hits() {
            if sim.alice.hit_ice(hit) {
                continue;
            }
            let hit_origin = targets
                .iter()
                .find(|t| t.id == hit.id)
                .map_or(sim.player.eye(), |t| t.center);
            if hit.id == crate::dice::ALICE {
                sim.stats.damage(hit.damage);
                sim.player.knockback(hit.knockback * 2.5);
                continue;
            }
            if hit.id == crate::dice::SUMMON {
                sim.alice.hit_summon(hit);
                continue;
            }
            if hit.kind.is_demon() {
                sim.npcs.provoke_summon(hit.id);
                if let Some(s) = &mut sim.interactions.encounters {
                    s.provoke_summon(hit.id);
                }
                if let Some(s) = &mut sim.interactions.school2 {
                    s.provoke_summon(hit.id);
                }
                sim.interactions.levels_provoke_summon(hit.id);
                if hit.id == crate::duchess::ID {
                    if let Some(d) = &mut sim.interactions.duchess {
                        d.state.opponents.demon = true;
                    }
                }
                hit.kind = hit.kind.means();
            } else {
                hit.damage = sim.stats.attack_damage(hit.damage);
            }
            if hit.id == crate::duchess::ID {
                // A projectile still in flight after Alice dies must not lock retry in a defeat cinematic.
                if !sim.stats.alive() {
                    continue;
                }
                if let Some(sound) = sim
                    .interactions
                    .duchess
                    .as_mut()
                    .and_then(|d| d.hit(hit.damage))
                {
                    audio_step.effect(sound, Some(hit_origin));
                }
                continue;
            }
            // Each registration's exact hit range comes before the `>= encounters::BASE`
            // catch-all below, which would otherwise swallow every registry id.
            if let Some(sound) = sim.interactions.hit_level(hit) {
                if let Some(sound) = sound {
                    audio_step.effect(sound, Some(hit_origin));
                }
                continue;
            }
            if hit.id >= crate::encounters::BASE {
                if let Some(sound) = sim
                    .interactions
                    .encounters
                    .as_mut()
                    .and_then(|s| s.hit(hit))
                {
                    audio_step.effect(sound, Some(hit_origin));
                }
                continue;
            }
            if hit.id >= crate::interaction::SHOT_BASE {
                let e = sim.interactions.shoot(hit);
                if let Some(message) = e.message {
                    hud_ops.push(HudOp::Announce(message.into()));
                }
                if let Some(sound) = e.sound {
                    audio_step.effect(sound, Some(hit_origin));
                }
                sim.interactions.sync(&mut sim.world);
                continue;
            }
            if hit.id >= crate::school2::ENEMY_BASE {
                if let Some(sound) = sim
                    .interactions
                    .school2
                    .as_mut()
                    .and_then(|s| s.hit_attack(hit))
                {
                    audio_step.effect(sound, Some(hit_origin));
                }
                continue;
            }
            if let Some((message, sound)) = sim.npcs.hit(hit) {
                hud_ops.push(HudOp::Announce(message.into()));
                audio_step.effect(sound, Some(hit_origin));
            }
        }
        sim.world.set_weapon_obstacles(sim.alice.ice_targets());
        if active && loot_visit == save::visit_key(&map_name, sim.entry_spawn.as_deref()) {
            let mut after = sim.interactions.loot_sources();
            after.extend(sim.npcs.loot_sources());
            for message in crate::loot::tick_with(
                &mut sim.stats,
                loot_visit,
                world_dt,
                loot_before,
                &after,
                sim.player.feet,
                &sim.world,
                |item| {
                    art_ops.push(ArtOp::PickupCollected {
                        item: item.clone(),
                        clock: sim.pickup_clock,
                    })
                },
            ) {
                hud_ops.push(HudOp::Announce(message.into()));
                audio_step.effect("sound/item/essence/small/mtespu.wav", None);
            }
            if let Some(message) = sim.pickup_feedback.update(
                &sim.stats,
                &sim.pickups,
                loot_visit,
                sim.player.feet,
                &sim.world,
            ) {
                hud_ops.push(HudOp::Announce(message.into()));
            }
        }
        if sim.stats.active_power().map(|(k, _)| k) != power_before {
            if let Some((kind, _)) = sim.stats.active_power() {
                audio_step.effect(
                    match kind {
                        crate::powerups::Kind::Rage => "sound/item/ragebox/ragebox_power.wav",
                        crate::powerups::Kind::Tea => "sound/item/tea/powerup.wav",
                        crate::powerups::Kind::Glass => "sound/item/glass/powerup.wav",
                    },
                    None,
                );
            }
        }
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        {
            scopes.frame("frame_audio");
        }
        crate::spike_log::sim(crate::spike_log::Sim::Audio);
        #[cfg(target_os = "vita")]
        {
            scopes.frame("frame_audio");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::None);
            crate::phase_census::set_owner(crate::phase_census::Owner::Other);
        }
        for sound in weapon_sounds {
            audio_step.effect(sound, None);
        }
        for (path, origin) in sim.alice.take_world_audio() {
            audio_step.effect(path, Some(origin));
        }
        audio_step.world(&sim.interactions, sim.alice.audio_loops());
        for cue in sim.alice.take_audio() {
            audio_step.ops.push(AudioOp::Animation(cue));
        }
        audio_step.ops.push(AudioOp::Player {
            sanity: sim.stats.sanity() as f32,
            swimming: sim.player.swimming,
            underwater: sim.player.immersion.level == 3,
            drowning: sim.player.immersion.level == 3 && sim.player.breath.remaining() <= 0.,
        });
        sim.alice.power_appearance(&sim.stats);
        sync_water(sim, art_ops);
    } // simulation: Alice, weapons and world audio
    let rage_scene = sim
        .interactions
        .beyond
        .as_ref()
        .and_then(|b| b.rage_scene());
    if let Some((time, finished)) = rage_scene.filter(|_| simulate) {
        sim.alice.rage_scene(time, finished, &sim.player);
    }
    scopes.census(crate::phase_census::Region::CameraScene);
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    {
        scopes.frame("frame_camera");
    }
    crate::spike_log::sim(crate::spike_log::Sim::Camera);
    #[cfg(target_os = "vita")]
    {
        scopes.frame("frame_camera");
        crate::frame_profile::set_owner(crate::frame_profile::Owner::Camera);
        crate::phase_census::set_owner(crate::phase_census::Owner::Camera);
    }
    let census_leaf = crate::phase_census::span(crate::phase_census::Region::CameraScripted);
    let mut pand_camera = sim.interactions.camera_after(
        sim.interactions
            .duchess
            .as_ref()
            .and_then(|d| d.camera(&sim.world))
            .or_else(|| {
                sim.interactions
                    .pandemonium
                    .as_ref()
                    .and_then(|p| p.camera())
            })
            .or_else(|| sim.interactions.pool.as_ref().and_then(|p| p.camera()))
            .or_else(|| {
                sim.interactions
                    .school
                    .as_ref()
                    .and_then(|s| s.scene_camera())
            })
            .or_else(|| {
                sim.interactions
                    .school2
                    .as_ref()
                    .and_then(|s| s.scene_camera())
            })
            .or_else(|| {
                sim.interactions
                    .beyond
                    .as_ref()
                    .and_then(|b| b.scene_camera_in(&sim.world))
            })
            .or_else(|| {
                sim.interactions
                    .fortress
                    .as_ref()
                    .and_then(|f| f.cinema.camera(&f.state.cinema))
            })
            .or_else(|| {
                sim.interactions
                    .village
                    .as_ref()
                    .and_then(|v| v.cinema.camera())
            }),
        &sim.world,
    );
    if let Some(camera) = pand_camera.filter(|_| {
        sim.interactions
            .pandemonium
            .as_ref()
            .is_some_and(|p| p.rail_camera_active())
    }) {
        pand_camera = Some(
            sim.rail_camera.rail(
                &sim.world,
                camera,
                &sim.interactions
                    .pandemonium
                    .as_ref()
                    .unwrap()
                    .rail_camera_ahead(),
                if active { dt } else { 0. },
            ),
        );
    } else {
        sim.rail_camera.reset();
    }
    let cinematic_camera = sim
        .interactions
        .school
        .as_ref()
        .and_then(|s| s.return_visit.as_ref())
        .and_then(|s| s.camera());
    let scripted_camera = pand_camera.or_else(|| {
        cinematic_camera.map(|(eye, target, _)| crate::cinematic::Camera::look(eye, target))
    });
    drop(census_leaf);
    let census_leaf = crate::phase_census::span(crate::phase_census::Region::CameraFollow);
    let (eye, aim, gameplay_alice) = (|| {
        #[cfg(target_os = "vita")]
        if let Some(pose) = bench_camera {
            return pose;
        }
        if sim.third_person && !sim.flying {
            sim.follow_camera.update(
                &sim.world,
                sim.player.feet,
                direction,
                preferences.camera_distance,
                if active { dt } else { 0. },
            )
        } else {
            sim.follow_camera.reset();
            (sim.pos, sim.pos + direction, false)
        }
    })();
    drop(census_leaf);
    let census_leaf = crate::phase_census::span(crate::phase_census::Region::CameraHandoff);
    let fade = sim
        .interactions
        .fade_after(
            sim.interactions
                .village
                .as_ref()
                .map(|v| v.cinema.fade())
                .or_else(|| {
                    sim.interactions
                        .fortress
                        .as_ref()
                        .map(|f| f.cinema.fade(&f.state.cinema))
                })
                .or_else(|| {
                    sim.interactions
                        .pandemonium
                        .as_ref()
                        .map(|p| (p.fade_color(), p.fade()))
                })
                .or_else(|| sim.interactions.pool.as_ref().map(|p| p.fade()))
                .or_else(|| sim.interactions.beyond.as_ref().map(|b| b.scene_fade()))
                .or_else(|| sim.interactions.school.as_ref().map(|s| s.scene_fade()))
                .or_else(|| sim.interactions.school2.as_ref().map(|s| s.scene_fade())),
        )
        .map_or(Color::new(0., 0., 0., 0.), |(c, a)| Color { a, ..c });
    if scripted_camera.is_some() {
        sim.camera_handoff.return_duration(
            if sim
                .interactions
                .pool
                .as_ref()
                .is_some_and(|p| p.scene_id() == Some(crate::pool::cinema::ENTRY))
            {
                2.
            } else {
                0.45
            },
        );
    }
    let view = (|| {
        #[cfg(target_os = "vita")]
        if let Some((eye, target, _)) = bench_camera {
            return crate::cinematic::Camera::look(eye, target);
        }
        sim.camera_handoff.update(
            &sim.world,
            scripted_camera,
            crate::cinematic::Camera::look(eye, aim),
            fade,
            if active { dt } else { 0. },
        )
    })();
    let quake = sim
        .interactions
        .pool
        .as_ref()
        .map_or(Vec3::ZERO, |p| p.quake_offset())
        + sim
            .interactions
            .levels
            .iter()
            .map(|s| s.ctl.quake_offset())
            .sum::<Vec3>();
    let (camera, target) = (view.eye + quake, view.target + quake);
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    let (camera, target) = crate::sim_maps::probe::moving_view(frame, camera, target);
    #[cfg(target_os = "vita")]
    let (camera, target) = bench_camera.map_or((camera, target), |(eye, target, _)| (eye, target));
    let show_alice = !sim.interactions.level_hides_player()
        && if scripted_camera.is_some() {
            rage_scene.is_some_and(|(_, finished)| !finished)
                || (cinematic_camera.is_some_and(|(_, _, show)| show) && pand_camera.is_none())
        } else {
            gameplay_alice && camera.distance(sim.player.feet + Vec3::Z * 32.) > 30.
        };
    drop(census_leaf);
    if simulate {
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        {
            scopes.frame("frame_audio_mix");
        }
        crate::spike_log::sim(crate::spike_log::Sim::AudioMix);
        #[cfg(target_os = "vita")]
        {
            scopes.frame("frame_audio_mix");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::None);
            crate::phase_census::set_owner(crate::phase_census::Owner::Other);
        }
        let listen_direction = target - camera;
        audio_step.ops.push(AudioOp::Listen {
            listener: camera,
            submerged: sim.world.liquid_at(camera) != 0,
            dt: elapsed,
            world_dt,
            yaw: listen_direction.y.atan2(listen_direction.x),
            paused: console_input || menu || sim.paused || inventory_menu || !focused,
            voice_paused: !story_active,
        });
    } // simulation: audio listener and clocks
    let presentation = crate::presentation_input::PresentationInput::capture(
        &sim.world,
        camera,
        &sim.interactions,
    );
    let camera_out = CameraOut {
        position: camera,
        target,
        up: pand_camera.map_or(Vec3::Z, |c| c.up),
        fovy: sim
            .interactions
            .beyond
            .as_ref()
            .and_then(|b| b.scene_fovy(aspect))
            .or_else(|| {
                sim.interactions
                    .levels
                    .iter()
                    .find_map(|s| s.ctl.scene_fovy(aspect))
            })
            .unwrap_or(75.0f32.to_radians()),
    };
    let mut transforms = sim.interactions.transforms();
    transforms.extend(sim.world.traversal.transforms(&sim.player));
    let dynamic = crate::render_snapshot::DynamicInput {
        alice_feet: sim.player.feet,
        alice_visible: show_alice
            && !sim
                .interactions
                .school
                .as_ref()
                .is_some_and(|s| s.cinematic()),
        alice_shadow: show_alice && sim.stats.invisible <= 0.,
        alice_ghost: sim.stats.invisible > 0.,
        alice_effects: show_alice,
        environment_clock: sim.environment_clock,
        pickup_clock: sim.pickup_clock,
        transforms,
        ..Default::default()
    };
    let view = StepView {
        stamp,
        presentation,
        camera_out,
        dynamic,
        active,
        direction,
        targets,
        pand_cinematic,
        scripted_camera,
        show_alice,
        #[cfg(target_os = "vita")]
        gameplay_alice,
    };
    (job, view)
}

fn sync_water(sim: &mut SimState, art_ops: &mut Vec<ArtOp>) {
    if sim.alice.water_appearance(&sim.player) {
        art_ops.push(ArtOp::ResetWaterEmitter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{level::LevelController, story::Story};
    use std::any::Any;

    struct Checkpoint {
        pending: bool,
        ending: bool,
    }

    impl LevelController for Checkpoint {
        fn id(&self) -> &'static str {
            "checkpoint-fixture"
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
        fn checkpoint_requested(&self) -> bool {
            self.pending
        }
        fn checkpoint_written(&mut self) {
            self.pending = false;
        }
        fn ending_ready(&self) -> bool {
            self.ending && !self.pending
        }
    }

    fn fixture(ending: bool) -> Interactions {
        let mut interactions = Interactions::empty();
        interactions.levels.push(crate::level::Slot {
            reg: &crate::levels::qlair::REGISTRATION,
            ctl: Box::new(Checkpoint {
                pending: true,
                ending,
            }),
        });
        interactions
    }

    #[test]
    fn step_due_checkpoint_ack_and_ending_precede_story_exit() {
        let mut interactions = fixture(true);
        let mut story = Story::default();
        let mut pending = None;
        let exit = ("song".to_owned(), Some("story-spawn".to_owned()));
        story.defer_exit(exit.clone());

        assert!(matches!(
            step_due(
                true,
                true,
                false,
                &mut interactions,
                &mut story,
                &mut pending,
                false
            ),
            Due::Checkpoint
        ));
        assert_eq!(story.pending_exit_map(), Some("song"));
        super::super::acknowledge_checkpoint(Ok(()), &mut interactions.levels).unwrap();
        assert!(matches!(
            step_due(
                false,
                true,
                false,
                &mut interactions,
                &mut story,
                &mut pending,
                false
            ),
            Due::Ending
        ));
        assert_eq!(story.pending_exit_map(), Some("song"));
        assert!(
            matches!(step_due(false, true, true, &mut interactions, &mut story, &mut pending, false), Due::Exit(out) if out == exit)
        );
        assert!(story.pending_exit_map().is_none());
    }

    #[test]
    fn step_due_failed_ack_keeps_checkpoint_pending_and_host_override_wins() {
        let mut interactions = fixture(false);
        let mut story = Story::default();
        let mut pending = None;
        story.defer_exit(("story-only".into(), None));

        assert!(super::super::acknowledge_checkpoint(
            Err(anyhow::anyhow!("write failed")),
            &mut interactions.levels
        )
        .is_err());
        assert!(matches!(
            step_due(
                true,
                true,
                false,
                &mut interactions,
                &mut story,
                &mut pending,
                false
            ),
            Due::Checkpoint
        ));
        assert!(
            matches!(step_due(false, true, false, &mut interactions, &mut story, &mut pending, true),
            Due::Exit((map, None)) if map == crate::transition_memory::host_destination())
        );
        assert!(story.pending_exit_map().is_none());
        assert!(interactions.levels[0].ctl.checkpoint_requested());
    }

    #[test]
    fn sim_state_size() {
        println!(
            "SimState {} bytes (boxed at the serial adapter)",
            std::mem::size_of::<SimState>()
        );
    }
}
