use super::{LevelArt, LevelDispatch, SnapshotTiming};
use crate::{
    character::AliceArt,
    hud::Hud,
    lighting::{Selection, ShadowInput},
    npc::NpcArt,
    particles::SteamArt,
    powerups::{self, PickupInput},
    render::Scene,
    render_snapshot::{DynamicInput, HandoffReturn, WaveInput},
    targeting,
};
use macroquad::camera::{Camera, Camera3D};

pub(super) struct FreeRender<'a> {
    pub(super) scene: &'a mut Scene,
    pub(super) level_art: &'a mut LevelArt,
    pub(super) alice: &'a mut AliceArt,
    pub(super) npcs: &'a mut NpcArt,
    pub(super) steam: &'a SteamArt,
    pub(super) power_art: &'a mut powerups::Art,
    pub(super) aim_pointer: &'a mut targeting::Pointer,
    pub(super) hud: &'a Hud,
    pub(super) handoff_return: &'a mut HandoffReturn,
    pub(super) overlap_probe: &'a mut crate::overlap_probe::Probe,
    pub(super) world_camera: &'a Camera3D,
    pub(super) dynamic: &'a DynamicInput,
    pub(super) level: &'a LevelDispatch,
    pub(super) pickups: &'a PickupInput,
    pub(super) waves: &'a [WaveInput],
    pub(super) aim: &'a targeting::Input,
    pub(super) shadow: Option<ShadowInput>,
    pub(super) lights: Selection,
    pub(super) first_person: bool,
    pub(super) empty_level_effects: bool,
    pub(super) fullbright: bool,
    pub(super) draw_world: bool,
    pub(super) world_only: bool,
    pub(super) aim_dt: f32,
    pub(super) frame: u64,
    pub(super) handoff_minimum_frame: u64,
    pub(super) snapshot_log: bool,
}

pub(super) struct RenderScopes {
    pub(super) census_block: crate::phase_census::Timer,
    #[cfg(target_os = "vita")]
    pub(super) viewer_block: crate::frame_profile::Span,
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    pub(super) host_block: crate::frame_profile::Span,
}

