//! World batches, animated material layers and inline door models.
#[cfg(feature = "vita_renderer")]
pub(crate) mod uniform_fields;
use crate::{
    assets::Assets,
    bsp::Bsp,
    collision::World,
    environment::Atmosphere,
    materials::{Blend, Stage},
    texture,
};
use anyhow::{Context, Result};
use macroquad::{
    miniquad::{
        BlendFactor, BlendState, BlendValue, Comparison, Equation, PipelineParams, ShaderSource,
    },
    prelude::*,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::{Rc, Weak},
};

mod billboard_check;
mod sky_face_check;
#[cfg(test)]
mod visibility_tests;
pub use sky_face_check::check_school_floor;
mod water_check;
pub use water_check::check as check_water;
mod camera_portal;
#[cfg(feature = "vita_renderer")]
pub(crate) mod vita_renderer;
pub use billboard_check::check as check_billboards;

pub(crate) const BLEND_COUNT: usize = 109;

pub(crate) const VERTEX: &str = r#"#version 100
attribute vec3 position; attribute vec2 texcoord; attribute vec4 color0; attribute vec4 normal;
uniform mat4 Model; uniform mat4 Projection;
varying highp vec2 uv; varying highp vec3 lm; varying lowp vec4 color; varying highp vec3 worldPosition;
void main(){worldPosition=(Model*vec4(position,1.0)).xyz; gl_Position=Projection*vec4(worldPosition,1.0);uv=texcoord;lm=normal.xyz;color=color0/255.0;}
"#;
pub(crate) const FRAGMENT: &str = r#"#version 100
precision mediump float;
varying highp vec2 uv; varying highp vec3 lm; varying lowp vec4 color;
uniform sampler2D Texture; uniform sampler2D LightAtlas; uniform vec2 AtlasGrid;
uniform sampler2D SkyBackdrop; uniform vec2 SkySize; uniform float SkyFade;
uniform float Fullbright; uniform float Lit; uniform float ClampUV; uniform float Cutout; uniform vec4 Gain; uniform float FogMode;
uniform float CutoutReference;
uniform float FxModel; uniform vec4 FxAppearance;
// LIGHTS
// FOG
void main(){
 if(FxAppearance.x>0.0 && (mod(floor(gl_FragCoord.x)*3.0+floor(gl_FragCoord.y)*5.0,16.0)+0.5)/16.0<FxAppearance.x)discard;
 if(FxAppearance.y>0.5 && mod(floor(gl_FragCoord.x)+floor(gl_FragCoord.y),4.0)>0.5)discard;
 vec2 coords=mix(uv,clamp(uv,vec2(0.001),vec2(0.999)),ClampUV);vec4 diffuse=texture2D(Texture,coords);
 float alpha=diffuse.a*Gain.a*color.a;
 if((Cutout>0.5&&Cutout<1.5&&alpha<=0.0)||(Cutout>1.5&&Cutout<2.5&&alpha<0.5)||(Cutout>2.5&&Cutout<3.5&&alpha>=0.5)||(Cutout>3.5&&Cutout<4.5&&alpha<0.35))discard;
 if((Cutout>4.5&&Cutout<5.5&&alpha<CutoutReference)||(Cutout>5.5&&Cutout<6.5&&alpha<=CutoutReference)||(Cutout>6.5&&Cutout<7.5&&alpha>=CutoutReference)||(Cutout>7.5&&alpha>CutoutReference))discard;
 vec3 lighting=max(color.rgb,vec3(0.18));
 if(lm.z>0.5&&FxModel<0.5){float index=floor(lm.z-1.0+0.1);vec2 tile=vec2(mod(index,AtlasGrid.x),floor(index/AtlasGrid.x));vec2 sampleUV=(tile+clamp(lm.xy,vec2(0.5/128.0),vec2(127.5/128.0)))/AtlasGrid;lighting=min(vec3(1.0),texture2D(LightAtlas,sampleUV).rgb*2.0);}
 vec3 rgb=diffuse.rgb*mix(vec3(1.0),lighting,Lit*(1.0-Fullbright))*Gain.rgb;
 if(FxModel>0.5&&Lit<0.5)rgb*=color.rgb;
 rgb+=diffuse.rgb*dynamicLight(worldPosition)*Gain.rgb;
 if(FxAppearance.z>1.5)rgb=mix(rgb,rgb*vec3(0.65,1.15,0.7),0.6);else if(FxAppearance.z>0.5)rgb=mix(rgb,rgb*vec3(1.3,0.55,0.4),0.6);
 // Multiplicative actor layers fade toward their neutral blend colour.
 if(FxAppearance.w>0.0)rgb=mix(rgb,vec3(1.0),FxAppearance.w);else if(FxAppearance.w<0.0)rgb=mix(rgb,vec3(0.5),-FxAppearance.w);
 if(FogMode>3.5){
  // Source-alpha/source-colour needs white RGB and zero alpha to leave a
  // fully fogged destination unchanged.
  float visible=fogged(vec3(1.0),1.0).r;
  rgb=mix(vec3(1.0),rgb,visible);alpha*=visible;
 }else if(FogMode>=0.0)rgb=fogged(rgb,FogMode);
 // The remastered Fortress exterior fades its towers into the actual portal
 // sky, preserving the authored fog distances without black rectangular caps.
 if(SkyFade>0.5 && FogMode>=0.0 && FogMode<0.5){
  float a=FogA.w>0.0?clamp(fogLength(FogAMin,FogAMax)/FogA.w,0.0,1.0):0.0;
  float b=FogB.w>0.0?clamp(fogLength(FogBMin,FogBMax)/FogB.w,0.0,1.0):0.0;
  rgb+=texture2D(SkyBackdrop,gl_FragCoord.xy/SkySize).rgb*(1.0-(1.0-a)*(1.0-b));
 }
 gl_FragColor=vec4(rgb,alpha);
}"#;
struct Layer {
    spec: Stage,
    textures: Vec<Texture2D>,
    material: usize,
    lit: bool,
}
struct Batch {
    #[cfg(target_os = "vita")]
    static_offset: Option<Vec3>,
    face_side: crate::materials::FaceSide,
    motion: Option<crate::sky_sequence::Motion>,
    full_indices: Vec<u16>,
    parts: Vec<Part>,
    visibility_work: VisibilityWork,
    bounds: (Vec3, Vec3),
    detail: Vec<crate::model_detail::Level>,
    animation: Option<crate::ambient_animation::Frames>,
    mesh: Mesh,
    base: Vec<Vertex>,
    normals: Vec<Vec3>,
    deforms: Vec<crate::materials::Deform>,
    sky: bool,
    portal: bool,
    layers: Vec<Layer>,
    center: Vec3,
    model: Option<usize>,
}
pub struct Scene {
    #[cfg(feature = "vita_renderer")]
    retained: Option<vita_renderer::Renderer>,
    world_material_keepalive: WorldMaterialKeepalive,
    camera_portals: Vec<camera_portal::Portal>,
    rope_shapes: Vec<crate::rope::Strand>,
    pub optimize: bool,
    pub stats: DrawStats,
    pub map: Bsp,
    pub world: World,
    pub atmosphere: Atmosphere,
    batches: Vec<Batch>,
    face_mask_cache: FaceMaskCache,
    decoration_start: usize,
    materials: Vec<Rc<Material>>,
    bound_materials: Vec<Rc<Material>>,
    atlas: Texture2D,
    atlas_grid: Vec2,
    pub missing: Vec<String>,
    pub triangles: usize,
    sky_origin: Option<Vec3>,
    sky_rotation: Quat,
    sky_motion: Option<crate::sky_sequence::Motion>,
    sky_props: Vec<Batch>,
    sky_backdrop: Option<Texture2D>,
}
#[derive(Default, Clone, Copy, serde::Serialize)]
pub struct DrawStats {
    pub batches: usize,
    pub culled: usize,
    pub triangles: usize,
    pub detail_saved: usize,
}
struct Part {
    face: usize,
    indices: std::ops::Range<usize>,
    min: Vec3,
    max: Vec3,
}
#[derive(Default)]
struct VisibilityWork {
    pvs_parts: Vec<usize>,
    visible_parts: Vec<usize>,
    pvs: Option<PvsEntry>,
    parts_cover_full: bool,
}
enum PvsParts {
    All,
    Empty,
    SomeParts(Vec<usize>),
}
enum PvsResult {
    Parts(PvsParts),
    Bounds(bool),
}
struct PvsEntry {
    cluster: Option<usize>,
    result: PvsResult,
    bounds: Option<(Vec3, Vec3)>,
}
impl VisibilityWork {
    fn invalidate(&mut self) {
        if let Some(PvsEntry {
            result: PvsResult::Parts(PvsParts::SomeParts(parts)),
            ..
        }) = self.pvs.take()
        {
            self.pvs_parts = parts;
        }
        self.pvs_parts.clear();
        self.visible_parts.clear();
    }
    fn parts(&mut self, parts: &[Part], cluster: Option<usize>, faces: Option<&[bool]>) {
        if self
            .pvs
            .as_ref()
            .is_some_and(|e| e.cluster == cluster && matches!(e.result, PvsResult::Parts(_)))
        {
            return;
        }
        self.invalidate();
        for (i, p) in parts.iter().enumerate() {
            if !faces.is_some_and(|v| !v[p.face]) {
                self.pvs_parts.push(i);
            }
        }
        let result = if self.pvs_parts.len() == parts.len() {
            PvsParts::All
        } else if self.pvs_parts.is_empty() {
            PvsParts::Empty
        } else {
            PvsParts::SomeParts(std::mem::take(&mut self.pvs_parts))
        };
        self.pvs = Some(PvsEntry {
            cluster,
            result: PvsResult::Parts(result),
            bounds: None,
        });
    }
    fn bounds(
        &mut self,
        visibility: &crate::visibility::Visibility,
        cluster: Option<usize>,
        min: Vec3,
        max: Vec3,
    ) -> bool {
        if let Some(PvsEntry {
            cluster: key,
            result: PvsResult::Bounds(visible),
            bounds,
        }) = &self.pvs
        {
            if *key == cluster && *bounds == Some((min, max)) {
                return *visible;
            }
        }
        self.invalidate();
        let visible = visibility.bounds(cluster, min, max);
        self.pvs = Some(PvsEntry {
            cluster,
            result: PvsResult::Bounds(visible),
            bounds: Some((min, max)),
        });
        visible
    }
}
#[derive(Default)]
struct FaceMaskCache {
    // The outer None is uninitialised; a None cluster is a valid fail-open key.
    entry: Option<(Option<usize>, Option<Rc<[bool]>>)>,
}
impl FaceMaskCache {
    fn faces(
        &mut self,
        visibility: &crate::visibility::Visibility,
        cluster: Option<usize>,
    ) -> Option<Rc<[bool]>> {
        if let Some((key, faces)) = &self.entry {
            if *key == cluster {
                return faces.clone();
            }
        }
        let faces = visibility.faces(cluster).map(Rc::<[bool]>::from);
        self.entry = Some((cluster, faces.clone()));
        faces
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndexSelection {
    Full,
    Filtered,
    Detail(usize),
}
// This guard owns the batch borrow until all submissions have finished. A return
// or unwind restores both Vecs, so the immutable source cannot be lost.
struct IndexLease<'a> {
    batch: &'a mut Batch,
    swapped: bool,
}
impl std::ops::Deref for IndexLease<'_> {
    type Target = Batch;
    fn deref(&self) -> &Batch {
        self.batch
    }
}
impl std::ops::DerefMut for IndexLease<'_> {
    fn deref_mut(&mut self) -> &mut Batch {
        self.batch
    }
}
impl Drop for IndexLease<'_> {
    fn drop(&mut self) {
        if self.swapped {
            std::mem::swap(&mut self.batch.mesh.indices, &mut self.batch.full_indices);
        }
    }
}
impl<'a> IndexLease<'a> {
    fn new(batch: &'a mut Batch, selection: IndexSelection, read_only: bool) -> Self {
        let _indices = crate::frame_profile::span("world_visibility_indices");
        let swapped = selection == IndexSelection::Full && read_only;
        if swapped {
            std::mem::swap(&mut batch.mesh.indices, &mut batch.full_indices);
        } else {
            batch.mesh.indices.clear();
            match selection {
                IndexSelection::Full => batch.mesh.indices.extend_from_slice(&batch.full_indices),
                IndexSelection::Filtered => {
                    for &i in &batch.visibility_work.visible_parts {
                        batch
                            .mesh
                            .indices
                            .extend_from_slice(&batch.full_indices[batch.parts[i].indices.clone()]);
                    }
                }
                IndexSelection::Detail(i) => batch
                    .mesh
                    .indices
                    .extend_from_slice(&batch.detail[i].indices),
            }
        }
        Self { batch, swapped }
    }
}
struct DrawContext<'a> {
    enabled: bool,
    required_pvs: bool,
    faces: Option<Rc<[bool]>>,
    cluster: Option<usize>,
    visibility: &'a crate::visibility::Visibility,
    stats: DrawStats,
}
/// Lossy visual cap; collision, navigation, sound and simulation are unchanged.
pub(crate) fn draw_distance_far(original: f32) -> f32 {
    #[cfg(feature="vita_renderer")]
    if let Some(cap) = vita_renderer::config().draw_distance_cap { return original.min(cap); }
    original
}
fn transform_bounds(
    min: Vec3,
    max: Vec3,
    transform: Option<&(usize, Vec3, Quat)>,
    offset: Vec3,
) -> (Vec3, Vec3) {
    #[cfg(feature = "vita_renderer")]
    if vita_renderer::config().fast_frustum && transform.is_none() {
        return (min.min(max) + offset, min.max(max) + offset);
    }
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = -lo;
    for x in [min.x, max.x] {
        for y in [min.y, max.y] {
            for z in [min.z, max.z] {
                let p = vec3(x, y, z);
                let p = transform.map_or(p, |(_, o, r)| *o + *r * p) + offset;
                lo = lo.min(p);
                hi = hi.max(p);
            }
        }
    }
    (lo, hi)
}
pub(crate) fn outside_bounds(min: Vec3, max: Vec3) -> bool {
    let Some(m) = crate::render_fx::projection() else {
        return false;
    };
    outside_matrix(m, min, max)
}
pub(crate) fn outside_matrix(m: Mat4, min: Vec3, max: Vec3) -> bool {
    #[cfg(feature = "vita_renderer")]
    if vita_renderer::config().fast_frustum { return aabb_outside_matrix(m, min, max); }
    corner_outside_matrix(m, min, max)
}
fn corner_outside_matrix(m: Mat4, min: Vec3, max: Vec3) -> bool {
    let points = [
        vec3(min.x, min.y, min.z),
        vec3(min.x, min.y, max.z),
        vec3(min.x, max.y, min.z),
        vec3(min.x, max.y, max.z),
        vec3(max.x, min.y, min.z),
        vec3(max.x, min.y, max.z),
        vec3(max.x, max.y, min.z),
        vec3(max.x, max.y, max.z),
    ]
    .map(|p| m * p.extend(1.));
    (0..3).any(|a| points.iter().all(|p| p[a] < -p.w) || points.iter().all(|p| p[a] > p.w))
}
// Adapted from NextOs-Ports/Looking-Glass-NextOS 668de14 (felc18-blip).
fn aabb_outside_matrix(m: Mat4, min: Vec3, max: Vec3) -> bool {
    if !(m.is_finite() && min.is_finite() && max.is_finite()) { return false; }
    let center = min * 0.5 + max * 0.5;
    let extent = (max * 0.5 - min * 0.5).abs();
    let rows = m.transpose();
    for row in [rows.x_axis, rows.y_axis, rows.z_axis] {
        for sign in [-1., 1.] {
            let plane = rows.w_axis + row * sign;
            let distance = plane.truncate().dot(center) + plane.w;
            let radius = plane.truncate().abs().dot(extent);
            // Bound uncancelled corner/plane arithmetic, including oblique views.
            let magnitude = (rows.w_axis.abs() + row.abs()).truncate().dot(center.abs() + extent)
                + rows.w_axis.w.abs() + row.w.abs() + 1.;
            let slack = 32. * f32::EPSILON * magnitude;
            if distance + radius < -slack { return true; }
        }
    }
    false
}

