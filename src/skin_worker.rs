//! Scoped math workers: fixed application masks, explicit stacks, no renderer imports.
pub(crate) const STACK_BYTES: usize = 512 * 1024;
pub(crate) fn mask(worker: usize) -> i32 { [0x20000, 0x40000][worker] }

#[cfg(target_os = "vita")]
#[repr(C)]
struct ThreadInfo {
    size: u32, process: i32, name: [u8; 32], attr: u32, status: u32,
    entry: u32, stack: u32, stack_size: i32, init_priority: i32, priority: i32,
    init_affinity: i32, affinity: i32, cpu: i32, last_cpu: i32, wait_type: u32,
    wait_id: i32, exit_status: i32, run_clocks: u64, intr_preempt: u32,
    preempt: u32, release: u32, changes: i32, notify: i32, reserved: i32,
}
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
        assert!(rc >= 0, "skin worker affinity failed: {rc:#x}");
        if probe { inventory("worker", Some(requested)); }
    }
    #[cfg(not(target_os = "vita"))]
    if probe { eprintln!("[lg/worker-probe] worker={worker} requested={:#x} host_affinity=unavailable", mask(worker)); }
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
            for _ in 0..64 { value = std::hint::black_box(value.wrapping_mul(1664525).wrapping_add(1013904223)); }
            let cpu = sceKernelGetCpuId();
            if let Some(bin) = histogram.get_mut(cpu as usize) { *bin += 1; }
        }
        eprintln!("[lg/worker-probe] role={role} id={id} requested={requested:?} info_rc={rc} effective={:#x} initial={:#x} priority={} cpu_histogram={histogram:?} sample_ms=8", info.affinity, info.init_affinity, info.priority);
    }
    #[cfg(not(target_os = "vita"))]
    let _ = (role, requested);
}
pub(crate) fn startup(workers: usize, probe: bool) {
    if !probe { return; }
    inventory("game", None);
    // Scoped implementation has no persistent pool. Probe the exact worker policy,
    // including the otherwise-disabled A/B arm, before backend initialization.
    std::thread::scope(|scope| {
        for worker in 0..workers.max(1) {
            std::thread::Builder::new().name(format!("skin-probe-{worker}"))
                .stack_size(STACK_BYTES).spawn_scoped(scope, move || pin(worker, true))
                .expect("start skin probe");
        }
    });
}
