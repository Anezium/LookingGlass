//! One transparent queue for world, skin, pickup and effect geometry.
//! Packets own their uniforms: shared pipeline state never leaks across actors.
use crate::{
    assets::Assets,
    environment::Atmosphere,
    materials::{Blend, Stage},
    texture,
};
use anyhow::Result;
#[cfg(feature = "vita_renderer")]
pub(crate) mod retained;
#[cfg(feature="vita_renderer")]
mod actor_cache;
#[cfg(feature="vita_renderer")]
mod prepared;
use macroquad::prelude::*;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::{Rc, Weak},
};

#[derive(Clone, Copy, Default, PartialEq)]
pub struct Appearance {
    pub dissolve: f32,
    pub ghost: bool,
    pub power: f32,
}
pub struct Pass {
    _game_thread: crate::game_thread::GameThread,
    #[cfg(feature="vita_renderer")]
    geometry:Option<Rc<prepared::Geometry>>,
    #[cfg(feature="vita_renderer")]
    effect:bool,
    #[cfg(feature = "vita_renderer")]
    blend_index: usize,
    #[cfg(feature = "vita_renderer")]
    depth_equal: bool,
    #[cfg(target_os = "vita")]
    pub world_static: Option<bool>,
    #[cfg(target_os = "vita")]
    world_geometry_counted: bool,
    pub mesh: Mesh,
    pub material: Option<Rc<Material>>,
    pub lit: bool,
    pub clamp: bool,
    pub cutout: f32,
    pub cutout_reference: f32,
    pub gain: Vec4,
    pub fog: f32,
    pub model: bool,
    pub appearance: Appearance,
    pub filter_fade: f32,
}
#[cfg(feature="vita_renderer")]
impl Pass {
    fn geometry(&self)->&Mesh { self.geometry.as_ref().map_or(&self.mesh, |g| &g.mesh) }
    fn own_geometry(&mut self) {
        if let Some(geometry)=self.geometry.take() {
            let texture=self.mesh.texture.take();
            self.mesh=Rc::try_unwrap(geometry).map(|g|g.mesh).unwrap_or_else(|m|copy_mesh(&m));
            self.mesh.texture=texture;
        }
    }
}
struct Group {
    #[cfg(feature = "vita_renderer")]
    transparent: bool,
    passes: Vec<Pass>,
}
#[derive(Default)]
struct Frame {
    active: bool,
    #[cfg(feature = "vita_renderer")]
    direct: bool,
    #[cfg(feature = "vita_renderer")]
    depth_read_only: bool,
    camera: Vec3,
    direction: Vec3,
    time: f32,
    atmosphere: Atmosphere,
    fullbright: bool,
    appearance: Appearance,
    groups: Vec<Group>,
    vertices: usize,
    dropped: usize,
    retained: Vec<(Rc<Material>, Option<Texture2D>)>,
    projection: Option<Mat4>,
}
thread_local! {
    static FRAME: RefCell<Frame> = RefCell::default();
    static SKIN_OPACITY: std::cell::Cell<f32> = const { std::cell::Cell::new(1.) };
    static SURFACES: RefCell<Vec<(macroquad::miniquad::TextureId, Weak<Surface>)>> = RefCell::default();
    #[cfg(feature="vita_renderer")]
    static SURFACE_LOOKUP:RefCell<std::collections::HashMap<macroquad::miniquad::TextureId,Weak<Surface>>>=RefCell::default();
}
const MAX_VERTICES: usize = 1_000_000;
pub fn copy_mesh(m: &Mesh) -> Mesh {
    Mesh {
        vertices: m.vertices.clone(),
        indices: m.indices.clone(),
        texture: m.texture.clone(),
    }
}
pub fn active() -> bool {
    FRAME.with(|f| f.borrow().active)
}
pub(crate) fn retained_active() -> bool {
    #[cfg(feature = "vita_renderer")]
    {
        FRAME.with(|f| {
            let f = f.borrow();
            f.active && f.direct
        })
    }
    #[cfg(not(feature = "vita_renderer"))]
    {
        false
    }
}
#[cfg(feature = "vita_renderer")]
pub(crate) fn group_depth(center: Vec3) -> f32 {
    FRAME.with(|f| {
        let f = f.borrow();
        (center - f.camera).dot(f.direction)
    })
}
/// Deferred packets carry depth policy; no GL state change surrounds the closure.
#[cfg(feature = "vita_renderer")]
pub(crate) fn retained_depth_scope(draw: impl FnOnce()) {
    let old = FRAME.with(|f| {
        let mut f = f.borrow_mut();
        let old = f.depth_read_only;
        f.depth_read_only = true;
        old
    });
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            FRAME.with(|f| f.borrow_mut().depth_read_only = self.0);
        }
    }
    let _restore = Restore(old);
    draw();
}
pub fn time() -> f32 {
    FRAME.with(|f| f.borrow().time)
}
pub fn projection() -> Option<Mat4> {
    FRAME.with(|f| f.borrow().projection)
}
/// Use the actual view matrix for conservative geometry and effect rejection,
/// including reflected cameras with an oblique aperture plane.
pub(crate) fn set_projection(matrix: Mat4) {
    FRAME.with(|f| f.borrow_mut().projection = Some(matrix));
}
pub fn projected_error(center: Vec3, radius: f32, error: f32) -> f32 {
    FRAME.with(|f| {
        let f = f.borrow();
        let Some(m) = f.projection else {
            return f32::INFINITY;
        };
        projected_error_for(m, screen_height(), center, radius, error)
    })
}
pub(crate) fn projected_error_for(
    m: Mat4,
    height: f32,
    center: Vec3,
    radius: f32,
    error: f32,
) -> f32 {
    let clip = m * center.extend(1.);
    let distance = (clip.w - radius).max(0.001);
    // Projection row magnitude also handles rolled cameras without knowing FOV.
    let row = vec3(m.x_axis.y, m.y_axis.y, m.z_axis.y).length();
    error * row * height * 0.5 / distance
}
pub fn appearance(a: Appearance) {
    FRAME.with(|f| f.borrow_mut().appearance = a);
}
pub fn begin(camera: Vec3, direction: Vec3, time: f32, atmosphere: &Atmosphere, fullbright: bool) {
    unsafe {
        {
            #[cfg(target_os = "vita")]
            crate::frame_profile::flush_boundary("flush_fx_begin");
            #[cfg(target_os = "vita")]
            let _flush_profile = crate::frame_profile::span("gl_flush");
            #[cfg(target_os = "vita")]
            let _flush_site_profile = crate::frame_profile::span("flush_fx_begin");

            get_internal_gl().flush();
        }
    }
    FRAME.with(|f| {
        *f.borrow_mut() = Frame {
            active: true,
            camera,
            direction: direction.normalize_or_zero(),
            time,
            atmosphere: atmosphere.clone(),
            fullbright,
            ..Default::default()
        }
    });
}
pub fn begin_view(camera: &Camera3D, time: f32, atmosphere: &Atmosphere, fullbright: bool) {
    crate::game_thread::assert_current();
    use macroquad::camera::Camera;
    begin(
        camera.position,
        camera.target - camera.position,
        time,
        atmosphere,
        fullbright,
    );
    set_projection(camera.matrix());
}
fn same_style(a: &Pass, b: &Pass) -> bool {
    a.material.as_ref().zip(b.material.as_ref()).is_some_and(|(a,b)| Rc::ptr_eq(a,b))
        && a.mesh.texture.as_ref().map(Texture2D::raw_miniquad_id)
            == b.mesh.texture.as_ref().map(Texture2D::raw_miniquad_id)
        && a.lit == b.lit
        && a.clamp == b.clamp
        && a.cutout == b.cutout
        && a.cutout_reference == b.cutout_reference
        && a.gain == b.gain
        && a.fog == b.fog
        && a.model == b.model
        && a.appearance == b.appearance
        && a.filter_fade == b.filter_fade
}
fn outside(points: [Vec4; 3]) -> bool {
    (0..3).any(|axis| points.iter().all(|p| p[axis] < -p.w) || points.iter().all(|p| p[axis] > p.w))
}
fn draw_pass(p: &Pass, indices: Option<&[u16]>) {
    #[cfg(target_os = "vita")]
    let _world_submit = p.world_static.map(|static_batch| crate::frame_profile::world_span("world_submit", static_batch));
    FRAME.with(|f| {
        let f = f.borrow();
        let m = p.material.as_ref().expect("legacy pass material");
        f.atmosphere.apply(m, f.camera);
        m.set_uniform("Fullbright", if f.fullbright { 1_f32 } else { 0. });
        m.set_uniform("Lit", if p.lit { 1_f32 } else { 0. });
        m.set_uniform("ClampUV", if p.clamp { 1_f32 } else { 0. });
        m.set_uniform("Cutout", p.cutout);
        m.set_uniform("CutoutReference", p.cutout_reference);
        m.set_uniform("Gain", p.gain);
        m.set_uniform("FogMode", p.fog);
        m.set_uniform("SkyFade", 0_f32);
        m.set_uniform("FxModel", if p.model { 1_f32 } else { 0. });
        m.set_uniform(
            "FxAppearance",
            vec4(
                p.appearance.dissolve,
                if p.appearance.ghost { 1. } else { 0. },
                p.appearance.power,
                p.filter_fade,
            ),
        );
        crate::lighting::apply(m, p.model, !f.fullbright && p.lit);
        gl_use_material(m);
        if let Some(indices) = indices {
            // Interleaved surfaces can contribute just one triangle per run.
            // Uploading the whole source mesh for each run is quadratic in mesh
            // size; compact this span and stay within the u16 index range.
            let gl = unsafe { get_internal_gl() };
            gl.quad_gl.texture(p.mesh.texture.as_ref());
            gl.quad_gl.draw_mode(DrawMode::Triangles);
            for chunk in indices.chunks(65532) {
                #[cfg(target_os = "vita")]
                let _compact_profile = crate::frame_profile::span("transparent_compact");
                let vertices: Vec<_> = chunk.iter().map(|&i| p.mesh.vertices[i as usize]).collect();
                let compact: Vec<u16> = (0..vertices.len() as u16).collect();
                #[cfg(target_os = "vita")]
                if let Some(static_batch) = p.world_static {
                    crate::frame_profile::record_layer(
                        compact.len() / 3,
                        vertices.len(),
                        static_batch,
                    );
                }
                #[cfg(target_os = "vita")]
                drop(_compact_profile);
                #[cfg(target_os = "vita")]
                crate::frame_profile::counts(&[
                    ("transparent_vertices_copied", vertices.len() as u64),
                    ("lg_fx_submissions", 1),
                ]);
                #[cfg(target_os = "vita")]
                crate::frame_profile::record_lg_submission();
                gl.quad_gl.geometry(&vertices, &compact);
            }
        } else {
            #[cfg(target_os = "vita")]
            if !p.world_geometry_counted && !p.mesh.indices.is_empty() {
                if let Some(static_batch) = p.world_static {
                    crate::frame_profile::record_layer(
                        p.mesh.indices.len() / 3,
                        p.mesh.vertices.len(),
                        static_batch,
                    );
                }
            }
            #[cfg(target_os = "vita")]
            crate::frame_profile::counts(&[("lg_fx_submissions", 1)]);
            #[cfg(target_os = "vita")]
            crate::frame_profile::record_lg_submission();
            draw_mesh(&p.mesh);
        }
    });
}
pub fn submit(passes: Vec<Pass>, transparent: bool) {
    crate::game_thread::assert_current();
    if passes.is_empty() {
        return;
    }
    #[cfg(feature = "vita_renderer")]
    if retained_active() {
        let c=crate::render::vita_renderer::config();
        let limit=if passes.iter().all(|p|p.effect){c.fx_max_layers}else{c.actor_max_layers};
        let mut passes = passes;
        if passes.len() > limit {
            retained::count("vita_fx_dropped_layers", (passes.len() - limit) as u64);
            passes.truncate(limit);
        }
        FRAME.with(|f| {
            let mut f = f.borrow_mut();
            let count = passes.iter().map(|p| p.geometry().vertices.len()).sum::<usize>();
            if f.vertices.saturating_add(count) > MAX_VERTICES {
                f.dropped += 1;
                return;
            }
            f.vertices += count;
            let transparent = transparent || f.depth_read_only;
            f.groups.push(Group {
                passes,
                transparent,
            });
        });
        return;
    }
    if !transparent {
        for p in &passes {
            draw_pass(p, None);
        }
        FRAME.with(|f| {
            f.borrow_mut()
                .retained
                .extend(passes.into_iter().map(|p| (p.material.expect("legacy pass material"), p.mesh.texture)))
        });
        return;
    }
    FRAME.with(|f| {
        let mut f = f.borrow_mut();
        let count = passes.iter().map(|p| p.mesh.vertices.len()).sum::<usize>();
        if f.vertices.saturating_add(count) > MAX_VERTICES {
            f.dropped += 1;
            return;
        }
        f.vertices += count;
        f.groups.push(Group {
            passes,
            #[cfg(feature = "vita_renderer")]
            transparent: true,
        });
    });
}
pub fn finish() -> (usize, usize) {
    let (groups, camera, direction, dropped, projection) = FRAME.with(|f| {
        let mut f = f.borrow_mut();
        (
            std::mem::take(&mut f.groups),
            f.camera,
            f.direction,
            f.dropped,
            f.projection,
        )
    });
    // Sorting triangles rather than whole objects fixes effects on opposite sides
    // of one water surface. All layers of a triangle stay in authored order.
    #[cfg(target_os = "vita")]
    let transparent_profile = crate::frame_profile::span("transparent_sort");
    #[cfg(target_os = "vita")]
    let _keys_profile = crate::frame_profile::span("transparent_keys");
    #[cfg(target_os = "vita")]
    let mut candidates = 0u64;
    let mut order = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        let mesh = &group.passes[0].mesh;
        for (t, triangle) in mesh.indices.chunks_exact(3).enumerate() {
            #[cfg(target_os = "vita")]
            {
                candidates += 1;
            }
            if projection.is_some_and(|m| {
                outside([
                    m * mesh.vertices[triangle[0] as usize].position.extend(1.),
                    m * mesh.vertices[triangle[1] as usize].position.extend(1.),
                    m * mesh.vertices[triangle[2] as usize].position.extend(1.),
                ])
            }) {
                continue;
            }
            let center = triangle
                .iter()
                .map(|&i| mesh.vertices[i as usize].position)
                .sum::<Vec3>()
                / 3.;
            order.push(((center - camera).dot(direction), g, t));
        }
    }
    #[cfg(target_os = "vita")]
    drop(_keys_profile);
    #[cfg(target_os = "vita")]
    crate::frame_profile::counts(&[
        ("transparent_candidates", candidates),
        ("transparent_triangles", order.len() as u64),
        ("transparent_rejected", candidates - order.len() as u64),
        ("transparent_groups", groups.len() as u64),
    ]);
    #[cfg(target_os = "vita")]
    let _order_profile = crate::frame_profile::span("transparent_order");
    order.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    #[cfg(target_os = "vita")]
    drop(_order_profile);
    crate::render::depth_read_only(|| {
        let mut start = 0;
        while start < order.len() {
            #[cfg(target_os = "vita")]
            let _pack_profile = crate::frame_profile::span("transparent_pack");
            #[cfg(target_os = "vita")]
            crate::frame_profile::counts(&[("transparent_segments", 1)]);

            let group = order[start].1;
            let mut end = start + 1;
            let passes = &groups[group].passes;
            if passes.len() == 1 {
                let p = &passes[0];
                while end < order.len()
                    && end - start < 20000
                    && groups[order[end].1].passes.len() == 1
                    && same_style(p, &groups[order[end].1].passes[0])
                {
                    end += 1;
                }
                let mut vertices = Vec::with_capacity((end - start) * 3);
                #[cfg(target_os = "vita")]
                let mut world_triangles = [0_usize; 2];
                for &(_, g, t) in &order[start..end] {
                    #[cfg(target_os = "vita")]
                    if let Some(static_batch) = groups[g].passes[0].world_static {
                        world_triangles[usize::from(!static_batch)] += 1;
                    }
                    let mesh = &groups[g].passes[0].mesh;
                    vertices.extend(
                        mesh.indices[t * 3..t * 3 + 3]
                            .iter()
                            .map(|&i| mesh.vertices[i as usize]),
                    );
                }
                #[cfg(target_os = "vita")]
                crate::frame_profile::counts(&[(
                    "transparent_vertices_copied",
                    vertices.len() as u64,
                )]);
                let indices = (0..vertices.len() as u16).collect();
                #[cfg(target_os = "vita")]
                let has_world = world_triangles.iter().sum::<usize>() > 0;
                #[cfg(target_os = "vita")]
                if has_world {
                    for (kind, triangles) in world_triangles.into_iter().enumerate() {
                        crate::frame_profile::record_geometry(triangles, triangles*3, kind == 0);
                    }
                    crate::frame_profile::record_world_submission();
                }
                #[cfg(target_os = "vita")]
                drop(_pack_profile);
                #[cfg(target_os = "vita")]
                let _submit_profile = crate::frame_profile::span("transparent_submit");
                draw_pass(
                    &Pass {
                        _game_thread: crate::game_thread::GameThread::default(),
                        #[cfg(feature="vita_renderer")]
                        geometry:None,
                        #[cfg(feature="vita_renderer")]
                        effect:p.effect,
                        #[cfg(feature = "vita_renderer")]
                        blend_index: p.blend_index,
                        #[cfg(feature = "vita_renderer")]
                        depth_equal: p.depth_equal,
                        #[cfg(target_os = "vita")]
                        world_static: has_world.then_some(world_triangles[1] == 0),
                        #[cfg(target_os = "vita")]
                        world_geometry_counted: true,
                        cutout_reference: p.cutout_reference,
                        mesh: Mesh {
                            vertices,
                            indices,
                            texture: p.mesh.texture.clone(),
                        },
                        material: p.material.clone(),
                        lit: p.lit,
                        clamp: p.clamp,
                        cutout: p.cutout,
                        gain: p.gain,
                        fog: p.fog,
                        model: p.model,
                        appearance: p.appearance,
                        filter_fade: p.filter_fade,
                    },
                    None,
                );
                start = end;
                continue;
            }
            while end < order.len() && order[end].1 == group {
                end += 1;
            }
            let mut indices = Vec::with_capacity((end - start) * 3);
            for &(_, _, t) in &order[start..end] {
                indices.extend_from_slice(&passes[0].mesh.indices[t * 3..t * 3 + 3]);
            }
            #[cfg(target_os = "vita")]
            drop(_pack_profile);
            {
                #[cfg(target_os = "vita")]
                let _submit_profile = crate::frame_profile::span("transparent_submit");
                for p in passes {
                    draw_pass(p, Some(&indices));
                }
            }
            start = end;
        }
    });
    #[cfg(target_os = "vita")]
    drop(transparent_profile);
    unsafe {
        {
            #[cfg(target_os = "vita")]
            crate::frame_profile::flush_boundary("flush_fx_finish");
            #[cfg(target_os = "vita")]
            let _flush_profile = crate::frame_profile::span("gl_flush");
            #[cfg(target_os = "vita")]
            let _flush_site_profile = crate::frame_profile::span("flush_fx_finish");

            get_internal_gl().flush();
        }
    }
    FRAME.with(|f| {
        let mut f = f.borrow_mut();
        f.active = false;
        f.retained.clear();
    });
    gl_use_default_material();
    (order.len(), dropped)
}
pub fn fog(blend: Blend) -> f32 {
    match blend {
        Blend::Add | Blend::AlphaAdd | Blend::ColorAdd => 1.,
        Blend::Filter => 2.,
        Blend::DoubleFilter => 3.,
        Blend::AlphaColor => 4.,
        _ => 0.,
    }
}
pub fn pass(
    mesh: Mesh,
    stage: &Stage,
    model: bool,
    lit: bool,
    appearance: Appearance,
) -> Result<Pass> {
    pass_at(mesh, stage, model, lit, appearance, time())
}
fn pass_at(
    mesh: Mesh,
    stage: &Stage,
    model: bool,
    lit: bool,
    appearance: Appearance,
    clock: f32,
) -> Result<Pass> {
    let rgb = stage.constant_rgb.unwrap_or(Vec3::ONE)
        * stage
            .rgb
            .as_ref()
            .map_or(1., |w| w.sample(clock))
            .clamp(0., 2.);
    Ok(Pass {
        _game_thread: crate::game_thread::GameThread::default(),
        #[cfg(feature="vita_renderer")]
        geometry:None,
        #[cfg(feature="vita_renderer")]
        effect:false,
        #[cfg(feature = "vita_renderer")]
        blend_index: crate::render::blend_index(stage.blend),
        #[cfg(feature = "vita_renderer")]
        depth_equal: stage.depth_equal,
        #[cfg(target_os = "vita")]
        world_static: None,
        #[cfg(target_os = "vita")]
        world_geometry_counted: false,
        mesh,
        material: if retained_active() { None } else { Some(crate::render::world_material(
            crate::render::blend_index(stage.blend)
                + if stage.depth_equal {
                    crate::render::BLEND_COUNT
                } else {
                    0
                },
        )?) },
        lit,
        clamp: stage.clamp,
        cutout: stage.alpha_test as f32,
        cutout_reference: stage.alpha_reference,
        gain: rgb.extend(
            stage
                .alpha
                .as_ref()
                .map(|w| w.sample(clock))
                .or(stage.constant_alpha)
                .unwrap_or(1.)
                .clamp(0., 1.),
        ),
        fog: fog(stage.blend),
        model,
        appearance,
        filter_fade: 0.,
    })
}
pub fn effect(mesh: &Mesh, blend: Blend) {
    if !active() {
        #[cfg(target_os = "vita")]
        crate::frame_profile::record_lg_submission();
        draw_mesh(mesh);
        return;
    }
    let mesh = copy_mesh(mesh);
    // Source-alpha/additive is equivalent to the former shader's premultiply,
    // including the texture alpha (not only the vertex fade).
    let blend = if blend == Blend::Add {
        Blend::AlphaAdd
    } else {
        blend
    };
    if let Ok(mut p) = pass(
        mesh,
        &Stage {
            blend,
            ..Default::default()
        },
        true,
        false,
        Appearance::default(),
    ) {
        #[cfg(feature="vita_renderer")] {p.effect=true;}
        submit(vec![p], true);
    }
}
pub fn line(a: Vec3, b: Vec3, color: Color) {
    if !active() {
        draw_line_3d(a, b, color);
        return;
    }
    let eye = FRAME.with(|f| f.borrow().camera);
    let side = (eye - (a + b) * 0.5).cross(b - a).normalize_or_zero() * 0.5;
    let vertices = [a - side, b - side, b + side, a + side]
        .into_iter()
        .map(|position| Vertex {
            position,
            uv: Vec2::ZERO,
            normal: Vec4::ZERO,
            color: color.into(),
        })
        .collect();
    effect(
        &Mesh {
            vertices,
            indices: vec![0, 1, 2, 0, 2, 3],
            texture: None,
        },
        Blend::Alpha,
    );
}
pub fn sphere(center: Vec3, radius: f32, texture: Option<&Texture2D>, color: Color) {
    if !active() {
        draw_sphere(center, radius, texture, color);
        return;
    }
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for y in 0..=8 {
        let a = std::f32::consts::PI * y as f32 / 8.;
        for x in 0..=12 {
            let b = std::f32::consts::TAU * x as f32 / 12.;
            vertices.push(Vertex {
                position: center + vec3(a.sin() * b.cos(), a.sin() * b.sin(), a.cos()) * radius,
                uv: vec2(x as f32 / 12., y as f32 / 8.),
                normal: Vec4::ZERO,
                color: color.into(),
            });
        }
    }
    for y in 0..8 {
        for x in 0..12 {
            let i = y * 13 + x;
            indices.extend([i, i + 1, i + 13, i + 1, i + 14, i + 13]);
        }
    }
    effect(
        &Mesh {
            vertices,
            indices,
            texture: texture.cloned(),
        },
        Blend::Alpha,
    );
}
struct Layer {
    stage: Stage,
    textures: Vec<Texture2D>,
}
pub struct Surface {
    layers: Vec<Layer>,
    transparent: bool,
    preserve_skin: bool,
    #[cfg(feature="vita_renderer")]
    actors:RefCell<std::collections::HashMap<usize,actor_cache::State>>,
}
pub fn register(
    assets: &mut Assets,
    base: &Texture2D,
    names: &[String],
    specs: &BTreeMap<String, texture::MaterialSpec>,
) -> Result<Rc<Surface>> {
    let spec = names.iter().find_map(|n| {
        let key = n.to_lowercase();
        specs
            .get(&key)
            .or_else(|| key.rsplit_once('.').and_then(|(s, _)| specs.get(s)))
    });
    let stages = spec.map(|s| s.stages.clone()).unwrap_or_default();
    let transparent = spec.is_some_and(|s| s.transparent)
        || stages
            .first()
            .is_some_and(|s| s.blend != Blend::Opaque && s.alpha_test == 0 && !s.depth_write);
    let mut layers = Vec::new();
    for stage in stages.into_iter().filter(|s| {
        !s.unsupported && !s.images.is_empty() && !s.images.iter().any(|n| n == "$lightmap")
    }) {
        let mut textures = Vec::new();
        for name in &stage.images {
            if name == "$whiteimage" || name == "*white" {
                textures.push(crate::texture::upload_rgba8(1, 1, &[255; 4]));
                continue;
            }
            let Some(path) = texture::resolve(assets, name, &BTreeMap::new()) else {
                continue;
            };
            let image = texture::decode(assets, &path)?;
            let tex = crate::texture::upload_rgba8(image.width, image.height, &image.pixels);
            tex.set_filter(FilterMode::Linear);
            // Model shaders tile too (notably the Queen's tentacle and base).
            // CPU UV modifiers may leave [0,1]; clamping samples one edge texel.
            if !stage.clamp {
                unsafe {
                    get_internal_gl().quad_context.texture_set_wrap(
                        tex.raw_miniquad_id(),
                        macroquad::miniquad::TextureWrap::Repeat,
                        macroquad::miniquad::TextureWrap::Repeat,
                    );
                }
            }
            textures.push(tex);
        }
        if textures.len() == stage.images.len() {
            layers.push(Layer { stage, textures });
        }
    }
    if layers.is_empty() {
        layers.push(Layer {
            stage: Stage {
                alpha_test: 4,
                ..Default::default()
            },
            textures: vec![base.clone()],
        });
    }
    let surface = Rc::new(Surface {
        preserve_skin: layers.iter().any(|l| skin_stage_needs_normals(&l.stage)),
        layers,
        transparent,
        #[cfg(feature="vita_renderer")]
        actors:RefCell::default(),
    });
    SURFACES.with(|s| {
        let mut s = s.borrow_mut();
        s.retain(|(_, v)| v.strong_count() > 0);
        s.push((base.raw_miniquad_id(), Rc::downgrade(&surface)));
    });
    #[cfg(feature="vita_renderer")]
    SURFACE_LOOKUP.with(|s|{let mut s=s.borrow_mut();s.retain(|_,v|v.strong_count()>0);s.insert(base.raw_miniquad_id(),Rc::downgrade(&surface));});
    Ok(surface)
}
fn skin_stage_needs_normals(stage: &Stage) -> bool {
    stage.environment || stage.dot_alpha.is_some()
}
#[cfg(test)]
mod skin_preservation_tests {
    use super::*;
    #[test]
    fn normal_driven_layers_require_skin_preservation() {
        assert!(!skin_stage_needs_normals(&Stage::default()));
        assert!(skin_stage_needs_normals(&Stage { environment: true, ..Default::default() }));
        assert!(skin_stage_needs_normals(&Stage { dot_alpha: Some(Vec2::ONE), ..Default::default() }));
    }
}
/// The registered layers consume rebuilt normals after the base skin is prepared.
pub(crate) fn skin_requires_preservation(mesh: &Mesh) -> bool {
    let Some(texture) = &mesh.texture else { return false };
    let id = texture.raw_miniquad_id();
    #[cfg(feature = "vita_renderer")]
    let surface = SURFACE_LOOKUP.with(|s| s.borrow().get(&id).and_then(Weak::upgrade));
    #[cfg(not(feature = "vita_renderer"))]
    let surface = SURFACES.with(|s| s.borrow().iter().rev()
        .find(|(key, _)| *key == id).and_then(|(_, v)| v.upgrade()));
    surface.is_some_and(|s| s.preserve_skin)
}
pub fn skin(mesh: &Mesh) {
    crate::game_thread::assert_current();
    SKIN_OPACITY.with(|a| skin_timed(mesh, None, a.get()));
}