#[cfg(test)]
mod fast_frustum_tests {
    use super::*;
    #[test]
    fn ten_thousand_boxes_never_reject_corner_reference_visibility() {
        let projection = Mat4::perspective_rh_gl(1.1, 16. / 9., 0.1, 10000.);
        let view = Mat4::look_at_rh(vec3(100., 200., 300.), Vec3::ZERO, Vec3::Z);
        let mut seed = 7_u32;
        for i in 0..10000 {
            let mut random = || { seed = seed.wrapping_mul(1664525).wrapping_add(1013904223); (seed >> 8) as f32 / 16777216. };
            let center = vec3(random(), random(), random()) * 20000. - Vec3::splat(10000.);
            let extent = vec3(random(), random(), random()) * 500. + Vec3::ONE;
            let mut m = projection * view;
            // Reflected and oblique clip matrices use the same homogeneous planes.
            if i % 3 == 1 { m = m * Mat4::from_scale(vec3(-1., 1., 1.)); }
            if i % 3 == 2 { m.z_axis += m.x_axis * 0.37; }
            let lo = center - extent; let hi = center + extent;
            assert!(!aabb_outside_matrix(m, lo, hi) || corner_outside_matrix(m, lo, hi), "box {i}");
            assert_eq!(aabb_outside_matrix(m, lo, hi), aabb_outside_matrix(m, hi, lo));
        }
    }
    #[test]
    fn near_plane_camera_crossing_behind_camera_and_ambiguous_edges() {
        let m = Mat4::perspective_rh_gl(1.1, 16. / 9., 0.1, 10000.);
        for (lo, hi, outside) in [
            (vec3(-0.01,-0.01,-0.10001),vec3(0.01,0.01,-0.09999),false),
            (vec3(-1.,-1.,-2.),vec3(1.,1.,2.),false),
            (vec3(-0.01,-0.01,1.),vec3(0.01,0.01,2.),true),
            (vec3(-0.001,-0.001,-0.05),vec3(0.001,0.001,-0.01),true),
            (vec3(-1.,-1.,-100.),vec3(1.,1.,-90.),false),
        ] {
            assert_eq!(aabb_outside_matrix(m,lo,hi),outside);
            assert!(!aabb_outside_matrix(m,lo,hi) || corner_outside_matrix(m,lo,hi));
        }
        assert!(!aabb_outside_matrix(Mat4::IDENTITY,Vec3::splat(-1.),Vec3::ONE));
        assert!(aabb_outside_matrix(Mat4::IDENTITY,Vec3::splat(2.),Vec3::splat(3.)));
        assert!(!aabb_outside_matrix(m,Vec3::splat(f32::NAN),Vec3::ONE));
        assert!(!aabb_outside_matrix(m,Vec3::splat(-f32::MAX),Vec3::splat(f32::MAX)));
    }
    #[test]
    fn identity_bounds_shortcut_matches_corner_arithmetic() {
        let lo = vec3(-17., 12., -3.); let hi = vec3(35., 47., 83.); let offset = vec3(100.,-400.,21.);
        assert_eq!(transform_bounds(lo,hi,None,offset),(lo+offset,hi+offset));
        assert_eq!(transform_bounds(hi,lo,None,offset),(lo+offset,hi+offset));
    }
}
fn selected_mesh(mesh: &Mesh, compact: bool) -> Mesh {
    if !compact {
        return crate::render_fx::copy_mesh(mesh);
    }
    let mut mapping = vec![u16::MAX; mesh.vertices.len()];
    let mut vertices = Vec::new();
    let indices = mesh
        .indices
        .iter()
        .map(|&i| {
            let next = &mut mapping[i as usize];
            if *next == u16::MAX {
                *next = vertices.len() as u16;
                vertices.push(mesh.vertices[i as usize]);
            }
            *next
        })
        .collect();
    Mesh {
        vertices,
        indices,
        texture: mesh.texture.clone(),
    }
}
fn upload(image: texture::RgbaImage) -> Texture2D {
    let t = crate::texture::upload_rgba8(image.width, image.height, &image.pixels);
    t.set_filter(FilterMode::Linear);
    // World UVs tile. Use the sampler's wrap instead of fract() in the shader:
    // fract introduces derivative discontinuities and incorrect mip levels.
    unsafe {
        let gl = get_internal_gl();
        gl.quad_context.texture_set_wrap(
            t.raw_miniquad_id(),
            macroquad::miniquad::TextureWrap::Repeat,
            macroquad::miniquad::TextureWrap::Repeat,
        );
        gl.quad_context
            .texture_generate_mipmaps(t.raw_miniquad_id());
        gl.quad_context.texture_set_min_filter(
            t.raw_miniquad_id(),
            FilterMode::Linear,
            macroquad::miniquad::MipmapFilterMode::Linear,
        );
    }
    t
}
pub(crate) fn blend_index(b: Blend) -> usize {
    match b {
        Blend::Opaque => 0,
        Blend::Alpha => 1,
        Blend::Add => 2,
        Blend::Filter => 3,
        Blend::DoubleFilter => 4,
        Blend::AlphaAdd => 5,
        Blend::ColorAdd => 6,
        Blend::Invisible => 7,
        Blend::AlphaColor => 8,
        Blend::Factors(s, d) => 9 + s as usize * 10 + d as usize,
    }
}
pub(crate) fn blend(i: usize) -> Option<BlendState> {
    if i >= 9 {
        let factor = |i| match i {
            0 => BlendFactor::Zero,
            1 => BlendFactor::One,
            2 => BlendFactor::Value(BlendValue::SourceColor),
            3 => BlendFactor::OneMinusValue(BlendValue::SourceColor),
            4 => BlendFactor::Value(BlendValue::DestinationColor),
            5 => BlendFactor::OneMinusValue(BlendValue::DestinationColor),
            6 => BlendFactor::Value(BlendValue::SourceAlpha),
            7 => BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
            8 => BlendFactor::Value(BlendValue::DestinationAlpha),
            _ => BlendFactor::OneMinusValue(BlendValue::DestinationAlpha),
        };
        return Some(BlendState::new(
            Equation::Add,
            factor((i - 9) / 10),
            factor((i - 9) % 10),
        ));
    }
    let (s, d) = match i {
        0 => return None,
        1 => (
            BlendFactor::Value(BlendValue::SourceAlpha),
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        ),
        2 => (BlendFactor::One, BlendFactor::One),
        3 => (
            BlendFactor::Value(BlendValue::DestinationColor),
            BlendFactor::Zero,
        ),
        4 => (
            BlendFactor::Value(BlendValue::DestinationColor),
            BlendFactor::Value(BlendValue::SourceColor),
        ),
        5 => (
            BlendFactor::Value(BlendValue::SourceAlpha),
            BlendFactor::One,
        ),
        7 => (BlendFactor::Zero, BlendFactor::One),
        8 => (
            BlendFactor::Value(BlendValue::SourceAlpha),
            BlendFactor::Value(BlendValue::SourceColor),
        ),
        _ => (
            BlendFactor::Value(BlendValue::DestinationColor),
            BlendFactor::One,
        ),
    };
    Some(BlendState::new(Equation::Add, s, d))
}
pub(crate) type WorldMaterialKeepalive = Rc<RefCell<BTreeMap<usize, Rc<Material>>>>;
pub(crate) fn activate_world_materials(owner:&WorldMaterialKeepalive) {
    ACTIVE_WORLD_MATERIALS.with(|active| *active.borrow_mut()=Rc::downgrade(owner));
}
thread_local! {
    static ACTIVE_WORLD_MATERIALS: RefCell<Weak<RefCell<BTreeMap<usize,Rc<Material>>>>> = RefCell::default();
    // Macroquad has only 32 simultaneous pipeline slots. Save restoration and
    // map transitions keep two scenes alive; share immutable pipeline definitions.
    // Weak sharing avoids permanently consuming Macroquad slots. Each active
    // scene also owns its effect materials: a weak-only cache recompiles passes
    // after the retained renderer releases the old world materials.
    static WORLD_MATERIALS: RefCell<BTreeMap<usize, Weak<Material>>> = RefCell::default();
}
pub(crate) fn world_material(index: usize) -> Result<Rc<Material>> {
    let owner=ACTIVE_WORLD_MATERIALS.with(|active| active.borrow().upgrade());
    if let Some(material)=owner.as_ref().and_then(|o|o.borrow().get(&index).cloned()) { return Ok(material); }
    WORLD_MATERIALS.with(|cache| {
        if let Some(material) = cache.borrow().get(&index).and_then(Weak::upgrade) {
            if let Some(owner)=&owner { owner.borrow_mut().insert(index,material.clone()); }
            return Ok(material);
        }
        let fragment = crate::lighting::fragment(&crate::environment::fragment(FRAGMENT));
        let mut uniforms = vec![
            UniformDesc::new("AtlasGrid", UniformType::Float2),
            UniformDesc::new("Fullbright", UniformType::Float1),
            UniformDesc::new("Lit", UniformType::Float1),
            UniformDesc::new("ClampUV", UniformType::Float1),
            UniformDesc::new("Cutout", UniformType::Float1),
            UniformDesc::new("CutoutReference", UniformType::Float1),
            UniformDesc::new("Gain", UniformType::Float4),
            UniformDesc::new("FogMode", UniformType::Float1),
            UniformDesc::new("SkySize", UniformType::Float2),
            UniformDesc::new("SkyFade", UniformType::Float1),
            UniformDesc::new("FxModel", UniformType::Float1),
            UniformDesc::new("FxAppearance", UniformType::Float4),
        ];
        uniforms.extend(crate::lighting::uniforms());
        uniforms.extend(crate::environment::uniforms());
        let material = Rc::new(
            crate::frame_profile::load_material(
                ShaderSource::Glsl {
                    vertex: VERTEX,
                    fragment: &fragment,
                },
                MaterialParams {
                    pipeline_params: PipelineParams {
                        depth_test: if index >= BLEND_COUNT {
                            Comparison::Equal
                        } else {
                            Comparison::LessOrEqual
                        },
                        ..depth_pipeline(blend(index % BLEND_COUNT))
                    },
                    uniforms,
                    textures: vec!["LightAtlas".into(), "SkyBackdrop".into()],
                },
            )
            .map_err(|e| anyhow::anyhow!("World shader: {e:?}"))?,
        );
        cache.borrow_mut().insert(index, Rc::downgrade(&material));
        if let Some(owner)=&owner { owner.borrow_mut().insert(index,material.clone()); }
        Ok(material)
    })
}
#[allow(clippy::too_many_arguments)]
fn load_layers(
    assets: &mut Assets,
    name: &str,
    spec: Option<&texture::MaterialSpec>,
    specs: &BTreeMap<String, texture::MaterialSpec>,
    cache: &mut BTreeMap<String, Texture2D>,
    fallback: &Texture2D,
    white: &Texture2D,
    missing: &mut Vec<String>,
) -> Result<Vec<Layer>> {
    let lightmapped = spec.is_some_and(|s| {
        s.stages
            .iter()
            .any(|p| p.images.iter().any(|i| i == "$lightmap"))
    });
    let mut stages = world_stages(spec);
    if stages.is_empty() {
        stages.push(Stage {
            images: vec![name.to_owned()],
            ..Default::default()
        });
    }
    let mut layers = Vec::new();
    for (index, mut stage) in stages.into_iter().enumerate() {
        if index == 0
            && lightmapped
            && !spec.is_some_and(|s| s.transparent)
            && stage.blend == Blend::Filter
        {
            stage.blend = Blend::Opaque;
        }
        let mut textures = Vec::new();
        for image in &stage.images {
            if image == "$whiteimage" || image == "*white" {
                textures.push(white.clone());
                continue;
            }
            // Stage names resolve directly, never recursively through their material.
            let path = texture::resolve(assets, image, &BTreeMap::new()).or_else(|| {
                if image == name {
                    texture::resolve(assets, image, specs)
                } else {
                    None
                }
            });
            let tex = if let Some(path) = path {
                if let Some(tex) = cache.get(&path) {
                    tex.clone()
                } else {
                    match texture::decode(assets, &path) {
                        Ok(img) => {
                            let tex = upload(img);
                            cache.insert(path, tex.clone());
                            tex
                        }
                        Err(e) => {
                            eprintln!("Texture {path}: {e:#}");
                            missing.push(path);
                            fallback.clone()
                        }
                    }
                }
            } else {
                missing.push(image.clone());
                fallback.clone()
            };
            textures.push(tex);
        }
        let lit = index == 0 && !spec.is_some_and(|s| s.unlit) && (lightmapped || !stage.identity);
        layers.push(Layer {
            material: blend_index(stage.blend)
                + if stage.depth_equal && index > 0 {
                    BLEND_COUNT
                } else {
                    0
                },
            spec: stage,
            textures,
            lit,
        });
    }
    Ok(layers)
}
fn world_stages(spec: Option<&texture::MaterialSpec>) -> Vec<Stage> {
    let Some(spec) = spec else { return Vec::new() };
    let mut stages: Vec<_> = spec
        .stages
        .iter()
        .filter(|s| !s.images.iter().any(|i| i == "$lightmap") && !s.unsupported)
        .cloned()
        .collect();
    if let Some(base) = spec
        .stages
        .first()
        .filter(|s| s.images.iter().any(|i| i == "$lightmap") && s.blend == Blend::Opaque)
    {
        // Only an immediately following filter can absorb the lightmap into
        // its diffuse draw. Detail/additive stages need the original opaque
        // destination first, or whole floor triangles blend over one another.
        if spec.transparent || stages.first().is_none_or(|s| s.blend != Blend::Filter) {
            let mut base = base.clone();
            base.images = vec!["$whiteimage".into()];
            stages.insert(0, base);
        }
    }
    stages
}
impl Batch {
    fn prepare_visibility(&mut self) {
        self.visibility_work.invalidate();
        self.visibility_work.pvs_parts.reserve(self.parts.len());
        self.visibility_work.visible_parts.reserve(self.parts.len());
        self.mesh.indices.reserve(
            self.full_indices
                .len()
                .saturating_sub(self.mesh.indices.len()),
        );
        let mut end = 0;
        self.visibility_work.parts_cover_full = self.parts.iter().all(|p| {
            let contiguous = p.indices.start == end;
            end = p.indices.end;
            contiguous
        }) && end == self.full_indices.len();
    }
    fn indices_read_only(&self, queue_active: bool) -> bool {
        self.face_side == crate::materials::FaceSide::Both && (!self.transparent() || queue_active)
    }
    fn transparent(&self) -> bool {
        let s = &self.layers[0].spec;
        !self.sky && s.blend != Blend::Opaque && !(s.depth_write && s.alpha_test > 0)
    }
}
// Autosprite2 turns around the quad's long axis, preserving its two edge
// midpoints. UVs may be offset/tiled/animated and must never drive geometry.
fn axial_billboard(quad: &mut [Vertex], camera: Vec3) {
    let first = if quad[0].position.distance_squared(quad[1].position)
        <= quad[1].position.distance_squared(quad[2].position)
    {
        0
    } else {
        1
    };
    let ids = [first, (first + 1) % 4, (first + 2) % 4, (first + 3) % 4];
    let [a, b, c, d] = ids.map(|i| quad[i].position);
    let lower = (a + b) * 0.5;
    let upper = (c + d) * 0.5;
    let Some(axis) = (upper - lower).try_normalize() else {
        return;
    };
    let authored_right = (b - a).normalize_or_zero();
    let mut right = axis
        .cross(camera - (lower + upper) * 0.5)
        .try_normalize()
        .unwrap_or(authored_right);
    // Both sides are visible. Choose the facing plane's orientation nearest
    // the authored edge, instead of turning the texture over behind the plane.
    // This matters for wide/near-square flames whose locked axis is horizontal
    // (including skool1's 72 x 70 fireplace), and for tilted effects on movers.
    if right.dot(authored_right) < 0. {
        right = -right;
    }
    let low_radius = a.distance(b) * 0.5;
    let high_radius = c.distance(d) * 0.5;
    for (i, position) in ids.into_iter().zip([
        lower - right * low_radius,
        lower + right * low_radius,
        upper + right * high_radius,
        upper - right * high_radius,
    ]) {
        quad[i].position = position;
    }
}

