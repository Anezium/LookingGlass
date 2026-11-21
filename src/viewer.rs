use crate::{
    hud::HudOp,
    hud_view::HudView,
    assets::Assets,
    audio::{step::{AudioOp, AudioStep, VoiceRequest}, Audio},
    character::{self, Character},
    collision::World,
    hud::Hud,
    interaction::{self, Interactions},
    inventory::{self, Catalog, Stats},
    look::{window_focused, MouseLook},
    movement::{FixedClock, Player, EYE_HEIGHT},
    recovery::{self, Recovery},
    Options,
};
use anyhow::{Context, Result};
use macroquad::prelude::*;
// Recorded or replayed input when a tape is active; otherwise the macroquad calls.
use crate::replay::input::{
    get_char_pressed, get_time, is_key_pressed, is_quit_requested,
    source_frame_time, ClockView, StepGuard,
};

#[cfg(target_os = "vita")]
#[link(name = "SceProcessmgr_stub")]
extern "C" {
    fn sceKernelPowerTick(tick_type: i32) -> i32;
}
#[cfg(target_os = "vita")]
const SCE_KERNEL_POWER_TICK_DEFAULT: i32 = 0;

#[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
pub(crate) fn probe_window_request() -> (i32, i32, bool) { (960, 544, false) }

pub fn config() -> macroquad::conf::Conf {
    eprintln!("[lg/startup] window_configuration begin");
    #[cfg(target_os = "vita")]
    let (width, height, high_dpi) = (960, 544, false);
    #[cfg(not(target_os = "vita"))]
    let (width, height, high_dpi) = (1200, 680, true);
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    let (width, height, high_dpi) = if crate::sim_maps::probe::enabled() { probe_window_request() } else { (width, height, high_dpi) };
    macroquad::conf::Conf {
        miniquad_conf: Conf {
            window_title: "Looking Glass".into(),
            window_width: width,
            window_height: height,
            high_dpi,
            #[cfg(feature="vita_renderer")]
            sample_count: crate::render::vita_renderer::config().msaa_samples,
            #[cfg(not(feature="vita_renderer"))]
            sample_count: 1,
            ..Default::default()
        },
        draw_call_vertex_capacity: 60000,
        draw_call_index_capacity: 180000,
        ..Default::default()
    }
}

use crate::render::{Loaded, Scene};
use crate::save::{self, Campaign, Game, Level, Slot, Store, View};
fn load_scene(
    assets: &mut Assets,
    name: &str,
    difficulty: crate::powerups::Difficulty,
) -> Result<Loaded> {
    Scene::load_for(assets, name, difficulty)
}
struct Entered {
    art: LevelArt,
    scene: Scene,
    world: World,
    interactions: Interactions,
    story: crate::story::Story,
    hints: crate::cheshire::Hints,
    npcs: crate::npc::Npcs,
    steam: crate::particles::Steam,
    environment_clock: f32,
    pickup_clock: f32,
}
/// Art owns GPU resources and may contain map-specific geometry. Replace this
/// whole group on every kind of level change, including the Tab/console chooser.
#[derive(Default)]
struct LevelArt {
    snapshot_visit: crate::render_snapshot::RenderVisit,
    fortress: Option<crate::fortress::cinema::Art>,
    village: Option<crate::village::cinema::Art>,
    school: Option<crate::school::Art>,
    gym: Option<crate::gym::Art>,
    beyond: Option<crate::beyond::Art>,
    pool: Option<crate::pool::Art>,
    school2: Option<crate::school2::Art>,
    duchess: Option<crate::duchess::Art>,
    pandemonium: Option<crate::pandemonium::Art>,
    encounters: Option<crate::encounters::Art>,
    cheshire: Option<crate::cheshire::Art>,
    /// Registered controllers' art, aligned with `Interactions::levels` (`None`: no art).
    levels: Vec<Option<Box<dyn crate::level::LevelArt>>>,
}

/// Ordered resource slots follow LevelArt, never controller addresses. An
/// incomplete selected family still dispatches live under its whole-frame fence.
struct LevelDispatch {
    visit: u64,
    selected: [bool; 10],
    registered: Vec<bool>,
    school_staging: bool,
    cheshire: Option<crate::cheshire::Input>,
    overflow: bool,
}
const _: fn() = || {
    fn send<T: Send>() {}
    send::<Interactions>();
    send::<crate::story::Story>();
    send::<crate::cheshire::Hints>();
    send::<crate::input::Input>();
    send::<Recovery>();
    send::<recovery::RetryPoint>();
    send::<crate::cinematic::Skip>();
    send::<crate::cinematic::Camera>();
    send::<crate::camera::Follow>();
    send::<crate::camera::Handoff>();
    send::<Player>();
    send::<Stats>();
    send::<FixedClock>();
    send::<Campaign>();
    send::<Vec<inventory::Pickup>>();
    send::<crate::replay::OwnedProbe>();
    send::<ClockView>();
    send::<crate::lighting::Selection>();
    send::<LevelDispatch>();
    send::<crate::render_snapshot::HudInput>();
    send::<crate::render_snapshot::TailInput>();
    send::<crate::render_snapshot::DynamicInput>();
    send::<crate::render_snapshot::ParticleInput>();
    send::<crate::render_snapshot::WaveInput>();
    send::<crate::cheshire::Input>();
    send::<crate::targeting::Input>();
    send::<crate::powerups::PickupInput>();
    send::<std::sync::Arc<dyn crate::level::LevelCapture>>();
    send::<Box<dyn std::any::Any + Send>>();
};

struct HudSources {
    duchess: Option<u64>,
    registered: Vec<(usize, crate::level::HudSource)>,
}

impl HudSources {
    fn capture(
        &self,
        interactions: &Interactions,
        view: crate::level::HudCaptureView,
    ) -> Vec<crate::render_snapshot::HudCommand> {
        let mut commands = Vec::new();
        if let (Some(resource), Some(duchess)) = (self.duchess, &interactions.duchess) {
            let mut painter = crate::render_snapshot::Painter::for_resource(resource, &mut commands);
            crate::duchess::capture_hud(duchess, view, &mut painter);
        }
        for &(index, source) in &self.registered {
            if let Some(slot) = interactions.levels.get(index) {
                source.capture(&*slot.ctl, view, &mut commands);
            }
        }
        commands
    }
}
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<HudSources>();
};

impl LevelDispatch {
    fn valid(&self, art: &LevelArt) -> bool {
        !self.overflow && self.visit == art.snapshot_visit.id()
            && self.registered.len() == art.levels.len()
            && self.cheshire.is_some() == art.cheshire.is_some()
            && self.cheshire.as_ref().is_none_or(|i| art.cheshire.as_ref()
                .is_some_and(|a| a.valid_input(i, self.visit)))
    }
}

type RebuiltScene = (LevelArt, crate::npc::Npcs, crate::particles::Steam, Loaded, Option<Character>);

/// Allocate recovery state only after a failed transition. Keeping these awaits
/// inside `run` reserves their live checkpoint/temporaries in every viewer frame.
async fn rebuild_outgoing(
    assets: &mut Assets,
    checkpoint: &Game,
    interactions: &Interactions,
    difficulty: crate::powerups::Difficulty,
    options: &Options,
    reload_alice: bool,
    escape_menu: &mut crate::menu::Menu,
    ui: &crate::ui::Ui,
    preferences: &crate::preferences::Preferences,
) -> Option<Box<RebuiltScene>> {
    let mut attempts = 0;
    loop {
        // Nothing from a failed load outlives its window into this retirement.
        crate::texture::release_shared();
        crate::render_fx::release_scene();
        crate::transition_memory::released().await;
        // Partial resources drop together on error, before the next retirement; the
        // load guard drops this attempt's cache with them, on success and error alike.
        let attempt = {
            let _share = crate::texture::load_scope();
            (|| -> Result<_> {
                crate::transition_memory::recovery_fault()?;
                let level = checkpoint.level()?;
                let Loaded { mut scene, mut world } = Scene::load(assets, &level.map)?;
                // The recovered scene still owns the only map handle before cast loading.
                scene.map_mut().difficulty = difficulty;
                interactions.sync(&mut world);
                let mut cast = crate::npc::Npcs::load(assets, &scene.map, &level.map,
                    level.entry.as_deref(), options.npc_preview, options.combat_preview)?;
                cast.restore(&level.npcs)?;
                let particles = crate::particles::Steam::load(assets, &scene.map)?;
                let mut art = LevelArt::default();
                art.prepare(assets, &scene, interactions, true)?;
                let character = if reload_alice {
                    let mut character = Character::load(assets)?;
                    character.restore(&checkpoint.character)?;
                    Some(character)
                } else { None };
                Ok(Box::new((art, cast, particles, Loaded { scene, world }, character)))
            })()
        };
        // A successful rebuild is a live map now; a failed one is freed before the
        // next attempt. Either way this attempt leaves no cache owners behind.
        crate::texture::release_shared();
        match attempt {
            Ok(resources) => return Some(resources),
            Err(error) => {
                attempts += 1;
                eprintln!("[lg/transition-recovery] outgoing rebuild failed attempt={attempts}: {error:#}");
                if attempts == 1 { continue; }
                // A persistent failure retains the untouched live checkpoint;
                // quitting returns directly, without the normal Auto-save path.
                let mut recovery_input = crate::input::Input::default();
                #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                let mut screen_frame = 0;
                loop {
                    escape_menu.backdrop();
                    ui.toast("Could not restore the current map", screen_height() - 110.);
                    ui.toast(&crate::prompts::shown("Enter / Cross: retry    Escape / Circle: quit", &HudView::capture(&recovery_input), preferences), screen_height() - 70.);
                    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                    crate::transition_memory::host_recovery_capture(attempts, screen_frame + 1);
                    next_frame().await;
                    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                    { screen_frame += 1; crate::transition_memory::host_recovery_screen(attempts, screen_frame); }
                    recovery_input.update(preferences, window_focused());
                    if recovery_input.ui(KeyCode::Escape) || is_quit_requested() { return None; }
                    if recovery_input.ui(KeyCode::Enter) { break; }
                }
            }
        }
    }
}

impl LevelArt {
    fn handoff_source(&self) -> Option<(usize, &crate::cinematic::ActorPose)> {
        self.village.as_ref().and_then(|a| a.alice.handoff_pose()).map(|p| (0, p))
            .or_else(|| self.fortress.as_ref().and_then(|a| a.alice.handoff_pose()).map(|p| (1, p)))
            .or_else(|| self.pandemonium.as_ref().and_then(|a| a.alice.handoff_pose()).map(|p| (2, p)))
            .or_else(|| self.duchess.as_ref().and_then(|a| a.alice.handoff_pose()).map(|p| (3, p)))
            .or_else(|| self.pool.as_ref().and_then(|a| a.cinema.alice.handoff_pose()).map(|p| (4, p)))
            .or_else(|| self.school.as_ref().and_then(|a| a.returning.alice.handoff_pose()).map(|p| (5, p)))
            .or_else(|| self.levels.iter().enumerate().find_map(|(slot, a)|
                a.as_ref().and_then(|a| a.handoff_pose()).map(|p| (6 + slot, p))))
    }
    fn handoff_pose(&self) -> Option<&crate::cinematic::ActorPose> {
        self.handoff_source().map(|(_, pose)| pose)
    }
    fn handoff_completion(&self, frame: u64, minimum_frame: u64) -> crate::render_snapshot::HandoffCompletion {
        let pose = self.pool.as_ref().and_then(|art| art.cinema.alice.handoff_pose()).filter(|p|
            p.rendered.is_some_and(|(visit, drawn)| visit == self.snapshot_visit.id() && drawn >= minimum_frame));
        crate::render_snapshot::HandoffCompletion::rendered(&self.snapshot_visit, 4, frame, pose)
    }
    fn prepare(
        &mut self,
        assets: &mut Assets,
        scene: &Scene,
        i: &Interactions,
        cat: bool,
    ) -> Result<()> {
        if i.fortress.is_some() && self.fortress.is_none() {
            self.fortress = Some(crate::fortress::cinema::Art::load(assets)?);
        }
        let mut timing = crate::startup::Phase::new("level_art");
        if cat && self.cheshire.is_none() {
            self.cheshire = Some(crate::cheshire::Art::load(assets)?);
        }
        timing.mark("cheshire");
        if i.school.is_some() && self.school.is_none() {
            let specs = crate::texture::read_materials(assets)?;
            self.school = Some(crate::school::Art::load(assets, &specs)?);
        }
        if i.beyond.is_some() && self.beyond.is_none() {
            self.beyond = Some(crate::beyond::Art::load(assets)?);
        }
        if let Some(p) = &i.pool {
            if self.pool.is_none() {
                self.pool = Some(crate::pool::Art::load(assets, p)?);
            }
        }
        if i.gym.is_some() && self.gym.is_none() {
            self.gym = Some(crate::gym::Art::load(assets)?);
        }
        if let Some(s) = &i.school2 {
            if self.school2.is_none() {
                self.school2 = Some(crate::school2::Art::load(assets, s)?);
            }
        }
        if i.duchess.is_some() && self.duchess.is_none() {
            self.duchess = Some(crate::duchess::Art::load(assets, &scene.map)?);
        }
        if i.village.is_some() && self.village.is_none() {
            self.village = Some(crate::village::cinema::Art::load(assets)?);
        }
        if i.pandemonium.is_some() && self.pandemonium.is_none() {
            self.pandemonium = Some(crate::pandemonium::Art::load(assets)?);
        }
        timing.mark("destination_cast");
        if i.encounters.is_some() && self.encounters.is_none() {
            self.encounters = Some(crate::encounters::Art::load(assets)?);
        }
        timing.mark("encounters");
        if self.levels.is_empty() {
            for slot in &i.levels {
                self.levels.push(
                    slot.reg
                        .art
                        .map(|load| load(assets, &scene.map, &*slot.ctl))
                        .transpose()?,
                );
            }
        }
        Ok(())
    }
    fn extract_dispatch(&mut self, i: &Interactions, hints: &crate::cheshire::Hints,
        story: &crate::story::Story) -> LevelDispatch {
        // Check both shapes before walking or allocating the registry. A rejected
        // command is fixed size and can only select the serial fenced fallback.
        if self.levels.len() > 1024 || self.levels.len() != i.levels.len() {
            for a in self.levels.iter_mut().take(1024).flatten() {a.clear_snapshot();}
            return LevelDispatch { visit: self.snapshot_visit.id(), selected: [false; 10],
                registered: Vec::new(), school_staging: false, cheshire: None, overflow: true };
        }
        match (&mut self.encounters, &i.encounters) {
            (Some(a), Some(s)) => a.extract_snapshot(s),
            (Some(a), None) => a.clear_snapshot(),
            _ => {}
        }
        for (art, slot) in self.levels.iter_mut().zip(&i.levels) {
            if let Some(a)=art {
                // A capture source reads the controller and story numbers only; the art
                // then consumes the plain value it returned.
                if let Some(source)=a.capture_source() {
                    let pose=source.story(story);
                    a.publish_snapshot(source.capture(&*slot.ctl,&pose));
                } else { a.extract_snapshot(&*slot.ctl,story); }
            }
        }
        LevelDispatch { visit: self.snapshot_visit.id(),
            selected: [i.fortress.is_some(), i.school.is_some(), i.village.is_some(),
                i.pandemonium.is_some(), i.beyond.is_some(), i.pool.is_some(), i.gym.is_some(),
                i.duchess.is_some(), i.school2.is_some(), i.encounters.is_some()],
            registered: self.levels.iter().map(Option::is_some).collect(),
            school_staging: i.school.as_ref().is_some_and(|s| s.scene_id() == Some(crate::school::cinema::THEATRE)),
            cheshire: self.cheshire.as_ref().map(|art| {
                hints.capture(&art.capture_rig(), story, self.snapshot_visit.id())
            }),
            overflow: self.levels.len() > 1024 || self.levels.len() != i.levels.len(),
        }
    }
    fn story_pose(&mut self, story: &crate::story::Story) { self.story_pose_dispatch(story, false); }
    fn story_pose_dispatch(&mut self, story: &crate::story::Story, owned: bool) {
        if let Some(a) = &mut self.school {
            a.first.story_pose(story);
        }
        if !owned { if let Some(a) = &mut self.pool { a.cinema.story_pose(story); } }
        if let Some(a) = &mut self.fortress {
            a.story_pose(story);
        }
        if let Some(a) = &mut self.village {
            a.story_pose(story);
        }
        if let Some(art) = &mut self.cheshire { art.story_pose(story); }
        if let Some(art) = &mut self.pandemonium {
            art.story_pose(story);
        }
        if let Some(art) = &mut self.duchess {
            art.story_pose(story);
        }
        if let Some(art) = &mut self.school2 {
            art.story_pose(story);
        }
        for art in self.levels.iter_mut().flatten() {
            if !owned || !art.snapshot_ready() { art.story_pose(story); }
        }
    }
    fn draw(&mut self, i: &Interactions, hints: &crate::cheshire::Hints,
        atmosphere: &crate::environment::Atmosphere, camera: Vec3, fullbright: bool) {
        self.draw_dispatch(i, None, hints, atmosphere, camera, fullbright);
    }
    fn draw_dispatch(
        &mut self,
        i: &Interactions,
        dispatch: Option<&LevelDispatch>,
        hints: &crate::cheshire::Hints,
        atmosphere: &crate::environment::Atmosphere,
        camera: Vec3,
        fullbright: bool,
    ) {
        let dispatch = dispatch.filter(|d| d.valid(self));
        let selected = dispatch.map_or([true; 10], |d| d.selected);
        if selected[0] { if let (Some(a), Some(f)) = (&mut self.fortress, &i.fortress) {
            a.draw(f, atmosphere, camera, fullbright);
        }
        }
        if let Some(a) = &mut self.cheshire {
            if let Some(input) = dispatch.and_then(|d| d.cheshire.as_ref()) {
                a.draw_input(input, atmosphere, camera, fullbright);
            } else { a.draw(hints, atmosphere, camera, fullbright); }
        }
        if selected[1] { if let (Some(a), Some(s)) = (&mut self.school, &i.school) {
            a.draw(s, fullbright, atmosphere, camera);
        }
        }
        if selected[2] { if let (Some(a), Some(v)) = (&mut self.village, &i.village) {
            a.draw(&v.cinema, atmosphere, camera, fullbright);
        }
        }
        if selected[3] { if let (Some(a), Some(s)) = (&mut self.pandemonium, &i.pandemonium) {
            a.draw(s, fullbright, atmosphere, camera);
        }
        }
        if selected[4] { if let (Some(a), Some(s)) = (&mut self.beyond, &i.beyond) {
            a.draw(s, fullbright, atmosphere, camera);
        }
        }
        if selected[5] { if let Some(a) = &mut self.pool {
            if dispatch.is_none() || !a.publish_input(fullbright, atmosphere, camera) {
                if let Some(s) = &i.pool { a.draw(s, fullbright, atmosphere, camera); }
            }
        }
        }
        if selected[6] { if let (Some(a), Some(s)) = (&mut self.gym, &i.gym) {
            a.draw(s, fullbright, atmosphere, camera);
        }
        }
        if selected[7] { if let (Some(a), Some(s)) = (&mut self.duchess, &i.duchess) {
            a.draw(s, fullbright, atmosphere, camera);
        }
        }
        if selected[8] { if let (Some(a), Some(s)) = (&mut self.school2, &i.school2) {
            a.draw(s, fullbright, atmosphere, camera);
        }
        }
        if selected[9] { if let (Some(a), Some(s)) = (&mut self.encounters, &i.encounters) {
            if dispatch.is_none() || !a.draw_snapshot(fullbright, atmosphere, camera) {
                a.school_staging = dispatch.map_or_else(|| i.school.as_ref()
                    .is_some_and(|s| s.scene_id() == Some(crate::school::cinema::THEATRE)), |d| d.school_staging);
                a.draw(s, fullbright, atmosphere, camera);
            }
        }
        }
        if let Some(command) = dispatch {
            for (index, present) in command.registered.iter().copied().enumerate() {
                if !present { continue; }
                // This family's body remains incomplete and fenced. Only that
                // family may borrow its controller; absent dispatch is world-free.
                if let Some(a)=&mut self.levels[index] {
                    if a.draw_snapshot(atmosphere,camera,fullbright) {continue;}
                    if let Some(slot)=i.levels.get(index) {a.draw(&*slot.ctl,atmosphere,camera,fullbright);}
                }
            }
        } else {
            for (art, slot) in self.levels.iter_mut().zip(&i.levels) {
                if let Some(a) = art { a.draw(&*slot.ctl, atmosphere, camera, fullbright); }
            }
        }
    }
    /// Registered controllers' effects, drawn with the particle pass.
    fn draw_captured(
        &mut self,
        command: &LevelDispatch,
        atmosphere: &crate::environment::Atmosphere,
        camera: Vec3,
        fullbright: bool,
    ) {
        if let (Some(art), Some(input)) = (&mut self.cheshire, &command.cheshire) {
            art.draw_input(input, atmosphere, camera, fullbright);
        }
        if command.selected[5] {
            if let Some(art) = &mut self.pool {
                assert!(art.publish_input(fullbright, atmosphere, camera));
            }
        }
        if command.selected[9] {
            if let Some(art) = &mut self.encounters {
                assert!(art.draw_snapshot(fullbright, atmosphere, camera));
            }
        }
        for (selected, art) in command.registered.iter().zip(&mut self.levels) {
            if let (true, Some(art)) = (*selected, art) {
                assert!(art.draw_snapshot(atmosphere, camera, fullbright));
            }
        }
    }

    fn captured_effects(&mut self, command: &LevelDispatch, camera: Vec3, atmosphere: &crate::environment::Atmosphere) {
        for (selected, art) in command.registered.iter().zip(&mut self.levels) {
            if let (true, Some(art)) = (*selected, art) {
                if !art.snapshot_effects_empty() {
                    assert!(art.effects_snapshot(camera, atmosphere));
                }
            }
        }
    }

    fn effects(&mut self, i: &Interactions, camera: Vec3, atmosphere: &crate::environment::Atmosphere) {
        self.effects_dispatch(i, None, camera, atmosphere);
    }
    fn effects_dispatch(
        &mut self,
        i: &Interactions,
        dispatch: Option<&LevelDispatch>,
        camera: Vec3,
        atmosphere: &crate::environment::Atmosphere,
    ) {
        let dispatch = dispatch.filter(|d| d.valid(self));
        if let Some(command) = dispatch {
            for (index, present) in command.registered.iter().copied().enumerate() {
                if !present { continue; }
                // Registered effects remain an explicitly fenced family.
                if let Some(a)=&mut self.levels[index] {
                    if a.effects_snapshot(camera,atmosphere) {continue;}
                    if let Some(slot)=i.levels.get(index) {a.effects(&*slot.ctl,camera,atmosphere);}
                }
            }
        } else {
            for (art, slot) in self.levels.iter_mut().zip(&i.levels) {
                if let Some(a) = art { a.effects(&*slot.ctl, camera, atmosphere); }
            }
        }
    }
    fn hud_sources(&self) -> HudSources {
        HudSources {
            duchess: self.duchess.as_ref().map(crate::duchess::Art::hud_resource),
            registered: self.levels.iter().enumerate().filter_map(|(index, art)| {
                Some((index, art.as_ref()?.hud_capture_source()?))
            }).collect(),
        }
    }
    /// Registered controllers' HUD elements.
    fn hud(&mut self, i: &Interactions) {
        for (art, slot) in self.levels.iter_mut().zip(&i.levels) {
            if let Some(a) = art {
                a.hud(&*slot.ctl);
            }
        }
    }
}
fn enter_level(
    assets: &mut Assets,
    name: &str,
    entry: Option<&str>,
    cached: Option<&Level>,
    options: &Options,
    play_entry: bool,
) -> Result<Entered> {
    crate::loading_screen::run_ready(enter_level_async(assets, name, entry, cached, options, play_entry))
}
/// `enter_level` with its stage boundaries open to a loading frame.
async fn enter_level_async(
    assets: &mut Assets,
    name: &str,
    entry: Option<&str>,
    cached: Option<&Level>,
    options: &Options,
    play_entry: bool,
) -> Result<Entered> {
    crate::transition_memory::fault("level")?;
    let loaded =
        crate::campaign::load_visit_async(assets, name, entry, cached, options.difficulty, play_entry).await?;
    finish_entered(assets, loaded, name, entry, cached, options).await
}
/// The window half of entering a level: the scene, the cast, the particles and the art, built on
/// logic that `campaign::load_visit` or `campaign::arrive` already prepared.
async fn finish_entered(
    assets: &mut Assets,
    loaded: crate::campaign::Loaded,
    name: &str,
    entry: Option<&str>,
    cached: Option<&Level>,
    options: &Options,
) -> Result<Entered> {
    let crate::campaign::Loaded {
        map,
        world,
        mut interactions,
        story,
        hints,
    } = loaded;
    // One window for the whole destination build. The guard clears on success and
    // on every `?` below, so a failure cannot retain destination textures across
    // the load boundary.
    let _share = crate::texture::load_scope();
    let mut timing = crate::startup::Phase::new("entered_resources");
    let Loaded { scene, world } = Scene::from_parts(assets, name, map, world)?;
    timing.mark("scene");
    crate::loading_screen::between("scene").await;
    crate::transition_memory::fault("scene")?;
    if options.npc_preview || options.combat_preview || options.larva_preview {
        interactions.encounters = None;
    }
    let mut npcs = crate::npc::Npcs::load(
        assets,
        &scene.map,
        name,
        entry,
        options.npc_preview,
        options.combat_preview,
    )?;
    if options.larva_preview && name == "centipede1" {
        npcs = crate::npc::Npcs::larva_preview(assets)?;
    }
    if let Some(saved) = cached {
        npcs.restore(&saved.npcs)?;
    }
    timing.mark("npcs");
    crate::loading_screen::between("npcs").await;
    crate::transition_memory::fault("npcs")?;
    let steam = crate::particles::Steam::load(assets, &scene.map)?;
    timing.mark("steam");
    crate::transition_memory::fault("steam")?;
    // Prepare the destination cast before replacing any live level state.
    // Tab, console maps and campaign exits must carry this exact scene/art pair.
    let mut art = LevelArt::default();
    art.prepare(assets, &scene, &interactions, true)?;
    timing.mark("level_art");
    crate::loading_screen::between("level_art").await;
    crate::transition_memory::fault("level_art")?;
    Ok(Entered {
        art,
        scene,
        world,
        interactions,
        story,
        hints,
        npcs,
        steam,
        environment_clock: cached.map_or(0., |s| s.environment_clock),
        pickup_clock: cached.map_or(0., |s| s.pickup_clock),
    })
}

// Present before the synchronous resource work starts. This overlay remains
// visible until the first destination frame; it does not claim animated progress.
pub(crate) fn draw_transition_loading(name: &str) {
    set_default_camera();
    clear_background(BLACK);
    let text = "Loading";
    let size = 32.;
    let width = measure_text(text, None, size as u16, 1.).width;
    draw_text(text, (screen_width()-width)*0.5, screen_height()*0.48, size, WHITE);
    let title = crate::campaign::level_title(name);
    let width = measure_text(title, None, 20, 1.).width;
    draw_text(title, (screen_width()-width)*0.5, screen_height()*0.58, 20., GRAY);
}
async fn present_transition_loading(name: &str) {
    crate::frame_profile::exclude_transition_frame();
    let start = std::time::Instant::now();
    crate::loading_screen::draw_or(|| draw_transition_loading(name));
    #[cfg(not(target_os="vita"))]
    if let Some(path) = std::env::var_os("LOOKING_GLASS_LOADING_CAPTURE") {
        save_capture(std::path::Path::new(&path)).expect("loading indicator capture");
    }
    next_frame().await;
    crate::loading_screen::note_present(start.elapsed().as_secs_f64() * 1000.);
    eprintln!("[lg/transition] loading_indicator=presented destination={name}");
}

async fn enter_natural(
    assets: &mut Assets, stats: &mut Stats, campaign: &mut Campaign,
    leaving: Level, exit: &crate::campaign::Exit, options: &Options,
) -> Result<(Entered, crate::movement::Player, Vec3, f32)> {
    crate::transition_memory::fault("level")?;
    let visit = crate::campaign::arrive_async(assets, stats, campaign, leaving, exit, false).await?;
    let crate::campaign::NextVisit { map, entry, cached, loaded, player, eye, yaw, .. } = visit;
    let entered = finish_entered(assets, loaded, &map, entry.as_deref(), cached.as_ref(), options).await?;
    Ok((entered, player, eye, yaw))
}

#[cfg(all(feature="vita_renderer", not(target_os="vita")))]
pub(crate) async fn check_transition_loading(assets: &mut Assets, options: &Options) -> Result<()> {
    let store = Store::new(options.save_dir.clone(), assets.fingerprint()?);
    let loaded = store.read(Slot::Auto)?;
    let mut game = loaded.game;
    let leaving = game.level()?.clone();
    let exit = ("pandemonium".to_string(), Some("player_start".to_string()));
    present_transition_loading(&exit.0).await;
    let start = std::time::Instant::now();
    let (next, _, _, _) = Box::pin(enter_natural(assets, &mut game.stats, &mut game.campaign, leaving, &exit, options)).await?;
    anyhow::ensure!(next.interactions.pandemonium.is_some() && next.art.pandemonium.is_some(), "natural destination cast");
    println!("PASS natural transition loading indicator and destination; host_load_ms={:.3}", start.elapsed().as_secs_f64()*1000.);
    Ok(())
}

/// The chapter and the restore loads, as the viewer runs them, with whatever `loading_screen`
/// the renderer config sets: on, each load presents one frame per stage boundary (five stage frames, six presents counting the
/// retirement swap) and the
/// release frames show step zero; off, none. Prints host wall times for both loads.
#[cfg(all(feature="vita_renderer", not(target_os="vita")))]
pub(crate) async fn check_loading_screen(assets: &mut Assets, options: &Options) -> Result<()> {
    let on = crate::loading_screen::enabled();
    let maps: Vec<String> = std::env::var("LOOKING_GLASS_LOADING_SCREEN_MAPS").ok()
        .map(|m| m.split(',').map(str::to_owned).collect())
        .unwrap_or_else(|| vec!["gvillage".into(), "hatter1".into()]);
    let expect = |kind: &str, map: &str| -> Result<()> {
        let counts = crate::loading_screen::counts();
        if on {
            anyhow::ensure!(counts == Some((5, 6)), "{kind} {map}: expected five stage frames and six presents (one retirement swap), got {counts:?}");
        } else {
            anyhow::ensure!(counts.is_none(), "{kind} {map}: loading_screen off must not start a load");
        }
        Ok(())
    };
    for map in &maps {
        let load = crate::loading_screen::begin("chapter-or-console", map);
        anyhow::ensure!(load.is_some() == on, "begin follows the key");
        crate::loading_screen::released().await;
        let start = std::time::Instant::now();
        let next = Box::pin(enter_level_async(assets, map, None, None, options, true)).await?;
        let chapter_ms = start.elapsed().as_secs_f64() * 1000.;
        expect("chapter-or-console", map)?;
        drop(load);
        drop(next);
        // A restore of the same visit, as death retry and save load run it.
        let saved = crate::recovery::retry_check::capture(&crate::recovery::retry_check::fixture(assets, map, "entry")?);
        let load = crate::loading_screen::begin("death-retry", map);
        crate::loading_screen::released().await;
        let start = std::time::Instant::now();
        let restored = Box::pin(save::Restored::build_async(assets, saved)).await?;
        let mut art = LevelArt::default();
        art.prepare(assets, &restored.scene, &restored.interactions, true)?;
        crate::loading_screen::between("level_art_models_textures").await;
        let restore_ms = start.elapsed().as_secs_f64() * 1000.;
        expect("death-retry", map)?;
        drop(load);
        println!("PASS loading screen {} map={map} chapter_host_ms={chapter_ms:.1} restore_host_ms={restore_ms:.1}",
            if on { "on: five stage frames per load" } else { "off: no load frames" });
    }
    Ok(())
}

