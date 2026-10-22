//! Deliberately one Alice surface. CPU animation/consumers remain authoritative.
//! Sources own immutable geometry; queued packets own the source and affine rows.
use super::*;
use crate::{character::skinning::{self, Params}, skeletal::{Skeleton, SkinSurface, Transform}};
use anyhow::{ensure, Context};
use macroquad::miniquad::*;
use std::{cell::Cell, collections::BTreeMap, sync::Arc};
const BONES: usize = 11;
const ROWS: usize = BONES * 3;
const CAP: usize = 2 * 1024 * 1024;
const SOURCES: usize = 2; // one current plus one still owned by an old packet
const FLOATS: usize = 147 + ROWS * 4 + 4;
const VERTEX: &str = r#"#version 100
attribute vec4 offset0; attribute vec4 reference0;
attribute vec4 offset1; attribute vec4 reference1; attribute vec2 texcoord;
uniform mat4 Projection; uniform vec4 SkinRows[33]; uniform vec4 SkinArgs;
varying highp vec2 uv; varying highp vec3 lm; varying lowp vec4 color; varying highp vec3 worldPosition;
vec3 point(vec4 p, int i){return vec3(dot(SkinRows[i],p),dot(SkinRows[i+1],p),dot(SkinRows[i+2],p));}
void main(){
 int a=int(reference0.w)*3; int b=int(reference1.w)*3;
 worldPosition=point(vec4(offset0.xyz,1.0),a)*offset0.w+point(vec4(offset1.xyz,1.0),b)*offset1.w;
 vec3 n=point(vec4(reference0.xyz,0.0),a)*offset0.w+point(vec4(reference1.xyz,0.0),b)*offset1.w;
 float len=length(n); n=len>0.0?n/len:vec3(0.0);
 float light=SkinArgs.x>0.5?1.0:0.65+0.35*max(0.0,dot(n,normalize(vec3(0.3,-0.5,1.0))));
 light=floor(light*255.0)/255.0;
 color=vec4(vec3(light),SkinArgs.y);lm=vec3(0.0);uv=texcoord;
 gl_Position=Projection*vec4(worldPosition,1.0);
}"#;
#[repr(C)]
#[derive(Clone, Copy)]
struct Packed { offset0:[f32;4], reference0:[f32;4], offset1:[f32;4], reference1:[f32;4], uv:[f32;2] }
struct Source {
    cpu:Arc<SkinSurface>, bones:Vec<usize>, packed:Vec<Packed>,
    reference:Rc<prepared::Geometry>, bounds:Vec<(Vec3,Vec3)>,
    shader:ShaderId, pipelines:BTreeMap<(usize,bool),Pipeline>, bindings:Bindings,
    bytes:usize, cpu_bytes:usize, uniform_calls:u64,
}
impl Drop for Source {
    fn drop(&mut self) {
        let ctx=unsafe {get_internal_gl()}.quad_context;
        for &p in self.pipelines.values() {ctx.delete_pipeline(p);}
        for &b in &self.bindings.vertex_buffers {ctx.delete_buffer(b);}
        ctx.delete_buffer(self.bindings.index_buffer);ctx.delete_shader(self.shader);
        CACHE_BYTES.with(|n|n.set(n.get()-self.bytes));
        CACHE_SOURCES.with(|n|n.set(n.get()-1));
        CACHE_CPU_BYTES.with(|n|n.set(n.get()-self.cpu_bytes));
    }
}
pub(crate) struct Packet { source:Rc<Source>, rows:[[f32;4];ROWS], args:[f32;4], pub(crate) center:Vec3 }
thread_local! {
    static SOURCE:RefCell<Option<Rc<Source>>>=const {RefCell::new(None)};
    static WARM_FAILED:Cell<bool>=const {Cell::new(false)};
    static CACHE_BYTES:Cell<usize>=const {Cell::new(0)};
    static CACHE_SOURCES:Cell<usize>=const {Cell::new(0)};
    static CACHE_CPU_BYTES:Cell<usize>=const {Cell::new(0)};
    static PACKETS:Cell<usize>=const {Cell::new(0)};
    #[cfg(not(target_os="vita"))]
    static OVERRIDE:Cell<Option<bool>>=const {Cell::new(None)};
}
pub(crate) fn enabled()->bool {
    #[cfg(not(target_os="vita"))]
    if let Some(on)=OVERRIDE.with(Cell::get) {return on;}
    crate::render::vita_renderer::config().gpu_skin
}
#[cfg(not(target_os="vita"))]
pub(crate) fn host_switch(on:bool) {OVERRIDE.with(|v|v.set(Some(on)));}
fn count(name:&'static str,n:u64) {if crate::frame_profile::enabled() {retained::count(name,n);}}
fn reject(reason:&'static str)->bool {count(reason,1);false}
fn unit(t:Transform)->bool {
    t.translation.is_finite() && t.rotation.is_finite() && (t.rotation.length_squared()-1.).abs()<=0.0001
}
fn pack(cpu:&SkinSurface,pose:&[Transform])->Result<(Vec<Packed>,Vec<usize>,Vec<Vertex>)> {
    ensure!(!cpu.vertices.is_empty() && cpu.indices.len()%3==0 && cpu.indices.iter().all(|&i|(i as usize)<cpu.vertices.len()),"topology");
    ensure!(!cpu.used_bones.is_empty() && cpu.used_bones.len()<=BONES,"palette exceeds {BONES}");
    ensure!(cpu.vertices.iter().all(|v|(1..=2).contains(&v.weights.len())),"more than two influences");
    ensure!(cpu.used_bones.iter().all(|&b|pose.get(b).is_some_and(|t|unit(*t))),"reference pose");
    let mut reference=cpu.bind_vertices.clone();
    let p=Params {pose,scales:&[],palette:&[],transform:Transform {rotation:Quat::IDENTITY,translation:Vec3::ZERO},scale:1.,flat:false,fullbright:false,alpha:1.};
    skinning::kernel(&mut skinning::Job {input:&cpu.vertices,vertices:&mut reference,indices:&cpu.indices},p);
    let bones=cpu.used_bones.clone();
    let packed=cpu.vertices.iter().zip(&reference).map(|(v,r)| {
        // Derive a reference normal from ordered triangle accumulation, then put
        // that normal in EACH influence's local frame. Never use stored SKB normals.
        let n=r.normal.truncate().normalize_or_zero();
        let one=|i:usize|v.weights.get(i).map_or(([0.;4],[0.;4]),|w| {
            let normal=pose[w.bone].rotation.conjugate()*n;
            (w.offset.extend(w.amount).to_array(),normal.extend(bones.binary_search(&w.bone).unwrap() as f32).to_array())
        });
        let (offset0,reference0)=one(0);let(offset1,reference1)=one(1);
        Packed {offset0,reference0,offset1,reference1,uv:v.uv.to_array()}
    }).collect();
    Ok((packed,bones,reference))
}
/// Called only from Alice::load after all clips/textures have loaded, never in gather.
/// Failure disables this source and retains the original CPU publication path.
pub(crate) fn warm(skeleton:&Skeleton,pose:&[Transform]) {
    SOURCE.with(|s|*s.borrow_mut()=None);
    if !enabled() {return;}
    // The pinned backend does not expose failed shader handles for reclamation.
    // Never retry a failed warm at each map load; keep that failure bounded.
    if WARM_FAILED.with(Cell::get) {
        eprintln!("[lg/gpu-skin] CPU fallback warm: previous warm failed in this process");return;
    }
    if !crate::render::vita_renderer::config().actor_single_prepare_active() {
        eprintln!("[lg/gpu-skin] CPU fallback warm: single-preparation prerequisites disabled");return;
    }
    let result=(||->Result<Rc<Source>> {
        let surface=skeleton.surfaces.iter().find(|s|s.name=="material3").context("Alice material3 missing")?;
        let cpu=surface.cpu();let(packed,bones,reference)=pack(&cpu,pose)?;
        let mut bounds=vec![(Vec3::splat(f32::INFINITY),Vec3::splat(f32::NEG_INFINITY));bones.len()];
        for w in cpu.vertices.iter().flat_map(|v|&v.weights).filter(|w|w.amount>0.) {
            let (lo,hi)=&mut bounds[bones.binary_search(&w.bone).unwrap()];*lo=lo.min(w.offset);*hi=hi.max(w.offset);
        }
        let bytes=packed.len()*72+cpu.indices.len()*2;
        ensure!(CACHE_BYTES.with(|n|n.get()+bytes<=CAP) && CACHE_SOURCES.with(|n|n.get()<SOURCES),"static cache/descriptor cap");
        let fragment=crate::lighting::fragment(&crate::environment::fragment(crate::render::FRAGMENT));
        let mut meta=retained::meta();
        meta.uniforms.uniforms.push(UniformDesc::new("SkinRows",UniformType::Float4).array(ROWS));
        meta.uniforms.uniforms.push(UniformDesc::new("SkinArgs",UniformType::Float4));
        let uniform_names:Vec<_>=meta.uniforms.uniforms.iter().map(|u|std::ffi::CString::new(u.name.as_str()).unwrap()).collect();
        crate::frame_profile::shader_creation();
        let _name=crate::shader_cache::Name::new("alice-gpu-skin-two-v1");
        let ctx=unsafe {get_internal_gl()}.quad_context;
        #[cfg(not(target_os="vita"))]
        let forced=std::env::var("LOOKING_GLASS_GPU_SKIN_FAILURE").unwrap_or_default();
        #[cfg(not(target_os="vita"))]
        let vertex=match forced.as_str() {
            "compile"=>"#version 100\ninvalid shader".to_owned(),
            "link"=>VERTEX.replace("varying highp vec2 uv;","varying highp vec3 uv;").replace("uv=texcoord;","uv=vec3(texcoord,0.0);"),
            _=>VERTEX.to_owned(),
        };
        #[cfg(target_os="vita")]
        let vertex=VERTEX;
        let shader=ctx.new_shader(ShaderSource::Glsl {vertex:&vertex,fragment:&fragment},meta).context("GPU skin compile/link")?;
        // The GL backend leaves the newly linked program current. Reflect every
        // affine row, including the highest address, rather than trusting limits.
        let reflected=unsafe {
            let mut program=0;gl::glGetIntegerv(0x8B8D,&mut program);
            program>0 && (0..ROWS).all(|i| {
                let name=std::ffi::CString::new(format!("SkinRows[{i}]")).unwrap();
                gl::glGetUniformLocation(program as u32,name.as_ptr())>=0
            })
        };
        #[cfg(not(target_os="vita"))]
        let reflected=reflected && forced!="reflection";
        if !reflected {ctx.delete_shader(shader);anyhow::bail!("GPU skin palette reflection (rows 0..32)");}
        let uniform_calls=unsafe {
            let mut program=0;gl::glGetIntegerv(0x8B8D,&mut program);
            uniform_names.iter().filter(|name|gl::glGetUniformLocation(program as u32,name.as_ptr())>=0).count() as u64
        };
        let vertex=ctx.new_buffer(BufferType::VertexBuffer,BufferUsage::Immutable,BufferSource::slice(&packed));
        let index=ctx.new_buffer(BufferType::IndexBuffer,BufferUsage::Immutable,BufferSource::slice(&cpu.indices));
        let mut pipelines=BTreeMap::new();
        for blend in 0..crate::render::BLEND_COUNT {for equal in [false,true] {
            crate::frame_profile::pipeline_creation();
            let pipeline=ctx.new_pipeline(&[BufferLayout {stride:72,..Default::default()}],&[
                VertexAttribute::new("offset0",VertexFormat::Float4),VertexAttribute::new("reference0",VertexFormat::Float4),
                VertexAttribute::new("offset1",VertexFormat::Float4),VertexAttribute::new("reference1",VertexFormat::Float4),
                VertexAttribute::new("texcoord",VertexFormat::Float2)],shader,
                PipelineParams {depth_test:if equal {Comparison::Equal}else{Comparison::LessOrEqual},
                    ..crate::render::depth_pipeline(crate::render::blend(blend))});
            pipelines.insert((blend,equal),pipeline);
        }}
        let cpu_bytes=packed.capacity()*72+bones.capacity()*std::mem::size_of::<usize>()+bounds.capacity()*24
            +reference.capacity()*40+cpu.indices.len()*2;
        CACHE_CPU_BYTES.with(|n|n.set(n.get()+cpu_bytes));
        CACHE_BYTES.with(|n|n.set(n.get()+bytes));
        CACHE_SOURCES.with(|n|n.set(n.get()+1));
        println!("[lg/gpu-skin] WARM surface=material3 vertices={} bones={} affine_rows={} reflected_last=32 static_bytes={} vertex_vec4=38 cache_cap={CAP}",packed.len(),bones.len(),ROWS,bytes);
        let source=Rc::new(Source {cpu:cpu.clone(),bones,packed,bounds,reference:Rc::new(prepared::Geometry::new(Mesh {
            vertices:reference,indices:cpu.indices.clone(),texture:None})),shader,pipelines,
            bindings:Bindings {vertex_buffers:vec![vertex],index_buffer:index,images:Vec::new()},bytes,cpu_bytes,uniform_calls});
        warm_draw(&source);
        Ok(source)
    })();
    match result {Ok(source)=>SOURCE.with(|s|*s.borrow_mut()=Some(source)),Err(e)=>{WARM_FAILED.with(|f|f.set(true));eprintln!("[lg/gpu-skin] CPU fallback warm: {e:#}");}}
}
fn warm_draw(source:&Source) {
    let target=render_target(4,4);
    let white=crate::texture::upload_rgba8(1,1,&[255;4]);
    let texture=white.raw_miniquad_id();
    let mut bindings=source.bindings.clone();bindings.images=vec![texture;3];
    let mut uniforms=[0_f32;FLOATS];
    uniforms[16..32].copy_from_slice(&Mat4::orthographic_rh_gl(-100.,100.,-100.,100.,-100.,100.).to_cols_array());
    uniforms[32..34].copy_from_slice(&[1.,1.]);uniforms[34]=1.;uniforms[39..43].fill(1.);uniforms[47]=1.;
    for i in 0..BONES {for axis in 0..3 {uniforms[147+i*12+axis*4+axis]=1.;}}
    uniforms[FLOATS-4..].copy_from_slice(&[1.,1.,0.,0.]);
    let ctx=unsafe {get_internal_gl()}.quad_context;
    ctx.begin_pass(Some(target.render_pass.raw_miniquad_id()),PassAction::clear_color(0.,0.,0.,0.));
    ctx.apply_pipeline(&source.pipelines[&(0,false)]);ctx.apply_bindings(&bindings);
    ctx.apply_uniforms(UniformsSource::table(&uniforms));ctx.draw(0,source.cpu.indices.len() as i32,1);
    ctx.end_render_pass();
    // Flush the driver patch/first submission while still in load, not gather.
    unsafe {gl::glFinish();}
    println!("[lg/gpu-skin] load warm draw complete high_index={}",source.bones.len()-1);
}
fn packet(source:Rc<Source>,p:Params<'_>)->Option<Rc<Packet>> {
    if p.flat || p.alpha!=1. || !p.scale.is_finite() || p.scale<=0. || !unit(p.transform)
        || source.bones.iter().any(|&b| !p.pose.get(b).is_some_and(|t|unit(*t)) || p.scales.get(b).is_some_and(|&s|s!=1.)) {return None;}
    let mut rows=[[0.;4];ROWS];
    let world=skinning::rotation(p.transform.rotation);
    let mut lo=Vec3::splat(f32::INFINITY);let mut hi=Vec3::splat(f32::NEG_INFINITY);
    for (i,&b) in source.bones.iter().enumerate() {
        let r=world*skinning::rotation(p.pose[b].rotation)*p.scale;
        let t=p.transform.point(p.pose[b].translation*p.scale);
        if !r.is_finite() || !t.is_finite() {return None;}
        rows[i*3]=[r.x_axis.x,r.y_axis.x,r.z_axis.x,t.x];
        rows[i*3+1]=[r.x_axis.y,r.y_axis.y,r.z_axis.y,t.y];
        rows[i*3+2]=[r.x_axis.z,r.y_axis.z,r.z_axis.z,t.z];
        let (min,max)=source.bounds[i];
        if min.is_finite() && max.is_finite() {
            let center=r*((min+max)*0.5)+t;let half=(max-min)*0.5;
            let extent=r.x_axis.abs()*half.x+r.y_axis.abs()*half.y+r.z_axis.abs()*half.z;
            lo=lo.min(center-extent);hi=hi.max(center+extent);
        }
    }
    PACKETS.with(|n|n.set(n.get()+1));
    Some(Rc::new(Packet {source,rows,args:[f32::from(p.fullbright), (p.alpha.clamp(0.,1.)*255.) as u8 as f32/255.,0.,0.],center:(lo+hi)*0.5}))
}
/// Called before dispatch. Accepting means NO CPU deformation/publication for
/// this surface. Every rejection returns to the exact pre-existing CPU path.
pub(crate) fn try_capture(surface:&crate::skeletal::Surface,mesh:&Mesh,p:Params<'_>)->bool {
    if !enabled() {return false;}
    let source=SOURCE.with(|s|s.borrow().as_ref().filter(|s|surface.shares_cpu(&s.cpu)).cloned());
    let Some(source)=source else {return false;};
    let started=crate::frame_profile::enabled().then(std::time::Instant::now);
    count("gpu_skin_candidates",1);
    if skinning::mutable_required() || skin_requires_preservation(mesh) {return reject("gpu_skin_reject_consumer");}
    if mesh.indices!=source.cpu.indices || mesh.vertices.len()!=source.packed.len()
        || mesh.vertices.iter().zip(&source.packed).any(|(v,p)|v.uv.to_array().map(f32::to_bits)!=p.uv.map(f32::to_bits)) {return reject("gpu_skin_reject_live_fields");}
    if SKIN_OPACITY.with(Cell::get)!=1. {return reject("gpu_skin_reject_opacity");}
    let material=mesh.texture.as_ref().and_then(|t|SURFACE_LOOKUP.with(|s|s.borrow().get(&t.raw_miniquad_id()).and_then(Weak::upgrade)));
    let Some(material)=material else {return reject("gpu_skin_reject_material");};
    let (time,appearance,depth)=FRAME.with(|f| {let f=f.borrow();(f.time,f.appearance,f.depth_read_only)});
    if appearance!=Appearance::default() || depth {return reject("gpu_skin_reject_appearance");}
    let Some((mut passes,_))=actor_cache::recipe(&material,time,appearance) else {return reject("gpu_skin_reject_layers");};
    let Some(packet)=packet(source.clone(),p) else {return reject("gpu_skin_reject_transform");};
    let n=source.packed.len()*passes.len();
    if FRAME.with(|f|f.borrow().vertices.saturating_add(n)>MAX_VERTICES) {return reject("gpu_skin_reject_budget");}
    count("gpu_skin_eligible_surfaces",1);count("gpu_skin_eligible_layers",passes.len() as u64);
    count("gpu_skin_cpu_vertices_avoided",source.packed.len() as u64);
    count("gpu_skin_palette_packet_bytes",std::mem::size_of::<Packet>() as u64);
    if let Some(t)=started {count("gpu_skin_prepare_us",t.elapsed().as_micros() as u64);}
    for pass in &mut passes {pass.geometry=Some(source.reference.clone());pass.gpu_skin=Some(packet.clone());}
    FRAME.with(|f| {let mut f=f.borrow_mut();f.vertices+=n;f.groups.push(Group {passes,transparent:material.transparent});});
    true
}
impl Packet {
    pub(super) fn draw(&self,pass:&Pass,base:&[f32;147],depth_write:bool) {
        let _owner=crate::render_tail::owner(crate::light_profile::RenderPhase::Actors);
        let started=crate::frame_profile::enabled().then(std::time::Instant::now);
        let mut u=[0.;FLOATS];u[..147].copy_from_slice(base);
        for (i,row) in self.rows.iter().enumerate() {u[147+i*4..151+i*4].copy_from_slice(row);}
        u[FLOATS-4..].copy_from_slice(&self.args);
        let texture=pass.mesh.texture.as_ref().unwrap().raw_miniquad_id();
        let mut bindings=self.source.bindings.clone();bindings.images=vec![texture,texture,texture];
        let ctx=unsafe {get_internal_gl()}.quad_context;
        ctx.apply_pipeline(&self.source.pipelines[&(pass.blend_index,pass.depth_equal)]);
        unsafe {gl::glDepthMask(u8::from(depth_write));}
        ctx.apply_bindings(&bindings);ctx.apply_uniforms(UniformsSource::table(&u));
        ctx.draw(0,self.source.cpu.indices.len() as i32,1);
        count("gpu_skin_draws",1);count("gpu_skin_palette_bytes",(ROWS*16) as u64);
        let cpu_ns=started.map_or(0,|t|t.elapsed().as_nanos() as u64);
        count("gpu_skin_submit_us",cpu_ns/1000);
        crate::light_profile::render_add(crate::light_profile::RenderPhase::Actors,crate::light_profile::RenderMetrics {
            cpu_ns,
            draws:1,pipelines:1,depth_states:1,bindings:1,uniform_uploads:1,uniform_bytes:(FLOATS*4) as u64,
            texture_bind_slots:3,uniform_gl_calls:self.source.uniform_calls,..Default::default()});
    }
}
#[cfg(not(target_os="vita"))]
fn evaluate(v:&Packed,rows:&[[f32;4];ROWS])->(Vec3,Vec3) {
    let transform=|p:[f32;4],index:f32| {
        let i=index as usize*3;vec3(Vec4::from_array(rows[i]).dot(Vec4::from_array(p)),
            Vec4::from_array(rows[i+1]).dot(Vec4::from_array(p)),Vec4::from_array(rows[i+2]).dot(Vec4::from_array(p)))
    };
    let pos=|o:[f32;4],n:[f32;4]|transform([o[0],o[1],o[2],1.],n[3])*o[3];
    let norm=|o:[f32;4],n:[f32;4]|transform([n[0],n[1],n[2],0.],n[3])*o[3];
    (pos(v.offset0,v.reference0)+pos(v.offset1,v.reference1),
     (norm(v.offset0,v.reference0)+norm(v.offset1,v.reference1)).normalize_or_zero())
}

/// Host arithmetic oracle for the actual packed source, not a simulation hash.
#[cfg(not(target_os="vita"))]
pub(crate) fn audit(skeleton:&Skeleton,clips:&[crate::skeletal::Animation])->Result<serde_json::Value> {
    let source=SOURCE.with(|s|s.borrow().clone()).context("GPU source did not warm")?;
    let mut results=Vec::new();
    for (clip_id,clip) in clips.iter().enumerate() {
        let mut max_position=0_f32;let mut max_normal_deg=0_f32;let mut max_rgb=0_u8;
        let mut normals_over=0;let mut rgb_over=0;
        for sample in 0..16 {
            let pose=skeleton.global_pose(&clip.sample(sample as f32*0.137,true));
            let p=Params {pose:&pose,scales:&[],palette:&[],transform:Transform {
                rotation:Quat::from_rotation_z(sample as f32*0.29),translation:vec3(123.,-456.,78.)},
                scale:1.,flat:false,fullbright:false,alpha:1.};
            let packet=packet(source.clone(),p).context("oracle pose rejected")?;
            let mut exact=source.cpu.bind_vertices.clone();
            skinning::kernel(&mut skinning::Job {input:&source.cpu.vertices,vertices:&mut exact,indices:&source.cpu.indices},p);
            for (v,expected) in source.packed.iter().zip(&exact) {
                let (position,n)=evaluate(v,&packet.rows);
                let error=position.distance(expected.position);max_position=max_position.max(error);
                let normal=expected.normal.truncate().normalize_or_zero();
                let angle=if normal==Vec3::ZERO && n==Vec3::ZERO {0.} else {n.dot(normal).clamp(-1.,1.).acos().to_degrees()};
                max_normal_deg=max_normal_deg.max(angle);normals_over+=usize::from(angle>0.5);
                let light=((0.65+0.35*n.dot(vec3(0.3,-0.5,1.).normalize()).max(0.))*255.) as u8;
                let rgb=light.abs_diff(expected.color[0]);max_rgb=max_rgb.max(rgb);rgb_over+=usize::from(rgb>1);
            }
        }
        results.push(serde_json::json!({"clip":crate::character::CLIPS[clip_id],"samples":16,"vertices_per_sample":source.packed.len(),
            "max_position_world":max_position,"max_normal_degrees":max_normal_deg,"max_rgb_bytes":max_rgb,
            "normals_over_0_5_degrees":normals_over,"rgb_over_1_byte":rgb_over,"alpha_exact":true,
            "pass":max_position<=0.001 && max_normal_deg<=0.5 && max_rgb<=1}));
    }
    Ok(serde_json::json!({"surface":"material3","vertices":source.packed.len(),"palette_bones":source.bones,
        "high_local_bone_index":source.bones.len()-1,"packed_stride":std::mem::size_of::<Packed>(),"clips":results,
        "scope":"Host f32 shader-formula oracle versus ordered animated triangle normals; actual GPU images are separate."}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn two_influence_offsets_and_highest_palette_address_are_independent() {
        let v=Packed {offset0:[1.,2.,3.,0.25],reference0:[1.,0.,0.,0.],
            offset1:[-4.,5.,6.,0.75],reference1:[0.,1.,0.,10.],uv:[0.;2]};
        let mut rows=[[0.;4];ROWS];
        for i in 0..BONES {rows[i*3]=[1.,0.,0.,i as f32];rows[i*3+1]=[0.,1.,0.,2.];rows[i*3+2]=[0.,0.,1.,3.];}
        let (position,normal)=evaluate(&v,&rows);
        assert_eq!(position,vec3(1.,2.,3.)*0.25+vec3(6.,7.,9.)*0.75+vec3(0.,2.,3.)*0.25);
        assert!(normal.distance(vec3(0.25,0.75,0.).normalize())<1e-6);
        assert_eq!(std::mem::size_of::<Packed>(),72);
    }
    #[test]
    fn pack_rejects_extra_influences_and_nonunit_pose_without_truncation() {
        let mut surface=crate::skeletal::Surface::new("test".into(),
            [Vec3::ZERO,Vec3::X,Vec3::Y].into_iter().map(|offset|crate::skeletal::SkinVertex {
                uv:Vec2::ZERO,weights:vec![crate::skeletal::Weight {bone:0,amount:1.,offset}]}).collect(),vec![0,1,2]);
        let pose=[Transform {rotation:Quat::IDENTITY,translation:Vec3::ZERO}];
        assert!(pack(&surface,&pose).is_ok());
        assert!(pack(&surface,&[Transform {rotation:Quat::from_xyzw(0.,0.,0.,2.),translation:Vec3::ZERO}]).is_err());
        surface=crate::skeletal::Surface::new("test".into(),
            [Vec3::ZERO,Vec3::X,Vec3::Y].into_iter().map(|offset|crate::skeletal::SkinVertex {
                uv:Vec2::ZERO,weights:(0..3).map(|_|crate::skeletal::Weight {bone:0,amount:1./3.,offset}).collect()}).collect(),vec![0,1,2]);
        assert!(pack(&surface,&pose).is_err());
    }
}

/// Read back a synthetic triangle through the SAME linked shader and layout.
/// All wrong palette slots place it outside clip space. Includes local bone 10,
/// which accesses affine row 32. This diagnostic is absent from native packages.
#[cfg(not(target_os="vita"))]
pub(crate) fn address_check()->Result<serde_json::Value> {
    let source=SOURCE.with(|s|s.borrow().clone()).context("source missing for addressing check")?;
    let target=render_target(32,32);let white=crate::texture::upload_rgba8(1,1,&[255;4]);
    let tex=white.raw_miniquad_id();let mut results=Vec::new();
    for bone in [0,5,10] {
        let triangle=[[-0.75,-0.75,0.],[0.,-0.75,0.],[-0.75,0.,0.]].map(|p|Packed {
            offset0:[p[0],p[1],p[2],1.],reference0:[0.,0.,1.,bone as f32],
            offset1:[0.;4],reference1:[0.,0.,1.,0.],uv:[0.5;2]});
        let ctx=unsafe {get_internal_gl()}.quad_context;
        let vb=ctx.new_buffer(BufferType::VertexBuffer,BufferUsage::Immutable,BufferSource::slice(&triangle));
        let ib=ctx.new_buffer(BufferType::IndexBuffer,BufferUsage::Immutable,BufferSource::slice(&[0_u16,1,2]));
        let bindings=Bindings {vertex_buffers:vec![vb],index_buffer:ib,images:vec![tex;3]};
        let mut u=[0_f32;FLOATS];u[16..32].copy_from_slice(&Mat4::IDENTITY.to_cols_array());
        u[32..34].fill(1.);u[34]=1.;u[35]=1.;u[39..43].fill(1.);u[43]=-1.;u[47]=1.;
        for b in 0..BONES {
            u[147+b*12..159+b*12].copy_from_slice(&[1.,0.,0.,if b==bone {0.75}else{10.},0.,1.,0.,0.,0.,0.,1.,0.]);
        }
        u[FLOATS-4..].copy_from_slice(&[1.,1.,0.,0.]);
        ctx.begin_pass(Some(target.render_pass.raw_miniquad_id()),PassAction::clear_color(0.,0.,0.,0.));
        ctx.apply_pipeline(&source.pipelines[&(0,false)]);ctx.apply_bindings(&bindings);ctx.apply_uniforms(UniformsSource::table(&u));
        ctx.draw(0,3,1);ctx.end_render_pass();
        let image=target.texture.get_texture_data();let mut left=0;let mut right=0;
        for (i,p) in image.bytes.chunks_exact(4).enumerate() {
            if p[0]>128 {if i%32<16 {left+=1;} else {right+=1;}}
        }
        let ctx=unsafe {get_internal_gl()}.quad_context;
        ctx.delete_buffer(vb);ctx.delete_buffer(ib);
        ensure!(left==0 && right>=50,"palette index {bone} addressing: left={left} right={right}");
        results.push(serde_json::json!({"local_index":bone,"last_row":bone*3+2,"unexpected_left_pixels":left,"expected_right_pixels":right}));
    }
    Ok(serde_json::json!({"pass":true,"shader":"same production two-influence shader","cases":results}))
}

impl Drop for Packet {fn drop(&mut self) {PACKETS.with(|n|n.set(n.get()-1));}}
/// Actual cache footprint samples, once per frame (not once per view/layer).
/// CPU arrays exclude shared fallback data, allocator/tree and shader-driver storage.
pub(crate) fn publish_memory() {
    if !crate::frame_profile::enabled() {return;}
    count("gpu_skin_warm_disabled",u64::from(WARM_FAILED.with(Cell::get)));
    count("gpu_skin_static_bytes",CACHE_BYTES.with(Cell::get) as u64);
    count("gpu_skin_cpu_source_bytes",CACHE_CPU_BYTES.with(Cell::get) as u64);
    count("gpu_skin_pipeline_objects",(CACHE_SOURCES.with(Cell::get)*crate::render::BLEND_COUNT*2) as u64);
    count("gpu_skin_live_palette_bytes",(PACKETS.with(Cell::get)*std::mem::size_of::<Packet>()) as u64);
}
