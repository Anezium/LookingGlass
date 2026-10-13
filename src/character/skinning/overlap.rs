//! Same-frame CPU skin snapshots. Render handles never enter this storage.
//! Buffers are bounded and allocated at startup; overflow uses the serial draw.
use super::{rotation, shade_arrays, Params};
use crate::skeletal::{Skeleton, SkinSurface, Transform};
use macroquad::prelude::*;
use std::{
    cell::{Cell, RefCell, UnsafeCell},
    ops::Range,
    sync::Arc,
};

const DRAWS: usize = 128;
const SURFACES: usize = 2048;
const BONES: usize = 8192;
const VERTICES: usize = 32768;
#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Idle,
    Gather,
    Running,
    Ready,
}
thread_local! {
    static PHASE: Cell<Phase> = const { Cell::new(Phase::Idle) };
    static STORE: RefCell<Option<Box<Store>>> = const { RefCell::new(None) };
}
pub(crate) fn gathering() -> bool {
    PHASE.with(|p| p.get() == Phase::Gather)
}
pub(crate) fn ready() -> bool {
    PHASE.with(|p| p.get() == Phase::Ready)
}
struct Surface {
    mesh: usize,
    vertices: Range<usize>,
    cpu: Arc<SkinSurface>,
}
struct Draw {
    key: usize,
    pose: Range<usize>,
    scales: Range<usize>,
    palette: Range<usize>,
    surfaces: Range<usize>,
    transform: Transform,
    scale: f32,
    flat: bool,
    fullbright: bool,
    alpha: f32,
    visible: bool,
}
struct Inputs {
    draws: Vec<Draw>,
    surfaces: Vec<Surface>,
    pose: Vec<Transform>,
    scales: Vec<f32>,
    palette: Vec<(Mat3, Vec3)>,
}
struct Task {
    input: usize,
    output: usize,
    draws: Range<usize>,
}
impl Task {
    unsafe fn run(address: usize) {
        let task = &*(address as *const Task);
        let input = &*(task.input as *const Inputs);
        for draw in &input.draws[task.draws.clone()] {
            let pose = &input.pose[draw.pose.clone()];
            let scales = &input.scales[draw.scales.clone()];
            let palette = &input.palette[draw.palette.clone()];
            for surface in &input.surfaces[draw.surfaces.clone()] {
                // Each surface owns a disjoint range; tasks split at draw boundaries.
                let output = std::slice::from_raw_parts_mut(
                    (task.output as *mut Vertex).add(surface.vertices.start),
                    surface.vertices.len(),
                );
                for ((src, bind), dst) in surface.cpu.vertices
                    .iter().zip(&surface.cpu.bind_vertices)
                    .zip(&mut *output)
                {
                    let weights = &src.weights;
                    // Initialize untouched fields on the worker from load-time data.
                    *dst = *bind;
                    let position: Vec3 = if !palette.is_empty() {
                        weights
                            .iter()
                            .map(|w| {
                                let (r, t) = palette[w.bone];
                                (r * w.offset + t) * w.amount
                            })
                            .sum()
                    } else if scales.is_empty() {
                        weights
                            .iter()
                            .map(|w| pose[w.bone].point(w.offset) * w.amount)
                            .sum()
                    } else {
                        weights
                            .iter()
                            .map(|w| pose[w.bone].point(w.offset * scales[w.bone]) * w.amount)
                            .sum()
                    };
                    dst.position = if !palette.is_empty() {
                        rotation(draw.transform.rotation) * (position * draw.scale)
                            + draw.transform.translation
                    } else {
                        draw.transform.point(position * draw.scale)
                    };
                    dst.normal = Vec4::ZERO;
                }
                shade_arrays(
                    output,
                    &surface.cpu.indices,
                    draw.flat,
                    draw.fullbright,
                    draw.alpha,
                );
            }
        }
    }
}
struct Store {
    // Workers drain before storage is released, also on TLS shutdown.
    workers: Vec<crate::skin_worker::Worker>,
    tasks: [UnsafeCell<Task>; 2],
    input: Box<UnsafeCell<Inputs>>,
    output: Vec<Vertex>,
    output_len: usize,
    pending: usize,
    cursor: usize,
    overflow: bool,
    wait_us: u64,
    service_us: u64,
    fallback: usize,
    frame: u64,
}
impl Store {
    fn new(workers: usize, probe: bool) -> Box<Self> {
        assert!((1..=2).contains(&workers));
        // Compile-time CPU ownership checks; no unsafe Send impl for render types.
        fn cpu_only<T: Send>() {}
        cpu_only::<Inputs>();
        cpu_only::<Vertex>();
        Box::new(Self {
            workers: (0..workers)
                .map(|i| crate::skin_worker::Worker::new(i, probe))
                .collect(),
            tasks: std::array::from_fn(|_| {
                UnsafeCell::new(Task {
                    input: 0,
                    output: 0,
                    draws: 0..0,
                })
            }),
            input: Box::new(UnsafeCell::new(Inputs {
                draws: Vec::with_capacity(DRAWS),
                surfaces: Vec::with_capacity(SURFACES),
                pose: Vec::with_capacity(BONES),
                scales: Vec::with_capacity(BONES),
                palette: Vec::with_capacity(BONES),
            })),
            output: vec![Vertex { position: Vec3::ZERO, uv: Vec2::ZERO,
                normal: Vec4::ZERO, color: [0; 4] }; VERTICES],
            output_len: 0,
            pending: 0,
            cursor: 0,
            overflow: false,
            wait_us: 0,
            service_us: 0,
            fallback: 0,
            frame: 0,
        })
    }
    fn reset(&mut self) {
        assert_eq!(self.pending, 0);
        let i = unsafe { &mut *self.input.get() };
        i.draws.clear();
        i.surfaces.clear();
        i.pose.clear();
        i.scales.clear();
        i.palette.clear();
        self.output_len = 0;
        self.frame += 1;
        self.cursor = 0;
        self.overflow = false;
        self.wait_us = 0;
        self.service_us = 0;
        self.fallback = 0;
    }
    fn capture(
        &mut self,
        skeleton: &Skeleton,
        meshes: &[Option<Mesh>],
        p: Params<'_>,
        visible: bool,
    ) {
        if self.overflow {
            return;
        }
        let surfaces = if visible {
            meshes.iter().flatten().count()
        } else {
            0
        };
        let vertices = if visible {
            meshes.iter().flatten().map(|m| m.vertices.len()).sum()
        } else {
            0
        };
        let i = unsafe { &mut *self.input.get() };
        if i.draws.len() == DRAWS
            || i.surfaces.len() + surfaces > SURFACES
            || self.output_len + vertices > VERTICES
            || i.pose.len() + p.pose.len() > BONES
            || i.scales.len() + p.scales.len() > BONES
            || i.palette.len() + p.palette.len() > BONES
        {
            self.overflow = true;
            return;
        }
        let draw = Draw {
            key: skeleton as *const Skeleton as usize,
            pose: i.pose.len()..i.pose.len() + p.pose.len(),
            scales: i.scales.len()..i.scales.len() + p.scales.len(),
            palette: i.palette.len()..i.palette.len() + p.palette.len(),
            surfaces: i.surfaces.len()..i.surfaces.len() + surfaces,
            transform: p.transform,
            scale: p.scale,
            flat: p.flat,
            fullbright: p.fullbright,
            alpha: p.alpha,
            visible,
        };
        i.pose.extend_from_slice(p.pose);
        i.scales.extend_from_slice(p.scales);
        i.palette.extend_from_slice(p.palette);
        if visible {
            for (mesh, (surface, m)) in skeleton.surfaces.iter().zip(meshes).enumerate() {
                let Some(m) = m else {
                    continue;
                };
                assert_eq!(surface.vertices.len(), m.vertices.len());
                let vertices = self.output_len..self.output_len + m.vertices.len();
                // Capacity is checked above; only reserve output space on the game thread.
                // Workers initialize every field, including UV, from immutable load data.
                self.output_len = vertices.end;
                i.surfaces.push(Surface { mesh, vertices, cpu: surface.cpu() });
            }
        }
        i.draws.push(draw);
    }
    fn dispatch(&mut self) {
        assert_eq!(self.pending, 0);
        let i = unsafe { &*self.input.get() };
        let draws = i.draws.len();
        let mid = if self.workers.len() == 1 {
            draws
        } else {
            let half = self.output_len.div_ceil(2);
            i.draws
                .iter()
                .position(|d| {
                    i.surfaces[d.surfaces.clone()]
                        .last()
                        .is_some_and(|s| s.vertices.end >= half)
                })
                .map_or(draws, |i| i + 1)
        };
        let output = self.output.as_mut_ptr() as usize;
        let input = self.input.get() as usize;
        for (index, range) in [0..mid, mid..draws]
            .into_iter()
            .take(self.workers.len())
            .enumerate()
        {
            unsafe {
                *self.tasks[index].get() = Task {
                    input,
                    output,
                    draws: range,
                };
            }
            // Store is boxed before this call, and cannot be moved/reset/read until drain.
            unsafe {
                self.workers[index].start(self.tasks[index].get() as usize, Task::run);
            }
            self.pending += 1;
        }
    }
    fn drain(&mut self) -> Option<Box<dyn std::any::Any + Send>> {
        let start = std::time::Instant::now();
        let mut panic = None;
        for worker in &mut self.workers[..self.pending] {
            match worker.wait_result() {
                Ok(m) => self.service_us += m.ns / 1000,
                Err(p) => {
                    if panic.is_none() {
                        panic = Some(p);
                    }
                }
            }
        }
        self.pending = 0;
        self.wait_us = start.elapsed().as_micros() as u64;
        panic
    }
    fn consume(
        &mut self,
        skeleton: &Skeleton,
        meshes: &mut [Option<Mesh>],
        p: Params<'_>,
    ) -> Option<bool> {
        assert_eq!(self.pending, 0);
        let i = unsafe { &*self.input.get() };
        let Some(d) = i.draws.get(self.cursor) else {
            self.fallback += 1;
            return None;
        };
        self.cursor += 1;
        // A mismatching caller remains correct via the original serial kernel.
        if d.key != skeleton as *const Skeleton as usize
            || !same_transform(d.transform, p.transform)
            || d.scale.to_bits() != p.scale.to_bits()
            || d.flat != p.flat
            || d.fullbright != p.fullbright
            || d.alpha.to_bits() != p.alpha.to_bits()
            || !same_pose(&i.pose[d.pose.clone()], p.pose)
            || !same_floats(&i.scales[d.scales.clone()], p.scales)
            || i.palette[d.palette.clone()] != *p.palette
            || (d.visible
                && (meshes.iter().flatten().count() != d.surfaces.len()
                    || i.surfaces[d.surfaces.clone()].iter().any(|s| {
                        meshes.get(s.mesh).and_then(Option::as_ref).is_none_or(|m| {
                            m.vertices.len() != s.vertices.len()
                                || m.indices.as_slice() != s.cpu.indices.as_slice()
                                || skeleton.surfaces.get(s.mesh).is_none_or(|surface| !surface.shares_cpu(&s.cpu))
                        })
                    })))
        {
            self.fallback += 1;
            return None;
        }
        for surface in &i.surfaces[d.surfaces.clone()] {
            // The serial kernel leaves live UVs untouched (including any caller edits).
            // Only its three written fields return from the worker; no gather-time
            // copy of the live/bind vertices is needed to preserve the fourth field.
            for (dst, src) in meshes[surface.mesh].as_mut().unwrap().vertices.iter_mut()
                .zip(&self.output[surface.vertices.clone()]) {
                dst.position = src.position;
                dst.normal = src.normal;
                dst.color = src.color;
            }
        }
        Some(d.visible)
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = self.drain();
    }
}
fn same_transform(a: Transform, b: Transform) -> bool {
    a.translation.to_array().map(f32::to_bits) == b.translation.to_array().map(f32::to_bits)
        && a.rotation.to_array().map(f32::to_bits) == b.rotation.to_array().map(f32::to_bits)
}
pub(super) fn same_pose(a: &[Transform], b: &[Transform]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(&a, &b)| same_transform(a, b))
}
pub(super) fn same_floats(a: &[f32], b: &[f32]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.to_bits() == b.to_bits())
}

