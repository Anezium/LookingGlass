//! Default-off army-ant experiment. Off never creates GL objects or reads limits.
//! Five independent bone-local offsets are preserved; no influence truncation.
use super::*;
use crate::skeletal::{SkinSurface, Transform};
use std::{cell::{Cell,RefCell}, rc::{Rc,Weak}, sync::Arc};
const BONES: usize = 28;
const EXTRA: usize = BONES*12+16+4;
thread_local! {
    static AVAILABLE: Cell<usize> = const {Cell::new(0)};
    static INPUTS: RefCell<BTreeMap<usize,Weak<Input>>> = RefCell::default();
}
#[repr(C)]
struct Vertex { offsets: [[f32;4];5], indices: [f32;4], fifth: f32, normal: [f32;3], uv: [f32;2] }
pub(crate) struct Input { source: Arc<SkinSurface>, vertices: Vec<Vertex>, bones: Vec<usize> }
pub(crate) struct Occurrence { input: Rc<Input>, palette: [f32; EXTRA] }
pub(crate) fn available()->bool { AVAILABLE.with(|v|v.get()>0) }
fn input(source:Arc<SkinSurface>)->Option<Rc<Input>> {
    let id=Arc::as_ptr(&source) as usize;
    if let Some(input)=INPUTS.with(|c|c.borrow().get(&id).and_then(Weak::upgrade)) {return Some(input);}
    if source.vertices.iter().any(|v|v.weights.len()>5) || source.used_bones.len()>BONES
        || source.stored_normals.len()!=source.vertices.len() {return None;}
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
    let result=Rc::new(Input {source,vertices,bones});
    INPUTS.with(|c| {let mut c=c.borrow_mut();if c.len()>512 {c.retain(|_,v|v.strong_count()>0);} c.insert(id,Rc::downgrade(&result));});
    println!("[lg/gpu-skin] immutable vertices={} influences={} bones={}",result.vertices.len(),result.source.vertices.iter().map(|v|v.weights.len()).max().unwrap_or(0),result.bones.len());
    Some(result)
}
pub(crate) fn occurrence(source:Arc<SkinSurface>,pose:&[Transform],scales:&[f32],transform:Transform,scale:f32,fullbright:bool,alpha:f32)->Option<Rc<Occurrence>> {
    let input=input(source)?;
    let _span=crate::frame_profile::span("gpu_skin_palette");
    let _clock=Clock::new(0);
    let mut palette=[0.;EXTRA];
    for (i,&bone) in input.bones.iter().enumerate() {
        let b=pose.get(bone)?;let s=scales.get(bone).copied().unwrap_or(1.);
        let x=b.rotation*(Vec3::X*s);let y=b.rotation*(Vec3::Y*s);let z=b.rotation*(Vec3::Z*s);
        palette[i*12..i*12+12].copy_from_slice(&[x.x,y.x,z.x,b.translation.x,x.y,y.y,z.y,b.translation.y,x.z,y.z,z.z,b.translation.z]);
    }
    // Model transform is visual only; tags and pose retain the CPU Transform.
    let m=Mat4::from_cols((transform.rotation*(Vec3::X*scale)).extend(0.),(transform.rotation*(Vec3::Y*scale)).extend(0.),(transform.rotation*(Vec3::Z*scale)).extend(0.),transform.translation.extend(1.));
    palette[BONES*12..BONES*12+16].copy_from_slice(&m.to_cols_array());
    palette[BONES*12+16..].copy_from_slice(&[f32::from(fullbright),alpha,0.,0.]);
    Some(Rc::new(Occurrence {input,palette}))
}
fn vertex()->String {
    let mut s=format!("#version 100\nattribute vec4 Offset0,Offset1,Offset2,Offset3,Offset4;\nattribute vec4 Indices; attribute float Fifth; attribute vec3 BindNormal; attribute vec2 texcoord;\nuniform vec4 Bones[{}]; uniform mat4 SkinModel; uniform vec4 SkinLight;\nuniform mat4 Model,Projection;\nvarying highp vec2 uv; varying highp vec3 lm; varying lowp vec4 color; varying highp vec3 worldPosition;\nvec3 point(vec4 v,int b) {{int i=b*3;vec4 p=vec4(v.xyz,1.0);return vec3(dot(Bones[i],p),dot(Bones[i+1],p),dot(Bones[i+2],p))*v.w;}}\nvec3 direction(vec4 v,int b) {{int i=b*3;return vec3(dot(Bones[i].xyz,BindNormal),dot(Bones[i+1].xyz,BindNormal),dot(Bones[i+2].xyz,BindNormal))*v.w;}}\nvoid main() {{vec3 p=vec3(0.0),n=vec3(0.0);\n",BONES*3);
    for (i,index) in ["Indices.x","Indices.y","Indices.z","Indices.w","Fifth"].iter().enumerate() {
        // Zero-filled unused lanes do not need dynamic bone access.
        s+=&format!("if(Offset{i}.w>0.0){{p+=point(Offset{i},int({index}));n+=direction(Offset{i},int({index}));}}\n");
    }
    s+="worldPosition=(SkinModel*vec4(p,1.0)).xyz;gl_Position=Projection*vec4(worldPosition,1.0);uv=texcoord;lm=vec3(0.0);n=(SkinModel*vec4(n,0.0)).xyz;n/=max(length(n),0.000001);float light=SkinLight.x>0.5?1.0:0.65+0.35*max(dot(n,normalize(vec3(0.3,-0.5,1.0))),0.0);light=floor(light*255.0)/255.0;color=vec4(vec3(light),floor(clamp(SkinLight.y,0.0,1.0)*255.0)/255.0);}";
    s
}
fn skin_meta()->ShaderMeta {
    let mut m=meta();
    m.uniforms.uniforms.extend([UniformDesc::new("Bones",UniformType::Float4).array(BONES*3),UniformDesc::new("SkinModel",UniformType::Mat4),UniformDesc::new("SkinLight",UniformType::Float4)]);m
}
struct Buffers { _input:Rc<Input>, vertex:BufferId,index:BufferId }
pub(crate) struct Gpu { shader:ShaderId,pipelines:BTreeMap<(usize,bool),Pipeline>,buffers:BTreeMap<usize,Buffers> }
impl Gpu {
    pub(super) fn new(fragment:&str)->Result<Self> {
        let mut attributes=0;let mut uniforms=0;
        unsafe {gl::glGetIntegerv(0x8869,&mut attributes);}
        #[cfg(target_os="vita")]
        unsafe {gl::glGetIntegerv(0x8DFB,&mut uniforms);}
        #[cfg(not(target_os="vita"))]
        unsafe {gl::glGetIntegerv(0x8B4A,&mut uniforms);uniforms/=4;}
        println!("[lg/gpu-skin] limits attributes={attributes} vertex_uniform_vectors={uniforms} required_attributes=9 palette_vectors={} total_conservative_vectors={}",BONES*3,BONES*3+UNIFORM_FLOATS.div_ceil(4)+5);
        ensure!(attributes>=9 && uniforms as usize>=BONES*3+UNIFORM_FLOATS.div_ceil(4)+5,"insufficient queried limits");
        let ctx=unsafe{get_internal_gl()}.quad_context;
        let _name=crate::shader_cache::Name::new("retained-gpu-skin-ants");
        let shader=ctx.new_shader(ShaderSource::Glsl {vertex:&vertex(),fragment},skin_meta()).context("army-ant skin shader")?;
        AVAILABLE.with(|v|v.set(v.get()+1));
        Ok(Self {shader,pipelines:BTreeMap::new(),buffers:BTreeMap::new()})
    }
    pub(super) fn upload(&mut self,skin:&Occurrence) {
        let id=Rc::as_ptr(&skin.input) as usize;
        self.buffers.entry(id).or_insert_with(|| {
            let ctx=unsafe{get_internal_gl()}.quad_context;
            let vertex=ctx.new_buffer(BufferType::VertexBuffer,BufferUsage::Immutable,BufferSource::slice(&skin.input.vertices));
            let index=ctx.new_buffer(BufferType::IndexBuffer,BufferUsage::Immutable,BufferSource::slice(&skin.input.source.indices));
            Buffers {_input:skin.input.clone(),vertex,index}
        });
    }
    pub(super) fn draw(&mut self,p:&Pass,skin:&Occurrence,u:&[f32;UNIFORM_FLOATS],depth_write:bool,white:&Texture2D) {
        let _span=crate::frame_profile::span("gpu_skin_upload_draw");
        let _clock=Clock::new(1);
        let texture=p.mesh.texture.as_ref().unwrap_or(white).raw_miniquad_id();let white=white.raw_miniquad_id();
        let ctx=unsafe{get_internal_gl()}.quad_context;
        let pipeline=self.pipelines.entry((p.blend_index,p.depth_equal)).or_insert_with(||ctx.new_pipeline(
            &[BufferLayout {stride:std::mem::size_of::<Vertex>() as i32,..Default::default()}],
            &[VertexAttribute::new("Offset0",VertexFormat::Float4),VertexAttribute::new("Offset1",VertexFormat::Float4),VertexAttribute::new("Offset2",VertexFormat::Float4),VertexAttribute::new("Offset3",VertexFormat::Float4),VertexAttribute::new("Offset4",VertexFormat::Float4),VertexAttribute::new("Indices",VertexFormat::Float4),VertexAttribute::new("Fifth",VertexFormat::Float1),VertexAttribute::new("BindNormal",VertexFormat::Float3),VertexAttribute::new("texcoord",VertexFormat::Float2)],self.shader,
            PipelineParams {depth_test:if p.depth_equal {Comparison::Equal}else{Comparison::LessOrEqual},..crate::render::depth_pipeline(crate::render::blend(p.blend_index))}));
        let b=&self.buffers[&(Rc::as_ptr(&skin.input) as usize)];
        ctx.apply_pipeline(pipeline);unsafe{gl::glDepthMask(u8::from(depth_write));}
        ctx.apply_bindings(&Bindings {vertex_buffers:vec![b.vertex],index_buffer:b.index,images:vec![texture,white,white]});
        let mut words=[0.;UNIFORM_FLOATS+EXTRA];words[..UNIFORM_FLOATS].copy_from_slice(u);words[UNIFORM_FLOATS..].copy_from_slice(&skin.palette);
        ctx.apply_uniforms(UniformsSource::table(&words));
        #[cfg(all(feature="sim_maps_probe",not(target_os="vita")))]
        crate::sim_maps::probe::gpu_skin_packet(skin.input.vertices.len(),skin.input.source.indices.len(),u,
            &[p.effect as u32,p.blend_index as u32,p.depth_equal as u32,(!depth_write) as u32,p.lit as u32,p.clamp as u32,p.cutout.to_bits(),p.cutout_reference.to_bits(),p.fog.to_bits(),p.model as u32,p.filter_fade.to_bits()],p.mesh.texture.as_ref());
        ctx.draw(0,skin.input.source.indices.len() as i32,1);
        count("gpu_skin_draws",1);count("gpu_skin_palette_bytes",(skin.input.bones.len()*48) as u64);
    }
}
impl Drop for Gpu {fn drop(&mut self) {
    AVAILABLE.with(|v|v.set(v.get()-1));let ctx=unsafe{get_internal_gl()}.quad_context;
    for (_,b) in std::mem::take(&mut self.buffers) {ctx.delete_buffer(b.vertex);ctx.delete_buffer(b.index);}
    for (_,p) in std::mem::take(&mut self.pipelines) {ctx.delete_pipeline(p);}ctx.delete_shader(self.shader);
}}

