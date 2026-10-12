//! Whole actor/effect packets share one alpha order with immutable world groups.
//! Only dynamic LG meshes stream here; world buffers are never read or uploaded.
use super::{Frame, Pass, FRAME};
use crate::environment::Atmosphere;
use anyhow::{ensure, Context, Result};
use macroquad::{miniquad::*, prelude::*};
use std::collections::BTreeMap;

#[cfg(not(target_os = "vita"))]
thread_local! { static HOST_COUNTS:std::cell::RefCell<BTreeMap<&'static str,u64>>=std::cell::RefCell::default(); }
#[cfg(not(target_os = "vita"))]
pub(crate) fn host_report() -> serde_json::Value {
    HOST_COUNTS.with(|c| serde_json::to_value(&*c.borrow()).unwrap())
}
#[cfg(not(target_os = "vita"))]
pub(crate) fn host_reset() {
    HOST_COUNTS.with(|c| c.borrow_mut().clear());
}
pub(crate) fn count(name: &'static str, value: u64) {
    #[cfg(target_os = "vita")]
    crate::frame_profile::counts(&[(name, value)]);
    #[cfg(not(target_os = "vita"))]
    HOST_COUNTS.with(|c| *c.borrow_mut().entry(name).or_default() += value);
}
fn boundary(site: &'static str) {
    let _span = crate::frame_profile::span("gl_flush");
    let _site = crate::frame_profile::span(site);
    #[cfg(target_os = "vita")]
    crate::frame_profile::flush_boundary(site);
    unsafe {
        get_internal_gl().flush();
    }
    count("vita_3d_interop_flushes", 1);
}
pub(crate) fn begin_view(
    camera: &Camera3D,
    time: f32,
    atmosphere: &Atmosphere,
    fullbright: bool,
    flush: bool,
) {
    use macroquad::camera::Camera;
    #[cfg(not(target_os = "vita"))]
    if flush {
        host_reset();
    }
    if flush {
        boundary("flush_vita_3d_begin");
    }
    FRAME.with(|f| {
        *f.borrow_mut() = Frame {
            active: true,
            direct: true,
            camera: camera.position,
            direction: (camera.target - camera.position).normalize_or_zero(),
            time,
            atmosphere: atmosphere.clone(),
            fullbright,
            projection: Some(camera.matrix()),
            ..Default::default()
        }
    });
}
const UNIFORM_FLOATS: usize = 147;
fn meta() -> ShaderMeta {
    let mut uniforms = vec![
        UniformDesc::new("Model", UniformType::Mat4),
        UniformDesc::new("Projection", UniformType::Mat4),
    ];
    uniforms.extend(
        [
            ("AtlasGrid", UniformType::Float2),
            ("Fullbright", UniformType::Float1),
            ("Lit", UniformType::Float1),
            ("ClampUV", UniformType::Float1),
            ("Cutout", UniformType::Float1),
            ("CutoutReference", UniformType::Float1),
            ("Gain", UniformType::Float4),
            ("FogMode", UniformType::Float1),
            ("SkySize", UniformType::Float2),
            ("SkyFade", UniformType::Float1),
            ("FxModel", UniformType::Float1),
            ("FxAppearance", UniformType::Float4),
        ]
        .into_iter()
        .map(|(name, kind)| UniformDesc::new(name, kind)),
    );
    uniforms.extend(crate::lighting::uniforms());
    uniforms.extend(crate::environment::uniforms());
    ShaderMeta {
        images: vec!["Texture".into(), "LightAtlas".into(), "SkyBackdrop".into()],
        uniforms: UniformBlockLayout { uniforms },
    }
}
fn uniforms(p: &Pass, f: &Frame) -> [f32; UNIFORM_FLOATS] {
    let mut output = [0.; UNIFORM_FLOATS];
    let mut cursor = 0;
    let mut append = |values: &[f32]| {
        output[cursor..cursor + values.len()].copy_from_slice(values);
        cursor += values.len();
    };
    append(&Mat4::IDENTITY.to_cols_array());
    append(
        &f.projection
            .expect("actor camera projection")
            .to_cols_array(),
    );
    append(&[
        1.,
        1.,
        f32::from(f.fullbright),
        f32::from(p.lit),
        f32::from(p.clamp),
        p.cutout,
        p.cutout_reference,
    ]);
    append(&p.gain.to_array());
    append(&[p.fog, 1., 1., 0., f32::from(p.model)]);
    append(&[
        p.appearance.dissolve,
        f32::from(p.appearance.ghost),
        p.appearance.power,
        p.filter_fade,
    ]);
    for light in crate::lighting::packed(p.model, !f.fullbright && p.lit) {
        append(&light);
    }
    append(&f.camera.to_array());
    append(&f.atmosphere.distance.to_array());
    append(&f.atmosphere.liquid.to_array());
    for index in 0..2 {
        let (lo, hi, color) = f
            .atmosphere
            .volumes
            .get(index)
            .map_or(([0.; 3], [0.; 3], [0.; 4]), |v| {
                (v.min.to_array(), v.max.to_array(), v.color.to_array())
            });
        append(&lo);
        append(&hi);
        append(&color);
    }
    debug_assert_eq!(cursor, UNIFORM_FLOATS);
    output
}
struct Buffers {
    vertex: BufferId,
    index: BufferId,
    vertex_capacity: usize,
    index_capacity: usize,
}
pub(crate) struct Gpu {
    shader: ShaderId,
    pipelines: BTreeMap<(usize, bool), Pipeline>,
    buffers: Vec<Buffers>,
    white: Texture2D,
}
impl Gpu {
    pub(crate) fn new() -> Result<Self> {
        let white = crate::texture::upload_rgba8(1, 1, &[255; 4]);
        let fragment =
            crate::lighting::fragment(&crate::environment::fragment(crate::render::FRAGMENT));
        let ctx = unsafe { get_internal_gl() }.quad_context;
        let shader = ctx
            .new_shader(
                ShaderSource::Glsl {
                    vertex: crate::render::VERTEX,
                    fragment: &fragment,
                },
                meta(),
            )
            .context("retained actor shader")?;
        println!(
            "[lg/vita-renderer] actor READY grouped alpha, actor_max_layers={}",
            crate::render::vita_renderer::config().actor_max_layers
        );
        Ok(Self {
            shader,
            pipelines: BTreeMap::new(),
            buffers: Vec::new(),
            white,
        })
    }
    fn upload(&mut self, slot: usize, p: &Pass) -> Result<Bindings> {
        ensure!(
            valid_mesh(&p.mesh),
            "invalid actor range: {} vertices / {} indices",
            p.mesh.vertices.len(),
            p.mesh.indices.len()
        );
        // Resolve managed Texture2D IDs before borrowing the Macroquad context.
        let white = self.white.raw_miniquad_id();
        let texture = p
            .mesh
            .texture
            .as_ref()
            .map_or(white, Texture2D::raw_miniquad_id);
        let ctx = unsafe { get_internal_gl() }.quad_context;
        let vc = p.mesh.vertices.len().next_power_of_two();
        let ic = p.mesh.indices.len().next_power_of_two();
        if slot == self.buffers.len()
            || self.buffers[slot].vertex_capacity < vc
            || self.buffers[slot].index_capacity < ic
        {
            let b = Buffers {
                vertex: ctx.new_buffer(
                    BufferType::VertexBuffer,
                    BufferUsage::Stream,
                    BufferSource::empty::<Vertex>(vc),
                ),
                index: ctx.new_buffer(
                    BufferType::IndexBuffer,
                    BufferUsage::Stream,
                    BufferSource::empty::<u16>(ic),
                ),
                vertex_capacity: vc,
                index_capacity: ic,
            };
            if slot == self.buffers.len() {
                self.buffers.push(b);
            } else {
                let old = std::mem::replace(&mut self.buffers[slot], b);
                ctx.delete_buffer(old.vertex);
                ctx.delete_buffer(old.index);
            }
            count("vita_fx_buffer_growths", 1);
        }
        let b = &self.buffers[slot];
        // Vita's pinned compact-stream backend orphans in-flight storage with glBufferData.
        ctx.buffer_update(b.vertex, BufferSource::slice(&p.mesh.vertices));
        ctx.buffer_update(b.index, BufferSource::slice(&p.mesh.indices));
        count(
            "vita_fx_geometry_upload_bytes",
            (p.mesh.vertices.len() * 40 + p.mesh.indices.len() * 2) as u64,
        );
        count("vita_fx_mesh_uploads", 1);
        Ok(Bindings {
            vertex_buffers: vec![b.vertex],
            index_buffer: b.index,
            images: vec![texture, white, white],
        })
    }
    fn draw(&mut self, p: &Pass, bindings: &Bindings, u: &[f32; UNIFORM_FLOATS]) {
        let ctx = unsafe { get_internal_gl() }.quad_context;
        let key = (p.blend_index, p.depth_equal);
        let pipeline = self.pipelines.entry(key).or_insert_with(|| {
            ctx.new_pipeline(
                &[BufferLayout {
                    stride: 40,
                    ..Default::default()
                }],
                &[
                    VertexAttribute::new("position", VertexFormat::Float3),
                    VertexAttribute::new("texcoord", VertexFormat::Float2),
                    VertexAttribute::new("color0", VertexFormat::Byte4),
                    VertexAttribute::new("normal", VertexFormat::Float4),
                ],
                self.shader,
                PipelineParams {
                    depth_test: if p.depth_equal {
                        Comparison::Equal
                    } else {
                        Comparison::LessOrEqual
                    },
                    ..crate::render::depth_pipeline(crate::render::blend(p.blend_index))
                },
            )
        });
        ctx.apply_pipeline(pipeline);
        ctx.apply_bindings(bindings);
        ctx.apply_uniforms(UniformsSource::table(u));
        ctx.draw(0, p.mesh.indices.len() as i32, 1);
        count("vita_fx_gpu_draws", 1);
        count("vita_fx_triangles", p.mesh.indices.len() as u64 / 3);
        count("vita_fx_state_applications", 1);
        count("vita_fx_uniform_bytes", std::mem::size_of_val(u) as u64);
        #[cfg(target_os = "vita")]
        crate::frame_profile::record_lg_submission();
    }
}
impl Drop for Gpu {
    fn drop(&mut self) {
        let ctx = unsafe { get_internal_gl() }.quad_context;
        for b in &self.buffers {
            ctx.delete_buffer(b.vertex);
            ctx.delete_buffer(b.index);
        }
        for p in self.pipelines.values() {
            ctx.delete_pipeline(*p);
        }
        ctx.delete_shader(self.shader);
    }
}
fn valid_mesh(mesh: &Mesh) -> bool {
    !mesh.vertices.is_empty()
        && !mesh.indices.is_empty()
        && mesh.vertices.len() <= 65536
        && mesh.indices.len() % 3 == 0
        && mesh.indices.len() <= i32::MAX as usize
        && mesh
            .indices
            .iter()
            .all(|&i| (i as usize) < mesh.vertices.len())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    World(usize),
    Actor(usize),
}
fn order(mut list: Vec<(f32, Source)>, sort: bool) -> Vec<(f32, Source)> {
    // Stable sort keeps authored world layer ties and actor group submission ties.
    if sort {
        list.sort_by(|a, b| b.0.total_cmp(&a.0));
    }
    list
}
fn center(mesh: &Mesh) -> Vec3 {
    let (lo, hi) = mesh.vertices.iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(lo, hi), v| (lo.min(v.position), hi.max(v.position)),
    );
    (lo + hi) * 0.5
}
fn same_packet(a: &Pass, b: &Pass) -> bool {
    a.blend_index == b.blend_index
        && a.depth_equal == b.depth_equal
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
/// Coalesce only adjacent whole dynamic meshes with identical shader state.
/// The joint order is already final: a world group or layered actor is a barrier.
/// This offsets actor indices once, never reconstructs or reads world geometry.
fn merge_adjacent(groups: &mut [super::Group], list: &mut Vec<(f32, Source)>) {
    let mut previous = None;
    let mut kept = 0;
    for i in 0..list.len() {
        let item = list[i];
        let Source::Actor(id) = item.1 else {
            previous = None;
            list[kept] = item;
            kept += 1;
            continue;
        };
        let can_merge = previous.is_some_and(|previous: usize| {
            let (a, b) = (&groups[previous].passes, &groups[id].passes);
            a.len() == 1
                && b.len() == 1
                && valid_mesh(&a[0].mesh)
                && valid_mesh(&b[0].mesh)
                && a[0].mesh.vertices.len() + b[0].mesh.vertices.len() <= 60000
                && a[0].mesh.indices.len() + b[0].mesh.indices.len() <= 180000
                && same_packet(&a[0], &b[0])
        });
        if can_merge {
            let mut source = groups[id].passes.pop().unwrap();
            let target = &mut groups[previous.unwrap()].passes[0].mesh;
            let offset = target.vertices.len() as u16;
            count(
                "vita_fx_appended_vertices",
                source.mesh.vertices.len() as u64,
            );
            count("vita_fx_appended_indices", source.mesh.indices.len() as u64);
            target.vertices.append(&mut source.mesh.vertices);
            target
                .indices
                .extend(source.mesh.indices.into_iter().map(|i| i + offset));
            count("vita_fx_merged_meshes", 1);
        } else {
            previous = Some(id);
            list[kept] = item;
            kept += 1;
        }
    }
    list.truncate(kept);
}
pub(crate) fn finish(
    gpu: &mut Gpu,
    world: &[(f32, usize)],
    flush: bool,
    mut draw_world: impl FnMut(usize),
) -> (usize, usize) {
    let _span = crate::frame_profile::span("vita_3d_finish");
    let (mut groups, camera, direction, dropped) = FRAME.with(|f| {
        let mut f = f.borrow_mut();
        (
            std::mem::take(&mut f.groups),
            f.camera,
            f.direction,
            f.dropped,
        )
    });
    if flush {
        boundary("flush_vita_3d_packets");
    }
    let mut list = {
        let _span = crate::frame_profile::span("vita_joint_alpha_sort");
        let mut list = world
            .iter()
            .map(|&(d, id)| (d, Source::World(id)))
            .collect::<Vec<_>>();
        list.extend(
            groups
                .iter()
                .enumerate()
                .filter(|(_, g)| g.transparent && !g.passes[0].mesh.vertices.is_empty())
                .map(|(id, g)| {
                    (
                        (center(&g.passes[0].mesh) - camera).dot(direction),
                        Source::Actor(id),
                    )
                }),
        );
        order(list, crate::render::vita_renderer::config().sort_alpha)
    };
    if crate::render::vita_renderer::config().merge_fx {
        let _span = crate::frame_profile::span("vita_fx_merge_groups");
        merge_adjacent(&mut groups, &mut list);
        let mut opaque = groups
            .iter()
            .enumerate()
            .filter(|(_, g)| !g.transparent)
            .map(|(id, _)| (0., Source::Actor(id)))
            .collect::<Vec<_>>();
        merge_adjacent(&mut groups, &mut opaque);
    }
    let mut packets = Vec::new();
    let mut ranges = Vec::with_capacity(groups.len());
    let mut invalid = 0;
    {
        let _span = crate::frame_profile::span("vita_fx_upload");
        for group in &groups {
            let start = packets.len();
            for p in &group.passes {
                if p.mesh.indices.is_empty() {
                    continue;
                }
                match gpu.upload(packets.len(), p) {
                    Ok(b) => {
                        let u = FRAME.with(|f| uniforms(p, &f.borrow()));
                        packets.push((p, b, u));
                    }
                    Err(e) => {
                        eprintln!("[lg/vita-renderer] rejected actor packet: {e:#}");
                        invalid += 1;
                    }
                }
            }
            ranges.push(start..packets.len());
        }
    }
    count("vita_fx_capacity_samples", 1);
    count(
        "vita_fx_stream_capacity_bytes",
        gpu.buffers
            .iter()
            .map(|b| b.vertex_capacity * 40 + b.index_capacity * 2)
            .sum::<usize>() as u64,
    );
    count("vita_fx_invalid_packets", invalid as u64);
    count("vita_fx_dropped_groups", dropped as u64);
    count("vita_fx_sorted_triangles", 0);
    count("vita_fx_compacted_vertices", 0);
    let draw_group = |gpu: &mut Gpu, id: usize| {
        for i in ranges[id].clone() {
            let (p, b, u) = &packets[i];
            gpu.draw(p, b, u);
        }
    };
    unsafe {
        get_internal_gl()
            .quad_context
            .begin_default_pass(PassAction::Nothing);
        macroquad::miniquad::gl::glDepthMask(1);
    }
    {
        let _span = crate::frame_profile::span("vita_fx_opaque_submit");
        for (id, g) in groups.iter().enumerate().filter(|(_, g)| !g.transparent) {
            let _ = g;
            draw_group(gpu, id);
        }
    }
    count("vita_joint_alpha_groups", list.len() as u64);
    unsafe {
        macroquad::miniquad::gl::glDepthMask(0);
    }
    {
        let _span = crate::frame_profile::span("vita_joint_alpha_submit");
        for (_, source) in &list {
            match *source {
                Source::World(id) => draw_world(id),
                Source::Actor(id) => draw_group(gpu, id),
            }
        }
    }
    unsafe {
        macroquad::miniquad::gl::glDepthMask(1);
        get_internal_gl().quad_context.end_render_pass();
    }
    FRAME.with(|f| {
        let mut f = f.borrow_mut();
        f.active = false;
        f.direct = false;
        f.retained.clear();
    });
    gl_use_default_material();
    (list.len(), dropped + invalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uniform_and_vertex_layout_preserve_legacy_shader() {
        assert_eq!(std::mem::size_of::<Vertex>(), 40);
        assert_eq!(std::mem::offset_of!(Vertex, normal), 24);
        assert_eq!(
            meta()
                .uniforms
                .uniforms
                .iter()
                .map(|u| u.uniform_type.size() * u.array_count)
                .sum::<usize>(),
            UNIFORM_FLOATS * 4
        );
    }
    #[test]
    fn world_actor_effect_order_and_authored_ties() {
        let input = vec![
            (20., Source::World(0)),
            (20., Source::World(1)),
            (30., Source::Actor(0)),
            (10., Source::Actor(1)),
        ];
        let sorted = order(input.clone(), true);
        assert_eq!(
            sorted.iter().map(|p| p.1).collect::<Vec<_>>(),
            vec![
                Source::Actor(0),
                Source::World(0),
                Source::World(1),
                Source::Actor(1)
            ]
        );
        assert_eq!(order(input.clone(), false), input);
    }
    #[test]
    fn actor_ranges_reject_truncated_and_out_of_bounds_indices() {
        let mut m = Mesh {
            vertices: vec![Vertex::new(0., 0., 0., 0., 0., WHITE); 3],
            indices: vec![0, 1, 2],
            texture: None,
        };
        assert!(valid_mesh(&m));
        m.indices.push(0);
        assert!(!valid_mesh(&m));
        m.indices = vec![0, 1, 3];
        assert!(!valid_mesh(&m));
    }
    fn fixture_group() -> super::super::Group {
        super::super::Group {
            transparent: true,
            passes: vec![Pass {
                blend_index: 1,
                depth_equal: false,
                mesh: Mesh {
                    vertices: vec![Vertex::new(0., 0., 0., 0., 0., WHITE); 3],
                    indices: vec![0, 1, 2],
                    texture: None,
                },
                material: None,
                lit: false,
                clamp: false,
                cutout: 0.,
                cutout_reference: 0.5,
                gain: Vec4::ONE,
                fog: 0.,
                model: true,
                appearance: Default::default(),
                filter_fade: 0.,
            }],
        }
    }
    #[test]
    fn coalescing_keeps_whole_mesh_order_and_world_barriers() {
        let mut groups = vec![fixture_group(), fixture_group(), fixture_group()];
        let mut list = vec![
            (3., Source::Actor(0)),
            (2., Source::Actor(1)),
            (1.5, Source::World(0)),
            (1., Source::Actor(2)),
        ];
        merge_adjacent(&mut groups, &mut list);
        assert_eq!(
            list.iter().map(|p| p.1).collect::<Vec<_>>(),
            vec![Source::Actor(0), Source::World(0), Source::Actor(2)]
        );
        assert_eq!(groups[0].passes[0].mesh.indices, vec![0, 1, 2, 3, 4, 5]);
        assert!(groups[1].passes.is_empty());
        assert_eq!(groups[2].passes[0].mesh.indices, vec![0, 1, 2]);
        // Layered/material differences must not be folded into one draw.
        groups = vec![fixture_group(), fixture_group()];
        groups[1].passes[0].appearance.ghost = true;
        let mut list = vec![(2., Source::Actor(0)), (1., Source::Actor(1))];
        merge_adjacent(&mut groups, &mut list);
        assert_eq!(list.len(), 2);
        groups[1].passes[0].appearance.ghost = false;
        groups[1].passes[0].mesh.indices[2] = 65535;
        merge_adjacent(&mut groups, &mut list);
        assert_eq!(list.len(), 2);
    }
    #[test]
    fn uniform_packets_keep_actor_appearance_and_depth_independent() {
        let mut group = fixture_group();
        let p = &mut group.passes[0];
        p.appearance = super::super::Appearance {
            dissolve: 0.4,
            ghost: true,
            power: 2.,
        };
        p.filter_fade = 0.25;
        let f = Frame {
            projection: Some(Mat4::IDENTITY),
            ..Default::default()
        };
        let u = uniforms(p, &f);
        assert_eq!(&u[48..52], &[0.4, 1., 2., 0.25]);
        p.appearance = Default::default();
        p.filter_fade = 0.;
        assert_eq!(&uniforms(p, &f)[48..52], &[0.; 4]);
    }
    #[test]
    fn deferred_depth_scope_restores_nested_and_empty_calls() {
        FRAME.with(|f| {
            *f.borrow_mut() = Frame {
                active: true,
                direct: true,
                ..Default::default()
            }
        });
        super::super::retained_depth_scope(|| {
            assert!(FRAME.with(|f| f.borrow().depth_read_only));
            super::super::retained_depth_scope(|| {});
            assert!(FRAME.with(|f| f.borrow().depth_read_only));
        });
        assert!(!FRAME.with(|f| f.borrow().depth_read_only));
        FRAME.with(|f| *f.borrow_mut() = Frame::default());
    }
}
