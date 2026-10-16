//! Persistent math workers: fixed application masks, explicit stacks, no renderer imports.
pub(crate) const STACK_BYTES: usize = 512 * 1024;
pub(crate) fn mask(worker: usize) -> i32 {
    [0x20000, 0x40000][worker]
}

#[cfg(target_os = "vita")]
#[repr(C)]
struct ThreadInfo {
    size: u32,
    process: i32,
    name: [u8; 32],
    attr: u32,
    status: u32,
    entry: u32,
    stack: u32,
    stack_size: i32,
    init_priority: i32,
    priority: i32,
    init_affinity: i32,
    affinity: i32,
    cpu: i32,
    last_cpu: i32,
    wait_type: u32,
    wait_id: i32,
    exit_status: i32,
    run_clocks: u64,
    intr_preempt: u32,
    preempt: u32,
    release: u32,
    changes: i32,
    notify: i32,
    reserved: i32,
}
#[cfg(target_os = "vita")]
const _: [(); 0x80] = [(); std::mem::size_of::<ThreadInfo>()];
#[cfg(target_os = "vita")]
extern "C" {
    fn sceKernelGetThreadId() -> i32;
    fn sceKernelChangeThreadCpuAffinityMask(id: i32, mask: i32) -> i32;
    fn sceKernelGetThreadInfo(id: i32, info: *mut ThreadInfo) -> i32;
    fn sceKernelGetCpuId() -> i32;
}
/// Kernel readback is deliberately used instead of pthread_getaffinity_np.
pub(crate) fn pin(worker: usize, probe: bool) {
    #[cfg(target_os = "vita")]
    unsafe {
        let id = sceKernelGetThreadId();
        let requested = mask(worker);
        let rc = sceKernelChangeThreadCpuAffinityMask(id, requested);
        if probe {
            eprintln!(
                "[lg/worker-probe] worker={worker} affinity_rc={rc} requested={requested:#x}"
            );
            inventory("worker", Some(requested));
        }
        assert!(rc >= 0, "skin worker affinity failed: {rc:#x}");
    }
    #[cfg(not(target_os = "vita"))]
    if probe {
        eprintln!(
            "[lg/worker-probe] worker={worker} requested={:#x} host_affinity=unavailable",
            mask(worker)
        );
    }
}
/// Reserve CPU0 for the game thread when both remaining application cores skin.
pub(crate) fn pin_game(probe: bool) {
    #[cfg(target_os = "vita")]
    unsafe {
        let rc = sceKernelChangeThreadCpuAffinityMask(sceKernelGetThreadId(), 0x10000);
        assert!(rc >= 0, "game affinity failed: {rc:#x}");
        if probe { inventory("game-overlap", Some(0x10000)); }
    }
    #[cfg(not(target_os = "vita"))]
    let _ = probe;
}
pub(crate) fn inventory(role: &str, requested: Option<i32>) {
    #[cfg(target_os = "vita")]
    unsafe {
        assert_eq!(std::mem::size_of::<ThreadInfo>(), 0x80);
        let id = sceKernelGetThreadId();
        let mut info: ThreadInfo = std::mem::zeroed();
        info.size = std::mem::size_of::<ThreadInfo>() as u32;
        let rc = sceKernelGetThreadInfo(id, &mut info);
        let mut histogram = [0u32; 4];
        let start = std::time::Instant::now();
        let mut value = 1u32;
        while start.elapsed() < std::time::Duration::from_millis(8) {
            for _ in 0..64 {
                value = std::hint::black_box(value.wrapping_mul(1664525).wrapping_add(1013904223));
            }
            let cpu = sceKernelGetCpuId();
            if let Some(bin) = histogram.get_mut(cpu as usize) {
                *bin += 1;
            }
        }
        eprintln!("[lg/worker-probe] role={role} id={id} requested={requested:?} info_rc={rc} effective={:#x} initial={:#x} priority={} cpu_histogram={histogram:?} sample_ms=8", info.affinity, info.init_affinity, info.priority);
    }
    #[cfg(not(target_os = "vita"))]
    let _ = (role, requested);
}

