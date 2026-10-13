//! Ownership fence for renderer handles. Worker jobs contain only borrowed CPU arrays.
use std::{marker::PhantomData, rc::Rc, sync::OnceLock, thread::{self, ThreadId}};

#[derive(Default)]
pub(crate) struct GameThread(PhantomData<Rc<()>>);
static OWNER: OnceLock<ThreadId> = OnceLock::new();
pub(crate) fn assert_current() {
    let current = thread::current().id();
    if let Some(owner) = OWNER.get() {
        debug_assert_eq!(*owner, current, "renderer called outside the game/GL thread");
    }
}
impl GameThread {
    pub(crate) fn new() -> Self { OWNER.get_or_init(|| thread::current().id()); assert_current(); Self::default() }
}