// The production callbacks use the current projection. Tests supply the same
// geometry calculations without requiring a window or game archives.
fn select_batch_indices(
    batch: &mut Batch,
    transform: Option<&(usize, Vec3, Quat)>,
    offset: Vec3,
    backdrop: bool,
    context: &mut DrawContext<'_>,
) -> Option<IndexSelection> {
    select_batch_indices_for(
        batch,
        transform,
        offset,
        backdrop,
        context,
        outside_bounds,
        crate::render_fx::projected_error,
    )
}
#[allow(clippy::too_many_arguments)]
fn select_batch_indices_for(
    batch: &mut Batch,
    transform: Option<&(usize, Vec3, Quat)>,
    offset: Vec3,
    backdrop: bool,
    context: &mut DrawContext<'_>,
    outside: impl Fn(Vec3, Vec3) -> bool,
    projected_error: impl Fn(Vec3, f32, f32) -> f32,
) -> Option<IndexSelection> {
    let bounds_profile = crate::frame_profile::span("world_visibility_bounds");
    let (min, max) = transform_bounds(batch.bounds.0, batch.bounds.1, transform, offset);
    let can_cull = (context.enabled || context.required_pvs)
        && !batch
            .deforms
            .iter()
            .any(crate::materials::Deform::moves_vertices)
        && batch.motion.is_none();
    if can_cull && context.enabled && outside(min, max) {
        return None;
    }
    drop(bounds_profile);
    let selection = if can_cull && !batch.parts.is_empty() {
        {
            let _pvs = crate::frame_profile::span("world_visibility_pvs");
            batch
                .visibility_work
                .parts(&batch.parts, context.cluster, context.faces.as_deref());
        }
        let _bounds = crate::frame_profile::span("world_visibility_bounds");
        let work = &mut batch.visibility_work;
        work.visible_parts.clear();
        let mut index_count = 0;
        let mut visit = |i: usize| {
            let p = &batch.parts[i];
            let (lo, hi) = transform_bounds(p.min, p.max, transform, offset);
            if !context.enabled || !outside(lo, hi) {
                work.visible_parts.push(i);
                index_count += p.indices.len();
            }
        };
        match &work.pvs.as_ref().unwrap().result {
            PvsResult::Parts(PvsParts::All) => (0..batch.parts.len()).for_each(&mut visit),
            PvsResult::Parts(PvsParts::SomeParts(parts)) => {
                parts.iter().copied().for_each(&mut visit)
            }
            PvsResult::Parts(PvsParts::Empty) => {}
            PvsResult::Bounds(_) => unreachable!(),
        }
        if index_count == 0 {
            return None;
        }
        if work.visible_parts.len() == batch.parts.len() && work.parts_cover_full {
            IndexSelection::Full
        } else {
            IndexSelection::Filtered
        }
    } else {
        {
            let _pvs = crate::frame_profile::span("world_visibility_pvs");
            // Preserve the original mover, animation, deformation and sky guards.
            if can_cull
                && batch.model.is_none()
                && batch.animation.is_none()
                && batch.parts.is_empty()
                && !backdrop
            {
                // Only the normal fixed pose may reuse bounds membership. A
                // transformed view computes it directly and leaves the cache intact.
                let visible = if transform.is_none() && offset == Vec3::ZERO {
                    batch
                        .visibility_work
                        .bounds(context.visibility, context.cluster, min, max)
                } else {
                    context.visibility.bounds(context.cluster, min, max)
                };
                if !visible {
                    return None;
                }
            }
        }
        if batch.full_indices.is_empty() {
            return None;
        }
        IndexSelection::Full
    };
    let _bounds = crate::frame_profile::span("world_visibility_bounds");
    if can_cull && context.enabled && !backdrop && batch.parts.is_empty() {
        let center = (min + max) * 0.5;
        let radius = (max - min).length() * 0.5;
        if let Some(i) = batch
            .detail
            .iter()
            .rposition(|l| projected_error(center, radius, l.error) <= 0.35)
        {
            context.stats.detail_saved +=
                (batch.full_indices.len() - batch.detail[i].indices.len()) / 3;
            return Some(IndexSelection::Detail(i));
        }
    }
    Some(selection)
}

