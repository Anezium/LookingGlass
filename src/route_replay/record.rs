//! Host-only guidance: the existing LG physics planner proposes ordinary controls.
use super::*;
use std::collections::VecDeque;
#[derive(Default)]
pub(super) struct Driver {
    goal: usize,
    queue: VecDeque<Controls>,
    pub done: bool,
    pub error: Option<String>,
    pub planning_seconds:f32,
    post: bool,
    post_start: Option<u64>,
    loiter: bool,
}
impl Driver {
    pub fn segment(&self) -> String {
        if self.post {
            "post-scene-walk".into()
        } else {
            format!("hallway-{}", self.goal)
        }
    }
    pub fn control(
        &mut self,
        world: &crate::collision::World,
        player: &Player,
        status: &Status,
        moving: u64,
    ) -> Controls {
        const GOALS: [[f32; 3]; 17] = [
            [-4180., 5900., 16.],
            [-4192., 5416., -52.],
            [-4412., 5328., -52.],
            [-4780., 5392., -120.],
            [-4968., 5424., -120.],
            [-5100., 5200., -120.],
            [-5100., 4780., -56.],
            [-4784., 4784., -120.],
            [-4512., 4800., -96.],
            [-4288., 4808., -136.],
            [-3984., 4712., -176.],
            [-3972., 4332., -160.],
            [-3960., 4056., -224.],
            [-3968., 3840., -272.],
            [-3776., 3832., -208.],
            [-3544., 3856., -136.],
            [-3544., 3856., -136.],
        ];
        if !status.entry_done || (status.blade_copies > 0 && !status.knife_done) {
            return Controls::default();
        }
        if status.knife_done && !self.post {
            self.post = true;
            self.post_start = Some(moving);
            self.queue.clear();
        }
        if self.post && moving.saturating_sub(self.post_start.unwrap_or(moving)) >= 7200 {
            self.done = true;
            return Controls::default();
        }
        if let Some(c) = self.queue.pop_front() {
            return c;
        }
        let goal = if self.post {
            if self.loiter {
                vec3(-3544., 3856., -136.)
            } else {
                vec3(-3776., 3832., -208.)
            }
        } else {
            if self.goal >= GOALS.len() {
                return Controls::default();
            }
            Vec3::from_array(GOALS[self.goal])
        };
        if player.feet.distance(goal) < 35. && player.grounded {
            if self.post {
                self.loiter = !self.loiter;
            } else {
                self.goal += 1;
            }
            return Controls::default();
        }
        let plan_start=Instant::now();
        let path = crate::route::Route::plan(world, player, goal, 20.)
            .or_else(|_| crate::route::Route::plan(world, player, goal, 48.));
        self.planning_seconds+=plan_start.elapsed().as_secs_f32();
        match path {
            Ok(path) => {
                for (c, _) in path {
                    for tick in 0..24 {
                        self.queue.push_back(Controls {
                            jump: c.jump && tick == 0,
                            ..c
                        });
                    }
                }
                println!(
                    "[lg/route] planned segment={} goal={goal:?} controls={}",
                    self.segment(),
                    self.queue.len()
                );
                self.queue.pop_front().unwrap_or_default()
            }
            Err(states) => {
                eprintln!(
                    "[lg/route] guidance blocked at {:?} toward {goal:?} ({states} states)",
                    player.feet
                );
                self.error = Some(format!("guidance blocked toward {goal:?}"));
                self.done = true;
                Controls::default()
            }
        }
    }
}