/// Scope entity translucency to one actor, including its attachment meshes.
pub fn with_skin_opacity(alpha: f32, draw: impl FnOnce()) {
    struct Restore(f32);
    impl Drop for Restore {
        fn drop(&mut self) {
            SKIN_OPACITY.with(|a| a.set(self.0));
        }
    }
    let _restore = Restore(SKIN_OPACITY.with(|a| a.replace(alpha.clamp(0., 1.))));
    draw();
}

/// Spawned TAN effects animate from their birth clock and carry entity alpha.
pub(crate) fn skin_effect(mesh: &Mesh, age: f32, alpha: f32) {
    skin_timed(mesh, Some(age), alpha.clamp(0., 1.));
}

fn skin_timed(mesh: &Mesh, clock: Option<f32>, alpha: f32) {
    let layers_profile = crate::frame_profile::characters_span("characters_layers");
    if !active() {
        drop(layers_profile);
        let _submit_profile = crate::frame_profile::characters_span("characters_submit");
        #[cfg(target_os = "vita")]
        crate::frame_profile::record_lg_submission();
        draw_mesh(mesh);
        return;
    }
    #[cfg(feature="vita_renderer")]
    let fast=retained_active()&&crate::render::vita_renderer::config().actor_fast_path;
    #[cfg(not(feature="vita_renderer"))]
    let fast=false;
    let surface=mesh.texture.as_ref().and_then(|t|{
        let id=t.raw_miniquad_id();
        #[cfg(feature="vita_renderer")]
        if fast {return SURFACE_LOOKUP.with(|s|s.borrow().get(&id).and_then(Weak::upgrade));}
        SURFACES.with(|s|s.borrow().iter().rev().find(|(key,_)|*key==id).and_then(|(_,v)|v.upgrade()))
    });
    let (camera, time, appearance) = FRAME.with(|f| {
        let f = f.borrow();
        (f.camera, f.time, f.appearance)
    });
    let time = clock.unwrap_or(time);
    let Some(surface) = surface else {
        let mut copy = copy_mesh(mesh);
        for v in &mut copy.vertices {
            v.color[3] = (v.color[3] as f32 * alpha) as u8;
        }
        if let Ok(p) = pass(
            copy,
            &Stage {
                alpha_test: if alpha < 1. { 0 } else { 4 },
                blend: if alpha < 1. {
                    Blend::Alpha
                } else {
                    Blend::Opaque
                },
                ..Default::default()
            },
            true,
            true,
            appearance,
        ) {
            drop(layers_profile);
            let _submit_profile = crate::frame_profile::characters_span("characters_submit");
            submit(vec![p], alpha < 1.);
        }
        return;
    };
    #[cfg(feature="vita_renderer")]
    if fast && clock.is_none() && alpha==1. && crate::render::vita_renderer::config().actor_layer_cache {
        if let Some(passes)=actor_cache::prepare(&surface,mesh,time,appearance) {
            drop(layers_profile);
            let _submit_profile=crate::frame_profile::characters_span("characters_submit");
            submit(passes,surface.transparent);
            return;
        }
    }
    let mut passes = Vec::new();
    #[cfg(feature = "vita_renderer")]
    let limit = if retained_active() {
        {let c=crate::render::vita_renderer::config();if clock.is_some(){c.fx_max_layers}else{c.actor_max_layers}}
    } else {
        usize::MAX
    };
    #[cfg(not(feature = "vita_renderer"))]
    let limit = usize::MAX;
    #[cfg(feature = "vita_renderer")]
    if surface.layers.len() > limit {
        retained::count(
            "vita_fx_dropped_layers",
            (surface.layers.len() - limit) as u64,
        );
    }
    for layer in surface.layers.iter().take(limit) {
        let mut copy = copy_mesh(mesh);
        copy.texture = Some(
            layer.textures[if layer.stage.images.is_empty() {
                0
            } else {
                layer.stage.frame(time)
            }]
            .clone(),
        );
        for v in &mut copy.vertices {
            if fast && !layer.stage.environment && layer.stage.dot_alpha.is_none() {
                if !layer.stage.mods.is_empty() || layer.stage.vector_uv.is_some(){
                    v.uv=layer.stage.view_uv(v.uv,v.position,Vec3::ZERO,camera,time);
                }
                let opacity=if layer.stage.vertex_alpha{v.color[3] as f32/255.}else{1.};
                v.color[3]=(255.*alpha*opacity.clamp(0.,1.)) as u8;
            }else{
                let n=v.normal.truncate().normalize_or_zero();
                v.uv=layer.stage.view_uv(v.uv,v.position,n,camera,time);
                v.color[3]=(255.*alpha*layer.stage.opacity(n,(camera-v.position).normalize_or_zero(),v.color[3])) as u8;
            }
        }
        let fade=clock.is_none()&&alpha<1.;
        let stage=if fast {std::borrow::Cow::Borrowed(&layer.stage)}else{
            let mut s=layer.stage.clone();
            if fade {if s.blend==Blend::Opaque{s.blend=Blend::Alpha;}s.alpha_test=0;s.depth_write=false;s.depth_equal=false;}
            std::borrow::Cow::Owned(s)
        };
        if let Ok(mut p) = pass_at(copy, &stage, true, !stage.identity, appearance, time) {
            #[cfg(feature="vita_renderer")]
            if fast&&fade {if stage.blend==Blend::Opaque{p.blend_index=crate::render::blend_index(Blend::Alpha);}p.cutout=0.;p.depth_equal=false;}
            // Multiplicative layers ignore source alpha. Fade their colour to
            // the blend's neutral value so a hidden ghost leaves no silhouette.
            p.filter_fade = match stage.blend {
                Blend::Filter | Blend::Factors(0, 2) | Blend::Factors(4, 0) => 1. - alpha,
                Blend::DoubleFilter | Blend::Factors(4, 2) => alpha - 1.,
                _ => 0.,
            };
            #[cfg(feature="vita_renderer")] {p.effect=clock.is_some();}
            passes.push(p);
        }
    }
    drop(layers_profile);
    let _submit_profile = crate::frame_profile::characters_span("characters_submit");
    submit(
        passes,
        surface.transparent || (clock.is_none() && alpha < 1.),
    );
}

