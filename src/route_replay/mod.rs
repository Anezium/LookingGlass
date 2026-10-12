//! Input-only route diagnostics. State restoration and writes use the normal LG store.
use crate::{
    movement::{Controls, Player, FIXED_DT},
    save::{Game, Slot, Store},
    Options,
};
use anyhow::{ensure, Context, Result};
use macroquad::prelude::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Read, path::PathBuf, time::Instant};
#[cfg(not(target_os = "vita"))]
mod record;
const VERSION: u32 = 1;
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub tick: u64,
    pub time_s: f64,
    pub stick: [f32; 2],
    pub look: [f32; 2],
    pub jump: bool,
    pub run: bool,
    pub segment: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gate {
    pub tick: u64,
    pub scene_done: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tape {
    pub version: u32,
    pub start: String,
    pub min_moving_seconds: f64,
    pub samples: Vec<Sample>,
    pub gates: Vec<Gate>,
}
impl Tape {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.version == VERSION && self.start == "new_game",
            "unsupported route version/start"
        );
        ensure!(
            self.min_moving_seconds.is_finite() && (60. ..=600.).contains(&self.min_moving_seconds),
            "invalid minimum input time"
        );
        ensure!(
            !self.samples.is_empty() && self.samples.len() <= 100_000,
            "invalid sample count"
        );
        let mut previous = None;
        for s in &self.samples {
            ensure!(
                previous.is_none_or(|t| s.tick > t) && s.tick < 120 * 3600,
                "unsorted/oversized route time"
            );
            ensure!(
                s.time_s.is_finite() && (s.time_s - s.tick as f64 / 120.).abs() < 0.0001,
                "tick/time mismatch"
            );
            ensure!(
                s.stick
                    .iter()
                    .chain(&s.look)
                    .all(|n| n.is_finite() && n.abs() <= 1.0001),
                "invalid stick value"
            );
            ensure!(s.segment.len() <= 48, "invalid segment name");
            previous = Some(s.tick);
        }
        ensure!(
            self.samples[0].tick == 0 && self.gates.len() <= 8,
            "invalid route origin/gates"
        );
        for g in &self.gates {
            ensure!(
                g.tick <= self.samples.last().unwrap().tick
                    && matches!(g.scene_done.as_str(), "entry" | "knife_cat"),
                "invalid scene gate"
            );
        }
        Ok(())
    }
}
#[derive(PartialEq, Eq)]
enum Mode {
    Record,
    Replay,
    Continue,
}
#[derive(Default, Clone, Serialize)]
pub struct Status {
    pub map: String,
    pub entry_done: bool,
    pub knife_done: bool,
    pub blade_copies: u32,
    pub alive: bool,
    pub flying: bool,
}
#[derive(Serialize, Deserialize)]
struct End {
    map: String,
    feet: [f32; 3],
    yaw: f32,
    pitch: f32,
    blade_copies: u32,
    knife_done: bool,
}
pub struct Session {
    mode: Mode,
    root: PathBuf,
    tape: Tape,
    tick: u64,
    cursor: usize,
    moving_ticks: u64,
    post_moving_ticks: u64,
    status: Status,
    segment: String,
    frame_start: Option<Instant>,
    wall: Vec<(String, f64)>,
    start: Instant,
    done: bool,
    heap_start: Option<u64>,
    windows: u64,
    total_frames: u64,
    shader_after_warmup: u64,
    pipeline_after_warmup: u64,
    aggregates: BTreeMap<String, (u64, f64, Vec<u64>)>,
    #[cfg(not(target_os = "vita"))]
    driver: record::Driver,
}
impl Session {
    pub fn open(options: &mut Options) -> Result<Option<Self>> {
        #[cfg(target_os = "vita")]
        let root = PathBuf::from("ux0:data/alice/private");
        #[cfg(not(target_os = "vita"))]
        let root = std::env::var_os("LOOKING_GLASS_ROUTE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("private"));
        #[cfg(target_os = "vita")]
        let mode = std::fs::read_to_string(root.join("profile_mode")).unwrap_or_default();
        #[cfg(not(target_os = "vita"))]
        let mode = std::env::var("LOOKING_GLASS_ROUTE").unwrap_or_default();
        let mode = match mode.trim() {
            "route" => Mode::Replay,
            "route-continue" => Mode::Continue,
            "record" => Mode::Record,
            _ => return Ok(None),
        };
        #[cfg(target_os = "vita")]
        ensure!(mode != Mode::Record, "route recording is host-only");
        let tape = if mode == Mode::Record {
            Tape {
                version: VERSION,
                start: "new_game".into(),
                min_moving_seconds: 60.,
                samples: Vec::new(),
                gates: vec![Gate {
                    tick: 0,
                    scene_done: "entry".into(),
                }],
            }
        } else {
            let mut bytes = Vec::new();
            std::fs::File::open(root.join("route-inputs.json"))
                .context("route-inputs.json")?
                .take(16 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() <= 16 * 1024 * 1024, "route file exceeds bound");
            let t: Tape = serde_json::from_slice(&bytes)?;
            t.validate()?;
            t
        };
        options.save_dir = root.join("route-saves");
        options.map = "gvillage".into();
        options.entry = None;
        options.new_game = mode != Mode::Continue;
        options.direct_map = true;
        options.load = None;
        options.difficulty = crate::powerups::Difficulty::Easy;
        // The diagnostic begins at village arrival; the presentation film is outside the route.
        options.frames = Some(200_000);
        println!(
            "[lg/route] start mode={} save_dir={}",
            if mode == Mode::Continue {
                "continue"
            } else if mode == Mode::Record {
                "record"
            } else {
                "replay"
            },
            options.save_dir.display()
        );
        Ok(Some(Self {
            mode,
            root,
            tape,
            tick: 0,
            cursor: 0,
            moving_ticks: 0,
            post_moving_ticks: 0,
            status: Status::default(),
            segment: "arrival".into(),
            frame_start: None,
            wall: Vec::new(),
            start: Instant::now(),
            done: false,
            heap_start: None,
            windows: 0,
            total_frames: 0,
            shader_after_warmup: 0,
            pipeline_after_warmup: 0,
            aggregates: BTreeMap::new(),
            #[cfg(not(target_os = "vita"))]
            driver: record::Driver::default(),
        }))
    }
    pub fn resume(&self, store: &Store) -> Result<Option<crate::save::Loaded>> {
        if self.mode != Mode::Continue {
            return Ok(None);
        }
        Ok(Some(
            store.latest().context("route Continue has no save")?.1,
        ))
    }
    pub fn continuing(&self) -> bool {
        self.mode == Mode::Continue
    }
    pub fn begin_frame(&mut self, status: Status) -> Result<()> {
        let now = Instant::now();
        if let Some(start) = self.frame_start.take() {
            let ms = now.duration_since(start).as_secs_f64() * 1000.;
            self.wall.push((self.segment.clone(), ms));
            self.total_frames += 1;
            for label in [self.segment.as_str(), "whole-passage"] {
                let a = self
                    .aggregates
                    .entry(label.into())
                    .or_insert_with(|| (0, 0., vec![0; 4001]));
                a.0 += 1;
                a.1 += ms;
                a.2[(ms.round() as usize).min(4000)] += 1;
            }
            if self.total_frames > 60 {
                let c = crate::frame_profile::current_creations();
                self.shader_after_warmup += c[0];
                self.pipeline_after_warmup += c[1];
            }
            self.frame_start = Some(now);
            if self.wall.len() >= 300 {
                self.export_window()?;
            }
        } else {
            self.start = Instant::now();
            self.heap_start = crate::frame_profile::heap_used_bytes();
            println!(
                "[lg/route] ready map={} heap_start_bytes={:?}",
                status.map, self.heap_start
            );
        }
        self.frame_start = Some(now);
        self.status = status;
        ensure!(
            self.status.map == "gvillage" && self.status.alive && !self.status.flying,
            "route left playable village state"
        );
        ensure!(self.start.elapsed().as_secs() < 3600, "route wall timeout");
        if !self.status.entry_done {
            self.segment = "arrival".into();
        } else if self.status.blade_copies > 0 && !self.status.knife_done {
            self.segment = "knife_cat".into();
            if self.mode == Mode::Record
                && !self.tape.gates.iter().any(|g| g.scene_done == "knife_cat")
            {
                self.tape.gates.push(Gate {
                    tick: self.tick,
                    scene_done: "knife_cat".into(),
                });
            }
        }
        Ok(())
    }
    pub fn control(
        &mut self,
        world: &crate::collision::World,
        player: &Player,
        yaw: &mut f32,
        pitch: &mut f32,
    ) -> Controls {
        if self.done {
            return Controls::default();
        }
        if self.tape.gates.iter().any(|g| {
            g.tick <= self.tick
                && match g.scene_done.as_str() {
                    "entry" => !self.status.entry_done,
                    _ => !self.status.knife_done,
                }
        }) {
            return Controls::default();
        }
        #[cfg(not(target_os = "vita"))]
        if self.mode == Mode::Record {
            let c = self
                .driver
                .control(world, player, &self.status, self.moving_ticks);
            if self.driver.error.is_some() {
                self.done = true;
                return Controls::default();
            }
            if self.status.blade_copies > 0 && !self.status.knife_done {
                if !self.tape.gates.iter().any(|g| g.scene_done == "knife_cat") {
                    self.tape.gates.push(Gate {
                        tick: self.tick,
                        scene_done: "knife_cat".into(),
                    });
                }
                return Controls::default();
            }
            if self.driver.done
                && self.status.knife_done
                && self.post_moving_ticks as f64 / 120. >= self.tape.min_moving_seconds
            {
                self.done = true;
                return Controls::default();
            }
            let flat = vec2(yaw.cos(), yaw.sin());
            let right = vec2(yaw.sin(), -yaw.cos());
            self.segment = self.driver.segment();
            self.tape.samples.push(Sample {
                tick: self.tick,
                time_s: self.tick as f64 / 120.,
                stick: [c.wish.dot(right), c.wish.dot(flat)],
                look: [0.; 2],
                jump: c.jump,
                run: c.run,
                segment: self.segment.clone(),
            });
            self.tick += 1;
            if c.wish.length_squared() > 0.01 {
                self.moving_ticks += 1;
                if self.status.knife_done {
                    self.post_moving_ticks += 1;
                }
            }
            return c;
        }
        let _ = (world, player);
        while self.cursor + 1 < self.tape.samples.len()
            && self.tape.samples[self.cursor + 1].tick <= self.tick
        {
            self.cursor += 1;
        }
        let s = &self.tape.samples[self.cursor];
        if self.segment != s.segment {
            self.segment.clone_from(&s.segment);
        }
        *yaw -= s.look[0] * FIXED_DT * 2.5;
        *pitch = (*pitch + s.look[1] * FIXED_DT * 2.).clamp(-1.5, 1.5);
        let wish =
            vec2(yaw.cos(), yaw.sin()) * s.stick[1] + vec2(yaw.sin(), -yaw.cos()) * s.stick[0];
        self.tick += 1;
        if wish.length_squared() > 0.01 {
            self.moving_ticks += 1;
            if self.status.knife_done {
                self.post_moving_ticks += 1;
            }
        }
        if self.tick > self.tape.samples.last().unwrap().tick {
            self.done = true;
        }
        Controls {
            wish: wish.clamp_length_max(1.),
            jump: s.jump,
            run: s.run,
            ..Default::default()
        }
    }
    fn export_window(&mut self) -> Result<()> {
        if self.wall.is_empty() {
            return Ok(());
        }
        let dir = self.root.join("route-frame-windows");
        std::fs::create_dir_all(&dir)?;
        self.windows += 1;
        std::fs::write(
            dir.join(format!("window-{:05}.json", self.windows)),
            serde_json::to_vec(&self.wall)?,
        )?;
        self.wall.clear();
        Ok(())
    }
    pub fn should_finish(&self) -> bool {
        self.done || self.mode == Mode::Continue
    }
    pub fn finish(&mut self, store: &Store, game: &Game, yaw: f32, pitch: f32) -> Result<()> {
        #[cfg(not(target_os = "vita"))]
        ensure!(
            self.driver.error.is_none(),
            "{}",
            self.driver.error.as_deref().unwrap_or("")
        );
        ensure!(
            self.status.knife_done && self.status.blade_copies == 1,
            "route did not finish authored Blade/knife_cat progression"
        );
        let end = End {
            map: self.status.map.clone(),
            feet: game.player.feet.to_array(),
            yaw,
            pitch,
            blade_copies: self.status.blade_copies,
            knife_done: self.status.knife_done,
        };
        std::fs::create_dir_all(&self.root)?;
        if self.mode == Mode::Continue {
            let expected: End =
                serde_json::from_slice(&std::fs::read(self.root.join("route-end.json"))?)?;
            ensure!(
                end.map == expected.map
                    && Vec3::from_array(end.feet).distance(Vec3::from_array(expected.feet)) < 0.01
                    && (yaw - expected.yaw).abs() < 0.0001
                    && (pitch - expected.pitch).abs() < 0.0001,
                "Continue pose mismatch"
            );
            println!(
                "[lg/route] PASS Continue map={} feet={:?}",
                end.map, end.feet
            );
        } else {
            ensure!(
                self.post_moving_ticks as f64 / 120. >= self.tape.min_moving_seconds,
                "route has fewer than 60 seconds of moving inputs after knife_cat"
            );
            store.write(Slot::Auto, game)?;
            if self.mode == Mode::Record {
                self.tape.validate()?;
                std::fs::write(
                    self.root.join("route-inputs.json"),
                    serde_json::to_vec_pretty(&self.tape)?,
                )?;
            }
            std::fs::write(
                self.root.join("route-end.json"),
                serde_json::to_vec_pretty(&end)?,
            )?;
            println!(
                "[lg/route] PASS saved map={} feet={:?} moving_s={:.3} post_knife_cat_s={:.3}",
                end.map,
                end.feet,
                self.moving_ticks as f64 / 120.,
                self.post_moving_ticks as f64 / 120.
            );
        }
        let save_ms = self
            .frame_start
            .take()
            .map(|s| s.elapsed().as_secs_f64() * 1000.);
        if let Some(ms) = save_ms {
            self.wall.push(("save-or-continue".into(), ms));
            self.total_frames += 1;
            for label in ["save-or-continue", "whole-passage"] {
                let a = self
                    .aggregates
                    .entry(label.into())
                    .or_insert_with(|| (0, 0., vec![0; 4001]));
                a.0 += 1;
                a.1 += ms;
                a.2[(ms.round() as usize).min(4000)] += 1;
            }
        }
        self.export_window()?;
        let stats=self.aggregates.iter().map(|(label,(n,sum,hist))|{
            let percentile=|rank:u64|{let mut count=0;hist.iter().enumerate().find_map(|(i,&v)|{count+=v;(count>rank).then_some(i)})};
            (label,serde_json::json!({"samples":n,"mean_ms":if *n>0{Some(sum/(*n as f64))}else{None},"median_ms_rounded":percentile(n/2),"p95_ms_rounded":percentile(n*95/100)}))
        }).collect::<BTreeMap<_,_>>();
        let heap_end = crate::frame_profile::heap_used_bytes();
        println!(
            "[lg/route] heap end start={:?} end={heap_end:?}",
            self.heap_start
        );
        let report = serde_json::json!({"version":VERSION,"continue":self.continuing(),"moving_input_seconds":self.moving_ticks as f64/120.,"post_knife_cat_moving_input_seconds":self.post_moving_ticks as f64/120.,"end":end,"frame_wall_ms":stats.get(&"whole-passage".to_string()),"segments":stats,"raw_windows":self.windows,"raw_path":"route-frame-windows/window-*.json","window_capacity_frames":300,"quantiles":"1ms histogram, >=4000ms in overflow bin; raw windows permit exact quantiles","save_cpu_ms":save_ms,"wall_seconds":self.start.elapsed().as_secs_f64(),"shader_creations_after_60_frames":self.shader_after_warmup,"pipeline_creations_after_60_frames":self.pipeline_after_warmup,"heap":{"start_bytes":self.heap_start,"end_bytes":heap_end,"delta_bytes":self.heap_start.zip(heap_end).map(|(a,b)|b as i64-a as i64)},"configuration":{"vita_renderer":crate::render::vita_renderer::config(),"vita_sim":crate::vita_sim::effective()},"cumulative_frame_profiler":"disabled in native route modes"});
        std::fs::write(
            self.root.join(if self.continuing() {
                "route-continue-report.json"
            } else {
                "route-report.json"
            }),
            serde_json::to_vec_pretty(&report)?,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tape() -> Tape {
        Tape {
            version: 1,
            start: "new_game".into(),
            min_moving_seconds: 60.,
            samples: vec![
                Sample::default(),
                Sample {
                    tick: 1,
                    time_s: 1. / 120.,
                    ..Default::default()
                },
            ],
            gates: vec![Gate {
                tick: 0,
                scene_done: "entry".into(),
            }],
        }
    }
    #[test]
    fn rejects_inputs_that_cannot_be_replayed_safely() {
        assert!(tape().validate().is_ok());
        let mut t = tape();
        t.samples[1].tick = 0;
        assert!(t.validate().is_err());
        let mut t = tape();
        t.samples[0].stick[0] = f32::NAN;
        assert!(t.validate().is_err());
        let mut t = tape();
        t.samples[1].time_s = 4.;
        assert!(t.validate().is_err());
        let mut t = tape();
        t.min_moving_seconds = 59.;
        assert!(t.validate().is_err());
        let mut t = tape();
        t.gates[0].scene_done = "debug_complete".into();
        assert!(t.validate().is_err());
    }
}
