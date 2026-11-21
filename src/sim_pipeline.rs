//! Owner-side lag-1 protocol. The owner supplies tape, publication and presentation
//! callbacks. Only the job, state and reusable slot buffer cross to the worker.

use std::{
    panic::resume_unwind,
    sync::mpsc::{sync_channel, Receiver, SyncSender},
    thread::{self, JoinHandle},
    time::Instant,
};

pub const DEFAULT_STACK_SIZE: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub frame: u64,
    pub tape_step: u64,
    pub visit_id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub frame: Frame,
    /// Zero for serial frames. Worker generations advance on each use of a slot.
    pub slot_generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    Quit,
    EndOfTape,
}

pub enum Pulse<J> {
    Dispatchable(Frame, J),
    PreFenced(Frame, J),
    Exit(Exit),
}

pub enum Step<D> {
    Complete,
    /// The slot may contain the audio/operation prefix needed to resume the step.
    MidStep(D),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposition {
    Free,
    PostFenced,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Iteration {
    Continue,
    Stop(Exit),
}

#[derive(Debug, PartialEq, Eq)]
pub enum PipelineError<E> {
    Owner(E),
    Disconnected,
}

/// All methods run on the owner. A pulse includes tape advance, capture, apply,
/// prologue and head, up to the hop. Carry the latched sample in the returned job.
/// Any owner transaction from the serial continuation belongs in `serial`.
pub trait Owner<J, S, O, D> {
    type Error;

    fn pulse(&mut self, state: &mut S) -> Result<Pulse<J>, Self::Error>;

    /// Apply audio, publish, validate and decide the disposition with state home.
    /// Before using the output, check that the worker refilled it for this stamp.
    /// Debug-assert the captured stamp where the output type exposes one.
    /// A rejected capture's provisional seal must be replaced by the fenced path.
    fn classify(
        &mut self,
        state: &mut S,
        stamp: Stamp,
        output: &mut O,
    ) -> Result<Disposition, Self::Error>;

    /// Draw, finish the free tail and present under the supplied stamp. Keep any
    /// output borrow through skin completion. This path has no state access.
    async fn render_free(&mut self, stamp: Stamp, output: &mut O) -> Result<(), Self::Error>;

    /// Discard the capture for drawing and finish the live frame, including the
    /// fenced tail, owner seal and present.
    async fn post_fenced(
        &mut self,
        state: &mut S,
        stamp: Stamp,
        output: &mut O,
    ) -> Result<Iteration, Self::Error>;

    /// Apply the prefix, handle Due and resume on the owner as appropriate.
    async fn mid_step(
        &mut self,
        state: &mut S,
        stamp: Stamp,
        prefix: &mut O,
        due: D,
    ) -> Result<Iteration, Self::Error>;

    /// Re-enter the captured sample's clock/input view after any in-line flush.
    async fn serial(
        &mut self,
        state: &mut S,
        stamp: Stamp,
        job: J,
    ) -> Result<Iteration, Self::Error>;
}

struct Work<S, O> {
    stamp: Stamp,
    state: S,
    slot: usize,
    output: O,
}

struct Job<J, S, O> {
    input: J,
    work: Work<S, O>,
}

struct Done<S, O, D> {
    work: Work<S, O>,
    step: Step<D>,
}

pub struct BlockedTimes {
    pub submit_start: Instant,
    pub join_start: Instant,
    pub join_end: Instant,
}

pub struct Pipeline<J, S, O, D> {
    jobs: Option<SyncSender<Job<J, S, O>>>,
    done: Option<Receiver<Done<S, O, D>>>,
    worker: Option<JoinHandle<()>>,
    state: Option<S>,
    slots: [Option<O>; 2],
    generations: [u64; 2],
    in_flight: bool,
    terminal: bool,
}

impl<J: Send + 'static, S: Send + 'static, O: Send + 'static, D: Send + 'static>
    Pipeline<J, S, O, D>
{
    /// The worker resets or refills the supplied buffer for each step's stamp.
    /// The owner's `classify` checks freshness before using a complete output.
    /// Both buffers are returned to their own slots for reuse.
    pub fn new(
        state: S,
        slots: [O; 2],
        stack_size: Option<usize>,
        mut step: impl FnMut(Stamp, J, &mut S, &mut O) -> Step<D> + Send + 'static,
    ) -> std::io::Result<Self> {
        Self::new_with_reply_stamp(
            state,
            slots,
            stack_size,
            move |stamp, input, state, output| step(*stamp, input, state, output),
        )
    }

    pub(crate) fn new_with_reply_stamp(
        state: S,
        slots: [O; 2],
        stack_size: Option<usize>,
        step: impl FnMut(&mut Stamp, J, &mut S, &mut O) -> Step<D> + Send + 'static,
    ) -> std::io::Result<Self> {
        Self::spawn(state, slots, stack_size, 1, |jobs, done| {
            Self::work_loop(jobs, done, step);
        })
    }

    fn spawn(
        state: S,
        slots: [O; 2],
        stack_size: Option<usize>,
        done_capacity: usize,
        worker: impl FnOnce(Receiver<Job<J, S, O>>, SyncSender<Done<S, O, D>>) + Send + 'static,
    ) -> std::io::Result<Self> {
        let (jobs, job_rx) = sync_channel(1);
        let (done_tx, done) = sync_channel(done_capacity);
        let worker = thread::Builder::new()
            .name("sim-pipeline".into())
            .stack_size(stack_size.unwrap_or(DEFAULT_STACK_SIZE))
            .spawn(move || worker(job_rx, done_tx))?;
        Ok(Self {
            jobs: Some(jobs),
            done: Some(done),
            worker: Some(worker),
            state: Some(state),
            slots: slots.map(Some),
            generations: [0; 2],
            in_flight: false,
            terminal: false,
        })
    }

    fn work_loop(
        jobs: Receiver<Job<J, S, O>>,
        done: SyncSender<Done<S, O, D>>,
        mut step: impl FnMut(&mut Stamp, J, &mut S, &mut O) -> Step<D>,
    ) {
        while let Ok(Job { input, mut work }) = jobs.recv() {
            let step = step(&mut work.stamp, input, &mut work.state, &mut work.output);
            if done.send(Done { work, step }).is_err() {
                break;
            }
        }
    }

    /// One owner iteration. Entry blocks for the first job and may pulse a second
    /// time with its own tape sample. Continuing lag joins before pulsing. A
    /// pre-fence or exit flushes the held image before continuing or stopping.
    ///
    /// Stop, error or cancellation during an iteration is terminal. Drop the
    /// pipeline to release any work still in flight and join its thread, or call
    /// `shutdown` to inspect the worker's result explicitly.
    pub async fn iteration<A: Owner<J, S, O, D>>(
        &mut self,
        owner: &mut A,
    ) -> Result<Iteration, PipelineError<A::Error>> {
        assert!(!self.terminal, "iteration called after pipeline stopped");
        self.terminal = true;
        let result = self.run_iteration(owner).await;
        if matches!(result, Ok(Iteration::Continue)) {
            self.terminal = false;
        }
        result
    }

    async fn run_iteration<A: Owner<J, S, O, D>>(
        &mut self,
        owner: &mut A,
    ) -> Result<Iteration, PipelineError<A::Error>> {
        if !self.in_flight {
            match owner
                .pulse(self.state.as_mut().unwrap())
                .map_err(PipelineError::Owner)?
            {
                Pulse::Dispatchable(frame, job) => self.dispatch(0, frame, job)?,
                Pulse::PreFenced(frame, job) => return self.run_serial(owner, frame, job).await,
                Pulse::Exit(exit) => return Ok(Iteration::Stop(exit)),
            }
        }

        let Done { work, step } = self.join()?;
        let Work {
            stamp,
            state,
            slot,
            mut output,
        } = work;
        self.state = Some(state);
        let state = self.state.as_mut().unwrap();
        match step {
            Step::MidStep(due) => {
                let outcome = owner
                    .mid_step(state, stamp, &mut output, due)
                    .await
                    .map_err(PipelineError::Owner)?;
                self.slots[slot] = Some(output);
                return Ok(outcome);
            }
            Step::Complete => {
                let disposition = owner
                    .classify(state, stamp, &mut output)
                    .map_err(PipelineError::Owner)?;
                if disposition == Disposition::PostFenced {
                    let outcome = owner
                        .post_fenced(state, stamp, &mut output)
                        .await
                        .map_err(PipelineError::Owner)?;
                    self.slots[slot] = Some(output);
                    return Ok(outcome);
                }
            }
        }

        let pulse = owner.pulse(state).map_err(PipelineError::Owner)?;
        let after_flush = match pulse {
            Pulse::Dispatchable(frame, job) => {
                self.dispatch(1 - slot, frame, job)?;
                None
            }
            other => Some(other),
        };
        owner
            .render_free(stamp, &mut output)
            .await
            .map_err(PipelineError::Owner)?;
        self.slots[slot] = Some(output);
        match after_flush {
            Some(Pulse::PreFenced(frame, job)) => self.run_serial(owner, frame, job).await,
            Some(Pulse::Exit(exit)) => Ok(Iteration::Stop(exit)),
            None => Ok(Iteration::Continue),
            Some(Pulse::Dispatchable(..)) => unreachable!(),
        }
    }

    async fn run_serial<A: Owner<J, S, O, D>>(
        &mut self,
        owner: &mut A,
        frame: Frame,
        job: J,
    ) -> Result<Iteration, PipelineError<A::Error>> {
        let stamp = Stamp {
            frame,
            slot_generation: 0,
        };
        owner
            .serial(self.state.as_mut().unwrap(), stamp, job)
            .await
            .map_err(PipelineError::Owner)
    }

    /// The viewer retains the displayed output while the other slot is in flight.
    pub(crate) fn submit_viewer(
        &mut self,
        slot: usize,
        stamp: Stamp,
        input: J,
    ) -> Result<(), PipelineError<std::convert::Infallible>> {
        self.submit(slot, stamp, input)
    }

    pub(crate) fn join_viewer(
        &mut self,
    ) -> Result<(Stamp, &mut O), PipelineError<std::convert::Infallible>> {
        let Done { work, step } = self.join()?;
        assert!(matches!(step, Step::Complete));
        self.state = Some(work.state);
        self.slots[work.slot] = Some(work.output);
        Ok((work.stamp, self.slots[work.slot].as_mut().unwrap()))
    }

    /// W0 uses a single idle slot and returns state before the owner resumes.
    /// A continuation of the same frame uses the same capture stamp.
    pub fn run_blocked(
        &mut self,
        stamp: Stamp,
        input: J,
        timed: bool,
    ) -> Result<
        (Stamp, Step<D>, &mut O, Option<BlockedTimes>),
        PipelineError<std::convert::Infallible>,
    > {
        let submit_start = timed.then(Instant::now);
        self.submit(0, stamp, input)?;
        let join_start = timed.then(Instant::now);
        let Done { work, step } = self.join()?;
        let times = submit_start
            .zip(join_start)
            .map(|(submit_start, join_start)| BlockedTimes {
                submit_start,
                join_start,
                join_end: Instant::now(),
            });
        self.state = Some(work.state);
        self.slots[work.slot] = Some(work.output);
        Ok((
            work.stamp,
            step,
            self.slots[work.slot].as_mut().unwrap(),
            times,
        ))
    }

    fn dispatch<E>(&mut self, slot: usize, frame: Frame, input: J) -> Result<(), PipelineError<E>> {
        assert!(!self.in_flight);
        let generation = &mut self.generations[slot];
        *generation = generation
            .checked_add(1)
            .expect("slot generation exhausted");
        let stamp = Stamp {
            frame,
            slot_generation: *generation,
        };
        self.submit(slot, stamp, input)
    }

    fn submit<E>(&mut self, slot: usize, stamp: Stamp, input: J) -> Result<(), PipelineError<E>> {
        assert!(!self.in_flight);
        let work = Work {
            stamp,
            state: self.state.take().unwrap(),
            slot,
            output: self.slots[slot].take().unwrap(),
        };
        if self
            .jobs
            .as_ref()
            .unwrap()
            .send(Job { input, work })
            .is_err()
        {
            return Err(self.disconnected());
        }
        self.in_flight = true;
        Ok(())
    }

    fn join<E>(&mut self) -> Result<Done<S, O, D>, PipelineError<E>> {
        match self.done.as_ref().unwrap().recv() {
            Ok(done) => {
                self.in_flight = false;
                Ok(done)
            }
            Err(_) => Err(self.disconnected()),
        }
    }

    fn disconnected<E>(&mut self) -> PipelineError<E> {
        if let Err(panic) = self.close_and_join() {
            resume_unwind(panic);
        }
        PipelineError::Disconnected
    }
}

impl<J, S, O, D> Pipeline<J, S, O, D> {
    /// Release any work still in flight and join, returning a worker panic to
    /// the caller. Dropping without calling this method propagates the panic.
    pub fn shutdown(mut self) -> thread::Result<()> {
        self.close_and_join()
    }

    fn close_and_join(&mut self) -> thread::Result<()> {
        drop(self.jobs.take());
        // Release a pending send before joining, even with a rendezvous Done channel.
        drop(self.done.take());
        self.worker.take().map_or(Ok(()), JoinHandle::join)
    }
}

impl<J, S, O, D> Pipeline<J, S, O, D> {
    pub(crate) fn finish_worker(&mut self) {
        if let Err(panic) = self.close_and_join() {
            if !thread::panicking() {
                resume_unwind(panic);
            }
            // The worker's panic hook already reported the payload. Log the
            // shutdown failure while preserving the owner's active unwind.
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr().lock(),
                "sim-pipeline worker panicked during owner unwind"
            );
            // A custom panic payload can itself panic when dropped.
            std::mem::forget(panic);
        }
    }
}

impl<J, S, O, D> Drop for Pipeline<J, S, O, D> {
    fn drop(&mut self) {
        self.finish_worker();
    }
}

#[cfg(test)]
mod tests;
