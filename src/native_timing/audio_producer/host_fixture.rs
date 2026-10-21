//! Host fixtures drive the real journal hooks and ownership queues. No hardware.
use crate::native_timing::{
    audio_producer::*,
    audio_types::ThreadPolicy,
    clock::{self, Checked, Source},
    transfer,
};
use std::{cell::Cell, rc::Rc, sync::Arc, time::Duration};
#[derive(Clone)]
pub struct Fake(pub Rc<Cell<Result<u64, clock::Error>>>);
impl Source for Fake {
    fn domain(&self) -> &'static str {
        "host-injected-fake-us"
    }
    fn read_us(&mut self) -> Result<u64, clock::Error> {
        self.0.get()
    }
}
pub fn setup() -> (Owner, Capture<Fake>, Fake, Arc<Bootstrap>) {
    let universe = Box::leak(Box::new(Universe::new()));
    let mut state = AudioState::mixer(universe);
    let zero = Stamp {
        pre_us: Some(0),
        post_us: Some(0),
        fault: None,
    };
    state.birth = Some(Birth {
        clock_id: "host-injected-fake-us",
        creation: zero,
        ..state.birth.unwrap()
    });
    let mut b = Bootstrap::new(&state);
    b.spawn = zero;
    b.entry
        .set(WorkerBirth {
            entry: zero,
            policy: ThreadPolicy {
                host: true,
                ..ThreadPolicy::default()
            },
            applied: true,
        })
        .ok()
        .unwrap();
    let bootstrap = Arc::new(b);
    let fake = Fake(Rc::new(Cell::new(Ok(0))));
    let mut clock = Checked::new(fake.clone());
    let (tx, rx) = transfer::channel(1);
    let owner = Owner::prepare(bootstrap.clone(), &tx, &mut clock, Some([7; 16]));
    assert_eq!(owner.status, "Pending");
    (
        owner,
        Capture::new(fake.clone(), bootstrap.clone(), rx),
        fake,
        bootstrap,
    )
}
pub fn filled(blocks: usize, fences: usize, legacy: usize) -> Owner {
    assert!(blocks < BLOCK_CAP && fences < FENCE_CAP && (legacy == fences - 1 || legacy == fences));
    let (mut owner, mut journal, fake, _) = setup();
    owner.request_start(Some(1));
    fake.0.set(Ok(2));
    journal.poll();
    for i in 0..blocks {
        fake.0.set(Ok(3 + i as u64 * 100));
        let before = journal.block_begin();
        fake.0.set(Ok(4 + i as u64 * 100));
        journal.block_end(before, 6.0 + (i % 5) as f64 / 10.);
        if (i + 1) * legacy / blocks > i * legacy / blocks {
            fake.0.set(Ok(5 + i as u64 * 100));
            let cut = journal.legacy_begin();
            fake.0.set(Ok(5 + i as u64 * 100));
            journal.legacy_end(
                cut,
                crate::native_timing::audio_types::Window::default(),
                true,
            );
        }
    }
    if legacy != fences {
        fake.0.set(Ok(10 + blocks as u64 * 100));
    }
    let mut c = Checked::new(fake.clone());
    owner.request_seal(&mut c);
    journal.poll();
    owner.collect(Duration::ZERO);
    owner.freeze();
    assert_eq!(owner.status, "Delivered");
    owner
}
