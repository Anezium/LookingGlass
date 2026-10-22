//! Exact legacy report values shared with the optional frozen journal.
#[derive(Clone, Copy, Default)]
#[cfg_attr(feature = "native_timing", derive(serde::Serialize))]
pub struct ThreadPolicy {
    pub(crate) host: bool,
    pub(crate) id: i32,
    pub(crate) priority_requested: i32,
    pub(crate) priority_rc: i32,
    pub(crate) affinity_requested: i32,
    pub(crate) affinity_rc: i32,
    pub(crate) info_rc: i32,
    pub(crate) priority: i32,
    pub(crate) affinity: i32,
    pub(crate) stack: i32,
}
#[derive(Clone, Copy, Default)]
#[cfg_attr(feature = "native_timing", derive(serde::Serialize))]
pub struct Window {
    pub window_ms: u128,
    pub blocks: usize,
    pub mix: [f64; 3],
    pub starved: usize,
    /// Diagnostic subset; never subtracted from raw or legacy starvation.
    /// Console text only: preserve the frozen native journal's Window schema.
    #[cfg_attr(feature = "native_timing", serde(skip))]
    pub flush_silence: usize,
    pub rejected: usize,
    pub flushes: usize,
    pub queued: usize,
    pub api_max_us: u32,
    pub live_peak: usize,
    pub game_stage: (u32, u32),
    pub stages: Option<([[f64; 3]; 7], usize)>,
    pub voices: (usize, usize),
    /// Longest interval between producer loop wakeups, its 1 ms sleep
    /// overshoot, and wakeups more than one block apart.
    pub gap_max_ms: f64,
    pub sleep_over_ms: f64,
    pub gaps_over_block: u32,
    /// Time the producer spent printing the previous window (direct mode).
    pub log_ms: f64,
    pub dropped_reports: u32,
    pub music: Option<[f64; 6]>,
}