pub(crate) fn startup(workers: usize, probe: bool) {
    // J13's game thread was sampled on CPU2: reserve CPU0, never contend with worker2.
    crate::skin_worker::pin_game(probe);
    STORE.with(|s| *s.borrow_mut() = Some(Store::new(workers, probe)));
}
/// A non-Send guard drains pending CPU work before a frame/load/panic can leave scope.
pub(crate) struct Frame {
    _game: crate::game_thread::GameThread,
    gather_start: std::time::Instant,
    gather_us: u64,
}
impl Frame {
    pub(crate) fn begin() -> Self {
        let gather_start = std::time::Instant::now();
        assert!(PHASE.with(|p| p.get() == Phase::Idle));
        STORE.with(|s| s.borrow_mut().as_mut().expect("overlap startup").reset());
        super::prepared::begin();
        PHASE.with(|p| p.set(Phase::Gather));
        Self {
            _game: crate::game_thread::GameThread::default(),
            gather_start,
            gather_us: 0,
        }
    }
    pub(crate) fn assert_consumed(&self) {
        STORE.with(|s| {
            let s = s.borrow();
            let s = s.as_ref().unwrap();
            let i = unsafe { &*s.input.get() };
            assert!(!s.overflow, "overlap arena overflow");
            assert_eq!(s.fallback, 0, "overlap input mismatch");
            assert_eq!(s.cursor, i.draws.len(), "unconsumed gathered skin draws");
        });
        super::prepared::assert_consumed();
    }
    pub(crate) fn dispatch(&mut self) {
        assert!(gathering());
        self.gather_us = self.gather_start.elapsed().as_micros() as u64;
        STORE.with(|s| s.borrow_mut().as_mut().unwrap().dispatch());
        PHASE.with(|p| p.set(Phase::Running));
    }
    pub(crate) fn collect(&mut self) {
        assert!(PHASE.with(|p| p.get() == Phase::Running));
        let panic = STORE.with(|s| s.borrow_mut().as_mut().unwrap().drain());
        PHASE.with(|p| p.set(Phase::Ready));
        STORE.with(|s| {
            let s = s.borrow();
            let s = s.as_ref().unwrap();
            crate::light_profile::skin_collect_wait(s.wait_us);
            #[cfg(feature = "vita_renderer")]
            crate::render_fx::retained::count("skin_collect_wait_us", s.wait_us);
        });
        if let Some(p) = panic {
            std::panic::resume_unwind(p);
        }
    }
}
impl Drop for Frame {
    fn drop(&mut self) {
        STORE.with(|s| { if let Some(s)=s.borrow_mut().as_mut() { if s.pending>0 { let _=s.drain(); }
            crate::light_profile::skin_overlap(self.gather_us, s.fallback as u64, s.overflow);
            #[cfg(feature="vita_renderer")]
            { crate::render_fx::retained::count("skin_overlap_fallback_draws",s.fallback as u64);
              crate::render_fx::retained::count("skin_gather_us",self.gather_us);
              crate::render_fx::retained::count("skin_overflow",u64::from(s.overflow));
              crate::render_fx::retained::count("skin_worker_service_us",s.service_us); }
            if crate::light_profile::full() && !std::thread::panicking() {
                eprintln!("[lg/skin-overlap] frame={} gather_us={} collect_wait_us={} worker_service_us={} fallback_draws={} overflow={}",s.frame,self.gather_us,s.wait_us,s.service_us,s.fallback,s.overflow);
            }
        }});
        PHASE.with(|p| p.set(Phase::Idle));
    }
}
/// Only owning immutable CPU handles and per-frame numeric inputs persist.
pub(in crate::character) fn handle(
    skeleton: &Skeleton,
    meshes: &mut [Option<Mesh>],
    p: Params<'_>,
    visible: bool,
) -> Option<bool> {
    PHASE.with(|phase| match phase.get() {
        Phase::Gather => {
            STORE.with(|s| {
                s.borrow_mut()
                    .as_mut()
                    .unwrap()
                    .capture(skeleton, meshes, p, visible)
            });
            Some(visible)
        }
        Phase::Ready => STORE.with(|s| {
            s.borrow_mut()
                .as_mut()
                .unwrap()
                .consume(skeleton, meshes, p)
        }),
        Phase::Idle => None,
        Phase::Running => panic!("skin submitted before overlap collect"),
    })
}

