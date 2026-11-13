//! World alpha fill cuts, each behind its own key and default off: `world_alpha_lite` (a lean
//! fragment program for unlit, untested world alpha) and `water_stage_merge` (one draw for a
//! filter stage and the alpha stage that follows it on the same surface). The effect-pass
//! program `fx_lite` is in `render_fx::retained`.
use super::*;

/// FRAGMENT for an unlit, unclamped world stage without alpha test. Surface.y=0 makes the
/// lighting factor exactly 1, Surface.x=0 leaves uv as is and Surface.z=0 never discards, so
/// each removed line is a no-op there. The uniform block is FRAGMENT's.
pub(super) const ALPHA_LITE_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying highp vec2 uv;
varying lowp vec4 color;
uniform sampler2D Diffuse;
uniform highp vec4 Gain, Alpha;
// FOG
void main(){
 vec4 diffuse=texture2D(Diffuse,uv);
 float a=diffuse.a*Gain.a*color.a;
 vec3 rgb=diffuse.rgb*Gain.rgb;
 gl_FragColor=vec4(fogged(rgb,Alpha.z),a);
}"#;

/// VERTEX for a merged pair: the same deformation and texture coordinates, once per stage.
/// Neither stage uses environment or vector coordinates or a dot alpha (see `merge_stage`).
pub(super) const MERGE_VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
attribute vec4 lightmap;
attribute vec3 normal;
uniform mat4 Mvp, Model, Uv, UvAfter, Uv2, UvAfter2;
uniform highp vec4 Turb, Deform, Wave, MoveDir, Turb2, Alpha2;
varying highp vec2 uv;
varying highp vec2 uv2;
varying lowp vec4 color;
varying highp vec3 worldPosition;
float wave(float x,float kind){
 x=fract(x);
 if(kind<0.5)return sin(x*6.28318530718);
 if(kind<1.5)return 1.0-4.0*abs(x-0.5);
 if(kind<2.5)return x<0.5?1.0:-1.0;
 if(kind<3.5)return x;
 if(kind<4.5)return 1.0-x;
 return 0.0;
}
void main(){
 vec3 p=position;
 float phase=Wave.z+Deform.z*Wave.w;
 if(Deform.x>0.5&&Deform.x<1.5){
  phase+=(p.x+p.y+p.z)/max(Deform.y,0.001);
  p+=normal*(Wave.x+Wave.y*wave(phase,Deform.w));
 }else if(Deform.x>1.5&&Deform.x<2.5){
  p+=MoveDir.xyz*(Wave.x+Wave.y*wave(phase,Deform.w));
 }else if(Deform.x>2.5){
  p+=normal*sin(texcoord.x*Wave.x+Deform.z*Wave.z)*Wave.y;
 }
 vec4 world=Model*vec4(p,1.0);
 worldPosition=world.xyz;
 vec2 base=texcoord;
 uv=(Uv*vec4(base,0.0,1.0)).xy;
 vec2 ripple=sin(vec2((world.x+world.z)/1024.0,world.y/1024.0)*6.28318530718
  +vec2((Turb.y+Turb.z*Turb.w)*6.28318530718))*Turb.x;
 uv+=(UvAfter*vec4(ripple,0.0,0.0)).xy;
 uv2=(Uv2*vec4(base,0.0,1.0)).xy;
 vec2 ripple2=sin(vec2((world.x+world.z)/1024.0,world.y/1024.0)*6.28318530718
  +vec2((Turb2.y+Turb2.z*Turb2.w)*6.28318530718))*Turb2.x;
 uv2+=(UvAfter2*vec4(ripple2,0.0,0.0)).xy;
 color=color0/255.0;
 color.a=Alpha2.y>0.5?color.a:1.0;
 color.a=clamp(color.a,0.0,1.0);
 gl_Position=Mvp*vec4(p,1.0);
}"#;

/// The filter stage then the alpha stage give c2*a2 + dst*c1*(1-a2). Blending one,src_alpha
/// keeps the first term and replaces the colour c1 by its luma, the only per-pixel scalar a
/// fixed blend can apply to dst together with an added term.
pub(super) const MERGE_FRAGMENT: &str = r#"#version 100
precision mediump float;
varying highp vec2 uv;
varying highp vec2 uv2;
varying lowp vec4 color;
uniform sampler2D Diffuse, Detail;
uniform highp vec4 Gain, Alpha, Gain2, Alpha2;
// FOG
void main(){
 vec4 t1=texture2D(Diffuse,uv);
 vec4 t2=texture2D(Detail,uv2);
 vec3 c1=fogged(t1.rgb*Gain.rgb,Alpha.z);
 float a2=t2.a*Gain2.a*color.a;
 vec3 c2=fogged(t2.rgb*Gain2.rgb,Alpha2.z);
 gl_FragColor=vec4(c2*a2,dot(c1,vec3(0.299,0.587,0.114))*(1.0-a2));
}"#;

/// Uniform words of the merged program: the filter stage's world payload, then the alpha
/// stage's Uv2, UvAfter2, Turb2, Gain2 and Alpha2.
pub(super) const MERGE_FLOATS: usize = 143 + 16 + 16 + 4 + 4 + 4;

