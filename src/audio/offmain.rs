//! Default-off immutable acoustic work on the existing PCM producer.
//! One reusable bounded packet crosses SPSC queues. PCM demand wins before
//! every trace; no game World, decoder, sink or lazy collider leaves main.
use super::queue;
use crate::collision::{StaticGeometry, Trace, World};
use macroquad::prelude::Vec3;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
const CAP: usize = 256;
#[derive(Clone, Copy, Default)]
struct Query {
    start: Vec3,
    end: Vec3,
}
impl Query {
    fn same(self, other: Self) -> bool {
        self.start.to_array().map(f32::to_bits) == other.start.to_array().map(f32::to_bits)
            && self.end.to_array().map(f32::to_bits) == other.end.to_array().map(f32::to_bits)
    }
}
struct Batch {
    geometry: Option<Arc<StaticGeometry>>,
    queries: [Query; CAP],
    results: [Trace; CAP],
    len: usize,
    valid: bool,
}
#[derive(Default)]
struct Stats {
    traces: AtomicU64,
    trace_max_us: AtomicU64,
}
pub(super) struct Client {
    tx: queue::Sender<Box<Batch>>,
    rx: queue::Receiver<Box<Batch>>,
    batch: Option<Box<Batch>>,
    geometry: Option<Arc<StaticGeometry>>,
    alive: Arc<AtomicBool>,
    stats: Arc<Stats>,
    count: usize,
    read: usize,
    dirty: bool,
    ready: bool,
    pub(super) submitted: u64,
    pub(super) reused: u64,
    pub(super) fallback: u64,
    wait_max_us: u128,
}
pub(super) struct Server {
    rx: queue::Receiver<Box<Batch>>,
    tx: queue::Sender<Box<Batch>>,
    batch: Option<Box<Batch>>,
    at: usize,
    alive: Arc<AtomicBool>,
    stats: Arc<Stats>,
}
pub(super) fn channel() -> (Client, Server) {
    let (tx, rx_work) = queue::channel(1);
    let (tx_done, rx) = queue::channel(1);
    let alive = Arc::new(AtomicBool::new(true));
    let stats = Arc::new(Stats::default());
    (
        Client {
            tx,
            rx,
            batch: Some(Box::new(Batch {
                geometry: None,
                queries: [Query::default(); CAP],
                results: [Trace::default(); CAP],
                len: 0,
                valid: false,
            })),
            geometry: None,
            alive: alive.clone(),
            stats: stats.clone(),
            count: 0,
            read: 0,
            dirty: true,
            ready: false,
            submitted: 0,
            reused: 0,
            fallback: 0,
            wait_max_us: 0,
        },
        Server {
            rx: rx_work,
            tx: tx_done,
            batch: None,
            at: 0,
            alive,
            stats,
        },
    )
}
impl Client {
    pub(super) fn reset(&mut self) {
        self.geometry = None;
        if self.batch.is_none() {
            self.batch = self.rx.pop();
        }
        if let Some(b) = &mut self.batch {
            b.geometry = None;
            b.valid = false;
        }
        self.ready = false;
    }
    pub(super) fn begin(&mut self, world: &World) {
        self.ready = false;
        self.read = 0;
        self.count = 0;
        self.dirty = false;
        if self.batch.is_none() {
            self.batch = self.rx.pop();
        }
        if let Some(b) = &mut self.batch {
            let new_scene = self.geometry.is_none();
            let geometry = self
                .geometry
                .get_or_insert_with(|| world.audio_geometry());
            if new_scene {
                self.dirty = true;
                b.valid = false;
            }
            b.geometry = Some(geometry.clone());
        }
    }
    pub(super) fn query(&mut self, start: Vec3, end: Vec3) -> Trace {
        if let Some(b) = &mut self.batch {
            if self.count < CAP {
                let q = Query { start, end };
                let changed = !q.same(b.queries[self.count]);
                self.dirty |= changed;
                b.valid &= !changed;
                b.queries[self.count] = q;
            }
        }
        self.count += 1;
        Trace::default() // Collection only; no control values are published here.
    }
    pub(super) fn execute(&mut self) {
        // Preserve the last nonempty packet between the 10 Hz room updates.
        // A frame with no traces must not evict useful exact endpoint results.
        if self.count == 0 {
            self.ready = true;
            return;
        }
        let Some(b) = &mut self.batch else {
            self.fallback += 1;
            return;
        };
        if self.count > CAP || !self.alive.load(Ordering::Acquire) {
            self.fallback += 1;
            self.dirty = true;
            return;
        }
        self.dirty |= b.len != self.count || !b.valid;
        b.len = self.count;
        if !self.dirty {
            self.ready = true;
            self.reused += 1;
            return;
        }
        let b = self.batch.take().unwrap();
        if let Err(b) = self.tx.push(b) {
            self.batch = Some(b);
            self.fallback += 1;
            return;
        }
        self.submitted += 1;
        let start = Instant::now();
        loop {
            if let Some(b) = self.rx.pop() {
                self.batch = Some(b);
                self.ready = true;
                break;
            }
            if start.elapsed() >= Duration::from_millis(100) || !self.alive.load(Ordering::Acquire)
            {
                self.fallback += 1;
                break;
            }
            std::thread::sleep(Duration::from_micros(100));
        }
        self.wait_max_us = self.wait_max_us.max(start.elapsed().as_micros());
    }
    pub(super) fn complete(&mut self, world: &World, start: Vec3, end: Vec3) -> Trace {
        if !self.ready {
            return world.sweep(start, end, Vec3::ZERO);
        }
        let b = self.batch.as_ref().unwrap();
        let q = Query { start, end };
        assert!(
            self.read < b.len && q.same(b.queries[self.read]),
            "acoustic packet order changed"
        );
        let result = world.audio_complete(b.results[self.read], start, end);
        self.read += 1;
        result
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        eprintln!("[lg/audio-offmain] submitted={} reused={} fallback={} static_traces={} trace_max_us={} wait_max_us={} capacity={CAP}",
            self.submitted,self.reused,self.fallback,self.stats.traces.load(Ordering::Relaxed),self.stats.trace_max_us.load(Ordering::Relaxed),self.wait_max_us);
    }
}
impl Server {
    /// Called only when the producer's PCM cushion is full. The next producer
    /// iteration rechecks PCM occupancy even if this packet is unfinished.
    pub(super) fn step(&mut self) -> bool {
        if self.batch.is_none() {
            self.batch = self.rx.pop();
            self.at = 0;
        }
        let Some(b) = &mut self.batch else {
            return false;
        };
        if self.at < b.len {
            let q = b.queries[self.at];
            let start = Instant::now();
            b.results[self.at] = b.geometry.as_ref().unwrap().sweep(q.start, q.end);
            self.stats
                .trace_max_us
                .fetch_max(start.elapsed().as_micros() as u64, Ordering::Relaxed);
            self.stats.traces.fetch_add(1, Ordering::Relaxed);
            self.at += 1;
        }
        if self.at == b.len {
            b.valid = true;
            // Results remain reusable, but an idle/late returned packet must not
            // retain an outgoing map. Main owns the scene identity and reattaches
            // its shared backing before submitting another packet.
            b.geometry = None;
            // One in-flight packet; client cannot submit again before reclaiming.
            assert!(self.tx.push(self.batch.take().unwrap()).is_ok());
        }
        true
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use macroquad::prelude::vec3;
    fn test_box(center: Vec3, half: Vec3) -> crate::collision::Collider {
        crate::collision::Collider::box_bounds(center - half, center + half)
    }
    fn bits(t: Trace) -> (u32, [u32; 3], bool, bool) {
        (
            t.fraction.to_bits(),
            t.normal.to_array().map(f32::to_bits),
            t.start_solid,
            t.all_solid,
        )
    }
    #[test]
    fn immutable_part_plus_live_geometry_is_exact() {
        let mut world = World::fixture(&[(vec3(-5., -50., -50.), vec3(5., 50., 50.))]);
        let geometry = world.audio_geometry();
        world.set_settled_supports(vec![test_box(vec3(-30., 80., 0.), vec3(8., 8., 8.))]);
        world.set_weapon_obstacles(vec![crate::combat::Target {
            id: 7,
            center: vec3(40., 40., 0.),
            half: vec3(12., 12., 12.),
        }]);
        for offset in [-100., 0., 100.] {
            world.set_dynamic(vec![test_box(vec3(offset, 70., 0.), vec3(8., 8., 8.))]);
            for i in 0..100 {
                let a = vec3(-150., i as f32 * 2. - 60., 10.);
                let b = vec3(150., 80. - i as f32, 0.);
                assert_eq!(
                    bits(world.sweep(a, b, Vec3::ZERO)),
                    bits(world.audio_complete(geometry.sweep(a, b), a, b))
                );
            }
        }
    }
    #[test]
    fn producer_trace_step_has_no_allocations() {
        let world = World::fixture(&[(Vec3::splat(-5.), Vec3::splat(5.))]);
        let (mut client, mut server) = channel();
        for i in 0..64 {
            let a = Vec3::X * -20.;
            let b = Vec3::new(20., i as f32, 0.);
            client.begin(&world);
            client.query(a, b);
            let mut packet = client.batch.take().unwrap();
            packet.len = 1;
            assert!(client.tx.push(packet).is_ok());
            let scope = crate::kernel_alloc::Scope::begin();
            assert!(server.step());
            assert_eq!(scope.finish(), 0);
            client.batch = client.rx.pop();
            client.ready = true;
            assert_eq!(
                bits(client.complete(&world, a, b)),
                bits(world.sweep(a, b, Vec3::ZERO))
            );
        }
    }
    #[test]
    fn bounded_packet_reuse_overflow_timeout_and_late_return() {
        let world = World::fixture(&[(Vec3::splat(-5.), Vec3::splat(5.))]);
        let (mut client, mut server) = channel();
        let a = vec3(-20., 0., 0.);
        let b = vec3(20., 0., 0.);
        // No producer servicing: bounded timeout uses the original same-frame trace.
        client.begin(&world);
        client.query(a, b);
        client.execute();
        assert!(!client.ready && client.batch.is_none());
        assert_eq!(
            bits(client.complete(&world, a, b)),
            bits(world.sweep(a, b, Vec3::ZERO))
        );
        assert!(server.step());
        client.reset();
        let world = World::fixture(&[]);
        // A late packet is reclaimed; endpoint changes invalidate its old results.
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let worker = std::thread::spawn(move || {
            while !done.load(Ordering::Acquire) {
                if !server.step() {
                    std::thread::yield_now();
                }
            }
        });
        for i in 0..20 {
            let end = b + Vec3::Y * i as f32;
            client.begin(&world);
            client.query(a, end);
            client.execute();
            assert!(client.ready);
            assert_eq!(
                bits(client.complete(&world, a, end)),
                bits(world.sweep(a, end, Vec3::ZERO))
            );
            let scope = crate::kernel_alloc::Scope::begin();
            client.begin(&world);
            client.query(a, end);
            client.execute();
            client.complete(&world, a, end);
            assert_eq!(scope.finish(), 0);
        }
        client.begin(&world);
        for _ in 0..CAP + 1 {
            client.query(a, b);
        }
        client.execute();
        assert!(!client.ready);
        assert_eq!(
            bits(client.complete(&world, a, b)),
            bits(world.sweep(a, b, Vec3::ZERO))
        );
        client.reset();
        client.begin(&world);
        client.query(a, b);
        client.execute();
        assert!(client.ready);
        stop.store(true, Ordering::Release);
        worker.join().unwrap();
    }
}

#[cfg(test)]
mod lifetime_tests {
    use super::*;
    #[test]
    fn reset_and_late_return_release_outgoing_map_without_allocating() {
        let world = World::fixture(&[(Vec3::splat(-5.), Vec3::splat(5.))]);
        let backing = world.audio_geometry();
        let weak = Arc::downgrade(&backing);
        let (mut client, mut server) = channel();
        client.begin(&world);
        assert!(Arc::ptr_eq(client.geometry.as_ref().unwrap(), &backing));
        assert!(Arc::ptr_eq(client.batch.as_ref().unwrap().geometry.as_ref().unwrap(), &backing));
        drop(backing);
        let mut packet = client.batch.take().unwrap();
        packet.len = 2;
        assert!(client.tx.push(packet).is_ok());
        assert!(server.step()); // Reset while the producer still needs the map.
        client.reset();
        drop(world);
        assert!(weak.upgrade().is_some());
        assert!(server.step()); // Late return contains results, no map handle.
        assert!(weak.upgrade().is_none());
        client.reset();
        assert!(client.batch.as_ref().unwrap().geometry.is_none());
        assert!(!client.batch.as_ref().unwrap().valid);
        let world = World::fixture(&[]);
        let scope = crate::kernel_alloc::Scope::begin();
        client.begin(&world);
        assert_eq!(scope.finish(), 0);
        client.reset();
        assert_eq!(Arc::strong_count(&world.audio_geometry()), 2);
    }
}
