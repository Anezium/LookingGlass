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
fn heading_step(yaw:f32,target:f32)->Option<f32> {
    let error=(target-yaw+std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)-std::f32::consts::PI;
    (error.abs()>0.0001).then(||yaw+error.clamp(-2.5*FIXED_DT,2.5*FIXED_DT))
}
fn replay_wish(sample: &Sample, yaw: f32, world_wish: bool) -> Vec2 {
    let heading = if world_wish { sample.reference_yaw.unwrap_or(yaw) } else { yaw };
    vec2(heading.cos(), heading.sin()) * sample.stick[1]
        + vec2(heading.sin(), -heading.cos()) * sample.stick[0]
}
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_feet: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_yaw: Option<f32>,
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
            self.version == VERSION && matches!(self.start.as_str(), "new_game" | "save:slot1"),
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
        let segments = self
            .samples
            .iter()
            .map(|s| s.segment.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        ensure!(segments.len() <= 64, "too many route segments");
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
            ensure!(s.reference_yaw.is_none_or(f32::is_finite),"nonfinite route heading");
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
    pub feet: [f32; 3],
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
    run_id: String,
    tape: Tape,
    tick: u64,
    cursor: usize,
    moving_ticks: u64,
    post_moving_ticks: u64,
    status: Status,
    entry_saved: bool,
    drift_reported: bool,
    heading_ticks:u32,
    heading_total:u64,
    route_error:Option<String>,
    opening_played:bool,
    segment: String,
    frame_start: Option<Instant>,
    wall: Vec<(String, f64)>,
    counters: Vec<[u32;8]>,
    counter_samples: u64,
    counter_sums: [u64;8],
    counter_peaks: [u32;8],
    start: Instant,
    startup_seconds:f64,
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
        let startup=Instant::now();
        eprintln!("[lg/startup] route_mode read begin");
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
        eprintln!("[lg/startup] route_mode ready elapsed_ms={:.3}",startup.elapsed().as_secs_f64()*1000.);
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
            eprintln!("[lg/startup] route_tape read begin path={}",root.join("route-inputs.json").display());
            let file=std::fs::File::open(root.join("route-inputs.json")).context("route-inputs.json")?;
            let size=file.metadata()?.len();
            ensure!(size<=16*1024*1024,"route file exceeds bound");
            let mut bytes=Vec::with_capacity(size as usize);
            std::io::BufReader::with_capacity(64*1024,file).take(16*1024*1024+1).read_to_end(&mut bytes)?;
            ensure!(bytes.len() <= 16 * 1024 * 1024, "route file exceeds bound");
            eprintln!("[lg/startup] route_tape read done bytes={} elapsed_ms={:.3}; parse begin",bytes.len(),startup.elapsed().as_secs_f64()*1000.);
            let t: Tape = serde_json::from_slice(&bytes)?;
            eprintln!("[lg/startup] route_tape parse done samples={} elapsed_ms={:.3}; validate begin",t.samples.len(),startup.elapsed().as_secs_f64()*1000.);
            t.validate()?;
            eprintln!("[lg/startup] route_tape validate done elapsed_ms={:.3}",startup.elapsed().as_secs_f64()*1000.);
            t
        };
        options.save_dir = root.join("route-saves");
        options.map = "gvillage".into();
        options.entry = None;
        options.new_game = mode != Mode::Continue;
        options.direct_map = true;
        options.load = None;
        options.difficulty = crate::powerups::Difficulty::Easy;
        // New Game uses the ordinary opening-film path; named-save/record modes
        // retain the finite diagnostic frame guard. Session also has a wall bound.
        options.frames = if mode==Mode::Replay && tape.start=="new_game" {None}else{Some(200_000)};
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
        let run_id = format!(
            "{}-{}",
            match mode {
                Mode::Record => "record",
                Mode::Replay => "replay",
                Mode::Continue => "continue",
            },
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        );
        Ok(Some(Self {
            mode,
            run_id,
            root,
            tape,
            tick: 0,
            cursor: 0,
            moving_ticks: 0,
            post_moving_ticks: 0,
            status: Status::default(),
            entry_saved: false,
            drift_reported: false,
            heading_ticks:0,heading_total:0,route_error:None,opening_played:false,
            segment: "arrival".into(),
            frame_start: None,
            wall: Vec::new(),
            counters: Vec::new(),
            counter_samples: 0,
            counter_sums: [0;8],
            counter_peaks: [0;8],
            start: Instant::now(),startup_seconds:0.,
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
            return if self.mode == Mode::Replay && self.tape.start == "save:slot1" {
                Ok(Some(store.read(Slot::parse("slot1")?)?))
            } else {
                Ok(None)
            };
        }
        Ok(Some(
            store.latest().context("route Continue has no save")?.1,
        ))
    }
    pub fn entry_checkpoint_due(&mut self) -> bool {
        if self.mode == Mode::Record && self.status.entry_done && !self.entry_saved {
            self.entry_saved = true;
            true
        } else {
            false
        }
    }
    #[cfg(not(target_os = "vita"))]
    pub fn recording_pause_seconds(&mut self) -> f32 {
        if self.mode == Mode::Record {
            std::mem::take(&mut self.driver.planning_seconds)
        } else {
            0.
        }
    }
    pub fn continuing(&self) -> bool {
        self.mode == Mode::Continue
    }
    pub fn needs_opening(&self)->bool {self.mode==Mode::Replay && self.tape.start=="new_game"}
    pub fn opening_complete(&mut self) {self.opening_played=true;println!("[lg/route] opening film complete");}
    pub fn begin_frame(&mut self, status: Status) -> Result<()> {
        let now = Instant::now();
        if let Some(start) = self.frame_start.take() {
            let ms = now.duration_since(start).as_secs_f64() * 1000.;
            self.wall.push((self.segment.clone(), ms));
            let counters=crate::frame_profile::route_counters();
            self.counters.push(counters);
            self.counter_samples+=1;
            for i in 0..8 {
                self.counter_sums[i]+=u64::from(counters[i]);
                self.counter_peaks[i]=self.counter_peaks[i].max(counters[i]);
            }
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
            self.startup_seconds=self.start.elapsed().as_secs_f64();
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
            "route left playable village state tick={} segment={} status={}",
            self.tick,
            self.segment,
            serde_json::to_string(&self.status)?
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
                reference_feet: Some(player.feet.to_array()),
                reference_yaw: Some(*yaw),
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
        let world_wish = crate::render::vita_renderer::config().route_world_wish;
        if crate::render::vita_renderer::config().route_heading_lock && !world_wish {
            if let Some(target)=s.reference_yaw {
                if let Some(next)=heading_step(*yaw,target) {
                    if self.heading_ticks==0 {println!("[lg/route] camera alignment begin tick={} yaw={} target={target}",self.tick,*yaw);}
                    self.heading_ticks+=1;self.heading_total+=1;
                    if self.heading_ticks>1200 {
                        self.route_error=Some(format!("camera alignment exceeded 10s at tick {}",self.tick));
                        self.done=true;return Controls::default();
                    }
                    // Same bounded look integration as a stick input; no player
                    // pose, facing, scene state or gameplay clock is overwritten.
                    *yaw=next;self.segment="camera-align".into();
                    return Controls::default();
                }
            }
        }
        if self.heading_ticks>0 {
            println!("[lg/route] camera alignment done tick={} steps={}",self.tick,self.heading_ticks);
            self.heading_ticks=0;
        }
        if !self.drift_reported
            && s.reference_feet
                .is_some_and(|f| player.feet.distance(Vec3::from_array(f)) > 8.)
        {
            println!("[lg/route] replay drift tick={} segment={} expected={:?} actual={:?} yaw_expected={:?} yaw_actual={}",self.tick,s.segment,s.reference_feet,player.feet,s.reference_yaw,*yaw);
            self.drift_reported = true;
        }

        if self.segment != s.segment {
            self.segment.clone_from(&s.segment);
        }
        *yaw -= s.look[0] * FIXED_DT * 2.5;
        *pitch = (*pitch + s.look[1] * FIXED_DT * 2.).clamp(-1.5, 1.5);
        let wish = replay_wish(s, *yaw, world_wish);
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
        let dir = self.root.join("route-frame-windows").join(&self.run_id);
        std::fs::create_dir_all(&dir)?;
        self.windows += 1;
        std::fs::write(
            dir.join(format!("window-{:05}.json", self.windows)),
            serde_json::to_vec(&self.wall)?,
        )?;
        ensure!(self.counters.len()==self.wall.len(),"route counter/window alignment");
        std::fs::write(
            dir.join(format!("counters-{:05}.json", self.windows)),
            serde_json::to_vec(&self.counters)?,
        )?;
        self.wall.clear();
        self.counters.clear();
        Ok(())
    }
    pub fn should_finish(&self) -> bool {
        self.done || self.mode == Mode::Continue
    }
    pub fn finish(&mut self, store: &Store, game: &Game, yaw: f32, pitch: f32) -> Result<()> {
        ensure!(self.route_error.is_none(),"{}",self.route_error.as_deref().unwrap_or(""));
        ensure!(!self.needs_opening()||self.opening_played,"New Game route did not complete opening film");
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
            // No draw during this interval; excluded from rendered-frame counter means.
            self.counters.push([0;8]);
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
        let count_summary=|i:usize| serde_json::json!({"samples":self.counter_samples,"mean":(self.counter_samples>0).then(||self.counter_sums[i] as f64/self.counter_samples as f64),"max":self.counter_peaks[i]});
        let report = serde_json::json!({"version":VERSION,"continue":self.continuing(),"moving_input_seconds":self.moving_ticks as f64/120.,"post_knife_cat_moving_input_seconds":self.post_moving_ticks as f64/120.,"end":end,"frame_wall_ms":stats.get(&"whole-passage".to_string()),"segments":stats,"raw_windows":self.windows,"run_id":self.run_id,"raw_path":format!("route-frame-windows/{}/window-*.json",self.run_id),"counter_raw_path":format!("route-frame-windows/{}/counters-*.json",self.run_id),"counter_columns":["live_puffs","fx_gpu_draws","skinned_draws_total","skinned_draws_culled","gpu_probe_upload_requests","gpu_probe_identical_uploads","gpu_probe_uniform_requests","gpu_probe_identical_uniforms"],"counter_population":"Rendered frames; final save-only row is all zeros and excluded from means. FX draws count direct dynamic actor/object/effect packets, including first-person.","live_puffs":count_summary(0),"fx_gpu_draws":count_summary(1),"skinned_draws_total":count_summary(2),"skinned_draws_culled":count_summary(3),"gpu_probe_upload_requests":count_summary(4),"gpu_probe_identical_uploads":count_summary(5),"gpu_probe_uniform_requests":count_summary(6),"gpu_probe_identical_uniforms":count_summary(7),"window_capacity_frames":300,"histogram_segment_limit":64,"profiler_capacity_bytes":self.wall.capacity()*std::mem::size_of::<(String,f64)>()+self.counters.capacity()*std::mem::size_of::<[u32;8]>()+self.aggregates.values().map(|a|a.2.capacity()*8).sum::<usize>(),"quantiles":"1ms histogram, >=4000ms in overflow bin; raw windows permit exact quantiles","save_cpu_ms":save_ms,"wall_seconds":self.start.elapsed().as_secs_f64(),"opening_film_played":self.opening_played,"camera_alignment_ticks":self.heading_total,"camera_alignment_seconds":self.heading_total as f64/120.,"input_timeline":"120Hz script excludes scene waits and bounded camera recentering; frame wall includes both","scene_startup_seconds":self.startup_seconds,"including_scene_startup_seconds":self.startup_seconds+self.start.elapsed().as_secs_f64(),"startup_population":"Session open to first ready frame; pilot launch timestamp also captures pre-session initialization","shader_creations_after_60_frames":self.shader_after_warmup,"pipeline_creations_after_60_frames":self.pipeline_after_warmup,"heap":{"start_bytes":self.heap_start,"end_bytes":heap_end,"delta_bytes":self.heap_start.zip(heap_end).map(|(a,b)|b as i64-a as i64)},"configuration":{"vita_renderer":crate::render::vita_renderer::config(),"vita_sim":crate::vita_sim::effective()},"cumulative_frame_profiler":if crate::light_profile::full() {"full"} else {"light"}});
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
    #[test]
    fn world_wish_diagnostic_keeps_inputs_independent_of_camera_without_editing_pose_or_time() {
        let sample = Sample {stick:[0.25,-0.5],reference_yaw:Some(0.),..Default::default()};
        for yaw in [0., 1.5, -3., 7.] {
            assert_eq!(replay_wish(&sample,yaw,true),vec2(-0.5,-0.25));
            let expected=vec2(yaw.cos(),yaw.sin())*-0.5+vec2(yaw.sin(),-yaw.cos())*0.25;
            assert_eq!(replay_wish(&sample,yaw,false),expected);
        }
        let no_reference=Sample {reference_yaw:None,..sample};
        assert_eq!(replay_wish(&no_reference,1.5,true),replay_wish(&no_reference,1.5,false));
    }
    #[test]
    fn heading_recenter_uses_shortest_bounded_look_and_restores_recorded_wish() {
        for (from,target) in [(4.712389,3.141593),(6.27,0.01),(-3.13,3.13)] {
            let mut yaw=from;let mut ticks=0;
            while let Some(next)=heading_step(yaw,target) {
                assert!((next-yaw).abs()<=2.5*FIXED_DT+0.000001);
                yaw=next;ticks+=1;assert!(ticks<180);
            }
            let wish=vec2(yaw.cos(),yaw.sin())*0.8+vec2(yaw.sin(),-yaw.cos())*0.2;
            let expected=vec2(target.cos(),target.sin())*0.8+vec2(target.sin(),-target.cos())*0.2;
            assert!(wish.distance(expected)<0.0001);
        }
    }
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