pub(crate) fn save_capture(path: &std::path::Path) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    save_capture_into(path)
}

/// Write a capture into a directory that already exists. The console cannot create
/// one: replay pictures go straight into the pre-existing `ux0:data/alice/private`
/// (MKD is forbidden there), so this path never attempts a directory create.
pub(crate) fn save_capture_into(path: &std::path::Path) -> Result<()> {
    #[cfg(feature="native_timing")]
    crate::native_timing::operations::attempt();
    let frame = get_screen_data();
    // OpenGL reads rows from the bottom; PNG stores rows from the top.
    let mut pixels = frame.bytes;
    let stride = frame.width as usize * 4;
    for y in 0..frame.height as usize / 2 {
        let opposite = frame.height as usize - 1 - y;
        let (top, bottom) = pixels.split_at_mut(opposite * stride);
        top[y * stride..(y + 1) * stride].swap_with_slice(&mut bottom[..stride]);
    }
    image::save_buffer(
        path,
        &pixels,
        frame.width as u32,
        frame.height as u32,
        image::ColorType::Rgba8,
    )?;
    println!("Capture: {}", path.display());
    Ok(())
}

/// Exercise the complete resource graph, including two live players/scenes
/// during replacement. Map-only tests miss lazily loaded encounter/prop materials.
pub async fn check_level_swaps(assets: &mut Assets, options: &Options) -> Result<()> {
    crate::character::check_shared_material().await?;
    let catalog = Catalog::load(assets)?;
    let hud = Hud::load(assets)?;
    let mut power_art = crate::powerups::Art::load(assets)?;
    let mut alice = Character::load(assets)?;
    let mut active = enter_level(assets, "skool1", None, None, options, true)?;
    anyhow::ensure!(active.art.school.is_some(), "Missing starting school cast");
    let names = assets.maps();
    let mut visits = names.iter().map(|n| (n.as_str(), None)).collect::<Vec<_>>();
    visits.extend(
        names
            .iter()
            .filter_map(|n| crate::campaign::return_entry(n).map(|e| (n.as_str(), Some(e)))),
    );
    let mut count = 0;
    for _ in 0..2 {
        for &(name, entry) in &visits {
            // Keep the old full scene/art alive until the new graph is ready.
            let next = enter_level(assets, name, entry, None, options, true)?;
            let mut next_alice = Character::load(assets)?;
            active = next;
            std::mem::swap(&mut alice, &mut next_alice);
            drop(next_alice);
            let (eye, yaw) = interaction::spawn_with_static(&active.scene.map, entry, Some(active.world.audio_geometry()));
            let player = Player::spawn(&active.world, eye)
                .unwrap_or_else(|| Player::new(eye - Vec3::Z * EYE_HEIGHT));
            alice.reset(&player, yaw);
            let direction = vec3(yaw.cos(), yaw.sin(), 0.);
            let (camera, target, _) =
                character::follow_camera(&active.world, player.feet, direction);
            let transforms = active.interactions.transforms();
            clear_background(active.scene.atmosphere.background());
            set_camera(&Camera3D {
                position: camera,
                target,
                up: Vec3::Z,
                fovy: 75_f32.to_radians(),
                z_near: 2.,
                z_far: crate::render::draw_distance_far(30000.),
                ..Default::default()
            });
            active.scene.draw(camera, 1., false, false, &transforms);
            alice.atmosphere(&active.scene.atmosphere, camera);
            alice.draw_ghost(player.feet, false, false);
            active.npcs.draw(
                camera,
                (target - camera).normalize_or_zero(),
                &active.scene.atmosphere,
                false,
                false,
            );
            active.art.draw(
                &active.interactions,
                &active.hints,
                &active.scene.atmosphere,
                camera,
                false,
            );
            let stats = starting_stats(options, name, entry);
            let pickups = inventory::pickups_for_visit(&active.scene.map, name, entry, &catalog);
            power_art.draw(
                &pickups,
                &stats,
                &save::visit_key(name, entry),
                &active.scene.atmosphere,
                camera,
                1.,
            );
            crate::render::depth_read_only(|| {
                active.scene.draw_with_particles(
                    camera,
                    (target - camera).normalize_or_zero(),
                    1.,
                    false,
                    &transforms,
                    &active.steam,
                );
                alice.draw_effects(camera, false, true);
            });
            set_default_camera();
            hud.draw(
                &stats,
                &catalog,
                false,
                &HudView::capture(&crate::input::Input::default()),
                &crate::preferences::Preferences::default(),
            );
            next_frame().await;
            count += 1;
            println!("PASS full level replacement {count}: {name} {entry:?}");
        }
    }
    println!("PASS {count} full level replacements with overlapping worlds, Alice, NPCs, particles, level art, Cheshire and power-up art");
    Ok(())
}

/// Exercise the same destination package used by Tab and console map changes.
/// The old level remains alive until the new scene and its cutscene cast are ready.
pub async fn check_level_entry(assets: &mut Assets, options: &Options) -> Result<()> {
    use crate::character::{visibility_image, visible_pixels};
    use crate::village::cinema::Beat;
    let mut active = enter_level(assets, "skool1", None, None, options, false)?;
    anyhow::ensure!(active.art.school.is_some(), "Missing starting school cast");
    for (visit, name) in ["gvillage", "skool1", "gvillage"].into_iter().enumerate() {
        let next = enter_level(assets, name, None, None, options, true)?;
        active = next;
        anyhow::ensure!(
            active.art.village.is_some() == active.interactions.village.is_some()
                && active.art.school.is_some() == active.interactions.school.is_some(),
            "Destination cutscene cast does not match {name}"
        );
        if name != "gvillage" {
            continue;
        }
        let cinema = &mut active.interactions.village.as_mut().unwrap().cinema;
        anyhow::ensure!(
            cinema.state.beat == Some(Beat::Fall),
            "Village opening did not start"
        );
        // Use an authored opening conversation shot; the falling Alice starts
        // above the camera at time zero and cannot prove visible geometry.
        cinema.state.beat = Some(Beat::Cat);
        cinema.state.time = 17.;
        let shot = cinema.camera().context("Missing Village opening camera")?;
        let camera = Camera3D {
            position: shot.eye,
            target: shot.target,
            up: shot.up,
            fovy: 75_f32.to_radians(),
            z_near: 2.,
            z_far: crate::render::draw_distance_far(30000.),
            ..Default::default()
        };
        clear_background(BLACK);
        set_camera(&camera);
        crate::render_fx::begin_view(&camera, 17., &active.scene.atmosphere, true);
        active.scene.draw(
            shot.eye,
            17.,
            true,
            false,
            &active.interactions.transforms(),
        );
        let before = visibility_image();
        crate::render_fx::begin_view(&camera, 17., &active.scene.atmosphere, true);
        active.art.story_pose(&active.story);
        active.art.draw(
            &active.interactions,
            &active.hints,
            &active.scene.atmosphere,
            shot.eye,
            true,
        );
        let after = visibility_image();
        let pixels = visible_pixels(&before, &after);
        after.export_png(&format!("private/visibility/level-entry-{visit}.png"));
        anyhow::ensure!(
            pixels > 200,
            "Missing Village cast after level switch: {pixels} pixels"
        );
        println!("PASS Tab/console destination cast after school-to-Village switch {visit}: {pixels} pixels");
        next_frame().await;
    }
    Ok(())
}

fn starting_stats(options: &Options, map: &str, entry: Option<&str>) -> Stats {
    let mut stats = if options.weapon_preview {
        Stats::weapon_preview()
    } else if options.hud_preview {
        Stats::preview()
    } else {
        Stats::for_level(map, entry)
    };
    stats.difficulty = options.difficulty;
    stats
}

mod start;

/// Collected only for snapshot diagnostic rows.
struct SnapshotTiming {
    start: std::time::Instant,
    scope: Option<crate::kernel_alloc::Scope>,
}
impl SnapshotTiming {
    fn start() -> Self {
        let scope = crate::kernel_alloc::Scope::begin_timed();
        Self { start: std::time::Instant::now(), scope }
    }

    fn finish(self) -> (f64, impl std::fmt::Display) {
        struct AllocationCount(Option<u64>);
        impl std::fmt::Display for AllocationCount {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self.0 {
                    Some(count) => write!(f, "{count}"),
                    None => f.write_str("off"),
                }
            }
        }

        let us = self.start.elapsed().as_secs_f64() * 1_000_000.;
        let count = self.scope.map(crate::kernel_alloc::Scope::finish);
        (us, AllocationCount(count))
    }
}

mod step;
mod blocked_step;
pub(crate) mod sim_audit;
mod sim_timing;
mod free_render;
mod lag_frame;
mod lag_control;
use step::{
    step_a, step_b, step_due, Due, SimState, ConsoleOps, StepAInput, StepBInput, StepJob, StepLocals, StepScopes,
    StepView,
};

fn acknowledge_checkpoint(result: Result<()>, levels: &mut [crate::level::Slot]) -> Result<()> {
    result?;
    for slot in levels {
        slot.ctl.checkpoint_written();
    }
    Ok(())
}