pub(super) fn merge_meta() -> ShaderMeta {
    let mut meta = shader::meta();
    meta.images = vec!["Diffuse".into(), "Detail".into()];
    meta.uniforms.uniforms.extend([
        UniformDesc::new("Uv2", UniformType::Mat4), UniformDesc::new("UvAfter2", UniformType::Mat4),
        UniformDesc::new("Turb2", UniformType::Float4), UniformDesc::new("Gain2", UniformType::Float4),
        UniformDesc::new("Alpha2", UniformType::Float4),
    ]);
    meta
}

/// A stage `world_alpha_lite` can draw: same arithmetic as FRAGMENT (see ALPHA_LITE_FRAGMENT).
pub(super) fn lite_recipe(r: &Recipe) -> bool {
    !r.opaque && !r.lit && !r.stage.clamp && r.stage.alpha_test == 0
}

/// Shared conditions for both stages of a merged pair: unlit, unclamped, untested, plain
/// texture coordinates and no dot alpha.
fn merge_stage(r: &Recipe) -> bool {
    !r.opaque && !r.lit && !r.stage.clamp && r.stage.alpha_test == 0 && !r.stage.environment
        && r.stage.vector_uv.is_none() && r.stage.dot_alpha.is_none() && !r.stage.depth_equal
}

/// For every group, the alpha-stage group it can merge with when it is a filter stage. Two
/// pending groups with the same origin list received the same faces in the same order, so
/// their geometry is identical; the rank pair makes them consecutive stages of one surface.
pub(super) fn partners(recipes: &[&Recipe], origins: &[Vec<(u32, u32)>]) -> Vec<Option<u32>> {
    let mut by_origin: std::collections::HashMap<&[(u32, u32)], Vec<usize>> = Default::default();
    for (id, o) in origins.iter().enumerate() {
        if !o.is_empty() { by_origin.entry(o.as_slice()).or_default().push(id); }
    }
    let mut out = vec![None; recipes.len()];
    for ids in by_origin.values() {
        for &f in ids {
            let rf = recipes[f];
            if rf.stage.blend != Blend::Filter || !merge_stage(rf) { continue; }
            let partner = ids.iter().copied().find(|&a| {
                let ra = recipes[a];
                ra.rank == rf.rank + 1 && ra.stage.blend == Blend::Alpha && merge_stage(ra)
                    && ra.cull == rf.cull && ra.model == rf.model && ra.motion.is_none() && rf.motion.is_none()
            });
            out[f] = partner.map(|a| a as u32);
        }
    }
    out
}

/// This frame's pairs among the prepared world alpha draws: (filter index, alpha index), both
/// indices into `prepared`. A pair is taken only when no other world draw has exactly its
/// depth: the joint sort is stable and puts world draws before actors at equal depth, so the two
/// stages then sit next to each other in the submitted order and one draw can stand for both.
pub(super) fn frame_pairs<T>(prepared: &[T], at: impl Fn(&T) -> (f32, usize), partner: &[Option<u32>], out: &mut Vec<(usize, usize)>) {
    out.clear();
    for (i, p) in prepared.iter().enumerate() {
        let (depth, group) = at(p);
        let Some(a) = partner[group] else { continue };
        let Some(j) = prepared.iter().position(|q| at(q).1 == a as usize) else { continue };
        let bits = depth.to_bits();
        if j <= i || at(&prepared[j]).0.to_bits() != bits { continue; }
        if prepared.iter().filter(|q| at(q).0.to_bits() == bits).count() == 2 { out.push((i, j)); }
    }
}

/// The merged payload: the filter stage's words, then the alpha stage's.
pub(super) fn merge_payload(filter: &shader::Uniforms, alpha: &shader::Uniforms, out: &mut [f32; MERGE_FLOATS]) {
    out[..143].copy_from_slice(filter.values());
    let mut at = 143;
    for field in [&alpha.uv[..], &alpha.uv_after[..], &alpha.turb[..], &alpha.gain[..], &alpha.alpha[..]] {
        out[at..at + field.len()].copy_from_slice(field);
        at += field.len();
    }
    debug_assert_eq!(at, MERGE_FLOATS);
}

