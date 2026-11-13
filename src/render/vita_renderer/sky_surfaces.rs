//! The map's sky surfaces as one opaque, depth-writing draw before the world opaque pass.
//!
//! `sky_depth` draws them into depth only, over the gradient, so scenery placed behind the sky
//! stays hidden. `sky_cube` (`super::super::sky_cube`): the room faces of the capture replace the
//! gradient as a full-screen background, as the legacy path draws the sky room behind everything
//! (holes in the world show it too); the portal faces then go into depth only, and the sky shader
//! faces sample the layer faces of the capture by view direction. All of it runs where the
//! gradient ran, before any world group.
use super::*;

/// Positions only. `Rot` turns the eye-to-surface ray into the captured sky room's frame.
pub(super) const CUBE_VERTEX: &str = r#"#version 100
attribute vec3 position;
uniform mat4 Mvp; uniform mat4 Rot; uniform highp vec4 Eye; uniform highp vec4 Face;
varying highp vec3 dir;
void main(){dir=(Rot*vec4(position-Eye.xyz,0.0)).xyz;gl_Position=Mvp*vec4(position,1.0);}"#;
/// Faces sit in a 3 by 2 grid in the order +X -X +Y / -Y +Z -Z. On face (F, U), with right
/// R = F x U, a ray d lands at (d.R, d.U) / d.F in -1..1, the capture camera's NDC.
/// `Face.x` is half a texel of one face, so bilinear filtering never reads a neighbour face.
/// `Face.y` flips V for targets stored top row first (the Vita copy, as `scale.rs` upscales).
/// `Face.z` is the first face row of the set drawn, `Face.w` the target's rows of faces.
pub(super) const CUBE_FRAGMENT: &str = r#"#version 100
precision highp float;
varying highp vec3 dir;
uniform sampler2D Cube; uniform highp vec4 Face;
void main(){
 vec3 d=dir;vec3 a=abs(d);vec2 f;vec2 cell;
 if(a.x>=a.y&&a.x>=a.z){
  if(d.x>0.0){f=vec2(-d.y,d.z)/a.x;cell=vec2(0.0,0.0);}else{f=vec2(d.y,d.z)/a.x;cell=vec2(1.0,0.0);}
 }else if(a.y>=a.z){
  if(d.y>0.0){f=vec2(d.x,d.z)/a.y;cell=vec2(2.0,0.0);}else{f=vec2(-d.x,d.z)/a.y;cell=vec2(0.0,1.0);}
 }else{
  if(d.z>0.0){f=vec2(d.y,d.x)/a.z;cell=vec2(1.0,1.0);}else{f=vec2(-d.y,d.x)/a.z;cell=vec2(2.0,1.0);}
 }
 vec2 t=clamp(f*0.5+0.5,vec2(Face.x),vec2(1.0-Face.x));
 vec2 uv=(cell+t+vec2(0.0,Face.z))/vec2(3.0,Face.w);
 if(Face.y>0.5)uv.y=1.0-uv.y;
 gl_FragColor=vec4(texture2D(Cube,uv).rgb,1.0);
}"#;
/// The full-screen triangle of the gradient. `Mvp` is the inverse view-projection here: the
/// homogeneous point minus the eye scaled by its w is the ray, linear in screen space.
pub(super) const BACKGROUND_VERTEX: &str = r#"#version 100
attribute vec2 position;
uniform mat4 Mvp; uniform mat4 Rot; uniform highp vec4 Eye; uniform highp vec4 Face;
varying highp vec3 dir;
void main(){vec4 p=Mvp*vec4(position,1.0,1.0);dir=(Rot*vec4(p.xyz-Eye.xyz*p.w,0.0)).xyz;gl_Position=vec4(position,0.999,1.0);}"#;
pub(super) const DEPTH_VERTEX: &str = r#"#version 100
attribute vec3 position; uniform mat4 Mvp;
void main(){gl_Position=Mvp*vec4(position,1.0);}"#;
pub(super) const DEPTH_FRAGMENT: &str = r#"#version 100
precision lowp float;
void main(){gl_FragColor=vec4(0.0);}"#;

#[repr(C)]
struct CubeUniforms { mvp: Mat4, rot: Mat4, eye: [f32; 4], face: [f32; 4] }
#[repr(C)]
struct DepthUniforms { mvp: Mat4 }