pub async fn run(mut assets: Assets, mut options: Options) -> Result<()> {
    #[cfg(target_os="vita")]
    let mut r13_survey = crate::r13_survey::Survey::open(&mut options)?;
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    let physical_input = !crate::sim_maps::probe::enabled();
    #[cfg(not(all(feature = "sim_maps_probe", not(target_os = "vita"))))]
    let physical_input = true;
    // A survey keeps the pad off unless its config sets physical_input (human-played census).
    #[cfg(target_os="vita")]
    let physical_input = physical_input && r13_survey.as_ref().is_none_or(|s|s.physical_input());
    // SELECT arms a survey, so it must not also open the chapter menu in that frame.
    #[cfg(target_os="vita")]
    let survey_owns_select = r13_survey.is_some();
    #[cfg(not(target_os="vita"))]
    let survey_owns_select = false;
    let startup=std::time::Instant::now();
    eprintln!("[lg/startup] viewer begin; UI begin");
    // Release cursor/audio before Miniquad destroys its native request queue.
    prevent_quit();
    let ui = {let _r13=crate::allocation_census::Scope::new(9);crate::ui::Ui::load(&mut assets)?};
    eprintln!("[lg/startup] UI done elapsed_ms={:.3}; menu begin",startup.elapsed().as_secs_f64()*1000.);
    // Explicit native layout diagnostic; never opens or modifies player saves.
    if options.frames.is_some() && std::env::var_os("LOOKING_GLASS_PRESENTATION_CHECK").is_some() {
        return crate::hud::check_presentation(&mut assets).await;
    }
    let mut escape_menu = crate::menu::Menu::load(&mut assets).context("Loading original menus")?;
    eprintln!("[lg/startup] menu done elapsed_ms={:.3}",startup.elapsed().as_secs_f64()*1000.);
    let mut preferences = std::sync::Arc::new(crate::preferences::Preferences::load());
    let mut input = crate::input::Input::default();
    let mut weapon_buttons = crate::weapon_rules::Buttons::default();
    weapon_buttons.block();
    // Host fidelity captures keep the probe's 960x544 window, the Vita's screen, instead of
    // the saved display size. Opt-in, so other probe baselines keep their window.
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    let keep_window = crate::sim_maps::probe::enabled() && std::env::var_os("LOOKING_GLASS_PROBE_KEEP_WINDOW").is_some();
    #[cfg(not(all(feature = "sim_maps_probe", not(target_os = "vita"))))]
    let keep_window = false;
    if !keep_window { preferences.display(); }
    escape_menu.backdrop();
    ui.toast("Opening Wonderland...", screen_height() - 70.);
    next_frame().await;
    let saves_enabled = !(options.npc_preview
        || options.combat_preview
        || options.larva_preview
        || options.weapon_preview
        || options.hud_preview
        || options.shelf_preview
        || options.start_at.is_some()
        || options.story_preview.is_some());
    #[cfg(target_os="vita")]
    let saves_enabled=saves_enabled && r13_survey.is_none();
    #[cfg(feature="vita_renderer")]
    let mut route_session=crate::route_replay::Session::open(&mut options)?;
    #[cfg(feature = "vita_renderer")]
    let live_chapter = {
        let config = crate::render::vita_renderer::config();
        if config.sim_overlap_live {
            anyhow::ensure!(route_session.is_none(), "sim_overlap_live and route diagnostics are exclusive");
            crate::replay::configure_live_chapter(&mut options, &config.replay_start)?;
        }
        config.sim_overlap_live
    };
    #[cfg(not(feature = "vita_renderer"))]
    let live_chapter = false;
    eprintln!("[lg/startup] route options ready; save store begin");
    let save_store_start = std::time::Instant::now();
    let fingerprint = assets.fingerprint()?;
    let store_start = std::time::Instant::now();
    let store = Store::new(options.save_dir.clone(), fingerprint);
    // A tape session (src/replay) picks the start and, for a replay, the control settings.
    let mut replay = crate::replay::Session::open(&mut options, &store, std::sync::Arc::make_mut(&mut preferences))?;
    #[cfg(feature="vita_renderer")]
    anyhow::ensure!(!(replay.is_some() && route_session.is_some()), "replay and route diagnostics are exclusive");
    let replay_resume = replay.as_mut().and_then(|r| r.take_resume());
    let store = if replay.as_ref().is_some_and(|r| r.replaying()) { store.discarding_writes() } else { store };
    let mut replay_end: Option<&'static str> = None;
    eprintln!("[lg/startup] save store ready elapsed_ms={:.3} store_construct_ms={:.3} viewer_elapsed_ms={:.3}",save_store_start.elapsed().as_secs_f64()*1000.,store_start.elapsed().as_secs_f64()*1000.,startup.elapsed().as_secs_f64()*1000.);
    #[cfg(feature="vita_renderer")]
    let route_resume=route_session.as_ref().map(|r|r.resume(&store)).transpose()?.flatten();
    eprintln!("[lg/startup] requested save ready elapsed_ms={:.3}",startup.elapsed().as_secs_f64()*1000.);
    #[cfg(not(feature="vita_renderer"))]
    let route_resume:Option<save::Loaded>=None;
    let maps = assets.maps();
    let mut audio = Audio::new(options.no_audio, options.audio_capture.clone());
    let mut startup_memory: Option<crate::transition_memory::Transition> = None;
    #[cfg(target_os = "vita")]
    let mut bench_session = crate::frame_profile::startup_bench();
    #[cfg(target_os = "vita")]
    let mut bench_restored = None;
    #[cfg(target_os = "vita")]
    let bench_resume = if bench_session && saves_enabled {
        match store.read_bench_auto().and_then(|loaded| {
            // Validate the complete restoration before bypassing the front end.
            // The scene built here is installed below; load_scene must not run again.
            println!("bench: loading auto save {}", loaded.game.current);
            startup_memory = Some(crate::transition_memory::Transition::begin(
                "startup-entry", "menu", &loaded.game.level()?.map));
            let restored = save::Restored::build(&mut assets, loaded.game.clone())?;
            bench_restored = Some(restored);
            Ok(loaded)
        }) {
            Ok(loaded) => Some(loaded),
            Err(e) => {
                if let Some(mut memory) = startup_memory.take() { memory.finish(false); }
                eprintln!("bench: no auto save ({e:#})");
                None
            }
        }
    } else {
        None
    };
    #[cfg(not(target_os = "vita"))]
    let bench_resume: Option<save::Loaded> = None;
    let mut opening_requested = options.new_game && options.map == "gvillage";
    // A tape starts like the menu's Chapter choice or a loaded save: never with the opening film.
    if replay.is_some() || live_chapter { opening_requested = false; }
    let resume = if let Some(loaded)=route_resume { Some(loaded) } else if let Some(loaded) = replay_resume { Some(loaded) } else if let Some(loaded) = bench_resume {
        Some(loaded)
    } else if let Some(slot) = options.load {
        anyhow::ensure!(
            saves_enabled,
            "Save loading is unavailable in a staged preview"
        );
        Some(store.read(slot)?)
    } else if saves_enabled && !options.new_game && !options.direct_map
        && (options.frames.is_none() || crate::transition_memory::host_start_chapter().is_some())
    {
        match start::choose(
            &mut options,
            &mut escape_menu,
            std::sync::Arc::make_mut(&mut preferences),
            &mut input,
            &mut audio,
            &store,
            &maps,
        )
        .await
        {
            start::Start::Chapter => {
                opening_requested = false;
                None
            }
            start::Start::NewGame => {
                opening_requested = true;
                None
            }
            start::Start::Load(loaded) => Some(*loaded),
            start::Start::Quit => return Ok(()),
        }
    } else {
        None
    };
    escape_menu.frontend = false;
    escape_menu.close();
    if let Some(loaded) = &resume {
        options.map = loaded.game.level()?.map.clone();
        options.entry = loaded.game.level()?.entry.clone();
        options.difficulty = loaded.game.stats.difficulty;
    }
    let mut startup_memory = startup_memory.unwrap_or_else(||
        crate::transition_memory::Transition::begin("startup-entry", "menu", &options.map));
    escape_menu.backdrop();
    ui.toast("Opening Wonderland...", screen_height() - 70.);
    next_frame().await;
    let level_choices = crate::campaign::level_choices(&maps);
    let mut current = maps
        .iter()
        .position(|m| m == &options.map)
        .context("Map not found")?;
    let mut console = crate::console::Console::new();
    let mut pickup_feedback = inventory::PickupFeedback::default();
    let mut third_person = true;
    let mut follow_camera = crate::camera::Follow::default();
    let mut camera_handoff = crate::camera::Handoff::default();
    let mut rail_camera = crate::camera::Follow::default();
    let mut clock = FixedClock::default();
    #[cfg(feature = "native_timing")]
    let mut timing = crate::native_timing::Session::open()?;
    #[cfg(feature = "native_timing")]
    audio.bind_native_counters(&mut timing);
    let mut recovery = Recovery::default();
    let mut retry = recovery::RetryPoint::default();
    let mut paused = false;
    let mut pitch = 0.0f32;
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    let steam_poison_frame = std::env::var("LOOKING_GLASS_SNAPSHOT_POISON_STEAM_FRAME")
        .ok().map(|value| value.parse::<u64>().expect("invalid steam poison frame"));
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    let force_acoustic_view = std::env::var("LOOKING_GLASS_ACOUSTIC_VIEW").as_deref() == Ok("1");
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    let profile_acoustic_view = std::env::var("LOOKING_GLASS_ACOUSTIC_VIEW_PROFILE").as_deref() == Ok("1");
    let mut frame = 0;
    let mut handoff_return = crate::render_snapshot::HandoffReturn::default();
    let mut handoff_minimum_frame = 0u64;
    let mut snapshot_frames = 0u64;
    let mut wave_input=Vec::new();
    let mut pickup_input=crate::powerups::PickupInput::default();
    let mut snapshot_fenced_frames = 0u64;
    let mut acoustic_view = None;
    #[cfg(feature = "vita_renderer")]
    let sim_overlap = crate::render::vita_renderer::config().sim_overlap;
    #[cfg(not(feature = "vita_renderer"))]
    let sim_overlap = false;
    let mut sim_audit = sim_audit::Audit::new();
    let mut sim_worker = sim_overlap.then(|| blocked_step::Worker::new(&sim_audit)).transpose()?;
    let mut worker_generation = 0u64;
    let mut dispatched_frames = 0u64;
    let mut sim_trace = sim_timing::Trace::new();
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    let force_checkpoint_frame = std::env::var("LOOKING_GLASS_SIM_FORCE_CHECKPOINT")
        .ok().map(|value| value.parse::<u64>()).transpose()?;
    let mut menu = false;
    let mut inventory_menu = false;
    let mut help_until = 0.0;
    let mut fullbright = false;
    let mut selected = current;
    let mut chapters = crate::chapters::Chapters::default();
    let mut selected_entry: Option<String> = None;
    let mut notice = String::new();
    let mut new_game_requested = None;
    let mut chapter_requested = None;
    let mut mouse_look = MouseLook::default();
    let mut was_focused = true;
    let mut played = resume.is_none();
    let mut post_game = false;
    #[cfg(feature = "vita_renderer")]
    let mut load_probe = Some(crate::restore_diagnostic::Probe::new("startup-entry"));
    let mut save_frame = crate::save_preview::Frame::default();
    let mut escape_preview_ready = false;
    let mut pending_menu_save = None;
    let mut pending_menu_newgame = None;
    let mut scene: Scene;
    let mut world: World;
    let mut level_art: LevelArt;
    let mut skip_scene: crate::cinematic::Skip;
    let mut power_art: crate::powerups::Art;
    let mut hints: crate::cheshire::Hints;
    let mut steam: crate::particles::Steam;
    let mut interactions: Interactions;
    let mut entry_spawn: Option<String>;
    let mut npcs: crate::npc::Npcs;
    let catalog: Catalog;
    let mut stats: Stats;
    let mut hud: Hud;
    let mut pickups: Vec<inventory::Pickup>;
    let mut pickup_clock: f32;
    let mut environment_clock: f32;
    let mut story: crate::story::Story;
    let mut pos: Vec3;
    let mut yaw: f32;
    let mut flying: bool;
    let mut spawn_landing: bool;
    let mut player: Player;
    let mut alice: Character;
    let mut aim_pointer: crate::targeting::Pointer;
    let mut campaign: Campaign;
    #[cfg(feature = "native_timing")]
    macro_rules! timing_state {
        () => { crate::native_timing::State::observe(environment_clock, pickup_clock,
            clock.ticks, [pos.x, pos.y, pos.z, yaw, pitch]) };
    }
    #[cfg(feature = "native_timing")]
    macro_rules! timing_ready {
        ($reason:expr) => { timing.with(|r| r.ready(&maps[current], entry_spawn.as_deref(),
            $reason, timing_state!())) };
    }
    #[cfg(feature = "native_timing")]
    macro_rules! timing_install_reason {
        (initial) => { crate::native_timing::Reason::StartSave };
        (replace) => { crate::native_timing::Reason::Load };
        (retry) => { crate::native_timing::Reason::Retry };
    }
    macro_rules! install_loaded_scene {
        (initial, $next:expr, $from:expr, $to:expr) => { Loaded { scene, world } = $next; };
        (retry, $next:expr, $from:expr, $to:expr) => { install_loaded_scene!(replace, $next, $from, $to); };
        (replace, $next:expr, $from:expr, $to:expr) => {
            Loaded { scene, world } = $next;
        };
    }
    // Leave the step and drop its clock before yielding to owner work. Only the
    // controls that the base reads live can change on return.
    macro_rules! owner_yield {
        ($guards:ident, $future:expr) => {{
            let in_step = $guards.take().is_some();
            let result = ($future).await;
            input.refresh_controls();
            if in_step {
                let step = StepGuard::enter();
                $guards = Some((step, ClockView::capture().enter()));
            }
            result
        }};
    }
    macro_rules! restore_install_yield {
        () => { crate::loading_screen::between("level_art_models_textures").await };
        ($guards:ident) => { owner_yield!($guards, crate::loading_screen::between("level_art_models_textures")) };
    }
    macro_rules! apply_loaded {
        ($restored:expr) => { apply_loaded!($restored, true) };
        ($restored:expr, $pause:expr) => { apply_loaded!($restored, $pause, replace) };
        ($restored:expr, $pause:expr, $install:ident $(, $guards:ident)?) => {{
            #[cfg(feature = "vita_renderer")]
            { load_probe = Some(crate::restore_diagnostic::Probe::new("restore")); }
            let r = $restored;
            let level = r.game.level()?.clone();
            #[cfg(feature = "native_timing")]
            timing.with(|r| r.before_install());
            let _previous_map = current;
            current = maps
                .iter()
                .position(|m| m == &level.map)
                .context("Saved map unavailable")?;
            selected = current;
            retry.remember(r.game.clone());
            entry_spawn = level.entry;
            selected_entry = entry_spawn.clone();
            install_loaded_scene!($install, Loaded { scene: r.scene, world: r.world }, &maps[_previous_map], &maps[current]);
            interactions = r.interactions;
            story = r.story;
            hints = r.hints;
            npcs = r.npcs;
            alice = r.alice;
            steam = r.steam;
            campaign = r.game.campaign;
            stats = r.game.stats;
            options.difficulty = stats.difficulty;
            player = r.game.player;
            recovery = r.game.recovery;
            pos = r.game.view.position;
            yaw = r.game.view.yaw;
            pitch = r.game.view.pitch;
            flying = r.game.view.flying;
            third_person = r.game.view.third_person;
            follow_camera.reset();
            camera_handoff.reset();
            rail_camera.reset();
            fullbright = r.game.view.fullbright;
            spawn_landing = r.game.view.spawn_landing;
            environment_clock = level.environment_clock;
            pickup_clock = level.pickup_clock;
            power_art.clear_effects();
            pickups = inventory::pickups_for_visit(
                &scene.map,
                &maps[current],
                entry_spawn.as_deref(),
                &catalog,
            );
            level_art = LevelArt::default();
            let mut install_timing = crate::startup::Phase::new("restore_install");
            level_art.prepare(&mut assets, &scene, &interactions, true)?;
            install_timing.mark("level_art_models_textures");
            restore_install_yield!($($guards)?);
            skip_scene = crate::cinematic::Skip::default();
            audio.load(&mut assets, &maps[current], &scene.map);
            install_timing.mark("audio_load");
            clock.pause();
            paused = $pause;
            menu = false;
            inventory_menu = false;
            console.open = false;
            help_until = 0.;
            input.suppress();
            mouse_look.release();
            notice.clear();
            played = true;
            post_game = false;
            escape_menu.post_game = false;
            save_frame.clear();
            #[cfg(feature = "native_timing")]
            timing_ready!(timing_install_reason!($install));
            crate::texture::release_shared();
        }};
    }
    #[cfg(target_os = "vita")]
    let prepared = bench_restored.take();
    #[cfg(not(target_os = "vita"))]
    let prepared: Option<save::Restored> = None;
    // Menu Continue/Load and --load must reuse their restored scene just like benches.
    // Prepare before the fresh-game branch; failed restoration keeps the existing
    // paused fresh-scene/error behavior, without retrying an expensive restore.
    let mut resume_error = None;
    let prepared = if prepared.is_some() { prepared } else if let Some(loaded) = &resume {
        let restored = {
            // The restore build is its own load window; a failed one keeps nothing.
            let _share = crate::texture::load_scope();
            save::Restored::build(&mut assets, loaded.game.clone())
        };
        match restored {
            Ok(restored) => Some(restored),
            Err(error) => { resume_error = Some(format!("Could not restore save: {error:#}")); None }
        }
    } else { None };
    let mut resume_applied = false;
    let validated = prepared.is_some();
    if save::bench_scene(validated) == save::BenchScene::ReuseValidated {
        // The restored scene's build and its install are one load window.
        let _share = crate::texture::load_scope();
        let mut timing = crate::startup::Phase::new("restore_resources");
        power_art = crate::powerups::Art::load(&mut assets)?;
        timing.mark("power_art");
        catalog = Catalog::load(&mut assets)?;
        timing.mark("inventory_catalog");
        hud = Hud::load(&mut assets)?;
        timing.mark("hud_textures");
        aim_pointer = crate::targeting::Pointer::load(&mut assets)?;
        timing.mark("aim_pointer");
        let fresh = starting_stats(&options, &maps[current], options.entry.as_deref());
        println!(
            "Level loadout {}: {}",
            maps[current],
            fresh.weapon_summary()
        );
        #[cfg(target_os = "vita")]
        let pause_on_load = !bench_session;
        #[cfg(not(target_os = "vita"))]
        let pause_on_load = true;
        #[cfg(all(feature="sim_maps_probe", not(target_os="vita")))]
        let pause_on_load = pause_on_load && !(crate::sim_maps::probe::enabled()
            && std::env::var_os("LOOKING_GLASS_SIM_MAPS_RESUME_LOAD").is_some());
        apply_loaded!(prepared.unwrap(), pause_on_load, initial);
        timing.mark("installed");
        resume_applied = true;
        println!("[lg/startup] restored scene reused map={} source={}", maps[current],
            if options.load.is_some() { "load" } else { "continue-or-bench" });
    } else {
        #[cfg(feature = "native_timing")]
        timing.with(|r| r.before_install());
        // The boot route builds the whole level on the main thread before the viewer
        // loop exists, so the menu stays frozen unless a load is opened here. With
        // `loading_screen` off, none of this presents a frame.
        //
        // A fresh boot has no outgoing resources, so there is no retirement
        // sequence: one step-zero frame replaces the menu, then one frame per stage.
        // The load guard makes this build a single cache window and clears it on
        // every return path.
        let _share = crate::texture::load_scope();
        let boot_loading = crate::loading_screen::begin("chapter-or-console", &maps[current]);
        crate::loading_screen::show().await;
        Loaded { scene, world } = load_scene(&mut assets, &maps[current], options.difficulty)?;
        crate::loading_screen::between("scene").await;
        level_art = LevelArt::default();
        skip_scene = crate::cinematic::Skip::default();
        power_art = crate::powerups::Art::load(&mut assets)?;
        hints = crate::cheshire::Hints::load(&mut assets, &scene.map, &maps[current])?;
        steam = crate::particles::Steam::load(&mut assets, &scene.map)?;
        interactions = Interactions::load(&scene.map)?;
        interactions.set_entry(
            &mut assets,
            &scene.map,
            &maps[current],
            options.entry.as_deref(),
        )?;
        if options.npc_preview || options.combat_preview || options.larva_preview {
            interactions.encounters = None;
        }
        interactions.sync(&mut world);
        save::validate_entry(&scene.map, options.entry.as_deref())?;
        entry_spawn = options.entry.clone();
        npcs = crate::npc::Npcs::load(
            &mut assets,
            &scene.map,
            &maps[current],
            entry_spawn.as_deref(),
            options.npc_preview,
            options.combat_preview,
        )?;
        if options.larva_preview && maps[current] == "centipede1" {
            npcs = crate::npc::Npcs::larva_preview(&mut assets)?;
        }
        crate::loading_screen::between("npcs").await;
        catalog = Catalog::load(&mut assets)?;
        stats = starting_stats(&options, &maps[current], entry_spawn.as_deref());
        println!(
            "Level loadout {}: {}",
            maps[current],
            stats.weapon_summary()
        );
        hud = Hud::load(&mut assets)?;
        if options.hud_preview {
            hud.announce("HUD preview / sample resources and all toys");
        }
        if options.weapon_preview {
            hud.announce("Weapon preview / all toys equipped with 1-0");
        }
        if options.combat_preview {
            hud.announce("Combat preview / 1 Blade, 2 Cards, 3 Mallet / Home retry");
        }
        pickups =
            inventory::pickups_for_visit(&scene.map, &maps[current], entry_spawn.as_deref(), &catalog);
        pickup_clock = 0.;
        environment_clock = 0.;
        audio.load(&mut assets, &maps[current], &scene.map);
        story = crate::story::Story::load(&mut assets, &maps[current]);
        crate::loading_screen::between("sound").await;
        let mut entry_timing = crate::startup::Phase::new("entry_after_story");
        eprintln!("[lg/startup] entry owner=engine map={} opening_roq_requested={opening_requested}", maps[current]);
        if options.shelf_preview {
            anyhow::ensure!(maps[current] == "skool1", "--shelf-preview requires skool1");
            let mut staging = Player::spawn(&world, scene.map.spawn().0)
                .context("School staging entrance blocked")?;
            let school = interactions
                .school
                .as_mut()
                .context("School staging unavailable")?;
            for event in [
                "Theatre_Cinematic",
                "Skool1_OG_MoveShelf",
                "shelf_cinematic",
            ] {
                school.event(event);
            }
            for _ in 0..1250 {
                interactions.advance_school(
                    crate::movement::FIXED_DT,
                    &scene.map,
                    &mut world,
                    &mut staging,
                )?;
            }
            hud.announce("Shelf crossing preview / WASD move, Space jump");
        }
        if let Some(event) = &options.story_preview {
            anyhow::ensure!(story.trigger(event), "Unknown story preview {event}");
        } else if options.start_at.is_none() && !options.fly && !options.shelf_preview {
            interactions.entry_story(&mut story);
        }
        (pos, yaw) = interaction::spawn_with_static(&scene.map, entry_spawn.as_deref(), Some(world.audio_geometry()));
        if let Some(feet) = options.start_at {
            pos = feet + Vec3::Z * EYE_HEIGHT;
        }
        // Host census only: the spawn yaw, and a yaw (degrees) that replaces it, so a `--start-at`
        // view can face the way a console tape faced.
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        if crate::sim_maps::probe::draw_query::active() || std::env::var_os("LOOKING_GLASS_START_YAW").is_some() {
            println!("START_YAW spawn={:.4} rad", yaw);
            if let Some(degrees) = std::env::var("LOOKING_GLASS_START_YAW").ok().and_then(|v| v.parse::<f32>().ok()) {
                yaw = degrees.to_radians();
                println!("START_YAW set={:.4} rad", yaw);
            }
        }
        if options.larva_preview && maps[current] == "centipede1" {
            (pos, yaw) = npcs.larva_preview_start(&world)?;
            hud.announce("Larva encounter / WASD move, mouse attack / Home retry");
        }
        if options.shelf_preview {
            // The exact formerly stalled contact from the real-data drop regression.
            pos = vec3(220., 3300., 413.13885) + Vec3::Z * EYE_HEIGHT;
            yaw = std::f32::consts::FRAC_PI_4 * 3.;
        }
        let spawn = Player::spawn(&world, pos);
        flying = options.fly || spawn.is_none();
        // Some map starts still expect a cinematic or scripted moving support.
        spawn_landing = true;
        player = spawn.unwrap_or_else(|| Player::new(pos - Vec3::Z * EYE_HEIGHT));
        entry_timing.mark("spawn_and_story_entry");
        alice = Character::load(&mut assets).context("Loading Alice's character")?;
        entry_timing.mark("alice_character");
        crate::loading_screen::between("alice").await;
        aim_pointer = crate::targeting::Pointer::load(&mut assets)?;
        alice.reset(&player, yaw);
        entry_timing.mark("aim_pointer_and_character_reset");
        level_art.prepare(&mut assets, &scene, &interactions, true)?;
        entry_timing.mark("level_art");
        crate::loading_screen::between("level_art").await;
        drop(boot_loading);
        campaign = Campaign::default();
        #[cfg(feature = "native_timing")]
        timing_ready!(crate::native_timing::Reason::Fresh);
    }
    let mut preview_throttle = crate::frame_profile::PreviewThrottle::default();
    #[cfg(target_os = "vita")]
    let mut bench_camera: Option<(Vec3, Vec3, bool)> = None;
    macro_rules! level_snapshot {
        () => {
            Level {
                map: maps[current].clone(),
                entry: entry_spawn.clone(),
                interactions: interactions.snapshot(),
                npcs: npcs.snapshot(),
                story: story.snapshot(),
                hints: hints.snapshot(),
                environment_clock,
                pickup_clock,
            }
        };
    }
    macro_rules! game_snapshot {
        () => {{
            let level = level_snapshot!();
            let key = level.key();
            let mut saved_campaign = campaign.clone();
            saved_campaign.levels.insert(key.clone(), level);
            Game {
                current: key,
                campaign: saved_campaign,
                stats: stats.clone(),
                player: player.clone(),
                view: View {
                    position: pos,
                    yaw,
                    pitch,
                    third_person,
                    flying,
                    fullbright,
                    spawn_landing,
                },
                recovery: recovery.clone(),
                character: alice.snapshot(),
            }
        }};
    }
    if resume.is_none()
        && saves_enabled
        && opening_requested
        && options.frames.is_none()
    {
        #[cfg(feature = "vita_renderer")]
        anyhow::ensure!(!(crate::movie::bypass_requested()
            && route_session.as_ref().is_some_and(|r| r.needs_opening())),
            "New Game route requires the opening film; disable story_film_bypass");
        #[cfg(feature = "native_timing")]
        timing.with(|r| r.discontinuity(crate::native_timing::Milestone::OpeningEnter, crate::native_timing::CINEMA));
        match crate::movie::play(
            &mut assets,
            &mut audio,
            &ui,
            &mut input,
            &preferences,
            "opening",
        )
        .await
        {
            Ok(false) => return Ok(()),
            Err(e) => {
                eprintln!("Opening film unavailable: {e:#}");
                #[cfg(feature="vita_renderer")]
                if route_session.as_ref().is_some_and(|r|r.needs_opening()) {
                    return Err(e.context("New Game route requires the opening film"));
                }
                hud.announce("Opening film unavailable / continuing to Wonderland");
            }
            Ok(true) => {
                #[cfg(feature="vita_renderer")]
                if let Some(route)=&mut route_session {route.opening_complete();}
            },
        }
        #[cfg(feature = "native_timing")]
        timing.with(|r| r.discontinuity(crate::native_timing::Milestone::OpeningExit, crate::native_timing::CINEMA));
        clock.pause();
    }
    if resume_applied {
        #[cfg(target_os = "vita")]
        if bench_session { println!("bench: auto save restored {}", save::visit_key(&maps[current], entry_spawn.as_deref())); }
        hud.announce(if resume.as_ref().is_some_and(|loaded| loaded.backup) {
            "Previous save restored / P or click to resume"
        } else { "Game restored / P or click to resume" });
    } else if let Some(error) = resume_error {
        paused = true;
        notice = error;
    } else if saves_enabled
        && !options.new_game
        && store.latest().is_none()
        && (store.exists(Slot::Quick) || store.exists(Slot::Auto))
    {
        hud.announce("A saved game could not be read. F9 retries quick load.");
    }
    startup_memory.destination(&maps[current]);
    startup_memory.finish(true);
    retry.remember(game_snapshot!());
    #[cfg(target_os = "vita")]
    let mut profile_report_due = false;
    // A transition owns only a compact gameplay checkpoint while constructing its
    // destination. In particular no outgoing World/SAH, scene buffers, NPC models,
    // level art, particles or decoded level PCM survive into the build.
    macro_rules! unload_scene {
        () => {{
            // The recycle slot shares the outgoing World geometry. Release it
            // before budgeting the destination build.
            if sim_overlap { drop(acoustic_view.take()); }
            crate::render_fx::release_scene();
            let resources = (level_art, npcs, steam, Loaded { scene, world });
            if crate::transition_memory::legacy_overlap() { Some(resources) } else {
                drop(resources);
                audio.release_scene();
                None
            }
        }};
    }
    macro_rules! rollback_resources {
        ($checkpoint:expr, $outgoing:expr, $particles:expr, $audio:expr, $reload_alice:expr, $guards:ident) => {{
            if let Some((art, cast, particles, level)) = $outgoing.take() {
                level_art = art; npcs = cast; steam = particles;
                Loaded { scene, world } = level;
                None
            } else {
                // This is a live resource checkpoint, not an external saved game.
                // Dead, airborne and scripted frames must not pass save validators
                // or migrate/reset their untouched live controllers and clocks.
                let Some(rebuilt) = owner_yield!($guards, Box::pin(rebuild_outgoing(&mut assets, &$checkpoint,
                    &interactions, stats.difficulty, &options, $reload_alice,
                    &mut escape_menu, &ui, &preferences))) else {
                    mouse_look.release();
                    // Transition's drop guard closes this operation as failed.
                    return Ok(());
                };
                let (art, cast, mut particles, level, character) = *rebuilt;
                particles.restore_runtime($particles);
                level_art = art; npcs = cast; steam = particles;
                Loaded { scene, world } = level;
                audio.reload_failed_transition(&mut assets, &maps[current], &scene.map);
                audio.restore_transition($audio);
                character
            }
        }};
    }
    macro_rules! restore_transition {
        ($game:expr, $kind:expr, $pause:expr, $install:ident, $guards:ident) => {{
            let incoming = $game;
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            let incoming = {
                let mut incoming = incoming;
                if std::env::var("LOOKING_GLASS_TRANSITION_FAIL_BUILD").as_deref() == Ok("invalid-level") {
                    incoming.current = "missing-live-load-fixture".into();
                }
                incoming
            };
            // Invalid level identities are ordinary load errors, before release.
            match incoming.level().map(|level| level.map.clone()) {
              Err(error) => Err(error),
              Ok(destination) => {
                let checkpoint = game_snapshot!();
                let particles = steam.runtime();
                let audio_checkpoint = audio.transition_checkpoint();
                let mut memory = crate::transition_memory::Transition::begin($kind, &maps[current], &destination);
                let _loading = crate::loading_screen::begin($kind, &destination);
                // The destination build, its install and any rollback share one
                // cache window; the guard clears it on success and every error.
                let _share = crate::texture::load_scope();
                let mut outgoing = unload_scene!();
                let outgoing_alice = if crate::transition_memory::legacy_overlap() { Some(alice) } else { drop(alice); None };
                owner_yield!($guards, crate::loading_screen::released());
                memory.after_release();
                let result = match owner_yield!($guards, Box::pin(save::Restored::build_async(&mut assets, incoming))) {
                    Ok(restored) => {
                        apply_loaded!(restored, $pause, $install, $guards);
                        Ok(())
                    }
                    Err(error) => {
                        let character = rollback_resources!(checkpoint, outgoing, particles, audio_checkpoint, true, $guards);
                        alice = if let Some(alice) = outgoing_alice { alice } else {
                            character.expect("successful outgoing rebuild includes Alice")
                        };
                        world.set_weapon_obstacles(alice.ice_targets());
                        Err(error)
                    }
                };
                drop(outgoing);
                memory.finish(result.is_ok());
                result
              }
            }
        }};
    }
    use crate::sim_pipeline::{Frame as StampFrame, Stamp};
    let mut frame_stamp: Stamp;
    let mut stored_replay_probe: Option<(Stamp, crate::replay::OwnedProbe, &'static str, bool)> = None;
    #[cfg(not(target_os = "vita"))]
    let mut replay_probe_fallbacks = 0u64;
    #[cfg(not(target_os = "vita"))]
    let mut previous_step_exit = "before-first-step";
    #[cfg(not(target_os = "vita"))]
    let mut replay_probe_assertions = 0u64;
    macro_rules! replay_probe {
        ($want_state:expr) => {{
            step::capture_replay_probe!(maps[current].as_str(), $want_state, player, stats,
                interactions, npcs, story, hints, alice, environment_clock, pickup_clock, yaw, pitch, clock)
        }};
    }
    macro_rules! finish_replay_step {
        ($path:literal) => {
            if let Some(r) = replay.as_ref().filter(|r| r.running()) {
                #[cfg(not(target_os = "vita"))]
                { previous_step_exit = $path; }
                let due = r.due();
                // Host builds compare the lightweight probe even between tape checks.
                if cfg!(not(target_os = "vita")) || due.is_some() {
                    assert!(stored_replay_probe.is_none(), "duplicate replay probe for one step");
                    stored_replay_probe = Some((frame_stamp, replay_probe!(due.unwrap_or(false)), $path, false));
                }
            }
        };
    }
    let mut overlap_probe = crate::overlap_probe::Probe::configured();
    // Early exits and post-HUD saves leave their notices for the next HUD block.
    let mut hud_ops = Vec::new();
    let mut audio_step = AudioStep::default();
    macro_rules! flush_audio {
        ($barrier:literal) => {
            audio_step.apply_at_barrier($barrier, &mut audio, &mut assets, &world)
        };
    }
    let mut art_ops = Vec::new();
    let mut pending: Option<lag_frame::Frame> = None;
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    let mut pending_probe = crate::sim_maps::probe::StepRows::default();
    #[cfg(feature = "vita_renderer")]
    let lag_maps = &crate::render::vita_renderer::config().sim_overlap_maps[..];
    #[cfg(not(feature = "vita_renderer"))]
    let lag_maps: &[String] = &[];
    #[cfg(not(target_os = "vita"))]
    let fence_every = std::env::var("LOOKING_GLASS_SIM_FENCE_EVERY")
        .ok().map(|s| s.parse::<u64>()).transpose()?.unwrap_or(0);
    #[cfg(target_os = "vita")]
    let fence_every = 0;
    let mut last_present: Option<u64> = None;
    let mut forced_flush;
    let lag_control = lag_control::Controls::read()?;
    let mut render_peer: Option<lag_control::Peer> = None;

    // An owner transaction can flush at its original call site. The tape
    // sample and the latched input are untouched by the extra framework epoch.
    macro_rules! render_pending {
        ($next_frame:expr, $alice:expr, $npcs:expr, $steam:expr) => {
            if let Some(image) = pending.take() {
                let stamp = image.stamp;
                assert_eq!(stamp.frame.visit_id, level_art.snapshot_visit.id(), "stale pending visit");
                assert_eq!(Some(stamp.frame.frame), last_present.map_or(Some(0), |n| n.checked_add(1)),
                    "presented frame sequence is not contiguous");
                let map = &maps[current];
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                let mut next_probe = crate::sim_maps::probe::StepRows::default();
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                {
                    next_probe.swap();
                    pending_probe.swap();
                    crate::sim_maps::probe::set_step_frame(stamp.frame.frame);
                }
                let report_due = image.present(lag_frame::Art {
                    scene: &mut scene, level: &mut level_art, alice: $alice, npcs: $npcs,
                    steam: $steam, power: &mut power_art, aim: &mut aim_pointer, handoff: &mut handoff_return,
                    overlap_probe: &mut overlap_probe, hud: &mut hud, pickups: &pickup_input, waves: &wave_input,
                }, &ui, &catalog, &mut save_frame, &mut sim_trace, map, render_peer.take(),
                    lag_control, &sim_audit).await?;
                #[cfg(target_os = "vita")]
                { profile_report_due = report_due; }
                #[cfg(not(target_os = "vita"))]
                let _ = report_due;
                sim_audit.event("present", stamp);
                last_present = Some(stamp.frame.frame);
                audio.scene_presented();
                assert!(handoff_return.consume(&level_art.snapshot_visit), "missing pending handoff completion");
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                {
                    crate::sim_maps::probe::set_frame($next_frame);
                    crate::sim_maps::probe::begin();
                    next_probe.swap();
                }
                #[cfg(target_os = "vita")]
                crate::frame_profile::begin_frame(map);
                crate::frame_profile::begin_viewer_frame();
                crate::frame_phases::begin();
                crate::spike_log::begin_frame();
                crate::light_profile::cpu_begin();
            }
        };
    }
    macro_rules! flush_pending {
        ($frame:expr, $guards:ident, $input:ident) => {
            if pending.is_some() {
                forced_flush = true;
                let restore_clock = $guards.take().is_some();
                render_pending!($frame, alice.art_mut(), npcs.art_mut(), steam.art());
                if restore_clock {
                    $guards = Some((StepGuard::enter(), $input.enter_clock()));
                }
            }
        };
    }
    macro_rules! take_step {
        ($locals:ident, $stamp:expr) => {{
            let (alice_sim, alice_art) = alice.into_parts();
            let (npcs_sim, npcs_art) = npcs.into_parts();
            let (steam_sim, steam_art) = steam.into_parts();
            let sim = Box::new(SimState {
                world,
                interactions,
                alice: alice_sim,
                npcs: npcs_sim,
                steam: steam_sim,
                input,
                weapon_buttons,
                pickup_feedback,
                third_person,
                follow_camera,
                camera_handoff,
                rail_camera,
                clock,
                recovery,
                retry,
                paused,
                post_game,
                skip_scene,
                hints,
                entry_spawn,
                stats,
                pickups,
                pickup_clock,
                environment_clock,
                story,
                pos,
                yaw,
                pitch,
                flying,
                spawn_landing,
                player,
                campaign,
                played,
            });
            let job = StepJob {
                stamp: $stamp, sim, locals: $locals,
                art_ops, hud_ops, audio_step, console: ConsoleOps::default(),
                #[cfg(feature = "vita_renderer")]
                route_control: route_session.as_mut().map(crate::route_replay::Session::take_control),
            };
            (job, (alice_art, npcs_art, steam_art))
        }};
    }
    macro_rules! restore_step {
        ($job:expr, $art:expr, $locals:ident) => {{
            let StepJob {
                stamp: _, sim: mut state, locals, art_ops: ops, hud_ops: hud, audio_step: audio,
                console: console_ops,
                #[cfg(feature = "vita_renderer")]
                route_control,
            } = $job;
            $locals = locals;
            art_ops = ops;
            hud_ops = hud;
            audio_step = audio;
            #[cfg(feature = "vita_renderer")]
            if let Some(control) = route_control {
                route_session.as_mut().unwrap().restore_control(control);
            }
            let (mut alice_art, npcs_art, steam_art) = $art;
            for op in art_ops.drain(..) {
                if let Some(body) = op.apply(&mut alice_art, &mut power_art) {
                    state.alice.recycle_body_input(body);
                }
            }
            alice = Character::from_parts(state.alice, alice_art);
            npcs = crate::npc::Npcs::from_parts(state.npcs, npcs_art);
            steam = crate::particles::Steam::from_parts(state.steam, steam_art);
            world = state.world;
            interactions = state.interactions;
            input = state.input;
            weapon_buttons = state.weapon_buttons;
            pickup_feedback = state.pickup_feedback;
            third_person = state.third_person;
            follow_camera = state.follow_camera;
            camera_handoff = state.camera_handoff;
            rail_camera = state.rail_camera;
            clock = state.clock;
            recovery = state.recovery;
            retry = state.retry;
            paused = state.paused;
            post_game = state.post_game;
            skip_scene = state.skip_scene;
            hints = state.hints;
            entry_spawn = state.entry_spawn;
            stats = state.stats;
            pickups = state.pickups;
            pickup_clock = state.pickup_clock;
            environment_clock = state.environment_clock;
            story = state.story;
            pos = state.pos;
            yaw = state.yaw;
            pitch = state.pitch;
            flying = state.flying;
            spawn_landing = state.spawn_landing;
            player = state.player;
            campaign = state.campaign;
            played = state.played;
            console_ops
        }};
    }
    macro_rules! run_step {
        ($function:ident, $locals:ident, $scopes:ident, $step_input:expr,
            $stamp:expr, $worker:expr, $clock:expr, $cut:ident, $scope:ident,
            $send:expr, $reply:pat => $value:expr $(,)?) => {{
            let (job, art) = take_step!($locals, $stamp);
            let step_input = $step_input;
            let (job, result) = if let Some(worker) = $worker {
                let (job, result) = worker.run(job, $clock, $send(step_input), &mut sim_trace)?;
                let $reply = result else { unreachable!("unexpected sim worker reply"); };
                (job, $value)
            } else {
                let started = sim_trace.mark(job.stamp.frame.frame);
                let (job, result) = $function(job, step_input, &mut $scopes);
                sim_trace.owner(job.stamp, sim_timing::Scope::$scope, Some(sim_timing::Cut::$cut), started);
                (job, result)
            };
            let console_ops = restore_step!(job, art, $locals);
            (result, console_ops)
        }};
    }
    loop {
        forced_flush = false;
        let head_started = sim_trace.mark(frame);
        let lag_map = lag_maps.iter().any(|name| name == &maps[current])
            && (live_chapter || replay.as_ref().is_some_and(|r| r.running() && r.replaying()));
        #[cfg(feature = "native_timing")]
        let lag_map = lag_map && timing.recorder.is_none();
        debug_assert!(audio_step.ops.is_empty());
        // The tape oracle uses release builds. Check the same invariant there.
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        assert!(audio_step.ops.is_empty());
        audio_step.frame = frame;
        #[cfg(feature="vita_renderer")]
        let snapshot_handoff = crate::render::vita_renderer::config().diag_snapshot || lag_map;
        #[cfg(not(feature="vita_renderer"))]
        let snapshot_handoff = false;
        if snapshot_handoff {
            // All frames remain serial fences. A missing exact completion is a
            // protocol error; never advance simulation using a replacement pose.
            assert!(handoff_return.consume(&level_art.snapshot_visit), "cinematic render completion missing");
        } else { handoff_return.retire(); }
        #[cfg(feature = "native_timing")]
        { timing.poll_schedule_review(); timing.with(|r| r.begin(timing_state!())); }
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        crate::phase_census::begin_configured();
        let mut census_block = crate::phase_census::span(crate::phase_census::Region::Input);
        if pending.is_none() {
            crate::frame_profile::begin_viewer_frame();
            crate::frame_phases::begin();
            crate::spike_log::begin_frame();
        }
        #[cfg(feature="vita_renderer")]
        let route_running=route_session.is_some();
        #[cfg(not(feature="vita_renderer"))]
        let route_running=false;
        #[cfg(feature="vita_renderer")]
        if let Some(route)=&mut route_session {
            let village=interactions.village.as_ref().context("route requires village cinema")?;
            route.begin_frame(crate::route_replay::Status{map:maps[current].clone(),entry_done:village.cinema.state.done.contains("entry"),knife_done:village.cinema.state.done.contains("knife_cat"),blade_copies:stats.copies(0) as u32,alive:stats.alive(),flying,feet:player.feet.to_array()})?;
            if route.entry_checkpoint_due(){store.write(save::Slot::parse("slot1")?,&game_snapshot!())?;println!("[lg/route] recorded entry checkpoint");}
            if route.should_finish() {
                route.finish(&store,&game_snapshot!(),yaw,pitch)?;
                crate::frame_profile::report(&maps[current])?;
                flush_audio!("route-end");
                break;
            }
            paused=false;
        }
        // One tape step per viewer frame, before any input or frame time is read.
        let mut first_replay_step = false;
        if let Some(r) = &mut replay {
            if r.wants_aim() {
                let wrap = |a: f32| (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                r.aim(npcs.targets().iter().map(|t| {
                    let d = t.center - player.feet;
                    (wrap(d.y.atan2(d.x) - yaw), d.truncate().length())
                }).min_by(|a, b| a.1.total_cmp(&b.1)));
            }
            let due = r.due();
            let probe = if let Some((stamp, probe, _path, worker_seal)) = stored_replay_probe.take() {
                assert_eq!(stamp.frame.tape_step, r.current_step(), "stale replay seal stamp");
                #[cfg(not(target_os = "vita"))]
                {
                    let top = replay_probe!(due.unwrap_or(false));
                    probe.assert_same_state(&top, frame, r.current_step(), _path);
                    replay_probe_assertions += 1;
                }
                if worker_seal { sim_audit.seal_consumed(stamp, due.is_some()); }
                due.map(|_| probe)
            } else {
                #[cfg(not(target_os = "vita"))]
                if r.running() {
                    let step = r.current_step();
                    assert_eq!(step, 0,
                        "missing replay step seal: frame={frame} step={step} previous_exit={previous_step_exit}");
                    replay_probe_fallbacks += 1;
                    println!("[lg/replay-probe] probe.fallback={replay_probe_fallbacks} frame={frame} step={step} path=initial");
                }
                match due {
                    Some(want_state) => Some(replay_probe!(want_state)),
                    None => None,
                }
            };
            match r.begin_step(&maps[current], stats.alive(), probe) {
                crate::replay::Step::Run { first: true } => {
                    // Same input state in a recording and its replay, whatever came before.
                    first_replay_step = true;
                    input = crate::input::Input::default();
                    input.suppress();
                }
                crate::replay::Step::Run { .. } | crate::replay::Step::Off => {}
                crate::replay::Step::Finish => {
                    replay_end = Some(if r.replaying() { "tape-end" } else { "step-limit" });
                    render_pending!(frame, alice.art_mut(), npcs.art_mut(), steam.art());
                    flush_audio!("replay-end");
                    break;
                }
            }
        }
        frame_stamp = Stamp {
            frame: StampFrame {
                frame,
                tape_step: replay.as_ref().map_or(0, |r| r.current_step()),
                visit_id: level_art.snapshot_visit.id(),
            },
            slot_generation: 0,
        };
        #[cfg(not(target_os = "vita"))]
        if replay.as_ref().is_some_and(|r| r.running()) {
            previous_step_exit = "unmarked";
        }
        #[cfg(not(target_os="vita"))]
        if pending.is_none() { crate::frame_profile::begin_creation_frame(); }
        #[cfg(target_os = "vita")]
        {
            // Synthetic companion input does not reset the screen/idle timers.
            unsafe { sceKernelPowerTick(SCE_KERNEL_POWER_TICK_DEFAULT) };
            if pending.is_none() { crate::frame_profile::begin_frame(&maps[current]); }
            if profile_report_due {
                // The next begin_frame completes the last measured wall interval.
                // Exit before simulating or drawing an extra gameplay frame.
                profile_report_due = false;
                crate::frame_phases::mark(7);
                let report = crate::frame_profile::report(&maps[current]);
                crate::frame_phases::mark(8);
                let mode = crate::frame_profile::mode();
                if mode.is_bench() && crate::frame_profile::bench_frames() == Some(120) {
                    if let Err(e) = report {
                        eprintln!("bench: report failed ({e:#})");
                    } else {
                        if crate::light_profile::full() {
                            println!("bench: done 120 frames mode={}, report ux0:data/alice/private/frame_profile/{}-{}-120.json", mode.name(), maps[current], mode.name());
                        } else {println!("bench: done 120 frames mode={} profiler=light", mode.name());}
                    }
                    finish_replay_step!("benchmark");
                    flush_audio!("benchmark-end");
                    break;
                }
                if let Err(e) = report { eprintln!("Frame profile report failed: {e:#}"); }
            }
        }
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        {
            if pending.is_none() {
                crate::sim_maps::probe::set_frame(frame);
                crate::sim_maps::probe::begin();
            } else {
                crate::sim_maps::probe::begin_worker_step(frame);
            }
            crate::sim_maps::probe::script_hits(frame, &mut npcs);
        }
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        let mut host_block = crate::frame_profile::span("frame_input");
        #[cfg(feature = "native_timing")]
        timing.with(|r| r.cpu_begin());
        if pending.is_none() { crate::light_profile::cpu_begin(); }
        if !(menu || inventory_menu || console.open || paused) {crate::character::skinning_bench_begin();}
        crate::frame_phases::mark(1);
        let mut frame_profile = crate::frame_profile::span("frame_cpu");
        #[cfg(target_os = "vita")]
        let mut viewer_block = crate::frame_profile::span("frame_input");

        #[cfg(target_os = "vita")]
        let bench = crate::frame_profile::mode().is_bench();
        #[cfg(not(target_os = "vita"))]
        let bench = false;
        #[cfg(target_os = "vita")]
        {
            bench_session |= bench;
            if crate::frame_profile::bench_accepted_frames().is_none_or(|n| n == 0) {
                bench_camera = None;
            }
        }
        #[cfg(target_os = "vita")]
        let saves_writable = crate::frame_profile::saves_writable(saves_enabled, bench_session);
        #[cfg(not(target_os = "vita"))]
        let saves_writable = saves_enabled;
        #[cfg(target_os = "vita")]
        let simulate = !crate::frame_profile::simulation_frozen() && !r13_survey.as_ref().is_some_and(|s|s.done);
        #[cfg(not(target_os = "vita"))]
        let simulate = true;
        // The frontend remains interactive if the reference save could not load.
        // Inside the bench, retain physics with neutral input, including sticks.
        let mut step_guards = None;
        let mut input_started = sim_trace.mark(frame);
        let mut step_input = input.capture_step(
            replay.as_ref().filter(|r| r.running()).map_or(frame, |r| r.current_step() - 1),
            physical_input && window_focused() && !bench && !route_running,
        );
        input.apply_step(&step_input, &preferences);
        #[cfg(feature="native_timing")]
        timing.with(|r| {if r.schedule.is_some(){r.schedule_input(input.diagnostic_activity());}});
        preview_throttle.advance_frame();
        let host_transition = crate::transition_memory::host_action(frame);
        if !host_transition.is_empty() {
            flush_pending!(frame, step_guards, step_input);
        }
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        if host_transition.is_empty() && std::env::var_os("LOOKING_GLASS_TRANSITION_RESUME").is_some() { paused = false; }
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        if frame == 1 {
            if let Some(path) = std::env::var_os("LOOKING_GLASS_TRANSITION_PORTAL_CAPTURE") {
                owner_yield!(step_guards,
                    scene.show_transition_portal(&world, std::path::Path::new(&path)))?;
            }
            if let Ok(path) = std::env::var("LOOKING_GLASS_TRANSITION_SNAPSHOT") {
                std::fs::write(path, serde_json::to_vec(&game_snapshot!())?)?;
                store.write(Slot::Quick, &game_snapshot!())?;
            }
        }
        if host_transition == "chapter" {
            flush_pending!(frame, step_guards, step_input);
            let destination = crate::transition_memory::host_destination();
            let choice = level_choices.iter().find(|c| maps[c.map] == destination).context("probe destination")?;
            chapter_requested = Some(crate::campaign::LevelChoice { map: choice.map, entry: choice.entry, title: choice.title.clone() });
        }
        let overlay_before = menu || inventory_menu || console.open || paused;
        let context_before = (menu, inventory_menu, console.open, paused);
        if input.disconnected {
            paused = true;
            hud_ops.push(HudOp::Announce("Controller disconnected / reconnect or use keyboard and mouse".into()));
        }
        let pause_before_capture = paused;
        #[cfg(feature = "vita_renderer")]
        let throttle_normal = crate::render::vita_renderer::config().save_preview == "throttled";
        #[cfg(not(feature = "vita_renderer"))]
        let throttle_normal = false;
        #[cfg(target_os = "vita")]
        let throttle_normal = throttle_normal && crate::frame_profile::mode() == crate::frame_profile::Mode::Full;
        let mut quit_after_frame = is_quit_requested()
            || (replay.as_ref().is_some_and(|r| r.replaying()) && macroquad::input::is_quit_requested());
        quit_after_frame |= host_transition == "quit" || lag_control.quit == Some(frame);
        if lag_control.quit == Some(frame) || (quit_after_frame && !throttle_normal) {
            flush_pending!(frame, step_guards, step_input);
            finish_replay_step!("quit");
            flush_audio!("quit");
            break;
        }
        if physical_input && window_focused() && !console.open && !bench {
            if (is_key_pressed(KeyCode::Escape) || input.pad_pressed("Start"))
                && !menu
                && !inventory_menu
            {
                flush_pending!(frame, step_guards, step_input);
                flush_audio!("escape-open");
                escape_menu.open(crate::menu::Page::Main, &preferences, audio.settings);
            } else if is_key_pressed(KeyCode::F3) {
                flush_pending!(frame, step_guards, step_input);
                menu = false;
                inventory_menu = false;
                flush_audio!("escape-open");
                escape_menu.open(crate::menu::Page::Audio, &preferences, audio.settings);
            }
        }
        // Start ends a recording; the menu then runs on the live pad. A replay never opens it.
        if escape_menu.page.is_some() {
            flush_pending!(frame, step_guards, step_input);
            if let Some(r) = replay.as_mut().filter(|r| r.running()) {
                r.stop("menu");
                if r.replaying() { flush_audio!("menu-replay-end"); break; }
                input.refresh_controls();
            }
        }
        // A modal menu owns its own frames. Render one frozen clean world frame
        // before entering it, so thumbnails never capture the menu or old play.
        let opening_escape_menu = throttle_normal && escape_menu.page.is_some() && !escape_preview_ready;
        if opening_escape_menu { paused = true; clock.pause(); }
        if (escape_menu.page.is_some() && (!throttle_normal || escape_preview_ready)) || matches!(host_transition.as_str(), "native-load" | "new-game" | "new-game+retry" | "new-game+quick-load") {
            #[cfg(feature = "native_timing")]
            timing.with(|r| {
                r.discontinuity(crate::native_timing::Milestone::ModalEnter, crate::native_timing::MODAL | crate::native_timing::PAUSED);
                r.request_stop(); // Seal producer before entering any blocking modal loop.
            });
            escape_preview_ready = false;
            weapon_buttons.block();
            input.suppress();
            mouse_look.release();
            clock.pause();
            help_until = 0.;
            #[cfg(target_os = "vita")]
            crate::frame_profile::log_report_error(crate::frame_profile::report(&maps[current]));
            // Consume Escape before entering the separate modal frame loop. No
            // physics, story, enemies, power-up timers or world clocks run there.
            drop(census_block);
            flush_audio!("escape-modal");
            owner_yield!(step_guards, next_frame());
            census_block=crate::phase_census::span(crate::phase_census::Region::Other);
            let action = if host_transition == "native-load" { crate::menu::Action::Load(Slot::Auto) }
                else if host_transition.starts_with("new-game") { crate::menu::Action::NewGame(options.difficulty) } else {
                owner_yield!(step_guards, escape_menu.run(std::sync::Arc::make_mut(&mut preferences), &mut audio, &store, pos, yaw, &mut input))
            };
            #[cfg(feature = "native_timing")]
            timing.with(|r| r.discontinuity(crate::native_timing::Milestone::ModalExit, crate::native_timing::MODAL));
            use crate::menu::Action;
            match action {
                Action::Resume => {
                    paused = false;
                }
                Action::Quit => { if throttle_normal { quit_after_frame = true; } else { flush_audio!("escape-quit"); break; } },
                Action::Chapters => {
                    flush_audio!("chapter-picker");
                    match owner_yield!(step_guards, escape_menu
                        .choose_chapter(
                            &maps,
                            current,
                            entry_spawn.as_deref(),
                            &mut options.difficulty,
                            &preferences,
                            &mut audio,
                            &mut input,
                        ))
                    {
                        crate::menu::ChapterChoice::Begin(choice) => {
                            chapter_requested = Some(choice);
                            escape_menu.close();
                        }
                        crate::menu::ChapterChoice::Quit => { if throttle_normal { quit_after_frame = true; } else { flush_audio!("chapter-picker-quit"); break; } },
                        crate::menu::ChapterChoice::Back => (),
                    }
                }
                Action::Save(slot) => {
                    if throttle_normal && saves_writable && stats.alive() {
                        pending_menu_save = Some(slot);
                    } else {
                        escape_menu.message = if !saves_writable {
                            "Saving is unavailable in a staged preview".into()
                        } else if !stats.alive() {
                            "Retry or load a game before saving".into()
                        } else {
                            match store.write_with_preview(
                                slot,
                                &game_snapshot!(),
                                save_frame.preview().as_ref(),
                            ) {
                                Ok(()) => {
                                    retry.remember(game_snapshot!());
                                    played = true;
                                    format!("Game saved in {}", slot.title())
                                }
                                Err(e) => format!("Save failed: {e:#}"),
                            }
                        };
                    }
                }
                Action::Load(slot) => {
                    #[cfg(feature = "native_timing")]
                    timing.with(|r| r.discontinuity(crate::native_timing::Milestone::LoadAttempt, crate::native_timing::OPERATION));
                    if saves_enabled {
                        match store
                            .read(slot)
                            .map(|loaded| loaded.game)
                        {
                            Ok(game) => match restore_transition!(game, "native-load", true, replace, step_guards) {
                              Ok(()) => {
                                escape_menu.close();
                                hud_ops.push(HudOp::Announce("Game restored / P or click to resume".into()));
                              }
                              Err(e) => {
                                #[cfg(feature = "native_timing")]
                                timing.with(|r| r.event(crate::native_timing::Milestone::LoadFailed, crate::native_timing::OPERATION));
                                escape_menu.message = format!("Load failed: {e:#}");
                                eprintln!("Load failed: {e:#}");
                              }
                            },
                            Err(e) => {
                                #[cfg(feature = "native_timing")]
                                timing.with(|r| r.event(crate::native_timing::Milestone::LoadFailed, crate::native_timing::OPERATION));
                                escape_menu.message = format!("Load failed: {e:#}");
                            },
                        }
                    } else {
                        escape_menu.message = "Loading is unavailable in a staged preview".into();
                    }
                }
                Action::NewGame(difficulty) => {
                    if throttle_normal { pending_menu_newgame = Some(difficulty); } else {
                        // Preserve the current session before a new campaign replaces it.
                        let saved = if saves_writable && stats.alive() && !post_game {
                            store.write_with_preview(
                                Slot::Auto,
                                &game_snapshot!(),
                                save_frame.preview().as_ref(),
                            )
                        } else {
                            Ok(())
                        };
                        if !saves_enabled {
                            escape_menu.open(crate::menu::Page::Main, &preferences, audio.settings);
                            escape_menu.message = "New Game is unavailable in a staged preview".into();
                        } else if let Err(e) = saved {
                            escape_menu.open(crate::menu::Page::Main, &preferences, audio.settings);
                            escape_menu.message = format!("Could not preserve the current game: {e:#}");
                        } else {
                            post_game = false;
                            escape_menu.post_game = false;
                            new_game_requested = Some(difficulty);
                        }
                    }
                }
            }
            clock.pause();
            input.suppress();
            if pending_menu_save.is_none() && pending_menu_newgame.is_none() && !quit_after_frame {
                flush_audio!("pause-continue");
                finish_replay_step!("pause");
                drop(census_block);
            next_frame().await;
            census_block=crate::phase_census::span(crate::phase_census::Region::Other);
                continue;
            }
        }
        let freeze_capture = opening_escape_menu || pending_menu_save.is_some() || pending_menu_newgame.is_some() || quit_after_frame;
        if freeze_capture { paused = true; clock.pause(); }
        let elapsed = source_frame_time();
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        let elapsed = crate::sim_maps::probe::elapsed(elapsed);
        #[cfg(all(feature="vita_renderer",not(target_os="vita")))]
        let elapsed=if let Some(r)=&mut route_session {(elapsed-r.recording_pause_seconds()).max(0.)}else{elapsed};
        let mut weapon_click;
        let mut weapon_input_enabled = false;
        let pending_transition = None;
        let advance_dialogue = false;
        let dt = if simulate { elapsed.clamp(0., 0.05) } else { 0. };
        let world_dt = stats.powers.world_dt(dt);
        #[cfg(feature = "native_timing")]
        timing.with(|r| r.simulation(elapsed, dt, world_dt));
        let power_before = stats.active_power().map(|(k, _)| k);
        let loot_visit = save::visit_key(&maps[current], entry_spawn.as_deref());
        let mut loot_before = interactions.loot_sources();
        loot_before.extend(npcs.loot_sources());
        let focused = window_focused() || route_running;
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        let focused = focused || crate::sim_maps::probe::enabled();
        if !focused && was_focused {
            paused = true;
            clock.pause();
            // A key released in another app may never send this window a key-up.
            // Ignore it until released here or explicitly pressed again.
            input.suppress();
        }
        was_focused = focused;
        // Macroquad 0.4.14 pops its character Vec from the end. Restore event
        // order when typing/pasting delivers several characters in one frame.
        let mut chars = std::iter::from_fn(get_char_pressed).collect::<Vec<_>>();
        chars.reverse();
        let toggle_console = physical_input && focused && !bench
            && (is_key_pressed(KeyCode::GraveAccent)
                || chars.iter().any(|c| matches!(c, '`' | '~' | '\u{00ac}')));
        let console_input = console.open || toggle_console;
        let mut console_map = if let Some(choice) = chapter_requested.take() {
            selected = choice.map;
            selected_entry = choice.entry.map(str::to_owned);
            post_game = false;
            escape_menu.post_game = false;
            true
        } else { false };
        let mut new_campaign_memory = None;
        if let Some(difficulty) = new_game_requested.take() {
            // The opening movie belongs to this operation too. Keep its bounded
            // decode/upload allocations inside the same before/peak/after bracket.
            new_campaign_memory = Some(crate::transition_memory::Transition::begin(
                "new-game", &maps[current], "gvillage"));
            #[cfg(feature = "native_timing")]
            timing.with(|r| r.discontinuity(crate::native_timing::Milestone::OpeningEnter, crate::native_timing::CINEMA));
            flush_pending!(frame, step_guards, step_input);
            flush_audio!("opening-film");
            match owner_yield!(step_guards, crate::movie::play(
                &mut assets,
                &mut audio,
                &ui,
                &mut input,
                &preferences,
                "opening",
            )) {
                Ok(false) => {
                    flush_audio!("opening-quit");
                    finish_replay_step!("opening-quit");
                    break;
                }
                Err(e) => {
                    eprintln!("Opening film unavailable: {e:#}");
                    hud_ops.push(HudOp::Announce("Opening film unavailable / continuing to Wonderland".into()));
                }
                Ok(true) => (),
            }
            clock.pause();
            #[cfg(feature = "native_timing")]
            timing.with(|r| r.discontinuity(crate::native_timing::Milestone::OpeningExit, crate::native_timing::CINEMA));
            selected = maps
                .iter()
                .position(|name| name == "gvillage")
                .context("Campaign opening map missing")?;
            selected_entry = None;
            options.difficulty = difficulty;
            options.fly = false;
            console_map = true;
        }
        let mut console_restart = false;
        let mut console_fly = false;
        let mut summon_cat = false;
        let mut save_requested =
            focused && !console_input && input.key(&preferences, KeyCode::F5, true);
        let mut load_requested =
            (focused && !console_input && input.key(&preferences, KeyCode::F9, true))
                .then_some(Slot::Quick);
        if matches!(host_transition.as_str(), "quick-load" | "new-game+quick-load") { load_requested = Some(Slot::Quick); }
        let mut autosave_requested = false;
        let mut finale_checkpoint_requested = None;
        if toggle_console || (console.open && focused && input.ui(KeyCode::Escape)) {
            console.open = !console.open;
            menu = false;
            inventory_menu = false;
            help_until = 0.;
            clock.pause();
            input.suppress();
        }
        if console.open && focused {
            if let Some(command) = console.input(&chars) {
                flush_audio!("console");
                use crate::console::Command;
                match command {
                    Err(message) => console.print(message),
                    Ok(Command::Help) => {
                        console.print("help/cmdlist  clear  version  status/getpos  maplist");
                        console.print(
                            "map <level[$entrance]>  restart  noclip  god  notarget  fullbright",
                        );
                        console.print("wuss  give all|weapons|health|will|<item.tik>  itemlist");
                        console.print("health <number>  cg_cameradist -45 (first person) / 128 (third person)");
                        console.print("cheshire  quit");
                        console
                            .print("save / F5  quick save    load / F9  quick load    load auto");
                        console.print("Map starts a fresh visit. While alive, restart recovers the entrance. After death, retry restores the last save or visit start.");
                        console.print("Saves retain cheat settings. Unsupported original commands are not executed.");
                    }
                    Ok(Command::Clear) => console.clear(),
                    Ok(Command::Version) => console.print(format!(
                        "Looking Glass {} / independent Rust runtime",
                        env!("CARGO_PKG_VERSION")
                    )),
                    Ok(Command::Status) => {
                        console.print(format!(
                            "{} {:?} / feet {:.2} {:.2} {:.2} / yaw {:.1} pitch {:.1}",
                            maps[current],
                            entry_spawn,
                            player.feet.x,
                            player.feet.y,
                            player.feet.z,
                            yaw.to_degrees(),
                            pitch.to_degrees()
                        ));
                        console.print(format!(
                            "Sanity {:.0} / Will {:.0} / god {} / notarget {} / noclip {} / grounded {}",
                            stats.sanity(),
                            stats.will(),
                            stats.god,
                            stats.notarget,
                            flying,
                            player.grounded
                        ));
                        console.print(stats.weapon_summary());
                        console.print(format!(
                            "cg_cameradist {} / {}",
                            if third_person {
                                preferences.camera_distance
                            } else {
                                -45.
                            },
                            if third_person {
                                "third person"
                            } else {
                                "first person"
                            }
                        ));
                        console.print(format!(
                            "Difficulty {} / power {:?} / Watch {:.2}s / recharge {:.2}s",
                            stats.difficulty.name(),
                            stats.active_power(),
                            stats.powers.stopped,
                            stats.powers.recharge
                        ));
                        console.print(format!(
                            "Campaign: {} completed visits / {} cached visits",
                            campaign.completed.len(),
                            campaign.levels.len()
                        ));
                        if let Some(q) = &interactions.school2 {
                            console.print(q.quest.objective());
                        }
                        if let Some(q) = &interactions.school {
                            console.print(q.objective());
                        }
                    }
                    Ok(Command::Maps) => {
                        for chunk in maps.chunks(6) {
                            console.print(chunk.join("  "));
                        }
                    }
                    Ok(Command::Map(name, entry)) => {
                        if let Some(index) = maps.iter().position(|m| m == &name) {
                            let valid = if let Some(entrance) = &entry {
                                assets
                                    .read(&format!("maps/{name}.bsp"))
                                    .and_then(|b| crate::bsp::Bsp::parse(&b))
                                    .is_ok_and(|m| {
                                        m.entities
                                            .iter()
                                            .any(|e| e.get("targetname") == Some(entrance))
                                    })
                            } else {
                                true
                            };
                            if valid {
                                selected = index;
                                selected_entry = entry;
                                console_map = true;
                            } else {
                                console.print("That named entrance does not exist in this level.");
                            }
                        } else {
                            console.print("Unknown level. Use maplist.");
                        }
                    }
                    Ok(Command::Restart) => {
                        console_restart = true;
                        console.open = false;
                    }
                    Ok(Command::Save) => save_requested = true,
                    Ok(Command::Load(slot)) => load_requested = Some(slot),
                    Ok(Command::Noclip) => {
                        console_fly = true;
                    }
                    Ok(Command::God) => {
                        stats.god = !stats.god;
                        console.print(format!("God mode {}", if stats.god { "ON" } else { "OFF" }));
                    }
                    Ok(Command::Notarget) => {
                        stats.notarget = !stats.notarget;
                        console.print(format!(
                            "Notarget {}",
                            if stats.notarget { "ON" } else { "OFF" }
                        ));
                    }
                    Ok(Command::Health(value)) => {
                        let dead = !stats.alive();
                        match stats.set_health(value) {
                            Ok(()) => {
                                if dead && stats.alive() {
                                    alice.reset(&player, yaw);
                                    follow_camera.reset();
                                    camera_handoff.reset();
                                }
                                console.print(format!("Sanity {}", stats.sanity()));
                            }
                            Err(e) => console.print(e.to_string()),
                        }
                    }
                    Ok(Command::CameraDistance(value)) => {
                        if let Some(value) = value {
                            crate::console::gameplay::camera(
                                value,
                                &mut third_person,
                                std::sync::Arc::make_mut(&mut preferences),
                            );
                            follow_camera.reset();
                            camera_handoff.reset();
                            if let Err(e) = preferences.save() {
                                console.print(format!(
                                    "Camera applied; settings could not be saved: {e}"
                                ));
                            }
                        }
                        console.print(format!(
                            "cg_cameradist {} / {}",
                            if third_person {
                                preferences.camera_distance
                            } else {
                                -45.
                            },
                            if third_person {
                                "third person"
                            } else {
                                "first person"
                            }
                        ));
                    }
                    Ok(Command::Wuss) => {
                        match crate::console::gameplay::give(
                            "weapons", &assets, &catalog, &mut stats,
                        ) {
                            Ok(message) => console.print(message),
                            Err(e) => console.print(e.to_string()),
                        }
                    }
                    Ok(Command::Give(item)) => {
                        match crate::console::gameplay::give(&item, &assets, &catalog, &mut stats) {
                            Ok(message) => {
                                crate::console::gameplay::sync_inventory(&stats, &mut interactions);
                                stats.prepare_player(&mut player);
                                console.print(message);
                            }
                            Err(e) => console.print(e.to_string()),
                        }
                    }
                    Ok(Command::Items) => {
                        for row in crate::console::gameplay::items(&assets).chunks(3) {
                            console.print(row.join("  "));
                        }
                        console.print("Use give <filename>. Power-ups activate immediately; story items do not finish unrelated quest stages.");
                    }
                    Ok(Command::Cheshire) => {
                        summon_cat = true;
                    }
                    Ok(Command::Fullbright) => {
                        fullbright = !fullbright;
                        console.print(format!("Fullbright {fullbright}"));
                    }
                    Ok(Command::Quit) => {
                        if throttle_normal { quit_after_frame = true; }
                        else { finish_replay_step!("console-quit"); flush_audio!("console-quit"); break; }
                    },
                }
            }
        }
        // A confirmed New Game owns this iteration. Discard simultaneous F9 or
        // Retry input; neither may overwrite its selected destination/checkpoint.
        if new_campaign_memory.is_some() || pending_menu_newgame.is_some() { load_requested = None; }
        if let Some(slot) = load_requested {
            flush_pending!(frame, step_guards, step_input);
            // A load reads the slot as it is now, which a later replay cannot rely on.
            if let Some(r) = replay.as_mut().filter(|r| r.running()) {
                r.stop("load");
                if r.replaying() { flush_audio!("load-replay-end"); break; }
            }
            #[cfg(feature = "native_timing")]
            timing.with(|r| r.discontinuity(crate::native_timing::Milestone::LoadAttempt, crate::native_timing::OPERATION));
            if saves_enabled {
                let result = match store.read(slot) {
                    Ok(loaded) => {
                        let backup = loaded.backup;
                        flush_audio!("quick-or-console-load");
                        restore_transition!(loaded.game, "quick-or-console-load", true, replace, step_guards).map(|()| backup)
                    }
                    Err(error) => Err(error),
                };
                match result {
                    Ok(backup) => {
                        let message = if backup {
                            "Previous save restored / P or click to resume"
                        } else {
                            "Game restored / P or click to resume"
                        };
                        hud_ops.push(HudOp::Announce(message.into()));
                        console.print(message);
                    }
                    Err(e) => {
                        clock.pause();
                        paused = true;
                        #[cfg(feature = "native_timing")]
                        timing.with(|r| r.event(crate::native_timing::Milestone::LoadFailed, crate::native_timing::OPERATION));
                        let message = format!("Load failed: {e:#}");
                        hud_ops.push(HudOp::Announce(message.clone().into()));
                        console.print(&message);
                    }
                }
            } else {
                hud_ops.push(HudOp::Announce("Save/load is unavailable in a staged preview".into()));
            }
            flush_audio!("quick-or-console-load-continue");
            finish_replay_step!("quick-or-console-load");
            drop(census_block);
            next_frame().await;
            census_block=crate::phase_census::span(crate::phase_census::Region::Other);
            continue;
        }
        // Install the sampled clock before the owner head.
        let step = StepGuard::enter();
        step_input.refresh_clock();
        step_guards = Some((step, step_input.enter_clock()));
        macro_rules! pressed {
            ($key:expr) => { focused && !console_input && input.key(&preferences, $key, true) };
        }
        if input.ui(KeyCode::Escape) && !console_input {
            if inventory_menu {
                inventory_menu = false;
            } else if menu {
                menu = false;
            }
        }
        if pressed!(KeyCode::Tab) && !survey_owns_select {
            menu = !menu;
            inventory_menu = false;
            help_until = 0.0;
            chapters.open(
                crate::campaign::choice_position(&level_choices, current, entry_spawn.as_deref()),
                level_choices.len(),
            );
        }
        let inventory_opened = inventory_toggle(pressed!(KeyCode::I) && focused, menu, &mut inventory_menu);
        if inventory_opened || (!menu && pressed!(KeyCode::I) && focused) {
            menu = false;
            help_until = 0.;
            clock.pause();
        }
        let mut audio_changed = false;
        if pressed!(KeyCode::M) {
            flush_pending!(frame, step_guards, step_input);
            audio.settings.muted = !audio.settings.muted;
            audio_changed = true;
        }
        if audio_changed {
            if let Err(e) = audio.settings.save() {
                notice = format!("Could not save sound settings: {e}");
            }
        }
        if (pressed!(KeyCode::H) || pressed!(KeyCode::F1))
            && !menu
            && !inventory_menu
            && !console_input
        {
            help_until = if get_time() < help_until {
                0.0
            } else {
                get_time() + 8.0
            };
        }
        if pressed!(KeyCode::F2) {
            fullbright = !fullbright;
        }
        if pressed!(KeyCode::V) && !menu && !inventory_menu && !console_input {
            third_person = !third_person;
            follow_camera.reset();
            camera_handoff.reset();
        }
        if pressed!(KeyCode::P) && focused {
            paused = !paused;
            clock.pause();
        }
        let was_paused = paused;
        let pointer = input.pointer();
        if paused
            && focused
            && !menu
            && !inventory_menu
            && !console_input
            && (input.frame().mouse_button(MouseButton::Left, true) || input.pad_pressed("A"))
        {
            let (mx, my) = pointer.position;
            if mx >= 0. && my >= 0. && mx < pointer.width && my < pointer.height {
                paused = false;
                clock.pause();
            }
        }
        mouse_look.sensitivity = preferences.sensitivity;
        mouse_look.inverted = preferences.invert_mouse;
        let scripted_view = interactions.scripted()
            || interactions.duchess.as_ref().is_some_and(|d| d.cinematic())
            || interactions
                .school
                .as_ref()
                .and_then(|s| s.return_visit.as_ref())
                .is_some_and(|s| s.camera().is_some());
        if !scripted_view {
            player.script_facing = yaw;
        }
        mouse_look.update(
            physical_input && focused
                && !paused
                && !menu
                && !inventory_menu
                && !console_input
                && !scripted_view
                && !bench
                && !route_running
                && stats.alive(),
            pointer,
            &mut yaw,
            &mut pitch,
        );
        hud_ops.push(HudOp::Tick(dt));
        if focused && !console_input && stats.alive() && !menu && (inventory_menu || !paused) {
            let keys = [
                KeyCode::Key1,
                KeyCode::Key2,
                KeyCode::Key3,
                KeyCode::Key4,
                KeyCode::Key5,
                KeyCode::Key6,
                KeyCode::Key7,
                KeyCode::Key8,
                KeyCode::Key9,
                KeyCode::Key0,
            ];
            let selected = keys.iter().position(|&k| pressed!(k)).or_else(|| {
                if inventory_menu {
                    crate::hud::inventory_hit()
                } else {
                    None
                }
            });
            if let Some(i) = selected {
                if stats.select(i) {
                    if let Some(selected) = stats.equipped() {
                        hud_ops.push(HudOp::Announce(inventory::WEAPONS[selected].1.into()));
                    }
                } else {
                    hud_ops.push(HudOp::Announce(format!("Not yet available: {}", inventory::WEAPONS[i].1).into()));
                }
            }
            let (previous, next) = inventory_cycle(inventory_opened,
                input.action(&preferences, "Wheel Up", true)
                    || (inventory_menu && (input.ui(KeyCode::Up) || input.ui(KeyCode::Left))),
                input.action(&preferences, "Wheel Down", true)
                    || (inventory_menu && (input.ui(KeyCode::Down) || input.ui(KeyCode::Right))));
            if previous || next {
                stats.cycle(if next { 1 } else { -1 });
                if let Some(selected) = stats.equipped() {
                    hud_ops.push(HudOp::Announce(inventory::WEAPONS[selected].1.into()));
                }
            }
            weapon_input_enabled = !inventory_menu
                && stats.equipped().is_some()
                && !was_paused
                && !overlay_before
                && stats.alive()
                && !flying
                && player.script_motion == 0
                && !interactions.level_blocks_weapons();
            if weapon_input_enabled
                && (input.action(&preferences, "Mouse 1", true)
                    || input.action(&preferences, "Mouse 2", true))
            {
                if !crate::weapons::supported(stats.selected()) {
                    hud_ops.push(HudOp::Announce("This toy's attacks are not yet available".into()));
                } else if stats.selected() == 6 && !alice.ready_to_attack(6) {
                    hud_ops.push(HudOp::Announce("Demon Dice are still active or recovering".into()));
                } else if stats.selected() == 9 && stats.powers.recharge > 0. {
                    hud_ops.push(HudOp::Announce("Pocket Watch is recharging".into()));
                }
            }
        }
        weapon_click = weapon_buttons.sample(
            [
                input.action(&preferences, "Mouse 1", false),
                input.action(&preferences, "Mouse 2", false),
            ],
            weapon_input_enabled,
        );
        if inventory_menu && input.ui(KeyCode::Enter) {
            inventory_menu = false;
        }
        if matches!(host_transition.as_str(), "retry" | "new-game+retry")
            && new_campaign_memory.is_none() && pending_menu_newgame.is_none() { stats.damage(10000.); }
        let recover = matches!(host_transition.as_str(), "retry" | "new-game+retry") || focused
            && !menu
            && !inventory_menu
            && !console_input
            && ((!stats.alive() && (pressed!(KeyCode::Enter) || input.pad_pressed("A")))
                || (!paused && pressed!(KeyCode::R)));
        let restart =
            focused && !menu && !inventory_menu && !console_input && pressed!(KeyCode::Home);
        // Discard recovery input as well as the dead-player Retry branch. A
        // queued New Game must not move/reset the outgoing live frame either.
        let new_game_owns_input = new_campaign_memory.is_some() || pending_menu_newgame.is_some();
        let recover = recover && !new_game_owns_input;
        let restart = restart && !new_game_owns_input;
        let console_restart = console_restart && !new_game_owns_input;
        if simulate {
            if let Some(s) = &mut interactions.school {
                s.sync_inventory(&mut stats);
            }
        }
        let in_transport = interactions.duchess.as_ref().is_some_and(|d| d.cinematic())
            || interactions.scripted()
            || interactions.school.as_ref().is_some_and(|s| {
                s.cinematic()
                    || s.return_visit
                        .as_ref()
                        .is_some_and(|r| r.phase == crate::school_return::Phase::Rising)
            })
            || interactions.levels_transport();
        let skip_id = interactions.scene_id_after(
            interactions
                .duchess
                .as_ref()
                .and_then(|d| d.scene_id())
                .or_else(|| interactions.pandemonium.as_ref().and_then(|p| p.scene_id()))
                .or_else(|| interactions.pool.as_ref().and_then(|p| p.scene_id()))
                .or_else(|| interactions.school.as_ref().and_then(|s| s.scene_id()))
                .or_else(|| interactions.school2.as_ref().and_then(|s| s.scene_id()))
                .or_else(|| interactions.beyond.as_ref().and_then(|b| b.scene_id()))
                .or_else(|| {
                    interactions
                        .fortress
                        .as_ref()
                        .and_then(|f| f.state.cinema.scene_id())
                })
                .or_else(|| {
                    interactions
                        .village
                        .as_ref()
                        .and_then(|v| v.cinema.scene_id())
                }),
        );
        skip_scene.binding(input.film_skip_label(&preferences));
        let skip_requested = skip_scene.update(
            skip_id,
            !paused
                && !menu
                && !inventory_menu
                && !console_input
                && focused
                && !flying
                && stats.alive(),
            input.film_skip(&preferences),
        );
        skip_scene.note_input(
            input.film_skip(&preferences),
            input.frame().key(KeyCode::Enter, false),
            input.frame().key(KeyCode::E, false),
        );
        #[cfg(target_os = "vita")]
        // Use the same scene owner/end-state path as a held Cross skip.
        let skip_requested = skip_requested
            || (bench && skip_id.is_some() && !paused && !menu && !inventory_menu
                && !console_input && focused && !flying && stats.alive());
        if skip_requested {
            flush_pending!(frame, step_guards, step_input);
            let speaking = skip_id.is_some_and(|id| story.progress(id).is_some());
            if interactions.skip_cinematic(&scene.map, &mut world, &mut player, &mut story)? {
                #[cfg(feature = "native_timing")]
                timing.with(|r| r.discontinuity(crate::native_timing::Milestone::SkipComplete, crate::native_timing::SKIP));
                skip_scene.event("engine_exit accepted=true");
                camera_handoff.skip();
                if speaking {
                    audio_step.ops.push(AudioOp::Voice(VoiceRequest::Stop));
                }
                skip_scene.event(&format!("soundtrack_stop=not-applicable voice_stop={speaking}"));
                skip_scene.handoff("skipped");
                clock.pause();
                pos = player.eye();
                yaw = player.script_facing;
                pitch = 0.;
                weapon_click = None;
                weapon_buttons.block();
                recovery.clear();
                spawn_landing = true;
                hud_ops.push(HudOp::Announce("Scene skipped".into()));
                #[cfg(target_os = "vita")]
                if bench {
                    println!("bench: skipped scene {}", skip_id.unwrap_or("unknown"));
                }
            }
        }
        // A failed attempt must roll the entire world back with Alice. Do this
        // before the live transport guard: dying during a ride must remain retryable.
        let mut gameplay = false;
        if new_campaign_memory.is_none() && pending_menu_newgame.is_none()
            && saves_enabled && !stats.alive() && (recover || restart || console_restart) {
            flush_pending!(frame, step_guards, step_input);
            #[cfg(feature = "native_timing")]
            timing.with(|r| r.discontinuity(crate::native_timing::Milestone::LoadAttempt, crate::native_timing::OPERATION));
            let result = match retry.game_for(&save::visit_key(&maps[current], entry_spawn.as_deref())) {
                Ok(game) => {
                    flush_audio!("death-retry");
                    restore_transition!(game, "death-retry", false, retry, step_guards)
                }
                Err(error) => Err(error),
            };
            match result {
                Ok(()) => {
                    weapon_buttons.block();
                    hud_ops.push(HudOp::Announce("Checkpoint restored".into()));
                    println!("Death retry: restored complete checkpoint for {}", maps[current]);
                }
                Err(e) => {
                    paused = true;
                    #[cfg(feature = "native_timing")]
                    timing.with(|r| r.event(crate::native_timing::Milestone::LoadFailed, crate::native_timing::OPERATION));
                    hud_ops.push(HudOp::Announce(format!("Could not restore checkpoint: {e:#}").into()));
                    eprintln!("Death retry failed: {e:#}");
                }
            }
            // All per-frame clocks, targets and inputs above belong to the old
            // attempt. Begin a fresh frame before advancing the restored one.
            flush_audio!("death-retry-continue");
            finish_replay_step!("death-retry");
            drop(step_guards.take());
            drop(census_block);
            next_frame().await;
            census_block=crate::phase_census::span(crate::phase_census::Region::Other);
            continue;
        }
        if (recover || restart || console_restart) && in_transport {
            hud_ops.push(HudOp::Announce((if interactions.scripted() {
                "A scene is playing / hold Enter to skip"
            } else {
                "The ride is in progress / recovery is available after landing"
            }).into()));
        }
        if (recover || restart || console_restart) && !in_transport {
            flush_pending!(frame, step_guards, step_input);
            #[cfg(feature = "native_timing")]
            timing.with(|r| r.discontinuity(crate::native_timing::Milestone::Recovery, crate::native_timing::OPERATION));
            let entry = interaction::spawn_with_static(&scene.map, entry_spawn.as_deref(), Some(world.audio_geometry()));
            let entry = if options.larva_preview && maps[current] == "centipede1" {
                npcs.larva_preview_start(&world)?
            } else {
                entry
            };
            let entry = interactions
                .pandemonium
                .as_ref()
                .map_or(entry, |p| p.recovery_entry(entry));
            let entry = interactions
                .duchess
                .as_ref()
                .map_or(entry, |d| d.recovery_entry(entry));
            let entry = interactions.levels_recovery_entry(entry);
            let checkpoint = interactions
                .school
                .as_ref()
                .and_then(|s| s.return_visit.as_ref())
                .is_some_and(|r| r.phase == crate::school_return::Phase::Observatory);
            let entry = if checkpoint {
                (vec3(692., 4100., 1072.), -std::f32::consts::FRAC_PI_2)
            } else {
                entry
            };
            if restart || console_restart || (checkpoint && player.feet.z < 1000.) {
                recovery.clear();
            }
            if let Some((restored, angle, nearby)) =
                recovery.restore(&world, &interactions, player.feet, entry)
            {
                if let Some(p) = &mut interactions.pandemonium {
                    p.release_rope();
                }
                player = restored;
                yaw = angle;
                pos = player.eye();
                pitch = 0.;
                flying = false;
                paused = false;
                stats.restore();
                npcs.reset_combat();
                if options.larva_preview && maps[current] == "centipede1" {
                    flush_audio!("larva-preview");
                    npcs = crate::npc::Npcs::larva_preview(&mut assets)?;
                    npcs.update(0., &world, player.eye());
                    stats.loot.remove(&loot_visit);
                    let prefix = format!("drop:{loot_visit}:");
                    stats.collected.retain(|key| !key.starts_with(&prefix));
                }
                audio_step.ops.push(AudioOp::Voice(story.recover()));
                hints.recover();
                interactions.reset_contacts();
                spawn_landing = true;
                alice.reset(&player, yaw);
                follow_camera.reset();
                camera_handoff.reset();
                clock.pause();
                weapon_click = None;
                weapon_buttons.block();
                notice.clear();
                hud_ops.push(HudOp::Announce((if nearby {
                    "Restored to footing before the fall"
                } else {
                    "Restored at the level entrance"
                }).into()));
                println!(
                    "Recovery: feet={:?}, nearby={nearby}, sanity={}",
                    player.feet,
                    stats.sanity()
                );
            } else {
                hud_ops.push(HudOp::Announce("Entrance is obstructed / choose another level with Tab".into()));
            }
        }
        if (console_fly || (pressed!(KeyCode::F4) && !menu && !inventory_menu)) && stats.alive() {
            flush_pending!(frame, step_guards, step_input);
            if flying {
                let feet = pos - Vec3::Z * EYE_HEIGHT;
                if world.body_clear(feet) {
                    player = Player::new(feet);
                    spawn_landing = false;
                    alice.reset(&player, yaw);
                    follow_camera.reset();
                    camera_handoff.reset();
                    flying = false;
                    notice.clear();
                } else {
                    notice = "Move clear of the wall before switching to walking.".into();
                }
            } else {
                flying = true;
                pos = player.eye();
            }
            if console_fly {
                console.print(format!("Noclip {}", if flying { "ON" } else { "OFF" }));
                if !notice.is_empty() {
                    console.print(notice.clone());
                }
            }
            clock.pause();
        }
        if menu || console_map {
            flush_pending!(frame, step_guards, step_input);
            clock.pause();
            let mut chapter_action = None;
            if menu && !console_map {
                if focused && !console_input {
                    chapter_action = chapters.update(&input, level_choices.len(), &mut options.difficulty);
                }
                let choice = &level_choices[chapters.selected];
                selected = choice.map;
                selected_entry = choice.entry.map(str::to_owned);
            }
            if chapter_action == Some(crate::chapters::Hit::Back) {
                menu = false;
            } else if input.ui(KeyCode::Enter)
                || chapter_action == Some(crate::chapters::Hit::Begin)
                || console_map
            {
                #[cfg(feature = "vita_renderer")]
                { load_probe = Some(crate::restore_diagnostic::Probe::new("chapter-or-console")); }
                #[cfg(feature = "native_timing")]
                timing.with(|r| r.discontinuity(crate::native_timing::Milestone::LoadAttempt, crate::native_timing::OPERATION));
                let checkpoint = game_snapshot!();
                let particles = steam.runtime();
                flush_audio!("chapter-or-console");
                let audio_checkpoint = audio.transition_checkpoint();
                let mut memory = if let Some(memory) = new_campaign_memory.take() { memory } else {
                    crate::transition_memory::Transition::begin("chapter-or-console", &maps[current], &maps[selected])
                };
                let _loading = crate::loading_screen::begin("chapter-or-console", &maps[selected]);
                let mut outgoing = unload_scene!();
                owner_yield!(step_guards, crate::loading_screen::released());
                memory.after_release();
                match owner_yield!(step_guards, Box::pin(enter_level_async(
                    &mut assets,
                    &maps[selected],
                    selected_entry.as_deref(),
                    None,
                    &options,
                    !options.fly,
                ))) {
                    Ok(next) => {
                        #[cfg(feature = "native_timing")]
                        timing.with(|r| r.before_install());
                        scene = next.scene;
                        world = next.world;
                        level_art = next.art;
                        skip_scene = crate::cinematic::Skip::default();
                        hints = next.hints;
                        steam = next.steam;
                        npcs = next.npcs;
                        interactions = next.interactions;
                        story = next.story;
                        if console_map {
                            console.print(format!("Opened {}", maps[selected]));
                            console.open = false;
                        }
                        entry_spawn = selected_entry.clone();
                        recovery.clear();
                        stats = starting_stats(&options, &maps[selected], entry_spawn.as_deref());
                        println!(
                            "Level loadout {} {:?}: {}",
                            maps[selected],
                            entry_spawn,
                            stats.weapon_summary()
                        );
                        interactions.sync(&mut world);
                        environment_clock = 0.;
                        audio.load(&mut assets, &maps[selected], &scene.map);
                        current = selected;
                        campaign = Campaign::default();
                        played = true;
                        pickup_clock = 0.;
                        power_art.clear_effects();
                        pickups = inventory::pickups_for_visit(
                            &scene.map,
                            &maps[current],
                            entry_spawn.as_deref(),
                            &catalog,
                        );
                        (pos, yaw) = interaction::spawn_with_static(&scene.map, entry_spawn.as_deref(), Some(world.audio_geometry()));
                        let spawn = Player::spawn(&world, pos);
                        flying = options.fly || spawn.is_none();
                        spawn_landing = true;
                        player = spawn.unwrap_or_else(|| Player::new(pos - Vec3::Z * EYE_HEIGHT));
                        alice.reset(&player, yaw);
                        follow_camera.reset();
                        camera_handoff.reset();
                        clock.pause();
                        paused = false;
                        pitch = 0.;
                        notice.clear();
                        menu = false;
                        retry.remember(game_snapshot!());
                        #[cfg(feature = "native_timing")]
                        timing_ready!(crate::native_timing::Reason::ChapterOrConsole);
                        drop(outgoing);
                        memory.finish(true);
                    }
                    Err(e) => {
                        let _ = rollback_resources!(checkpoint, outgoing, particles, audio_checkpoint, false, step_guards);
                        drop(outgoing);
                        memory.finish(false);
                        #[cfg(feature = "native_timing")]
                        timing.with(|r| r.event(crate::native_timing::Milestone::LoadFailed, crate::native_timing::OPERATION));
                        notice = format!("Could not open level: {e:#}");
                        if console_map {
                            console.print(notice.clone());
                        }
                    }
                }
            }
        } else if simulate && !overlay_before
            && !paused
            && !inventory_menu
            && !console_input
            && focused
            && stats.alive()
        {
            gameplay = true;
        } else if simulate {
            clock.pause();
        }
        // This is the positive gameplay arm. Owner transactions and retained
        // renderer fallback stay on the serial lane.
        #[cfg(feature = "vita_renderer")]
        let step_load_probe = load_probe.is_some();
        #[cfg(not(feature = "vita_renderer"))]
        let step_load_probe = false;
        #[cfg(target_os = "vita")]
        let step_survey = r13_survey.is_some();
        #[cfg(not(target_os = "vita"))]
        let step_survey = false;
        let dispatch_failures = (sim_overlap || sim_audit.diagnose()).then(|| [
            ("gameplay", !gameplay),
            ("first_replay_step", first_replay_step),
            ("load_probe", step_load_probe),
            ("bench", bench),
            ("route", route_running),
            ("survey", step_survey),
            ("host_transition", !host_transition.is_empty()),
            ("save", save_requested),
            ("load", load_requested.is_some()),
            ("recover_restart", recover || restart || console_restart),
            ("camera_portals", scene.snapshot_portals_fenced()),
            ("autosave", autosave_requested),
            ("quit", quit_after_frame),
            ("menu_save", pending_menu_save.is_some()),
            ("menu_newgame", pending_menu_newgame.is_some()),
            ("context", context_before != (menu, inventory_menu, console.open, paused)),
            ("visit", frame_stamp.frame.visit_id != level_art.snapshot_visit.id()),
            ("retained_renderer", !scene.retained_active()),
            ("handoff", camera_handoff.was_scripted() && level_art.handoff_pose().is_some()),
            ("snapshot_handoff", snapshot_handoff && handoff_return.pose(&level_art.snapshot_visit).is_some()),
            ("frame_limit", options.frames.is_some_and(|limit| frame + 1 >= limit)),
            ("forced_fence", lag_map && fence_every != 0 && frame % fence_every == 0),
            ("early_flush", forced_flush),
            ("lag_handoff", lag_map && camera_handoff.was_scripted()),
        ]);
        let dispatch_failures = dispatch_failures.as_ref().map_or(&[][..], |failures| &failures[..]);
        let w0_eligible = !dispatch_failures.iter().any(|&(_, failed)| failed);
        let step_overlap = sim_overlap && (lag_maps.is_empty() || lag_map);
        let dispatchable = step_overlap && w0_eligible;
        let lag_dispatch = lag_map && dispatchable;
        if !lag_dispatch {
            flush_pending!(frame, step_guards, step_input);
        }
        if dispatchable {
            worker_generation = worker_generation.checked_add(1).expect("sim slot generation exhausted");
            frame_stamp.slot_generation = worker_generation;
            dispatched_frames += 1;
        }
        sim_trace.owner(frame_stamp, sim_timing::Scope::Head, None, head_started);
        let mut step_locals = StepLocals {
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
            power_before,
            advance_dialogue, pending_transition, weapon_click,
        };
        let mut step_scopes = StepScopes {
            #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
            frame: Some(host_block),
            #[cfg(target_os = "vita")]
            frame: Some(viewer_block),
            census: Some(census_block),
        };
        let input_a = StepAInput {
            gameplay,
            map: scene.map.clone(),
            preferences: preferences.clone(),
            saves_writable,
            host_natural_exit: host_transition == "natural-exit",
            #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
            force_checkpoint: force_checkpoint_frame == Some(frame),
        };
        let mut joined_b = None;
        let (step_a_result, console_ops) = if lag_dispatch {
            let seal = replay.as_ref().and_then(|r| {
                let due = r.due();
                (cfg!(not(target_os = "vita")) || due.is_some()).then_some(due.unwrap_or(false))
            });
            let input_b = StepBInput {
                story_active: false,
                dispatched: true,
                map_name: maps[current].clone(),
                preferences: preferences.clone(),
                aspect: screen_width() / screen_height(),
                handoff_pose: None,
                loot_visit: loot_visit.clone(),
                loot_before: loot_before.clone(),
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                frame,
                #[cfg(target_os = "vita")]
                bench_camera,
            };
            let (job, (mut alice_art, mut npcs_art, steam_art)) = take_step!(step_locals, frame_stamp);
            let worker = sim_worker.as_mut().expect("lag mode requires the sim worker");
            let (owner_peer, worker_peer) = lag_control.pair(pending.is_some());
            render_peer = owner_peer;
            sim_audit.event("dispatch", frame_stamp);
            worker.submit(job, step_input.clock(),
                blocked_step::Input::Lag(input_a, input_b, acoustic_view.take(), seal, worker_peer),
                &mut sim_trace)?;
            let serial_reply = lag_control.serial.then(|| worker.join(&mut sim_trace)).transpose()?;
            let restore_clock = step_guards.take().is_some();
            render_pending!(frame, &mut alice_art, &mut npcs_art, &steam_art);
            if restore_clock {
                step_guards = Some((StepGuard::enter(), step_input.enter_clock()));
            }
            let (job, output) = match serial_reply {
                Some(reply) => reply,
                None => worker.join(&mut sim_trace)?,
            };
            let blocked_step::Output::Lag(due, result) = output else { unreachable!("lag worker reply"); };
            joined_b = result;
            let console_ops = restore_step!(job, (alice_art, npcs_art, steam_art), step_locals);
            (due, console_ops)
        } else {
            if dispatchable { sim_audit.event("dispatch", frame_stamp); }
            run_step!(step_a, step_locals, step_scopes, input_a,
                frame_stamp, sim_worker.as_mut().filter(|_| dispatchable), step_input.clock(), A, StepA,
                blocked_step::Input::A, blocked_step::Output::A(result) => result)
        };
        if console_ops.close { console.open = false; }
        if let Some(message) = console_ops.message { console.print(message); }
        let (story_active, mut due) = step_a_result?;
        let worker_complete = dispatchable && matches!(due, Due::None);
        let mid_step = (dispatchable && !worker_complete).then(|| match &due {
            Due::None => unreachable!(),
            Due::Checkpoint => "mid_step_checkpoint",
            Due::Ending => "mid_step_ending",
            Due::Exit(_) => "mid_step_exit",
        });
        if let Some(reason) = mid_step { sim_audit.mid_step(frame_stamp, reason); }
        if simulate {
        if matches!(due, Due::Checkpoint) {
            if throttle_normal {
                // Keep the original checkpoint snapshot; only its thumbnail
                // capture and disk write wait for the clean render boundary.
                finale_checkpoint_requested = Some(game_snapshot!());
            } else {
            match acknowledge_checkpoint(store.write_finale_checkpoint(&game_snapshot!()), &mut interactions.levels) {
                Ok(()) => {
                    retry.remember(game_snapshot!());
                    hud_ops.push(HudOp::Announce("Checkpoint saved".into()));
                }
                Err(e) => { hud_ops.push(HudOp::Announce(format!("Checkpoint save failed: {e:#}").into())); }
            }
            }
            due = step_due(false, story_active, post_game, &mut interactions, &mut story,
                &mut step_locals.pending_transition, host_transition == "natural-exit");
        }
        if matches!(due, Due::Ending) {
            post_game = true;
            campaign.completed.insert(save::visit_key(&maps[current], entry_spawn.as_deref()));
            if saves_writable { if let Err(e) = store.mark_complete() { eprintln!("Completion marker failed: {e:#}"); } }
            mouse_look.release();
            input.suppress_captured();
            #[cfg(feature = "native_timing")]
            timing.with(|r| r.discontinuity(crate::native_timing::Milestone::EndingEnter, crate::native_timing::CINEMA));
            flush_audio!("ending-film");
            match owner_yield!(step_guards, crate::movie::play(
                &mut assets,
                &mut audio,
                &ui,
                &mut input,
                &preferences,
                "ending",
            )) {
                Ok(false) => {
                    flush_audio!("ending-quit");
                    finish_replay_step!("ending-quit");
                    break;
                }
                Err(e) => eprintln!("Ending film failed: {e:#}"),
                Ok(true) => (),
            }
            #[cfg(feature = "native_timing")]
            timing.with(|r| r.discontinuity(crate::native_timing::Milestone::EndingExit, crate::native_timing::CINEMA));
            escape_menu.post_game = true;
            escape_menu.open(crate::menu::Page::Credits, &preferences, audio.settings);
            escape_menu.message = "Campaign complete / Continue replays the final encounter".into();
            clock.pause();
            flush_audio!("ending-continue");
            finish_replay_step!("ending");
            continue;
        }
        if let Due::Exit(exit) = due {
            options.difficulty = stats.difficulty;
            let (name, target_spawn) = exit.clone();
            let leaving = level_snapshot!();
            // The shared transition (`campaign::arrive`) decides everything the headless routes
            // also decide. The exit is all or nothing, so it works on copies of the resources and
            // the ledger until the destination's window half has loaded too.
            let (mut next_stats, mut next_campaign) = (stats.clone(), campaign.clone());
            clock.pause();
            flush_audio!("natural-exit");
            audio.update(0., pos, yaw, true, true);
            #[cfg(target_os="vita")]
            crate::frame_profile::log_report_error(crate::frame_profile::report(&maps[current]));
            #[cfg(feature = "native_timing")]
            timing.with(|r| r.discontinuity(crate::native_timing::Milestone::TransitionBegin, crate::native_timing::TRANSITION));
            let _loading = crate::loading_screen::begin("natural-exit", &name);
            owner_yield!(step_guards, present_transition_loading(&name));
            let transition_started = std::time::Instant::now();
            let checkpoint = game_snapshot!();
            let particles = steam.runtime();
            let audio_checkpoint = audio.transition_checkpoint();
            let mut memory = crate::transition_memory::Transition::begin("natural-exit", &maps[current], &name);
            let mut outgoing = unload_scene!();
            owner_yield!(step_guards, crate::transition_memory::released_with(|| {
                crate::loading_screen::draw_or(|| draw_transition_loading(&name));
                // Inspect the submitted loading frame before the swap. Reading
                // the next back buffer after next_frame only sees its clear.
                #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                if let Some(path) = std::env::var_os("LOOKING_GLASS_LOADING_BUILD_CAPTURE") {
                    save_capture(std::path::Path::new(&path)).expect("last loading frame before build");
                }
            }));
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            if crate::transition_memory::legacy_overlap() {
                if let (Some(source), Some(target)) = (std::env::var_os("LOOKING_GLASS_LOADING_CAPTURE"), std::env::var_os("LOOKING_GLASS_LOADING_BUILD_CAPTURE")) {
                    std::fs::copy(source, target)?;
                }
            }
            memory.after_release();
            let arrived = owner_yield!(step_guards, Box::pin(enter_natural(
                &mut assets, &mut next_stats, &mut next_campaign, leaving, &exit, &options,
            )));
            match arrived {
                Ok((next, arrived_player, arrived_eye, arrived_yaw)) => {
                    #[cfg(feature = "native_timing")]
                    timing.with(|r| r.before_install());
                    scene = next.scene;
                    world = next.world;
                    stats = next_stats;
                    campaign = next_campaign;
                    hints = next.hints;
                    steam = next.steam;
                    npcs = next.npcs;
                    interactions = next.interactions;
                    story = next.story;
                    environment_clock = next.environment_clock;
                    pickup_clock = next.pickup_clock;
                    power_art.clear_effects();
                    current = maps
                        .iter()
                        .position(|m| m == &name)
                        .context("Exit map unavailable")?;
                    selected = current;
                    entry_spawn = target_spawn;
                    println!(
                        "Exit loadout {name} {:?}: {}",
                        entry_spawn,
                        stats.weapon_summary()
                    );
                    recovery.clear();
                    interactions.sync(&mut world);
                    pickups = inventory::pickups_for_visit(
                        &scene.map,
                        &name,
                        entry_spawn.as_deref(),
                        &catalog,
                    );
                    audio.load(&mut assets, &name, &scene.map);
                    eprintln!("[lg/transition] destination={name} resources_audio_ms={:.3}", transition_started.elapsed().as_secs_f64()*1000.);
                    // A blocked entrance already failed the exit, so there is no flight fallback.
                    (pos, yaw) = (arrived_eye, arrived_yaw);
                    flying = false;
                    player = arrived_player;
                    alice.reset(&player, yaw);
                    follow_camera.reset();
                    camera_handoff.reset();
                    clock.pause();
                    pitch = 0.;
                    spawn_landing = true;
                    step_locals.weapon_click = None;
                    weapon_buttons.block();
                    level_art = next.art;
                    skip_scene = crate::cinematic::Skip::default();
                    autosave_requested = options.frames.is_none();
                    retry.remember(game_snapshot!());
                    let visit = crate::campaign::choice_position(
                        &level_choices,
                        current,
                        entry_spawn.as_deref(),
                    );
                    hud_ops.push(HudOp::Announce(level_choices[visit].title.clone().into()));
                    #[cfg(feature = "native_timing")]
                    timing_ready!(crate::native_timing::Reason::Natural);
                    drop(outgoing);
                    memory.finish(true);
                }
                Err(e) => {
                    let _ = rollback_resources!(checkpoint, outgoing, particles, audio_checkpoint, false, step_guards);
                    drop(outgoing);
                    memory.finish(false);
                    interactions.transition_failed(&exit);
                    #[cfg(feature = "native_timing")]
                    timing.with(|r| r.event(crate::native_timing::Milestone::LoadFailed, crate::native_timing::TRANSITION));
                    hud_ops.push(HudOp::Announce(format!("Could not enter next map: {e}").into()));
                }
            }
        }
        } // simulation: story and transitions
        if !worker_complete {
            frame_stamp.frame.visit_id = level_art.snapshot_visit.id();
        }
        let seal_request = replay.as_ref().filter(|r| r.running()).and_then(|r| {
            let due = r.due();
            (snapshot_handoff && (cfg!(not(target_os = "vita")) || due.is_some()))
                .then_some(due.unwrap_or(false))
        });
        let mut worker_seal = None;
        let mut worker_present = None;
        let view = if let Some(result) = joined_b {
                let (view, acoustic, present, seal) = result?;
                worker_present = Some(present);
                acoustic_view = Some(acoustic);
                worker_seal = seal.map(|(stamp, probe)| (stamp, probe, "tail"));
                view
        } else {
            run_step!(step_b, step_locals, step_scopes, StepBInput {
            story_active,
            dispatched: worker_complete,
            map_name: maps[current].clone(),
            preferences: preferences.clone(),
            aspect: screen_width() / screen_height(),
            handoff_pose: if snapshot_handoff && level_art.pool.is_some() {
                handoff_return.pose(&level_art.snapshot_visit).cloned()
            } else {
                level_art.handoff_pose().cloned()
            },
            loot_visit,
            loot_before,
            #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
            frame,
            #[cfg(target_os = "vita")]
            bench_camera,
        }, frame_stamp, sim_worker.as_mut().filter(|_| worker_complete), step_input.clock(), B, StepB,
            |input| blocked_step::Input::B(input, acoustic_view.take(), seal_request),
            blocked_step::Output::B(result) => {
                let (view, acoustic, present, seal) = result?;
                worker_present = Some(present);
                acoustic_view = Some(acoustic);
                worker_seal = seal.map(|(stamp, probe)| (stamp, probe, "tail"));
                view
            }).0

        };
        let StepView {
            stamp: present_stamp, presentation, camera_out, dynamic: mut dynamic_input, active, direction, targets, pand_cinematic, scripted_camera, show_alice,
            #[cfg(target_os = "vita")]
            gameplay_alice,
        } = view;
        let present_stamp = worker_present.unwrap_or(present_stamp);
        assert_eq!(present_stamp, frame_stamp, "stale present stamp");
        assert_eq!(present_stamp.frame.visit_id, level_art.snapshot_visit.id(), "stale present visit");
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        { host_block = step_scopes.frame.take().unwrap(); }
        #[cfg(target_os = "vita")]
        { viewer_block = step_scopes.frame.take().unwrap(); }
        census_block = step_scopes.census.take().unwrap();
        let (camera, target) = (camera_out.position, camera_out.target);
        // The step, including any Pool of Tears re-publication, is complete.
        let audio_started = sim_trace.mark(present_stamp.frame.frame);
        if worker_complete {
            audio_step.apply(&mut audio, &mut assets, acoustic_view.as_ref().unwrap());
        } else {
            #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
            if simulate && force_acoustic_view {
                let started = profile_acoustic_view.then(std::time::Instant::now);
                let view = world.capture_acoustic_view(&mut acoustic_view);
                if let Some(started) = started {
                    let ns = started.elapsed().as_nanos();
                    let (settled, dynamic, weapon) = view.collider_counts();
                    println!("ACOUSTIC_CAPTURE frame={frame} ns={ns} settled={settled} dynamic={dynamic} weapon={weapon}");
                }
                audio_step.apply(&mut audio, &mut assets, view);
            } else {
                audio_step.apply(&mut audio, &mut assets, &world);
            }
            #[cfg(not(all(feature = "sim_maps_probe", not(target_os = "vita"))))]
            {
                audio_step.apply(&mut audio, &mut assets, &world);
            }
        }
        sim_trace.owner(present_stamp, sim_timing::Scope::AudioApply, None, audio_started);
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        { drop(host_block); host_block = crate::frame_profile::span("frame_scene"); }
        crate::spike_log::sim(crate::spike_log::Sim::Scene);
        #[cfg(target_os = "vita")]
        {
            drop(viewer_block);
            viewer_block = crate::frame_profile::span("frame_scene");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::Scene);
        }
        let census_leaf = crate::phase_census::span(crate::phase_census::Region::CameraPresentation);
        scene.apply_presentation(&presentation);
        drop(census_leaf); let census_leaf = crate::phase_census::span(crate::phase_census::Region::CameraPrepare3d);
        #[cfg(feature="vita_renderer")]
        let display_clear = !crate::render::vita_renderer::display_clear_skipped();
        #[cfg(not(feature="vita_renderer"))]
        let display_clear = true;
        if display_clear { clear_background(scene.atmosphere.background()); }
        #[cfg(feature="vita_renderer")]
        crate::render::vita_renderer::prepare_3d(scene.atmosphere.background());
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        crate::sim_maps::probe::sample_world_viewport();
        #[cfg(feature = "vita_renderer")]
        if let Some(p) = &load_probe { p.gl("prepare-3d-returned"); }
        let world_camera = Camera3D {
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            aspect:crate::sim_maps::probe::enabled().then_some(960./544.),
            #[cfg(feature="vita_renderer")]
            viewport: crate::render::vita_renderer::world_viewport(),
            position: camera_out.position,
            target: camera_out.target,
            up: camera_out.up,
            fovy: camera_out.fovy,
            z_near: 2.,
            z_far: crate::render::draw_distance_far(30000.),
            ..Default::default()
        };
        #[cfg(feature="vita_renderer")]
        crate::render::vita_renderer::set_3d_camera(&world_camera);
        #[cfg(all(feature="sim_maps_probe", not(target_os="vita")))]
        crate::sim_maps::probe::note_projection(world_camera.matrix());
        #[cfg(not(feature="vita_renderer"))]
        set_camera(&world_camera);
        #[cfg(feature = "vita_renderer")]
        crate::render::vita_renderer::record_view(&world_camera);
        #[cfg(feature = "vita_renderer")]
        let world_only = crate::render::vita_renderer::world_only();
        #[cfg(not(feature = "vita_renderer"))]
        let world_only = false;
        drop(census_leaf); let census_leaf = crate::phase_census::span(crate::phase_census::Region::CameraTransforms);
        if worker_complete {
            assert!(scene.retained_active(), "worker frame reached the legacy rope path");
        } else {
            let rope_hand = interactions
                .pandemonium
                .as_ref()
                .and_then(|p| p.rope_hand())
                .or_else(|| {
                    player
                        .rope
                        .as_ref()
                        .and_then(|g| world.traversal.rope(g.id))
                        .map(|r| (r.model, player.feet + Vec3::Z * 40.))
                });
            scene.update_ropes(&world, world_dt, rope_hand, &dynamic_input.transforms);
        }
        drop(census_leaf);
        crate::spike_log::sim(crate::spike_log::Sim::Particles2);
        drop(census_block); census_block=crate::phase_census::span(crate::phase_census::Region::Particles);
        {
            #[cfg(target_os = "vita")]
            let _hotspot = crate::frame_profile::span("steam_animate");
            steam.animate(environment_clock);
        }
        {
            #[cfg(target_os = "vita")]
            let _hotspot = crate::frame_profile::span("steam_sync");
            steam.sync(&interactions.event_world);
        }
        {
            #[cfg(target_os = "vita")]
            let _hotspot = crate::frame_profile::span("steam_collected");
            steam.collected(&pickups, &stats);
        }
        if interactions
            .school
            .as_ref()
            .is_some_and(|s| s.theatre_finished())
        {
            steam.stop();
        }
        if let Some(v) = &interactions.village {
            {
                #[cfg(target_os = "vita")]
                let _hotspot = crate::frame_profile::span("village_particle_bindings");
                v.place_particles(steam.sim_mut());
            }
        }
        if let Some(p) = &interactions.pandemonium {
            p.gate_particles(steam.sim_mut());
        }
        interactions.place_level_particles(steam.sim_mut());
        // All resolvers execute without issuing graphics calls. The opt-in runs
        // them before submission; the control preserves the original HUD phase.
        macro_rules! resolve_prompt { () => {{
                use crate::interaction::friendly::{use_owner, UseOwner};
                let rope = player.rope_prompt(&world);
                let world_rope = interactions
                    .pandemonium
                    .as_ref()
                    .is_some_and(|p| p.prompt(&world, player.eye()).is_some());
                let talk =
                    interactions.conversation(&world, player.eye(), direction, &npcs, &story);
                let world_prompt = interactions.prompt(&world, player.eye(), direction);
                let owner = use_owner(
                    story.busy(),
                    interactions.scripted(),
                    rope.is_some(),
                    world_rope,
                    talk.is_some(),
                    world_prompt.is_some(),
                );
                let prompt = match owner {
                    UseOwner::Talk => talk.map(|c| c.prompt()),
                    UseOwner::SharedRope => rope.map(str::to_owned),
                    UseOwner::World => world_prompt.map(str::to_owned),
                    _ => None,
                };
            prompt
        }}; }
        macro_rules! resolve_objective { ($help:expr) => {{
            let help = $help;
            let objective = interactions
                .school
                .as_ref()
                .filter(|_| help)
                .map(|s| s.objective())
                .or_else(|| {
                    interactions
                        .school2
                        .as_ref()
                        .filter(|_| help)
                        .map(|s| s.quest.objective())
                })
                .or_else(|| {
                    interactions
                        .pandemonium
                        .as_ref()
                        .filter(|_| help)
                        .map(|p| p.objective())
                })
                .or_else(|| {
                    interactions
                        .fortress
                        .as_ref()
                        .filter(|_| help)
                        .map(|f| f.objective())
                })
                .or_else(|| {
                    interactions
                        .pool
                        .as_ref()
                        .filter(|_| help)
                        .map(|p| p.objective())
                })
                .or_else(|| interactions.levels_objective().filter(|_| help))
                .or_else(|| help.then(|| story.objective().map(str::to_owned)).flatten());
            objective
        }}; }
        macro_rules! resolve_skip { () => {{
interactions
                    .village
                    .as_ref()
                    .is_some_and(|v| v.cinema.active())
                    || interactions
                        .fortress
                        .as_ref()
                        .is_some_and(|f| f.state.cinema.active())
                    || interactions
                        .duchess
                        .as_ref()
                        .and_then(|d| d.scene_id())
                        .is_some()
                    || interactions
                        .pandemonium
                        .as_ref()
                        .and_then(|p| p.scene_id())
                        .is_some()
                    || interactions
                        .beyond
                        .as_ref()
                        .is_some_and(|b| b.scene_id().is_some())
                    || interactions.scene_id_after(None).is_some()
                    || interactions
                        .pool
                        .as_ref()
                        .is_some_and(|p| p.scene_id().is_some())
                    || interactions
                        .school
                        .as_ref()
                        .is_some_and(|s| s.scene_id().is_some())

        }}; }
        macro_rules! resolve_fades { () => {{
            let mut colors = crate::render_snapshot::Fades::default();
                let cover = camera_handoff.overlay();
                if cover.a > 0. {
                    colors.push(cover);
                }
                if let Some(f) = &interactions.fortress {
                    let (mut color, alpha) = f.cinema.fade(&f.state.cinema);
                    color.a = alpha;
                    if alpha > 0. {
                        colors.push(color);
                    }
                }
                if let Some(p) = &interactions.pandemonium {
                    let fade = p.fade();
                    if fade > 0. {
                        colors.push(Color { a: fade, ..p.fade_color() });
                    }
                }
                if let Some(v) = &interactions.village {
                    let (mut color, alpha) = v.cinema.fade();
                    color.a = alpha;
                    if alpha > 0. {
                        colors.push(color);
                    }
                }
                if let Some(p) = &interactions.pool {
                    let (mut color, alpha) = p.fade();
                    color.a = alpha;
                    if alpha > 0. {
                        colors.push(color);
                    }
                }
                if let Some(s) = &interactions.school {
                    let (mut color, alpha) = s.scene_fade();
                    color.a = alpha;
                    if alpha > 0. {
                        colors.push(color);
                    }
                }
                if let Some(b) = &interactions.beyond {
                    let (mut color, alpha) = b.scene_fade();
                    color.a = alpha;
                    if alpha > 0. {
                        colors.push(color);
                    }
                }
                if let Some((mut color, alpha)) = interactions.fade_after(None) {
                    color.a = alpha;
                    if alpha > 0. {
                        colors.push(color);
                    }
                }
            colors
        }}; }
        #[cfg(feature="vita_renderer")]
        let snapshot_enabled = snapshot_handoff;
        #[cfg(not(feature="vita_renderer"))]
        let snapshot_enabled = false;
        #[cfg(feature="vita_renderer")]
        let snapshot_log = crate::render::vita_renderer::config().snapshot_log_enabled();
        #[cfg(not(feature="vita_renderer"))]
        let snapshot_log = false;
        let hud_snapshot_timing = snapshot_log.then(SnapshotTiming::start);
        let hud_snapshot = snapshot_enabled.then(|| crate::render_snapshot::HudInput {
            visit: level_art.snapshot_visit.id(),
            prompt: if !world_only && active { resolve_prompt!() } else { None },
            objective: if !world_only { resolve_objective!(get_time() < help_until) } else { None },
            help: get_time() < help_until,
            stats: stats.hud_input(), breath: player.breath.clone(),
            immersion: player.immersion.level, script_motion: player.script_motion,
            swimming: player.swimming, scripted: interactions.scripted(),
            fades: resolve_fades!(), subtitle: story.subtitle_input(), skip: resolve_skip!(),
            level: level_art.hud_sources().capture(&interactions, crate::level::HudCaptureView {
                width: screen_width(), height: screen_height(),
            }),
        });
        // Validate the entire HUD batch before drawing any command. A rejection
        // retains the serial live fallback and fences this frame.
        let hud_snapshot = hud_snapshot.filter(|s| s.visit == level_art.snapshot_visit.id()
            && s.valid());
        let tail_snapshot = snapshot_enabled.then(|| crate::render_snapshot::TailInput {
            alive: stats.alive(), feet: player.feet,
            cinema: skip_id.is_some() || pand_cinematic || scripted_camera.is_some() || story.busy()
                || interactions.school.as_ref().is_some_and(|s| s.cinematic()),
            #[cfg(feature="native_timing")]
            timing: timing_state!(),
        });
        let captured_save = if snapshot_enabled && (pending_menu_save.is_some()
            || (pending_menu_newgame.is_some() && saves_writable && stats.alive() && !post_game)
            || ((save_requested || autosave_requested) && saves_writable && stats.alive())) {
            Some(game_snapshot!())
        } else { None };
        if let (Some(timing), Some(snapshot)) = (hud_snapshot_timing, &hud_snapshot) {
            let (us, allocations) = timing.finish();
            println!("[lg/snapshot] frame={} hud_bytes={} extraction_us={:.3} allocations={} save={}",
                frame, snapshot.bytes(), us, allocations, captured_save.is_some());
        }
        // Shared transform/view input. Particle and pickup consumers now own
        // their dedicated arenas; do not duplicate their old census payloads.
        if snapshot_enabled {
            use macroquad::camera::Camera;
            let timing = snapshot_log.then(SnapshotTiming::start);
            dynamic_input.finish_view(world_camera.matrix(), &scene.atmosphere);
            if let Some(timing) = timing {
                let (us, allocations) = timing.finish();
                println!("[lg/snapshot-dynamic] frame={} bytes={} capacity_bytes={} extraction_us={:.3} allocations={} transforms={} particles={} pickups={}",
                    frame, dynamic_input.bytes(), dynamic_input.capacity_bytes(), us, allocations,
                    dynamic_input.transforms.len(), 0, 0);
            }
        }
        // Resolve dynamic lights at the simulation boundary. Resident lighting
        // textures and the selected-light TLS stay owned by the renderer.
        let extracted_lights = snapshot_enabled.then(|| {
            let timing = snapshot_log.then(SnapshotTiming::start);
            let mut lights = steam.lights();
            lights.extend(alice.lights());
            lights.extend(npcs.lights());
            for slot in &interactions.levels { lights.extend(slot.ctl.lights()); }
            let selection = crate::lighting::resolve(lights, camera, &world);
            if let Some(timing) = timing {
                let (us, allocations) = timing.finish();
                println!("[lg/snapshot-lights] frame={} bytes={} capacity_bytes={} extraction_us={:.3} allocations={}",
                    frame, selection.bytes(), selection.capacity_bytes(), us, allocations);
            }
            selection
        });
        let preview_capture = if saves_writable {
            #[cfg(feature = "vita_renderer")]
            let throttled = crate::render::vita_renderer::config().save_preview == "throttled";
            #[cfg(not(feature = "vita_renderer"))]
            let throttled = false;
            #[cfg(target_os = "vita")]
            let preview_mode = crate::frame_profile::mode();
            #[cfg(not(target_os = "vita"))]
            let preview_mode = crate::frame_profile::Mode::Full;
            let required = save_requested || autosave_requested || finale_checkpoint_requested.is_some()
                || pending_menu_save.is_some() || pending_menu_newgame.is_some() || opening_escape_menu
                || (!context_before.3 && paused) || (!context_before.0 && menu)
                || (!context_before.1 && inventory_menu) || quit_after_frame;
            preview_throttle.capture_requested(preview_mode, throttled, required)
        } else { false };
        let tail_save_fence = save_requested || autosave_requested || finale_checkpoint_requested.is_some()
            || pending_menu_save.is_some() || pending_menu_newgame.is_some();
        #[cfg(target_os="vita")]
        let tail_survey_fence = r13_survey.is_some();
        #[cfg(not(target_os="vita"))]
        let tail_survey_fence = false;
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        let tail_diagnostic_fence = crate::transition_memory::host_probe();
        #[cfg(not(all(feature="sim_maps_probe",not(target_os="vita"))))]
        let tail_diagnostic_fence = false;
        let tail_terminal_fence = quit_after_frame || options.frames.is_some_and(|n| frame+1>=n);
        let snapshot_tail_fenced = !snapshot_enabled || hud_snapshot.is_none() || tail_save_fence
            || tail_survey_fence || tail_diagnostic_fence || tail_terminal_fence;
        if !snapshot_enabled {for art in level_art.levels.iter_mut().flatten() {art.clear_snapshot();}
            if let Some(a)=&mut level_art.encounters {a.clear_snapshot();}}
        let mut snapshot_rejected=false;
        let level_dispatch = snapshot_enabled.then(|| {
            let timing = snapshot_log.then(SnapshotTiming::start);
            let input = level_art.extract_dispatch(&interactions, &hints, &story);
            if let Some(timing) = timing {
                let (us, allocations) = timing.finish();
                println!("[lg/snapshot-dispatch] frame={} bytes={} extraction_us={:.3} allocations={} overflow={}",
                    frame, std::mem::size_of::<LevelDispatch>() + input.registered.capacity()
                        + level_art.levels.iter().flatten().map(|a|a.snapshot_bytes()).sum::<usize>()
                        + input.cheshire.as_ref().map_or(0, |i| i.bytes())
                        + level_art.encounters.as_ref().map_or(0,|a|a.snapshot_bytes()),
                    us, allocations, u8::from(input.overflow));
            }
            input
        });
        if snapshot_enabled {
            // Script state is resolved before cinematic pose extraction; materials
            // and lazy mesh/effect publication remain on the renderer thread.
            if let (Some(art), Some(pool)) = (&mut level_art.pool, &interactions.pool) {
                art.cinema.story_pose(&story);
                let timing = snapshot_log.then(SnapshotTiming::start);
                let (bytes, overflow) = art.extract(pool);
                if let Some(timing) = timing {
                    let (us, allocations) = timing.finish();
                    println!("[lg/snapshot-pool] frame={} bytes={} extraction_us={:.3} allocations={} overflow={}",
                        frame, bytes, us, allocations, u8::from(overflow));
                }
                if overflow { snapshot_rejected=true; handoff_return.retire(); handoff_minimum_frame = frame as u64; }
            }
        } else if let Some(art) = &mut level_art.pool { art.clear_input(); }
        if snapshot_enabled {
            let timing = snapshot_log.then(SnapshotTiming::start);
            let (bytes, ants, overflow) = npcs.extract_ants(camera, (target-camera).normalize_or_zero());
            let measurement = timing.map(SnapshotTiming::finish);
            if overflow { snapshot_rejected=true; handoff_return.retire(); handoff_minimum_frame = frame as u64; }
            snapshot_frames += 1;
            if let Some((us, allocations)) = measurement {
                println!("[lg/snapshot-actors] frame={} bytes={} extraction_us={:.3} allocations={} ants={} overflow={}",
                    frame, bytes, us, allocations, ants, u8::from(overflow));
            }
        } else { npcs.clear_actor_inputs(); }
        if snapshot_enabled {
            let timing = snapshot_log.then(SnapshotTiming::start);
            let (bytes,overflow)=alice.extract_body(dynamic_input.alice_feet);
            if overflow { snapshot_rejected=true;handoff_return.retire();handoff_minimum_frame=frame as u64;}
            if let Some(timing) = timing {
                let (us, allocations) = timing.finish();
                println!("[lg/snapshot-alice] frame={} bytes={} extraction_us={:.3} allocations={} overflow={}",frame,bytes,us,allocations,u8::from(overflow));
            }
        } else { alice.clear_body_input(); }
        if snapshot_enabled {
            let timing = snapshot_log.then(SnapshotTiming::start);
            #[cfg(target_os="vita")] let will_draw=!crate::frame_profile::mode().no_world() && !world_only;
            #[cfg(not(target_os="vita"))] let will_draw=!world_only;
            let visit = save::visit_key(&maps[current], entry_spawn.as_deref());
            power_art.prepare_visit(&pickup_input, &visit);
            let (generation, history) = power_art.capture_history();
            pickup_input.capture(generation, &pickups, &stats, &visit, camera, pickup_clock, will_draw, history);
            power_art.publish_input(&pickup_input, will_draw);
            if pickup_input.overflow() {snapshot_rejected=true;handoff_return.retire();handoff_minimum_frame=frame as u64;}
            if let Some(timing) = timing {
                let (us, allocations) = timing.finish();
                println!("[lg/snapshot-pickups] frame={} bytes={} extraction_us={:.3} allocations={} overflow={}",frame,pickup_input.bytes(),us,allocations,u8::from(pickup_input.overflow()));
            }
        }
        let steam_visit = level_art.snapshot_visit.id();
        if snapshot_enabled {
            let timing = snapshot_log.then(SnapshotTiming::start);
            let (bytes,overflow)=steam.extract_render_input(steam_visit);
            if overflow { snapshot_rejected=true;handoff_return.retire();handoff_minimum_frame=frame as u64;}
            if let Some(timing) = timing {
                let (us, allocations) = timing.finish();
                println!("[lg/snapshot-particles] frame={} bytes={} extraction_us={:.3} allocations={} overflow={}",frame,bytes,us,allocations,u8::from(overflow));
            }
        } else {steam.clear_render_input();}
        let empty_level_effects=snapshot_enabled && interactions.pandemonium.is_none() && interactions.duchess.is_none()
            && level_art.levels.iter().flatten().all(|a|a.snapshot_effects_empty());
        let wave_overflow=if snapshot_enabled {
            let timing = snapshot_log.then(SnapshotTiming::start);
            wave_input.clear();
            if let Some(s)=&interactions.encounters{s.extract_waves(&mut wave_input);}
            if let Some(s)=&interactions.school2{for b in &s.boojums{b.extract_waves(&mut wave_input);}}
            let overflow=wave_input.len()>16384;if overflow { snapshot_rejected=true;wave_input.clear();handoff_return.retire();handoff_minimum_frame=frame as u64;}
            if let Some(timing) = timing {
                let (us, allocations) = timing.finish();
                println!("[lg/snapshot-waves] frame={} bytes={} extraction_us={:.3} allocations={} overflow={} level_effects_complete={}",frame,std::mem::size_of_val(&wave_input)+wave_input.len()*std::mem::size_of::<crate::render_snapshot::WaveInput>(),us,allocations,u8::from(overflow),u8::from(empty_level_effects));
            }
            overflow
        } else {false};
        let aim_input=if snapshot_enabled {
            let timing = snapshot_log.then(SnapshotTiming::start);
            let can_aim = !interactions.level_blocks_weapons() && !flying
                && !pand_cinematic
                && scripted_camera.is_none()
                && !story.busy()
                && !menu
                && !inventory_menu
                && !console.open
                && stats.alive()
                && player.script_motion == 0
                && player.ledge.is_none()
                && !player.climbing();
            let (generation, rules) = aim_pointer.capture_rules();
            let input = crate::targeting::Input::capture(
                &rules, &crate::combat::Context { world: &world, targets: &targets },
                crate::targeting::Sight { eye: player.eye(), direction, weapon: can_aim.then(|| stats.equipped()).flatten() },
                generation, level_art.snapshot_visit.id(),
            );
            if let Some(timing) = timing {
                let (us, allocations) = timing.finish();
                println!("[lg/snapshot-aim] frame={} bytes={} extraction_us={:.3} allocations={} overflow=0",frame,std::mem::size_of_val(&input),us,allocations);
            }
            Some(input)
        } else {None};
        let first_person_input=snapshot_enabled.then(|| !world_only && !third_person && !pand_cinematic && !flying && stats.alive()
            && player.script_motion==0 && !interactions.school.as_ref().is_some_and(|s|s.cinematic()));
        let shadow_input=if snapshot_enabled {
            let timing = snapshot_log.then(SnapshotTiming::start);
            let will_draw=!world_only && dynamic_input.alice_shadow;
            #[cfg(target_os="vita")] let will_draw=will_draw && !crate::frame_profile::mode().no_world();
            let (input,queries)=if will_draw {crate::lighting::resolve_shadow(&world,dynamic_input.alice_feet,22.)}else{(None,0)};
            if let Some(timing) = timing {
                let (us, allocations) = timing.finish();
                println!("[lg/snapshot-shadow] frame={} bytes={} extraction_us={:.3} allocations={} queries={} overflow=0",frame,std::mem::size_of_val(&input),us,allocations,queries);
            }
            Some(input)
        } else {None};
        // Canonical audited blocker set, resolved before rendering. The same
        // set controls the real last-read boundary and the host census.
        let snapshot_blockers = if snapshot_enabled {
            let mut blockers = Vec::new();
            if snapshot_rejected || !alice.body_ready() || steam.draw_input(steam_visit).is_none()
                || level_art.pool.as_ref().is_some_and(|p| !p.snapshot_ready()) || !power_art.valid_input(&pickup_input)
                || aim_input.as_ref().is_none_or(|i| !aim_pointer.valid_input(i,level_art.snapshot_visit.id())) {
                blockers.push("input.rejected".to_owned());
            }
            #[cfg(feature="vita_renderer")]
            if load_probe.is_some() {blockers.push("diagnostic.load_probe".to_owned());}
            if scene.snapshot_portals_fenced() {blockers.push("render.camera_portal_lighting".to_owned());}
            if hud_snapshot.is_none() { blockers.push("hud.rejected".to_owned()); }
            if tail_survey_fence { blockers.push("tail.survey".to_owned()); }
            if tail_diagnostic_fence { blockers.push("tail.diagnostic".to_owned()); }
            if !level_dispatch.as_ref().is_some_and(|d| d.valid(&level_art)) {
                blockers.push("level.dispatch.rejected".into());
            }
            let mut art = |present: bool, name: &str| { if present { blockers.push(format!("story.{name}")); } };

            art(level_art.fortress.is_some(), "fortress");
            art(level_art.village.is_some(), "village");
            art(level_art.school.is_some(), "school");
            art(level_art.gym.is_some(), "gym");
            art(level_art.beyond.is_some(), "beyond");
            // Pool story values are resolved before its owned body extraction.
            art(level_art.school2.is_some(), "school2");
            art(level_art.duchess.is_some(), "duchess");
            art(level_art.pandemonium.is_some(), "pandemonium");
            art(level_art.encounters.as_ref().is_some_and(|a| !a.snapshot_ready()), "encounters");
            let registered: Vec<_> = level_art.levels.iter().zip(&interactions.levels)
                .filter_map(|(a,s)| a.as_ref().map(|a| (s.reg.id, a.snapshot_effects_empty(), a.snapshot_ready()))).collect();
            for (id, empty, ready) in &registered {
                if !ready { blockers.push(format!("story.registered.{id}")); }
                if !empty && !ready { blockers.push(format!("effects.registered.{id}")); }
                // Hatter1's legacy empty declaration is unsound for electricity.
                if *id=="hatter1" && !ready && *empty {blockers.push(format!("effects.registered.{id}"));}
            }
            if interactions.pandemonium.is_some() { blockers.push("effects.pandemonium".into()); }
            if interactions.duchess.is_some() { blockers.push("effects.duchess".into()); }
            if alice.has_live_summon() {
                blockers.push("alice.dice_summon".into());
            }
            let npc = npcs.snapshot_fence_census();
            blockers.extend(npc.keys().cloned());
            let save = save_requested || autosave_requested || finale_checkpoint_requested.is_some()
                || pending_menu_save.is_some() || pending_menu_newgame.is_some();
            if save { blockers.push("tail.save_checkpoint".into()); }

            if quit_after_frame || options.frames.is_some_and(|n|frame+1>=n) { blockers.push("tail.terminal".into()); }
            // Transition loading happens before extraction and is not a rendered tape frame.
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            if sim_audit.snapshot_census {
                let failed: Vec<_> = dispatch_failures.iter()
                    .filter_map(|&(condition, failed)| failed.then_some(condition)).collect();
                println!("SNAPSHOT_FENCE_CENSUS {}", serde_json::json!({
                    "frame": frame, "map": maps[current], "blockers": blockers, "npc_instances": npc,
                    "save_requested": save, "saves_writable": saves_writable,
                    "registered": registered, "transition_in_render": false, "survey": false,
                    "handoff_boundary": level_art.pool.is_some(),
                    "snapshot_accepted": blockers.is_empty() && !snapshot_tail_fenced,
                    "w0_eligible": w0_eligible, "w0_dispatched": dispatchable, "w0_failed": failed,
                }));
            }
            blockers
        } else {Vec::new()};
        let snapshot_world_free=snapshot_blockers.is_empty() && !snapshot_tail_fenced;
        #[cfg(any(debug_assertions, not(target_os = "vita")))]
        alice.require_body_input(snapshot_enabled && snapshot_world_free);
        crate::spike_log::world_free_decision(snapshot_world_free,frame as u64);
        if snapshot_enabled {
            snapshot_fenced_frames += u64::from(!snapshot_world_free);
            if snapshot_log {
                println!("[lg/snapshot-fence] frame={} fenced={} frames={} fenced_frames={}",
                    frame,u8::from(!snapshot_world_free),snapshot_frames,snapshot_fenced_frames);
            }
        }
        // All simulation queries and rejection decisions precede this boundary.
        // Remaining families/failures keep the conservative end-of-3D marker.
        let captured_hud_view = snapshot_world_free.then(|| HudView::capture(&input));
        crate::spike_log::world_last_read();
        let lag_free = lag_dispatch && worker_complete && snapshot_world_free && active && focused
            && stats.alive() && !paused && !menu && !inventory_menu && !console.open && !console_input
            && !audio.settings.muted
            && context_before == (menu, inventory_menu, console.open, paused)
            && skip_id.is_none() && !pand_cinematic && scripted_camera.is_none() && !story.busy()
            && !camera_handoff.was_scripted() && !freeze_capture && !opening_escape_menu
            && hud_snapshot.as_ref().is_some_and(|input| !input.help && !input.skip)
            && !route_running;
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        let lag_free = lag_free && steam_poison_frame != Some(frame);
        if lag_free {
            assert!(pending.is_none(), "pending frame was not drawn before publication");
            // The next pulse consumes this frame's already complete seal.
            if let Some((stamp, probe, path)) = worker_seal {
                assert_eq!(stamp, present_stamp, "stale worker seal stamp");
                assert!(stored_replay_probe.replace((stamp, probe, path, true)).is_none(),
                    "duplicate replay seal");
            } else {
                finish_replay_step!("tail");
            }
            #[cfg(not(target_os = "vita"))]
            { previous_step_exit = "tail"; }
            sim_audit.event("seal", present_stamp);
            sim_audit.event("publish", present_stamp);
            sim_audit.event("pending", present_stamp);
            sim_audit.frame(present_stamp, step_overlap, dispatch_failures, mid_step, true, true);
            #[cfg(target_os = "vita")]
            let draw_world = !crate::frame_profile::mode().no_world();
            #[cfg(not(target_os = "vita"))]
            let draw_world = true;
            #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
            let probe_state = crate::sim_maps::probe::enabled().then(|| serde_json::json!({
                "player": player, "stats": stats, "interactions": interactions.snapshot(),
                "npcs": npcs.snapshot(), "story": story.snapshot(), "hints": hints.snapshot(),
                "character": alice.snapshot(), "environment_clock": environment_clock,
                "pickup_clock": pickup_clock, "transforms": dynamic_input.transforms,
            }));
            pending = Some(lag_frame::Frame {
                stamp: present_stamp, clock: step_input.clock(), camera: world_camera, dynamic: dynamic_input,
                level: level_dispatch.expect("validated lag level input"),
                aim: aim_input.expect("validated lag aim input"),
                shadow: shadow_input.expect("captured lag shadow"),
                lights: extracted_lights.expect("captured lag lights"),
                first_person: first_person_input.expect("captured lag view"),
                empty_level_effects, fullbright, draw_world, world_only, dt, handoff_minimum_frame, snapshot_log,
                hud_input: hud_snapshot.expect("validated lag HUD input"),
                hud_view: captured_hud_view.expect("captured lag HUD view"),
                hud_ops: std::mem::take(&mut hud_ops), preferences: preferences.clone(), notice: notice.clone(),
                preview_capture, screenshot: physical_input && !bench && is_key_pressed(KeyCode::F12),
                input_started,
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                probe_state,
            });
            #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
            pending_probe.swap();
            frame += 1;
            continue;
        }
        if snapshot_enabled && snapshot_world_free { sim_audit.event("publish", present_stamp); }
        else { sim_audit.event("fallback", present_stamp); }
        let render_started = sim_trace.mark(present_stamp.frame.frame);
        let transforms = dynamic_input.transforms.as_slice();
        drop(census_block); census_block=crate::phase_census::span(crate::phase_census::Region::RenderSetup);
        if snapshot_world_free {
            #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
            let saved_steam = (steam_poison_frame == Some(frame)).then(|| {
                let saved = steam.poison_sim_for_draw(frame);
                if std::env::var_os("LOOKING_GLASS_SNAPSHOT_POISON_STEAM_RECAPTURE").is_some() {
                    let (_, overflow) = steam.extract_render_input(steam_visit);
                    assert!(!overflow, "poison recapture has to publish");
                }
                saved
            });
            #[cfg(target_os = "vita")]
            let draw_world = !crate::frame_profile::mode().no_world();
            #[cfg(not(target_os = "vita"))]
            let draw_world = true;
            let scopes = free_render::FreeRender {
                scene: &mut scene, level_art: &mut level_art,
                alice: alice.art_mut(), npcs: npcs.art_mut(), steam: steam.art(),
                power_art: &mut power_art, aim_pointer: &mut aim_pointer, hud: &hud,
                handoff_return: &mut handoff_return, overlap_probe: &mut overlap_probe,
                world_camera: &world_camera, dynamic: &dynamic_input,
                level: level_dispatch.as_ref().expect("validated dispatch"),
                pickups: &pickup_input, waves: &wave_input, aim: aim_input.as_ref().expect("validated aim"),
                shadow: shadow_input.expect("captured shadow"),
                lights: extracted_lights.expect("captured lights"),
                first_person: first_person_input.expect("captured first-person flag"),
                empty_level_effects, fullbright, draw_world, world_only,
                aim_dt: if active { dt } else { 0. }, frame, handoff_minimum_frame, snapshot_log,
            }.draw(census_block,
                #[cfg(target_os = "vita")]
                viewer_block,
                #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                host_block,
            );
            #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
            if let Some(sim) = saved_steam { steam.restore_sim_after_draw(sim); }
            census_block = scopes.census_block;
            #[cfg(target_os = "vita")]
            { viewer_block = scopes.viewer_block; }
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            { host_block = scopes.host_block; }
        } else {
            #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
            assert_ne!(steam_poison_frame, Some(frame), "steam poison requires a world-free frame");
        if snapshot_enabled {
            if level_art.pool.is_some() { handoff_return.expect(&level_art.snapshot_visit, 4, frame as u64); }
            crate::render_snapshot::handoff_render_begin(&level_art.snapshot_visit, frame as u64);
        }
        crate::spike_log::render_begin();
        overlap_probe.begin();
        #[cfg(target_os = "vita")]
        let _lights_profile = crate::frame_profile::span("lights_build");
        let live_lights = if snapshot_enabled { None } else {
            let mut lights = steam.lights();
            lights.extend(alice.lights());
            lights.extend(npcs.lights());
            for slot in &interactions.levels { lights.extend(slot.ctl.lights()); }
            Some(lights)
        };
        #[cfg(target_os = "vita")]
        drop(_lights_profile);
        #[cfg(target_os = "vita")]
        let draw_world = !crate::frame_profile::mode().no_world();
        #[cfg(not(target_os = "vita"))]
        let draw_world = true;
        if draw_world {
            scene.prepare_camera_portals(&world, &world_camera, environment_clock, fullbright, &transforms);
        }
        {
            #[cfg(target_os = "vita")]
            let _hotspot = crate::frame_profile::span("lights_select");
            if let Some(selection) = extracted_lights {
                crate::lighting::publish(selection);
            } else {
                crate::lighting::select(live_lights.unwrap(), camera, &world);
            }
        }
        if !world_only || !scene.retained_active() {
            scene.begin_fx_view(&world_camera, environment_clock, fullbright, true);
        } else {
            use macroquad::camera::Camera;
            crate::render_fx::set_projection(world_camera.matrix());
        }
        #[cfg(feature = "vita_renderer")]
        if let Some(p) = &load_probe {
            p.camera(&world_camera, tail_snapshot.as_ref().map_or_else(||player.feet,|s|s.feet), paused, skip_id);
            p.stage("skin-gather-begin");
        }
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        let host_gather=std::time::Instant::now();
        #[cfg(feature="vita_renderer")]
        {drop(census_block); census_block=crate::phase_census::span(crate::phase_census::Region::Other);}
        #[cfg(feature="vita_renderer")]
        let census_gather = crate::phase_census::span(crate::phase_census::Region::ActorGather);
        #[cfg(feature="vita_renderer")]
        let mut skin_frame = if crate::render::vita_renderer::config().skin_overlap && !world_only {
            #[cfg(target_os="vita")]
            {
                drop(viewer_block);
                viewer_block = crate::frame_profile::span("skin_gather");
                crate::frame_profile::set_owner(crate::frame_profile::Owner::Characters);
            }
            let snapshot_gather_start = snapshot_log.then(std::time::Instant::now);
            let gather_clock = crate::pass_clock::scope(crate::pass_clock::ACTOR_GATHER);
            let census_leaf = crate::phase_census::span(crate::phase_census::Region::GatherStoryPose);
            level_art.story_pose_dispatch(&story, level_dispatch.as_ref().is_some_and(|d| d.valid(&level_art)));
            drop(census_leaf); let census_leaf = crate::phase_census::span(crate::phase_census::Region::GatherFrameBegin);
            crate::audio::game_stage(3);
            let frame_index_for_snapshot = frame;
            let mut frame = crate::character::skinning::overlap::Frame::begin();
            drop(census_leaf); let census_leaf = crate::phase_census::span(crate::phase_census::Region::GatherAlice);
            if crate::character::skinning::overlap::single_frame() {
                if draw_world && (if snapshot_enabled {dynamic_input.alice_shadow} else {show_alice && stats.invisible<=0.}) { if let Some(input)=&shadow_input {crate::lighting::draw_shadow_input(input);}else{crate::lighting::shadow(&world,player.feet,22.);} }
                alice.atmosphere(&scene.atmosphere, camera);
            }
            if (if snapshot_enabled {dynamic_input.alice_visible} else {show_alice && !interactions.school.as_ref().is_some_and(|s|s.cinematic())}) {
                alice.draw_ghost(if snapshot_enabled {dynamic_input.alice_feet}else{player.feet}, fullbright, if snapshot_enabled {dynamic_input.alice_ghost}else{stats.invisible>0.});
            }
            drop(census_leaf); let census_leaf = crate::phase_census::span(crate::phase_census::Region::GatherNpcs);
            npcs.draw(camera, (target-camera).normalize_or_zero(), &scene.atmosphere, fullbright, snapshot_world_free);
            drop(census_leaf); let census_leaf = crate::phase_census::span(crate::phase_census::Region::GatherLevelArt);
            level_art.draw_dispatch(&interactions, level_dispatch.as_ref(), &hints, &scene.atmosphere, camera, fullbright);
            drop(census_leaf); let census_leaf = crate::phase_census::span(crate::phase_census::Region::GatherDispatch);
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
        } else { None };
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        { drop(host_block); host_block = crate::frame_profile::span("frame_world"); }
        #[cfg(target_os = "vita")]
        {
            drop(viewer_block);
            viewer_block = crate::frame_profile::span("frame_world");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::None);
            crate::phase_census::set_owner(crate::phase_census::Owner::Other);
        }
        #[cfg(feature = "vita_renderer")]
        if let Some(p) = &load_probe { p.stage("world-begin"); }
        #[cfg(feature="vita_renderer")]
        drop(census_gather);
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        crate::sim_maps::probe::span("actor_gather",host_gather.elapsed().as_secs_f64()*1000.);
        scene.draw(camera, environment_clock, fullbright, false, &transforms);
        #[cfg(feature = "vita_renderer")]
        if let Some(p) = &mut load_probe {
            p.world(scene.stats.batches, scene.stats.triangles, scene.retained_active());
            p.stage("skin-collect-begin");
        }
        #[cfg(feature="vita_renderer")]
        if let Some(frame) = &mut skin_frame {
            let _collect = crate::frame_profile::span("skin_collect");
            crate::audio::game_stage(2);
            let _census = crate::phase_census::span(crate::phase_census::Region::ActorCollect);
            let _clock = crate::pass_clock::scope(crate::pass_clock::ACTOR_COLLECT);
            frame.collect();
            crate::audio::game_stage(0);
        }
        drop(census_block); census_block = crate::phase_census::span(crate::phase_census::Region::ActorPublication);
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        { drop(host_block); host_block = crate::frame_profile::span("frame_characters"); }
        #[cfg(target_os = "vita")]
        {
            drop(viewer_block);
            viewer_block = crate::frame_profile::span("frame_characters");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::Characters);
        }
        #[cfg(not(target_os = "vita"))]
        let host_characters_profile = crate::frame_profile::span("frame_characters");
        #[cfg(feature = "vita_renderer")]
        if let Some(p) = &load_probe { p.stage("skin-collect-returned-actors-begin"); }
        let actors_timer = crate::light_profile::render_timer(crate::light_profile::RenderPhase::Actors);
        if !world_only && !crate::character::skinning::overlap::single_frame() {
        {
            let _characters_profile = crate::frame_profile::span("characters_setup");
            if draw_world && (if snapshot_enabled {dynamic_input.alice_shadow} else {show_alice && stats.invisible<=0.}) {
                if let Some(input)=&shadow_input {crate::lighting::draw_shadow_input(input);}else{crate::lighting::shadow(&world,player.feet,22.);}
            }
            alice.atmosphere(&scene.atmosphere, camera);
        }
        {
            let _characters_profile = crate::frame_profile::span("characters_alice");
            if (if snapshot_enabled {dynamic_input.alice_visible} else {show_alice && !interactions.school.as_ref().is_some_and(|s| s.cinematic())}) {
                alice.draw_ghost(if snapshot_enabled {dynamic_input.alice_feet}else{player.feet}, fullbright, if snapshot_enabled {dynamic_input.alice_ghost}else{stats.invisible>0.});
            }
        }
        {
            let _characters_profile = crate::frame_profile::span("characters_npcs");
            npcs.draw(
                camera,
                (target - camera).normalize_or_zero(),
                &scene.atmosphere,
                fullbright,
                snapshot_world_free,
            );
        }
        {
            let _characters_profile = crate::frame_profile::span("characters_level_art");
            #[cfg(feature="vita_renderer")]
            if skin_frame.is_none() { level_art.story_pose_dispatch(&story, level_dispatch.as_ref().is_some_and(|d| d.valid(&level_art))); }
            #[cfg(not(feature="vita_renderer"))]
            level_art.story_pose_dispatch(&story, level_dispatch.as_ref().is_some_and(|d| d.valid(&level_art)));
            level_art.draw_dispatch(&interactions, level_dispatch.as_ref(), &hints, &scene.atmosphere, camera, fullbright);
        }
        } // world-only diagnostic mask includes all character work
        #[cfg(feature="vita_renderer")]
        drop(skin_frame);
        if snapshot_enabled && level_art.pool.is_some() {
            let timing = snapshot_log.then(SnapshotTiming::start);
            let completion = level_art.handoff_completion(frame as u64, handoff_minimum_frame);
            assert!(handoff_return.publish(completion));
            if let Some(timing) = timing {
                let (us, allocations) = timing.finish();
                println!("[lg/snapshot-handoff] frame={} bytes={} extraction_us={:.3} allocations={}",
                    frame, handoff_return.bytes(), us, allocations);
            }
        }
        crate::render_snapshot::handoff_render_end();
        #[cfg(not(target_os = "vita"))]
        drop(host_characters_profile);
        drop(actors_timer);
        drop(census_block); census_block = crate::phase_census::span(crate::phase_census::Region::RenderSetup);
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        { drop(host_block); host_block = crate::frame_profile::span("frame_objects"); }
        #[cfg(target_os = "vita")]
        {
            drop(viewer_block);
            viewer_block = crate::frame_profile::span("frame_objects");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::None);
            crate::phase_census::set_owner(crate::phase_census::Owner::Other);
        }
        if draw_world && !world_only {
            if snapshot_enabled && power_art.valid_input(&pickup_input) {
                power_art.draw_input(&pickup_input,&scene.atmosphere,camera);
            } else {
            power_art.draw(
                &pickups,
                &stats,
                &save::visit_key(&maps[current], entry_spawn.as_deref()),
                &scene.atmosphere,
                camera,
                pickup_clock,
            );
                    }
        }
        drop(census_block); census_block = crate::phase_census::span(crate::phase_census::Region::Fx);
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        { drop(host_block); host_block = crate::frame_profile::span("frame_world_particles"); }
        #[cfg(target_os = "vita")]
        {
            drop(viewer_block);
            viewer_block = crate::frame_profile::span("frame_world_particles");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::None);
            crate::phase_census::set_owner(crate::phase_census::Owner::Other);
        }
        {
            let captured_steam = steam.draw_input(steam_visit);
            let particles = if snapshot_enabled && snapshot_world_free {
                captured_steam.expect("world-free steam input was accepted")
            } else {
                captured_steam.unwrap_or_else(|| steam.live_draw())
            };
            scene.draw_particle_frame(
                camera,
                (target - camera).normalize_or_zero(),
                environment_clock,
                fullbright,
                &transforms,
                particles,
            );
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            { drop(host_block); host_block = crate::frame_profile::span("frame_effects"); }
            #[cfg(target_os = "vita")]
            {
                drop(viewer_block);
                viewer_block = crate::frame_profile::span("frame_effects");
                crate::frame_profile::set_owner(crate::frame_profile::Owner::None);
            crate::phase_census::set_owner(crate::phase_census::Owner::Other);
            }
            if !world_only {
                if snapshot_enabled && !wave_overflow {for wave in &wave_input{wave.draw();}}
                else {
                if let Some(s) = &interactions.encounters {
                    s.draw_waves();
                }
                if let Some(s) = &interactions.school2 {
                    for b in &s.boojums {
                        b.draw_waves();
                    }
                }
                }
                if !empty_level_effects {
                if level_dispatch.as_ref().filter(|d| d.valid(&level_art)).is_none_or(|d| d.selected[3]) {
                    if let (Some(p), Some(art)) = (&interactions.pandemonium, &mut level_art.pandemonium) {
                        art.draw_effects(p, camera, &scene.atmosphere);
                    }
                }
                if level_dispatch.as_ref().filter(|d| d.valid(&level_art)).is_none_or(|d| d.selected[7]) {
                    if let (Some(d), Some(art)) = (&interactions.duchess, &mut level_art.duchess) {
                        art.effects(d, camera, &scene.atmosphere);
                    }
                }
                level_art.effects_dispatch(&interactions, level_dispatch.as_ref(), camera, &scene.atmosphere);
                }
                alice.draw_effects(camera, fullbright, if snapshot_enabled {dynamic_input.alice_effects}else{show_alice});
                if let Some(input)=aim_input.as_ref().filter(|i|aim_pointer.valid_input(i,level_art.snapshot_visit.id())) {aim_pointer.draw_input(input,level_art.snapshot_visit.id(),if active{dt}else{0.},camera,&scene.atmosphere);}
                else {
                let can_aim = !interactions.level_blocks_weapons() && !flying
                    && !pand_cinematic
                    && scripted_camera.is_none()
                    && !story.busy()
                    && !menu
                    && !inventory_menu
                    && !console.open
                    && stats.alive()
                    && player.script_motion == 0
                    && player.ledge.is_none()
                    && !player.climbing();
                aim_pointer.draw(
                    &crate::combat::Context {
                        world: &world,
                        targets: &targets,
                    },
                    crate::targeting::Sight {
                        eye: player.eye(),
                        direction,
                        weapon: can_aim.then(|| stats.equipped()).flatten(),
                    },
                    if active { dt } else { 0. },
                    camera,
                    &scene.atmosphere,
                );
                }
                if snapshot_enabled && power_art.valid_input(&pickup_input) {hud.draw_pickup_markers(&pickup_input.markers,pickup_clock);}
                else {hud.draw_pickups(&pickups,&stats,camera,pickup_clock);}
            }
        }
        if draw_world && !world_only {
            crate::lighting::flares(camera, (target - camera).normalize_or_zero());
        }
        let direct_fx = crate::render_fx::retained_active();
        if !world_only || !scene.retained_active() { scene.finish_fx(true); }
        drop(census_block); census_block = crate::phase_census::span(crate::phase_census::Region::FirstPerson);
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        { drop(host_block); host_block = crate::frame_profile::span("frame_first_person"); }
        #[cfg(target_os = "vita")]
        {
            drop(viewer_block);
            viewer_block = crate::frame_profile::span("frame_first_person");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::None);
            crate::phase_census::set_owner(crate::phase_census::Owner::Other);
        }
        if first_person_input.unwrap_or_else(|| !world_only && !third_person
            && !pand_cinematic
            && !flying
            && stats.alive()
            && player.script_motion == 0
            && !interactions.school.as_ref().is_some_and(|s| s.cinematic()))
        {
            if direct_fx {
                // All 3-D packets have reached miniquad; the view toy needs only a depth clear.
                crate::render::clear_direct_view_depth();
                scene.begin_fx_view(&world_camera, environment_clock, fullbright, false);
                alice.draw_first_person(fullbright);
                scene.finish_fx(false);
            } else {
                crate::render::clear_view_depth();
                alice.draw_first_person(fullbright);
            }
        }
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        { drop(host_block); host_block = crate::frame_profile::span("frame_capture"); }
        #[cfg(target_os = "vita")]
        {
            drop(viewer_block);
            viewer_block = crate::frame_profile::span("frame_capture");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::None);
            crate::phase_census::set_owner(crate::phase_census::Owner::Other);
        }
        }
        sim_trace.owner(present_stamp, sim_timing::Scope::Render, None, render_started);
        if !snapshot_world_free {crate::spike_log::world_last_read();}
        #[cfg(feature="vita_renderer")]
        crate::render::vita_renderer::present_3d();
        #[cfg(feature = "vita_renderer")]
        if let Some(p) = &load_probe { p.gl("present-queued"); }
        if preview_capture {
            let _profile = crate::frame_profile::span("save_preview");
            save_frame.update();
        }
        if freeze_capture { paused = pause_before_capture; clock.pause(); }
        if opening_escape_menu || pending_menu_save.is_some() { escape_preview_ready = true; }
        drop(census_block); census_block = crate::phase_census::span(crate::phase_census::Region::Hud);
        crate::spike_log::hud_begin();
        overlap_probe.finish();
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        { drop(host_block); host_block = crate::frame_profile::span("frame_hud"); }
        #[cfg(target_os = "vita")]
        {
            drop(viewer_block);
            viewer_block = crate::frame_profile::span("frame_hud");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::Hud);
        }
        hud.apply(&mut hud_ops);
        // Owner transactions can update the live Input before this HUD pass.
        let hud_view = captured_hud_view.unwrap_or_else(|| HudView::capture(&input));
        let hud_stats = hud_snapshot.as_ref().map_or_else(|| &stats, |s| &s.stats);
        let hud_breath = hud_snapshot.as_ref().map_or_else(|| &player.breath, |s| &s.breath);
        let hud_immersion = hud_snapshot.as_ref().map_or_else(|| player.immersion.level, |s| s.immersion);
        let hud_script_motion = hud_snapshot.as_ref().map_or_else(|| player.script_motion, |s| s.script_motion);
        let hud_swimming = hud_snapshot.as_ref().map_or_else(|| player.swimming, |s| s.swimming);
        let hud_scripted = hud_snapshot.as_ref().map_or_else(|| interactions.scripted(), |s| s.scripted);
        let mut hud_path=crate::phase_census::span(crate::phase_census::Region::HudDraw);
        crate::phase_census::count_under(crate::phase_census::Region::Hud,crate::phase_census::Counter::HudDraw,1);
        set_default_camera();
        let hud_timer = crate::light_profile::render_timer(crate::light_profile::RenderPhase::Hud);
        #[cfg(feature = "vita_renderer")]
        let mut hud_submitted = false;
        if !world_only {
            if active {
                #[cfg(target_os = "vita")]
                let _search_profile = crate::frame_profile::span("interaction_search_after");

                drop(hud_path);
                let search_path=crate::phase_census::span(crate::phase_census::Region::HudSearch);
                crate::phase_census::count_under(crate::phase_census::Region::Hud,crate::phase_census::Counter::HudSearch,1);
                let prompt = if let Some(snapshot) = &hud_snapshot {
                    snapshot.prompt.clone()
                } else { resolve_prompt!() };
                drop(search_path);
                hud_path=crate::phase_census::span(crate::phase_census::Region::HudDraw);
                crate::phase_census::count_under(crate::phase_census::Region::Hud,crate::phase_census::Counter::HudDraw,1);
                if let Some(prompt) = prompt {
                    let prompt = crate::prompts::world_prompt(&prompt, &hud_view, &preferences);
                    ui.toast(&prompt, screen_height() - 112.);
                }
            }
            let help = hud_snapshot.as_ref().map_or_else(|| get_time() < help_until, |s| s.help);
            if hud_immersion == 3
                && hud_stats.alive()
                && !menu
                && !inventory_menu
                && !console_input
            {
                crate::hud::air(hud_breath, &ui);
            }
            let objective = if let Some(snapshot) = &hud_snapshot {
                snapshot.objective.clone()
            } else { resolve_objective!(help) };
            let show_details = help || menu;
            if paused && !menu && !inventory_menu && !console_input && !help {
                ui.paused(&if hud_view.handheld {
                    "Cross or Start to resume".into()
                } else if hud_view.desktop_pad() {
                    "A or Start to resume".into()
                } else {
                    format!("{} or click to resume", hud_view.label(&preferences, "P"))
                });
            }
            if help && !menu && !inventory_menu && !console_input {
                let canvas = crate::ui::Canvas::new(screen_width(), screen_height());
                ui.panel(canvas.rect(Rect::new(16., 118., 608., 343.)));
                ui.font.draw(
                    canvas,
                    "A little guidance",
                    Rect::new(38., 133., 564., 32.),
                    28.,
                    crate::ui::INK,
                );
                if let Some(objective) = &objective {
                    let objective = crate::prompts::shown(objective, &hud_view, &preferences);
                    let rows = crate::story::wrap(&objective, 548., |t| ui.body.width(t, 14.));
                    for (i, line) in rows.iter().take(3).enumerate() {
                        ui.body.draw_left(
                            canvas,
                            line,
                            Rect::new(46., 174. + i as f32 * 18., 548., 18.),
                            14.,
                            crate::ui::INK,
                        );
                    }
                }
                let movement = if hud_view.desktop_pad() || hud_view.handheld {
                    format!(
                        "Left stick: move   {}: jump / rise   {}: dive",
                        hud_view.label(&preferences, "Space"),
                        hud_view.label(&preferences, "Ctrl")
                    )
                } else {
                    preferences.controls_text(if flying {
                        "W A S D move   Q / E down / up   Shift faster"
                    } else if hud_script_motion == 1 {
                        "W A S D swing   Space / Ctrl climb   E release"
                    } else if hud_swimming {
                        "W A S D swim   Space rise   Ctrl dive   E use"
                    } else {
                        "W A S D move   Space jump   Shift run   E interact"
                    })
                };
                let lines = [
                    movement,
                    format!(
                        "{}: look   {}: change view",
                        if hud_view.desktop_pad() || hud_view.handheld {
                            "Right stick"
                        } else {
                            "Mouse / look keys"
                        },
                        hud_view.label(&preferences, "V")
                    ),
                    format!(
                        "{} / {}: primary / alternate attack",
                        hud_view.label(&preferences, "Mouse 1"),
                        hud_view.label(&preferences, "Mouse 2")
                    ),
                    format!(
                        "{} / {}: toy   {}: inventory",
                        hud_view.label(&preferences, "Wheel Up"),
                        hud_view.label(&preferences, "Wheel Down"),
                        hud_view.label(&preferences, "I")
                    ),
                    format!(
                        "{}: Cheshire hints    {}: advance dialogue",
                        hud_view.label(&preferences, "C"),
                        hud_view.label(&preferences, "E")
                    ),
                    format!(
                        "{}: recover    Save / Load in the main menu",
                        hud_view.label(&preferences, "R")
                    ),
                    format!(
                        "{}: menus   {}: chapters   {}: help",
                        if hud_view.desktop_pad() || hud_view.handheld { "Start" } else { "Esc" },
                        hud_view.label(&preferences, "Tab"),
                        hud_view.label(&preferences, "H")
                    ),
                ];
                for (i, line) in lines.iter().enumerate() {
                    ui.font.draw_left(
                        canvas,
                        line,
                        Rect::new(46., 239. + i as f32 * 27., 548., 24.),
                        21.,
                        crate::ui::INK,
                    );
                }
            }
            if menu {
                let _menu_path=crate::phase_census::span(crate::phase_census::Region::HudMenus);
                crate::phase_census::count_under(crate::phase_census::Region::Hud,crate::phase_census::Counter::HudMenus,1);
                chapters.draw(
                    &ui,
                    &level_choices,
                    &maps,
                    options.difficulty,
                    focused && !console_input,
                    hud_view.desktop_pad(),
                );
            }
            if (!show_details || inventory_menu) && !hud_scripted {
                let inventory_path=inventory_menu.then(||crate::phase_census::span(crate::phase_census::Region::HudMenus));
                if inventory_menu {crate::phase_census::count_under(crate::phase_census::Region::Hud,crate::phase_census::Counter::HudMenus,1);}
                hud.draw(hud_stats, &catalog, inventory_menu, &hud_view, &preferences);
                drop(inventory_path);
                #[cfg(feature = "vita_renderer")]
                { hud_submitted = true; }
                if !paused && !inventory_menu && !console_input && hud_stats.alive() {
                    hud.draw_notice(&hud_view, &preferences);
                }
            }
            if !menu && !inventory_menu && !console_input && hud_stats.alive() {
                if let Some(snapshot) = &hud_snapshot {
                    for color in &snapshot.fades {
                        draw_rectangle(0., 0., screen_width(), screen_height(), *color);
                    }
                } else {
                let cover = camera_handoff.overlay();
                if cover.a > 0. {
                    draw_rectangle(0., 0., screen_width(), screen_height(), cover);
                }
                if let Some(f) = &interactions.fortress {
                    let (mut color, alpha) = f.cinema.fade(&f.state.cinema);
                    color.a = alpha;
                    if alpha > 0. {
                        draw_rectangle(0., 0., screen_width(), screen_height(), color);
                    }
                }
                if let Some(p) = &interactions.pandemonium {
                    let fade = p.fade();
                    if fade > 0. {
                        draw_rectangle(
                            0.,
                            0.,
                            screen_width(),
                            screen_height(),
                            Color {
                                a: fade,
                                ..p.fade_color()
                            },
                        );
                    }
                }
                if let Some(v) = &interactions.village {
                    let (mut color, alpha) = v.cinema.fade();
                    color.a = alpha;
                    if alpha > 0. {
                        draw_rectangle(0., 0., screen_width(), screen_height(), color);
                    }
                }
                if let Some(p) = &interactions.pool {
                    let (mut color, alpha) = p.fade();
                    color.a = alpha;
                    if alpha > 0. {
                        draw_rectangle(0., 0., screen_width(), screen_height(), color);
                    }
                }
                if let Some(s) = &interactions.school {
                    let (mut color, alpha) = s.scene_fade();
                    color.a = alpha;
                    if alpha > 0. {
                        draw_rectangle(0., 0., screen_width(), screen_height(), color);
                    }
                }
                if let Some(b) = &interactions.beyond {
                    let (mut color, alpha) = b.scene_fade();
                    color.a = alpha;
                    if alpha > 0. {
                        draw_rectangle(0., 0., screen_width(), screen_height(), color);
                    }
                }
                if let Some((mut color, alpha)) = interactions.fade_after(None) {
                    color.a = alpha;
                    if alpha > 0. {
                        draw_rectangle(0., 0., screen_width(), screen_height(), color);
                    }
                }
                }
                if preferences.subtitles {
                    if let Some(snapshot) = &hud_snapshot {
                        if let Some(subtitle) = &snapshot.subtitle {
                            subtitle.draw(&hud_view.label(&preferences, "E"), &ui);
                        }
                    } else { story.draw(&hud_view.label(&preferences, "E"), &ui); }
                }
                if let Some(snapshot) = &hud_snapshot {
                    for command in &snapshot.level { command.draw(); }
                } else {
                if let (Some(d), Some(art)) = (&interactions.duchess, &level_art.duchess) {
                    art.hud(d);
                }
                level_art.hud(&interactions);
                }
                if hud_snapshot.as_ref().map_or_else(|| resolve_skip!(), |s| s.skip)
                {
                    skip_scene.draw(
                        &ui,
                        &hud_view.film_skip_label(&preferences),
                    );
                }
            }
            if audio.settings.muted && help && !inventory_menu && !console_input {
                ui.toast(&crate::prompts::shown("Sound muted - M to restore", &hud_view, &preferences), 18.);
            }
            if !notice.is_empty() {
                ui.toast(&notice, 74.);
            }
            show_mouse(!focused);
            if focused && (menu || inventory_menu || paused) {
                ui.cursor();
            }
            {let _menu_path=crate::phase_census::span(crate::phase_census::Region::HudMenus);
             crate::phase_census::count_under(crate::phase_census::Region::Hud,crate::phase_census::Counter::HudMenus,1);
             console.draw(&ui);}
        }
        drop(hud_path);
        drop(census_block); census_block = crate::phase_census::span(crate::phase_census::Region::FrameworkTail);
        let save_path=crate::phase_census::span(crate::phase_census::Region::TailSave);
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        { drop(host_block); host_block = crate::frame_profile::span("frame_save"); }
        #[cfg(target_os = "vita")]
        {
            drop(viewer_block);
            viewer_block = crate::frame_profile::span("frame_save");
            crate::frame_profile::set_owner(crate::frame_profile::Owner::None);
            crate::phase_census::set_owner(crate::phase_census::Owner::Other);
        }
        drop(hud_timer);
        let tail_context_changed = context_before != (menu, inventory_menu, console.open, paused);
        let free_tail = snapshot_world_free && !tail_context_changed && (!dispatchable || worker_complete);
        let tail_hud_len = hud_ops.len();
        if free_tail {
            assert!(!tail_save_fence, "free tail would mutate save state");
        } else {
            if let Some(slot) = pending_menu_save.take() {
                escape_menu.message = match store.write_with_preview(slot, &captured_save.as_ref().cloned().unwrap_or_else(|| game_snapshot!()), save_frame.preview().as_ref()) {
                    Ok(()) => { retry.remember(captured_save.as_ref().cloned().unwrap_or_else(|| game_snapshot!())); played = true; format!("Game saved in {}", slot.title()) }
                    Err(e) => format!("Save failed: {e:#}"),
                };
            }
            if let Some(difficulty) = pending_menu_newgame.take() {
                let saved = if saves_writable && stats.alive() && !post_game {
                    store.write_with_preview(Slot::Auto, &captured_save.as_ref().cloned().unwrap_or_else(|| game_snapshot!()), save_frame.preview().as_ref())
                } else { Ok(()) };
                if !saves_enabled {
                    escape_menu.open(crate::menu::Page::Main, &preferences, audio.settings);
                    escape_menu.message = "New Game is unavailable in a staged preview".into();
                } else if let Err(e) = saved {
                    escape_menu.open(crate::menu::Page::Main, &preferences, audio.settings);
                    escape_menu.message = format!("Could not preserve the current game: {e:#}");
                } else {
                    post_game = false; escape_menu.post_game = false; new_game_requested = Some(difficulty);
                }
            }
            if let Some(game) = finale_checkpoint_requested {
                match acknowledge_checkpoint(
                    store.write_finale_checkpoint_with_preview(&game, save_frame.preview().as_ref()),
                    &mut interactions.levels,
                ) {
                    Ok(()) => {
                        retry.remember(game_snapshot!()); hud_ops.push(HudOp::Announce("Checkpoint saved".into()));
                    }
                    Err(e) => hud_ops.push(HudOp::Announce(format!("Checkpoint save failed: {e:#}").into())),
                }
            }
            if save_requested || autosave_requested {
                if !saves_writable {
                    if save_requested {
                        hud_ops.push(HudOp::Announce("Saving is unavailable in a staged preview".into()));
                    }
                } else if !stats.alive() {
                    if save_requested {
                        hud_ops.push(HudOp::Announce("Retry or load a game before saving".into()));
                    }
                } else {
                    let slot = if save_requested {
                        Slot::Quick
                    } else {
                        Slot::Auto
                    };
                    match store.write_with_preview(
                        slot,
                        &captured_save.as_ref().cloned().unwrap_or_else(|| game_snapshot!()),
                        save_frame.preview().as_ref(),
                    ) {
                        Ok(()) => {
                            retry.remember(captured_save.as_ref().cloned().unwrap_or_else(|| game_snapshot!()));
                            played = true;
                            hud_ops.push(HudOp::Announce((if slot == Slot::Quick {
                                "Game saved / F9 to load"
                            } else {
                                "Progress saved"
                            }).into()));
                            if save_requested {
                                console.print("Game saved / F9 to load");
                            }
                        }
                        Err(e) => {
                            let message = format!("Save failed: {e:#}");
                            hud_ops.push(HudOp::Announce(message.clone().into()));
                            console.print(message);
                        }
                    }
                }
            }
        }
        #[cfg(target_os = "vita")]
        drop(viewer_block);
        drop(save_path);
        // Both immediate and deferred checkpoint acknowledgements have completed.
        if free_tail {
            assert_eq!(tail_hud_len, hud_ops.len(), "free tail pushed a HUD operation");
        }
        if let Some((stamp, probe, path)) = worker_seal.filter(|_| free_tail) {
            assert_eq!(stamp, present_stamp, "stale worker seal stamp");
            assert!(stored_replay_probe.replace((stamp, probe, path, true)).is_none(),
                "duplicate replay probe for one step");
            #[cfg(not(target_os = "vita"))]
            { previous_step_exit = "tail"; }
        } else {
            finish_replay_step!("tail");
        }
        sim_audit.event("seal", present_stamp);
        let profiler_path=crate::phase_census::span(crate::phase_census::Region::TailProfiler);
        if !active {
            frame_profile.cancel();
        }
        drop(frame_profile);
        crate::character::skinning_bench_end();
        #[cfg(feature="native_timing")]
        if let Some(text)=timing.schedule_anchor(tail_snapshot.as_ref().map_or_else(|| timing_state!(), |s| s.timing)) {
            if text.as_str().starts_with("run=") {
                use crate::native_timing::overlay;
                draw_rectangle(0.,0.,screen_width(),overlay::BOX_HEIGHT as f32,BLACK);
                overlay::rectangles(text.as_str(),|x,y,w,h|draw_rectangle(x as f32,y as f32,w as f32,h as f32,WHITE));
            } else {
                draw_rectangle(0.,0.,screen_width(),42.,BLACK);
                draw_text(text.as_str(),4.,16.,12.,WHITE);
            }
        }
        #[cfg(feature = "native_timing")]
        timing.with(|r| {
            use crate::native_timing as nt;
            let mut reasons = 0;
            if !active { reasons |= nt::INACTIVE; }
            if paused { reasons |= nt::PAUSED; }
            if !focused { reasons |= nt::UNFOCUSED; }
            if menu || inventory_menu || console_input { reasons |= nt::OVERLAY; }
            if !tail_snapshot.as_ref().map_or_else(|| stats.alive(), |s| s.alive) { reasons |= nt::DEAD; }
            if flying { reasons |= nt::FLYING; }
            if !simulate { reasons |= nt::FROZEN_SIM; }
            if tail_snapshot.as_ref().map_or_else(|| skip_id.is_some() || pand_cinematic || scripted_camera.is_some() || story.busy() || interactions.school.as_ref().is_some_and(|s| s.cinematic()), |s| s.cinema) { reasons |= nt::CINEMA; }
            r.end(tail_snapshot.as_ref().map_or_else(|| timing_state!(), |s| s.timing), reasons);
        });
        drop(profiler_path);
        drop(census_block);
        let census_tail=crate::phase_census::span(crate::phase_census::Region::FrameworkTail);
        crate::light_profile::cpu_end();
        crate::frame_phases::mark(2);
        crate::spike_log::cpu_end();
        // Save completion and optional diagnostics are serial fences. Ordinary
        // enabled frames have consumed owned HUD and timing inputs above.
        if snapshot_tail_fenced { crate::spike_log::world_last_read(); }
        #[cfg(target_os = "vita")]
        {
            crate::frame_phases::mark(3);
            let gameplay = active && !tail_snapshot.as_ref().map_or_else(|| skip_id.is_some() || pand_cinematic || scripted_camera.is_some() || story.busy() || interactions.school.as_ref().is_some_and(|s| s.cinematic()), |s| s.cinema);
            let report_due = crate::frame_profile::end_frame(&maps[current], gameplay);
            #[cfg(feature = "native_timing")]
            timing.with(|r| r.measured(crate::frame_profile::native_timing_measured_id()));
            if bench && gameplay && crate::frame_profile::bench_accepted_frames().is_some_and(|n| n > 0)
                && bench_camera.is_none()
            {
                bench_camera = Some((camera, target, gameplay_alice));
                println!("bench: camera frozen map={} feet={:?}", maps[current], tail_snapshot.as_ref().map_or_else(|| player.feet, |s| s.feet));
            }
            // Report at the next begin_frame, once the wall sample is complete.
            profile_report_due = report_due;
        }
        #[cfg(target_os="vita")]
        if let Some(s)=&mut r13_survey {s.accept(active && tail_snapshot.as_ref().map_or_else(|| stats.alive(), |s| s.alive) && !paused && !menu && !inventory_menu && !console.open
            && !tail_snapshot.as_ref().map_or_else(|| skip_id.is_some() || pand_cinematic || scripted_camera.is_some() || story.busy()
            || interactions.school.as_ref().is_some_and(|s|s.cinematic()), |s| s.cinema));}
        #[cfg(not(target_os="vita"))]
        if !route_running {crate::frame_profile::end_creation_frame();}
        let terminal = quit_after_frame || options.frames.is_some_and(|n| frame + 1 >= n);
        let terminal_present = terminal.then(|| sim_trace.mark(present_stamp.frame.frame)).flatten();
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        if !crate::sim_maps::probe::enabled() && crate::replay::input::cross() { crate::sim_maps::probe::shot(present_stamp.frame.frame)?; }
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        if crate::sim_maps::probe::enabled() {
            drop(host_block);
            // Every frame's pixels are hashed; the domain edges also keep a PNG.
            #[cfg(feature = "vita_renderer")]
            crate::sim_maps::probe::record_exact_frame(&maps[current], present_stamp.frame.frame,
                matches!(present_stamp.frame.frame, 0 | 59 | 60 | 119 | 120 | 179))?;
            crate::sim_maps::probe::shot(present_stamp.frame.frame)?;
            let state = serde_json::json!({
                    "player": player, "stats": stats, "interactions": interactions.snapshot(),
                    "npcs": npcs.snapshot(), "story": story.snapshot(), "hints": hints.snapshot(),
                    "character": alice.snapshot(), "environment_clock": environment_clock,
                    "pickup_clock": pickup_clock, "transforms": transforms,
            });
            crate::sim_maps::probe::end(present_stamp.frame.frame, &maps[current], active, &state);
        }
        #[cfg(feature = "vita_renderer")]
        let presented_probe = load_probe.take();
        #[cfg(feature = "vita_renderer")]
        if let Some(p) = &presented_probe { p.finish(&maps[current], hud_submitted); }
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        if crate::transition_memory::host_probe() {
            println!("TRANSITION_STEADY {}", serde_json::json!({"frame":frame,"map":maps[current],"live_rust_request_bytes":crate::kernel_alloc::live_bytes(),"audio_cache_clips_retiring_effects_emitters":audio.memory_counts()}));
            use sha2::{Digest,Sha256};
            let value = game_snapshot!();
            println!("TRANSITION_STATE {}", serde_json::json!({"frame":frame,"map":maps[current],
                "state_sha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&value)?))}));
        }
        if terminal {
            sim_trace.owner(present_stamp, sim_timing::Scope::PresentWithoutYield, None, terminal_present);
            sim_trace.owner(present_stamp, sim_timing::Scope::InputToPresent, None, input_started.take());
            sim_audit.event("present", present_stamp);
            last_present = Some(present_stamp.frame.frame);
        }
        sim_audit.frame(present_stamp, step_overlap, dispatch_failures, mid_step,
            snapshot_enabled && snapshot_world_free, free_tail);
        frame += 1;
        if physical_input && !bench && is_key_pressed(KeyCode::F12) {
            save_capture(&std::path::PathBuf::from(format!(
                "private/screenshots/{}-{}.png",
                maps[current], present_stamp.frame.frame + 1
            )))?;
        }
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        crate::phase_census::accept(active, frame > 60);
        if quit_after_frame {
            if snapshot_tail_fenced { crate::spike_log::world_last_read(); }
            flush_audio!("quit-after-frame");
            break;
        }
        if options.frames.is_some_and(|limit| frame >= limit) {
            #[cfg(all(feature="vita_renderer",not(target_os="vita")))]
            println!("[lg/vita-renderer] host frame report {}",crate::render::vita_renderer::report());
            #[cfg(not(target_os="vita"))]
            println!("[lg/shaders] host frame creations {}",crate::frame_profile::creation_report());
            crate::frame_profile::report(&maps[current])?;
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            crate::phase_census::host_finish(&maps[current])?;
            if let Some(path) = &options.capture {
                save_capture(path)?;
            }
            flush_audio!("frame-limit");
            break;
        }
        if tail_context_changed {
            assert!(!free_tail, "free tail would suppress simulation input");
            input.suppress();
        }
        if snapshot_tail_fenced { crate::spike_log::world_last_read(); }
        crate::frame_phases::mark(9);
        drop(census_tail);
        #[cfg(feature = "vita_renderer")]
        if replay.is_some() {
            let step = present_stamp.frame.tape_step;
            if crate::replay_capture::wanted(step) {
                crate::replay_capture::save(step)?;
            }
        }
        drop(step_guards.take());
        let present_started = sim_trace.mark(present_stamp.frame.frame);
        next_frame().await;
        sim_trace.owner(present_stamp, sim_timing::Scope::Present, None, present_started);
        sim_trace.owner(present_stamp, sim_timing::Scope::InputToPresent, None, input_started);
        sim_audit.event("present", present_stamp);
        last_present = Some(present_stamp.frame.frame);
        #[cfg(feature = "vita_renderer")]
        if let Some(p) = presented_probe { p.gl("frame-presented"); p.stage("frame-yield-returned"); }
        audio.scene_presented();
        #[cfg(target_os="vita")]
        if let Some(s)=&mut r13_survey {
            s.presented(&maps[current],tail_snapshot.as_ref().map_or_else(|| stats.alive(), |s| s.alive),is_key_pressed(KeyCode::Tab))?;
            if s.take_snapshot() {
                crate::spike_log::world_last_read();
                crate::allocation_census::snapshot("presented");
                if crate::allocation_census::enabled() {
                    crate::allocation_census::payload("live_collision",world.census_payload());
                    crate::allocation_census::payload("live_audio",audio.census_payload());
                }
            }
        }
        if snapshot_tail_fenced { crate::spike_log::world_last_read(); }
    }
    assert!(pending.is_none(), "viewer exited with an unpublished presentation");
    sim_audit.finish(frame);
    if sim_overlap {
        drop(sim_worker.take());
        println!("[lg/sim-overlap] dispatched_frames={dispatched_frames}");
    }
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    crate::collision::lazy_profile::report();
    sim_audit.drain(last_present, frame);
    sim_trace.flush();
    crate::spike_log::end_play();
    #[cfg(not(target_os = "vita"))]
    if replay.is_some() {
        println!("[lg/replay-probe] assertions={replay_probe_assertions} probe.fallback={replay_probe_fallbacks}");
    }
    if let Some(r) = &mut replay { r.finish(replay_end.unwrap_or("exit")); }
    // Host-only, sim_maps_probe-only: the allocator ledger's continuous peak since the
    // map load began (transition_begin resets it) against the 192 MiB newlib ceiling.
    // The host request ledger is a lower bound for the Vita's newlib chunks; the
    // chunk model applies the 32-bit newlib minimum-chunk rounding.
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    if replay.is_some() {
        let [peak_live, peak_chunk] = crate::kernel_alloc::peak_bytes();
        println!("[lg/host-heap] peak_live_bytes={peak_live} peak_newlib32_chunk_bytes={peak_chunk} heap_ceiling_bytes={}",
            192 * 1024 * 1024);
    }
    #[cfg(target_os = "vita")]
    let saves_writable = crate::frame_profile::saves_writable(saves_enabled, bench_session);
    #[cfg(not(target_os = "vita"))]
    let saves_writable = saves_enabled;
    #[cfg(feature = "native_timing")]
    if let Err(error) = timing.export_after_viewer() { eprintln!("Native timing export failed: {error:#}"); }
    if saves_writable && played && stats.alive() && options.frames.is_none() && !post_game {
        if let Err(e) =
            store.write_with_preview(Slot::Auto, &game_snapshot!(), save_frame.preview().as_ref())
        {
            eprintln!("Automatic save failed: {e:#}");
        }
    }
    println!(
        "World interactions: {} uses, {} exit/teleport contacts",
        interactions.used, interactions.touched
    );
    mouse_look.release();
    println!("NPC greetings: {}", npcs.greetings());
    if options.shelf_preview {
        println!(
            "Shelf preview ended: feet={:?}, grounded={}, body_clear={}, jumps={}",
            player.feet,
            player.grounded,
            world.body_clear(player.feet),
            player.jumps
        );
    }
    println!(
        "Weapon actions: {} emitted, {} impacts",
        alice.visual_counts().0,
        alice.visual_counts().1
    );
    println!(
        "Viewer closed cleanly. Data folder: {}",
        assets.base.display()
    );
    let mut memory = crate::transition_memory::Transition::begin("quit", &maps[current], "menu-or-exit");
    let mut outgoing = unload_scene!();
    drop(alice);
    crate::transition_memory::released().await;
    memory.after_release();
    audio.finish()?;
    drop(outgoing);
    memory.finish(true);
    Ok(())
}

/// Alice feet x positions per fixture frame. The host oracle checks each domain:
/// 0-89 three draws (warm-up, then steady), 90-119 five draws (the fourth
/// publishes and the fifth stays serial with occurrences on), 120-149 two draws
/// in reversed order (the third occurrence goes unused), 150-179 three draws
/// whose order changes every frame.
#[cfg(all(feature="vita_renderer", feature="sim_maps_probe", not(target_os="vita")))]
fn occurrence_fixture_draws(frame: usize) -> &'static [f32] {
    match frame {
        0..=89 => &[-40., 0., 40.],
        90..=119 => &[-80., -40., 0., 40., 80.],
        120..=149 => &[40., -40.],
        _ if frame % 2 == 0 => &[0., 40., -40.],
        _ => &[-40., 0., 40.],
    }
}