#[allow(clippy::too_many_arguments)]
fn draw_batch(
    batch: &mut Batch,
    materials: &[Rc<Material>],
    camera: Vec3,
    time: f32,
    transform: Option<&(usize, Vec3, Quat)>,
    offset: Vec3,
    backdrop: bool,
    context: &mut DrawContext<'_>,
) {
    let _visibility_profile = crate::frame_profile::span("world_visibility");
    context.stats.batches += 1;
    let Some(selection) = select_batch_indices(batch, transform, offset, backdrop, context) else {
        context.stats.culled += 1;
        #[cfg(target_os = "vita")]
        crate::frame_profile::record_culled();
        return;
    };
    let full_index_count = batch.full_indices.len();
    let read_only = batch.indices_read_only(crate::render_fx::active());
    let mut index_lease = IndexLease::new(batch, selection, read_only);
    let batch = &mut *index_lease;
    drop(_visibility_profile);
    context.stats.triangles += batch.mesh.indices.len() / 3 * batch.layers.len();
    #[cfg(target_os = "vita")]
    let static_batch = batch.animation.is_none()
        && batch.deforms.is_empty()
        && batch.motion.is_none()
        && transform.is_none();
    #[cfg(target_os = "vita")]
    let retransform = !(crate::frame_profile::mode().skip_static_prep()
        && static_batch
        && batch.static_offset == Some(offset));
    #[cfg(not(target_os = "vita"))]
    let retransform = true;
    #[cfg(target_os = "vita")]
    let prep_profile = crate::frame_profile::world_span("world_prep", static_batch);
    if retransform {
        for (j, (v, base)) in batch.mesh.vertices.iter_mut().zip(&batch.base).enumerate() {
            // Billboards use source UV corners to build geometry. Animated layer
            // UVs from the previous frame must not move or rotate those corners.
            v.uv = base.uv;
            let mut p = batch
                .animation
                .as_ref()
                .map_or(base.position, |animation| animation.position(j, time));
            for deform in &batch.deforms {
                p = deform.position(p, batch.normals[j], base.uv, time);
            }
            if let Some(motion) = &batch.motion {
                p = motion.point(p, time);
            }
            v.position = transform.map_or(p, |(_, o, r)| *o + *r * p) + offset;
        }
    }
    #[cfg(target_os = "vita")]
    {
        batch.static_offset = if static_batch { Some(offset) } else { None };
    }
    if let Some(locked) = batch.deforms.iter().find_map(|d| {
        if let crate::materials::Deform::Sprite(lock) = d {
            Some(*lock)
        } else {
            None
        }
    }) {
        for quad in batch.mesh.vertices.chunks_exact_mut(4) {
            if locked {
                axial_billboard(quad, camera);
                continue;
            }
            let center = quad.iter().map(|v| v.position).sum::<Vec3>() / 4.;
            let direction = (camera - center).normalize_or_zero();
            let right = Vec3::Z.cross(direction).try_normalize().unwrap_or(Vec3::X);
            let up = direction.cross(right).normalize_or_zero();
            let radius = quad[0].position.distance(center) * 0.70710677;
            for v in quad {
                v.position = center
                    + right * ((v.uv.x - 0.5) * 2. * radius)
                    + up * ((0.5 - v.uv.y) * 2. * radius);
            }
        }
    }
    // TAN front faces use clockwise winding. Sky models are viewed from
    // inside their miniature set; showing their reverse faces exposes stretched
    // closure polygons beneath the school floor. Test the final animated pose.
    if batch.face_side != crate::materials::FaceSide::Both {
        let mut kept = 0;
        for i in (0..batch.mesh.indices.len()).step_by(3) {
            let ids = [
                batch.mesh.indices[i],
                batch.mesh.indices[i + 1],
                batch.mesh.indices[i + 2],
            ];
            let points = ids.map(|id| batch.mesh.vertices[id as usize].position);
            if batch.face_side.visible(points, camera) {
                batch.mesh.indices[kept..kept + 3].copy_from_slice(&ids);
                kept += 3;
            }
        }
        batch.mesh.indices.truncate(kept);
    }
    if batch.transparent() && !crate::render_fx::active() {
        let mut triangles = batch
            .mesh
            .indices
            .chunks_exact(3)
            .map(|t| [t[0], t[1], t[2]])
            .collect::<Vec<_>>();
        let distance = |t: &[u16; 3]| {
            (t.iter()
                .map(|i| batch.mesh.vertices[*i as usize].position)
                .sum::<Vec3>()
                / 3.)
                .distance_squared(camera)
        };
        triangles.sort_by(|a, b| distance(b).total_cmp(&distance(a)));
        batch.mesh.indices.clear();
        batch.mesh.indices.extend(triangles.into_iter().flatten());
    }
    #[cfg(target_os = "vita")]
    drop(prep_profile);
    #[cfg(target_os = "vita")]
    if batch.mesh.indices.is_empty() {
        crate::frame_profile::record_culled();
    } else {
        crate::frame_profile::record_batch(static_batch, retransform);
    }
    let mut queued = Vec::new();
    for layer in &batch.layers {
        let material = &materials[layer.material];
        #[cfg(target_os = "vita")]
        let layer_prep = crate::frame_profile::world_span("world_prep", static_batch);
        #[cfg(target_os = "vita")]
        crate::frame_profile::counts(&[
            ("world_layer_vertices", batch.mesh.vertices.len() as u64),
            ("world_prep_layers", 1),
        ]);

        for (j, (v, base)) in batch.mesh.vertices.iter_mut().zip(&batch.base).enumerate() {
            let mut n = batch.normals[j];
            for deform in &batch.deforms {
                n = deform.normal(base.position, n, time);
            }
            let n = batch.motion.as_ref().map_or(n, |m| m.normal(n, time));
            let normal = transform.map_or(n, |(_, _, r)| *r * n);
            let uv = if batch.sky && !batch.portal {
                let ray = (v.position - camera).normalize_or_zero();
                // Infinite, curved cloud projection; no translation with player motion.
                ray.truncate() / (ray.z.abs() + 0.25) * 0.25 + Vec2::splat(0.5)
            } else {
                base.uv
            };
            v.uv = layer.spec.view_uv(uv, v.position, normal, camera, time);
            v.color = base.color;
            v.color[3] = (255.
                * layer.spec.opacity(
                    normal,
                    (camera - v.position).normalize_or_zero(),
                    base.color[3],
                )) as u8;
        }
        batch.mesh.texture = Some(layer.textures[layer.spec.frame(time)].clone());
        #[cfg(target_os = "vita")]
        drop(layer_prep);
        #[cfg(target_os = "vita")]
        let _submit = crate::frame_profile::world_span("world_submit", static_batch);
        if crate::render_fx::active() && batch.transparent() && !backdrop && !batch.sky {
            if let Ok(mut pass) = crate::render_fx::pass(
                selected_mesh(&batch.mesh, batch.mesh.indices.len() < full_index_count),
                &layer.spec,
                false,
                layer.lit,
                Default::default(),
            ) {
                #[cfg(target_os = "vita")]
                {
                    pass.world_static = Some(static_batch);
                }
                pass.material = Some(material.clone());
                queued.push(pass);
            }
            continue;
        }
        material.set_uniform("FxModel", 0_f32);
        material.set_uniform("FxAppearance", Vec4::ZERO);
        crate::lighting::apply(
            material,
            false,
            crate::render_fx::active() && layer.lit && !backdrop && !batch.sky,
        );
        material.set_uniform(
            "Lit",
            if layer.lit && !batch.sky && !backdrop {
                1_f32
            } else {
                0.
            },
        );
        material.set_uniform("ClampUV", if layer.spec.clamp { 1_f32 } else { 0. });
        material.set_uniform("Cutout", layer.spec.alpha_test as f32);
        material.set_uniform("CutoutReference", layer.spec.alpha_reference);
        let gain = layer
            .spec
            .rgb
            .as_ref()
            .map(|w| w.sample(time))
            .unwrap_or(1.)
            .clamp(0., 2.);
        let rgb = layer.spec.constant_rgb.unwrap_or(Vec3::ONE) * gain;
        material.set_uniform(
            "Gain",
            rgb.extend(
                layer
                    .spec
                    .alpha
                    .as_ref()
                    .map(|w| w.sample(time))
                    .or(layer.spec.constant_alpha)
                    .unwrap_or(1.)
                    .clamp(0., 1.),
            ),
        );
        material.set_uniform(
            "FogMode",
            if backdrop || batch.sky {
                -1_f32
            } else {
                match layer.spec.blend {
                    Blend::Add | Blend::AlphaAdd | Blend::ColorAdd => 1.,
                    Blend::Filter => 2.,
                    Blend::DoubleFilter => 3.,
                    Blend::AlphaColor => 4.,
                    _ => 0.,
                }
            },
        );
        gl_use_material(material);
        if batch.mesh.indices.len() < full_index_count {
            let mesh = selected_mesh(&batch.mesh, true);
            #[cfg(target_os = "vita")]
            if !mesh.indices.is_empty() {
                crate::frame_profile::record_layer(
                    mesh.indices.len() / 3,
                    mesh.vertices.len(),
                    static_batch,
                );
            }
            #[cfg(target_os = "vita")]
            crate::frame_profile::record_lg_submission();
            draw_mesh(&mesh);
        } else {
            #[cfg(target_os = "vita")]
            if !batch.mesh.indices.is_empty() {
                crate::frame_profile::record_layer(
                    batch.mesh.indices.len() / 3,
                    batch.mesh.vertices.len(),
                    static_batch,
                );
            }
            #[cfg(target_os = "vita")]
            crate::frame_profile::record_lg_submission();
            draw_mesh(&batch.mesh);
        }
    }
    if !queued.is_empty() {
        #[cfg(target_os = "vita")]
        let _submit = crate::frame_profile::world_span("world_submit", static_batch);
        crate::render_fx::submit(queued, true);
    }
}

impl Scene {
    pub(crate) fn retained_active(&self) -> bool {
        #[cfg(feature = "vita_renderer")]
        { self.retained.is_some() }
        #[cfg(not(feature = "vita_renderer"))]
        { false }
    }

