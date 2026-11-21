//! W0 uses the P1 transport while the owner waits at each simulation cut.
#[cfg(not(target_os = "vita"))]
use super::sim_audit::WrongStamp;
use super::sim_audit::{self, Audit};
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
    owner_fp: u32,
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

pub(super) struct Worker {
    pipeline: Pipeline<Request, (), Option<Reply>, ()>,
    #[cfg(not(target_os = "vita"))]
    pub(super) wrong_stamp: Option<WrongStamp>,
}

impl Worker {
    pub(super) fn new(audit: &Audit) -> Result<Self> {
        // The V2 24 MiB budget covers both slots' retained bytes, skin-held
        // backing and this sim stack. The other thread stacks are separate.
        #[cfg(target_os = "vita")]
        let stack = DEFAULT_STACK_SIZE;
        #[cfg(not(target_os = "vita"))]
        let stack = match std::env::var("LOOKING_GLASS_SIM_STACK_MIB").as_deref() {
            Err(std::env::VarError::NotPresent) | Ok("2") => DEFAULT_STACK_SIZE,
            Ok("1") => 1024 * 1024,
            _ => anyhow::bail!("LOOKING_GLASS_SIM_STACK_MIB must be 1 or 2"),
        };
        #[cfg(not(target_os = "vita"))]
        let wrong_stamp = WrongStamp::from_env()?;
        #[cfg(all(target_os = "vita", feature = "vita_renderer"))]
        let sample_stack = crate::render::vita_renderer::config().diag_snapshot_log == Some(true);
        #[cfg(not(target_os = "vita"))]
        let sample_stack = audit.rows;
        #[cfg(all(target_os = "vita", not(feature = "vita_renderer")))]
        let sample_stack = false;
        if audit.rows {
            println!(
                "[lg/sim-overlap] slot_inline_bytes={} slots=2",
                std::mem::size_of::<Option<Reply>>()
            );
        }
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        anyhow::ensure!(
            std::env::var_os("LOOKING_GLASS_SKIN_ALLOC").is_none(),
            "sim_overlap excludes the host skin allocation probe"
        );
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        let report_slots = audit.rows;
        let mut initialized = false;
        let worker = Pipeline::new(
            (),
            [None, None],
            Some(stack),
            move |stamp, request: Request, _, slot| {
                if !initialized {
                    crate::skin_worker::pin_reserved_cpu1();
                    if sample_stack {
                        crate::skin_worker::inventory("sim-pipeline", Some(0x20000));
                        sim_audit::begin_stack_sampling();
                    } else {
                        #[cfg(target_os = "vita")]
                        crate::skin_worker::startup_inventory("sim-pipeline");
                    }
                }
                let Request {
                    job,
                    clock,
                    owner_fp,
                    input,
                    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                    mut probe,
                } = request;
                assert_eq!(job.stamp, stamp, "stale sim job stamp");
                let worker_fp = sim_audit::fp_control();
                assert_eq!(
                    worker_fp, owner_fp,
                    "sim worker FP control differs from owner"
                );
                if !initialized && sample_stack {
                    println!("[lg/sim-overlap] owner_fp={owner_fp:#x} worker_fp={worker_fp:#x}");
                }
                initialized = true;
                sim_audit::sample_stack("worker_entry");
                let _step = StepGuard::enter();
                let _clock = clock.enter();
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                {
                    crate::sim_maps::probe::begin_worker_step(stamp.frame.frame);
                    probe.swap();
                }
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                let _lazy_profile = crate::collision::lazy_profile::enter(
                    crate::collision::lazy_profile::Lane::Worker,
                );
                let mut scopes = StepScopes {
                    census: None,
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
                        let view = StepView {
                            stamp: WrongStamp::corrupt(wrong_stamp, "output", view.stamp),
                            ..view
                        };
                        let seal = seal
                            .map(|want_state| job.replay_probe(map.as_deref().unwrap(), want_state))
                            .transpose();
                        let output = match seal {
                            Ok(seal) => {
                                let acoustic = acoustic.unwrap();
                                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                                if report_slots {
                                    report_payload(stamp, &view, &acoustic, seal.as_ref());
                                }
                                Ok((view, acoustic, seal))
                            }
                            Err(error) => Err(error),
                        };
                        (job, Output::B(output))
                    }
                };
                if sample_stack {
                    sim_audit::stack_sample(stamp);
                }
                #[cfg(not(target_os = "vita"))]
                let job = StepJob {
                    stamp: WrongStamp::corrupt(wrong_stamp, "state", job.stamp),
                    ..job
                };
                drop(scopes);
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                probe.swap();
                *slot = Some(Reply {
                    job,
                    output,
                    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                    probe,
                });
                Step::Complete
            },
        )?;
        println!("[lg/sim-overlap] mode=W0 stack_bytes={stack}");
        Ok(Self {
            pipeline: worker,
            #[cfg(not(target_os = "vita"))]
            wrong_stamp,
        })
    }

    pub(super) fn run(
        &mut self,
        job: StepJob,
        clock: ClockView,
        input: Input,
    ) -> Result<(StepJob, Output)> {
        let expected = job.stamp;
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        let mut probe = crate::sim_maps::probe::StepRows::default();
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        probe.swap();
        #[cfg(not(target_os = "vita"))]
        let _owner = crate::character::skinning::identity::OwnerStep::begin();
        let (stamp, _, slot) = self
            .pipeline
            .run_blocked(
                expected,
                Request {
                    job,
                    clock,
                    owner_fp: sim_audit::fp_control(),
                    input,
                    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                    probe,
                },
            )
            .map_err(|error| anyhow!("W0 sim worker: {error:?}"))?;
        let reply = slot.take().expect("missing blocked step reply");
        #[cfg(not(target_os = "vita"))]
        let stamp = WrongStamp::corrupt(self.wrong_stamp, "transport", stamp);
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

#[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
fn report_payload(
    stamp: Stamp,
    view: &StepView,
    acoustic: &AcousticView,
    seal: Option<&crate::replay::OwnedProbe>,
) {
    fn backing<T>(values: &Vec<T>) -> usize {
        values.capacity() * std::mem::size_of::<T>()
    }
    let view_bytes = view.dynamic.capacity_bytes() - std::mem::size_of_val(&view.dynamic)
        + backing(&view.targets)
        + backing(&view.presentation.reflection.0)
        + backing(&view.presentation.reflection.1);
    let seal_bytes = seal.map_or(0, |seal| {
        seal.map.capacity() + backing(&seal.targets) + seal.state.as_ref().map_or(0, backing)
    });
    println!("[lg/sim-overlap] slot_payload frame={} view_heap_bytes={} acoustic_heap_bytes={} seal_heap_bytes={}",
        stamp.frame.frame, view_bytes, acoustic.owned_backing_bytes(), seal_bytes);
}