/// Alice drawn several times per frame through the overlap render-check path.
/// Host render-check only. Puppet loads stay on the serial puppet reason, and the
/// map entries do not repeat an eligible surface, so this is the publication witness.
#[cfg(all(feature="vita_renderer", feature="sim_maps_probe", not(target_os="vita")))]
async fn publication_occurrence_fixture(assets: &mut Assets, options: &Options) -> Result<()> {
    let mut active = enter_level(assets, "gvillage", None, None, options, true)?;
    let mut alice = Character::load(assets)?;
    alice.reset(&Player::new(Vec3::ZERO), 0.);
    crate::character::skinning::overlap::startup(2, false);
    for frame in 0..180 {
        crate::phase_census::begin_configured();
        crate::sim_maps::probe::begin();
        let view = Camera3D { position: vec3(150., -190., 85.), target: Vec3::Z * 38., up: Vec3::Z,
            fovy: 75_f32.to_radians(), z_near: 2., z_far: 30000., ..Default::default() };
        clear_background(BLACK);
        set_camera(&view);
        crate::sim_maps::probe::note_projection(view.matrix());
        active.scene.begin_fx_view(&view, frame as f32 * 0.05, false, true);
        let mut skin = crate::character::skinning::overlap::Frame::begin();
        for &x in occurrence_fixture_draws(frame) {
            alice.draw_ghost(vec3(x, 0., 0.), false, false);
        }
        skin.dispatch();
        skin.collect();
        skin.assert_consumed();
        drop(skin);
        active.scene.finish_fx(true);
        set_default_camera();
        crate::sim_maps::probe::record_exact_frame("occurrence-fixture", frame as u64, true)?;
        if frame == 179 { save_capture(std::path::Path::new("final.png"))?; }
        crate::sim_maps::probe::end(frame as u64, "occurrence-fixture", true, &frame);
        crate::phase_census::accept(true, frame >= 60);
        next_frame().await;
    }
    crate::phase_census::host_finish("occurrence-fixture")?;
    println!("PASS publication occurrence fixture: repeated draws of one Alice surface across 180 frames");
    Ok(())
}