    /// An isolated original prop proves the detail threshold independently of
    /// whether the entrance camera happens to contain a distant decoration.
    pub async fn check_detail(&mut self) -> Result<()> {
        use anyhow::ensure;
        self.bind_atlas();
        let index = self
            .batches
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                !b.detail.is_empty()
                    && b.model.is_none()
                    && b.motion.is_none()
                    && b.deforms.is_empty()
                    && !b.transparent()
            })
            .min_by(|(_, a), (_, b)| {
                let ratio =
                    |b: &Batch| b.detail[0].error / (b.bounds.1 - b.bounds.0).length().max(0.001);
                ratio(a).total_cmp(&ratio(b))
            })
            .map(|(i, _)| i)
            .context("No authored model collapse data in scene")?;
        let b = &self.batches[index];
        let center = (b.bounds.0 + b.bounds.1) * 0.5;
        let radius = (b.bounds.1 - b.bounds.0).length() * 0.5;
        let far = (b.detail[0].error * screen_height() / 0.3 + radius * 2.).max(radius * 6.);
        for (label, distance) in [("near", radius * 3.), ("far", far)] {
            let camera = Camera3D {
                position: center + vec3(0., -distance, distance * 0.25),
                target: center,
                up: Vec3::Z,
                fovy: 75_f32.to_radians(),
                z_near: 0.1,
                z_far: distance * 4. + 100.,
                ..Default::default()
            };
            let mut pixels = Vec::new();
            let mut savings = 0;
            for enabled in [false, true] {
                clear_background(Color::from_hex(0x303038));
                set_camera(&camera);
                crate::render_fx::begin_view(&camera, 0., &Atmosphere::default(), true);
                for m in &self.bound_materials {
                    Atmosphere::default().apply(m, camera.position);
                    m.set_uniform("Fullbright", 1_f32);
                    m.set_uniform("SkyFade", 0_f32);
                }
                let mut context = DrawContext {
                    enabled,
                    required_pvs: false,
                    faces: None,
                    cluster: None,
                    visibility: &self.map.visibility,
                    stats: DrawStats::default(),
                };
                draw_batch(
                    &mut self.batches[index],
                    &self.materials,
                    camera.position,
                    0.,
                    None,
                    Vec3::ZERO,
                    false,
                    &mut context,
                );
                crate::render_fx::finish();
                let image = get_screen_data();
                image.export_png(&format!(
                    "private/sky-performance/detail-{label}-{enabled}.png"
                ));
                pixels.push(image.bytes);
                savings = context.stats.detail_saved;
                set_default_camera();
                next_frame().await;
            }
            let changed = pixels[0]
                .chunks_exact(4)
                .zip(pixels[1].chunks_exact(4))
                .filter(|(a, b)| a[..3] != b[..3])
                .count();
            if label == "near" {
                ensure!(savings == 0 && changed == 0, "Near prop detail changed");
            } else {
                ensure!(
                    savings > 0 && changed < 8160,
                    "Distant prop detail failed: savings {savings}, changed {changed}"
                );
            }
            println!("PASS original prop {label}: {savings} triangles saved, {changed} changed pixels, distance {distance:.2}");
        }
        Ok(())
    }
    pub fn set_sky_origin(&mut self, origin: Vec3) {
        self.sky_origin = Some(origin);
    }
    pub async fn fortress_check(assets: &mut Assets) -> Result<()> {
        camera_portal::check(assets).await?;
        let mut scene = Self::load(assets, "fortress1")?;
        check_layers(&scene).await?;
        let mut interactions = crate::interaction::Interactions::load(&scene.map)?;
        interactions.set_entry(assets, &scene.map, "fortress1", None)?;
        let mut views = vec![
            (
                "sky-room",
                vec3(5240., -3872., -1296.),
                vec3(5240., -3300., -1336.),
            ),
            (
                "sky-outside",
                vec3(-3264., 2136., 48.),
                vec3(-2800., 2000., 400.),
            ),
            (
                "sky-school",
                vec3(-3060., 2700., 450.),
                vec3(-4060., 3088., 850.),
            ),
            (
                "sky-return",
                vec3(-3840., 3788., 510.),
                vec3(-3872., 3598., 570.),
            ),
            ("room", vec3(80., -2660., -144.), vec3(-250., -2050., 30.)),
            (
                "room-sky",
                vec3(80., -2660., -144.),
                vec3(80., -2100., 750.),
            ),
        ];
        let (eye, yaw) = crate::interaction::spawn(&scene.map, None);
        views.push(("entry", eye, eye + vec3(yaw.cos(), yaw.sin(), 0.)));
        let f = interactions.fortress.as_mut().unwrap();
        f.state.cinema.beat = Some(crate::fortress::cinema::Beat::Arrival);
        for (label, time) in [
            ("arrival", 4.),
            ("arrival-cave", 23.),
            ("arrival-exterior", 38.),
            ("arrival-landing", 83.2),
        ] {
            f.state.cinema.time = time;
            let c = f.cinema.camera(&f.state.cinema).unwrap();
            views.push((label, c.eye, c.target));
        }
        for (name, eye, target) in views {
            let mut reference = Vec::new();
            for optimized in [false, true] {
                scene.optimize = optimized;
                for frame in 0..3 {
                    clear_background(BLACK);
                    let camera = Camera3D {
                        position: eye,
                        target,
                        up: Vec3::Z,
                        fovy: 75_f32.to_radians(),
                        z_near: 2.,
                        z_far: 30000.,
                        ..Default::default()
                    };
                    set_camera(&camera);
                    crate::lighting::select(vec![], eye, &scene.world);
                    crate::render_fx::begin_view(&camera, 1., &scene.atmosphere, false);
                    let poses = interactions.transforms();
                    scene.draw(eye, 1., false, false, &poses);
                    depth_read_only(|| scene.draw(eye, 1., false, true, &poses));
                    crate::render_fx::finish();
                    set_default_camera();
                    if frame == 2 {
                        crate::viewer::save_capture(std::path::Path::new(&format!(
                            "private/fortress-{name}-{optimized}.png"
                        )))?;
                        let pixels = get_screen_data().bytes;
                        if matches!(name, "arrival" | "entry") {
                            let image = get_screen_data();
                            let p = image.get_pixel(20, image.height as u32 / 2);
                            anyhow::ensure!(
                                p.b > 0.03 && p.g > 0.03 && p.b > p.r,
                                "Missing Fortress sky in {name}: {p:?}"
                            );
                        }
                        if !optimized {
                            reference = pixels;
                        } else {
                            let changed = reference
                                .iter()
                                .zip(&pixels)
                                .filter(|(a, b)| a != b)
                                .count();
                            println!("Fortress {name} optimized difference: {changed} channels");
                            anyhow::ensure!(
                                reference
                                    .iter()
                                    .zip(&pixels)
                                    .all(|(&a, &b)| a.abs_diff(b) <= 1),
                                "Fortress visibility regression in {name}"
                            );
                        }
                    }
                    next_frame().await;
                }
            }
        }
        Ok(())
    }
    fn bind_atlas(&self) {
        // Extra sampler bindings are read when Macroquad flushes, unlike copied
        // per-draw uniforms. Finish an older scene before changing shared samplers.
        unsafe {
            {
                #[cfg(target_os = "vita")]
                crate::frame_profile::flush_boundary("flush_atlas");
                #[cfg(target_os = "vita")]
                let _flush_profile = crate::frame_profile::span("gl_flush");
                #[cfg(target_os = "vita")]
                let _flush_site_profile = crate::frame_profile::span("flush_atlas");

                get_internal_gl().flush();
            }
        }
        for material in &self.bound_materials {
            material.set_texture("LightAtlas", self.atlas.clone());
            material.set_uniform("AtlasGrid", self.atlas_grid);
            material.set_texture(
                "SkyBackdrop",
                self.sky_backdrop.as_ref().unwrap_or(&self.atlas).clone(),
            );
            material.set_uniform(
                "SkySize",
                self.sky_backdrop
                    .as_ref()
                    .map_or(Vec2::ONE, |t| vec2(t.width(), t.height())),
            );
            material.set_uniform(
                "SkyFade",
                if self.sky_backdrop.is_some() {
                    1_f32
                } else {
                    0_f32
                },
            );
        }
    }
    pub fn draw_with_particles(
        &mut self,
        camera: Vec3,
        direction: Vec3,
        time: f32,
        fullbright: bool,
        transforms: &[(usize, Vec3, Quat)],
        particles: &crate::particles::Steam,
    ) {
        #[cfg(target_os = "vita")]
        if crate::frame_profile::mode().no_world() {
            return;
        }
        #[cfg(feature = "vita_renderer")]
        {
            if self.retained.is_some() {
                self.draw(camera, time, fullbright, true, transforms);
                if !vita_renderer::world_only() {
                    let _producer = crate::light_profile::fx_timer(crate::light_profile::FxMetric::Producer);
                    let _render_timer = crate::light_profile::render_timer(crate::light_profile::RenderPhase::Fx);
                    particles.prepare_draw(camera, &self.atmosphere);
                    particles.prepare_quads(camera, Some(direction));
                    for (i, _) in particles.depths(camera, direction) { particles.draw_one(i, camera, direction); }
                }
                return;
            }
            if vita_renderer::world_only() {
                self.draw(camera, time, fullbright, true, transforms);
                return;
            }
        }
        #[cfg(target_os = "vita")]
        let _profile = crate::frame_profile::span("world_draw");
        self.bind_atlas();
        let cluster = self.map.visibility.cluster(camera, &self.map.planes);
        let mut context = DrawContext {
            enabled: self.optimize,
            required_pvs: self.sky_backdrop.is_some(),
            faces: self.face_mask_cache.faces(&self.map.visibility, cluster),
            cluster,
            visibility: &self.map.visibility,
            stats: DrawStats::default(),
        };
        for m in &self.bound_materials {
            self.atmosphere.apply(m, camera);
            m.set_uniform("Fullbright", if fullbright { 1_f32 } else { 0. });
        }
        particles.prepare_draw(camera, &self.atmosphere);
        let mut order = Vec::new();
        for (i, b) in self
            .batches
            .iter()
            .enumerate()
            .filter(|(_, b)| b.transparent())
        {
            let transform = b.model.and_then(|i| transforms.iter().find(|t| t.0 == i));
            if b.model.is_some() && transform.is_none() {
                continue;
            }
            let center = transform.map_or(b.center, |(_, p, r)| *p + *r * b.center);
            order.push(((center - camera).dot(direction), false, i));
        }
        order.extend(
            particles
                .depths(camera, direction)
                .map(|(i, d)| (d, true, i)),
        );
        if !crate::render_fx::active() {
            order.sort_by(|a, b| b.0.total_cmp(&a.0));
        }
        for (_, particle, i) in order {
            if particle {
                particles.draw_one(i, camera, direction);
            } else {
                let b = &mut self.batches[i];
                let transform = b.model.and_then(|i| transforms.iter().find(|t| t.0 == i));
                draw_batch(
                    b,
                    &self.materials,
                    camera,
                    time,
                    transform,
                    Vec3::ZERO,
                    false,
                    &mut context,
                );
            }
        }
        gl_use_default_material();
        self.stats.batches += context.stats.batches;
        self.stats.culled += context.stats.culled;
        self.stats.triangles += context.stats.triangles;
        self.stats.detail_saved += context.stats.detail_saved;
    }
    pub fn load(assets: &mut Assets, name: &str) -> Result<Self> {
        Self::load_for(assets, name, Default::default())
    }
    pub fn load_for(
        assets: &mut Assets,
        name: &str,
        difficulty: crate::powerups::Difficulty,
    ) -> Result<Self> {
        let mut timing = crate::startup::Phase::new("map_load");
        eprintln!("[lg/startup] map_load map={name}");
        let bytes = assets.read(&format!("maps/{name}.bsp"))?;
        timing.mark("bsp_read");
        let mut map = Bsp::parse(&bytes)
            .with_context(|| format!("Loading {name}"))?;
        drop(bytes);
        timing.mark("bsp_parse");
        map.difficulty = difficulty;
        let world = World::from_bsp(&map)?;
        timing.mark("collision_world");
        Self::from_parts(assets, name, map, world)
    }
    /// The window half of loading a scene, for a map and collision that `campaign::load_visit`
    /// already built (with the difficulty applied) so that the viewer and a headless route share
    /// one copy of them.
    pub fn from_parts(assets: &mut Assets, name: &str, map: Bsp, world: World) -> Result<Self> {
        #[cfg(feature="vita_renderer")]
        crate::character::skinning::overlap::reset_publications();
        let mut timing = crate::startup::Phase::new("scene_build");
        crate::lighting::load_art(assets)?;
        let specs = texture::read_materials(assets)?;
        let atmosphere = Atmosphere::load(assets, name, &map, &specs)?;
        timing.mark("lighting_material_specs_atmosphere");
        let count = map.lightmaps.len() / 49152;
        let cols = 8usize;
        let rows = count.div_ceil(cols).max(1);
        anyhow::ensure!(rows * 128 <= 8192, "Lightmap atlas too large");
        let mut atlas = vec![255u8; cols * 128 * rows * 128 * 4];
        for i in 0..count {
            for y in 0..128 {
                for x in 0..128 {
                    let src = i * 49152 + (y * 128 + x) * 3;
                    let dst = ((i / cols * 128 + y) * cols * 128 + i % cols * 128 + x) * 4;
                    atlas[dst..dst + 3].copy_from_slice(&map.lightmaps[src..src + 3]);
                }
            }
        }
        let atlas = crate::texture::upload_rgba8((cols * 128) as u16, (rows * 128) as u16, &atlas);
        atlas.set_filter(FilterMode::Linear);
        let fallback = crate::texture::upload_rgba8(
            2,
            2,
            &[
                120, 80, 135, 255, 65, 48, 80, 255, 65, 48, 80, 255, 120, 80, 135, 255,
            ],
        );
        let white = crate::texture::upload_rgba8(1, 1, &[255; 4]);
        timing.mark("lightmap_textures");
        let mut models = vec![0usize];
        for e in &map.entities {
            if e.get("classname")
                .is_some_and(|c| c == "func_rotatingdoor" || c == "func_door" || c == "func_rope")
                || (name == "skool1"
                    && (crate::interaction::school_platform(e).is_some()
                        || crate::school::supported_inline(e)
                        || crate::school::theatre_picture(e)))
                || (name == "skool2" && (crate::gym::supported(e) || crate::school2::supported(e)))
                || (name == "gvillage" && crate::village::supported(e))
                || (name == "fortress1" && crate::fortress::supported(e))
                || (name == "fortress2" && crate::beyond::supported(e))
                || (name == "potears1" && crate::pool::supported(e))
                || (name == "pandemonium" && crate::pandemonium::supported(e))
                || (name == "potears3" && crate::duchess::supported(e))
                || crate::levels::owns_submodel(name, e)
            {
                if let Some(i) = e
                    .get("model")
                    .and_then(|s| s.strip_prefix('*'))
                    .and_then(|s| s.parse::<usize>().ok())
                    .filter(|&i| i < map.models.len())
                {
                    models.push(i);
                }
            }
        }
        models.sort();
        models.dedup();
        let mut cache = BTreeMap::<String, Texture2D>::new();
        let mut missing = Vec::new();
        let mut batches = Vec::<Batch>::new();
        #[cfg(feature = "vita_renderer")]
        let retained_world = name == "gvillage" && vita_renderer::config().renderer == "vita";
        #[cfg(not(feature = "vita_renderer"))]
        let retained_world = false;
        #[cfg(feature = "vita_renderer")]
        println!("[lg/vita-renderer] selected map={name} renderer={} reason={}",
            if retained_world { "vita" } else { "legacy" },
            if name != "gvillage" { "unsupported-map" } else { "configuration" });
        let camera_portals = if retained_world { Vec::new() } else { camera_portal::load(assets, name, &map, &world, &specs)? };
        let mut groups = BTreeMap::<(usize, usize), usize>::new();
        let sky_origin = crate::sky::origin(assets, name, &map)?;
        let mut sky_motions = crate::sky_sequence::motions(assets, &map, name)?;
        timing.mark("inline_models_portals_sky");
        for model in models {
            if world.traversal.ropes.iter().any(|r| r.model == model) {
                continue;
            }
            for si in map.models[model].surfaces.clone() {
                if camera_portals.iter().any(|p| p.surface == si) {
                    continue;
                }
                let surface = &map.surfaces[si];
                let shader = &map.shaders[surface.shader];
                let spec = specs.get(&shader.name.to_ascii_lowercase());
                if ![1, 2, 3].contains(&surface.kind)
                    || shader.flags & 0x80 != 0
                    || spec.is_some_and(|s| s.hidden)
                {
                    continue;
                }
                let (vertices, indices) = map.triangulate(surface);
                if indices.is_empty() {
                    continue;
                }
                let min = vertices
                    .iter()
                    .fold(Vec3::splat(f32::INFINITY), |p, v| p.min(v.position));
                let max = vertices
                    .iter()
                    .fold(Vec3::splat(f32::NEG_INFINITY), |p, v| p.max(v.position));
                let exterior = crate::sky::fortress_exterior(name, model, &shader.name, min, max);
                let portal = exterior || spec.is_some_and(|s| s.portal_sky);
                let sky = exterior || shader.flags & 4 != 0 || spec.is_some_and(|s| s.sky);
                let separate = sky
                    || spec.is_some_and(|s| {
                        s.transparent
                            || s.deforms
                                .iter()
                                .any(crate::materials::Deform::moves_vertices)
                            || s.stages.first().is_some_and(|s| s.blend != Blend::Opaque)
                    });
                let existing = groups
                    .get(&(model, surface.shader))
                    .copied()
                    .filter(|_| !separate)
                    .filter(|&i| {
                        batches[i].mesh.vertices.len() + vertices.len() < 60000
                            && batches[i].mesh.indices.len() + indices.len() < 180000
                    });
                let bi = if let Some(i) = existing {
                    i
                } else {
                    let mut layers = load_layers(
                        assets,
                        &shader.name,
                        spec,
                        &specs,
                        &mut cache,
                        &fallback,
                        &white,
                        &mut missing,
                    )?;
                    if sky && portal && sky_origin.is_some() {
                        layers = vec![Layer {
                            spec: Stage {
                                blend: Blend::Invisible,
                                ..Default::default()
                            },
                            textures: vec![white.clone()],
                            material: 7,
                            lit: false,
                        }];
                    }
                    let i = batches.len();
                    batches.push(Batch {
                        #[cfg(target_os = "vita")]
                        static_offset: None,
                        face_side: crate::materials::FaceSide::Both,
                        motion: None,
                        full_indices: Vec::new(),
                        parts: Vec::new(),
                        visibility_work: VisibilityWork::default(),
                        bounds: (Vec3::ZERO, Vec3::ZERO),
                        detail: Vec::new(),
                        animation: None,
                        mesh: Mesh {
                            vertices: Vec::new(),
                            indices: Vec::new(),
                            texture: None,
                        },
                        base: Vec::new(),
                        normals: Vec::new(),
                        deforms: spec.map(|s| s.deforms.clone()).unwrap_or_default(),
                        sky,
                        portal,
                        layers,
                        center: Vec3::ZERO,
                        model: if model == 0 { None } else { Some(model) },
                    });
                    // Per-face sky overrides must never absorb later opaque
                    // faces that happen to use the same black material.
                    if !separate {
                        groups.insert((model, surface.shader), i);
                    }
                    i
                };
                let batch = &mut batches[bi];
                if model == 0 {
                    batch.parts.push(Part {
                        face: si,
                        indices: batch.mesh.indices.len()..batch.mesh.indices.len() + indices.len(),
                        min,
                        max,
                    });
                }
                let offset = batch.mesh.vertices.len() as u16;
                batch.normals.extend(vertices.iter().map(|v| v.normal));
                batch.mesh.vertices.extend(vertices.iter().map(|v| Vertex {
                    position: v.position,
                    uv: v.uv,
                    color: v.color,
                    normal: vec4(
                        v.light_uv.x,
                        v.light_uv.y,
                        (surface.lightmap + 1).max(0) as f32,
                        0.,
                    ),
                }));
                batch
                    .mesh
                    .indices
                    .extend(indices.iter().map(|i| i + offset));
            }
        }
        // The brush supplies the climbing bounds, never the rope's visible width.
        let rope_shapes: Vec<_> = world
            .traversal
            .ropes
            .iter()
            .map(crate::rope::Strand::new)
            .collect();
        for (rope, shape) in world.traversal.ropes.iter().zip(&rope_shapes) {
            let m = &map.models[rope.model];
            let skin = m
                .surfaces
                .clone()
                .next()
                .map(|i| map.shaders[map.surfaces[i].shader].name.as_str())
                .unwrap_or("models/vine/skin01.tga");
            let image = texture::resolve(assets, skin, &specs)
                .or_else(|| texture::resolve(assets, "models/vine/skin01.tga", &specs))
                .and_then(|path| texture::decode(assets, &path).ok())
                .map(upload)
                .unwrap_or_else(|| white.clone());
            let vertices = shape.vertices(rope.origin, Quat::IDENTITY);
            let count = vertices.len();
            batches.push(Batch {
                #[cfg(target_os = "vita")]
                static_offset: None,
                face_side: crate::materials::FaceSide::Both,
                motion: None,
                full_indices: Vec::new(),
                parts: Vec::new(),
                visibility_work: VisibilityWork::default(),
                bounds: (Vec3::ZERO, Vec3::ZERO),
                detail: Vec::new(),
                animation: None,
                mesh: Mesh {
                    vertices,
                    indices: shape.indices(),
                    texture: None,
                },
                base: Vec::new(),
                normals: vec![Vec3::Z; count],
                deforms: Vec::new(),
                sky: false,
                portal: false,
                layers: vec![Layer {
                    spec: Stage::default(),
                    textures: vec![image],
                    material: 0,
                    lit: true,
                }],
                center: Vec3::ZERO,
                model: Some(rope.model),
            });
        }
        let mut sky_props = Vec::new();
        let decoration_start = batches.len();
        let decorations = crate::decorations::Catalog::load(assets, &map, name)?;
        timing.mark("world_meshes_textures_decoration_models");
        println!("Decorations {name}: {} placements, {} baked duplicates avoided, {} scripted props deferred", decorations.placements.len(), decorations.baked, decorations.deferred.len());
        for placement in &decorations.placements {
            let def = &placement.model.def;
            for surface in &placement.model.surfaces {
                let skin = def
                    .skins
                    .get(&surface.name)
                    .or_else(|| def.skins.get("all"))
                    .context("Environmental model surface skin missing")?
                    .to_ascii_lowercase();
                let name = if specs.contains_key(&skin) {
                    skin
                } else {
                    format!("{}/{skin}", def.path)
                };
                let spec = specs
                    .get(&name.to_lowercase())
                    .or_else(|| specs.get(name.rsplit_once('.').map_or(name.as_str(), |(s, _)| s)));
                let layers = load_layers(
                    assets,
                    &name,
                    spec,
                    &specs,
                    &mut cache,
                    &fallback,
                    &white,
                    &mut missing,
                )?;
                let vertices = surface.frames[0]
                    .iter()
                    .zip(&surface.uv)
                    .map(|(p, uv)| Vertex {
                        position: placement.point(*p),
                        uv: *uv,
                        color: [255; 4],
                        normal: Vec4::ZERO,
                    })
                    .collect::<Vec<_>>();
                let mut normals = vec![Vec3::ZERO; vertices.len()];
                for tri in surface.indices.chunks_exact(3) {
                    let [a, b, c] = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
                    let n = (vertices[b].position - vertices[a].position)
                        .cross(vertices[c].position - vertices[a].position);
                    for i in [a, b, c] {
                        normals[i] += n;
                    }
                }
                for n in &mut normals {
                    *n = n.normalize_or_zero();
                }
                let batch = Batch {
                    #[cfg(target_os = "vita")]
                    static_offset: None,
                    face_side: if placement.sky {
                        spec.map_or(crate::materials::FaceSide::Front, |s| s.face_side)
                    } else {
                        crate::materials::FaceSide::Both
                    },
                    motion: map.entities[placement.entity]
                        .get("targetname")
                        .and_then(|n| sky_motions.get(n))
                        .cloned(),
                    full_indices: surface.indices.clone(),
                    parts: Vec::new(),
                    visibility_work: VisibilityWork::default(),
                    bounds: surface
                        .frames
                        .iter()
                        .flatten()
                        .map(|p| placement.point(*p))
                        .fold(
                            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                            |(a, b), p| (a.min(p), b.max(p)),
                        ),
                    detail: surface
                        .detail
                        .iter()
                        .map(|l| crate::model_detail::Level {
                            indices: l.indices.clone(),
                            error: l.error * placement.scale,
                        })
                        .collect(),
                    animation: (surface.frames.len() > 1).then(|| {
                        crate::ambient_animation::Frames {
                            positions: surface.frames.clone(),
                            frame_time: placement.model.frame_time,
                            transform: placement.pose,
                            scale: placement.scale,
                        }
                    }),
                    center: placement.pose.translation,
                    base: vertices.clone(),
                    normals,
                    mesh: Mesh {
                        vertices,
                        indices: surface.indices.clone(),
                        texture: None,
                    },
                    layers,
                    deforms: spec.map(|s| s.deforms.clone()).unwrap_or_default(),
                    sky: false,
                    portal: false,
                    model: placement.parent,
                };
                if placement.sky && sky_origin.is_some() {
                    sky_props.push(batch);
                } else {
                    batches.push(batch);
                }
            }
        }
        for batch in &mut batches {
            batch.full_indices = batch.mesh.indices.clone();
            if batch.animation.is_none() {
                batch.bounds = batch.mesh.vertices.iter().map(|v| v.position).fold(
                    (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                    |(a, b), p| (a.min(p), b.max(p)),
                );
            }
            batch.base = batch.mesh.vertices.clone();
            batch.center =
                batch.base.iter().map(|v| v.position).sum::<Vec3>() / batch.base.len() as f32;
        }
        for batch in batches.iter_mut().chain(&mut sky_props) {
            batch.prepare_visibility();
        }
        missing.sort();
        missing.dedup();
        let triangles = batches.iter().map(|b| b.mesh.indices.len() / 3).sum();
        let animated = batches
            .iter()
            .filter(|b| {
                b.layers
                    .iter()
                    .any(|l| l.spec.images.len() > 1 || !l.spec.mods.is_empty())
            })
            .count();
        timing.mark("decoration_meshes_visibility");
        let mut materials = (0..BLEND_COUNT)
            .map(|i| {
                world_material(
                    if i < 9
                        || batches
                            .iter()
                            .chain(&sky_props)
                            .any(|b| b.layers.iter().any(|l| l.material % BLEND_COUNT == i))
                    {
                        i
                    } else {
                        0
                    },
                )
            })
            .collect::<Result<Vec<_>>>()?;
        for i in BLEND_COUNT..BLEND_COUNT * 2 {
            // Equal-depth variants only consume slots if an actual layer uses one.
            materials.push(
                if batches
                    .iter()
                    .chain(&sky_props)
                    .any(|b| b.layers.iter().any(|l| l.material == i))
                {
                    world_material(i)?
                } else {
                    materials[i % BLEND_COUNT].clone()
                },
            );
        }
        let mut bound_materials = Vec::new();
        timing.mark("world_shaders");
        for material in &materials {
            if !bound_materials.iter().any(|m| Rc::ptr_eq(m, material)) {
                bound_materials.push(material.clone());
            }
        }
        println!("Loaded {name}: {triangles} triangles, {} batches, {animated} animated batches, {} fog volumes, distance fog {}, {} fallback images",batches.len(),atmosphere.volumes.len(),atmosphere.distance.w,missing.len());
        let mut scene = Self {
            #[cfg(feature = "vita_renderer")]
            retained: None,
            world_material_keepalive: Default::default(),
            camera_portals,
            face_mask_cache: FaceMaskCache::default(),
            rope_shapes,
            optimize: true,
            stats: DrawStats::default(),
            map,
            world,
            atmosphere,
            batches,
            decoration_start,
            materials,
            bound_materials,
            atlas,
            atlas_grid: vec2(cols as f32, rows as f32),
            missing,
            triangles,
            sky_origin,
            sky_rotation: crate::sky::rotation(name),
            sky_motion: sky_motions.remove("skycamera01"),
            sky_props,
            sky_backdrop: (name == "fortress1")
                .then(|| crate::texture::upload_rgba8(1, 1, &[0, 0, 0, 255])),
        };
        #[cfg(feature = "vita_renderer")]
        if retained_world {
            let corpus=assets.fingerprint()?;
            scene.retained = Some(vita_renderer::Renderer::new(&scene, &corpus)?);
            // The retained path owns its immutable buffers and material recipes.
            // Release the old CPU geometry and unused Macroquad world materials.
            scene.batches.clear();
            scene.sky_props.clear();
            scene.bound_materials.clear();
            scene.materials.clear();
        }
        timing.mark("retained_renderer");
        Ok(scene)
    }
    pub fn update_ropes(
        &mut self,
        dt: f32,
        hand: Option<(usize, Vec3)>,
        transforms: &[(usize, Vec3, Quat)],
    ) {
        #[cfg(feature = "vita_renderer")]
        if self.retained.is_some() { return; }
        for shape in &mut self.rope_shapes {
            shape.update(
                dt,
                hand.filter(|h| h.0 == shape.model).map(|h| h.1),
                &self.world,
            );
            let Some((_, origin, rotation)) = transforms.iter().find(|t| t.0 == shape.model) else {
                continue;
            };
            if let Some(batch) = self
                .batches
                .iter_mut()
                .find(|b| b.model == Some(shape.model))
            {
                batch.base = shape.vertices(*origin, *rotation);
                batch.bounds = batch.base.iter().fold(
                    (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                    |(lo, hi), v| (lo.min(v.position), hi.max(v.position)),
                );
                batch.visibility_work.invalidate();
                batch.center = (batch.bounds.0 + batch.bounds.1) * 0.5;
            }
        }
    }
    /// The retained scheduler owns all actor/world alpha; other scenes keep the legacy queue.
    pub(crate) fn begin_fx_view(
        &mut self,
        camera: &Camera3D,
        time: f32,
        fullbright: bool,
        flush: bool,
    ) {
        #[cfg(target_os="vita")]
        activate_world_materials(&self.world_material_keepalive);
        #[cfg(not(target_os="vita"))]
        if std::env::var_os("LOOKING_GLASS_REPRO_M2_LEAK").is_none() { activate_world_materials(&self.world_material_keepalive); }
        #[cfg(not(feature = "vita_renderer"))]
        let _ = flush;
        #[cfg(feature = "vita_renderer")]
        if vita_renderer::config().alpha_interop
            && self.retained.is_some()
            && !vita_renderer::world_only()
        {
            self.retained.as_mut().unwrap().begin_fx();
            crate::render_fx::retained::begin_view(
                camera,
                time,
                &self.atmosphere,
                fullbright,
                flush,
            );
            return;
        }
        crate::render_fx::begin_view(camera, time, &self.atmosphere, fullbright);
    }
    pub(crate) fn finish_fx(&mut self, flush: bool) {
        #[cfg(not(feature = "vita_renderer"))]
        let _ = flush;
        #[cfg(feature = "vita_renderer")]
        if crate::render_fx::retained_active() {
            if let Some(renderer) = self.retained.as_mut() {
                renderer.finish_fx(flush);
                return;
            }
        }
        crate::render_fx::finish();
    }
    pub fn draw(
        &mut self,
        camera: Vec3,
        time: f32,
        fullbright: bool,
        transparent: bool,
        transforms: &[(usize, Vec3, Quat)],
    ) {
        #[cfg(target_os = "vita")]
        if crate::frame_profile::mode().no_world() {
            self.stats = DrawStats::default();
            return;
        }
        let _profile = crate::frame_profile::span("world_draw");
        #[cfg(feature = "vita_renderer")]
        if let Some(mut renderer) = self.retained.take() {
            let stats = renderer.draw(self, camera, time, fullbright, transparent, transforms);
            self.retained = Some(renderer);
            if !transparent { self.stats = DrawStats::default(); }
            self.stats.batches += stats.batches;
            self.stats.culled += stats.culled;
            self.stats.triangles += stats.triangles;
            return;
        }
        self.bind_atlas();
        if !transparent {
            self.stats = DrawStats::default();
        }
        let cluster = self.map.visibility.cluster(camera, &self.map.planes);
        let mut context = DrawContext {
            enabled: self.optimize,
            // Fortress's expanded sky shell no longer occludes disconnected
            // rooms. Their authored PVS is required even in the uncapped
            // diagnostic baseline; only optional frustum/LOD work is toggled.
            required_pvs: self.sky_backdrop.is_some(),
            faces: self.face_mask_cache.faces(&self.map.visibility, cluster),
            cluster,
            visibility: &self.map.visibility,
            stats: DrawStats::default(),
        };
        if !transparent {
            if let Some(origin) = self.sky_origin {
                // A portal must stay in its own visibility set, even in the
                // profiling baseline: otherwise unrelated offstage rooms leak
                // through the sky. The world pass remains independently toggled.
                context.enabled = true;
                let cluster = self.map.visibility.cluster(origin, &self.map.planes);
                context.cluster = cluster;
                context.faces = self.face_mask_cache.faces(&self.map.visibility, cluster);
                // Portal scene uses the view rotation but its authored fixed origin.
                // Translating geometry is equivalent and preserves the caller's projection.
                let offset = camera - origin;
                let r = (self.sky_rotation
                    * self
                        .sky_motion
                        .as_ref()
                        .map_or(Quat::IDENTITY, |m| m.transform(time).1))
                .inverse();
                let sky_transform = (0, origin - r * origin, r);
                let plain = Atmosphere::default();
                for m in &self.bound_materials {
                    plain.apply(m, camera);
                    m.set_uniform("Fullbright", 1_f32);
                    m.set_uniform("SkyFade", 0_f32);
                }
                for b in self
                    .batches
                    .iter_mut()
                    .filter(|b| !b.sky && b.model.is_none() && !b.transparent())
                {
                    draw_batch(
                        b,
                        &self.materials,
                        camera,
                        time,
                        Some(&sky_transform),
                        offset,
                        true,
                        &mut context,
                    );
                }
                for b in self.sky_props.iter_mut().filter(|b| !b.transparent()) {
                    draw_batch(
                        b,
                        &self.materials,
                        camera,
                        time,
                        Some(&sky_transform),
                        offset,
                        true,
                        &mut context,
                    );
                }
                depth_read_only(|| {
                    for b in self
                        .batches
                        .iter_mut()
                        .filter(|b| !b.sky && b.model.is_none() && b.transparent())
                    {
                        draw_batch(
                            b,
                            &self.materials,
                            camera,
                            time,
                            Some(&sky_transform),
                            offset,
                            true,
                            &mut context,
                        );
                    }
                    for b in self.sky_props.iter_mut().filter(|b| b.transparent()) {
                        draw_batch(
                            b,
                            &self.materials,
                            camera,
                            time,
                            Some(&sky_transform),
                            offset,
                            true,
                            &mut context,
                        );
                    }
                });
                clear_view_depth();
                if let Some(texture) = &mut self.sky_backdrop {
                    let (width, height) = macroquad::miniquad::window::screen_size();
                    if texture.width() != width || texture.height() != height {
                        *texture = Texture2D::from_image(&Image::gen_image_color(
                            width as u16,
                            height as u16,
                            BLACK,
                        ));
                    }
                    // GPU-to-GPU copy of the flushed sky colour. No frame readback.
                    texture.grab_screen();
                    self.bind_atlas();
                }
            }
        }
        for m in &self.bound_materials {
            self.atmosphere.apply(m, camera);
            m.set_uniform("Fullbright", if fullbright { 1_f32 } else { 0. });
        }
        context.cluster = cluster;
        context.enabled = self.optimize;
        context.faces = self.face_mask_cache.faces(&self.map.visibility, cluster);
        let transform = |b: &Batch| b.model.and_then(|i| transforms.iter().find(|t| t.0 == i));
        let center = |b: &Batch| transform(b).map_or(b.center, |(_, p, r)| *p + *r * b.center);
        let mut order = (0..self.batches.len())
            .filter(|&i| self.batches[i].transparent() == transparent)
            .collect::<Vec<_>>();
        if transparent {
            order.sort_by(|&a, &b| {
                center(&self.batches[b])
                    .distance_squared(camera)
                    .total_cmp(&center(&self.batches[a]).distance_squared(camera))
            });
        }
        for i in order {
            let batch = &mut self.batches[i];
            let t = transform(batch);
            if batch.model.is_some() && t.is_none() {
                continue;
            }
            draw_batch(
                batch,
                &self.materials,
                camera,
                time,
                t,
                Vec3::ZERO,
                false,
                &mut context,
            );
        }
        if !transparent {
            for portal in &mut self.camera_portals {
                portal.draw(camera, transforms);
            }
        }
        gl_use_default_material();
        self.stats.batches += context.stats.batches;
        self.stats.culled += context.stats.culled;
        self.stats.triangles += context.stats.triangles;
        self.stats.detail_saved += context.stats.detail_saved;
    }
}

/// Miniquad 0.4.8's GL backend conflates `depth_write` with GL_DEPTH_TEST and
/// never changes glDepthMask. Keep pipeline depth_write=true for all 3-D draws,
/// then bracket a flushed transparent pass with the actual write mask disabled.
/// This is specific to our OpenGL renderer; do not replace with a Metal backend
/// without supplying an equivalent read-only depth attachment.
pub fn depth_read_only(draw: impl FnOnce()) {
    #[cfg(feature = "vita_renderer")]
    if crate::render_fx::retained_active() {
        crate::render_fx::retained_depth_scope(draw);
        return;
    }
    unsafe {
        {
            #[cfg(target_os = "vita")]
            crate::frame_profile::flush_boundary("flush_depth_begin");
            #[cfg(target_os = "vita")]
            let _flush_profile = crate::frame_profile::span("gl_flush");
            #[cfg(target_os = "vita")]
            let _flush_site_profile = crate::frame_profile::span("flush_depth_begin");

            get_internal_gl().flush();
        }
        macroquad::miniquad::gl::glDepthMask(0);
    }
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            unsafe {
                macroquad::miniquad::gl::glDepthMask(1);
            }
        }
    }
    let _restore = Restore;
    draw();
    unsafe {
        {
            #[cfg(target_os = "vita")]
            crate::frame_profile::flush_boundary("flush_depth_end");
            #[cfg(target_os = "vita")]
            let _flush_profile = crate::frame_profile::span("gl_flush");
            #[cfg(target_os = "vita")]
            let _flush_site_profile = crate::frame_profile::span("flush_depth_end");

            get_internal_gl().flush();
        }
    }
}