use std::sync::{Arc, Condvar, Mutex};
#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct Measurement {
    pub ns: u64,
    pub malloc_calls: u64,
}
type Work = (usize, unsafe fn(usize));
#[derive(Default)]
struct State {
    work: Option<Work>,
    busy: bool,
    stop: bool,
    result: Measurement,
    panic: Option<Box<dyn std::any::Any + Send>>,
}
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    done: Condvar,
}
/// One sleeping thread, fixed stack and affinity. Only CPU closures cross this boundary.
/// Condvar uses a pthread condition/event wake on Vita; no per-dispatch allocation.
pub(crate) struct Worker {
    shared: Arc<Shared>,
    handle: Option<std::thread::JoinHandle<()>>,
}
impl Worker {
    pub(crate) fn new(index: usize, probe: bool) -> Self {
        Self::create(index, probe, false)
    }
    pub(crate) fn measured(index: usize, probe: bool) -> Self {
        Self::create(index, probe, true)
    }
    fn create(index: usize, probe: bool, measured: bool) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            wake: Condvar::new(),
            done: Condvar::new(),
        });
        let other = shared.clone();
        let handle = std::thread::Builder::new()
            .name(format!("skin-persistent-{index}"))
            .stack_size(STACK_BYTES)
            .spawn(move || {
                let pin_result = std::panic::catch_unwind(|| pin(index, probe));
                // Ready acknowledgement proves pinning finished before the first dispatch.
                let mut s = other.state.lock().unwrap();
                s.result.ns = 1;
                s.panic = pin_result.err();
                other.done.notify_one();
                if s.panic.is_some() {
                    return;
                }
                loop {
                    while s.work.is_none() && !s.stop {
                        s = other.wake.wait(s).unwrap();
                    }
                    if s.stop {
                        break;
                    }
                    let (address, call) = s.work.take().unwrap();
                    drop(s);
                    let allocations = measured.then(crate::kernel_alloc::Scope::begin);
                    let start = std::time::Instant::now();
                    let result =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                            call(address)
                        }));
                    let ns = start.elapsed().as_nanos() as u64;
                    let malloc_calls = allocations.map_or(0, crate::kernel_alloc::Scope::finish);
                    s = other.state.lock().unwrap();
                    s.result = Measurement { ns, malloc_calls };
                    s.panic = result.err();
                    s.busy = false;
                    other.done.notify_one();
                }
            })
            .expect("start persistent skin worker");
        let mut worker = Self {
            shared,
            handle: Some(handle),
        };
        let mut s = worker.shared.state.lock().unwrap();
        while s.result.ns == 0 {
            s = worker.shared.done.wait(s).unwrap();
        }
        drop(s);
        if let Err(panic) = worker.wait_result() {
            std::panic::resume_unwind(panic);
        }
        worker
    }
    /// Caller owns stable Send CPU storage until wait completes, even during unwinding.
    /// No references to Mesh, texture, renderer or GL state may be supplied.
    pub(crate) unsafe fn start(&mut self, address: usize, call: unsafe fn(usize)) {
        let mut s = self.shared.state.lock().unwrap();
        assert!(!s.busy && !s.stop);
        s.busy = true;
        s.work = Some((address, call));
        self.shared.wake.notify_one();
    }
    pub(crate) fn wait_result(&mut self) -> Result<Measurement, Box<dyn std::any::Any + Send>> {
        let mut s = self.shared.state.lock().unwrap();
        while s.busy {
            s = self.shared.done.wait(s).unwrap();
        }
        let result = s.result;
        let panic = s.panic.take();
        drop(s);
        if let Some(panic) = panic {
            Err(panic)
        } else {
            Ok(result)
        }
    }
    pub(crate) fn wait(&mut self) -> Measurement {
        match self.wait_result() {
            Ok(value) => value,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
    pub(crate) fn pair<F: FnOnce() + Send, G: FnOnce() + Send>(
        a: &mut Self,
        b: &mut Self,
        first: F,
        second: G,
    ) -> [Measurement; 2] {
        unsafe fn call<F: FnOnce()>(address: usize) {
            (&mut *(address as *mut Option<F>)).take().unwrap()();
        }
        for worker in [&*a, &*b] {
            let s = worker.shared.state.lock().unwrap();
            assert!(!s.busy && !s.stop);
        }
        let mut first = Some(first);
        let mut second = Some(second);
        // Both closures remain alive until BOTH workers complete, including panic.
        unsafe {
            a.start(&mut first as *mut Option<F> as usize, call::<F>);
            b.start(&mut second as *mut Option<G> as usize, call::<G>);
        }
        let ra = a.wait_result();
        let rb = b.wait_result();
        match (ra, rb) {
            (Ok(a), Ok(b)) => [a, b],
            (Err(p), _) | (_, Err(p)) => std::panic::resume_unwind(p),
        }
    }
    /// Scoped lending to a persistent thread. The closure stays on this stack until
    /// completion; worker panics are propagated only after all borrowed access ends.
    pub(crate) fn execute(&mut self, work: impl FnOnce() + Send) -> Measurement {
        unsafe fn call<F: FnOnce()>(address: usize) {
            let work = &mut *(address as *mut Option<F>);
            work.take().unwrap()();
        }
        fn dispatch<F: FnOnce() + Send>(worker: &mut Worker, work: F) -> Measurement {
            let mut work = Some(work);
            unsafe {
                worker.start(&mut work as *mut Option<F> as usize, call::<F>);
            }
            worker.wait()
        }
        dispatch(self, work)
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        // Drain before teardown, including an asynchronous bench job.
        let mut s = self.shared.state.lock().unwrap();
        while s.busy {
            s = self.shared.done.wait(s).unwrap();
        }
        s.stop = true;
        self.shared.wake.notify_one();
        drop(s);
        if let Some(handle) = self.handle.take() {
            handle.join().expect("persistent worker teardown");
        }
    }
}
#[cfg(test)]
mod persistent_tests {
    use super::*;
    #[test]
    fn start_reuse_borrow_panic_recover_and_stop() {
        let mut worker = Worker::new(0, false);
        let id = worker.handle.as_ref().unwrap().thread().id();
        let mut values = [0u32; 128];
        for i in 0..20 {
            worker.execute(|| {
                values.fill(i);
                assert_eq!(std::thread::current().id(), id);
            });
            assert_eq!(values, [i; 128]);
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            worker.execute(|| panic!("test panic"))
        }));
        assert!(result.is_err());
        worker.execute(|| values.fill(99));
        assert_eq!(values, [99; 128]);
        drop(worker);
    }
    #[test]
    fn pair_panic_joins_both_borrowers_before_unwinding() {
        let mut a = Worker::new(0, false);
        let mut b = Worker::new(1, false);
        let barrier = std::sync::Barrier::new(2);
        let mut output = 0;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            Worker::pair(
                &mut a,
                &mut b,
                || {
                    barrier.wait();
                    panic!("pair panic");
                },
                || {
                    barrier.wait();
                    output = 42;
                },
            );
        }));
        assert!(result.is_err());
        assert_eq!(output, 42);
        Worker::pair(
            &mut a,
            &mut b,
            || {},
            || {
                output = 43;
            },
        );
        assert_eq!(output, 43);
    }
    #[test]
    fn stop_drains_pending_work() {
        use std::sync::atomic::{AtomicBool, Ordering};
        unsafe fn work(address: usize) {
            let flag = &*(address as *const AtomicBool);
            flag.store(true, Ordering::Release);
        }
        let flag = Box::new(AtomicBool::new(false));
        let mut worker = Worker::new(0, false);
        unsafe {
            worker.start(&*flag as *const AtomicBool as usize, work);
        }
        drop(worker);
        assert!(flag.load(Ordering::Acquire));
    }
}

/// Kernel running time is in microseconds, independent of CPU frequency.
#[cfg(target_os = "vita")]
pub(crate) fn current_runtime() -> Result<(i32, u64, i32, i32), i32> {
    unsafe {
        let id = sceKernelGetThreadId();
        let mut info: ThreadInfo = std::mem::zeroed();
        info.size = std::mem::size_of::<ThreadInfo>() as u32;
        let rc = sceKernelGetThreadInfo(id, &mut info);
        if rc < 0 { Err(rc) } else { Ok((id, info.run_clocks, info.affinity, info.priority)) }
    }
}