/// Native fixed-state integration: the real actor graph, preserved effects, world
/// ordering, both worker counts, frozen simulation and clean map replacement.
#[cfg(feature="vita_renderer")]
pub(crate) async fn check_skin_overlap(assets: &mut Assets, options: &Options) -> Result<()> {
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    if std::env::var_os("LOOKING_GLASS_PUBLICATION_OCCURRENCE_FIXTURE").is_some() {
        return publication_occurrence_fixture(assets, options).await;
    }
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    if std::env::var_os("LOOKING_GLASS_SKIN_EXACT_FIXTURES").is_some() {
        return crate::npc::skin_exact_check::check(assets,options).await;
    }
    let mut comparisons=0;
    for name in ["gvillage","skool1","skool2","hedge1","wchess1","garden1"] {
        for workers in [1,2] {
            let mut reference=Vec::new();
            for overlapped in [false,true] {
                let mut active=enter_level(assets,name,None,None,options,true)?;
                let mut alice=Character::load(assets)?;
                let (eye,yaw)=interaction::spawn_with_static(&active.scene.map,None,Some(active.world.audio_geometry()));
                let player=Player::spawn(&active.world,eye).unwrap_or_else(||Player::new(eye-Vec3::Z*EYE_HEIGHT));
                alice.reset(&player,yaw);
                let direction=vec3(yaw.cos(),yaw.sin(),0.);
                let (camera,target,_)=character::follow_camera(&active.world,player.feet,direction);
                if overlapped { crate::character::skinning::overlap::startup(workers,false); }
                for frame in 0..3 {
                    let time=frame as f32/60.;
                    clear_background(active.scene.atmosphere.background());
                    let view=Camera3D {position:camera,target,up:Vec3::Z,fovy:75_f32.to_radians(),z_near:2.,z_far:30000.,..Default::default()};
                    set_camera(&view);
                    let transforms=active.interactions.transforms();
                    active.scene.prepare_camera_portals(&active.world, &view,time,false,&transforms);
                    active.scene.begin_fx_view(&view,time,false,true);
                    active.art.story_pose(&active.story);
                    let mut skin_frame=if overlapped {
                        let mut f=crate::character::skinning::overlap::Frame::begin();
                        alice.draw_ghost(player.feet,false,frame==2);
                        active.npcs.draw(camera,(target-camera).normalize_or_zero(),&active.scene.atmosphere,false, false);
                        active.art.draw(&active.interactions,&active.hints,&active.scene.atmosphere,camera,false);
                        f.dispatch();Some(f)
                    } else {None};
                    active.scene.draw(camera,time,false,false,&transforms);
                    if let Some(f)=&mut skin_frame {f.collect();}
                    if !crate::character::skinning::overlap::single_frame() {
                    alice.atmosphere(&active.scene.atmosphere,camera);
                    alice.draw_ghost(player.feet,false,frame==2);
                    active.npcs.draw(camera,(target-camera).normalize_or_zero(),&active.scene.atmosphere,false, false);
                    active.art.draw(&active.interactions,&active.hints,&active.scene.atmosphere,camera,false);
                    }
                    if let Some(f)=&skin_frame {f.assert_consumed();}
                    drop(skin_frame);
                    active.scene.draw_with_particles(camera,(target-camera).normalize_or_zero(),time,false,&transforms,&active.steam);
                    active.art.effects(&active.interactions,camera,&active.scene.atmosphere);
                    alice.draw_effects(camera,false,true);
                    active.scene.finish_fx(true);
                    set_default_camera();
                    let image=get_screen_data();
                    let path=format!("private/skin-overlap/{name}-w{workers}-{}-{frame}.png",if overlapped {"overlap"} else {"serial"});
                    save_capture(std::path::Path::new(&path))?;
                    if overlapped {
                        anyhow::ensure!(reference[frame]==image.bytes,"overlap capture differs: {name} workers={workers} frame={frame}");
                        comparisons+=1;
                    } else {reference.push(image.bytes);}
                    next_frame().await;
                }
            }
        }
    }
    // Additional publication consumers and shared instances use the actual assets.
    // Fresh map resources in each arm exercise replacement; captures are outside timing.
    let specs=crate::texture::read_materials(assets)?;
    for workers in [1,2] {
        let mut reference=Vec::new();
        for overlapped in [false,true] {
            let mut active=enter_level(assets,"gvillage",None,None,options,true)?;
            let mut puppet=crate::npc::Puppet::load(assets,"c_torchgnome3",&["idle","death_frozen"],&specs)?;
            let mut alice=Character::load(assets)?;
            let player=Player::new(Vec3::ZERO);alice.reset(&player,0.);
            if overlapped {crate::character::skinning::overlap::startup(workers,false);}
            for case in 0..12 {
                let bright=case==6;let time=case as f32*0.137;
                let offset=if case==9 {Vec3::Y*32.} else {Vec3::ZERO};
                let view=Camera3D {position:vec3(150.,-190.,85.)+offset,target:Vec3::Z*38.+offset,
                    up:Vec3::Z,fovy:75_f32.to_radians(),z_near:2.,z_far:30000.,..Default::default()};
                clear_background(BLACK);set_camera(&view);
                active.scene.begin_fx_view(&view,time,bright,true);
                puppet.surface_visible("material1",case!=4);
                let draw=|puppet:&mut crate::npc::Puppet,alice:&mut Character| {
                    if case==8 {alice.draw_first_person(bright);} else {alice.draw_ghost(Vec3::ZERO,bright,case==7);}
                    for (n,at) in [vec3(-28.,0.,0.),vec3(28.,0.,0.)].into_iter().enumerate() {
                        let electric=if case>=10 {0.4} else {0.};
                        let consumer=puppet.electric_consumer(electric);
                        puppet.mouth((time+n as f32).sin().abs());
                        puppet.draw(if case==3 {"death_frozen"} else {"idle"},time+n as f32*0.19,true,
                            crate::skeletal::Transform {translation:at,rotation:Quat::from_rotation_z(time+n as f32)},1.,bright);
                        drop(consumer);
                        puppet.draw_electric(electric);
                    }
                };
                let mut frame=if overlapped {
                    let f=crate::character::skinning::overlap::Frame::begin();
                    draw(&mut puppet,&mut alice);Some(f)
                } else {None};
                if let Some(f)=&mut frame {f.dispatch();f.collect();}
                if !crate::character::skinning::overlap::single_frame() {draw(&mut puppet,&mut alice);}
                if let Some(f)=&frame {f.assert_consumed();}drop(frame);
                active.scene.finish_fx(true);set_default_camera();
                let image=get_screen_data();
                save_capture(std::path::Path::new(&format!("private/skin-overlap/consumers-w{workers}-{}-{case}.png",if overlapped {"overlap"} else {"serial"})))?;
                if overlapped {anyhow::ensure!(reference[case]==image.bytes,"publication consumer case {case} worker {workers} differs");comparisons+=1;}
                else {reference.push(image.bytes);}
                next_frame().await;
            }
        }
    }
    println!("PASS native early overlap: {comparisons} byte-identical frames across six real maps and consumer witnesses, worker1/2, changing poses, duplicate gnomes, blink/live UV, frozen shells, hidden/reappearing surfaces, attachments, fullbright, first-person/second view, clean resource replacement");
    Ok(())
}
#[cfg(not(feature="vita_renderer"))]
pub(crate) async fn check_skin_overlap(_: &mut Assets, _: &Options) -> Result<()> {
    anyhow::bail!("skin overlap render check requires vita_renderer")
}