/// Programs and per-frame scratch for the two world keys. Absent programs mean the key is off.
pub(super) struct State {
    /// `world_alpha_lite`: program, pipelines indexed like the world's (blend * 3 + cull), cache.
    pub lite: Option<(ShaderId, Vec<Pipeline>, super::super::uniform_fields::Cache)>,
    /// `water_stage_merge` = "merge": program, one pipeline per cull, cache.
    pub merge: Option<(ShaderId, Vec<Pipeline>, super::super::uniform_fields::Cache)>,
    /// `water_stage_merge` other than "off": the alpha-stage partner of each filter group.
    pub partner: Vec<Option<u32>>,
    pub drop_filter: bool,
    pub pairs: Vec<(usize, usize)>,
    pub payload: Box<[f32; MERGE_FLOATS]>,
}
impl State {
    pub fn off() -> Self {
        Self { lite: None, merge: None, partner: Vec::new(), drop_filter: false, pairs: Vec::new(),
            payload: Box::new([0.; MERGE_FLOATS]) }
    }
    pub fn active(&self) -> bool { self.lite.is_some() || !self.partner.is_empty() }
    pub fn interrupt(&mut self) {
        if let Some(l) = &mut self.lite { l.2.interrupt(); }
        if let Some(m) = &mut self.merge { m.2.interrupt(); }
    }
    pub fn delete(&mut self, ctx: &mut dyn RenderingBackend) {
        for (shader, pipelines, _) in self.lite.take().into_iter().chain(self.merge.take()) {
            for p in pipelines { ctx.delete_pipeline(p); }
            ctx.delete_shader(shader);
        }
    }
}
/// Three pipelines, one per world cull mode, with the world's vertex layout and depth test.
pub(super) fn pipelines(ctx: &mut dyn RenderingBackend, shader: ShaderId, attributes: &[VertexAttribute],
    color: Option<BlendState>, alpha: Option<BlendState>) -> Vec<Pipeline> {
    [CullFace::Nothing, CullFace::Back, CullFace::Front].into_iter().map(|cull| {
        crate::frame_profile::pipeline_creation();
        ctx.new_pipeline(&[BufferLayout { stride: 52, ..Default::default() }], attributes, shader,
            PipelineParams { depth_test: Comparison::LessOrEqual, depth_write: true, cull_face: cull,
                front_face_order: FrontFaceOrder::Clockwise, color_blend: color, alpha_blend: alpha,
                ..Default::default() })
    }).collect()
}
/// One,src_alpha on colour; destination alpha kept.
pub(super) fn merge_blend() -> (Option<BlendState>, Option<BlendState>) {
    (Some(BlendState::new(Equation::Add, BlendFactor::One, BlendFactor::Value(BlendValue::SourceAlpha))),
     Some(BlendState::new(Equation::Add, BlendFactor::Zero, BlendFactor::One)))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn recipe(blend: Blend, rank: usize) -> Recipe {
        Recipe { stage: Stage { blend, ..Default::default() }, textures: Vec::new(), lit: false, rank,
            blend: blend_class(blend), opaque: false, cull: 0, model: None, motion: None, deform: None }
    }
    #[test]
    fn partners_pair_only_a_filter_then_alpha_over_the_same_faces() {
        let f = recipe(Blend::Filter, 0); let a = recipe(Blend::Alpha, 1);
        let other = recipe(Blend::Alpha, 1); let add = recipe(Blend::Add, 1);
        let o1 = vec![(3, 0), (3, 1)]; let o2 = vec![(4, 0)];
        let p = partners(&[&f, &a, &other, &add], &[o1.clone(), o1.clone(), o2.clone(), o1.clone()]);
        assert_eq!(p, vec![Some(1), None, None, None]);
        // Another surface, or no origin record (key off), never pairs.
        assert_eq!(partners(&[&f, &a], &[o1.clone(), o2.clone()]), vec![None, None]);
        assert_eq!(partners(&[&f, &a], &[Vec::new(), Vec::new()]), vec![None, None]);
        // A lit, tested, clamped or environment-mapped stage keeps its own draw.
        for change in 0..4 {
            let mut b = a.clone();
            match change { 0 => b.lit = true, 1 => b.stage.alpha_test = 1, 2 => b.stage.clamp = true, _ => b.stage.environment = true }
            assert_eq!(partners(&[&f, &b], &[o1.clone(), o1.clone()]), vec![None, None], "change {change}");
        }
        let mut late = a.clone(); late.rank = 2;
        assert_eq!(partners(&[&f, &late], &[o1.clone(), o1.clone()]), vec![None, None]);
    }
    #[test]
    fn frame_pairs_need_both_stages_alone_at_their_depth() {
        let p = |group: usize, depth: f32| (depth, group);
        let at = |d: &(f32, usize)| *d;
        let partner = vec![Some(1), None, None];
        let mut out = Vec::new();
        frame_pairs(&[p(0, 5.), p(1, 5.), p(2, 7.)], at, &partner, &mut out);
        assert_eq!(out, vec![(0, 1)]);
        frame_pairs(&[p(0, 5.), p(2, 5.), p(1, 5.)], at, &partner, &mut out);
        assert!(out.is_empty(), "a third draw at the same depth could sit between them");
        frame_pairs(&[p(0, 5.), p(2, 7.)], at, &partner, &mut out);
        assert!(out.is_empty(), "the alpha stage is not visible");
        frame_pairs(&[p(1, 5.), p(0, 5.)], at, &partner, &mut out);
        assert!(out.is_empty(), "the alpha stage must follow the filter stage");
    }
    #[test]
    fn merge_meta_extends_the_world_block_in_payload_order() {
        let meta = merge_meta();
        let floats: usize = meta.uniforms.uniforms.iter().map(|u| u.uniform_type.size() / 4 * u.array_count).sum();
        assert_eq!(floats, MERGE_FLOATS);
        assert_eq!(meta.images, vec!["Diffuse".to_string(), "Detail".to_string()]);
    }
}
