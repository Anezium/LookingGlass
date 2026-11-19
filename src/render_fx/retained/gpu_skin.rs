//! Default-off vertex-shader skinning (`gpu_skin_ants`, `gpu_skin_actors`). Off never
//! creates GL objects or reads limits. Five independent bone-local offsets are preserved;
//! no influence truncation. Each packet keeps the fragment program RC20 would pick for
//! its CPU twin (full, discard-free, lean, lean discard-free); only the vertex program changes.
use super::*;
use crate::skeletal::{SkinSurface, Transform};
use std::{cell::{Cell,RefCell}, rc::{Rc,Weak}, sync::Arc};
/// Bones per surface: two vec4 each (rotation quaternion; translation and scale).
/// Vertex uniforms: 2*60 palette + 4 Projection + SkinOrigin + SkinEye = 126 of the
/// 128 vectors the Vita reports (the fragment uniforms are a separate budget).
const BONES: usize = 60;
const VERTEX_UNIFORM_VECTORS: usize = BONES*2+4+2;
const EXTRA: usize = BONES*8+4+4;
const ORIGIN: usize = BONES*8;
const EYE: usize = BONES*8+4;
thread_local! {
    static AVAILABLE: Cell<usize> = const {Cell::new(0)};
    static INPUTS: RefCell<BTreeMap<usize,Weak<Input>>> = RefCell::default();
    /// Refused surfaces, kept while the load-time surface lives (identity checked).
    static REFUSED: RefCell<BTreeMap<usize,(std::sync::Weak<SkinSurface>,Reason)>> = RefCell::default();
}
#[repr(C)]
struct Vertex { offsets: [[f32;4];5], indices: [f32;4], fifth: f32, normal: [f32;3], uv: [f32;2] }
pub(crate) struct Input { source: Arc<SkinSurface>, vertices: Vec<Vertex>, bones: Vec<usize>, placeholder: Rc<crate::render_fx::prepared::Geometry>,
    /// Per palette bone, the longest bone-local offset that references it; `None` when a
    /// vertex has a negative weight or weights that do not sum to one (no cull bounds then).
    radius: Option<Vec<f32>> }