pub fn depth_pipeline(color_blend: Option<BlendState>) -> PipelineParams {
    PipelineParams {
        depth_test: Comparison::LessOrEqual,
        depth_write: true,
        color_blend,
        ..Default::default()
    }
}

/// Direct 3-D has already submitted its packets. Bind the intended target even
/// after a readback, clear depth without colour, and leave the Macroquad queue alone.
pub(crate) fn clear_direct_view_depth() {
    unsafe {
        macroquad::miniquad::gl::glDepthMask(1);
        #[cfg(feature="vita_renderer")]
        vita_renderer::begin_3d_pass(macroquad::miniquad::PassAction::Clear {color:None,depth:Some(1.),stencil:None});
        #[cfg(not(feature="vita_renderer"))]
        get_internal_gl().quad_context.begin_default_pass(macroquad::miniquad::PassAction::Clear {color:None,depth:Some(1.),stencil:None});
        get_internal_gl().quad_context.end_render_pass();
    }
}

/// Run only after every world draw/effect has flushed. Keep colour, give the local
/// first-person toy its own depth layer, then draw only the 2-D HUD afterwards.
pub fn clear_view_depth() {
    unsafe {
        {
            #[cfg(target_os = "vita")]
            crate::frame_profile::flush_boundary("flush_clear_depth");
            #[cfg(target_os = "vita")]
            let _flush_profile = crate::frame_profile::span("gl_flush");
            #[cfg(target_os = "vita")]
            let _flush_site_profile = crate::frame_profile::span("flush_clear_depth");

            get_internal_gl().flush();
        }
        macroquad::miniquad::gl::glDepthMask(1);
        #[cfg(feature="vita_renderer")]
        { vita_renderer::begin_3d_pass(macroquad::miniquad::PassAction::Clear{color:None,depth:Some(1.),stencil:None});get_internal_gl().quad_context.end_render_pass(); }
        #[cfg(not(feature="vita_renderer"))]
        macroquad::miniquad::gl::glClear(macroquad::miniquad::gl::GL_DEPTH_BUFFER_BIT);
    }
}

