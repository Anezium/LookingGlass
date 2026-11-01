//! Same-frame CPU skin snapshots. Render handles never enter this storage.
//! Buffers are bounded and allocated at startup; overflow uses the serial draw.
#[cfg(feature="frame_total_only")]
use crate::light_profile::InactiveInstant as MetricInstant;
#[cfg(not(feature="frame_total_only"))]
use std::time::Instant as MetricInstant;
use super::{rotation, shade_arrays, Params};
use crate::render_fx::actor_vertices::{self, Revision};
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
/// The old traversal suppresses consumers; the single preparation executes them.
pub(crate) fn gather_only() -> bool { gathering() && !single_preparing() }
pub(crate) fn single_preparing() -> bool {
    #[cfg(feature="vita_renderer")]
    { gathering() && crate::render_fx::actor_publication::active() }
    #[cfg(not(feature="vita_renderer"))]
    { false }
}
pub(crate) fn single_frame() -> bool {
    #[cfg(feature="vita_renderer")]
    { crate::render_fx::actor_publication::active() }
    #[cfg(not(feature="vita_renderer"))]
    { false }
}
pub(crate) fn ready() -> bool {
    PHASE.with(|p| p.get() == Phase::Ready)
}
static SERIAL_FALLBACKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub(crate) fn note_serial_fallback() -> u64 {
    SERIAL_FALLBACKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
}
pub(crate) fn serial_fallbacks() -> u64 {
    SERIAL_FALLBACKS.load(std::sync::atomic::Ordering::Relaxed)
}
#[derive(Clone)]
pub(crate) struct OwnedSkin {
    pub pose: Vec<Transform>,
    pub scales: Vec<f32>,
    pub transform: Transform,
    pub scale: f32,
    pub fullbright: bool,
    pub alpha: f32,
}
/// Replay this skeleton's last gathered draw through the serial kernel.
/// That kernel is the reference a missed consumer declaration was protecting.
pub(crate) fn restore_serial_live(skeleton: &Skeleton, meshes: &mut [Option<Mesh>]) -> bool {
    if !gathering() { return false; }
    let key = skeleton as *const Skeleton as usize;
    STORE.with(|slot| {
        let store = slot.borrow();
        let Some(store) = store.as_ref() else { return false; };
        let input = unsafe { &*store.input.get() };
        let Some(draw) = input.draws.iter().rev().find(|draw| draw.visible && draw.key == key) else { return false; };
        let params = Params {
            pose: &input.pose[draw.pose.clone()],
            scales: &input.scales[draw.scales.clone()],
            palette: &input.palette[draw.palette.clone()],
            transform: draw.transform,
            scale: draw.scale,
            flat: draw.flat,
            fullbright: draw.fullbright,
            alpha: draw.alpha,
        };
        let surfaces = draw.surfaces.clone();
        let mut restored = false;
        for surface_index in surfaces {
            let mesh_id = input.surfaces[surface_index].mesh;
            let Some(cpu) = skeleton.surfaces.get(mesh_id) else { continue; };
            let Some(mesh) = meshes.get_mut(mesh_id).and_then(Option::as_mut) else { continue; };
            super::kernel(&mut super::Job {
                input: &cpu.vertices,
                vertices: &mut mesh.vertices,
                indices: &mesh.indices,
            }, params);
            restored = true;
        }
        restored
    })
}
/// Serial kernel for one pose. Used when this frame gathered no draw for the model.
pub(crate) fn serial_skin_current(
    skeleton: &Skeleton,
    meshes: &mut [Option<Mesh>],
    pose: &[Transform],
    scales: &[f32],
    transform: Transform,
    scale: f32,
    fullbright: bool,
    alpha: f32,
) -> bool {
    #[cfg(feature = "vita_renderer")]
    let use_palette = crate::render::vita_renderer::config().bone_palette;
    #[cfg(not(feature = "vita_renderer"))]
    let use_palette = false;
    #[cfg(feature = "vita_renderer")]
    let flat_cfg = crate::render::vita_renderer::config().flat_actor_light;
    #[cfg(not(feature = "vita_renderer"))]
    let flat_cfg = false;
    let (_cull, flat) = super::visual_options(false, flat_cfg, meshes);
    let mut wrote = false;
    super::with_palette(use_palette, pose, scales, |palette| {
        let params = super::Params {
            pose, scales, palette, transform, scale, flat, fullbright, alpha,
        };
        for (surface, mesh) in skeleton.surfaces.iter().zip(meshes.iter_mut()) {
            let Some(mesh) = mesh else { continue };
            super::kernel(&mut super::Job {
                input: &surface.vertices,
                vertices: &mut mesh.vertices,
                indices: &mesh.indices,
            }, params);
            wrote = true;
        }
    });
    wrote
}
/// Release path for an undeclared live read. Replays this frame's gathered draw
/// when one exists, otherwise skins the supplied current pose. Either way the
/// mesh is the serial kernel's output, and the counter moves by one.
pub(crate) fn release_serial_fallback(
    skeleton: &Skeleton,
    meshes: &mut [Option<Mesh>],
    current: &OwnedSkin,
) -> bool {
    let gathered = restore_serial_live(skeleton, meshes);
    let restored = if gathered {
        true
    } else {
        serial_skin_current(skeleton, meshes, &current.pose, &current.scales,
            current.transform, current.scale, current.fullbright, current.alpha)
    };
    let count = note_serial_fallback();
    let source = if gathered { "gathered_draw" } else { "current_pose" };
    eprintln!("[lg/skin] serial fallback count={count} restored={restored} source={source} reason=live skin read without a consumer declaration before body preparation");
    restored
}
#[derive(Clone)]
struct Surface {
    mesh: usize,
    vertices: Range<usize>,
    cpu: Arc<SkinSurface>,
    revision: Revision,
    output: usize,
    alpha_variant: bool,
    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
    oracle:usize,
}
struct Draw {
    instance: u64,
    actor: Option<super::identity::ActorId>,
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
    layers: usize,
    dirty: usize,
    census: bool,
    census_ns: u64,
}
#[cfg(test)]
static PANIC_INPUT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Test-only: workers of this thread's store panic while `run` executes.
#[cfg(test)]
pub(crate) fn inject_worker_panic<T>(run: impl FnOnce() -> T) -> T {
    struct Reset;
    impl Drop for Reset { fn drop(&mut self) { PANIC_INPUT.store(0, std::sync::atomic::Ordering::SeqCst); } }
    let input = STORE.with(|s| s.borrow().as_ref().map(|s| s.input.get() as usize)).expect("overlap startup");
    PANIC_INPUT.store(input, std::sync::atomic::Ordering::SeqCst);
    let _reset = Reset;
    run()
}
impl Task {
    unsafe fn run(address: usize) {
        let task = &mut *(address as *mut Task);
        #[cfg(test)]
        if task.input == PANIC_INPUT.load(std::sync::atomic::Ordering::SeqCst) { panic!("injected publication worker failure"); }
        let started=task.census.then(std::time::Instant::now);
        let input = &*(task.input as *const Inputs);
        for draw in &input.draws[task.draws.clone()] {
            let pose = &input.pose[draw.pose.clone()];
            let scales = &input.scales[draw.scales.clone()];
            let palette = &input.palette[draw.palette.clone()];
            for surface_id in draw.surfaces.clone() {
                let surface = &input.surfaces[surface_id];
                // Each surface owns a disjoint range; tasks split at draw boundaries.
                let output = std::slice::from_raw_parts_mut(
                    if surface.output == 0 { (task.output as *mut Vertex).add(surface.vertices.start) }
                    else { surface.output as *mut Vertex },
                    surface.vertices.len(),
                );
                for ((src, bind), dst) in surface.cpu.vertices
                    .iter().zip(&surface.cpu.bind_vertices)
                    .zip(&mut *output)
                {
                    let weights = &src.weights;
                    // Initialize untouched fields on the worker from load-time data.
                    if surface.output == 0 { *dst = *bind; }
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
                if surface.output != 0 {
                    for vertex in &mut *output { *vertex = actor_vertices::normalize(*vertex, surface.alpha_variant); }
                }
                if task.layers != 0 {
                    let layers = std::slice::from_raw_parts_mut(
                        (task.layers as *mut [Vertex; 2]).add(surface.vertices.start), output.len());
                    let mut dirty = [false; 2];
                    for (src, dst) in output.iter().zip(layers) {
                        for variant in 0..2 {
                            let v = actor_vertices::normalize(*src, variant == 1);
                            dirty[variant] |= !actor_vertices::equal_written(&v, &dst[variant]);
                            dst[variant] = v;
                        }
                    }
                    *(task.dirty as *mut [bool; 2]).add(surface_id) = dirty;
                }
            }
        }

        task.census_ns=started.map_or(0,|t|t.elapsed().as_nanos() as u64);
    }
}
struct Store {
    // Workers drain before storage is released, also on TLS shutdown.
    workers: Vec<crate::skin_worker::Worker>,
    tasks: [UnsafeCell<Task>; 2],
    input: Box<UnsafeCell<Inputs>>,
    #[cfg(feature="vita_renderer")]
    previous_numeric: Option<Box<Inputs>>,
    output: Vec<Vertex>,
    output_len: usize,
    layers: Vec<[Vertex; 2]>,
    dirty: Vec<[bool; 2]>,
    previous: Vec<Surface>,
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
        #[cfg(feature="vita_renderer")]
        let reuse = crate::render::vita_renderer::config().actor_layer_reuse;
        #[cfg(not(feature="vita_renderer"))]
        let reuse = false;
        let zero = Vertex { position: Vec3::ZERO, uv: Vec2::ZERO, normal: Vec4::ZERO, color: [0; 4] };
        Box::new(Self {
            workers: (0..workers)
                .map(|i| crate::skin_worker::Worker::new(i, probe))
                .collect(),
            tasks: std::array::from_fn(|_| {
                UnsafeCell::new(Task {
                    input: 0,
                    output: 0,
                    draws: 0..0, layers: 0, dirty: 0, census:false, census_ns:0,
                })
            }),
            input: Box::new(UnsafeCell::new(Inputs {
                draws: Vec::with_capacity(DRAWS),
                surfaces: Vec::with_capacity(SURFACES),
                pose: Vec::with_capacity(BONES),
                scales: Vec::with_capacity(BONES),
                palette: Vec::with_capacity(BONES),
            })),
            #[cfg(feature="vita_renderer")]
            previous_numeric:None,
            output: vec![Vertex { position: Vec3::ZERO, uv: Vec2::ZERO,
                normal: Vec4::ZERO, color: [0; 4] }; VERTICES],
            output_len: 0,
            layers: if reuse {vec![[zero; 2]; VERTICES]} else {Vec::new()},
            dirty: if reuse {vec![[false; 2]; SURFACES]} else {Vec::new()},
            previous: if reuse {Vec::with_capacity(SURFACES)} else {Vec::new()},
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
        #[cfg(feature="vita_renderer")]
        if single_frame() {
            let old=self.previous_numeric.get_or_insert_with(||Box::new(Inputs {
                draws:Vec::with_capacity(DRAWS),surfaces:Vec::with_capacity(SURFACES),pose:Vec::with_capacity(BONES),
                scales:Vec::with_capacity(BONES),palette:Vec::with_capacity(BONES)}));
            std::mem::swap(i,&mut **old);
        }
        i.draws.clear();
        self.previous.clear();
        if !self.layers.is_empty() {self.previous.extend(i.surfaces.iter().cloned());}
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
            instance: 0,
            actor: None,
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
                let cpu = surface.cpu();
                let revision = if self.layers.is_empty() {Revision {identity: 0, vertices: [0; 2]}}
                    else {self.previous.get(i.surfaces.len())
                        .filter(|old| old.vertices == vertices && Arc::ptr_eq(&old.cpu, &cpu))
                        .map_or_else(Revision::new, |old| old.revision)};
                i.surfaces.push(Surface { mesh, vertices, cpu, revision, output: 0, alpha_variant: false,
                    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))] oracle:usize::MAX });
            }
        }

        i.draws.push(draw);
    }
    #[cfg(feature="vita_renderer")]
    fn capture_single(&mut self, skeleton: &Skeleton, meshes: &mut [Option<Mesh>], p: Params<'_>, visible: bool) -> bool {
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        let _alloc=crate::sim_maps::probe::NumericAlloc::begin();
        let _census=crate::phase_census::span(crate::phase_census::Region::ActorNumeric);
        crate::frame_profile::skinned_draw(!visible);

        let actor=super::identity::current();
        if !visible { return false; }
        let i = unsafe { &mut *self.input.get() };
        let bounded = actor.is_some() && i.draws.len() < DRAWS && i.surfaces.len() + meshes.iter().flatten().count() <= SURFACES
            && self.output_len + meshes.iter().flatten().map(|m|m.vertices.len()).sum::<usize>() <= VERTICES
            && i.pose.len()+p.pose.len() <= BONES && i.scales.len()+p.scales.len() <= BONES
            && i.palette.len()+p.palette.len() <= BONES;
        let previous=self.previous_numeric.as_ref().and_then(|old|old.draws.iter().find(|d|
            d.actor==actor && !i.draws.iter().any(|used|used.instance==d.instance)
                && d.key==skeleton as *const Skeleton as usize && same_transform(d.transform,p.transform)
                && d.scale.to_bits()==p.scale.to_bits() && d.flat==p.flat && d.fullbright==p.fullbright
                && d.alpha.to_bits()==p.alpha.to_bits()).map(|d|(&**old,d)));
        let instance=previous.map_or_else(super::identity::next_instance,|(_,d)|d.instance);
        let start = i.surfaces.len();
        for (mesh_id, (surface, mesh)) in skeleton.surfaces.iter().zip(meshes).enumerate() {
            let Some(mesh) = mesh else { continue; };
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            let oracle=crate::sim_maps::probe::reserve_skin(actor,mesh_id);
            // Explicit consumers require live mutable fields now. Untracked topology,
            // layers and exhausted banks use the original exact serial kernel.
            let mut reasons=super::mutable_reasons();
            if reasons==0 {
                if !bounded { reasons=crate::phase_census::BOUNDS; }
                else if surface.vertices.len()!=mesh.vertices.len() || surface.indices!=mesh.indices {
                    reasons=crate::phase_census::TOPOLOGY;
                }
            }
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            let prepare_alloc=crate::sim_maps::probe::prepare_checkpoint();
            let direct = if reasons==0 {
                // A draw can animate unrelated bones while this surface is unchanged.
                // Compare its exact numeric dependencies on the game thread; never
                // add J17's dual normalized-output comparison loop to the workers.
                let unchanged=previous.is_some_and(|(old,d)|same_surface_inputs(surface,old,d,p));
                let _census=crate::phase_census::span(crate::phase_census::Region::ActorConsumers);
                crate::render_fx::actor_publication::prepare(mesh, surface.cpu(), unchanged, instance,
                    crate::render_fx::actor_publication::Key {actor:actor.unwrap(),surface:Arc::as_ptr(&surface.cpu()) as usize,occurrence:0})
            } else { None };
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            crate::sim_maps::probe::prepare_allocations(prepare_alloc,direct.is_some());
            if let Some((output, alpha_variant)) = direct {
                crate::phase_census::skin(actor,mesh.vertices.len(),0);
                let vertices = self.output_len..self.output_len+mesh.vertices.len();
                self.output_len = vertices.end;
                i.surfaces.push(Surface { mesh:mesh_id, vertices, cpu:surface.cpu(),
                    revision:Revision {identity:0,vertices:[0;2]}, output, alpha_variant,
                    #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))] oracle });
                #[cfg(feature="actor_diagnostics")]
                crate::render_fx::retained::count("actor_direct_surfaces",1);
            } else {
                if reasons==0 {reasons=crate::phase_census::PUBLICATION;}
                crate::phase_census::skin(actor,mesh.vertices.len(),reasons);
                { let _census=crate::phase_census::span(crate::phase_census::Region::ActorSerial);
                  super::kernel(&mut super::Job { input:&surface.vertices, vertices:&mut mesh.vertices, indices:&mesh.indices },p); }
                #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                crate::sim_maps::probe::record_skin(oracle,&mesh.vertices,&mesh.indices);
                #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
                crate::sim_maps::probe::record_serial_publication(oracle,&mesh.vertices,&mesh.indices,actor_vertices::normalize);
                let _census=crate::phase_census::span(crate::phase_census::Region::ActorConsumers);
                crate::render_fx::skin(mesh);
                #[cfg(feature="actor_diagnostics")]
                crate::render_fx::retained::count("actor_mutable_consumer_surfaces",1);
            }
        }
        if i.surfaces.len() > start {
            let d = Draw { instance, actor, key:skeleton as *const Skeleton as usize,
                pose:i.pose.len()..i.pose.len()+p.pose.len(), scales:i.scales.len()..i.scales.len()+p.scales.len(),
                palette:i.palette.len()..i.palette.len()+p.palette.len(), surfaces:start..i.surfaces.len(),
                transform:p.transform,scale:p.scale,flat:p.flat,fullbright:p.fullbright,alpha:p.alpha,visible };
            i.pose.extend_from_slice(p.pose); i.scales.extend_from_slice(p.scales); i.palette.extend_from_slice(p.palette);

            i.draws.push(d);
        }
        true
    }
    fn dispatch(&mut self) {
        let _census=crate::phase_census::span(crate::phase_census::Region::ActorWorkerDispatch);
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
                    layers: if self.layers.is_empty() {0} else {self.layers.as_mut_ptr() as usize},
                    dirty: self.dirty.as_mut_ptr() as usize,
                    census:crate::phase_census::active(), census_ns:0,
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
        let start = MetricInstant::now();
        let completed = self.pending > 0;
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

        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        if panic.is_none() && single_frame() {
            let input=unsafe {&*self.input.get()};
            for s in &input.surfaces {
                if s.oracle!=usize::MAX {
                    let vertices=unsafe {std::slice::from_raw_parts(s.output as *const Vertex,s.vertices.len())};
                    crate::sim_maps::probe::record_skin(s.oracle,vertices,&s.cpu.indices);
                    crate::sim_maps::probe::record_direct_publication(s.oracle,s.alpha_variant);
                }
            }
        }
        for index in 0..self.pending {
            crate::phase_census::worker_skin(index,unsafe {(*self.tasks[index].get()).census_ns});
        }
        self.pending = 0;
        if completed && !self.layers.is_empty() {
            let input = unsafe { &mut *self.input.get() };
            // Advance every completed producer, including unconsumed/mismatched
            // draws. Otherwise a fallback could hide a changed worker result.
            for (id, surface) in input.surfaces.iter_mut().enumerate() {
                if panic.is_none() {surface.revision.changed(self.dirty[id]);}
                else {surface.revision = Revision::new();}
            }
        }
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
        let i = unsafe { &mut *self.input.get() };
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

        for surface_id in d.surfaces.clone() {
            let surface = &mut i.surfaces[surface_id];

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

#[cfg(feature="vita_renderer")]
fn same_surface_inputs(surface:&SkinSurface, old:&Inputs, draw:&Draw, p:Params<'_>)->bool {
    let palette=&old.palette[draw.palette.clone()];
    let scales=&old.scales[draw.scales.clone()];
    if palette.is_empty()!=p.palette.is_empty() || (palette.is_empty() && scales.is_empty()!=p.scales.is_empty()) {return false;}
    surface.used_bones.iter().all(|&bone| {
        if !palette.is_empty() {

            palette.get(bone).zip(p.palette.get(bone)).is_some_and(|((ar,at),(br,bt))|
                same_floats(&ar.to_cols_array(),&br.to_cols_array()) && same_floats(&at.to_array(),&bt.to_array()))
        } else {

            old.pose[draw.pose.clone()].get(bone).zip(p.pose.get(bone)).is_some_and(|(&a,&b)|same_transform(a,b))
                && (scales.is_empty() || scales.get(bone).zip(p.scales.get(bone)).is_some_and(|(a,b)|a.to_bits()==b.to_bits()))
        }
    })
}

/// Map replacement and same-map restore invalidate previous numeric dependencies
/// as well as owned publications; the preallocated worker arena stays warm.
#[cfg(feature="vita_renderer")]
pub(crate) fn reset_publications() {
    assert!(PHASE.with(|p|p.get()==Phase::Idle),"scene load before skin workers drained");
    STORE.with(|s| {if let Some(store)=s.borrow_mut().as_mut() {
        assert_eq!(store.pending,0);
        store.previous_numeric=None;store.previous.clear();
        let i=unsafe {&mut *store.input.get()};
        i.draws.clear();i.surfaces.clear();i.pose.clear();i.scales.clear();i.palette.clear();
        store.output_len=0;
    }});
    crate::render_fx::actor_publication::reset();
}
pub(crate) fn startup(workers: usize, probe: bool) {
    // J13's game thread was sampled on CPU2: reserve CPU0, never contend with worker2.
    crate::skin_worker::pin_game(probe);
    STORE.with(|s| *s.borrow_mut() = Some(Store::new(workers, probe)));
}
/// Scoped successor batches use the skin pool after collection, including unwind.
/// Only Send CPU closures cross the boundary; no new workers are created.
#[cfg(feature = "vita_renderer")]
pub(crate) fn cpu_pair(first: impl FnOnce() + Send, second: impl FnOnce() + Send) {
    let started = MetricInstant::now();
    let service = STORE.with(|store| {
        let mut store = store.borrow_mut();
        if let Some(store) = store.as_mut().filter(|s| s.pending == 0) {
            if store.workers.len() == 2 {
                let (a, b) = store.workers.split_at_mut(1);
                let times = crate::skin_worker::Worker::pair(&mut a[0], &mut b[0], first, second);
                return Some(times.iter().map(|m| m.ns / 1000).max().unwrap_or(0));
            }
            let time = store.workers[0].execute(first);
            second();
            return Some(time.ns / 1000);
        }
        first(); second(); None
    });
    let Some(service) = service else { return; };
    let wait = started.elapsed().as_micros() as u64;
    crate::light_profile::fx_add(crate::light_profile::FxMetric::WorkerWait, wait);
    crate::render_fx::retained::count("fx_worker_wait_us", wait);
    crate::render_fx::retained::count("fx_worker_pair_jobs", 1);
    crate::render_fx::retained::count("fx_worker_max_service_us", service);
}

/// A non-Send guard drains pending CPU work before a frame/load/panic can leave scope.
pub(crate) struct Frame {
    _game: crate::game_thread::GameThread,
    gather_start: MetricInstant,
    gather_us: u64,
}
impl Frame {
    pub(crate) fn begin() -> Self {
        let gather_start = MetricInstant::now();
        #[cfg(feature="vita_renderer")]
        crate::render_fx::actor_publication::begin();
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
        if single_frame() { return; }
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
        #[cfg(feature="vita_renderer")]
        if single_frame() { crate::render_fx::actor_publication::hold(); }
        self.gather_us = self.gather_start.elapsed().as_micros() as u64;
        STORE.with(|s| s.borrow_mut().as_mut().unwrap().dispatch());
        crate::light_profile::render_add(crate::light_profile::RenderPhase::Actors,
            crate::light_profile::RenderMetrics { cpu_ns: self.gather_start.elapsed().as_nanos() as u64, ..Default::default() });
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
            crate::light_profile::render_add(crate::light_profile::RenderPhase::Actors,
                crate::light_profile::RenderMetrics { cpu_ns: s.wait_us * 1000, ..Default::default() });
            #[cfg(feature = "vita_renderer")]
            crate::render_fx::retained::count("skin_collect_wait_us", s.wait_us);
        });
        #[cfg(feature="vita_renderer")]
        if panic.is_none() && single_frame() { crate::render_fx::actor_publication::complete(); }
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
        #[cfg(feature="vita_renderer")]
        crate::render_fx::actor_publication::end();
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
            #[cfg(feature="vita_renderer")]
            if single_frame() { return STORE.with(|s| Some(s.borrow_mut().as_mut().unwrap().capture_single(skeleton,meshes,p,visible))); }
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

/// Synchronous handoff after consume: no mesh mutation can intervene. The caller
/// still owns fully updated vertices for overlays, attachments and fallback stages.
pub(crate) fn submit(meshes: &[Option<Mesh>]) {
    STORE.with(|store| {
        let store = store.borrow();
        let store = store.as_ref().unwrap();
        assert_eq!(store.pending, 0);
        if store.layers.is_empty() {
            for mesh in meshes.iter().flatten() { crate::render_fx::skin(mesh); }
            return;
        }
        let input = unsafe { &*store.input.get() };
        let draw = &input.draws[store.cursor - 1];
        for surface in &input.surfaces[draw.surfaces.clone()] {
            crate::render_fx::skin_produced(meshes[surface.mesh].as_ref().unwrap(),
                actor_vertices::Publication { revision: surface.revision,
                    vertices: &store.layers[surface.vertices.clone()], indices: &surface.cpu.indices });
        }
    });
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
                            if !store.layers.is_empty() {
                                let input = unsafe { &*store.input.get() };
                                let draw = &input.draws[store.cursor - 1];
                                for surface in &input.surfaces[draw.surfaces.clone()] {
                                    for (source, prepared) in meshes[surface.mesh].as_ref().unwrap().vertices.iter()
                                        .zip(&store.layers[surface.vertices.clone()]) {
                                        for variant in 0..2 {
                                            anyhow::ensure!(actor_vertices::equal_written(
                                                &actor_vertices::normalize(*source,variant==1),&prepared[variant]),
                                                "worker layer output differs from serial");
                                        }
                                    }
                                }
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

/// Actual-data direct destinations preserve exact legacy fields without a mesh copyback.
#[cfg(feature="vita_renderer")]
pub(crate) fn check_direct(skeleton:&Skeleton,poses:&[Vec<Transform>])->anyhow::Result<usize> {
    let mut count=0;
    for workers in [1,2] {for pose in poses {for vertex_alpha in [false,true] {for fullbright in [false,true] {
        let mut meshes:Vec<_>=skeleton.surfaces.iter().map(|s|Some(Mesh {
            vertices:s.cpu().bind_vertices.clone(),indices:s.indices.clone(),texture:None})).collect();
        for mesh in meshes.iter_mut().flatten() {if let Some(v)=mesh.vertices.first_mut() {v.uv=Vec2::new(-0.,7.);}}
        let mut expected:Vec<_>=meshes.iter().flatten().map(crate::render_fx::copy_mesh).collect();
        let palette=super::palette(pose,&[]);
        let p=Params {pose,scales:&[],palette:&palette,transform:Transform {
            translation:vec3(10.,20.,30.),rotation:Quat::from_rotation_z(0.731)},scale:-0.75,flat:false,fullbright,alpha:0.731};
        for (surface,mesh) in skeleton.surfaces.iter().zip(&mut expected) {
            super::kernel(&mut super::Job {input:&surface.vertices,vertices:&mut mesh.vertices,indices:&mesh.indices},p);
            for v in &mut mesh.vertices {*v=actor_vertices::normalize(*v,vertex_alpha);}
        }
        let mut store=Store::new(workers,false);store.capture(skeleton,&meshes,p,true);
        anyhow::ensure!(!store.overflow,"direct actual data exceeds bounds");
        unsafe {let i=&mut *store.input.get();for (surface,mesh) in i.surfaces.iter_mut().zip(meshes.iter_mut().flatten()) {
            surface.output=mesh.vertices.as_mut_ptr() as usize;surface.alpha_variant=vertex_alpha;
        }}
        store.dispatch();anyhow::ensure!(store.drain().is_none(),"direct numeric worker panic");
        for (a,b) in expected.iter().zip(meshes.iter().flatten()) {anyhow::ensure!(same_vertices(&a.vertices,&b.vertices)&&a.indices==b.indices,"direct actual-data output differs");}
        count+=1;
    }}}}
    Ok(count)
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
    #[cfg(feature="vita_renderer")]
    #[test]
    fn surface_revisions_track_exact_dependencies_without_unrelated_bones_or_allocations() {
        let (s,meshes,mut pose)=fixture();pose.push(pose[0]);
        for palette_on in [false,true] {for scaled in [false,true] {
            let scales=if scaled {vec![1.,2.]} else {vec![]};
            let palette=if palette_on {super::super::palette(&pose,&scales)} else {vec![]};
            let p=Params {scales:&scales,palette:&palette,..params(&pose)};
            let mut store=Store::new(1,false);store.capture(&s,&meshes,p,true);
            let old=unsafe {&*store.input.get()};let d=&old.draws[0];
            let mut other_pose=pose.clone();other_pose[1].translation.x+=10.;
            let other_palette=if palette_on {super::super::palette(&other_pose,&scales)} else {vec![]};
            let other=Params {pose:&other_pose,palette:&other_palette,..p};
            let scope=crate::kernel_alloc::Scope::begin();
            for _ in 0..128 {assert!(same_surface_inputs(&s.surfaces[0],old,d,other));}
            assert_eq!(scope.finish(),0);
            other_pose[0].translation.x+=1.;
            let changed_palette=if palette_on {super::super::palette(&other_pose,&scales)} else {vec![]};
            assert!(!same_surface_inputs(&s.surfaces[0],old,d,Params {pose:&other_pose,palette:&changed_palette,..p}));
            if scaled {
                let changed=[2.,2.];
                let changed_palette=if palette_on {super::super::palette(&pose,&changed)} else {vec![]};
                assert!(!same_surface_inputs(&s.surfaces[0],old,d,Params {scales:&changed,palette:&changed_palette,..p}));
            }
        }}
        let surface=crate::skeletal::Surface::new("zero-weight".into(),vec![crate::skeletal::SkinVertex {
            uv:Vec2::ZERO,weights:vec![Weight {bone:0,amount:1.,offset:Vec3::ZERO},
                Weight {bone:1,amount:0.,offset:Vec3::ZERO}]}],vec![0,0,0]);
        assert_eq!(surface.used_bones,[0,1]);
        let palette=vec![(Mat3::IDENTITY,Vec3::ZERO);2];
        let p=Params {palette:&palette,..params(&pose)};
        let mut store=Store::new(1,false);store.capture(&s,&meshes,p,true);
        let old=unsafe {&*store.input.get()};let d=&old.draws[0];
        let mut signed=palette.clone();signed[0].1.x=-0.;
        assert!(!same_surface_inputs(&s.surfaces[0],old,d,Params {palette:&signed,..p}));
        signed=palette.clone();signed[1].1.x=-0.;
        assert!(same_surface_inputs(&s.surfaces[0],old,d,Params {palette:&signed,..p}));
        assert!(!same_surface_inputs(&surface,old,d,Params {palette:&signed,..p}));
    }
    #[cfg(feature="vita_renderer")]
    #[test]
    fn layer_revisions_cover_frozen_changed_and_unconsumed_outputs_and_new_identity() {
        let (s,mut meshes,pose)=fixture();let p=params(&pose);
        let mut store=Store::new(2,false);
        // Exercise opt-in revision storage independently of the selected default.
        let zero = Vertex { position: Vec3::ZERO, uv: Vec2::ZERO, normal: Vec4::ZERO, color: [0;4] };
        store.layers = vec![[zero;2]; VERTICES];
        store.dirty = vec![[false;2]; SURFACES];
        store.previous = Vec::with_capacity(SURFACES);
        let run=|store:&mut Store,p| {
            store.reset();store.capture(&s,&meshes,p,true);store.dispatch();assert!(store.drain().is_none());
            unsafe {(&*store.input.get()).surfaces[0].revision}
        };
        let a=run(&mut store,p);let b=run(&mut store,p);assert_eq!(a,b);
        // Changed output is not consumed; the following frozen frame still sees
        // its new revision rather than incorrectly reusing an older publication.
        let moved=Params {scale:2.,..p};let c=run(&mut store,moved);
        assert_eq!(c.identity,b.identity);assert_ne!(c.vertices,b.vertices);
        assert_eq!(run(&mut store,moved),c);
        assert_eq!(store.consume(&s,&mut meshes,moved),Some(true));
        let (other,other_meshes,_)=fixture();store.reset();
        store.capture(&other,&other_meshes,p,true);store.dispatch();assert!(store.drain().is_none());
        assert_ne!(unsafe {(&*store.input.get()).surfaces[0].revision.identity},a.identity);
        store.reset();store.capture(&s,&meshes,p,false);store.dispatch();assert!(store.drain().is_none());
        assert!(unsafe {(&*store.input.get()).surfaces.is_empty()});
    }
    #[cfg(feature="vita_renderer")]
    #[test]
    fn exact_workers_write_owned_publication_without_mutating_live_mesh() {
        let (s,meshes,pose)=fixture();
        for workers in [1,2] {for alpha in [false,true] {for bright in [false,true] {
            let p=Params {fullbright:bright,..params(&pose)};
            let mut store=Store::new(workers,false);
            let mut owned=crate::render_fx::copy_mesh(meshes[0].as_ref().unwrap());
            owned.vertices[0].uv=Vec2::new(-0.,7.);
            let mut reference=owned.vertices.clone();
            super::super::kernel(&mut super::super::Job {input:&s.surfaces[0].vertices,vertices:&mut reference,indices:&s.surfaces[0].indices},p);
            for v in &mut reference {*v=actor_vertices::normalize(*v,alpha);}
            store.capture(&s,&meshes,p,true);
            unsafe {let input=&mut *store.input.get();input.surfaces[0].output=owned.vertices.as_mut_ptr() as usize;input.surfaces[0].alpha_variant=alpha;}
            store.dispatch();assert!(store.drain().is_none());
            assert!(same_vertices(&reference,&owned.vertices));
            assert_eq!(meshes[0].as_ref().unwrap().vertices[0].position,Vec3::ZERO);
        }}}
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
    #[test]
    fn serial_fallback_matches_the_reference_kernel() {
        let (skeleton, mut meshes, pose) = fixture();
        let p = params(&pose);
        let mut reference: Vec<Option<Mesh>> = meshes.iter().map(|mesh| mesh.as_ref().map(|mesh| Mesh {
            vertices: mesh.vertices.clone(),
            indices: mesh.indices.clone(),
            texture: None,
        })).collect();
        for (surface, mesh) in skeleton.surfaces.iter().zip(reference.iter_mut().flatten()) {
            super::super::kernel(&mut super::super::Job {
                input: &surface.vertices,
                vertices: &mut mesh.vertices,
                indices: &mesh.indices,
            }, p);
        }
        for mesh in meshes.iter_mut().flatten() {
            for vertex in &mut mesh.vertices {
                vertex.position = Vec3::splat(99.);
                vertex.normal = Vec4::ZERO;
                vertex.color = [0; 4];
            }
        }
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                PHASE.with(|phase| phase.set(Phase::Idle));
                STORE.with(|slot| *slot.borrow_mut() = None);
            }
        }
        let _guard = Guard;
        PHASE.with(|phase| phase.set(Phase::Gather));
        STORE.with(|slot| *slot.borrow_mut() = Some(Store::new(1, false)));
        STORE.with(|slot| slot.borrow_mut().as_mut().unwrap().capture(&skeleton, &meshes, p, true));
        assert!(restore_serial_live(&skeleton, &mut meshes));
        for (expected, actual) in reference.iter().flatten().zip(meshes.iter().flatten()) {
            assert_eq!(expected.vertices.len(), actual.vertices.len());
            for (left, right) in expected.vertices.iter().zip(&actual.vertices) {
                assert_eq!(left.position.to_array().map(f32::to_bits), right.position.to_array().map(f32::to_bits));
                assert_eq!(left.normal.to_array().map(f32::to_bits), right.normal.to_array().map(f32::to_bits));
                assert_eq!(left.color, right.color);
                assert_eq!(left.uv.to_array().map(f32::to_bits), right.uv.to_array().map(f32::to_bits));
            }
            assert_eq!(expected.indices, actual.indices);
        }
        let before = serial_fallbacks();
        let after = note_serial_fallback();
        assert!(after > before);
    }
    #[test]
    fn release_fallback_skins_the_current_pose_when_no_draw_was_gathered() {
        let (skeleton, mut meshes, pose) = fixture();
        let p = params(&pose);
        let mut reference: Vec<Option<Mesh>> = meshes.iter().map(|mesh| mesh.as_ref().map(|mesh| Mesh {
            vertices: mesh.vertices.clone(),
            indices: mesh.indices.clone(),
            texture: None,
        })).collect();
        for (surface, mesh) in skeleton.surfaces.iter().zip(reference.iter_mut().flatten()) {
            super::super::kernel(&mut super::super::Job {
                input: &surface.vertices,
                vertices: &mut mesh.vertices,
                indices: &mesh.indices,
            }, p);
        }
        for mesh in meshes.iter_mut().flatten() {
            for vertex in &mut mesh.vertices {
                vertex.position = Vec3::splat(99.);
                vertex.normal = Vec4::ZERO;
                vertex.color = [0; 4];
            }
        }
        assert!(!gathering());
        assert!(!restore_serial_live(&skeleton, &mut meshes));
        let current = OwnedSkin {
            pose: pose.clone(),
            scales: Vec::new(),
            transform: p.transform,
            scale: p.scale,
            fullbright: p.fullbright,
            alpha: p.alpha,
        };
        let before = serial_fallbacks();
        assert!(release_serial_fallback(&skeleton, &mut meshes, &current));
        assert!(serial_fallbacks() > before);
        for (expected, actual) in reference.iter().flatten().zip(meshes.iter().flatten()) {
            for (left, right) in expected.vertices.iter().zip(&actual.vertices) {
                assert_eq!(left.position.to_array().map(f32::to_bits), right.position.to_array().map(f32::to_bits));
                assert_eq!(left.normal.to_array().map(f32::to_bits), right.normal.to_array().map(f32::to_bits));
                assert_eq!(left.color, right.color);
            }
        }
    }
    #[test]
    fn release_fallback_prefers_this_frames_gathered_draw() {
        let (skeleton, mut meshes, pose) = fixture();
        let gathered = params(&pose);
        let other = Transform { translation: vec3(4., 5., 6.), rotation: Quat::from_rotation_z(1.2) };
        let mut reference: Vec<Option<Mesh>> = meshes.iter().map(|mesh| mesh.as_ref().map(|mesh| Mesh {
            vertices: mesh.vertices.clone(), indices: mesh.indices.clone(), texture: None,
        })).collect();
        for (surface, mesh) in skeleton.surfaces.iter().zip(reference.iter_mut().flatten()) {
            super::super::kernel(&mut super::super::Job {
                input: &surface.vertices, vertices: &mut mesh.vertices, indices: &mesh.indices,
            }, gathered);
        }
        for mesh in meshes.iter_mut().flatten() {
            for vertex in &mut mesh.vertices { vertex.position = Vec3::splat(99.); }
        }
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                PHASE.with(|phase| phase.set(Phase::Idle));
                STORE.with(|slot| *slot.borrow_mut() = None);
            }
        }
        let _guard = Guard;
        PHASE.with(|phase| phase.set(Phase::Gather));
        STORE.with(|slot| *slot.borrow_mut() = Some(Store::new(1, false)));
        STORE.with(|slot| slot.borrow_mut().as_mut().unwrap().capture(&skeleton, &meshes, gathered, true));
        let current = OwnedSkin {
            pose: vec![other], scales: Vec::new(), transform: other, scale: 3., fullbright: true, alpha: 0.2,
        };
        assert!(release_serial_fallback(&skeleton, &mut meshes, &current));
        for (expected, actual) in reference.iter().flatten().zip(meshes.iter().flatten()) {
            for (left, right) in expected.vertices.iter().zip(&actual.vertices) {
                assert_eq!(left.position.to_array().map(f32::to_bits), right.position.to_array().map(f32::to_bits));
            }
        }
    }
}
