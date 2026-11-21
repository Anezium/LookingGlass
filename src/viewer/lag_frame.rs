//! Owner-only presentation retained across the next tape pulse.
use super::{
    free_render::{FreeRender, RenderScopes},
    LevelArt, LevelDispatch,
};
use crate::{
    character::AliceArt,
    hud::{Hud, HudOp},
    hud_view::HudView,
    lighting::{Selection, ShadowInput},
    npc::NpcArt,
    particles::SteamArt,
    powerups::{self, PickupInput},
    preferences::Preferences,
    render::Scene,
    render_snapshot::{DynamicInput, HandoffReturn, HudInput, WaveInput},
    replay::input::ClockView,
    sim_pipeline::Stamp,
    targeting,
};
use macroquad::prelude::*;
use std::sync::Arc;

pub(super) struct Frame {
    pub stamp: Stamp,
    pub clock: ClockView,
    pub camera: Camera3D,
    pub dynamic: DynamicInput,
    pub level: LevelDispatch,
    pub aim: targeting::Input,
    pub shadow: Option<ShadowInput>,
    pub lights: Selection,
    pub first_person: bool,
    pub empty_level_effects: bool,
    pub fullbright: bool,
    pub draw_world: bool,
    pub world_only: bool,
    pub dt: f32,
    pub handoff_minimum_frame: u64,
    pub snapshot_log: bool,
    pub hud_input: HudInput,
    pub hud_view: HudView,
    pub hud_ops: Vec<HudOp>,
    pub preferences: Arc<Preferences>,
    pub notice: String,
    pub preview_capture: bool,
    pub screenshot: bool,
    pub input_started: Option<super::sim_timing::Mark>,
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    pub probe_state: Option<serde_json::Value>,
}

pub(super) struct Art<'a> {
    pub scene: &'a mut Scene,
    pub level: &'a mut LevelArt,
    pub alice: &'a mut AliceArt,
    pub npcs: &'a mut NpcArt,
    pub steam: &'a SteamArt,
    pub power: &'a mut powerups::Art,
    pub aim: &'a mut targeting::Pointer,
    pub handoff: &'a mut HandoffReturn,
    pub overlap_probe: &'a mut crate::overlap_probe::Probe,
    pub hud: &'a mut Hud,
    pub pickups: &'a PickupInput,
    pub waves: &'a [WaveInput],
}

impl Frame {
    pub(super) async fn present(
        self,
        mut art: Art<'_>,
        ui: &crate::ui::Ui,
        catalog: &crate::inventory::Catalog,
        save_frame: &mut crate::save_preview::Frame,
        trace: &mut super::sim_timing::Trace,
        map: &str,
        peer: Option<super::lag_control::Peer>,
        control: super::lag_control::Controls,
        audit: &super::sim_audit::Audit,
    ) -> anyhow::Result<bool> {
        use super::sim_timing::Scope;
        let stamp = self.stamp;
        let clock_guard = self.clock.enter();
        let started = trace.mark(stamp.frame.frame);
        if let Some(peer) = peer {
            peer.enter();
            audit.event("owner_inner", stamp);
        }
        let (mut tail, scopes) = self.draw(&mut art);
        anyhow::ensure!(
            control.draw_error != Some(stamp.frame.frame),
            "injected lag draw error"
        );
        trace.owner(stamp, Scope::Render, None, started);
        #[cfg(feature = "vita_renderer")]
        crate::render::vita_renderer::present_3d();
        if tail.preview_capture {
            save_frame.update();
        }
        crate::spike_log::hud_begin();
        art.overlap_probe.finish();
        tail.draw_hud(art.hud, ui, catalog);
        drop(scopes);
        crate::light_profile::cpu_end();
        crate::frame_phases::mark(2);
        crate::spike_log::cpu_end();
        crate::character::skinning_bench_end();
        #[cfg(target_os = "vita")]
        let report_due = {
            crate::frame_phases::mark(3);
            crate::frame_profile::end_frame(map, true)
        };
        #[cfg(not(target_os = "vita"))]
        let report_due = {
            crate::frame_profile::end_creation_frame();
            false
        };
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        if let Some(state) = tail.probe_state {
            #[cfg(feature = "vita_renderer")]
            {
                let associated_frame = stamp.frame.frame
                    + u64::from(control.wrong_association == Some(stamp.frame.frame));
                crate::sim_maps::probe::record_exact_frame(
                    map,
                    associated_frame,
                    matches!(stamp.frame.frame, 0 | 59 | 60 | 119 | 120 | 179),
                )?;
            }
            crate::sim_maps::probe::shot(stamp.frame.frame)?;
            crate::sim_maps::probe::end(stamp.frame.frame, map, true, &state);
        } else if crate::replay::input::cross() {
            crate::sim_maps::probe::shot(stamp.frame.frame)?;
        }
        if tail.screenshot {
            super::save_capture(&std::path::PathBuf::from(format!(
                "private/screenshots/{map}-{}.png",
                stamp.frame.frame + 1
            )))?;
        }
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        crate::phase_census::accept(true, stamp.frame.frame >= 60);
        #[cfg(feature = "vita_renderer")]
        if crate::replay_capture::wanted(stamp.frame.tape_step) {
            crate::replay_capture::save(stamp.frame.tape_step)?;
        }
        drop(clock_guard);
        crate::frame_phases::mark(9);
        let started = trace.mark(stamp.frame.frame);
        next_frame().await;
        trace.owner(stamp, Scope::Present, None, started);
        trace.owner(stamp, Scope::InputToPresent, None, tail.input_started);
        Ok(report_due)
    }

