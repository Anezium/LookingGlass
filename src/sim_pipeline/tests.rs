use super::*;
use std::{
    collections::VecDeque,
    future::Future,
    panic::{catch_unwind, AssertUnwindSafe},
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Free,
    Fence,
    Reject,
    Mid,
    Last,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Sample(Frame),
    ConsumeSeal(Stamp),
    Yield(Stamp),
    Work(Stamp, usize, Instant),
    Captured(Stamp, Instant),
    Audio(Stamp),
    Publish(Stamp),
    Classified(Stamp, Disposition),
    Render(Stamp, Instant),
    DrawEnd(Stamp, Instant),
    Serial(Stamp),
    Resume(Stamp, Due),
    Fallback(Stamp),
    Seal(Stamp),
    Present(Stamp),
    Exit(Exit),
}

impl Event {
    fn stamp(self) -> Option<Stamp> {
        match self {
            Self::Sample(_) | Self::Exit(_) => None,
            Self::ConsumeSeal(s)
            | Self::Yield(s)
            | Self::Work(s, ..)
            | Self::Captured(s, ..)
            | Self::Audio(s)
            | Self::Publish(s)
            | Self::Classified(s, ..)
            | Self::Render(s, ..)
            | Self::DrawEnd(s, ..)
            | Self::Serial(s)
            | Self::Resume(s, ..)
            | Self::Fallback(s)
            | Self::Seal(s)
            | Self::Present(s) => Some(s),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Fault {
    Draw,
    Classify,
    RendezvousTimeout,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Due {
    checkpoint: bool,
    ending: bool,
    exit: bool,
}

// Move-only state. Each step must see the result of the last one exactly once.
#[derive(Default)]
struct State {
    stepped: u64,
    classified: u64,
    seal: Option<Stamp>,
}

struct Output {
    slot: usize,
    sample: Option<(Stamp, Kind)>,
}

type TestPipeline = Pipeline<Kind, State, Output, Due>;
type TestResult = Result<Iteration, PipelineError<Fault>>;
type RenderHook = Box<dyn FnMut(Stamp) -> Result<(), Fault> + Send>;

struct Model {
    tape: VecDeque<Kind>,
    sampled: u64,
    exit: Exit,
    trace: SyncSender<Event>,
    before_render: RenderHook,
    inside_render: RenderHook,
    after_render: RenderHook,
    reject_classification: bool,
    dispatch_enabled: bool,
    yield_in_render: bool,
}

fn frame(n: u64) -> Frame {
    Frame {
        frame: 100 + n,
        tape_step: n,
        visit_id: 7,
    }
}

fn n(stamp: Stamp) -> u64 {
    stamp.frame.tape_step
}

fn record(trace: &SyncSender<Event>, event: Event) {
    trace.send(event).unwrap();
}

impl Model {
    fn new(tape: &[Kind], trace: SyncSender<Event>) -> Self {
        Self {
            tape: tape.iter().copied().collect(),
            sampled: 0,
            exit: Exit::EndOfTape,
            trace,
            before_render: Box::new(|_| Ok(())),
            inside_render: Box::new(|_| Ok(())),
            after_render: Box::new(|_| Ok(())),
            reject_classification: false,
            dispatch_enabled: true,
            yield_in_render: false,
        }
    }

    fn present(&self, stamp: Stamp) {
        record(&self.trace, Event::Present(stamp));
    }

    fn finish_live(&self, state: &mut State, stamp: Stamp) {
        state.seal = Some(stamp);
        record(&self.trace, Event::Seal(stamp));
        self.present(stamp);
    }
}

impl Owner<Kind, State, Output, Due> for Model {
    type Error = Fault;

    fn pulse(&mut self, state: &mut State) -> Result<Pulse<Kind>, Fault> {
        assert_eq!(state.stepped, self.sampled);
        assert_eq!(state.classified, self.sampled);
        if self.sampled != 0 {
            let seal = state.seal.take().expect("the previous step must be sealed");
            assert_eq!(seal.frame, frame(self.sampled));
            record(&self.trace, Event::ConsumeSeal(seal));
        }
        let Some(kind) = self.tape.pop_front() else {
            record(&self.trace, Event::Exit(self.exit));
            return Ok(Pulse::Exit(self.exit));
        };
        self.sampled += 1;
        let frame = frame(self.sampled);
        record(&self.trace, Event::Sample(frame));
        let pre_fenced = matches!(kind, Kind::Fence | Kind::Last);
        Ok(
            if pre_fenced || (!self.dispatch_enabled && self.sampled > 1) {
                Pulse::PreFenced(frame, kind)
            } else {
                Pulse::Dispatchable(frame, kind)
            },
        )
    }

    fn classify(
        &mut self,
        state: &mut State,
        stamp: Stamp,
        output: &mut Output,
    ) -> Result<Disposition, Fault> {
        assert_eq!(state.stepped, n(stamp));
        let (captured, kind) = output.sample.unwrap();
        assert_eq!(captured, stamp);
        record(&self.trace, Event::Audio(stamp));
        record(&self.trace, Event::Publish(stamp));
        if self.reject_classification {
            return Err(Fault::Classify);
        }
        let disposition = if kind == Kind::Reject {
            Disposition::PostFenced
        } else {
            Disposition::Free
        };
        record(&self.trace, Event::Classified(stamp, disposition));
        state.classified = n(stamp);
        Ok(disposition)
    }

    async fn render_free(&mut self, stamp: Stamp, output: &mut Output) -> Result<(), Fault> {
        assert_eq!(output.sample, Some((stamp, Kind::Free)));
        (self.before_render)(stamp)?;
        record(&self.trace, Event::Render(stamp, Instant::now()));
        (self.inside_render)(stamp)?;
        self.present(stamp);
        record(&self.trace, Event::DrawEnd(stamp, Instant::now()));
        if self.yield_in_render {
            let mut yielded = false;
            std::future::poll_fn(|cx| {
                if yielded {
                    return Poll::Ready(());
                }
                yielded = true;
                record(&self.trace, Event::Yield(stamp));
                cx.waker().wake_by_ref();
                Poll::Pending
            })
            .await;
        }
        (self.after_render)(stamp)
    }

    async fn post_fenced(
        &mut self,
        state: &mut State,
        stamp: Stamp,
        output: &mut Output,
    ) -> Result<Iteration, Fault> {
        assert_eq!(state.stepped, n(stamp));
        assert_eq!(output.sample.take(), Some((stamp, Kind::Reject)));
        record(&self.trace, Event::Fallback(stamp));
        self.finish_live(state, stamp);
        Ok(Iteration::Continue)
    }

    async fn mid_step(
        &mut self,
        state: &mut State,
        stamp: Stamp,
        prefix: &mut Output,
        due: Due,
    ) -> Result<Iteration, Fault> {
        assert_eq!(prefix.sample.take(), Some((stamp, Kind::Mid)));
        assert_eq!(state.stepped, n(stamp));
        state.classified = n(stamp);
        record(&self.trace, Event::Audio(stamp));
        record(&self.trace, Event::Resume(stamp, due));
        self.finish_live(state, stamp);
        Ok(Iteration::Continue)
    }

    async fn serial(
        &mut self,
        state: &mut State,
        stamp: Stamp,
        job: Kind,
    ) -> Result<Iteration, Fault> {
        assert_eq!(stamp.slot_generation, 0);
        assert_eq!(state.stepped + 1, n(stamp));
        state.stepped = n(stamp);
        state.classified = n(stamp);
        record(&self.trace, Event::Serial(stamp));
        self.finish_live(state, stamp);
        Ok(if job == Kind::Last {
            Iteration::Stop(Exit::Quit)
        } else {
            Iteration::Continue
        })
    }
}

fn slots() -> [Output; 2] {
    [0, 1].map(|slot| Output { slot, sample: None })
}

const CHECKPOINT: Due = Due {
    checkpoint: true,
    ending: false,
    exit: false,
};

fn worker(
    trace: SyncSender<Event>,
    mut before: impl FnMut(Stamp) + Send + 'static,
    mut inside: impl FnMut(Stamp) + Send + 'static,
    mut after: impl FnMut(Stamp) + Send + 'static,
) -> impl FnMut(Stamp, Kind, &mut State, &mut Output) -> Step<Due> + Send {
    move |stamp, kind, state, output| {
        before(stamp);
        record(&trace, Event::Work(stamp, output.slot, Instant::now()));
        assert_eq!(state.stepped, n(stamp) - 1);
        assert_eq!(state.classified, n(stamp) - 1);
        assert_ne!(stamp.slot_generation, 0);
        state.stepped = n(stamp);
        output.sample = Some((stamp, kind));
        inside(stamp);
        if kind != Kind::Mid {
            // A complete capture carries a provisional seal. Fallback replaces it.
            state.seal = Some(stamp);
            record(&trace, Event::Captured(stamp, Instant::now()));
        }
        after(stamp);
        if kind == Kind::Mid {
            Step::MidStep(CHECKPOINT)
        } else {
            Step::Complete
        }
    }
}

fn setup(tape: &[Kind]) -> (TestPipeline, Model, Receiver<Event>) {
    let (trace, events) = sync_channel(512);
    let pipeline = Pipeline::new(
        State::default(),
        slots(),
        None,
        worker(trace.clone(), |_| {}, |_| {}, |_| {}),
    )
    .unwrap();
    (pipeline, Model::new(tape, trace), events)
}

fn run<F: Future>(future: F) -> F::Output {
    struct Parker(thread::Thread);
    impl Wake for Parker {
        fn wake(self: std::sync::Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(std::sync::Arc::new(Parker(thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::park(),
        }
    }
}

fn bounded<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (complete, result) = sync_channel(1);
    let thread = thread::spawn(move || complete.send(f()).ok());
    match result.recv_timeout(3 * WAIT) {
        Ok(value) => {
            thread.join().unwrap();
            value
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            resume_unwind(thread.join().unwrap_err());
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => panic!("protocol test timed out"),
    }
}

fn events(rx: &Receiver<Event>) -> Vec<Event> {
    rx.try_iter().collect()
}

fn position(events: &[Event], find: impl FnMut(&Event) -> bool) -> usize {
    events
        .iter()
        .position(find)
        .unwrap_or_else(|| panic!("missing event in {events:?}"))
}

fn presents(events: &[Event]) -> Vec<Stamp> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Present(stamp) => Some(*stamp),
            _ => None,
        })
        .collect()
}

fn samples(events: &[Event]) -> Vec<Frame> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Sample(frame) => Some(*frame),
            _ => None,
        })
        .collect()
}

fn assert_tape(events: &[Event], count: u64) {
    let presented = presents(events);
    let expected = (1..=count).map(frame).collect::<Vec<_>>();
    assert_eq!(samples(events), expected);
    assert_eq!(
        presented
            .iter()
            .map(|stamp| stamp.frame)
            .collect::<Vec<_>>(),
        expected
    );
    for stamp in presented {
        assert!(events
            .iter()
            .filter_map(|event| event.stamp())
            .filter(|seen| seen.frame == stamp.frame)
            .all(|seen| seen == stamp));
    }
}

fn continue_one(pipeline: &mut TestPipeline, owner: &mut Model) {
    assert_eq!(run(pipeline.iteration(owner)), Ok(Iteration::Continue));
}

#[test]
fn first_frame_is_serial() {
    let (mut pipeline, mut owner, rx) = setup(&[Kind::Fence, Kind::Free]);
    continue_one(&mut pipeline, &mut owner);
    let stamp = Stamp {
        frame: frame(1),
        slot_generation: 0,
    };
    assert_eq!(
        events(&rx),
        [
            Event::Sample(frame(1)),
            Event::Serial(stamp),
            Event::Seal(stamp),
            Event::Present(stamp),
        ]
    );
    assert_eq!(pipeline.state.as_ref().unwrap().stepped, 1);
    assert!(pipeline.slots.iter().all(Option::is_some));
    assert!(!pipeline.in_flight);
}

#[test]
fn entry_pulses_twice_then_presents_first() {
    let (mut pipeline, mut owner, rx) = setup(&[Kind::Fence, Kind::Free, Kind::Free]);
    continue_one(&mut pipeline, &mut owner);
    continue_one(&mut pipeline, &mut owner);
    let trace = events(&rx);
    assert_eq!(samples(&trace), [frame(1), frame(2), frame(3)]);
    assert_eq!(
        presents(&trace)
            .iter()
            .map(|stamp| n(*stamp))
            .collect::<Vec<_>>(),
        [1, 2]
    );
    let classified = position(
        &trace,
        |event| matches!(event, Event::Classified(s, Disposition::Free) if n(*s) == 2),
    );
    let sample = position(&trace, |event| *event == Event::Sample(frame(3)));
    let present = position(
        &trace,
        |event| matches!(event, Event::Present(s) if n(*s) == 2),
    );
    assert!(classified < sample && sample < present);
    assert!(pipeline.state.is_none() && pipeline.in_flight);
    assert!(pipeline.slots[0].is_some() && pipeline.slots[1].is_none());
    pipeline.shutdown().unwrap();
}

#[test]
fn free_free_alternates_slots_and_stamps() {
    let (mut pipeline, mut owner, rx) = setup(&[Kind::Free; 4]);
    for _ in 0..3 {
        continue_one(&mut pipeline, &mut owner);
    }
    assert_eq!(
        run(pipeline.iteration(&mut owner)),
        Ok(Iteration::Stop(Exit::EndOfTape))
    );
    let trace = events(&rx);
    assert_tape(&trace, 4);
    let uses = trace
        .iter()
        .filter_map(|event| match event {
            Event::Work(s, slot, _) => Some((*slot, s.slot_generation)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(uses, [(0, 1), (1, 1), (0, 2), (1, 2)]);
}

#[test]
fn pre_fenced_flushes_before_serial_continuation() {
    let (mut pipeline, mut owner, rx) = setup(&[Kind::Free, Kind::Fence]);
    owner.yield_in_render = true;
    continue_one(&mut pipeline, &mut owner);
    let trace = events(&rx);
    assert_tape(&trace, 2);
    let sample = position(&trace, |event| *event == Event::Sample(frame(2)));
    let present = position(
        &trace,
        |event| matches!(event, Event::Present(s) if n(*s) == 1),
    );
    let serial = position(
        &trace,
        |event| matches!(event, Event::Serial(s) if n(*s) == 2),
    );
    let yielded = position(
        &trace,
        |event| matches!(event, Event::Yield(s) if n(*s) == 1),
    );
    assert!(sample < present && present < yielded && yielded < serial);
    assert!(!pipeline.in_flight);
}

#[test]
fn mid_step_return_resumes_with_due_without_pulsing() {
    let (mut pipeline, mut owner, rx) = setup(&[Kind::Free, Kind::Mid, Kind::Free]);
    continue_one(&mut pipeline, &mut owner);
    continue_one(&mut pipeline, &mut owner);
    let trace = events(&rx);
    assert_tape(&trace, 2);
    let first = position(
        &trace,
        |event| matches!(event, Event::Present(s) if n(*s) == 1),
    );
    let audio = position(
        &trace,
        |event| matches!(event, Event::Audio(s) if n(*s) == 2),
    );
    let resume = position(
        &trace,
        |event| matches!(event, Event::Resume(s, CHECKPOINT) if n(*s) == 2),
    );
    assert!(first < audio && audio < resume);
    assert!(!trace.iter().any(|event| matches!(
        event, Event::Captured(s, _) | Event::Render(s, _) if n(*s) == 2
    )));
    assert_eq!(
        pipeline
            .slots
            .each_ref()
            .map(|slot| slot.as_ref().unwrap().slot),
        [0, 1]
    );
    assert!(pipeline.slots[1].as_ref().unwrap().sample.is_none());
    assert_eq!(pipeline.state.as_ref().unwrap().stepped, 2);
    assert!(!pipeline.in_flight);
}

#[test]
fn mid_step_on_entry_resumes_without_second_pulse() {
    let (mut pipeline, mut owner, rx) = setup(&[Kind::Mid, Kind::Free]);
    continue_one(&mut pipeline, &mut owner);
    let trace = events(&rx);
    assert_tape(&trace, 1);
    assert!(trace.iter().any(|event| matches!(
        event, Event::Resume(s, CHECKPOINT) if s.frame == frame(1)
    )));
    assert!(!trace
        .iter()
        .any(|event| matches!(event, Event::Captured(..) | Event::Render(..))));
    assert_eq!(
        pipeline
            .slots
            .each_ref()
            .map(|slot| slot.as_ref().unwrap().slot),
        [0, 1]
    );
    assert!(pipeline
        .slots
        .iter()
        .flatten()
        .all(|slot| slot.sample.is_none()));
    assert!(pipeline.state.is_some() && !pipeline.in_flight);
}

#[test]
fn post_fenced_falls_back_on_owner_without_pulsing() {
    let (mut pipeline, mut owner, rx) = setup(&[Kind::Free, Kind::Reject, Kind::Free]);
    continue_one(&mut pipeline, &mut owner);
    continue_one(&mut pipeline, &mut owner);
    let trace = events(&rx);
    assert_tape(&trace, 2);
    let first = position(
        &trace,
        |event| matches!(event, Event::Present(s) if n(*s) == 1),
    );
    let fallback = position(
        &trace,
        |event| matches!(event, Event::Fallback(s) if n(*s) == 2),
    );
    let seal = position(
        &trace,
        |event| matches!(event, Event::Seal(s) if n(*s) == 2),
    );
    assert!(first < fallback && fallback < seal);
    assert!(!trace
        .iter()
        .any(|event| matches!(event, Event::Render(s, _) if n(*s) == 2)));
    assert!(pipeline.slots.iter().all(Option::is_some));
    assert!(!pipeline.in_flight);
}

#[test]
fn consecutive_fences_keep_both_slots_idle() {
    let (mut pipeline, mut owner, rx) = setup(&[Kind::Fence, Kind::Fence]);
    continue_one(&mut pipeline, &mut owner);
    continue_one(&mut pipeline, &mut owner);
    let trace = events(&rx);
    assert_tape(&trace, 2);
    assert!(presents(&trace)
        .iter()
        .all(|stamp| stamp.slot_generation == 0));
    assert!(!trace.iter().any(|event| matches!(event, Event::Work(..))));
    assert!(pipeline.slots.iter().all(Option::is_some));
    assert!(pipeline.state.is_some() && !pipeline.in_flight);
}

#[test]
fn last_free_presents_before_next_owner_fence_work() {
    let (mut pipeline, mut owner, rx) = setup(&[Kind::Free, Kind::Free, Kind::Fence]);
    continue_one(&mut pipeline, &mut owner);
    assert!(pipeline.state.is_none() && pipeline.in_flight);
    owner.yield_in_render = true;
    continue_one(&mut pipeline, &mut owner);
    let trace = events(&rx);
    assert_tape(&trace, 3);
    let last = position(
        &trace,
        |event| matches!(event, Event::Present(s) if n(*s) == 2),
    );
    let fence = position(
        &trace,
        |event| matches!(event, Event::Serial(s) if n(*s) == 3),
    );
    let yielded = position(
        &trace,
        |event| matches!(event, Event::Yield(s) if n(*s) == 2),
    );
    assert!(last < yielded && yielded < fence);
    assert!(pipeline.state.is_some() && !pipeline.in_flight);
    assert!(pipeline.slots.iter().all(Option::is_some));
}

#[test]
fn quit_flushes_held_image_and_quit_after_frame_is_serial() {
    let (mut pipeline, mut owner, rx) = setup(&[Kind::Free, Kind::Free]);
    owner.exit = Exit::Quit;
    continue_one(&mut pipeline, &mut owner);
    assert_eq!(
        run(pipeline.iteration(&mut owner)),
        Ok(Iteration::Stop(Exit::Quit))
    );
    let trace = events(&rx);
    assert_tape(&trace, 2);
    let quit = position(&trace, |event| *event == Event::Exit(Exit::Quit));
    let last = position(
        &trace,
        |event| matches!(event, Event::Present(s) if n(*s) == 2),
    );
    assert!(quit < last);
    assert!(!pipeline.in_flight);

    let (mut pipeline, mut owner, rx) = setup(&[Kind::Free, Kind::Last]);
    assert_eq!(
        run(pipeline.iteration(&mut owner)),
        Ok(Iteration::Stop(Exit::Quit))
    );
    let trace = events(&rx);
    assert_tape(&trace, 2);
    assert_eq!(presents(&trace)[1].slot_generation, 0);
    assert!(!pipeline.in_flight);
}

#[test]
fn exit_on_entry_leaves_state_and_both_slots_home() {
    for exit in [Exit::Quit, Exit::EndOfTape] {
        let (mut pipeline, mut owner, rx) = setup(&[]);
        owner.exit = exit;
        assert_eq!(
            run(pipeline.iteration(&mut owner)),
            Ok(Iteration::Stop(exit))
        );
        assert_eq!(events(&rx), [Event::Exit(exit)]);
        assert_eq!(pipeline.state.as_ref().unwrap().stepped, 0);
        assert!(pipeline.slots.iter().all(Option::is_some));
        assert!(!pipeline.in_flight);
        pipeline.shutdown().unwrap();
    }
}

#[test]
fn draw_error_returns_without_present_and_shutdown_joins() {
    bounded(|| {
        let (mut pipeline, mut owner, rx) = setup(&[Kind::Free, Kind::Free]);
        owner.inside_render = Box::new(|_| Err(Fault::Draw));
        assert_eq!(
            run(pipeline.iteration(&mut owner)),
            Err(PipelineError::Owner(Fault::Draw))
        );
        assert!(pipeline.in_flight);
        pipeline.shutdown().unwrap();
        let trace = events(&rx);
        assert!(presents(&trace).is_empty());
        assert!(trace
            .iter()
            .any(|event| matches!(event, Event::Captured(s, _) if n(*s) == 2)));
        assert!(!trace
            .iter()
            .any(|event| matches!(event, Event::Audio(s) if n(*s) == 2)));
        let consumed = trace
            .iter()
            .filter_map(|event| match event {
                Event::ConsumeSeal(stamp) => Some(stamp.frame),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(consumed, [frame(1)]);
        let seal = position(&trace, |event| matches!(event, Event::ConsumeSeal(_)));
        let sample = position(&trace, |event| *event == Event::Sample(frame(2)));
        let render = position(&trace, |event| matches!(event, Event::Render(..)));
        assert!(seal < sample && sample < render);
    });
}

#[test]
fn end_of_tape_with_nonempty_slot_presents_exactly_once() {
    let (mut pipeline, mut owner, rx) = setup(&[Kind::Free; 3]);
    continue_one(&mut pipeline, &mut owner);
    continue_one(&mut pipeline, &mut owner);
    assert_eq!(
        run(pipeline.iteration(&mut owner)),
        Ok(Iteration::Stop(Exit::EndOfTape))
    );
    let trace = events(&rx);
    assert_tape(&trace, 3);
    let end = position(&trace, |event| *event == Event::Exit(Exit::EndOfTape));
    let last = position(
        &trace,
        |event| matches!(event, Event::Present(s) if n(*s) == 3),
    );
    let consume = position(
        &trace,
        |event| matches!(event, Event::ConsumeSeal(s) if n(*s) == 3),
    );
    assert!(consume < end && end < last);
    assert!(pipeline.slots.iter().all(Option::is_some));
    assert!(pipeline.state.is_some() && !pipeline.in_flight);
}

#[test]
fn injected_rejection_on_entry_stays_serial_for_the_iteration() {
    let (mut pipeline, mut owner, rx) = setup(&[Kind::Reject, Kind::Free]);
    continue_one(&mut pipeline, &mut owner);
    let trace = events(&rx);
    assert_tape(&trace, 1);
    assert!(trace
        .iter()
        .any(|event| matches!(event, Event::Fallback(_))));
    assert!(!trace.iter().any(|event| matches!(event, Event::Render(..))));
    assert!(pipeline.state.is_some() && !pipeline.in_flight);
}

#[test]
fn classification_failure_prevents_next_state_send() {
    bounded(|| {
        let (mut pipeline, mut owner, rx) = setup(&[Kind::Free, Kind::Free]);
        owner.reject_classification = true;
        assert_eq!(
            run(pipeline.iteration(&mut owner)),
            Err(PipelineError::Owner(Fault::Classify))
        );
        assert_eq!(pipeline.state.as_ref().unwrap().stepped, 1);
        assert!(!pipeline.in_flight);
        drop(pipeline);
        let trace = events(&rx);
        assert_eq!(samples(&trace), [frame(1)]);
        assert!(!trace
            .iter()
            .any(|event| matches!(event, Event::Work(s, ..) if n(*s) == 2)));
    });
}

#[test]
fn worker_panic_payload_resumes_at_owner_join() {
    bounded(|| {
        #[derive(Debug, PartialEq, Eq)]
        struct WorkerPanic(u8);
        let (trace, _rx) = sync_channel(512);
        let mut pipeline = TestPipeline::new(
            State::default(),
            slots(),
            Some(1024 * 1024),
            |stamp, _, state, output| {
                if n(stamp) == 2 {
                    std::panic::panic_any(WorkerPanic(42));
                }
                state.stepped = n(stamp);
                state.seal = Some(stamp);
                output.sample = Some((stamp, Kind::Free));
                Step::Complete
            },
        )
        .unwrap();
        let mut owner = Model::new(&[Kind::Free, Kind::Free], trace);
        continue_one(&mut pipeline, &mut owner);
        let panic =
            catch_unwind(AssertUnwindSafe(|| run(pipeline.iteration(&mut owner)))).unwrap_err();
        assert_eq!(*panic.downcast::<WorkerPanic>().unwrap(), WorkerPanic(42));
    });
}

#[test]
fn disconnected_worker_reports_at_join() {
    bounded(|| {
        let (trace, _rx) = sync_channel(512);
        let mut pipeline =
            TestPipeline::spawn(State::default(), slots(), None, 1, |jobs, _done| {
                drop(jobs.recv().unwrap());
            })
            .unwrap();
        let mut owner = Model::new(&[Kind::Free], trace);
        assert_eq!(
            run(pipeline.iteration(&mut owner)),
            Err(PipelineError::Disconnected)
        );
        assert!(pipeline.worker.is_none());
    });
}

#[test]
fn idle_drop_closes_jobs_and_joins() {
    bounded(|| {
        let (ended, exit) = sync_channel(1);
        let pipeline =
            TestPipeline::spawn(State::default(), slots(), None, 1, move |jobs, _done| {
                assert!(jobs.recv().is_err());
                ended.send(()).unwrap();
            })
            .unwrap();
        drop(pipeline);
        exit.try_recv().unwrap();
    });
}

#[test]
#[should_panic(expected = "worker-side assertion")]
fn worker_assertion_fails_owner_at_shutdown() {
    let (trace, _rx) = sync_channel(512);
    let mut pipeline = TestPipeline::new(
        State::default(),
        slots(),
        None,
        worker(
            trace.clone(),
            |stamp| assert_eq!(n(stamp), 1, "worker-side assertion"),
            |_| {},
            |_| {},
        ),
    )
    .unwrap();
    let mut owner = Model::new(&[Kind::Free, Kind::Free], trace);
    continue_one(&mut pipeline, &mut owner);
    pipeline
        .shutdown()
        .unwrap_or_else(|panic| resume_unwind(panic));
}

#[test]
#[should_panic(expected = "worker panic during drop")]
fn drop_propagates_worker_panic() {
    let pipeline = TestPipeline::spawn(State::default(), slots(), None, 1, |jobs, _done| {
        assert!(jobs.recv().is_ok(), "worker panic during drop");
    })
    .unwrap();
    drop(pipeline);
}

#[test]
#[should_panic(expected = "owner unwind")]
fn drop_preserves_owner_panic_during_worker_panic() {
    struct PanicOnDrop;
    impl Drop for PanicOnDrop {
        fn drop(&mut self) {
            panic!("dropping worker panic payload");
        }
    }

    let _pipeline = TestPipeline::spawn(State::default(), slots(), None, 1, |jobs, _done| {
        assert!(jobs.recv().is_err());
        std::panic::panic_any(PanicOnDrop);
    })
    .unwrap();
    panic!("owner unwind");
}

#[test]
fn owner_error_returns_while_worker_is_mid_step() {
    bounded(|| {
        let (trace, rx) = sync_channel(512);
        let (started, entered) = sync_channel(1);
        let (release, resume) = sync_channel(1);
        let mut pipeline = Pipeline::new(
            State::default(),
            slots(),
            None,
            worker(
                trace.clone(),
                |_| {},
                move |stamp| {
                    if n(stamp) == 2 {
                        started.send(()).unwrap();
                        resume
                            .recv_timeout(WAIT)
                            .expect("owner must return before step can finish");
                    }
                },
                |_| {},
            ),
        )
        .unwrap();
        let mut owner = Model::new(&[Kind::Free, Kind::Free], trace);
        owner.inside_render = Box::new(move |_| {
            entered.recv_timeout(WAIT).unwrap();
            Err(Fault::Draw)
        });
        assert_eq!(
            run(pipeline.iteration(&mut owner)),
            Err(PipelineError::Owner(Fault::Draw))
        );
        assert!(pipeline.in_flight);
        assert!(events(&rx)
            .iter()
            .all(|event| !matches!(event, Event::Captured(s, _) if n(*s) == 2)));
        release.send(()).unwrap();
        pipeline.shutdown().unwrap();
        assert!(events(&rx)
            .iter()
            .any(|event| matches!(event, Event::Captured(s, _) if n(*s) == 2)));
    });
}

#[test]
fn shutdown_releases_worker_blocked_sending_done() {
    bounded(|| {
        let (trace, _rx) = sync_channel(512);
        let (sending, ready) = sync_channel(0);
        let (unblocked, result) = sync_channel(1);
        let mut pipeline =
            TestPipeline::spawn(State::default(), slots(), None, 0, move |jobs, done| {
                let mut step = worker(
                    trace,
                    |_| {},
                    |_| {},
                    move |stamp| {
                        if n(stamp) == 2 {
                            sending.send(()).unwrap();
                        }
                    },
                );
                // With a zero-capacity Done channel, joining before disconnect
                // would hang: no owner receives the second result.
                TestPipeline::work_loop(jobs, done, |stamp, input, state, output| {
                    step(*stamp, input, state, output)
                });
                unblocked.send(()).unwrap();
            })
            .unwrap();
        let (trace, _owner_rx) = sync_channel(512);
        let mut owner = Model::new(&[Kind::Free, Kind::Free], trace);
        owner.inside_render = Box::new(move |_| {
            ready.recv_timeout(WAIT).unwrap();
            Err(Fault::Draw)
        });
        assert_eq!(
            run(pipeline.iteration(&mut owner)),
            Err(PipelineError::Owner(Fault::Draw))
        );
        assert!(pipeline.in_flight);
        assert!(result.try_recv().is_err());
        pipeline.shutdown().unwrap();
        result.try_recv().unwrap();
    });
}

#[test]
fn owner_first_and_worker_first_with_delays() {
    for delay_us in [0, 2_000, 5_000, 12_000] {
        for owner_first in [true, false] {
            bounded(move || {
                let delay = Duration::from_micros(delay_us);
                let (trace, rx) = sync_channel(512);
                let (owner_ended, owner_end) = sync_channel(1);
                let (worker_ended, worker_end) = sync_channel(1);
                let mut pipeline = Pipeline::new(
                    State::default(),
                    slots(),
                    None,
                    worker(
                        trace.clone(),
                        move |stamp| {
                            if n(stamp) == 2 && owner_first {
                                owner_end.recv_timeout(WAIT).unwrap();
                            }
                        },
                        move |_| thread::sleep(delay),
                        move |stamp| {
                            if n(stamp) == 2 && !owner_first {
                                worker_ended.send(()).unwrap();
                            }
                        },
                    ),
                )
                .unwrap();
                let mut owner = Model::new(&[Kind::Free, Kind::Free], trace);
                owner.before_render = Box::new(move |stamp| {
                    if n(stamp) == 1 && !owner_first {
                        worker_end.recv_timeout(WAIT).unwrap();
                    }
                    Ok(())
                });
                owner.inside_render = Box::new(move |_| {
                    thread::sleep(delay);
                    Ok(())
                });
                owner.after_render = Box::new(move |stamp| {
                    if n(stamp) == 1 && owner_first {
                        owner_ended.send(()).unwrap();
                    }
                    Ok(())
                });
                continue_one(&mut pipeline, &mut owner);
                assert!(pipeline.in_flight);
                assert_eq!(
                    run(pipeline.iteration(&mut owner)),
                    Ok(Iteration::Stop(Exit::EndOfTape))
                );
                pipeline.shutdown().unwrap();
                let trace = events(&rx);
                assert_tape(&trace, 2);
                if owner_first {
                    let draw_end = position(
                        &trace,
                        |event| matches!(event, Event::DrawEnd(s, _) if n(*s) == 1),
                    );
                    let work = position(
                        &trace,
                        |event| matches!(event, Event::Work(s, ..) if n(*s) == 2),
                    );
                    assert!(draw_end < work);
                } else {
                    let captured = position(
                        &trace,
                        |event| matches!(event, Event::Captured(s, _) if n(*s) == 2),
                    );
                    let draw = position(
                        &trace,
                        |event| matches!(event, Event::Render(s, _) if n(*s) == 1),
                    );
                    assert!(captured < draw);
                }
            });
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RendezvousOrder {
    Overlap,
    DispatchDisabled,
    RenderFirst,
}

fn rendezvous(order: RendezvousOrder) -> TestResult {
    bounded(move || {
        let (worker_mark, worker_in_step) = sync_channel(1);
        let (owner_mark, owner_in_render) = sync_channel(1);
        let (render_returned, allow_worker) = sync_channel(1);
        let (exchanged, worker_exchanged) = sync_channel(1);
        let (trace, _rx) = sync_channel(512);
        let mut pipeline = Pipeline::new(
            State::default(),
            slots(),
            None,
            worker(
                trace.clone(),
                move |stamp| {
                    if n(stamp) == 2 && order == RendezvousOrder::RenderFirst {
                        // Model serial scheduling with a causal gate. Even a
                        // delayed render gets its full timeout before step 2.
                        allow_worker.recv_timeout(3 * WAIT).unwrap();
                    }
                },
                move |stamp| {
                    if n(stamp) == 2 {
                        worker_mark.send(()).unwrap();
                        owner_in_render
                            .recv_timeout(WAIT)
                            .expect("worker missed render rendezvous");
                        exchanged.send(()).unwrap();
                    }
                },
                |_| {},
            ),
        )
        .unwrap();
        let mut owner = Model::new(&[Kind::Free, Kind::Free], trace);
        owner.dispatch_enabled = order != RendezvousOrder::DispatchDisabled;
        owner.inside_render = Box::new(move |stamp| {
            assert_eq!(n(stamp), 1);
            // Send our mark from inside render, then wait for the stepping worker.
            owner_mark.send(()).unwrap();
            worker_in_step.recv_timeout(WAIT).map_err(|err| {
                assert_eq!(err, std::sync::mpsc::RecvTimeoutError::Timeout);
                Fault::RendezvousTimeout
            })
        });
        let result = run(pipeline.iteration(&mut owner));
        render_returned.send(()).unwrap();
        pipeline.shutdown().unwrap();
        if order == RendezvousOrder::DispatchDisabled {
            assert!(worker_exchanged.try_recv().is_err());
        } else {
            worker_exchanged.try_recv().unwrap();
        }
        result
    })
}

#[test]
fn rendezvous_requires_both_lanes_inside_their_intervals() {
    assert_eq!(
        rendezvous(RendezvousOrder::Overlap),
        Ok(Iteration::Continue)
    );
}

#[test]
fn rendezvous_times_out_when_overlap_dispatch_is_disabled() {
    assert_eq!(
        rendezvous(RendezvousOrder::DispatchDisabled),
        Err(PipelineError::Owner(Fault::RendezvousTimeout))
    );
}

#[test]
fn rendezvous_times_out_with_both_hooks_in_serial_order() {
    assert_eq!(
        rendezvous(RendezvousOrder::RenderFirst),
        Err(PipelineError::Owner(Fault::RendezvousTimeout))
    );
}

fn intervals_overlap(first: (Instant, Instant), second: (Instant, Instant)) -> bool {
    first.0.max(second.0) < first.1.min(second.1)
}

fn synthetic_work(duration: Duration) {
    let until = Instant::now() + duration;
    let mut value = 1_u64;
    while Instant::now() < until {
        for _ in 0..512 {
            value = value.wrapping_mul(31).wrapping_add(17);
        }
        std::hint::black_box(value);
    }
}

#[test]
fn unforced_overlap_intersects_measured_intervals() {
    const PAIRS: u64 = 24;
    bounded(|| {
        let (trace, rx) = sync_channel(512);
        let mut pipeline = Pipeline::new(
            State::default(),
            slots(),
            None,
            worker(
                trace.clone(),
                |_| {},
                |_| synthetic_work(Duration::from_millis(8)),
                |_| {},
            ),
        )
        .unwrap();
        let mut owner = Model::new(&[Kind::Free; PAIRS as usize + 1], trace);
        owner.inside_render = Box::new(|_| {
            synthetic_work(Duration::from_millis(24));
            Ok(())
        });
        for _ in 0..PAIRS {
            continue_one(&mut pipeline, &mut owner);
        }
        assert_eq!(
            run(pipeline.iteration(&mut owner)),
            Ok(Iteration::Stop(Exit::EndOfTape))
        );
        let trace = events(&rx);
        assert_tape(&trace, PAIRS + 1);
        let mut intersects = 0;
        for pair in 1..=PAIRS {
            let start_work = trace
                .iter()
                .find_map(|event| match event {
                    Event::Work(s, _, t) if s.frame == frame(pair + 1) => Some(*t),
                    _ => None,
                })
                .unwrap();
            let end_work = trace
                .iter()
                .find_map(|event| match event {
                    Event::Captured(s, t) if s.frame == frame(pair + 1) => Some(*t),
                    _ => None,
                })
                .unwrap();
            let start_render = trace
                .iter()
                .find_map(|event| match event {
                    Event::Render(s, t) if s.frame == frame(pair) => Some(*t),
                    _ => None,
                })
                .unwrap();
            let end_render = trace
                .iter()
                .find_map(|event| match event {
                    Event::DrawEnd(s, t) if s.frame == frame(pair) => Some(*t),
                    _ => None,
                })
                .unwrap();
            if intervals_overlap((start_work, end_work), (start_render, end_render)) {
                intersects += 1;
            }
        }
        eprintln!("unforced overlap: {intersects}/{PAIRS} pairs");
        assert!(
            intersects * 100 >= PAIRS * 95,
            "{intersects}/{PAIRS} pairs overlapped"
        );
    });
}