pub async fn check(assets: &mut Assets) -> Result<()> {
    use anyhow::ensure;
    std::fs::create_dir_all("private/render-fx")?;
    let mut scene = crate::render::Scene::load(assets, "skool1")?;
    let white = crate::texture::upload_rgba8(1, 1, &[255; 4]);
    let quad = |z: f32, color: Color| Mesh {
        vertices: [
            vec3(-1., -1., z),
            vec3(1., -1., z),
            vec3(1., 1., z),
            vec3(-1., 1., z),
        ]
        .into_iter()
        .zip([vec2(0., 1.), vec2(1., 1.), vec2(1., 0.), vec2(0., 0.)])
        .map(|(position, uv)| Vertex {
            position,
            uv,
            color: color.into(),
            normal: Vec4::ZERO,
        })
        .collect(),
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: Some(white.clone()),
    };
    let camera = Camera3D {
        position: vec3(0., 0., 10.),
        target: Vec3::ZERO,
        up: Vec3::Y,
        projection: Projection::Orthographics,
        fovy: 4.,
        z_near: 0.1,
        z_far: 50.,
        ..Default::default()
    };
    crate::lighting::select(vec![], camera.position, &scene.world);
    for order in [[0, 1, 2], [2, 0, 1], [1, 2, 0]] {
        clear_background(BLACK);
        set_camera(&camera);
        begin(camera.position, -Vec3::Z, 0., &Atmosphere::default(), false);
        submit(
            vec![pass(
                quad(0., WHITE),
                &Stage::default(),
                true,
                false,
                Default::default(),
            )?],
            false,
        );
        for i in order {
            let color = [
                Color::new(0., 0., 1., 0.5),
                Color::new(1., 0., 0., 0.5),
                Color::new(0., 1., 0., 0.5),
            ][i];
            effect(&quad(i as f32 + 1., color), Blend::Alpha);
        }
        let (_, dropped) = finish();
        ensure!(dropped == 0, "Transparent queue dropped geometry");
        let frame = get_screen_data();
        let pixel = frame.get_pixel(frame.width as u32 / 2, frame.height as u32 / 2);
        ensure!(
            (pixel.r - 0.375).abs() < 0.025
                && (pixel.g - 0.625).abs() < 0.025
                && (pixel.b - 0.25).abs() < 0.025,
            "Cross-category order changed compositing: {order:?} {pixel:?}"
        );
        next_frame().await;
    }
    println!("PASS triangle-sorted water/actor/pickup/effect alpha ordering across three submission permutations");
    let mut blue = quad(0., WHITE);
    blue.texture = Some(crate::texture::upload_rgba8(1, 1, &[0, 0, 255, 255]));
    let mut red = quad(1., WHITE);
    red.texture = Some(crate::texture::upload_rgba8(1, 1, &[255, 0, 0, 255]));
    for alpha in [0_f32, 0.12, 0.75, 1.] {
        clear_background(BLACK);
        set_camera(&camera);
        begin(camera.position, -Vec3::Z, 0., &Atmosphere::default(), true);
        skin(&blue);
        with_skin_opacity(alpha, || skin(&red));
        finish();
        let frame = get_screen_data();
        let p = frame.get_pixel(frame.width as u32 / 2, frame.height as u32 / 2);
        ensure!(
            (p.r - alpha).abs() < 0.025 && (p.b - (1. - alpha)).abs() < 0.025,
            "Entity alpha was discarded or ignored: {alpha}/{p:?}"
        );
        next_frame().await;
        clear_background(BLACK);
        set_camera(&camera);
        begin(camera.position, -Vec3::Z, 0., &Atmosphere::default(), true);
        skin(&blue);
        finish();
        let frame = get_screen_data();
        let p = frame.get_pixel(frame.width as u32 / 2, frame.height as u32 / 2);
        ensure!(p.b > 0.98, "Entity opacity leaked into another draw");
        next_frame().await;
    }
    let layers = crate::materials::parse("ghost_fixture\n{\nsurfaceparm trans\n{\nmap $whiteimage\nblendfunc blend\nrgbgen const 1 0 0\n}\n{\nmap $whiteimage\nblendfunc filter\nrgbgen const .5 .5 .5\n}\n}")?;
    let ghost_texture = crate::texture::upload_rgba8(1, 1, &[255; 4]);
    let owner = register(assets, &ghost_texture, &["ghost_fixture".into()], &layers)?;
    let mut ghost = quad(1., WHITE);
    ghost.texture = Some(ghost_texture);
    for alpha in [0_f32, 0.12, 0.75, 1.] {
        clear_background(BLACK);
        set_camera(&camera);
        begin(camera.position, -Vec3::Z, 0., &Atmosphere::default(), true);
        skin(&blue);
        with_skin_opacity(alpha, || skin(&ghost));
        finish();
        let frame = get_screen_data();
        let p = frame.get_pixel(frame.width as u32 / 2, frame.height as u32 / 2);
        let filter = 1. - 0.5 * alpha;
        ensure!(
            (p.r - alpha * filter).abs() < 0.025 && (p.b - (1. - alpha) * filter).abs() < 0.025,
            "Multiplicative ghost layer ignored entity fade: {alpha}/{p:?}"
        );
        next_frame().await;
    }
    drop(owner);
    println!("PASS Phantasmagoria opacity, low-alpha visibility and independent actor state");
    clear_background(BLACK);
    set_camera(&camera);
    begin(camera.position, -Vec3::Z, 0., &Atmosphere::default(), false);
    submit(
        vec![pass(
            quad(4., Color::new(1., 0., 0., 1.)),
            &Stage::default(),
            true,
            false,
            Default::default(),
        )?],
        false,
    );
    effect(&quad(1., BLUE), Blend::Alpha);
    finish();
    let frame = get_screen_data();
    let pixel = frame.get_pixel(frame.width as u32 / 2, frame.height as u32 / 2);
    ensure!(
        pixel.r > 0.98 && pixel.b < 0.02,
        "Transparent geometry bypassed opaque depth: {pixel:?}"
    );
    next_frame().await;
    println!("PASS queued transparency respects solid depth");
    let layered = crate::materials::parse("fixture\n{\nsurfaceparm trans\n{\nmap $whiteimage\nblendfunc add\nrgbgen const .2 0 0\n}\n{\nmap $whiteimage\nblendfunc filter\nrgbgen const .5 1 1\n}\n{\nmap $whiteimage\nblendfunc add\nrgbgen const 0 0 .3\n}\n}")?;
    let owner = register(assets, &white, &["fixture".into()], &layered)?;
    clear_background(Color::new(0.2, 0.2, 0.2, 1.));
    set_camera(&camera);
    begin(camera.position, -Vec3::Z, 0., &Atmosphere::default(), false);
    skin(&quad(1., WHITE));
    finish();
    let frame = get_screen_data();
    let pixel = frame.get_pixel(frame.width as u32 / 2, frame.height as u32 / 2);
    ensure!(
        (pixel.r - 0.2).abs() < 0.025
            && (pixel.g - 0.2).abs() < 0.025
            && (pixel.b - 0.5).abs() < 0.025,
        "Skin/pickup layer order changed: {pixel:?}"
    );
    drop(owner);
    next_frame().await;
    let mut translucent = quad(1., WHITE);
    translucent.texture = Some(crate::texture::upload_rgba8(1, 1, &[255, 0, 0, 64]));
    clear_background(BLACK);
    set_camera(&camera);
    begin(camera.position, -Vec3::Z, 0., &Atmosphere::default(), false);
    effect(&translucent, Blend::Add);
    finish();
    let frame = get_screen_data();
    let pixel = frame.get_pixel(frame.width as u32 / 2, frame.height as u32 / 2);
    ensure!(
        (pixel.r - 0.25).abs() < 0.025 && pixel.g < 0.02,
        "Additive sprite lost texture alpha: {pixel:?}"
    );
    next_frame().await;
    println!("PASS registered model layers and additive sprite texture alpha");
    // Real light declarations and GPU contribution, including the models-only mask.
    let lights = vec![crate::lighting::Light {
        position: vec3(0., 0., 2.),
        color: vec3(0.4, 0., 0.),
        radius: 10.,
        only_models: false,
        flare: false,
    }];
    crate::lighting::fixture_lights(lights);
    clear_background(BLACK);
    set_camera(&camera);
    begin(camera.position, -Vec3::Z, 0., &Atmosphere::default(), false);
    submit(
        vec![pass(
            quad(0., Color::new(0.2, 0.2, 0.2, 1.)),
            &Stage::default(),
            true,
            true,
            Default::default(),
        )?],
        false,
    );
    finish();
    let image = get_screen_data();
    let pixel = image.get_pixel(image.width as u32 / 2, image.height as u32 / 2);
    ensure!(
        pixel.r > pixel.g + 0.25 && pixel.g > 0.15,
        "Dynamic light did not reach GPU: {pixel:?}"
    );
    crate::viewer::save_capture(std::path::Path::new(
        "private/render-fx/light-regression.png",
    ))?;
    next_frame().await;
    println!("PASS dynamic-light colour/radius reaches actual world shader");
    let authored = crate::particles::declared_lights(assets, "croquetball")?;
    ensure!(
        !authored.is_empty(),
        "Missing supplied croquet light declaration"
    );
    for model in [false, true] {
        crate::lighting::fixture_lights(vec![crate::lighting::Light {
            position: vec3(0., 0., 2.),
            color: vec3(0.4, 0., 0.),
            radius: 10.,
            only_models: true,
            flare: false,
        }]);
        clear_background(BLACK);
        set_camera(&camera);
        begin(camera.position, -Vec3::Z, 0., &Atmosphere::default(), false);
        submit(
            vec![pass(
                quad(0., Color::new(0.2, 0.2, 0.2, 1.)),
                &Stage::default(),
                model,
                true,
                Default::default(),
            )?],
            false,
        );
        finish();
        let f = get_screen_data();
        let p = f.get_pixel(f.width as u32 / 2, f.height as u32 / 2);
        ensure!(
            if model {
                p.r > p.g + 0.25
            } else {
                (p.r - p.g).abs() < 0.025
            },
            "models-only light mask: {model} {p:?}"
        );
        next_frame().await;
    }
    let floor =
        crate::collision::World::fixture(&[(vec3(-200., -200., -10.), vec3(200., 200., 0.))]);
    let shadow_camera = Camera3D {
        position: Vec3::Z * 100.,
        target: Vec3::ZERO,
        up: Vec3::Y,
        projection: Projection::Orthographics,
        fovy: 80.,
        z_near: 0.1,
        z_far: 200.,
        ..Default::default()
    };
    crate::lighting::fixture_lights(vec![]);
    for height in [2., 200.] {
        clear_background(WHITE);
        set_camera(&shadow_camera);
        begin(
            shadow_camera.position,
            -Vec3::Z,
            0.,
            &Atmosphere::default(),
            false,
        );
        crate::lighting::shadow(&floor, Vec3::Z * height, 22.);
        finish();
        let f = get_screen_data();
        let p = f.get_pixel(f.width as u32 / 2, f.height as u32 / 2);
        ensure!(
            if height < 10. { p.r < 0.8 } else { p.r > 0.98 },
            "Projected source shadow height: {height} {p:?}"
        );
        next_frame().await;
    }
    let flare = crate::particles::declared_lights(assets, "fx_emitter_lensflare")?;
    ensure!(
        flare.len() == 1 && flare[0].flare,
        "Supplied lens flare declaration missing"
    );
    for occluded in [false, true] {
        let wall =
            crate::collision::World::fixture(&[(vec3(-20., -20., 20.), vec3(20., 20., 21.))]);
        crate::lighting::select(
            flare.clone(),
            shadow_camera.position,
            if occluded { &wall } else { &floor },
        );
        clear_background(BLACK);
        set_camera(&shadow_camera);
        begin(
            shadow_camera.position,
            -Vec3::Z,
            0.,
            &Atmosphere::default(),
            false,
        );
        crate::lighting::flares(shadow_camera.position, -Vec3::Z);
        finish();
        let f = get_screen_data();
        let p = f.get_pixel(f.width as u32 / 2, f.height as u32 / 2);
        ensure!(
            if occluded { p.b < 0.02 } else { p.b > 0.1 },
            "Source flare occlusion: {occluded} {p:?}"
        );
        next_frame().await;
    }
    println!("PASS supplied light declarations, models-only mask, projected shadow height and flare wall occlusion");
    let mut timings = Vec::new();
    for name in ["skool1", "potears1", "pandemonium"] {
        scene = crate::render::Scene::load(assets, name)?;
        let mut interactions = crate::interaction::Interactions::load(&scene.map)?;
        interactions.set_entry(assets, &scene.map, name, None)?;
        let mut steam = crate::particles::Steam::load(assets, &scene.map)?;
        steam.animate(1.);
        steam.sync(&interactions.event_world);
        let eye = scene.map.spawn().0 + vec3(0., 0., 50.);
        for _ in 0..60 {
            steam.update(1. / 60., eye, &scene.world);
        }
        let direction = Vec3::Y;
        for mode in ["legacy", "queue", "queue_and_light"] {
            let enabled = mode == "queue_and_light";
            let mut total = 0.;
            let mut samples = Vec::new();
            let mut visible_lights = 0;
            for i in 0..8 {
                clear_background(scene.atmosphere.background());
                let view = Camera3D {
                    position: eye,
                    target: eye + direction,
                    up: Vec3::Z,
                    z_near: 2.,
                    z_far: 30000.,
                    fovy: 75f32.to_radians(),
                    ..Default::default()
                };
                set_camera(&view);
                let start = std::time::Instant::now();
                let mut lights = steam.lights();
                // A named fixture adds an actual authored projectile light ahead of
                // this fixed camera, so the on/off cost is measured even in unlit maps.
                lights.extend(crate::lighting::croquet(eye + direction * 32.));
                crate::lighting::select(if enabled { lights } else { vec![] }, eye, &scene.world);
                visible_lights = crate::lighting::count();
                if mode != "legacy" {
                    begin_view(&view, 1., &scene.atmosphere, false);
                }
                let transforms = interactions.transforms();
                scene.draw(eye, 1., false, false, &transforms);
                scene.draw_with_particles(eye, direction, 1., false, &transforms, &steam);
                let (_, dropped) = if mode != "legacy" { finish() } else { (0, 0) };
                ensure!(dropped == 0, "Queue budget exceeded in {name}");
                unsafe {
                    {
                        #[cfg(target_os = "vita")]
                        crate::frame_profile::flush_boundary("flush_fx_check");
                        #[cfg(target_os = "vita")]
                        let _flush_profile = crate::frame_profile::span("gl_flush");
                        #[cfg(target_os = "vita")]
                        let _flush_site_profile = crate::frame_profile::span("flush_fx_check");

                        get_internal_gl().flush();
                    }
                    macroquad::miniquad::gl::glFinish();
                }
                if i > 1 {
                    let ms = start.elapsed().as_secs_f64() * 1000.;
                    total += ms;
                    samples.push(ms);
                }
                if enabled && i == 7 {
                    crate::viewer::save_capture(std::path::Path::new(&format!(
                        "private/render-fx/{name}.png"
                    )))?;
                }
                next_frame().await;
            }
            timings.push(
                serde_json::json!({"map":name,"mode":mode,"authored_projectile_fixture":enabled,"visible_lights":visible_lights,"mean_cpu_gpu_ms":total/6.,"samples_ms":samples}),
            );
        }
    }
    std::fs::write(
        "private/render-fx/performance.json",
        serde_json::to_vec_pretty(&timings)?,
    )?;
    println!("PASS three real-map particle/transparency fixtures; synchronized CPU+GPU timings: {timings:?}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clip_rejection_preserves_crossing_triangles() {
        assert!(outside([
            vec4(2., 0., 0., 1.),
            vec4(3., 0., 0., 1.),
            vec4(2., 1., 0., 1.)
        ]));
        assert!(!outside([
            vec4(-2., 0., 0., 1.),
            vec4(2., 0., 0., 1.),
            vec4(0., 2., 0., 1.)
        ]));
        assert!(!outside([
            vec4(0., 0., -2., 1.),
            vec4(0., 0., 0., 1.),
            vec4(1., 0., 0., 1.)
        ]));
    }
}