impl FreeRender<'_> {
    pub(super) fn draw(
        self,
        mut census_block: crate::phase_census::Timer,
        #[cfg(target_os = "vita")] mut viewer_block: crate::frame_profile::Span,
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        mut host_block: crate::frame_profile::Span,
    ) -> RenderScopes {
        let Self {
            scene,
            level_art,
            alice,
            npcs,
            steam,
            power_art,
            aim_pointer,
            hud,
            handoff_return,
            overlap_probe,
            world_camera,
            dynamic,
            level,
            pickups,
            waves,
            aim,
            shadow,
            lights,
            first_person,
            empty_level_effects,
            fullbright,
            draw_world,
            world_only,
            aim_dt,
            frame,
            handoff_minimum_frame,
            snapshot_log,
        } = self;
        let camera = world_camera.position;
        let direction = (world_camera.target - camera).normalize_or_zero();
        let environment_clock = dynamic.environment_clock;
        let transforms = &dynamic.transforms;
        if level_art.pool.is_some() {
            handoff_return.expect(&level_art.snapshot_visit, 4, frame);
        }
        crate::render_snapshot::handoff_render_begin(&level_art.snapshot_visit, frame);
        crate::spike_log::render_begin();
        overlap_probe.begin();
        {
            #[cfg(target_os = "vita")]
            let _hotspot = crate::frame_profile::span("lights_select");
            crate::lighting::publish(lights);
        }
        if !world_only || !scene.retained_active() {
            scene.begin_fx_view(world_camera, environment_clock, fullbright, true);
        } else {
            use macroquad::camera::Camera;
            crate::render_fx::set_projection(world_camera.matrix());
        }
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        let host_gather = std::time::Instant::now();
        #[cfg(feature = "vita_renderer")]
        {
            drop(census_block);
            census_block = crate::phase_census::span(crate::phase_census::Region::Other);
        }
        #[cfg(feature = "vita_renderer")]
        let census_gather = crate::phase_census::span(crate::phase_census::Region::ActorGather);
        #[cfg(feature = "vita_renderer")]
        let mut skin_frame = if crate::render::vita_renderer::config().skin_overlap && !world_only {
            #[cfg(target_os = "vita")]
            {
                drop(viewer_block);
                viewer_block = crate::frame_profile::span("skin_gather");
                crate::frame_profile::set_owner(crate::frame_profile::Owner::Characters);
            }
            let snapshot_gather_start = snapshot_log.then(std::time::Instant::now);
            let gather_clock = crate::pass_clock::scope(crate::pass_clock::ACTOR_GATHER);
            let census_leaf =
                crate::phase_census::span(crate::phase_census::Region::GatherStoryPose);

            drop(census_leaf);
            let census_leaf =
                crate::phase_census::span(crate::phase_census::Region::GatherFrameBegin);
            crate::audio::game_stage(3);
            let frame_index_for_snapshot = frame;
            let mut frame = crate::character::skinning::overlap::Frame::begin();
            drop(census_leaf);
            let census_leaf = crate::phase_census::span(crate::phase_census::Region::GatherAlice);
            if crate::character::skinning::overlap::single_frame() {
                if draw_world && dynamic.alice_shadow {
                    crate::lighting::draw_shadow_input(&shadow);
                }
                alice.atmosphere(&scene.atmosphere, camera);
            }
            if dynamic.alice_visible {
                alice.draw_captured(fullbright, dynamic.alice_ghost);
            }
            drop(census_leaf);
            let census_leaf = crate::phase_census::span(crate::phase_census::Region::GatherNpcs);
            npcs.draw_captured(camera, direction, &scene.atmosphere, fullbright);
            drop(census_leaf);
            let census_leaf =
                crate::phase_census::span(crate::phase_census::Region::GatherLevelArt);
            level_art.draw_captured(level, &scene.atmosphere, camera, fullbright);
            drop(census_leaf);
            let census_leaf =
                crate::phase_census::span(crate::phase_census::Region::GatherDispatch);
            if let Some(start) = snapshot_gather_start {
                let (bytes, draws, bones, surfaces) = frame.snapshot_census();
                println!("[lg/snapshot-skin] frame={} bytes={} draws={} bones={} surfaces={} gather_us={:.3}",
                    frame_index_for_snapshot, bytes, draws, bones, surfaces, start.elapsed().as_secs_f64()*1_000_000.);
            }
            frame.dispatch();
            drop(census_leaf);
            drop(gather_clock);
            crate::audio::game_stage(0);
            Some(frame)
        } else {
            None
        };
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        {
            drop(host_block);
            host_block = crate::frame_profile::span("frame_world");
        }
        #[cfg(target_os = "vita")]
        {
            drop(viewer_block);
            viewer_block = crate::frame_profile::span("frame_world");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::None);
            crate::phase_census::set_owner(crate::phase_census::Owner::Other);
        }
        #[cfg(feature = "vita_renderer")]
        drop(census_gather);
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        crate::sim_maps::probe::span("actor_gather", host_gather.elapsed().as_secs_f64() * 1000.);
        scene.draw(camera, environment_clock, fullbright, false, transforms);
        #[cfg(feature = "vita_renderer")]
        if let Some(frame) = &mut skin_frame {
            let _collect = crate::frame_profile::span("skin_collect");
            crate::audio::game_stage(2);
            let _census = crate::phase_census::span(crate::phase_census::Region::ActorCollect);
            let _clock = crate::pass_clock::scope(crate::pass_clock::ACTOR_COLLECT);
            frame.collect();
            crate::audio::game_stage(0);
        }
        drop(census_block);
        census_block = crate::phase_census::span(crate::phase_census::Region::ActorPublication);
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        {
            drop(host_block);
            host_block = crate::frame_profile::span("frame_characters");
        }
        #[cfg(target_os = "vita")]
        {
            drop(viewer_block);
            viewer_block = crate::frame_profile::span("frame_characters");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::Characters);
        }
        #[cfg(not(target_os = "vita"))]
        let host_characters_profile = crate::frame_profile::span("frame_characters");
        let actors_timer =
            crate::light_profile::render_timer(crate::light_profile::RenderPhase::Actors);
        if !world_only && !crate::character::skinning::overlap::single_frame() {
            {
                let _characters_profile = crate::frame_profile::span("characters_setup");
                if draw_world && dynamic.alice_shadow {
                    crate::lighting::draw_shadow_input(&shadow);
                }
                alice.atmosphere(&scene.atmosphere, camera);
            }
            {
                let _characters_profile = crate::frame_profile::span("characters_alice");
                if dynamic.alice_visible {
                    alice.draw_captured(fullbright, dynamic.alice_ghost);
                }
            }
            {
                let _characters_profile = crate::frame_profile::span("characters_npcs");
                npcs.draw_captured(camera, direction, &scene.atmosphere, fullbright);
            }
            {
                let _characters_profile = crate::frame_profile::span("characters_level_art");

                level_art.draw_captured(level, &scene.atmosphere, camera, fullbright);
            }
        } // world-only diagnostic mask includes all character work
        #[cfg(feature = "vita_renderer")]
        drop(skin_frame);
        if level_art.pool.is_some() {
            let timing = snapshot_log.then(SnapshotTiming::start);
            let completion = level_art.handoff_completion(frame, handoff_minimum_frame);
            assert!(handoff_return.publish(completion));
            if let Some(timing) = timing {
                let (us, allocations) = timing.finish();
                println!(
                    "[lg/snapshot-handoff] frame={} bytes={} extraction_us={:.3} allocations={}",
                    frame,
                    handoff_return.bytes(),
                    us,
                    allocations
                );
            }
        }
        crate::render_snapshot::handoff_render_end();
        #[cfg(not(target_os = "vita"))]
        drop(host_characters_profile);
        drop(actors_timer);
        drop(census_block);
        census_block = crate::phase_census::span(crate::phase_census::Region::RenderSetup);
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        {
            drop(host_block);
            host_block = crate::frame_profile::span("frame_objects");
        }
        #[cfg(target_os = "vita")]
        {
            drop(viewer_block);
            viewer_block = crate::frame_profile::span("frame_objects");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::None);
            crate::phase_census::set_owner(crate::phase_census::Owner::Other);
        }
        if draw_world && !world_only {
            power_art.draw_input(pickups, &scene.atmosphere, camera);
        }
        drop(census_block);
        census_block = crate::phase_census::span(crate::phase_census::Region::Fx);
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        {
            drop(host_block);
            host_block = crate::frame_profile::span("frame_world_particles");
        }
        #[cfg(target_os = "vita")]
        {
            drop(viewer_block);
            viewer_block = crate::frame_profile::span("frame_world_particles");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::None);
            crate::phase_census::set_owner(crate::phase_census::Owner::Other);
        }
        {
            let particles = steam
                .draw_input(level.visit)
                .expect("validated steam input");
            scene.draw_particle_frame(
                camera,
                direction,
                environment_clock,
                fullbright,
                transforms,
                particles,
            );
            #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
            {
                drop(host_block);
                host_block = crate::frame_profile::span("frame_effects");
            }
            #[cfg(target_os = "vita")]
            {
                drop(viewer_block);
                viewer_block = crate::frame_profile::span("frame_effects");
                crate::frame_profile::set_owner(crate::frame_profile::Owner::None);
                crate::phase_census::set_owner(crate::phase_census::Owner::Other);
            }
            if !world_only {
                for wave in waves {
                    wave.draw();
                }
                if !empty_level_effects {
                    level_art.captured_effects(level, camera, &scene.atmosphere);
                }
                alice.draw_captured_effects(camera, fullbright, dynamic.alice_effects);
                aim_pointer.draw_input(aim, level.visit, aim_dt, camera, &scene.atmosphere);
                hud.draw_pickup_markers(&pickups.markers, dynamic.pickup_clock);
            }
        }
        if draw_world && !world_only {
            crate::lighting::flares(camera, direction);
        }
        let direct_fx = crate::render_fx::retained_active();
        if !world_only || !scene.retained_active() {
            scene.finish_fx(true);
        }
        drop(census_block);
        census_block = crate::phase_census::span(crate::phase_census::Region::FirstPerson);
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        {
            drop(host_block);
            host_block = crate::frame_profile::span("frame_first_person");
        }
        #[cfg(target_os = "vita")]
        {
            drop(viewer_block);
            viewer_block = crate::frame_profile::span("frame_first_person");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::None);
            crate::phase_census::set_owner(crate::phase_census::Owner::Other);
        }
        if first_person {
            if direct_fx {
                // All 3-D packets have reached miniquad; the view toy needs only a depth clear.
                crate::render::clear_direct_view_depth();
                scene.begin_fx_view(world_camera, environment_clock, fullbright, false);
                alice.draw_captured_first_person(fullbright);
                scene.finish_fx(false);
            } else {
                crate::render::clear_view_depth();
                alice.draw_captured_first_person(fullbright);
            }
        }
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        {
            drop(host_block);
            host_block = crate::frame_profile::span("frame_capture");
        }
        #[cfg(target_os = "vita")]
        {
            drop(viewer_block);
            viewer_block = crate::frame_profile::span("frame_capture");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::None);
            crate::phase_census::set_owner(crate::phase_census::Owner::Other);
        }
        RenderScopes {
            census_block,
            #[cfg(target_os = "vita")]
            viewer_block,
            #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
            host_block,
        }
    }
}