pub(crate) struct Occurrence { input: Rc<Input>, palette: [f32; EXTRA], bounds: Option<(Vec3,Vec3)> }
/// One GPU-skinned layer: the shared occurrence and whether this layer is an
/// environment (reflection) stage.
#[derive(Clone)]
pub(crate) struct Skin { pub(crate) occurrence: Rc<Occurrence>, pub(crate) environment: bool }
impl Occurrence {
    pub(crate) fn placeholder(&self)->Rc<crate::render_fx::prepared::Geometry> {self.input.placeholder.clone()}
    /// Conservative world box: every skinned vertex is a convex combination of its bones'
    /// transformed offsets, each inside a sphere around that bone's origin.
    pub(crate) fn bounds(&self)->Option<(Vec3,Vec3)> {self.bounds}
}
pub(crate) fn available()->bool { AVAILABLE.with(|v|v.get()>0) }
/// Why one skinned surface occurrence keeps CPU skinning (host census names).
#[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord)]
pub(crate) enum Reason { Gpu, Program, Corpse, Translucent, Mutable, Phase, Topology, Material, Influences, Bones, Pose }
/// The immutable stream of one load-time surface, built once; a refusal is remembered.
pub(crate) fn input(source:Arc<SkinSurface>)->Result<Rc<Input>,Reason> {
    let id=Arc::as_ptr(&source) as usize;
    if let Some(input)=INPUTS.with(|c|c.borrow().get(&id).and_then(Weak::upgrade)) {return Ok(input);}
    if let Some(reason)=REFUSED.with(|c|c.borrow().get(&id).filter(|(w,_)|w.upgrade().is_some_and(|a|Arc::ptr_eq(&a,&source))).map(|(_,r)|*r)) {
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        if census::enabled() {census::refused(reason,&source);}
        return Err(reason);
    }
    let refused=build(&source);
    if let Err(reason)=refused {
        REFUSED.with(|c| {let mut c=c.borrow_mut();if c.len()>512 {c.retain(|_,(w,_)|w.strong_count()>0);} c.insert(id,(Arc::downgrade(&source),reason));});
    }
    refused.map(|(vertices,bones,radius)| {
        let placeholder=Rc::new(crate::render_fx::prepared::Geometry::new(Mesh {upload_identity:None,surface:None,vertices:source.bind_vertices.clone(),indices:source.indices.clone(),texture:None}));
        let result=Rc::new(Input {source,vertices,bones,placeholder,radius});
        INPUTS.with(|c| {let mut c=c.borrow_mut();if c.len()>512 {c.retain(|_,v|v.strong_count()>0);} c.insert(id,Rc::downgrade(&result));});
        if crate::spike_log::on() || cfg!(all(feature="sim_maps_probe",not(target_os="vita"))) && std::env::var_os("LOOKING_GLASS_GPU_SKIN_CENSUS").is_some() {
            println!("[lg/gpu-skin] immutable vertices={} influences={} bones={}",result.vertices.len(),result.source.vertices.iter().map(|v|v.weights.len()).max().unwrap_or(0),result.bones.len());
        }
        result
    })
}
fn build(source:&SkinSurface)->Result<(Vec<Vertex>,Vec<usize>,Option<Vec<f32>>),Reason> {
    let refuse=|reason:Reason| {
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        if census::enabled() {census::refused(reason,source);}
        Err(reason)
    };
    if source.vertices.iter().any(|v|v.weights.len()>5) {return refuse(Reason::Influences);}
    if source.used_bones.len()>BONES {return refuse(Reason::Bones);}
    if source.stored_normals.len()!=source.vertices.len() {return refuse(Reason::Topology);}
    let bones=source.used_bones.clone();
    let vertices=source.vertices.iter().zip(&source.stored_normals).map(|(v,n)| {
        let mut out=Vertex {offsets:[[0.;4];5],indices:[0.;4],fifth:0.,normal:n.to_array(),uv:v.uv.to_array()};
        for (i,w) in v.weights.iter().enumerate() {
            out.offsets[i]=w.offset.extend(w.amount).to_array();
            let index=bones.binary_search(&w.bone).unwrap() as f32;
            if i<4 {out.indices[i]=index;} else {out.fifth=index;}
        }
        out
    }).collect();
    let mut radius=vec![0f32;bones.len()];
    let convex=source.vertices.iter().all(|v| v.weights.iter().all(|w|w.amount>=0.)
        && (v.weights.iter().map(|w|w.amount).sum::<f32>()-1.).abs()<=1e-3);
    for v in &source.vertices { for w in &v.weights {
        let i=bones.binary_search(&w.bone).unwrap();radius[i]=radius[i].max(w.offset.length());
    }}
    Ok((vertices,bones,convex.then_some(radius)))
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn occurrence(input:Rc<Input>,pose:&[Transform],scales:&[f32],transform:Transform,scale:f32,fullbright:bool,alpha:f32,eye:Vec3)->Result<Rc<Occurrence>,Reason> {
    let _span=crate::frame_profile::span("gpu_skin_palette");
    let _clock=Clock::new(0);
    let mut palette=[0.;EXTRA];
    // The model rotation and scale are folded into each bone; the model translation is
    // added once (SkinOrigin), as Transform::point adds it once on the CPU.
    for (i,&bone) in input.bones.iter().enumerate() {
        let b=pose.get(bone).ok_or(Reason::Pose)?;let s=scales.get(bone).copied().unwrap_or(1.);
        let q=transform.rotation*b.rotation;let t=transform.rotation*(b.translation*scale);
        palette[i*8..i*8+8].copy_from_slice(&[q.x,q.y,q.z,q.w,t.x,t.y,t.z,s*scale]);
    }
    let bounds=input.radius.as_ref().map(|radius| {
        let (mut lo,mut hi)=(Vec3::splat(f32::INFINITY),Vec3::splat(f32::NEG_INFINITY));
        for (i,r) in radius.iter().enumerate() {
            let c=Vec3::from_slice(&palette[i*8+4..i*8+7])+transform.translation;
            // 1 % and one unit of slack for float rounding in the vertex program.
            let r=r*palette[i*8+7].abs()*1.01+1.;
            lo=lo.min(c-Vec3::splat(r));hi=hi.max(c+Vec3::splat(r));
        }
        (lo,hi)
    });
    palette[ORIGIN..ORIGIN+4].copy_from_slice(&[transform.translation.x,transform.translation.y,transform.translation.z,f32::from(fullbright)]);
    palette[EYE..EYE+4].copy_from_slice(&[eye.x,eye.y,eye.z,0.]);
    let _=alpha;
    Ok(Rc::new(Occurrence {input,palette,bounds}))
}
fn vertex()->String {
    let mut s=format!("#version 100\nattribute vec4 Offset0,Offset1,Offset2,Offset3,Offset4;\nattribute vec4 Indices; attribute float Fifth; attribute vec3 BindNormal; attribute vec2 texcoord;\nuniform vec4 Bones[{}]; uniform vec4 SkinOrigin; uniform vec4 SkinEye; uniform mat4 Projection;\nvarying highp vec2 uv; varying highp vec3 lm; varying lowp vec4 color; varying highp vec3 worldPosition;\nvec3 turn(vec4 q,vec3 v) {{vec3 t=2.0*cross(q.xyz,v);return v+q.w*t+cross(q.xyz,t);}}\nvoid main() {{vec3 p=vec3(0.0),n=vec3(0.0);vec4 q,ts;\n",BONES*2);
    for (i,index) in ["Indices.x","Indices.y","Indices.z","Indices.w","Fifth"].iter().enumerate() {
        // Zero-filled unused lanes do not need dynamic bone access.
        s+=&format!("if(Offset{i}.w>0.0){{int b=int({index})*2;q=Bones[b];ts=Bones[b+1];p+=(turn(q,Offset{i}.xyz*ts.w)+ts.xyz)*Offset{i}.w;n+=turn(q,BindNormal)*Offset{i}.w;}}\n");
    }
    // SkinEye.w: an environment layer takes the CPU reflection coordinates
    // (Stage::view_uv) from the skinned normal and the eye. SkinOrigin.w: fullbright.
    s+="worldPosition=p+SkinOrigin.xyz;gl_Position=Projection*vec4(worldPosition,1.0);uv=texcoord;lm=vec3(0.0);n/=max(length(n),0.000001);\
if(SkinEye.w>0.5){vec3 view=SkinEye.xyz-worldPosition;view/=max(length(view),0.000001);vec3 r=n*(2.0*dot(n,view))-view;uv=vec2(0.5+r.y*0.5,0.5-r.z*0.5);}\
float light=SkinOrigin.w>0.5?1.0:0.65+0.35*max(dot(n,normalize(vec3(0.3,-0.5,1.0))),0.0);light=floor(light*255.0)/255.0;color=vec4(vec3(light),1.0);}";
    s
}
fn skin_meta()->ShaderMeta {
    let mut m=meta();
    m.uniforms.uniforms.extend([UniformDesc::new("Bones",UniformType::Float4).array(BONES*2),UniformDesc::new("SkinOrigin",UniformType::Float4),UniformDesc::new("SkinEye",UniformType::Float4)]);m
}
struct Buffers { _input:Rc<Input>, vertex:BufferId,index:BufferId,bindings:Option<Bindings> }
/// Fragment program variants, indexed like `variant()`: 0 full, 1 discard-free,
/// 2 lean, 3 lean discard-free. A missing variant draws with the full program.
pub(crate) const VARIANTS: [&str; 4] = ["retained-gpu-skin-ants","retained-gpu-skin-discard-free","retained-gpu-skin-lean","retained-gpu-skin-lean-opaque"];
pub(crate) struct Gpu { shaders:[Option<ShaderId>;4],pipelines:BTreeMap<(usize,usize,bool),Pipeline>,buffers:BTreeMap<usize,Buffers> }
impl Gpu {
    pub(super) fn new(fragments:[Option<String>;4])->Result<Self> {
        let mut attributes=0;let mut uniforms=0;
        unsafe {gl::glGetIntegerv(0x8869,&mut attributes);}
        #[cfg(target_os="vita")]
        unsafe {gl::glGetIntegerv(0x8DFB,&mut uniforms);}
        #[cfg(not(target_os="vita"))]
        unsafe {gl::glGetIntegerv(0x8B4A,&mut uniforms);uniforms/=4;}
        println!("[lg/gpu-skin] limits attributes={attributes} vertex_uniform_vectors={uniforms} required_attributes=9 palette_vectors={} vertex_program_vectors={VERTEX_UNIFORM_VECTORS}",BONES*2);
        ensure!(attributes>=9 && uniforms as usize>=VERTEX_UNIFORM_VECTORS,"insufficient queried limits");
        let ctx=unsafe{get_internal_gl()}.quad_context;
        let vertex=vertex();
        let mut shaders=[None;4];
        for (i,fragment) in fragments.iter().enumerate() {
            let Some(fragment)=fragment else {continue;};
            crate::frame_profile::shader_creation();
            let _name=crate::shader_cache::Name::new(VARIANTS[i]);
            match ctx.new_shader(ShaderSource::Glsl {vertex:&vertex,fragment},skin_meta()) {
                Ok(shader)=>shaders[i]=Some(shader),
                Err(e) if i==0=>return Err(e).context("GPU skin shader"),
                Err(e)=>eprintln!("[lg/gpu-skin] warning {} failed, full program kept: {e:?}",VARIANTS[i]),
            }
        }
        AVAILABLE.with(|v|v.set(v.get()+1));
        println!("[lg/gpu-skin] READY programs={}",shaders.iter().flatten().count());
        Ok(Self {shaders,pipelines:BTreeMap::new(),buffers:BTreeMap::new()})
    }
    pub(super) fn upload(&mut self,skin:&Occurrence) {
        let id=Rc::as_ptr(&skin.input) as usize;
        self.buffers.entry(id).or_insert_with(|| {
            let ctx=unsafe{get_internal_gl()}.quad_context;
            let vertex=ctx.new_buffer(BufferType::VertexBuffer,BufferUsage::Immutable,BufferSource::slice(&skin.input.vertices));
            let index=ctx.new_buffer(BufferType::IndexBuffer,BufferUsage::Immutable,BufferSource::slice(&skin.input.source.indices));
            Buffers {_input:skin.input.clone(),vertex,index,bindings:None}
        });
    }
    /// One GPU-skinned packet. `variant` is the fragment program the CPU twin would use.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw(&mut self,p:&Pass,skin:&Skin,variant:usize,u:&[f32;UNIFORM_FLOATS],depth_write:bool,white:&Texture2D,cull:bool) {
        let _span=crate::frame_profile::span("gpu_skin_upload_draw");
        let _clock=Clock::new(1);
        let variant=if self.shaders[variant].is_some() {variant} else {count("vita_gpu_skin_variant_fallbacks",1);0};
        count(["vita_gpu_skin_full_draws","vita_gpu_skin_discard_free_draws","vita_gpu_skin_lean_draws","vita_gpu_skin_lean_opaque_draws"][variant],1);
        let occurrence=&skin.occurrence;
        let texture=p.mesh.texture.as_ref().unwrap_or(white).raw_miniquad_id();let white=white.raw_miniquad_id();
        let ctx=unsafe{get_internal_gl()}.quad_context;
        let shader=self.shaders[variant].unwrap();
        let pipeline=self.pipelines.entry((variant,p.blend_index,p.depth_equal)).or_insert_with(||{
            crate::frame_profile::pipeline_creation();
            ctx.new_pipeline(
            &[BufferLayout {stride:std::mem::size_of::<Vertex>() as i32,..Default::default()}],
            &[VertexAttribute::new("Offset0",VertexFormat::Float4),VertexAttribute::new("Offset1",VertexFormat::Float4),VertexAttribute::new("Offset2",VertexFormat::Float4),VertexAttribute::new("Offset3",VertexFormat::Float4),VertexAttribute::new("Offset4",VertexFormat::Float4),VertexAttribute::new("Indices",VertexFormat::Float4),VertexAttribute::new("Fifth",VertexFormat::Float1),VertexAttribute::new("BindNormal",VertexFormat::Float3),VertexAttribute::new("texcoord",VertexFormat::Float2)],shader,
            PipelineParams {depth_test:if p.depth_equal {Comparison::Equal}else{Comparison::LessOrEqual},..crate::render::depth_pipeline(crate::render::blend(p.blend_index))})});
        let b=self.buffers.get_mut(&(Rc::as_ptr(&occurrence.input) as usize)).unwrap();
        let bindings=b.bindings.get_or_insert_with(||Bindings {vertex_buffers:vec![b.vertex],index_buffer:b.index,images:vec![texture,white,white]});
        bindings.images[0]=texture;
        ctx.apply_pipeline(pipeline);unsafe{gl::glDepthMask(u8::from(depth_write));}
        ctx.apply_bindings(bindings);
        let mut words=[0.;UNIFORM_FLOATS+EXTRA];words[..UNIFORM_FLOATS].copy_from_slice(u);words[UNIFORM_FLOATS..].copy_from_slice(&occurrence.palette);
        words[UNIFORM_FLOATS+EYE+3]=f32::from(skin.environment);
        {let _upload=Clock::new(2);ctx.apply_uniforms(UniformsSource::table(&words));}
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        crate::sim_maps::probe::gpu_skin_packet(occurrence.input.vertices.len(),occurrence.input.source.indices.len(),u,
            &[p.effect as u32,p.blend_index as u32,p.depth_equal as u32,(!depth_write) as u32,p.lit as u32,p.clamp as u32,p.cutout.to_bits(),p.cutout_reference.to_bits(),p.fog.to_bits(),p.model as u32,p.filter_fade.to_bits()],p.mesh.texture.as_ref());
        if !crate::gpu_timing::masked(crate::gpu_timing::MASK_ACTORS) {
            // Same opt-in front-face cull as the CPU actor draw, restored at once.
            unsafe { if cull { gl::glEnable(0x0B44); gl::glCullFace(0x0404); } }
            ctx.draw(0,occurrence.input.source.indices.len() as i32,1);
            unsafe { if cull { gl::glDisable(0x0B44); } }
        }
        count("gpu_skin_draws",1);count("gpu_skin_logical_palette_bytes",(occurrence.input.bones.len()*32) as u64);count("gpu_skin_uniform_upload_bytes",((UNIFORM_FLOATS+EXTRA)*4) as u64);
    }
}
impl Drop for Gpu {fn drop(&mut self) {
    AVAILABLE.with(|v|v.set(v.get()-1));let ctx=unsafe{get_internal_gl()}.quad_context;
    for (_,b) in std::mem::take(&mut self.buffers) {ctx.delete_buffer(b.vertex);ctx.delete_buffer(b.index);}
    for (_,p) in std::mem::take(&mut self.pipelines) {ctx.delete_pipeline(p);}
    for shader in self.shaders.iter().flatten() {ctx.delete_shader(*shader);}
}}