thread_local! {static CENSUS:Cell<[u64;3]>=const {Cell::new([0;3])};}
struct Clock(Option<std::time::Instant>,usize);
impl Clock {fn new(i:usize)->Self {Self(crate::spike_log::on().then(std::time::Instant::now),i)}}
impl Drop for Clock {fn drop(&mut self) {if let Some(t)=self.0 {CENSUS.with(|c| {let mut row=c.get();row[self.1]+=t.elapsed().as_nanos() as u64;if self.1==1 {row[2]+=1;}c.set(row);});}}}
pub(crate) fn census_begin() {CENSUS.with(|v|v.set([0;3]));}
pub(crate) fn census_append(out:&mut String) {
    if !crate::render::vita_renderer::config().gpu_skin_ants {return;}
    use std::fmt::Write;let [palette,submit,draws]=CENSUS.with(Cell::get);
    let _=write!(out," gpu_skin={draws}/{:.6}/{:.6}",palette as f64/1e6,submit as f64/1e6);
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
        let five=input(surface(5)).unwrap();assert_eq!(five.vertices[0].fifth,4.);assert_eq!(five.vertices[0].offsets[4],[4.,4.,4.,0.2]);
        assert!(input(surface(6)).is_none());
        let mut many=surface(1);Arc::get_mut(&mut many).unwrap().used_bones=(0..29).collect();assert!(input(many).is_none());
    }
    #[test]
    fn declaration_and_uniform_budget_fit_native_contract() {
        assert_eq!(std::mem::size_of::<Vertex>(),120);
        assert!(BONES*3+UNIFORM_FLOATS.div_ceil(4)+5<=128);
        let shader=vertex();assert!(shader.contains("Offset4"));assert!(shader.contains("Indices.w"));assert!(shader.contains("int(Fifth)"));
    }
}