/// Sky triangles in chunks that fit u16 indices. Input: one (positions, indices) per surface.
pub(super) fn chunks<'a>(surfaces: impl IntoIterator<Item = (&'a [Vec3], &'a [u16])>) -> Vec<(Vec<[f32; 3]>, Vec<u16>)> {
    let mut out: Vec<(Vec<[f32; 3]>, Vec<u16>)> = Vec::new();
    for (positions, indices) in surfaces {
        if indices.is_empty() || positions.len() > u16::MAX as usize { continue; }
        if out.last().is_none_or(|c| c.0.len() + positions.len() > u16::MAX as usize) {
            out.push((Vec::new(), Vec::new()));
        }
        let chunk = out.last_mut().unwrap();
        let base = chunk.0.len() as u16;
        chunk.0.extend(positions.iter().map(|p| p.to_array()));
        chunk.1.extend(indices.iter().map(|&i| base + i));
    }
    out
}

/// The captured target as the cube draw reads it.
pub(super) struct CubeView {
    pub texture: TextureId,
    pub rotation: Mat4,
    pub inset: f32,
    pub flip: bool,
    pub rows: u32,
    pub room_row: Option<u32>,
    pub layers_row: Option<u32>,
}
pub(super) struct SkySurfaces {
    /// Portal faces (every layer invisible: they show the sky room), then the sky shader faces.
    chunks: [Vec<(BufferId, BufferId, i32)>; 2],
    depth: (ShaderId, Pipeline),
    cube: Option<(ShaderId, Pipeline)>,
    background: Option<(ShaderId, Pipeline)>,
    pub(super) triangles: usize,
}
impl SkySurfaces {
    /// The static sky surfaces of the scene. `None` when the map has none or neither key is on.
    pub(super) fn new(scene: &Scene, ctx: &mut dyn RenderingBackend, resources: &mut ConstructionResources) -> Result<Option<Self>> {
        let c = config();
        if !c.sky || c.authored_sky || !(c.sky_depth || c.sky_cube) { return Ok(None); }
        // A map with a sky camera but no sky face still shows the room in its holes.
        let room = c.sky_cube && scene.sky_origin.is_some();
        let mut skipped = 0;
        let mut positions: [Vec<(Vec<Vec3>, &Vec<u16>)>; 2] = [Vec::new(), Vec::new()];
        for b in scene.batches.iter().filter(|b| b.sky) {
            if b.model.is_some() || b.animation.is_some() || b.deforms.iter().any(Deform::moves_vertices) || b.motion.is_some() {
                skipped += 1;
                continue;
            }
            let portal = b.layers.iter().all(|l| l.spec.blend == Blend::Invisible);
            positions[usize::from(!portal)].push((b.base.iter().map(|v| v.position).collect::<Vec<_>>(), &b.full_indices));
        }
        let groups = positions.each_ref().map(|g| chunks(g.iter().map(|(p, i)| (&p[..], &i[..]))));
        let triangles = groups.iter().flatten().map(|c| c.1.len() / 3).sum::<usize>();
        println!("[lg/vita-renderer] sky_surfaces portal_batches={} layer_batches={} skipped_moving={skipped} chunks={} triangles={triangles} vertices={}",
            positions[0].len(), positions[1].len(), groups.iter().flatten().count(), groups.iter().flatten().map(|c| c.0.len()).sum::<usize>());
        if groups.iter().all(Vec::is_empty) && !room { return Ok(None); }
        let mut buffers: [Vec<(BufferId, BufferId, i32)>; 2] = [Vec::new(), Vec::new()];
        for (group, out) in groups.iter().zip(&mut buffers) {
            for (vertices, indices) in group {
                let vertex = ctx.new_buffer(BufferType::VertexBuffer, BufferUsage::Immutable, BufferSource::slice(vertices));
                let index = ctx.new_buffer(BufferType::IndexBuffer, BufferUsage::Immutable, BufferSource::slice(indices));
                resources.buffers.extend([vertex, index]);
                out.push((vertex, index, indices.len() as i32));
            }
        }
        let params = PipelineParams {
            depth_test: Comparison::LessOrEqual,
            depth_write: true,
            ..Default::default()
        };
        let layout = [BufferLayout::default()];
        let attributes = [VertexAttribute::new("position", VertexFormat::Float3)];
        let _name = crate::shader_cache::Name::new("retained-sky-depth");
        let depth_shader = ctx.new_shader(ShaderSource::Glsl { vertex: DEPTH_VERTEX, fragment: DEPTH_FRAGMENT },
            ShaderMeta { images: vec![], uniforms: UniformBlockLayout { uniforms: vec![UniformDesc::new("Mvp", UniformType::Mat4)] } })
            .context("sky depth shader")?;
        resources.shaders.push(depth_shader);
        let depth_pipeline = ctx.new_pipeline(&layout, &attributes, depth_shader,
            PipelineParams { color_write: (false, false, false, false), ..params });
        resources.pipelines.push(depth_pipeline);
        let cube_meta = || ShaderMeta { images: vec!["Cube".into()], uniforms: UniformBlockLayout { uniforms: vec![
            UniformDesc::new("Mvp", UniformType::Mat4), UniformDesc::new("Rot", UniformType::Mat4),
            UniformDesc::new("Eye", UniformType::Float4), UniformDesc::new("Face", UniformType::Float4)] } };
        let cube = if c.sky_cube && !buffers[1].is_empty() {
            let _name = crate::shader_cache::Name::new("retained-sky-cube");
            let shader = ctx.new_shader(ShaderSource::Glsl { vertex: CUBE_VERTEX, fragment: CUBE_FRAGMENT }, cube_meta())
                .context("sky cube shader")?;
            resources.shaders.push(shader);
            let pipeline = ctx.new_pipeline(&layout, &attributes, shader, params);
            resources.pipelines.push(pipeline);
            Some((shader, pipeline))
        } else { None };
        let background = if room {
            let _name = crate::shader_cache::Name::new("retained-sky-cube-background");
            let shader = ctx.new_shader(ShaderSource::Glsl { vertex: BACKGROUND_VERTEX, fragment: CUBE_FRAGMENT }, cube_meta())
                .context("sky cube background shader")?;
            resources.shaders.push(shader);
            // The gradient's state: no depth test or write.
            let pipeline = ctx.new_pipeline(&layout, &[VertexAttribute::new("position", VertexFormat::Float2)], shader,
                PipelineParams::default());
            resources.pipelines.push(pipeline);
            Some((shader, pipeline))
        } else { None };
        Ok(Some(Self { chunks: buffers, depth: (depth_shader, depth_pipeline), cube, background, triangles }))
    }
    /// True when `draw_background` replaces the gradient: a captured room set.
    pub(super) fn has_background(&self, cube: Option<&CubeView>) -> bool {
        self.background.is_some() && cube.is_some_and(|c| c.room_row.is_some())
    }
    /// The room faces as the full-screen background, on the gradient's triangle.
    pub(super) fn draw_background(&self, ctx: &mut dyn RenderingBackend, triangle: &Bindings, projection: Mat4, eye: Vec3, cube: &CubeView) {
        let (Some((_, pipeline)), Some(row)) = (self.background, cube.room_row) else { return };
        ctx.apply_pipeline(&pipeline);
        ctx.apply_bindings(&Bindings { vertex_buffers: triangle.vertex_buffers.clone(), index_buffer: triangle.index_buffer,
            images: vec![cube.texture] });
        ctx.apply_uniforms(UniformsSource::table(&CubeUniforms { mvp: projection.inverse(), rot: cube.rotation,
            eye: eye.extend(1.).to_array(), face: [cube.inset, if cube.flip { 1. } else { 0. }, row as f32, cube.rows as f32] }));
        ctx.draw(0, 3, 1);
    }
    /// One draw per chunk. With `cube`, the sky shader faces sample their own set of faces;
    /// portal faces (shown by the background) and any face whose set was not captured go into
    /// depth only.
    pub(super) fn draw(&self, ctx: &mut dyn RenderingBackend, projection: Mat4, eye: Vec3, cube: Option<&CubeView>) -> u64 {
        let mut draws = 0;
        let mut depth_groups = Vec::new();
        for (group, chunks) in self.chunks.iter().enumerate() {
            if chunks.is_empty() { continue; }
            let row = cube.and_then(|c| if group == 0 { None } else { c.layers_row });
            let (Some(cube), Some(row), Some((_, pipeline))) = (cube, row, self.cube) else {
                depth_groups.push(chunks);
                continue;
            };
            ctx.apply_pipeline(&pipeline);
            let uniforms = CubeUniforms { mvp: projection, rot: cube.rotation, eye: eye.extend(1.).to_array(),
                face: [cube.inset, if cube.flip { 1. } else { 0. }, row as f32, cube.rows as f32] };
            for (i, &(vertex, index, count)) in chunks.iter().enumerate() {
                ctx.apply_bindings(&Bindings { vertex_buffers: vec![vertex], index_buffer: index, images: vec![cube.texture] });
                if i == 0 { ctx.apply_uniforms(UniformsSource::table(&uniforms)); }
                ctx.draw(0, count, 1);
                draws += 1;
            }
        }
        if !depth_groups.is_empty() {
            ctx.apply_pipeline(&self.depth.1);
            for (i, &(vertex, index, count)) in depth_groups.into_iter().flatten().enumerate() {
                ctx.apply_bindings(&Bindings { vertex_buffers: vec![vertex], index_buffer: index, images: vec![] });
                if i == 0 { ctx.apply_uniforms(UniformsSource::table(&DepthUniforms { mvp: projection })); }
                ctx.draw(0, count, 1);
                draws += 1;
            }
        }
        draws
    }
    pub(super) fn delete(&self, ctx: &mut dyn RenderingBackend) {
        for &(vertex, index, _) in self.chunks.iter().flatten() { ctx.delete_buffer(vertex); ctx.delete_buffer(index); }
        ctx.delete_pipeline(self.depth.1);
        ctx.delete_shader(self.depth.0);
        for (shader, pipeline) in self.cube.into_iter().chain(self.background) { ctx.delete_pipeline(pipeline); ctx.delete_shader(shader); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sky_chunks_rebase_indices_and_split_below_the_u16_limit() {
        let a = vec![Vec3::ZERO; 3];
        let b = vec![Vec3::ONE; 4];
        let out = chunks([(&a[..], &[0u16, 1, 2][..]), (&b[..], &[0u16, 1, 2, 2, 3, 0][..])]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].1, vec![0, 1, 2, 3, 4, 5, 5, 6, 3]);
        let big = vec![Vec3::ZERO; 40000];
        let out = chunks([(&big[..], &[0u16, 1, 2][..]), (&big[..], &[0u16, 1, 2][..])]);
        assert_eq!(out.len(), 2, "two 40000-vertex surfaces cannot share u16 indices");
        assert_eq!(out[1].1, vec![0, 1, 2]);
        assert!(chunks([(&a[..], &[][..])]).is_empty(), "empty surfaces add nothing");
    }
    /// The fragment shader's face table, replayed on the CPU: each face camera's NDC point maps
    /// back to the same face cell and the same coordinates.
    #[test]
    fn cube_lookup_inverts_every_capture_face() {
        fn lookup(d: Vec3) -> (Vec2, Vec2) {
            let a = d.abs();
            if a.x >= a.y && a.x >= a.z {
                if d.x > 0. { (vec2(-d.y, d.z) / a.x, vec2(0., 0.)) } else { (vec2(d.y, d.z) / a.x, vec2(1., 0.)) }
            } else if a.y >= a.z {
                if d.y > 0. { (vec2(d.x, d.z) / a.y, vec2(2., 0.)) } else { (vec2(-d.x, d.z) / a.y, vec2(0., 1.)) }
            } else if d.z > 0. { (vec2(d.y, d.x) / a.z, vec2(1., 1.)) } else { (vec2(-d.y, d.x) / a.z, vec2(2., 1.)) }
        }
        for (face, (forward, up)) in crate::render::sky_cube::FACES.iter().enumerate() {
            let camera = Mat4::perspective_rh_gl(std::f32::consts::FRAC_PI_2, 1., 4., 1000.)
                * Mat4::look_at_rh(Vec3::ZERO, *forward, *up);
            for ndc in [vec2(0.3, -0.6), vec2(-0.9, 0.2), vec2(0., 0.)] {
                let right = forward.cross(*up);
                let d = *forward + right * ndc.x + *up * ndc.y;
                let clip = camera * (d * 50.).extend(1.);
                assert!((clip.truncate().truncate() / clip.w - ndc).length() < 1e-4, "face {face} camera");
                let (f, cell) = lookup(d);
                assert!((f - ndc).length() < 1e-5, "face {face} coordinates");
                assert_eq!(cell, vec2((face % 3) as f32, (face / 3) as f32), "face {face} cell");
            }
        }
    }
}
