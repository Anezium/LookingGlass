//! One clock domain for all current/future instrumentation users. Never UTC.
//! Vita: newlib clock_gettime(4), process-lifetime ProcessTimeWide us (existing
//! ELF evidence only). Host: one OnceLock<Instant> session-relative origin.
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Error {
    ReadFailed,
    InvalidTimespec,
    Backward,
    Overflow,
}

pub trait Source {
    fn read_us(&mut self) -> Result<u64, Error>;
}
#[derive(Default)]
pub struct Native;
#[cfg(target_os = "vita")]
pub const DOMAIN: &str = "vita-newlib-clock4-process-us";
#[cfg(not(target_os = "vita"))]
pub const DOMAIN: &str = "host-shared-Instant-session-us";

// SDK time.h/sys/_types.h: clockid_t unsigned long, time_t long with
// _USE_LONG_TIME_T on Vita; timespec is two 32-bit longs. No Linux ABI constants.
#[cfg(target_os = "vita")]
#[repr(C)]
struct Timespec {
    sec: i32,
    nsec: i32,
}
#[cfg(target_os = "vita")]
extern "C" {
    fn clock_gettime(id: u32, out: *mut Timespec) -> i32;
}
#[cfg(target_os = "vita")]
const _: () = assert!(std::mem::size_of::<Timespec>() == 8);

pub fn timespec_us(sec: i64, nsec: i64) -> Result<u64, Error> {
    if sec < 0 || !(0..1_000_000_000).contains(&nsec) {
        return Err(Error::InvalidTimespec);
    }
    (sec as u64)
        .checked_mul(1_000_000)
        .and_then(|v| v.checked_add(nsec as u64 / 1000))
        .ok_or(Error::Overflow)
}

/// Stateless common domain adapter; each stream MUST use Checked to detect
/// backwards readings against its preceding observation. No fallback/reset.
pub fn now_us() -> Result<u64, Error> {
    #[cfg(target_os = "vita")]
    {
        let mut ts = Timespec { sec: 0, nsec: 0 };
        if unsafe { clock_gettime(4, &mut ts) } != 0 {
            return Err(Error::ReadFailed);
        }
        timespec_us(ts.sec as i64, ts.nsec as i64)
    }
    #[cfg(not(target_os = "vita"))]
    {
        use std::{sync::OnceLock, time::Instant};
        static ORIGIN: OnceLock<Instant> = OnceLock::new();
        let origin = ORIGIN.get_or_init(Instant::now);
        let d = Instant::now()
            .checked_duration_since(*origin)
            .ok_or(Error::Backward)?;
        u64::try_from(d.as_micros()).map_err(|_| Error::Overflow)
    }
}
impl Source for Native {
    fn read_us(&mut self) -> Result<u64, Error> {
        now_us()
    }
}
pub struct Checked<C> {
    pub source: C,
    pub last: Option<u64>,
    pub fault: Option<Error>,
}
impl<C: Source> Checked<C> {
    pub fn new(source: C) -> Self {
        Self {
            source,
            last: None,
            fault: None,
        }
    }
    pub fn read(&mut self) -> Option<u64> {
        if self.fault.is_some() {
            return None;
        }
        match self.source.read_us() {
            Ok(v) if self.last.is_none_or(|old| v >= old) => {
                self.last = Some(v);
                Some(v)
            }
            Ok(_) => {
                self.fault = Some(Error::Backward);
                None
            }
            Err(e) => {
                self.fault = Some(e);
                None
            }
        }
    }
}