thread_local! {static CENSUS:Cell<[u64;4]>=const {Cell::new([0;4])};}
struct Clock(Option<std::time::Instant>,usize);
impl Clock {fn new(i:usize)->Self {Self(crate::spike_log::on().then(std::time::Instant::now),i)}}
impl Drop for Clock {fn drop(&mut self) {if let Some(t)=self.0 {CENSUS.with(|c| {let mut row=c.get();row[self.1]+=t.elapsed().as_nanos() as u64;if self.1==1 {row[3]+=1;}c.set(row);});}}}
pub(crate) fn census_begin() {CENSUS.with(|v|v.set([0;4]));}
pub(crate) fn census_append(out:&mut String) {
    if !crate::render::vita_renderer::config().gpu_skin_ants {return;}
    use std::fmt::Write;let [palette,submit,upload,draws]=CENSUS.with(Cell::get);
    let _=write!(out," gpu_skin={draws}/{:.6}/{:.6}/{:.6}",palette as f64/1e6,upload as f64/1e6,submit as f64/1e6);
}
/// Host-only coverage census (`LOOKING_GLASS_GPU_SKIN_CENSUS`): per actor family and
/// reason, the skinned surface occurrences and vertices of each frame. Never on the Vita.
#[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
pub(crate) mod census {
    use super::Reason;
    use std::{cell::RefCell, collections::BTreeMap};
    thread_local! {
        static FAMILY: RefCell<Option<String>> = const {RefCell::new(None)};
        static ROWS: RefCell<BTreeMap<(String,Reason),[u64;2]>> = RefCell::default();
    }
    pub(crate) fn enabled()->bool {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ON.get_or_init(||std::env::var_os("LOOKING_GLASS_GPU_SKIN_CENSUS").is_some())
    }
    pub(crate) struct Family(Option<Option<String>>);
    impl Drop for Family { fn drop(&mut self) { if let Some(old)=self.0.take() {FAMILY.with(|f|*f.borrow_mut()=old);} } }
    pub(crate) fn family(name:&str)->Family {
        if !enabled() {return Family(None);}
        Family(Some(FAMILY.with(|f|f.borrow_mut().replace(name.to_owned()))))
    }
    pub(crate) fn note(reason:Reason,vertices:usize) {
        if !enabled() {return;}
        let family=FAMILY.with(|f|f.borrow().clone()).unwrap_or_else(||"unscoped".into());
        ROWS.with(|r| {let mut r=r.borrow_mut();let row=r.entry((family,reason)).or_default();row[0]+=1;row[1]+=vertices as u64;});
    }
    thread_local! { static REFUSED: RefCell<BTreeMap<(String,String),u64>> = RefCell::default(); }
    /// The surface facts behind a load-time refusal (bones, influences, vertices).
    pub(crate) fn refused(reason:Reason,source:&crate::skeletal::SkinSurface) {
        let family=FAMILY.with(|f|f.borrow().clone()).unwrap_or_else(||"unscoped".into());
        let influences=source.vertices.iter().map(|v|v.weights.len()).max().unwrap_or(0);
        let over=source.vertices.iter().filter(|v|v.weights.len()>5).count();
        let key=format!("{reason:?} bones={} influences={influences} vertices={} over5={over}",source.used_bones.len(),source.vertices.len());
        REFUSED.with(|b|*b.borrow_mut().entry((family,key)).or_default()+=1);
    }
    pub(crate) fn flush(frame:u64) {
        if enabled() {
            let rows=REFUSED.with(|b|std::mem::take(&mut *b.borrow_mut()));
            if !rows.is_empty() {println!("GPU_SKIN_REFUSED {}",serde_json::json!({"frame":frame,"rows":rows.into_iter().map(|((f,k),n)|serde_json::json!([f,k,n])).collect::<Vec<_>>()}));}
        }
        if !enabled() {return;}
        let rows=ROWS.with(|r|std::mem::take(&mut *r.borrow_mut()));
        let rows:Vec<_>=rows.into_iter().map(|((f,r),[n,v])|serde_json::json!([f,format!("{r:?}"),n,v])).collect();
        println!("GPU_SKIN_CENSUS {}",serde_json::json!({"frame":frame,"rows":rows}));
    }
}
#[cfg(not(all(feature="sim_maps_probe",not(target_os="vita"))))]
pub(crate) mod census {
    pub(crate) struct Family;
    pub(crate) fn family(_:&str)->Family {Family}
    pub(crate) fn note(_:super::Reason,_:usize) {}
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::skeletal::{SkinVertex,Weight};
    fn surface(n:usize)->Arc<SkinSurface> {
        Arc::new(SkinSurface {gpu_ant:true,stored_normals:vec![Vec3::Z],vertices:vec![SkinVertex {uv:Vec2::ZERO,weights:(0..n).map(|i|Weight {bone:i,amount:1./n as f32,offset:Vec3::splat(i as f32)}).collect()}],indices:vec![0,0,0],bind_vertices:vec![],used_bones:(0..n).collect()})
    }
    #[test]
    fn fifth_influence_is_preserved_and_sixth_falls_back() {
        let five=input(surface(5)).ok().unwrap();assert_eq!(five.vertices[0].fifth,4.);assert_eq!(five.vertices[0].offsets[4],[4.,4.,4.,0.2]);
        assert_eq!(input(surface(6)).err(),Some(Reason::Influences));
        let mut many=surface(1);Arc::get_mut(&mut many).unwrap().used_bones=(0..BONES+1).collect();assert_eq!(input(many).err(),Some(Reason::Bones));
    }
    /// The vertex program's position arithmetic, in Rust, for the CPU comparison below.
    fn emulate(v:&Vertex,palette:&[f32;EXTRA])->Vec3 {
        let turn=|q:Vec4,v:Vec3| {let t=2.*q.truncate().cross(v);v+q.w*t+q.truncate().cross(t)};
        let mut p=Vec3::ZERO;
        for lane in 0..5 {
            let o=Vec4::from_array(v.offsets[lane]);
            if o.w>0. {
                let b=(if lane<4 {v.indices[lane]} else {v.fifth}) as usize*8;
                let q=Vec4::from_slice(&palette[b..b+4]);let ts=Vec4::from_slice(&palette[b+4..b+8]);
                p+=(turn(q,o.truncate()*ts.w)+ts.truncate())*o.w;
            }
        }
        p+Vec3::from_slice(&palette[ORIGIN..ORIGIN+3])
    }
    #[test]
    fn vertex_program_positions_match_cpu_skinning() {
        let weights=||vec![Weight {bone:1,amount:0.5,offset:vec3(3.,-2.,7.)},Weight {bone:4,amount:0.3,offset:vec3(-1.,5.,0.5)},
            Weight {bone:6,amount:0.2,offset:vec3(0.,0.,-9.)}];
        let source=Arc::new(SkinSurface {gpu_ant:false,stored_normals:vec![Vec3::Z],vertices:vec![SkinVertex {uv:Vec2::ZERO,weights:weights()}],
            indices:vec![0,0,0],bind_vertices:vec![macroquad::prelude::Vertex::new(0.,0.,0.,0.,0.,WHITE)],used_bones:vec![1,4,6]});
        let pose:Vec<Transform>=(0..8).map(|i| Transform {rotation:Quat::from_euler(glam::EulerRot::XYZ,0.3*i as f32,-0.2*i as f32,0.7),
            translation:vec3(i as f32,2.*i as f32,-(i as f32))}).collect();
        let scales:Vec<f32>=(0..8).map(|i|1.+0.1*i as f32).collect();
        let model=Transform {rotation:Quat::from_rotation_z(1.1),translation:vec3(100.,-50.,20.)};
        let input=input(source).ok().unwrap();
        let o=occurrence(input.clone(),&pose,&scales,model,1.25,false,1.,Vec3::ZERO).ok().unwrap();
        let cpu=model.point(weights().iter().map(|w|pose[w.bone].point(w.offset*scales[w.bone])*w.amount).sum::<Vec3>()*1.25);
        let gpu=emulate(&input.vertices[0],&o.palette);
        assert!((cpu-gpu).length()<=1e-4*cpu.length().max(1.),"cpu {cpu} gpu {gpu}");
        let (lo,hi)=o.bounds().unwrap();
        assert!(cpu.cmpge(lo).all() && cpu.cmple(hi).all(),"vertex inside the cull box");
    }
    #[test]
    fn declaration_and_uniform_budget_fit_native_contract() {
        assert_eq!(std::mem::size_of::<Vertex>(),120);
        assert!(VERTEX_UNIFORM_VECTORS<=128);
        assert!(vertex().contains("SkinEye.w>0.5") && vertex().contains("uniform vec4 Bones[120]"));
        assert!(!vertex().contains("uniform mat4 Model"));
        let shader=vertex();assert!(shader.contains("Offset4"));assert!(shader.contains("Indices.w"));assert!(shader.contains("int(Fifth)"));
    }
}