/// Use the actual world layer shader, not a substitute, for cutout/sky/fog regressions.
pub async fn check_layers(scene: &Scene) -> Result<()> {
    use anyhow::ensure;
    scene.bind_atlas();
    for m in &scene.materials {
        m.set_uniform("SkyFade", 0_f32);
    }
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
    let draw = |rgba: [u8; 4], test: u8, blend_mode: Blend, z: f32, sky: bool| {
        let vertices = [
            vec3(-1., -1., z),
            vec3(1., -1., z),
            vec3(1., 1., z),
            vec3(-1., 1., z),
        ]
        .into_iter()
        .map(|position| Vertex {
            position,
            uv: Vec2::splat(0.5),
            normal: Vec4::ZERO,
            color: [255; 4],
        })
        .collect::<Vec<_>>();
        let mut b = Batch {
            #[cfg(target_os = "vita")]
            static_offset: None,
            face_side: crate::materials::FaceSide::Both,
            motion: None,
            full_indices: vec![0, 1, 2, 0, 2, 3],
            parts: vec![],
            visibility_work: VisibilityWork::default(),
            bounds: (Vec3::ZERO, Vec3::ZERO),
            detail: vec![],
            animation: None,
            base: vertices.clone(),
            normals: vec![Vec3::Z; 4],
            deforms: Vec::new(),
            sky,
            portal: sky,
            center: Vec3::ZERO,
            model: None,
            mesh: Mesh {
                vertices,
                indices: vec![0, 1, 2, 0, 2, 3],
                texture: None,
            },
            layers: vec![Layer {
                textures: vec![crate::texture::upload_rgba8(1, 1, &rgba)],
                spec: Stage {
                    blend: blend_mode,
                    alpha_test: test,
                    ..Default::default()
                },
                lit: false,
                material: blend_index(blend_mode),
            }],
        };
        draw_batch(
            &mut b,
            &scene.materials,
            camera.position,
            0.,
            None,
            Vec3::ZERO,
            false,
            &mut DrawContext {
                enabled: false,
                required_pvs: false,
                faces: None,
                cluster: None,
                visibility: &scene.map.visibility,
                stats: DrawStats::default(),
            },
        );
    };
    let center = || {
        let f = get_screen_data();
        f.get_pixel(f.width as u32 / 2, f.height as u32 / 2)
    };
    for (cutout, alpha, visible) in [
        (1, 0, false),
        (1, 1, true),
        (2, 127, false),
        (2, 128, true),
        (3, 127, true),
        (3, 128, false),
    ] {
        clear_background(Color::new(0., 0., 1., 1.));
        set_camera(&camera);
        for m in &scene.materials {
            Atmosphere::default().apply(m, camera.position);
            m.set_uniform("Fullbright", 1_f32);
        }
        draw([255, 0, 0, alpha], cutout, Blend::Opaque, 0., false);
        let c = center();
        ensure!(
            if visible {
                c.r > 0.98 && c.b < 0.02
            } else {
                c.b > 0.98 && c.r < 0.02
            },
            "World alpha test {cutout}/{alpha}: {c:?}"
        );
        gl_use_default_material();
        set_default_camera();
        next_frame().await;
    }
    println!("PASS actual world shader GT0/GE128/LT128 cutout boundaries");
    // This exact two-stage operation is used by Pandemonium's green slime.
    for background in [Vec3::ZERO, vec3(0.2, 0.4, 0.6)] {
        for alpha in [0, 64, 255] {
            clear_background(Color::new(background.x, background.y, background.z, 1.));
            set_camera(&camera);
            for m in &scene.materials {
                Atmosphere::default().apply(m, camera.position);
            }
            depth_read_only(|| {
                draw([64, 128, 32, alpha], 0, Blend::AlphaColor, 0., false);
                draw([16, 64, 32, 255], 0, Blend::ColorAdd, 0., false);
            });
            let expected = vec3(64., 128., 32.) / 255.
                * (background + Vec3::splat(alpha as f32 / 255.))
                * (Vec3::ONE + vec3(16., 64., 32.) / 255.);
            let actual = center();
            ensure!(
                (vec3(actual.r, actual.g, actual.b) - expected)
                    .abs()
                    .max_element()
                    < 0.018,
                "Slime blend differs from authored factors: {actual:?} vs {expected:?}"
            );
            gl_use_default_material();
            set_default_camera();
            next_frame().await;
        }
    }
    println!("PASS two-stage slime blend on black/coloured backgrounds at three alpha levels");
    for b in [
        Blend::Filter,
        Blend::DoubleFilter,
        Blend::Add,
        Blend::AlphaColor,
    ] {
        clear_background(Color::new(0.2, 0.4, 0.6, 1.));
        set_camera(&camera);
        for m in &scene.materials {
            Atmosphere {
                distance: vec4(0.8, 0.1, 0.2, 5.),
                ..Default::default()
            }
            .apply(m, camera.position);
        }
        depth_read_only(|| draw([40, 80, 120, 255], 0, b, 0., false));
        let c = center();
        ensure!(
            (c.r - 0.2).abs() < 0.015 && (c.g - 0.4).abs() < 0.015 && (c.b - 0.6).abs() < 0.015,
            "Fogged {b:?} was not blend-neutral: {c:?}"
        );
        gl_use_default_material();
        set_default_camera();
        next_frame().await;
    }
    println!("PASS additive/filter/double-filter layers fade to their neutral blend under fog");
    clear_background(Color::new(0., 0., 1., 1.));
    set_camera(&camera);
    for m in &scene.materials {
        Atmosphere::default().apply(m, camera.position);
    }
    draw([255; 4], 0, Blend::Invisible, 1., true);
    draw([255, 0, 0, 255], 0, Blend::Opaque, 0., false);
    let c = center();
    ensure!(
        c.b > 0.98 && c.r < 0.02,
        "Sky aperture did not occlude geometry beyond it: {c:?}"
    );
    draw([0, 255, 0, 255], 0, Blend::Opaque, 2., false);
    let c = center();
    ensure!(
        c.g > 0.98 && c.b < 0.02,
        "Sky aperture covered foreground geometry: {c:?}"
    );
    println!(
        "PASS sky aperture preserves sky colour, occludes far geometry and admits foreground walls"
    );
    // The ordinary fog regression above must stay unchanged in other maps.
    // Fortress alone substitutes the sky at the fully fogged end of the ramp.
    clear_background(BLACK);
    let sky = crate::texture::upload_rgba8(1, 1, &[32, 96, 160, 255]);
    for m in &scene.materials {
        m.set_texture("SkyBackdrop", sky.clone());
        m.set_uniform("SkySize", Vec2::ONE);
        m.set_uniform("SkyFade", 1_f32);
        m.set_uniform("FogAMin", vec3(-100., -100., -100.));
        m.set_uniform("FogAMax", vec3(100., 100., 100.));
        m.set_uniform("FogA", vec4(0., 0., 0., 1.));
    }
    draw([255, 0, 0, 255], 0, Blend::Opaque, 0., false);
    let c = center();
    ensure!(
        (vec3(c.r, c.g, c.b) - vec3(32., 96., 160.) / 255.)
            .abs()
            .max_element()
            < 0.02,
        "Fortress fog failed to match sky: {c:?}"
    );
    println!("PASS Fortress fog fade matches the captured sky colour");
    scene.bind_atlas();
    gl_use_default_material();
    set_default_camera();
    next_frame().await;
    Ok(())
}

