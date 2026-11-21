//! W0 uses the P1 transport while the owner waits at each simulation cut.
use super::step::{step_a, step_b, StepAInput, StepBInput, StepJob, StepScopes, StepView};
use crate::{
    collision::AcousticView,
    replay::input::{ClockView, StepGuard},
    sim_pipeline::{Pipeline, Stamp, Step, DEFAULT_STACK_SIZE},
};
use anyhow::{anyhow, Result};

pub(super) enum Input {
    A(StepAInput),
    B(StepBInput, Option<AcousticView>, Option<bool>),
}

pub(super) enum Output {
    A(Result<(bool, super::step::Due)>),
    B(Result<(StepView, AcousticView, Option<crate::replay::OwnedProbe>)>),
}

struct Request {
    job: StepJob,
    clock: ClockView,
    input: Input,
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    probe: crate::sim_maps::probe::StepRows,
}

struct Reply {
    job: StepJob,
    output: Output,
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    probe: crate::sim_maps::probe::StepRows,
}

pub(super) struct Worker(Pipeline<Request, (), Option<Reply>, ()>);

impl Worker {
    pub(super) fn new() -> Result<Self> {
        // Vita's two existing 8 MiB stacks plus this 2 MiB stack leave
        // 6 MiB of the 24 MiB stack allowance for the other threads.
        #[cfg(target_os = "vita")]
        let stack = DEFAULT_STACK_SIZE;
        #[cfg(not(target_os = "vita"))]
        let stack = match std::env::var("LOOKING_GLASS_SIM_STACK_MIB").as_deref() {
            Err(std::env::VarError::NotPresent) | Ok("2") => DEFAULT_STACK_SIZE,
            Ok("1") => 1024 * 1024,
            _ => anyhow::bail!("LOOKING_GLASS_SIM_STACK_MIB must be 1 or 2"),
        };
        #[cfg(not(target_os = "vita"))]
        let wrong_stamp_frame = std::env::var("LOOKING_GLASS_SIM_WRONG_STAMP")
            .ok().map(|value| value.parse::<u64>()).transpose()?;
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        anyhow::ensure!(std::env::var_os("LOOKING_GLASS_SKIN_ALLOC").is_none(),
            "sim_overlap excludes the host skin allocation probe");
        let mut initialized = false;
        let worker = Pipeline::new((), [None, None], Some(stack), move |stamp, request: Request, _, slot| {
            if !initialized {
                crate::skin_worker::pin_reserved_cpu1();
                crate::skin_worker::inventory("sim-pipeline", Some(0x20000));
                #[cfg(target_os = "vita")]
                if let Some((_, bytes)) = crate::skin_worker::current_stack() {
                    let entry_margin = crate::skin_worker::current_stack_margin().unwrap_or(0);
                    println!("[lg/sim-overlap] thread_info_stack_bytes={bytes} entry_margin_bytes={entry_margin}");
                }
                initialized = true;
            }
            let Request {
                job, clock, input,
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                mut probe,
            } = request;
            assert_eq!(job.stamp, stamp, "stale sim job stamp");
            let _step = StepGuard::enter();
            let _clock = clock.enter();
            #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
            {
                crate::sim_maps::probe::begin_worker_step(stamp.frame.frame);
                probe.swap();
            }
            let mut scopes = StepScopes { census: None,
                #[cfg(any(target_os = "vita", feature = "sim_maps_probe"))]
                frame: None,
            };
            let (job, output) = match input {
                Input::A(input) => {
                    let (job, due) = step_a(job, input, &mut scopes);
                    (job, Output::A(due))
                }
                Input::B(input, mut acoustic, seal) => {
                    let map = seal.map(|_| input.map_name.clone());
                    let (job, view) = step_b(job, input, &mut scopes);
                    job.sim.world.capture_acoustic_view(&mut acoustic);
                    #[cfg(not(target_os = "vita"))]
                    let view = {
                        let mut view = view;
                        if wrong_stamp_frame == Some(stamp.frame.frame) {
                            view.stamp.slot_generation = view.stamp.slot_generation.wrapping_add(1);
                        }
                        view
                    };
                    let seal = seal.map(|want_state| job.replay_probe(map.as_deref().unwrap(), want_state)).transpose();
                    let output = match seal {
                        Ok(seal) => Ok((view, acoustic.unwrap(), seal)),
                        Err(error) => Err(error),
                    };
                    (job, Output::B(output))
                }
            };
            drop(scopes);
            #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
            probe.swap();
            *slot = Some(Reply {
                job, output,
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                probe,
            });
            Step::Complete
        })?;
        println!("[lg/sim-overlap] mode=W0 stack_bytes={stack}");
        Ok(Self(worker))
    }

    pub(super) fn run(&mut self, job: StepJob, clock: ClockView, input: Input)
        -> Result<(StepJob, Output)>
    {
        let expected = job.stamp;
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        let mut probe = crate::sim_maps::probe::StepRows::default();
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        probe.swap();
        #[cfg(not(target_os = "vita"))]
        let _owner = crate::character::skinning::identity::OwnerStep::begin();
        let (stamp, _, slot) = self.0.run_blocked(expected, Request {
            job, clock, input,
            #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
            probe,
        }).map_err(|error| anyhow!("W0 sim worker: {error:?}"))?;
        let reply = slot.take().expect("missing blocked step reply");
        assert_eq!(stamp, expected, "stale sim transport stamp");
        assert_eq!(reply.job.stamp, expected, "stale sim state stamp");
        if let Output::B(Ok((view, _, _))) = &reply.output {
            assert_eq!(view.stamp, expected, "stale sim output stamp");
        }
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        {
            let mut probe = reply.probe;
            probe.swap();
        }
        Ok((reply.job, reply.output))
    }
}
