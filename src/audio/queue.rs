//! Bounded single-producer/single-consumer transfer. Neither endpoint waits.
use std::{
    cell::{Cell, UnsafeCell},
    marker::PhantomData,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
struct Shared<T> {
    slots: Box<[UnsafeCell<Option<T>>]>,
    read: AtomicUsize,
    write: AtomicUsize,
}
// Each occupied slot belongs exclusively to the consumer until read advances;
// each empty slot belongs exclusively to the producer until write is published.
// The endpoint types are !Sync and are neither Clone nor copyable.
unsafe impl<T: Send> Sync for Shared<T> {}
unsafe impl<T: Send> Send for Shared<T> {}
pub struct Sender<T> {
    shared: Arc<Shared<T>>,
    unique: PhantomData<Cell<()>>,
}
pub struct Receiver<T> {
    shared: Arc<Shared<T>>,
    unique: PhantomData<Cell<()>>,
}
/// Requested storage for the existing allocation, including Arc's two counters.
/// Endpoint inline storage is charged by its owner; allocator bookkeeping excluded.
#[cfg(feature = "native_timing")]
pub const fn allocation_bytes<T>(capacity: usize) -> usize {
    std::mem::size_of::<Shared<T>>() + 2 * std::mem::size_of::<usize>()
        + capacity * std::mem::size_of::<Option<T>>()
}
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    assert!(capacity.is_power_of_two() && capacity < usize::MAX / 2);
    let shared = Arc::new(Shared {
        slots: (0..capacity).map(|_| UnsafeCell::new(None)).collect(),
        read: AtomicUsize::new(0),
        write: AtomicUsize::new(0),
    });
    (
        Sender {
            shared: shared.clone(),
            unique: PhantomData,
        },
        Receiver {
            shared,
            unique: PhantomData,
        },
    )
}
#[cfg(test)]
thread_local! {
    /// Test-only observer run at the start of every push on this thread.
    pub static PUSH_OBSERVER: std::cell::RefCell<Option<Box<dyn FnMut()>>> = std::cell::RefCell::new(None);
}
impl<T> Sender<T> {
    pub fn push(&self, value: T) -> Result<(), T> {
        #[cfg(test)]
        PUSH_OBSERVER.with(|o| if let Some(f) = o.borrow_mut().as_mut() { f() });
        let write = self.shared.write.load(Ordering::Relaxed);
        let read = self.shared.read.load(Ordering::Acquire);
        if write.wrapping_sub(read) >= self.shared.slots.len() {
            return Err(value);
        }
        unsafe {
            *self.shared.slots[write % self.shared.slots.len()].get() = Some(value);
        }
        self.shared
            .write
            .store(write.wrapping_add(1), Ordering::Release);
        Ok(())
    }
    pub fn len(&self) -> usize {
        self.shared
            .write
            .load(Ordering::Relaxed)
            .wrapping_sub(self.shared.read.load(Ordering::Acquire))
    }
}
impl<T> Receiver<T> {
    #[cfg(test)]
    pub fn has_pending(&self) -> bool {self.shared.write.load(Ordering::Acquire)!=self.shared.read.load(Ordering::Relaxed)}
    pub fn pop(&self) -> Option<T> {
        let read = self.shared.read.load(Ordering::Relaxed);
        if self.shared.write.load(Ordering::Acquire) == read {
            return None;
        }
        let value =
            unsafe { (&mut *self.shared.slots[read % self.shared.slots.len()].get()).take() };
        self.shared
            .read
            .store(read.wrapping_add(1), Ordering::Release);
        value
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_transfer_preserves_order_under_contention() {
        let (tx, rx) = channel(4);
        let producer = std::thread::spawn(move || {
            for n in 0..100_000 {
                let mut item = n;
                loop {
                    match tx.push(item) {
                        Ok(()) => break,
                        Err(v) => {
                            item = v;
                            std::thread::yield_now();
                        }
                    }
                }
            }
        });
        for expected in 0..100_000 {
            loop {
                if let Some(value) = rx.pop() {
                    assert_eq!(value, expected);
                    break;
                }
                std::thread::yield_now();
            }
        }
        producer.join().unwrap();
        assert!(rx.pop().is_none());
    }
    #[test]
    fn monotonic_indices_wrap_without_reusing_an_occupied_slot() {
        let (tx, rx) = channel(4);
        tx.shared.read.store(usize::MAX - 1, Ordering::Relaxed);
        tx.shared.write.store(usize::MAX - 1, Ordering::Relaxed);
        for n in 0..4 {
            tx.push(n).unwrap();
        }
        assert!(tx.push(9).is_err());
        for n in 0..4 {
            assert_eq!(rx.pop(), Some(n));
        }
        assert!(rx.pop().is_none());
    }
    #[test]
    fn full_queue_returns_ownership_and_drops_remaining_items_once() {
        let (tx, rx) = channel(1);
        tx.push(String::from("first")).unwrap();
        assert_eq!(tx.push(String::from("second")).unwrap_err(), "second");
        assert_eq!(rx.pop().unwrap(), "first");
        assert!(rx.pop().is_none());
    }
}