/// GPU/readback regression for the shared world/effect depth configuration.
/// Staged before/after comparisons through the same scene used by normal play.
pub async fn check_decorations(assets: &mut Assets) -> Result<()> {
    for (name, entity) in [
        ("skool1", 93),
        ("skool2", 98),
        ("garden1", 159),
        ("hatter1", 661),
        ("fortress1", 142),
    ] {
        let mut scene = Scene::load(assets, name)?;
        let catalog = crate::decorations::Catalog::load(assets, &scene.map, name)?;
        let prop = catalog
            .placements
            .iter()
            .find(|p| p.entity == entity)
            .context("Missing staged decoration")?;
        let mut interactions = crate::interaction::Interactions::load(&scene.map)?;
        interactions.set_entry(assets, &scene.map, name, None)?;
        interactions.sync(&mut scene.world);
        let poses = interactions.transforms();
        let parent = prop
            .parent
            .map(|id| {
                poses
                    .iter()
                    .find(|p| p.0 == id)
                    .context("Missing decoration mover transform")
            })
            .transpose()?;
        let world_point = |p| {
            let p = prop.point(p);
            parent.map_or(p, |(_, o, r)| *o + *r * p)
        };
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for p in prop.model.surfaces.iter().flat_map(|s| &s.frames[0]) {
            let p = world_point(*p);
            lo = lo.min(p);
            hi = hi.max(p);
        }
        let target = (lo + hi) * 0.5;
        let distance = ((hi - lo).length() * 1.2).clamp(100., 800.);
        let eye = (0..16)
            .find_map(|i| {
                let a = i as f32 * std::f32::consts::TAU / 16.;
                let eye = target + vec3(a.cos(), a.sin(), 0.22) * distance;
                // Several missing furnishings already have authored clip brushes.
                // Inspect from outside them; their centre need not be walkable.
                let hit = scene.world.sweep(eye, target, Vec3::splat(2.));
                (!hit.start_solid && hit.fraction > 0.65).then_some(eye)
            })
            .with_context(|| format!("No clear decoration camera {name}/{entity}"))?;
        println!("Decoration camera {name}/{entity}: {eye:?} -> {target:?}");
        let mut props = scene.batches.split_off(scene.decoration_start);
        let mut sky_props = std::mem::take(&mut scene.sky_props);
        let mut before = Vec::new();
        for restored in [false, true] {
            if restored {
                scene.batches.append(&mut props);
                scene.sky_props.append(&mut sky_props);
            }
            clear_background(scene.atmosphere.background());
            set_camera(&Camera3D {
                position: eye,
                target,
                up: Vec3::Z,
                fovy: 65_f32.to_radians(),
                z_near: 2.,
                z_far: 30000.,
                ..Default::default()
            });
            scene.draw(eye, 1., false, false, &poses);
            depth_read_only(|| scene.draw(eye, 1., false, true, &poses));
            let pixels = get_screen_data().bytes;
            if restored {
                let changed = pixels
                    .chunks_exact(4)
                    .zip(before.chunks_exact(4))
                    .filter(|(a, b)| a != b)
                    .count();
                anyhow::ensure!(
                    changed > 1000,
                    "Decoration is not visible: {name}/{entity}, {changed} pixels"
                );
                println!(
                    "PASS visible restored decoration {name}/{entity}: {changed} changed pixels"
                );
            } else {
                before = pixels;
            }
            let suffix = if restored { "restored" } else { "without" };
            crate::viewer::save_capture(std::path::Path::new(&format!(
                "private/decorations-{name}-{suffix}.png"
            )))?;
            gl_use_default_material();
            set_default_camera();
            next_frame().await;
        }
    }
    println!(
        "PASS staged decorations through normal scene placement, layers and parent transforms"
    );
    Ok(())
}

pub async fn check_depth() -> Result<()> {
    use anyhow::ensure;
    let source = crate::environment::fragment("#version 100\nprecision mediump float;\nvarying lowp vec4 color;\n// FOG\nvoid main(){gl_FragColor=vec4(fogged(color.rgb,0.0),color.a);}");
    let mut materials = Vec::new();
    for i in 0..7 {
        materials.push(
            crate::frame_profile::load_material(
                ShaderSource::Glsl {
                    vertex: crate::character::VERTEX,
                    fragment: &source,
                },
                MaterialParams {
                    pipeline_params: depth_pipeline(blend(i)),
                    uniforms: crate::environment::uniforms(),
                    ..Default::default()
                },
            )
            .map_err(|e| anyhow::anyhow!("Depth test shader: {e:?}"))?,
        );
    }
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
    let quad = |z: f32, color: Color| Mesh {
        vertices: [
            vec3(-1., -1., z),
            vec3(1., -1., z),
            vec3(1., 1., z),
            vec3(-1., 1., z),
        ]
        .into_iter()
        .map(|position| Vertex {
            position,
            uv: Vec2::ZERO,
            color: color.into(),
            normal: Vec4::ZERO,
        })
        .collect(),
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: None,
    };
    let plain = Atmosphere::default();
    let fog = Atmosphere {
        distance: vec4(0.7, 0.3, 0.4, 15.),
        ..Default::default()
    };
    for i in 1..7 {
        clear_background(BLACK);
        set_camera(&camera);
        plain.apply(&materials[0], camera.position);
        gl_use_material(&materials[0]);
        draw_mesh(&quad(2., Color::new(0.2, 0.2, 0.2, 1.)));
        depth_read_only(|| {
            fog.apply(&materials[i], camera.position);
            gl_use_material(&materials[i]);
            draw_mesh(&quad(0., Color::new(1., 0.8, 0.1, 0.8)));
        });
        let frame = get_screen_data();
        let c = frame.get_pixel(frame.width as u32 / 2, frame.height as u32 / 2);
        ensure!(
            (c.r - 0.2).abs() < 0.015 && (c.g - 0.2).abs() < 0.015 && (c.b - 0.2).abs() < 0.015,
            "Blend {i}: fogged effect leaked through wall: {c:?}"
        );
        println!("PASS blend {i}: wall occludes fogged effect");
        gl_use_default_material();
        set_default_camera();
        next_frame().await;
    }
    clear_background(BLACK);
    set_camera(&camera);
    plain.apply(&materials[0], camera.position);
    gl_use_material(&materials[0]);
    draw_mesh(&quad(0., Color::new(0.2, 0.2, 0.2, 1.)));
    depth_read_only(|| {
        plain.apply(&materials[1], camera.position);
        gl_use_material(&materials[1]);
        draw_mesh(&quad(4., Color::new(0., 1., 0., 0.5)));
        draw_mesh(&quad(3., Color::new(1., 0., 0., 0.5)));
    });
    let frame = get_screen_data();
    let c = frame.get_pixel(frame.width as u32 / 2, frame.height as u32 / 2);
    ensure!(
        (c.r - 0.55).abs() < 0.02 && (c.g - 0.3).abs() < 0.02 && (c.b - 0.05).abs() < 0.02,
        "Transparent pass unexpectedly wrote depth: {c:?}"
    );
    println!("PASS transparent layers test depth without writing it; subsequent frames retain working opaque depth");
    clear_view_depth();
    // The local weapon may cover world colour, but still needs its own self-occlusion.
    let mut small = quad(-2., BLUE);
    for vertex in &mut small.vertices {
        vertex.position.x *= 0.25;
        vertex.position.y *= 0.25;
    }
    gl_use_material(&materials[0]);
    draw_mesh(&small);
    for vertex in &mut small.vertices {
        vertex.position.z = -3.;
        vertex.color = RED.into();
    }
    draw_mesh(&small);
    let frame = get_screen_data();
    let c = frame.get_pixel(frame.width as u32 / 2, frame.height as u32 / 2);
    let outside = frame.get_pixel(
        frame.width as u32 / 2,
        frame.height as u32 / 2 + frame.height as u32 / 8,
    );
    ensure!(
        (c.b - BLUE.b).abs() < 0.015 && (c.g - BLUE.g).abs() < 0.015 && c.r < 0.02,
        "Weapon depth layer did not clear or self-occlude: {c:?}"
    );
    ensure!(
        (outside.r - 0.55).abs() < 0.02 && (outside.g - 0.3).abs() < 0.02,
        "Weapon depth clear damaged world colour: {outside:?}"
    );
    println!("PASS first-person depth layer preserves world colour and weapon self-occlusion");
    gl_use_default_material();
    set_default_camera();
    next_frame().await;
    Ok(())
}

#[cfg(test)]
mod billboard_tests {
    use super::*;

    #[test]
    fn leading_lightmap_keeps_opaque_destination_before_clock_detail_and_glow() {
        let specs = crate::materials::parse("clock\n{\nsurfaceparm nolightmap\n{\nmap $lightmap\nrgbGen identity\n}\n{\nmap detail\nblendfunc GL_DST_COLOR GL_SRC_COLOR\n}\n{\nmap face\nblendfunc filter\n}\n{\nmap face\nblendfunc add\n}\n}\nstone\n{\n{\nmap $lightmap\n}\n{\nmap stone\nblendfunc filter\n}\n}").unwrap();
        let clock = world_stages(Some(&specs["clock"]));
        assert_eq!(
            clock.iter().map(|s| s.blend).collect::<Vec<_>>(),
            [
                Blend::Opaque,
                Blend::DoubleFilter,
                Blend::Filter,
                Blend::Add
            ]
        );
        assert_eq!(clock[0].images, ["$whiteimage"]);
        // Ordinary lightmapped walls retain their single-draw fast path.
        assert_eq!(world_stages(Some(&specs["stone"])).len(), 1);
    }

    #[test]
    fn flame_keeps_its_mount_height_width_and_uvs_while_camera_orbits() {
        let positions = [
            vec3(-6., 0., 14.),
            vec3(6., 0., 14.),
            vec3(6., 0., 46.),
            vec3(-6., 0., 46.),
        ];
        // Include a tilted authored axis, alternate vertex order, extreme UVs,
        // and a camera directly along the axis (degenerate facing direction).
        for rotation in [Quat::IDENTITY, Quat::from_rotation_x(0.7)] {
            for shift in 0..4 {
                for camera in [vec3(100., 100., 50.), vec3(-100., 2., 4.), Vec3::Z * 100.] {
                    let mut quad = (0..4)
                        .map(|i| Vertex {
                            position: rotation * positions[(i + shift) % 4],
                            uv: vec2(i as f32 + 17.3, -4.8),
                            color: [255; 4],
                            normal: Vec4::ZERO,
                        })
                        .collect::<Vec<_>>();
                    let uvs = quad.iter().map(|v| v.uv).collect::<Vec<_>>();
                    axial_billboard(&mut quad, rotation * camera);
                    let center = quad.iter().map(|v| v.position).sum::<Vec3>() * 0.25;
                    assert!(center.distance(rotation * Vec3::Z * 30.) < 0.001);
                    for (i, v) in quad.iter().enumerate() {
                        let local = rotation.conjugate() * v.position;
                        assert!(local.is_finite());
                        assert!((local.z - positions[(i + shift) % 4].z).abs() < 0.001);
                        assert!((local.truncate().length() - 6.).abs() < 0.001);
                        assert_eq!(v.uv, uvs[i]);
                    }
                }
            }
        }
    }
}