    pub(super) fn draw(self, art: &mut Art<'_>) -> (FrameTail, RenderScopes) {
        let index = self.stamp.frame.frame;
        let scopes = FreeRender {
            scene: art.scene,
            level_art: art.level,
            alice: art.alice,
            npcs: art.npcs,
            steam: art.steam,
            power_art: art.power,
            aim_pointer: art.aim,
            hud: art.hud,
            handoff_return: art.handoff,
            overlap_probe: art.overlap_probe,
            world_camera: &self.camera,
            dynamic: &self.dynamic,
            level: &self.level,
            pickups: art.pickups,
            waves: art.waves,
            aim: &self.aim,
            shadow: self.shadow,
            lights: self.lights,
            first_person: self.first_person,
            empty_level_effects: self.empty_level_effects,
            fullbright: self.fullbright,
            draw_world: self.draw_world,
            world_only: self.world_only,
            aim_dt: self.dt,
            frame: index,
            handoff_minimum_frame: self.handoff_minimum_frame,
            snapshot_log: self.snapshot_log,
        }
        .draw(
            crate::phase_census::span(crate::phase_census::Region::RenderSetup),
            #[cfg(target_os = "vita")]
            crate::frame_profile::span("frame_world"),
            #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
            crate::frame_profile::span("frame_world"),
        );
        (
            FrameTail {
                hud_input: self.hud_input,
                hud_view: self.hud_view,
                hud_ops: self.hud_ops,
                preferences: self.preferences,
                notice: self.notice,
                world_only: self.world_only,
                preview_capture: self.preview_capture,
                screenshot: self.screenshot,
                input_started: self.input_started,
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                probe_state: self.probe_state,
            },
            scopes,
        )
    }
}

pub(super) struct FrameTail {
    pub hud_input: HudInput,
    pub hud_view: HudView,
    pub hud_ops: Vec<HudOp>,
    pub preferences: Arc<Preferences>,
    pub notice: String,
    pub world_only: bool,
    pub preview_capture: bool,
    pub screenshot: bool,
    pub input_started: Option<super::sim_timing::Mark>,
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    pub probe_state: Option<serde_json::Value>,
}

impl FrameTail {
    pub(super) fn draw_hud(
        &mut self,
        hud: &mut Hud,
        ui: &crate::ui::Ui,
        catalog: &crate::inventory::Catalog,
    ) {
        hud.apply(&mut self.hud_ops);
        set_default_camera();
        if self.world_only {
            return;
        }
        let _timer = crate::light_profile::render_timer(crate::light_profile::RenderPhase::Hud);
        let input = &self.hud_input;
        let view = &self.hud_view;
        let preferences = &self.preferences;
        assert!(
            !input.help && !input.skip,
            "lag HUD requires the gameplay view"
        );
        if let Some(prompt) = &input.prompt {
            let prompt = crate::prompts::world_prompt(prompt, view, preferences);
            ui.toast(&prompt, screen_height() - 112.);
        }
        if input.immersion == 3 && input.stats.alive() {
            crate::hud::air(&input.breath, ui);
        }
        if !input.scripted {
            hud.draw(&input.stats, catalog, false, view, preferences);
            if input.stats.alive() {
                hud.draw_notice(view, preferences);
            }
        }
        if input.stats.alive() {
            for &color in &input.fades {
                draw_rectangle(0., 0., screen_width(), screen_height(), color);
            }
            if preferences.subtitles {
                if let Some(subtitle) = &input.subtitle {
                    subtitle.draw(&view.label(preferences, "E"), ui);
                }
            }
            for command in &input.level {
                command.draw();
            }
        }
        if !self.notice.is_empty() {
            ui.toast(&self.notice, 74.);
        }
        show_mouse(false);
    }
}
