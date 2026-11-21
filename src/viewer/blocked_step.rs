//! W0 uses the P1 transport while the owner waits at each simulation cut.
#[cfg(not(target_os = "vita"))]
use super::sim_audit::WrongStamp;
use super::sim_audit::{self, Audit};
use super::sim_timing::{self, Clock, Cut, DetailSpec, Details, Scope, Trace, WorkerRows};
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
    Lag(
        StepAInput,
        StepBInput,
        Option<AcousticView>,
        Option<bool>,
        Option<super::lag_control::Peer>,
    ),
}

pub(super) type Seal = (Stamp, crate::replay::OwnedProbe);

pub(super) enum Output {
    A(Result<(bool, super::step::Due)>),
    B(Result<(StepView, AcousticView, Stamp, Option<Seal>)>),
    Lag(
        Result<(bool, super::step::Due)>,
        Option<Result<(StepView, AcousticView, Stamp, Option<Seal>)>>,
    ),
}

impl Input {
    fn cut(&self) -> Cut {
        match self {
            Self::A(_) => Cut::A,
            Self::B(..) => Cut::B,
            Self::Lag(..) => Cut::Lag,
        }
    }
}

struct Request {
    job: StepJob,
    clock: ClockView,
    owner_fp: u32,
    timing: Option<Clock>,
    details: DetailSpec,
    input: Input,
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    probe: crate::sim_maps::probe::StepRows,
}

struct Reply {
    job: StepJob,
    output: Output,
    rows: WorkerRows,
    late_lines: Option<crate::spike_log::WorkerLateLines>,
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    probe: crate::sim_maps::probe::StepRows,
}

pub(super) struct Worker {
    pipeline: Pipeline<Request, (), Option<Reply>, ()>,
    flight: Option<Flight>,
    next_slot: usize,
    events: bool,
}

struct Flight {
    stamp: Stamp,
    cut: Cut,
    timing: Option<Clock>,
    #[cfg(not(target_os = "vita"))]
    _owner: crate::character::skinning::identity::OwnerStep,
}

#[derive(Default)]
struct StackReport(Option<Stamp>);

impl StackReport {
    fn sampled(&mut self, stamp: Stamp) {
        self.0 = Some(stamp);
    }
}