/// Read-only saved-pose render benchmark. Frozen gameplay, live puffs settled
/// before timing, no readback in measured windows, no save writes or device IO.
#[cfg(all(feature="vita_renderer", not(target_os="vita")))]
pub(crate) async fn check_renderer_host(assets: &mut Assets, options: &Options) -> Result<()> {
    crate::light_profile::set_mode(crate::frame_profile::Mode::Full);
    let store = Store::new(options.save_dir.clone(), assets.fingerprint()?);
    let loaded = store.read(Slot::Auto)?;
    anyhow::ensure!(!loaded.backup, "reference primary save must be valid");
    let mut state = save::Restored::build(assets, loaded.game)?;
    let level = state.game.level()?.clone();
    let mut art = LevelArt::default();
    art.prepare(assets, &state.scene, &state.interactions, true)?;
    art.story_pose(&state.story);
    let direction = vec3(state.game.view.yaw.cos(), state.game.view.yaw.sin(), 0.);
    let (camera, target, _) = character::follow_camera(&state.world, state.game.player.feet, direction);
    // Rebuild the normal saved emitter population without changing gameplay.
    for _ in 0..600 {
        state.steam.animate(level.environment_clock);
        state.steam.sync(&state.interactions.event_world);
        state.steam.update_clocks(1./60., 1./60., camera, &state.world);
    }
    crate::character::skinning::overlap::startup(2, false);
    let hud = Hud::load(assets)?;
    let catalog = Catalog::load(assets)?;
    let input = crate::input::Input::default();
    let prefs = crate::preferences::Preferences::default();
    let clock_step = std::env::var("LOOKING_GLASS_HOST_CLOCK_STEP").ok()
        .map(|v| v.parse::<f32>()).transpose()?.unwrap_or(0.);
    let mut samples = Vec::with_capacity(360);
    std::fs::create_dir_all("private/renderer-host")?;
    for frame in 0..240 {
        let variant = if frame < 180 { 0 } else { (frame - 180) / 10 + 1 };
        let offset = match variant { 1 => Vec3::X * 32., 2 => Vec3::Y * 64., 3 => Vec3::Z * 16., _ => Vec3::ZERO };
        let view = Camera3D { position: camera + offset, target: target + offset,
            up: Vec3::Z, fovy: 75_f32.to_radians(), z_near: 2., z_far: 30000., ..Default::default() };
        let fullbright = variant == 4;
        let time = level.environment_clock + if variant == 5 { 0.25 } else { 0. }
            + clock_step * frame as f32;
        crate::light_profile::render_host_reset();
        crate::frame_profile::begin_creation_frame();
        clear_background(state.scene.atmosphere.background());
        crate::render::vita_renderer::prepare_3d(state.scene.atmosphere.background());
        crate::render::vita_renderer::set_3d_camera(&view);
        let transforms = state.interactions.transforms();
        let started = std::time::Instant::now();
        state.scene.prepare_camera_portals(&state.world, &view, time, fullbright, &transforms);
        state.scene.begin_fx_view(&view, time, fullbright, true);
        let mut skin = crate::character::skinning::overlap::Frame::begin();
        state.alice.draw_ghost(state.game.player.feet, fullbright, false);
        state.npcs.draw(view.position, (view.target-view.position).normalize_or_zero(), &state.scene.atmosphere, fullbright, false);
        art.draw(&state.interactions, &state.hints, &state.scene.atmosphere, view.position, fullbright);
        skin.dispatch();
        let allocation_scope = crate::kernel_alloc::Scope::begin();
        state.scene.draw(view.position, time, fullbright, false, &transforms);
        let world_allocations = allocation_scope.finish();
        skin.collect();
        {
            let _timer = crate::light_profile::render_timer(crate::light_profile::RenderPhase::Actors);
            if !crate::character::skinning::overlap::single_frame() {
            state.alice.atmosphere(&state.scene.atmosphere, view.position);
            state.alice.draw_ghost(state.game.player.feet, fullbright, false);
            state.npcs.draw(view.position, (view.target-view.position).normalize_or_zero(), &state.scene.atmosphere, fullbright, false);
            art.draw(&state.interactions, &state.hints, &state.scene.atmosphere, view.position, fullbright);
            }
            skin.assert_consumed();
        }
        drop(skin);
        state.scene.draw_with_particles(view.position, (view.target-view.position).normalize_or_zero(), time, fullbright, &transforms, &state.steam);
        art.effects(&state.interactions, view.position, &state.scene.atmosphere);
        state.alice.draw_effects(view.position, fullbright, true);
        state.scene.finish_fx(true);
        crate::render::vita_renderer::present_3d();
        let mut cpu_ns = started.elapsed().as_nanos() as u64;
        if matches!(frame, 179 | 189 | 199 | 209 | 219 | 229 | 239) {
            set_default_camera();
            save_capture(std::path::Path::new(&format!("private/renderer-host/pose-{variant}.png")))?;
        }
        set_default_camera();
        let hud_start = std::time::Instant::now();
        {
            let _timer = crate::light_profile::render_timer(crate::light_profile::RenderPhase::Hud);
            hud.draw(&state.game.stats, &catalog, false, &HudView::capture(&input), &prefs);
        }
        cpu_ns += hud_start.elapsed().as_nanos() as u64;
        if matches!(frame, 179 | 189 | 199 | 209 | 219 | 229 | 239) {
            save_capture(std::path::Path::new(&format!("private/renderer-host/hud-{variant}.png")))?;
        }
        next_frame().await;
        if frame >= 60 {
            samples.push(serde_json::json!({"frame":frame,"variant":variant,"cpu_ns":cpu_ns + crate::light_profile::render_host_deferred_ns(),"world_allocations":world_allocations,
                "phases":crate::light_profile::render_host_report(),
                "counts":crate::render_fx::retained::host_report()}));
        }
    }
    // Draw one live, authored steam puff against an empty depth target as a
    // visibility witness. World occlusion/joint alpha remain covered above and
    // by the existing native alpha suite. Neither witness is a timing sample.
    let (puff, focus) = state.steam.host_steam_puff().context("reference needs a live steam puff")?;
    let steam_camera = focus + vec3(96.,32.,24.);
    let steam_direction = (focus - steam_camera).normalize();
    let steam_view = Camera3D { position:steam_camera, target:focus, up:Vec3::Z,
        fovy:75_f32.to_radians(), z_near:2., z_far:30000., ..Default::default() };
    let mut steam_pixels = Vec::new();
    for visible in [true,false] {
        clear_background(BLACK);
        crate::render::vita_renderer::prepare_3d(BLACK);
        crate::render::vita_renderer::set_3d_camera(&steam_view);
        state.scene.begin_fx_view(&steam_view, level.environment_clock, false, true);
        if visible {
            state.steam.prepare_draw(steam_camera, &state.scene.atmosphere);
            state.steam.prepare_quads(steam_camera, Some(steam_direction));
            state.steam.draw_one(puff, steam_camera, steam_direction);
        }
        state.scene.finish_fx(true);
        crate::render::vita_renderer::present_3d(); set_default_camera();
        let pixels = get_screen_data().bytes;
        if visible {
            steam_pixels = pixels;
            save_capture(std::path::Path::new("private/renderer-host/steam.png"))?;
        } else { anyhow::ensure!(steam_pixels != pixels, "steam witness has no visible pixels"); }
        next_frame().await;
    }
    std::fs::write("private/renderer-host/samples.json", serde_json::to_vec_pretty(&serde_json::json!({
        "reference_feet":state.game.player.feet.to_array(),"camera":camera.to_array(),
        "target":target.to_array(),"environment_clock":level.environment_clock,
        "samples":samples,"console_access":false,"simulation_frozen":true,"clock_step":clock_step,
        "particle_settle_frames":600,"warmup_frames":60,"steam_visible_pixels_proven":true}))?)?;
    println!("PASS renderer host reference: 120 frozen timing frames, 60 invalidation frames, fifteen captures including HUD and visible steam");
    Ok(())
}
// The chapter chooser owns Down while open; the inventory-opening edge cannot cycle.
fn inventory_toggle(pressed: bool, menu: bool, inventory: &mut bool) -> bool {
    if pressed && !menu { *inventory = !*inventory; return *inventory; }
    false
}
fn inventory_cycle(opened: bool, previous: bool, next: bool) -> (bool, bool) {
    (!opened && previous, !opened && next)
}
#[cfg(test)]
mod reference_input_tests {
    use super::*;
    #[test]
    fn inventory_down_opens_without_cycling_then_navigates() {
        let mut inventory = false;
        let opened = inventory_toggle(true, false, &mut inventory);
        assert!(inventory && opened);
        assert_eq!(inventory_cycle(opened, false, true), (false, false));
        assert_eq!(inventory_cycle(false, false, true), (false, true));
        assert!(!inventory_toggle(true, false, &mut inventory));
        assert!(!inventory);
    }
    #[test]
    fn chapter_down_does_not_toggle_inventory() {
        let mut inventory = false;
        assert!(!inventory_toggle(true, true, &mut inventory));
        assert!(!inventory);
        let mut chapters = crate::chapters::Chapters::default();
        chapters.open(0, 36);
        chapters.select(chapters.selected + 1, 36);
        assert_eq!(chapters.selected, 1);
    }
}

#[cfg(test)]
mod snapshot_dispatch_tests {
    use super::*;
    #[test]
    fn dispatch_visit_overflow_and_registry_shape_reject_before_publication() {
        let art = LevelArt::default();
        let mut input = LevelDispatch { visit: art.snapshot_visit.id(), selected: [false; 10],
            registered: vec![], school_staging: false, cheshire: None, overflow: false };
        assert!(input.valid(&art));
        input.overflow = true; assert!(!input.valid(&art)); input.overflow = false;
        input.registered.push(false); assert!(!input.valid(&art)); input.registered.clear();
        let restored = LevelArt::default(); assert!(!input.valid(&restored));
        input.visit = 0; assert!(!input.valid(&art));
    }
}