fn same_vertices(a: &[Vertex], b: &[Vertex]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.position.to_array().map(f32::to_bits) == b.position.to_array().map(f32::to_bits)
                && a.normal.to_array().map(f32::to_bits) == b.normal.to_array().map(f32::to_bits)
                && a.uv.to_array().map(f32::to_bits) == b.uv.to_array().map(f32::to_bits)
                && a.color == b.color
        })
}
/// Headless actual-data comparison of owned early snapshots, duplicate shared-model
/// occurrences, unrelated game-thread work, and both persistent worker counts.
pub(crate) fn check_skeleton(
    skeleton: &Skeleton,
    poses: &[Vec<Transform>],
) -> anyhow::Result<usize> {
    let mut comparisons = 0;
    let mut meshes: Vec<_> = skeleton
        .surfaces
        .iter()
        .map(|s| {
            Some(Mesh {
                vertices: s
                    .vertices
                    .iter()
                    .map(|v| Vertex {
                        position: Vec3::ZERO,
                        uv: v.uv,
                        normal: Vec4::ZERO,
                        color: [0; 4],
                    })
                    .collect(),
                indices: s.indices.clone(),
                texture: None,
            })
        })
        .collect();
    for workers in [1, 2] {
        let mut store = Store::new(workers, false);
        for pose in poses {
            for palette_on in [false, true] {
                for flat in [false, true] {
                    for scaled in [false, true] {
                        let scales: Vec<_> = if scaled {
                            (0..pose.len()).map(|i| [0.65, 1.4, -0.75][i % 3]).collect()
                        } else {
                            vec![]
                        };
                        let palette = if palette_on {
                            super::palette(pose, &scales)
                        } else {
                            vec![]
                        };
                        let p = Params {
                            pose,
                            scales: &scales,
                            palette: &palette,
                            transform: Transform {
                                rotation: Quat::from_rotation_z(0.731),
                                translation: vec3(10., 20., 30.),
                            },
                            scale: -0.75,
                            flat,
                            fullbright: false,
                            alpha: 0.731,
                        };
                        store.reset();
                        store.capture(skeleton, &meshes, p, true);
                        let other = Params {
                            transform: Transform {
                                translation: vec3(-10., -20., -30.),
                                ..p.transform
                            },
                            fullbright: true,
                            ..p
                        };
                        store.capture(skeleton, &meshes, other, true);
                        anyhow::ensure!(
                            !store.overflow,
                            "real skin snapshot exceeds startup bounds"
                        );
                        store.dispatch();
                        std::hint::black_box(vec![42u8; 1024]); // concurrent game allocations do not touch CPU snapshots
                        anyhow::ensure!(store.drain().is_none(), "overlap worker panic");
                        for p in [p, other] {
                            let mut reference: Vec<_> = meshes
                                .iter()
                                .flatten()
                                .map(|m| m.vertices.clone())
                                .collect();
                            for (s, vertices) in skeleton.surfaces.iter().zip(&mut reference) {
                                super::kernel(
                                    &mut super::Job {
                                        input: &s.vertices,
                                        vertices,
                                        indices: &s.indices,
                                    },
                                    p,
                                );
                            }
                            anyhow::ensure!(
                                store.consume(skeleton, &mut meshes, p) == Some(true),
                                "early snapshot lookup failed"
                            );
                            for (reference, mesh) in reference.iter().zip(meshes.iter().flatten()) {
                                anyhow::ensure!(
                                    same_vertices(reference, &mesh.vertices),
                                    "overlap {workers} differs from serial"
                                );
                            }
                            comparisons += 1;
                        }
                    }
                }
            }
        }
    }
    Ok(comparisons)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeletal::Weight;
    fn fixture() -> (Skeleton, Vec<Option<Mesh>>, Vec<Transform>) {
        let pose = vec![Transform {
            rotation: Quat::from_rotation_z(0.123),
            translation: Vec3::ONE,
        }];
        let skeleton = Skeleton {
            bones: vec![crate::skeletal::Bone {
                parent: None,
                name: "root".into(),
            }],
            surfaces: vec![crate::skeletal::Surface::new("body".into(), [Vec3::ZERO, Vec3::X, Vec3::Y]
                    .into_iter()
                    .map(|offset| crate::skeletal::SkinVertex {
                        uv: Vec2::new(0.3, 0.7),
                        weights: vec![Weight {
                            bone: 0,
                            amount: 1.,
                            offset,
                        }],
                    })
                    .collect(),
                vec![0, 1, 2])],
            skin_bounds: vec![],
        };
        let meshes = vec![Some(Mesh {
            vertices: vec![
                Vertex {
                    position: Vec3::ZERO,
                    uv: Vec2::new(0.3, 0.7),
                    normal: Vec4::ONE,
                    color: [42; 4]
                };
                3
            ],
            indices: vec![0, 1, 2],
            texture: None,
        })];
        (skeleton, meshes, pose)
    }
    fn params(pose: &[Transform]) -> Params<'_> {
        Params {
            pose,
            scales: &[],
            palette: &[],
            transform: pose[0],
            scale: 1.,
            flat: false,
            fullbright: false,
            alpha: 0.731,
        }
    }
    #[test]
    fn owned_overlap_duplicate_shared_actor_poses_match_serial() {
        let (s, _, pose) = fixture();
        assert_eq!(check_skeleton(&s, &[pose]).unwrap(), 32);
    }
    #[test]
    fn load_handles_outlive_skeleton_and_live_mesh_while_workers_run() {
        let (s, meshes, pose) = fixture();
        let handle = s.surfaces[0].cpu();
        let mut store = Store::new(2, false);
        store.capture(&s, &meshes, params(&pose), true);
        let input = unsafe { &*store.input.get() };
        assert!(Arc::ptr_eq(&handle, &input.surfaces[0].cpu));
        let source = handle.vertices.as_ptr();
        store.dispatch();
        drop(s);
        drop(meshes);
        drop(handle);
        assert!(store.drain().is_none());
        let input = unsafe { &*store.input.get() };
        assert_eq!(source, input.surfaces[0].cpu.vertices.as_ptr());
        assert_eq!(store.output_len, 3);
        assert!(store.output[..3].iter().all(|v| v.position.is_finite()));
        store.reset();
    }
    #[test]
    fn live_uvs_are_preserved_and_changed_indices_fall_back() {
        let (s, mut meshes, pose) = fixture();
        meshes[0].as_mut().unwrap().vertices[0].uv = Vec2::new(-2., 4.);
        let mut reference = meshes[0].as_ref().unwrap().vertices.clone();
        let p = params(&pose);
        super::super::kernel(&mut super::super::Job {
            input: &s.surfaces[0].vertices, vertices: &mut reference,
            indices: &s.surfaces[0].indices,
        }, p);
        let mut store = Store::new(1, false);
        store.capture(&s, &meshes, p, true);
        store.dispatch(); assert!(store.drain().is_none());
        assert_eq!(store.consume(&s, &mut meshes, p), Some(true));
        assert!(same_vertices(&reference, &meshes[0].as_ref().unwrap().vertices));
        store.reset();
        meshes[0].as_mut().unwrap().indices.swap(1, 2);
        store.capture(&s, &meshes, p, true);
        store.dispatch(); assert!(store.drain().is_none());
        assert_eq!(store.consume(&s, &mut meshes, p), None);
        assert_eq!(store.fallback, 1);
    }
    #[test]
    fn gather_dispatch_collect_copy_are_allocation_free_and_bounded() {
        let (s, mut meshes, pose) = fixture();
        let p = params(&pose);
        for count in [1, 2] {
            let mut store = Store::new(count, false);
            store.workers = (0..count)
                .map(|i| crate::skin_worker::Worker::measured(i, false))
                .collect();
            // Warm OS wake/timer paths before measuring steady state.
            store.capture(&s, &meshes, p, true);
            store.dispatch();
            assert!(store.drain().is_none());
            let ptrs = (store.output.as_ptr(), unsafe {
                (&*store.input.get()).surfaces.as_ptr()
            });
            for _ in 0..20 {
                let scope = crate::kernel_alloc::Scope::begin();
                store.reset();
                store.capture(&s, &meshes, p, true);
                store.capture(&s, &meshes, p, true);
                store.dispatch();
                assert!(store.drain().is_none());
                assert_eq!(store.consume(&s, &mut meshes, p), Some(true));
                assert_eq!(store.consume(&s, &mut meshes, p), Some(true));
                assert_eq!(scope.finish(), 0);
                for worker in &mut store.workers {
                    assert_eq!(worker.wait().malloc_calls, 0);
                }
                assert_eq!(
                    ptrs,
                    (store.output.as_ptr(), unsafe {
                        (&*store.input.get()).surfaces.as_ptr()
                    })
                );
            }
        }
    }
    #[test]
    fn culls_overflow_and_changed_pose_use_ordered_safe_fallback() {
        let (s, mut meshes, pose) = fixture();
        let p = params(&pose);
        let mut store = Store::new(1, false);
        store.capture(&s, &meshes, p, false);
        store.capture(&s, &meshes, p, true);
        store.dispatch();
        assert!(store.drain().is_none());
        assert_eq!(store.consume(&s, &mut meshes, p), Some(false));
        assert_eq!(
            store.consume(&s, &mut meshes, Params { scale: 2., ..p }),
            None
        );
        store.reset();
        for _ in 0..DRAWS + 1 {
            store.capture(&s, &meshes, p, true);
        }
        assert!(store.overflow);
        assert_eq!(unsafe { (&*store.input.get()).draws.len() }, DRAWS);
        store.dispatch();
        assert!(store.drain().is_none());
        for _ in 0..DRAWS {
            assert_eq!(store.consume(&s, &mut meshes, p), Some(true));
        }
        assert_eq!(store.consume(&s, &mut meshes, p), None);
        assert_eq!(store.fallback, 1);
    }
    #[test]
    fn collect_drains_both_panics_and_workers_recover() {
        unsafe fn panic_job(_: usize) {
            panic!("injected overlap failure");
        }
        let mut store = Store::new(2, false);
        for worker in &mut store.workers {
            unsafe {
                worker.start(0, panic_job);
            }
        }
        store.pending = 2;
        assert!(store.drain().is_some());
        assert_eq!(store.pending, 0);
        let (s, mut meshes, pose) = fixture();
        store.reset();
        store.capture(&s, &meshes, params(&pose), true);
        store.dispatch();
        assert!(store.drain().is_none());
        assert_eq!(store.consume(&s, &mut meshes, params(&pose)), Some(true));
    }
    #[test]
    fn frame_unwind_drains_and_next_frame_reuses_storage() {
        startup(2, false);
        let (s, mut meshes, pose) = fixture();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut frame = Frame::begin();
            assert_eq!(handle(&s, &mut meshes, params(&pose), true), Some(true));
            frame.dispatch();
            panic!("game work failed");
        }));
        assert!(result.is_err());
        assert!(!gathering());
        assert!(!ready());
        let mut frame = Frame::begin();
        handle(&s, &mut meshes, params(&pose), true);
        frame.dispatch();
        frame.collect();
        assert_eq!(handle(&s, &mut meshes, params(&pose), true), Some(true));
        drop(frame);
    }
}