impl Drop for StackReport {
    fn drop(&mut self) {
        #[cfg(target_os = "vita")]
        if let Some(stamp) = self.0 {
            sim_audit::stack_sample(stamp);
        }
    }
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
        let sample_stack = crate::render::vita_renderer::config().diag_sim_timing
            || crate::render::vita_renderer::config().diag_snapshot_log == Some(true);
        #[cfg(not(target_os = "vita"))]
        let sample_stack = audit.depth;
        #[cfg(all(target_os = "vita", not(feature = "vita_renderer")))]
        let sample_stack = false;
        if audit.depth {
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
        let report_slots = audit.depth;
        let mut initialized = false;
        let events = audit.diagnose();
        let mut stack_report = StackReport::default();
        let worker = Pipeline::new_with_reply_stamp(
            (),
            [None, None],
            Some(stack),
            move |reply_stamp, request: Request, _, slot| {
                let stamp = *reply_stamp;
                let worker_start = sim_timing::mark(request.timing.is_some());
                if !initialized {
                    crate::character::skinning::identity::enter_sim_worker();
                    crate::skin_worker::pin_reserved_cpu1();
                    #[cfg(target_os = "vita")]
                    crate::frame_profile::setup_worker();
                    if crate::spike_log::on() {
                        crate::spike_log::setup_worker();
                    }
                    if sample_stack {
                        #[cfg(target_os = "vita")]
                        crate::skin_worker::startup_inventory("sim-pipeline");
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
                    timing,
                    details,
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
                if events {
                    sim_audit::event("worker", stamp);
                }
                let cut = input.cut();
                if details.phase {
                    crate::phase_census::worker_begin();
                }
                if details.spike {
                    crate::spike_log::worker_begin(stamp.frame.frame);
                }
                if details.profile {
                    crate::frame_profile::worker_begin();
                }
                let mut scopes = StepScopes {
                    census: None,
                    #[cfg(any(target_os = "vita", feature = "sim_maps_probe"))]
                    frame: None,
                };
                let finish_b = |job,
                                input: StepBInput,
                                mut acoustic,
                                seal: Option<bool>,
                                scopes: &mut StepScopes| {
                    let map = seal.map(|_| input.map_name.clone());
                    let (job, view) = step_b(job, input, scopes);
                    job.sim.world.capture_acoustic_view(&mut acoustic);
                    #[cfg(not(target_os = "vita"))]
                    let view = StepView {
                        stamp: WrongStamp::corrupt(wrong_stamp, "output", view.stamp),
                        ..view
                    };
                    let output = seal
                        .map(|want_state| job.replay_probe(map.as_deref().unwrap(), want_state))
                        .transpose()
                        .map(|seal| {
                            let acoustic = acoustic.unwrap();
                            #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                            if report_slots {
                                report_payload(stamp, &view, &acoustic, seal.as_ref());
                            }
                            #[cfg(not(target_os = "vita"))]
                            let corrupt = |field| WrongStamp::corrupt(wrong_stamp, field, stamp);
                            #[cfg(target_os = "vita")]
                            let corrupt = |_| stamp;
                            (
                                view,
                                acoustic,
                                corrupt("present"),
                                seal.map(|probe| (corrupt("seal"), probe)),
                            )
                        });
                    (job, output)
                };
                let begin = sim_timing::mark(timing.is_some());
                let (job, output, step_scope) = match input {
                    Input::A(input) => {
                        let (job, due) = step_a(job, input, &mut scopes);
                        (job, Output::A(due), Scope::StepA)
                    }
                    Input::B(input, acoustic, seal) => {
                        let (job, output) = finish_b(job, input, acoustic, seal, &mut scopes);
                        (job, Output::B(output), Scope::StepB)
                    }
                    Input::Lag(input, mut next, acoustic, seal, peer) => {
                        if let Some(peer) = &peer {
                            peer.enter();
                            if events {
                                sim_audit::event("worker_inner", stamp);
                            }
                        }
                        let (job, due) = step_a(job, input, &mut scopes);
                        let reply = if let Ok((story_active, super::step::Due::None)) = &due {
                            assert_eq!(
                                sim_audit::fp_control(),
                                owner_fp,
                                "sim worker FP control differs before cut B"
                            );
                            next.story_active = *story_active;
                            let (job, output) = finish_b(job, next, acoustic, seal, &mut scopes);
                            (job, Output::Lag(due, Some(output)), Scope::StepLag)
                        } else {
                            (job, Output::Lag(due, None), Scope::StepLag)
                        };
                        if let Some(peer) = peer {
                            if events {
                                sim_audit::event("worker_inner_end", stamp);
                            }
                            peer.leave();
                        }
                        reply
                    }
                };
                drop(scopes);
                let step_row =
                    timing.and_then(|clock| clock.finish(stamp, step_scope, Some(cut), begin));
                if sample_stack {
                    stack_report.sampled(stamp);
                    #[cfg(not(target_os = "vita"))]
                    sim_audit::stack_sample(stamp);
                }
                #[cfg(not(target_os = "vita"))]
                let job = StepJob {
                    stamp: WrongStamp::corrupt(wrong_stamp, "state", job.stamp),
                    ..job
                };
                #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                probe.swap();
                let phase = details.phase.then(crate::phase_census::worker_finish);
                let (spike, late_lines) = if details.spike {
                    let (row, lines) = crate::spike_log::worker_finish();
                    (timing.is_some().then_some(row), Some(lines))
                } else {
                    (None, None)
                };
                let profile = details.profile.then(crate::frame_profile::worker_finish);
                let details =
                    (phase.is_some() || spike.is_some() || profile.is_some()).then(|| {
                        Box::new(Details {
                            stamp,
                            cut,
                            phase,
                            spike,
                            profile,
                        })
                    });
                let work = timing
                    .and_then(|clock| clock.finish(stamp, Scope::Worker, Some(cut), worker_start));
                *slot = Some(Reply {
                    job,
                    output,
                    late_lines,
                    rows: WorkerRows {
                        work,
                        step: step_row,
                        details,
                    },
                    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                    probe,
                });
                #[cfg(not(target_os = "vita"))]
                {
                    *reply_stamp = WrongStamp::corrupt(wrong_stamp, "transport", stamp);
                }
                Step::Complete
            },
        )?;
        println!("[lg/sim-overlap] mode=W0 stack_bytes={stack}");
        Ok(Self {
            pipeline: worker,
            flight: None,
            next_slot: 0,
            events,
        })
    }

    pub(super) fn run(
        &mut self,
        job: StepJob,
        clock: ClockView,
        input: Input,
        trace: &mut Trace,
    ) -> Result<(StepJob, Output)> {
        self.submit(job, clock, input, trace)?;
        self.join(trace)
    }

    pub(super) fn submit(
        &mut self,
        job: StepJob,
        clock: ClockView,
        input: Input,
        trace: &mut Trace,
    ) -> Result<()> {
        assert!(self.flight.is_none(), "sim slot still in flight");
        let stamp = job.stamp;
        let cut = input.cut();
        let timing = trace.clock(stamp.frame.frame);
        let started = trace.mark(stamp.frame.frame);
        let details = DetailSpec {
            phase: timing.is_some() && crate::phase_census::active(),
            spike: crate::spike_log::on(),
            profile: crate::frame_profile::enabled(),
        };
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        let mut probe = crate::sim_maps::probe::StepRows::default();
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        probe.swap();
        let flight = Flight {
            stamp,
            cut,
            timing,
            #[cfg(not(target_os = "vita"))]
            _owner: crate::character::skinning::identity::OwnerStep::begin(),
        };
        self.pipeline
            .submit_viewer(
                self.next_slot,
                stamp,
                Request {
                    job,
                    clock,
                    input,
                    details,
                    timing,
                    owner_fp: sim_audit::fp_control(),
                    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
                    probe,
                },
            )
            .map_err(|error| anyhow!("sim worker submit: {error:?}"))?;
        self.next_slot ^= 1;
        self.flight = Some(flight);
        trace.owner(stamp, Scope::Submit, Some(cut), started);
        Ok(())
    }

    pub(super) fn join(&mut self, trace: &mut Trace) -> Result<(StepJob, Output)> {
        let flight = self.flight.take().expect("sim join without submit");
        let expected = flight.stamp;
        let started = trace.mark(expected.frame.frame);
        let (stamp, slot) = self
            .pipeline
            .join_viewer()
            .map_err(|error| anyhow!("sim worker join: {error:?}"))?;
        trace.owner(expected, Scope::Join, Some(flight.cut), started);
        let reply = slot.take().expect("missing blocked step reply");
        assert_eq!(stamp, expected, "stale sim transport stamp");
        assert_eq!(reply.job.stamp, expected, "stale sim state stamp");
        if let Output::B(Ok((view, ..))) | Output::Lag(_, Some(Ok((view, ..)))) = &reply.output {
            assert_eq!(view.stamp, expected, "stale sim output stamp");
        }
        #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
        {
            let mut probe = reply.probe;
            probe.swap();
        }
        if let Some(lines) = reply.late_lines {
            lines.append();
        }
        #[cfg(target_os = "vita")]
        if let Some(profile) = reply
            .rows
            .details
            .as_ref()
            .and_then(|detail| detail.profile.as_ref())
        {
            crate::frame_profile::merge_worker_counters(profile);
        }
        if flight.timing.is_some() {
            trace.worker(expected, reply.rows);
        }
        Ok((reply.job, reply.output))
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.pipeline.finish_worker();
        if self.events {
            if let Some(flight) = &self.flight {
                sim_audit::event("discarded_in_flight", flight.stamp);
            }
        }
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
