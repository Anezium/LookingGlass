//! Ownership fence for renderer handles. Worker jobs contain only borrowed CPU arrays.
use std::{marker::PhantomData, rc::Rc, sync::OnceLock, thread::{self, ThreadId}};

#[derive(Default)]
pub(crate) struct GameThread(PhantomData<Rc<()>>);
static OWNER: OnceLock<ThreadId> = OnceLock::new();
pub(crate) fn assert_current() {
    assert_owned_by(OWNER.get());
}
#[inline(always)]
fn assert_owned_by(owner: Option<&ThreadId>) {
    // The thread lookup feeds only the debug assertion; release builds can skip it.
    if crate::tls_fast::on() && !cfg!(debug_assertions) {
        crate::tls_fast::skipped(crate::tls_fast::Lookup::ThreadCurrent);
        return;
    }
    crate::tls_fast::performed(crate::tls_fast::Lookup::ThreadCurrent);
    let current = thread::current().id();
    if let Some(owner) = owner {
        debug_assert_eq!(*owner, current, "renderer called outside the game/GL thread");
    }
}
impl GameThread {
    pub(crate) fn new() -> Self { OWNER.get_or_init(|| thread::current().id()); assert_current(); Self::default() }
}

// These tests never set the process-wide OWNER: each test runs on its own thread.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tls_fast::{take_test_counts, with_tls_fast_path, Lookup};

    fn thread_counts(run: impl FnOnce()) -> [u64; 2] {
        take_test_counts();
        run();
        take_test_counts()[Lookup::ThreadCurrent as usize]
    }

    #[test]
    fn release_fast_path_skips_only_the_thread_lookup() {
        let me = thread::current().id();
        for on in [false, true] {
            let counts = with_tls_fast_path(on, || thread_counts(|| {
                for _ in 0..3 { assert_owned_by(None); assert_owned_by(Some(&me)); }
            }));
            if on && !cfg!(debug_assertions) {
                assert_eq!(counts, [0, 6], "release ON skips every lookup");
            } else {
                assert_eq!(counts, [6, 0], "OFF, and debug builds, perform every lookup");
            }
        }
    }

    // The ownership check stays active in debug builds, with the flag on or off.
    #[cfg(debug_assertions)]
    #[test]
    fn debug_builds_keep_the_ownership_assertion() {
        let other = thread::spawn(|| thread::current().id()).join().unwrap();
        for on in [false, true] {
            with_tls_fast_path(on, || {
                assert_owned_by(Some(&thread::current().id()));
                let r = std::panic::catch_unwind(|| assert_owned_by(Some(&other)));
                assert!(r.is_err(), "flag {on}: a foreign owner must still fail the debug assertion");
            });
        }
    }

    // Release builds compile the assertion out; with the flag off they still perform the lookup.
    #[cfg(not(debug_assertions))]
    #[test]
    fn release_builds_never_assert_ownership() {
        let other = thread::spawn(|| thread::current().id()).join().unwrap();
        for on in [false, true] {
            let counts = with_tls_fast_path(on, || thread_counts(|| assert_owned_by(Some(&other))));
            assert_eq!(counts, if on { [0, 1] } else { [1, 0] });
        }
    }
}
