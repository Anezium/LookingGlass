//! Same-frame CPU skin snapshots. Render handles never enter this storage.
//! Buffers are bounded and allocated at startup; overflow uses the serial draw.
#[cfg(feature="frame_total_only")]
use crate::light_profile::InactiveInstant as MetricInstant;
#[cfg(not(feature="frame_total_only"))]
use std::time::Instant as MetricInstant;
use super::{rotation, shade_arrays, Params};
use crate::render_fx::actor_vertices::{self, PublicationMode, Revision};
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
    mode: PublicationMode,
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
impl Task {
    unsafe fn run(address: usize) {
        let task = &mut *(address as *mut Task);
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
                // Raw publications keep the kernel bytes; the game thread builds layers.
                if surface.output != 0 && surface.mode != PublicationMode::Raw {
                    let vertex_alpha = surface.mode == PublicationMode::VertexAlpha;
                    for vertex in &mut *output { *vertex = actor_vertices::normalize(*vertex, vertex_alpha); }
                }
                if task.layers != 0 {
                    assert!(surface.mode != PublicationMode::Raw, "raw publication on the layer-revision path");
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
                i.surfaces.push(Surface { mesh, vertices, cpu, revision, output: 0, mode: PublicationMode::Opaque255,
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
                    crate::render_fx::actor_publication::Key {actor:actor.unwrap(),surface:Arc::as_ptr(&surface.cpu()) as usize})
            } else { None };
            #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
            crate::sim_maps::probe::prepare_allocations(prepare_alloc,direct.is_some());
            if let Some((output, mode)) = direct {
                // The direct path requires actor_layer_reuse=false, so no worker
                // layer-revision storage exists for a raw surface to enter.
                assert!(mode != PublicationMode::Raw || self.layers.is_empty(), "raw publication with layer revisions");
                crate::phase_census::skin(actor,mesh.vertices.len(),0);
                let vertices = self.output_len..self.output_len+mesh.vertices.len();
                self.output_len = vertices.end;
                i.surfaces.push(Surface { mesh:mesh_id, vertices, cpu:surface.cpu(),
                    revision:Revision {identity:0,vertices:[0;2]}, output, mode,
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
            surface.output=mesh.vertices.as_mut_ptr() as usize;surface.mode=PublicationMode::recipe(vertex_alpha);
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
            unsafe {let input=&mut *store.input.get();input.surfaces[0].output=owned.vertices.as_mut_ptr() as usize;input.surfaces[0].mode=PublicationMode::recipe(alpha);}
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

    /// actor_deferred_layers: every case runs the same frames with the flag off (serial
    /// layer fallback) and on (raw worker skin, layers built after collect) and compares
    /// the full render_fx output: group order and transparency, every pass field, texture
    /// identity, every vertex bit and colour byte, indices, puff barriers, FRAME.vertices,
    /// FRAME.dropped and vita_fx_dropped_layers. The on arm must actually defer.
    #[cfg(feature="vita_renderer")]
    mod deferred_layers {
        use super::*;
        use crate::character::skinning::identity::ActorId;
        use crate::materials::{Blend, Stage, TcMod, Wave};
        use crate::render_fx::{actor_publication as ap, test_support as ts};
        struct Model { layers: Vec<Stage>, transparent: bool, vertices: usize }
        fn layer(stage: Stage) -> Stage { Stage { images: if stage.images.is_empty() { vec!["a".into()] } else { stage.images.clone() }, ..stage } }
        fn simple(vertex_alpha: bool, blend: Blend) -> Stage { layer(Stage { vertex_alpha, blend, ..Default::default() }) }
        fn environment() -> Stage { layer(Stage { environment: true, alpha_test: 2, alpha_reference: 0.25, ..Default::default() }) }
        fn dot_alpha() -> Stage { layer(Stage { dot_alpha: Some(vec2(0.2, 0.9)), inverse_dot: true, blend: Blend::Alpha, vertex_alpha: true, depth_equal: true, ..Default::default() }) }
        fn scroll_animated() -> Stage {
            layer(Stage { mods: vec![TcMod::Scroll(vec2(0.31, -0.17)), TcMod::Rotate(23.)], images: vec!["a".into(), "b".into(), "c".into()], fps: 3.,
                rgb: Some(Wave { kind: "sin".into(), values: [1., 0.4, 0.1, 0.7] }), alpha: Some(Wave { kind: "triangle".into(), values: [0.6, 0.3, 0.2, 1.3] }),
                blend: Blend::Filter, clamp: true, identity: true, ..Default::default() })
        }
        fn vector_uv() -> Stage { layer(Stage { vector_uv: Some((vec3(0.01, 0.02, 0.), vec3(0., 0.015, 0.03))), constant_rgb: Some(vec3(0.5, 0.75, 1.)), constant_alpha: Some(0.6), blend: Blend::Add, ..Default::default() }) }
        fn full_matrix() -> Vec<Model> {
            vec![
                Model { layers: vec![environment(), simple(true, Blend::Add)], transparent: false, vertices: 6 },
                Model { layers: vec![dot_alpha(), scroll_animated()], transparent: false, vertices: 6 },
                Model { layers: vec![vector_uv()], transparent: true, vertices: 6 },
                Model { layers: vec![simple(false, Blend::Opaque), simple(false, Blend::Add)], transparent: false, vertices: 6 },
                Model { layers: vec![simple(true, Blend::Alpha), simple(false, Blend::Add)], transparent: false, vertices: 6 },
                Model { layers: vec![layer(Stage { mods: vec![TcMod::Scroll(vec2(0.5, 0.25))], ..Default::default() }), simple(true, Blend::DoubleFilter), simple(false, Blend::Add)], transparent: false, vertices: 6 },
            ]
        }
        struct Fixture { skeleton: Skeleton, mesh: Mesh }
        fn fixture(model: usize, spec: &Model) -> Fixture {
            let n = spec.vertices;
            let vertices = (0..n).map(|i| {
                let t = i as f32;
                crate::skeletal::SkinVertex { uv: vec2(0.13 * t - 0.4, 0.71 - 0.09 * t * t),
                    weights: vec![Weight { bone: 0, amount: 0.6, offset: vec3(t * 3., (t * 1.7).sin() * 9., (t * 0.9).cos() * 7.) },
                        Weight { bone: 1, amount: 0.4, offset: vec3(-t, 2. + t * 0.5, t * t * 0.3) }] }
            }).collect();
            let indices: Vec<u16> = (0..n as u16 - 2).flat_map(|i| if i % 2 == 0 { [i, i + 1, i + 2] } else { [i + 1, i, i + 2] }).collect();
            let skeleton = Skeleton { bones: vec![crate::skeletal::Bone { parent: None, name: "root".into() }, crate::skeletal::Bone { parent: Some(0), name: "b".into() }],
                surfaces: vec![crate::skeletal::Surface::new(format!("model{model}"), vertices, indices)], skin_bounds: vec![] };
            let base = ts::texture(1000 + 16 * model as u32);
            let layers = spec.layers.iter().enumerate().map(|(l, stage)| (stage.clone(),
                (0..stage.images.len()).map(|f| ts::texture(1001 + 16 * model as u32 + 4 * l as u32 + f as u32)).collect())).collect();
            // Kept alive by the Rc returned to the caller through `surfaces`.
            SURFACES_KEEP.with(|k| k.borrow_mut().push(ts::register(&base, layers, spec.transparent)));
            let cpu = skeleton.surfaces[0].cpu();
            let mut vertices = cpu.bind_vertices.clone();
            for v in &mut vertices { v.position = Vec3::splat(-77.); v.color = [9, 8, 7, 6]; }
            Fixture { mesh: Mesh { vertices, indices: cpu.indices.clone(), texture: Some(base) }, skeleton }
        }
        thread_local! { static SURFACES_KEEP: RefCell<Vec<std::rc::Rc<crate::render_fx::Surface>>> = RefCell::default(); }
        #[derive(Clone)]
        enum Step {
            Draw(usize, usize, f32), DrawShaded(usize, usize, f32, bool, bool), Hidden(usize, usize), Topology(usize, usize), Puff, ReadOnly(Vec<Step>), Opacity(f32, Vec<Step>),
            Budget(usize), Retain(usize, usize, usize), FillCapacity, WorkerPanic(usize, usize), GatherPanic,
        }
        struct Run { frames: Vec<ts::FrameSnapshot>, commits: u64, live_skinned: usize }
        fn pose(frame: usize) -> Vec<Transform> {
            let f = frame.min(1) as f32; // frames 1 and 2 share a pose: the unchanged path runs too
            vec![Transform { rotation: Quat::from_rotation_z(0.2 + 0.3 * f), translation: vec3(1., 2. + f, 3.) },
                Transform { rotation: Quat::from_rotation_x(0.4 - 0.1 * f), translation: vec3(-2., 1., 0.5 * f) }]
        }
        fn run(on: bool, limit: Option<usize>, models: &[Model], frames: &[Vec<Step>], world: bool) -> Run {
            let body = || {
                startup(2, false); reset_publications(); ts::clear(); SURFACES_KEEP.with(|k| k.borrow_mut().clear());
                let fixtures: Vec<_> = models.iter().enumerate().map(|(i, m)| fixture(i, m)).collect();
                let actors: Vec<ActorId> = (0..4).map(|_| ActorId::default()).collect();
                let mut meshes: Vec<Vec<Vec<Option<Mesh>>>> = fixtures.iter().map(|f| (0..4).map(|_| vec![Some(crate::render_fx::copy_mesh(&f.mesh))]).collect()).collect();
                let commits = ap::test_deferred_commits();
                let mut snapshots = Vec::new();
                for (index, steps) in frames.iter().enumerate() {
                    ts::begin_direct(
                        vec3((index % 5) as f32 * 0.3, -60. + (index % 4) as f32, 15. + (index % 3) as f32),
                        vec3(0.1 + (index % 2) as f32 * 0.05, 1., -0.2),
                        1.37 + 0.17 * index as f32,
                        crate::render_fx::Appearance { dissolve: (index % 8) as f32 * 0.04, ghost: index % 3 == 1, power: 0.2 + (index % 6) as f32 * 0.07 });
                    crate::render_fx::retained::host_reset();
                    let poses = pose(index);
                    let mut held = Vec::new();
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let mut frame = Frame::begin();
                        fn exec(step: &Step, fixtures: &[Fixture], meshes: &mut [Vec<Vec<Option<Mesh>>>], actors: &[ActorId], poses: &[Transform], held: &mut Vec<std::rc::Rc<crate::render_fx::prepared::Geometry>>) {
                            let p = |alpha: f32, flat: bool, fullbright: bool| Params { pose: poses, scales: &[], palette: &[], transform: Transform { rotation: Quat::from_rotation_y(0.1), translation: vec3(0., 5., 0.) },
                                scale: 1.25, flat, fullbright, alpha };
                            match step {
                                Step::Draw(m, a, alpha) => { let _s = actors[*a].scope(); assert_eq!(handle(&fixtures[*m].skeleton, &mut meshes[*m][*a], p(*alpha, false, false), true), Some(true)); }
                                Step::DrawShaded(m, a, alpha, flat, fullbright) => { let _s = actors[*a].scope(); assert_eq!(handle(&fixtures[*m].skeleton, &mut meshes[*m][*a], p(*alpha, *flat, *fullbright), true), Some(true)); }
                                Step::Hidden(m, a) => { let _s = actors[*a].scope(); assert_eq!(handle(&fixtures[*m].skeleton, &mut meshes[*m][*a], p(1., false, false), false), Some(false)); }
                                Step::Topology(m, a) => {
                                    let _s = actors[*a].scope();
                                    let mut changed: Vec<Option<Mesh>> = meshes[*m][*a].iter().map(|mesh| mesh.as_ref().map(|mesh| Mesh {
                                        vertices: mesh.vertices.clone(), indices: mesh.indices.clone(), texture: mesh.texture.clone(),
                                    })).collect();
                                    changed[0].as_mut().unwrap().indices.swap(0, 1);
                                    assert_eq!(handle(&fixtures[*m].skeleton, &mut changed, p(1., false, false), true), Some(true));
                                }
                                Step::Puff => crate::render_fx::world_puff_quad([Vertex::new(1., 2., 3., 0., 0., WHITE); 4], None, Blend::Add),
                                Step::ReadOnly(steps) => crate::render_fx::retained_depth_scope(|| for s in steps { exec(s, fixtures, meshes, actors, poses, held); }),
                                Step::Opacity(alpha, steps) => crate::render_fx::with_skin_opacity(*alpha, || for s in steps { exec(s, fixtures, meshes, actors, poses, held); }),
                                Step::Budget(left) => ts::set_frame_vertices(ts::MAX - left),
                                Step::Retain(m, a, slots) => held.extend(ap::test_retain(crate::render_fx::actor_publication::Key {
                                    actor: actors[*a], surface: Arc::as_ptr(&fixtures[*m].skeleton.surfaces[0].cpu()) as usize }, *slots)),
                                Step::FillCapacity => ap::test_fill_vertex_capacity(),
                                Step::WorkerPanic(m, a) => {
                                    // One palette entry for two bones: the worker indexes out of range.
                                    let palette = [(Mat3::IDENTITY, Vec3::ZERO)];
                                    let _s = actors[*a].scope();
                                    assert_eq!(handle(&fixtures[*m].skeleton, &mut meshes[*m][*a], Params { palette: &palette, ..p(1., false, false) }, true), Some(true));
                                }
                                Step::GatherPanic => panic!("injected game-thread failure during gather"),
                            }
                        }
                        for step in steps { exec(step, &fixtures, &mut meshes, &actors, &poses, &mut held); }
                        frame.dispatch();
                        if world {
                            // A world packet between dispatch and collect: actor groups follow it.
                            crate::render_fx::effect(&Mesh { vertices: vec![Vertex::new(0., 0., 0., 0.5, 0.5, RED); 3], indices: vec![0, 1, 2], texture: None }, Blend::Alpha);
                        }
                        frame.collect();
                        drop(frame);
                    }));
                    if result.is_err() {
                        assert_eq!(ap::test_deferred_pending(), (0, 0, 0), "stale deferred, pending or held state after unwind");
                    }
                    drop(held);
                    snapshots.push(ts::snapshot());
                    ts::end_frame();
                }
                let live_skinned = meshes.iter().flatten().flatten().flatten().filter(|m| m.vertices.iter().any(|v| v.position != Vec3::splat(-77.))).count();
                let result = Run { frames: snapshots, commits: ap::test_deferred_commits() - commits, live_skinned };
                reset_publications(); ts::clear(); SURFACES_KEEP.with(|k| k.borrow_mut().clear());
                result
            };
            ap::with_deferred_layers(on, || match limit { Some(n) => ts::with_actor_max_layers(n, body), None => body() })
        }
        /// Runs both arms, requires identical output and the exact number of deferrals.
        fn compare(limit: Option<usize>, models: &[Model], frames: &[Vec<Step>], world: bool, deferred: u64) -> (Run, Run) {
            let off = run(false, limit, models, frames, world);
            let on = run(true, limit, models, frames, world);
            assert_eq!(off.commits, 0);
            assert_eq!(on.commits, deferred, "deferred surfaces in the on arm");
            assert_eq!(off.frames.len(), on.frames.len());
            for (frame, (a, b)) in off.frames.iter().zip(&on.frames).enumerate() {
                assert!(a.groups.iter().all(|g| !g.1.is_empty()) && b.groups.iter().all(|g| !g.1.is_empty()), "empty placeholder at frame {frame}");
                assert_eq!(a, b, "flag off/on output differs at frame {frame}");
            }
            (off, on)
        }
        fn matrix_frame() -> Vec<Step> {
            use Step::*;
            vec![Draw(0, 0, 1.), Draw(1, 0, 0.731), Puff, Draw(2, 1, 1.), Draw(3, 0, 1.), ReadOnly(vec![Draw(0, 1, 0.5)]), Draw(4, 0, 0.731),
                Draw(5, 0, 1.), Opacity(0.5, vec![Draw(1, 1, 1.)]), Hidden(0, 2), Puff, Draw(3, 1, 0.2)]
        }
        #[test]
        fn deferred_layers_full_output_matches_serial_for_every_layer_kind() {
            let models = full_matrix();
            let frames = vec![matrix_frame(); 3];
            // Per frame: models 0 (twice, once depth-read-only), 1, 2 and 5 defer. Simple model 3
            // keeps the existing direct path, simple mixed-alpha model 4 and the faded draw stay serial.
            let (off, on) = compare(None, &models, &frames, true, 15);
            assert!(off.frames.iter().all(|f| f.dropped_layers == 1), "three-layer model drops one layer per frame");
            assert!(on.frames[0].groups.iter().any(|g| g.1.len() == 2 && !g.1[0].geometry), "deferred passes own their geometry");
            // Serial skins mutate the live meshes; deferred surfaces leave them untouched.
            assert!(on.live_skinned < off.live_skinned, "on arm skinned as many live meshes as off: {} vs {}", on.live_skinned, off.live_skinned);
        }
        #[test]
        fn deferred_layers_one_layer_cap_matches_serial() {
            let models = full_matrix();
            // With one layer, simple mixed-alpha model 4 becomes a uniform recipe on the existing path.
            let (off, _) = compare(Some(1), &models, &vec![matrix_frame(); 3], true, 15);
            assert!(off.frames.iter().all(|f| f.dropped_layers == 1 + 1 + 1 + 2 + 1), "{:?}", off.frames.iter().map(|f| f.dropped_layers).collect::<Vec<_>>());
        }
        #[test]
        fn zero_layer_cap_refuses_deferral_and_matches_serial() {
            compare(Some(0), &full_matrix(), &vec![vec![Step::Draw(0, 0, 1.), Step::Draw(5, 1, 1.)]; 2], false, 0);
        }
        #[test]
        fn same_key_draws_defer_on_free_slots_only_and_stay_ordered() {
            use Step::*;
            let models = full_matrix();
            let three = vec![Draw(0, 0, 1.), Puff, Draw(0, 0, 0.6), Draw(1, 1, 1.), Draw(0, 0, 0.3)];
            // Two free slots: the first two same-key draws defer, the third is serial.
            // One slot still retained by an older publication: one defers. Both retained: none.
            let frames = vec![three.clone(),
                [vec![Retain(0, 0, 1)], three.clone()].concat(),
                [vec![Retain(0, 0, 2)], three.clone()].concat(),
                three];
            compare(None, &models, &frames, true, 3 + 2 + 1 + 3);
        }
        #[test]
        fn frame_budget_and_capped_layers_refuse_before_any_counter_mutation() {
            use Step::*;
            let models = full_matrix();
            let frames = vec![
                // Exactly at the budget: model 0 (6 vertices x 2 layers) fits; capped model 5 does not.
                vec![Budget(12), Draw(0, 0, 1.), Draw(5, 0, 1.)],
                // One over: both refuse; the serial fallback drops each group once.
                vec![Budget(11), Draw(0, 0, 1.), Draw(5, 1, 1.)],
                // Two capped deferrals take both slots of model 5's bank.
                vec![Draw(5, 0, 1.), Draw(5, 0, 1.)],
                // Held refusal with capped layers: the serial fallback alone counts the drop.
                vec![Retain(5, 0, 2), Draw(5, 0, 1.), Draw(0, 0, 1.)],
            ];
            let (off, _) = compare(None, &models, &frames, false, 1 + 0 + 2 + 1);
            assert_eq!(off.frames.iter().map(|f| (f.dropped, f.dropped_layers)).collect::<Vec<_>>(), [(1, 1), (2, 1), (0, 2), (0, 1)]);
        }
        #[test]
        fn bank_capacity_refusal_with_capped_layers_matches_serial() {
            use Step::*;
            let (off, _) = compare(None, &full_matrix(), &[vec![FillCapacity, Draw(5, 0, 1.), Draw(0, 1, 1.)]], false, 0);
            assert_eq!(off.frames[0].dropped_layers, 1);
        }
        #[test]
        fn hidden_spawned_and_topology_changes_match_serial() {
            use Step::*;
            let frames = vec![
                vec![Draw(0, 0, 1.), Hidden(0, 1)],
                vec![Draw(0, 1, 1.), Topology(0, 0)],
                vec![Draw(0, 0, 1.), Draw(0, 1, 1.)],
            ];
            compare(None, &full_matrix(), &frames, true, 1 + 1 + 2);
        }
        #[test]
        fn failed_worker_or_gather_leaves_no_deferred_state_or_placeholder_for_the_next_frame() {
            use Step::*;
            let models = full_matrix();
            let normal = vec![Draw(0, 0, 1.), Draw(1, 1, 1.), Puff, Draw(3, 0, 1.)];
            let on = run(true, None, &models, &[normal.clone(), [normal.clone(), vec![WorkerPanic(2, 2)]].concat(), normal.clone(),
                [vec![Draw(0, 0, 1.)], vec![GatherPanic]].concat(), normal.clone()], true);
            let off = run(false, None, &models, &[normal.clone(), vec![], normal.clone(), vec![], normal.clone()], true);
            assert_eq!(on.commits, 2 + 3 + 2 + 1 + 2);
            for frame in [0, 2, 4] { assert_eq!(on.frames[frame], off.frames[frame], "frame {frame} after a failed frame"); }
            // The failed collect published no actor group: only the world packet remains.
            assert_eq!(on.frames[1].groups.len(), 1);
            assert!(on.frames[1].groups.iter().all(|g| !g.1.is_empty()), "worker failure left an unfilled placeholder");
            // Cleanup has already run. This snapshot is the failed view itself, before the next
            // begin_direct replaces FRAME.
            assert_eq!(on.frames[3].groups.iter().filter(|g| g.1.is_empty()).count(), 0, "gather failure left an empty placeholder");
            assert_eq!(on.frames[3].vertices, 0);
            assert_eq!(on.frames[3].dropped_layers, 0);
            assert_eq!(ap::test_deferred_pending(), (0, 0, 0));
        }
        #[test]
        fn dropped_guard_discards_unfilled_groups_and_the_next_frame_does_not_repair_them() {
            // Per frame: serial model 4, model 0 (deferred only when on), a puff, then direct
            // model 3, whose geometry would arrive in complete(). Frame 0 drops the guard during
            // gather, frame 1 after hold() and a world draw, frame 2 completes normally.
            let models = full_matrix();
            let go = |on: bool| -> Vec<ts::FrameSnapshot> {
                startup(2, false); reset_publications(); ts::clear(); SURFACES_KEEP.with(|k| k.borrow_mut().clear());
                let fixtures: Vec<_> = models.iter().enumerate().map(|(i, m)| fixture(i, m)).collect();
                let actors: Vec<_> = (0..3).map(|_| ActorId::default()).collect();
                let mut meshes: Vec<Vec<Vec<Option<Mesh>>>> = fixtures.iter()
                    .map(|f| actors.iter().map(|_| vec![Some(crate::render_fx::copy_mesh(&f.mesh))]).collect()).collect();
                let snapshots = ap::with_deferred_layers(on, || {
                    let mut snapshots = Vec::new();
                    for index in 0..3 {
                        let pose = pose(index);
                        ts::begin_direct(vec3(index as f32, -60., 15.), Vec3::Y, 1.5 + index as f32,
                            crate::render_fx::Appearance { dissolve: 0.2, ghost: index == 1, power: 0.4 });
                        crate::render_fx::retained::host_reset();
                        {
                            let mut frame = Frame::begin();
                            for (model, actor) in [(4, 0), (0, 1), (usize::MAX, 0), (3, 2)] {
                                if model == usize::MAX {
                                    crate::render_fx::world_puff_quad([Vertex::new(1., 2., 3., 0., 0., WHITE); 4], None, Blend::Add);
                                    continue;
                                }
                                let _s = actors[actor].scope();
                                let params = Params { pose: &pose, scales: &[], palette: &[], transform: Transform { rotation: Quat::from_rotation_y(0.1), translation: vec3(0., 5., 0.) },
                                    scale: 1.25, flat: false, fullbright: false, alpha: 1. };
                                assert_eq!(handle(&fixtures[model].skeleton, &mut meshes[model][actor], params, true), Some(true));
                            }
                            if index > 0 {
                                frame.dispatch();
                                crate::render_fx::effect(&Mesh { vertices: vec![Vertex::new(4., 5., 6., 0.2, 0.3, RED); 3], indices: vec![0, 1, 2], texture: None }, Blend::Alpha);
                            }
                            if index == 2 { frame.collect(); }
                            drop(frame);
                        }
                        // Read before the next begin_direct replaces FRAME.
                        assert_eq!(ap::test_deferred_pending(), (0, 0, 0), "frame {index}");
                        snapshots.push(ts::snapshot());
                    }
                    snapshots
                });
                reset_publications(); ts::clear(); SURFACES_KEEP.with(|k| k.borrow_mut().clear());
                snapshots
            };
            let off = go(false);
            let on = go(true);
            let unfilled = |frame: &ts::FrameSnapshot| frame.groups.iter().filter(|g| g.1.is_empty() || g.1.iter().any(|p| !p.geometry && p.vertices.is_empty())).count();
            for (index, frame) in on.iter().chain(&off).enumerate() { assert_eq!(unfilled(frame), 0, "unfilled group after cleanup, snapshot {index}: {frame:?}"); }
            // Frame 0: off keeps serial models 4 and 0; on keeps model 4 only. The puff index
            // moves back over the discarded placeholder, and its 12 vertices leave FRAME.
            let [off0, off1, off2]: [ts::FrameSnapshot; 3] = off.try_into().unwrap();
            let [on0, on1, on2]: [ts::FrameSnapshot; 3] = on.try_into().unwrap();
            assert_eq!(off0.groups.len(), 2);
            assert_eq!(off0.puffs.iter().map(|p| p.0).collect::<Vec<_>>(), [2]);
            let mut expect = off0;
            expect.groups.remove(1);
            expect.puffs[0].0 -= 1;
            expect.vertices -= 12;
            assert_eq!(on0, expect, "failed gather cleanup");
            // Frame 1: the world group drawn after hold comes first, then the finished held
            // groups in complete() order; the placeholder and the direct group are gone.
            assert_eq!(off1.groups.len(), 3);
            assert_eq!(off1.puffs.iter().map(|p| p.0).collect::<Vec<_>>(), [3]);
            let mut expect = off1;
            expect.groups.remove(2);
            expect.puffs[0].0 -= 1;
            expect.vertices -= 12;
            assert_eq!(on1, expect, "failed held-view cleanup");
            // Frame 2 runs the full path in both arms and must match exactly.
            assert_eq!(on2.groups.len(), 4);
            assert_eq!(on2, off2, "the frame after two failed views");
            assert!(on2.groups.iter().any(|g| g.1.len() == 2 && !g.1[0].geometry), "frame 2 did not defer model 0");
        }
        #[test]
        fn every_alpha_reaches_full_layers_for_each_non_simple_kind() {
            use Step::*;
            let models = vec![
                Model { layers: vec![environment()], transparent: false, vertices: 6 },
                Model { layers: vec![dot_alpha()], transparent: false, vertices: 6 },
                Model { layers: vec![scroll_animated()], transparent: false, vertices: 6 },
                Model { layers: vec![vector_uv()], transparent: true, vertices: 6 },
                Model { layers: vec![environment(), simple(true, Blend::Add)], transparent: false, vertices: 6 },
            ];
            assert!(models[4].layers.iter().any(|stage| stage.vertex_alpha), "the exhaustive fixture must include vertex alpha");
            assert!(models[4].layers.iter().any(|stage| !stage.vertex_alpha), "the exhaustive fixture must mix both alpha policies");
            let frames: Vec<Vec<Step>> = (0..=255).map(|alpha| {
                let byte = alpha as f32 / 255.;
                let flat = alpha % 2 == 0;
                let fullbright = alpha % 3 == 0;
                vec![
                    DrawShaded(0, 0, byte, flat, fullbright),
                    DrawShaded(1, 1, byte, flat, fullbright),
                    DrawShaded(2, 2, byte, flat, fullbright),
                    DrawShaded(3, 3, byte, flat, fullbright),
                    DrawShaded(4, 0, byte, flat, fullbright),
                ]
            }).collect();
            let (off, on) = compare(None, &models, &frames, true, 256 * 5);
            assert!(off.frames.iter().zip(&on.frames).all(|(a, b)| a == b));
            assert!(on.frames.iter().any(|frame| frame.groups.iter().any(|group| group.1.iter().any(|pass|
                pass.vertices.iter().any(|vertex| vertex.2 != [0, 0, 0, 0] || vertex.1 != [0, 0])))));
        }
        #[test]
        fn live_uv_edits_and_raw_normalized_raw_advance_the_same_slot_stamp() {
            let models = vec![Model { layers: vec![simple(false, Blend::Opaque), environment()], transparent: false, vertices: 6 }];
            startup(2, false); reset_publications(); ts::clear(); SURFACES_KEEP.with(|k| k.borrow_mut().clear());
            let fixture = fixture(0, &models[0]);
            let actor = ActorId::default();
            let key = ap::Key { actor, surface: Arc::as_ptr(&fixture.skeleton.surfaces[0].cpu()) as usize };
            let mut mesh = vec![Some(crate::render_fx::copy_mesh(&fixture.mesh))];
            let pose = pose(1);
            let params = Params { pose: &pose, scales: &[], palette: &[], transform: Transform { rotation: Quat::from_rotation_y(0.1), translation: vec3(0., 5., 0.) },
                scale: 1.25, flat: false, fullbright: false, alpha: 1. };
            let draw = |mesh: &mut [Option<Mesh>], cap: usize| {
                ts::set_actor_max_layers(Some(cap));
                ts::begin_direct(vec3(0., -60., 15.), Vec3::Y, 4.5, crate::render_fx::Appearance { dissolve: 0.2, ghost: false, power: 0.5 });
                crate::render_fx::retained::host_reset();
                let mut frame = Frame::begin();
                { let _s = actor.scope(); assert_eq!(handle(&fixture.skeleton, mesh, params, true), Some(true)); }
                frame.dispatch();
                frame.collect();
                ts::snapshot()
            };
            ap::with_deferred_layers(true, || {
                let raw = draw(&mut mesh, 2);
                let raw_stamp = ap::test_stamp(key).unwrap();
                assert_eq!(raw_stamp.1, PublicationMode::Raw);
                assert!(raw.groups.iter().any(|g| g.1.len() == 2));
                let uv = mesh[0].as_ref().unwrap().vertices[0].uv;
                mesh[0].as_mut().unwrap().vertices[0].uv = Vec2::new(uv.x + 0.37, -0.2);
                let edited = draw(&mut mesh, 2);
                let edited_stamp = ap::test_stamp(key).unwrap();
                assert_eq!(edited_stamp.1, PublicationMode::Raw);
                assert_ne!(edited_stamp.0, raw_stamp.0, "a live UV edit with the same pose kept the upload stamp");
                let edited_uv = mesh[0].as_ref().unwrap().vertices[0].uv.to_array().map(f32::to_bits);
                assert!(edited.groups.iter().flat_map(|g| &g.1).any(|pass| pass.vertices.iter().any(|v| v.1 == edited_uv)));
                let normalized = draw(&mut mesh, 1);
                let normalized_stamp = ap::test_stamp(key).unwrap();
                assert_ne!(normalized_stamp.1, PublicationMode::Raw);
                assert_ne!(normalized_stamp.0, edited_stamp.0, "Raw to normalized kept the upload stamp");
                assert!(normalized.groups.iter().all(|g| !g.1.is_empty()));
                let raw_again = draw(&mut mesh, 2);
                let raw_again_stamp = ap::test_stamp(key).unwrap();
                assert_eq!(raw_again_stamp.1, PublicationMode::Raw);
                assert_ne!(raw_again_stamp.0, normalized_stamp.0, "normalized to Raw kept the upload stamp");
                assert!(raw_again.groups.iter().any(|g| g.1.len() == 2));
                ts::set_actor_max_layers(None);
            });
            reset_publications(); ts::clear(); SURFACES_KEEP.with(|k| k.borrow_mut().clear());
        }
        /// One unique surface per call: a new bank key every draw. Two layers, so a one-layer
        /// cap drops one layer per draw in both arms.
        fn custom_surface(model: usize, vertex_count: usize, indices: Vec<u16>) -> Fixture {
            let layers = [environment(), simple(false, Blend::Add)];
            let vertices = (0..vertex_count).map(|i| {
                let t = i as f32;
                crate::skeletal::SkinVertex { uv: vec2(0.13 * t - 0.4, 0.71 - 0.02 * t),
                    weights: vec![Weight { bone: 0, amount: 0.6, offset: vec3(t, (t * 0.1).sin(), 1.) },
                        Weight { bone: 1, amount: 0.4, offset: vec3(-t * 0.2, 1., t * 0.05) }] }
            }).collect();
            let skeleton = Skeleton { bones: vec![crate::skeletal::Bone { parent: None, name: "root".into() }, crate::skeletal::Bone { parent: Some(0), name: "b".into() }],
                surfaces: vec![crate::skeletal::Surface::new(format!("cap{model}"), vertices, indices)], skin_bounds: vec![] };
            let base = ts::texture(100_000 + 4 * model as u32);
            let registered = layers.iter().enumerate().map(|(l, stage)| (stage.clone(), vec![ts::texture(100_001 + 4 * model as u32 + l as u32)])).collect();
            SURFACES_KEEP.with(|k| k.borrow_mut().push(ts::register(&base, registered, false)));
            let cpu = skeleton.surfaces[0].cpu();
            let mut vertices = cpu.bind_vertices.clone();
            for v in &mut vertices { v.position = Vec3::splat(-77.); v.color = [9, 8, 7, 6]; }
            Fixture { mesh: Mesh { vertices, indices: cpu.indices.clone(), texture: Some(base) }, skeleton }
        }
        struct Unique { commits: u64, frames: Vec<ts::FrameSnapshot>, capacities: Vec<(usize, usize)> }
        /// Real draws through prepare, capture, workers, drain and complete with a one-layer cap.
        /// Each frame lists the vertex counts of its unique surfaces. Capacities are the bank
        /// vertex/index totals after each collect.
        fn drive_unique(on: bool, frames: &[Vec<usize>], indices: &[u16]) -> Unique {
            startup(2, false); reset_publications(); ts::clear(); SURFACES_KEEP.with(|k| k.borrow_mut().clear());
            let mut model = 0usize;
            let mut snaps = Vec::new();
            let mut capacities = Vec::new();
            let commits = ap::with_deferred_layers(on, || ts::with_actor_max_layers(1, || {
                let before = ap::test_deferred_commits();
                for sizes in frames {
                    ts::begin_direct(vec3(0., -20., 8.), Vec3::Y, 1.2 + model as f32, crate::render_fx::Appearance::default());
                    crate::render_fx::retained::host_reset();
                    let mut frame = Frame::begin();
                    for &vertex_count in sizes {
                        let fixture = custom_surface(model, vertex_count, indices.to_vec());
                        let actor = ActorId::default();
                        let mut mesh = vec![Some(crate::render_fx::copy_mesh(&fixture.mesh))];
                        let pose = pose(1);
                        { let _s = actor.scope();
                            assert_eq!(handle(&fixture.skeleton, &mut mesh, Params { pose: &pose, scales: &[], palette: &[],
                                transform: Transform { rotation: Quat::IDENTITY, translation: Vec3::ZERO }, scale: 1., flat: false, fullbright: false, alpha: 1. }, true), Some(true)); }
                        model += 1;
                    }
                    frame.dispatch();
                    frame.collect();
                    drop(frame);
                    snaps.push(ts::snapshot());
                    capacities.push(ap::test_capacities());
                    ts::end_frame();
                }
                ap::test_deferred_commits() - before
            }));
            reset_publications(); ts::clear(); SURFACES_KEEP.with(|k| k.borrow_mut().clear());
            Unique { commits, frames: snaps, capacities }
        }
        #[test]
        fn populated_bank_capacity_and_held_slots_refuse_capped_draws_once() {
            use Step::*;
            let models = full_matrix();
            // Held slots populated by real draws in the previous frame, with a capped model.
            let held = vec![
                vec![Draw(5, 0, 1.), Draw(5, 0, 1.)],
                vec![Retain(5, 0, 2), Draw(5, 0, 1.), Draw(0, 1, 1.)],
            ];
            let (off, on) = compare(None, &models, &held, false, 2 + 1);
            assert_eq!(off.frames.iter().map(|f| f.dropped_layers).collect::<Vec<_>>(), vec![2, 1]);
            assert_eq!(on.frames.iter().map(|f| f.dropped_layers).collect::<Vec<_>>(), vec![2, 1]);
            // Vertex table: frame 0 fills 64 x 512 = 32768 real vertices; frame 1 adds 64 more
            // (65536 total) and then a 65th surface. Nothing is evictable yet (banks used in
            // the current or previous epoch cannot be reclaimed), so that draw hits the real
            // vertex refusal and completes serially. No test-only capacity assignment.
            let indices = vec![0u16, 1, 2];
            let mut second: Vec<usize> = vec![512; 64];
            second.push(768);
            let vertex_off = drive_unique(false, &[vec![512; 64], second.clone()], &indices);
            let vertex_on = drive_unique(true, &[vec![512; 64], second], &indices);
            assert_eq!(vertex_off.commits, 0);
            assert_eq!(vertex_on.commits, 128, "the 129th draw at real vertex capacity must refuse");
            assert_eq!(vertex_on.capacities.iter().map(|c| c.0).collect::<Vec<_>>(), [32768, 65536]);
            assert_eq!(vertex_off.frames, vertex_on.frames, "populated vertex table output differed from serial");
            assert_eq!(vertex_on.frames.iter().map(|f| f.dropped_layers).collect::<Vec<_>>(), [64, 65]);
            // Index table: 8000 indices per surface fill 256000 of 262144 at 32 surfaces. The
            // 33rd shares the frame, nothing is old enough to evict, and it refuses to serial
            // with its capped layer counted once, by the fallback.
            let fat: Vec<u16> = (0..8000).map(|i| (i % 3) as u16).collect();
            let index_off = drive_unique(false, &[vec![3; 40]], &fat);
            let index_on = drive_unique(true, &[vec![3; 40]], &fat);
            assert_eq!(index_off.commits, 0);
            assert_eq!(index_on.commits, 32, "index table accepted {}", index_on.commits);
            assert_eq!(index_on.capacities, [(32 * 3, 32 * 8000)]);
            assert_eq!(index_off.frames, index_on.frames, "index-cap refusal output differed from serial");
            assert_eq!(index_on.frames[0].groups.len(), 40);
            assert!(index_on.frames[0].groups.iter().all(|group| !group.1.is_empty()));
            assert_eq!((index_on.frames[0].dropped_layers, index_off.frames[0].dropped_layers), (40, 40));
        }
        #[test]
        fn raw_mode_keeps_every_kernel_byte_for_all_256_alphas() {
            let (s, meshes, pose) = super::fixture();
            let mut opaque_differs = 0;
            for workers in [1, 2] { for flat in [false, true] { for fullbright in [false, true] { for a in 0..=255u8 {
                let p = Params { flat, fullbright, alpha: a as f32 / 255., ..params(&pose) };
                let mut store = Store::new(workers, false);
                let mut owned: Vec<_> = (0..3).map(|_| { let mut m = crate::render_fx::copy_mesh(meshes[0].as_ref().unwrap()); m.vertices[0].uv = Vec2::new(-0., 7.); m }).collect();
                let mut reference = owned[0].vertices.clone();
                super::super::super::kernel(&mut super::super::super::Job { input: &s.surfaces[0].vertices, vertices: &mut reference, indices: &s.surfaces[0].indices }, p);
                for _ in 0..3 { store.capture(&s, &meshes, p, true); }
                let modes = [PublicationMode::Raw, PublicationMode::VertexAlpha, PublicationMode::Opaque255];
                unsafe { let input = &mut *store.input.get(); for (i, mode) in modes.into_iter().enumerate() {
                    input.surfaces[i].output = owned[i].vertices.as_mut_ptr() as usize; input.surfaces[i].mode = mode; } }
                store.dispatch(); assert!(store.drain().is_none());
                assert!(same_vertices(&reference, &owned[0].vertices), "raw bytes differ at alpha {a}");
                let normalized = |va: bool| reference.iter().map(|v| actor_vertices::normalize(*v, va)).collect::<Vec<_>>();
                assert!(same_vertices(&normalized(true), &owned[1].vertices));
                assert!(same_vertices(&normalized(false), &owned[2].vertices));
                opaque_differs += usize::from(owned[2].vertices[0].color[3] != reference[0].color[3]);
            }}}}
            assert!(opaque_differs > 0, "opaque255 matched the kernel for every alpha; raw was not distinguished");
        }
        #[test]
        fn worker_inputs_stay_numeric() {
            fn send_copy<T: Send + Copy>() {}
            send_copy::<PublicationMode>();
            fn send<T: Send>() {}
            send::<Inputs>();
        }
    }
}
