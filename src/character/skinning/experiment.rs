//! Default-off exact numeric experiments; no render handles cross worker boundaries.
use super::*;
use std::sync::{Mutex, atomic::{AtomicU64, Ordering}};
static FRAME: AtomicU64 = AtomicU64::new(0);
pub(super) fn begin() { FRAME.fetch_add(1, Ordering::Relaxed); }
pub(super) fn reset() { CACHE.lock().unwrap().clear(); }
pub(super) fn flags() -> (bool, bool, bool) {
    #[cfg(feature="vita_renderer")]
    { let c=crate::render::vita_renderer::config(); (c.skin_vector, c.skin_pose_reuse, c.skin_duplicate_reuse) }
    #[cfg(not(feature="vita_renderer"))]
    { (false,false,false) }
}
struct Entry {
    // Strong ownership prevents an unloaded surface's address being recycled.
    surface: std::sync::Arc<crate::skeletal::SkinSurface>,
    actor: Option<super::identity::ActorId>,
    frame: u64,
    key: Vec<u32>,
    output: Vec<(Vec3, Vec4, [u8;4])>,
}
static CACHE: Mutex<Vec<Entry>> = Mutex::new(Vec::new());
#[inline]
fn blend(a:Vec4,b:Vec4,c:Vec4,t:Vec4,s:[f32;4])->Vec4 {
    #[cfg(not(target_arch="arm"))]
    { ((a*s[0]+b*s[1])+c*s[2]+t)*s[3] }
    #[cfg(target_arch="arm")]
    { unsafe { neon_blend(a,b,c,t,s) } }
}
/// Explicit non-fused arithmetic: LLVM otherwise scalarizes the three live lanes
/// even inside a +neon function. The unused fourth lane never enters output.
#[cfg(target_arch="arm")]
#[target_feature(enable="neon")]
unsafe fn neon_blend(a:Vec4,b:Vec4,c:Vec4,t:Vec4,s:[f32;4])->Vec4 {
    let vectors=[a.to_array(),b.to_array(),c.to_array(),t.to_array()];
    let mut output=[0.;4];
    core::arch::asm!(
        "vld1.32 {{q0, q1}}, [{vectors}]!",
        "vld1.32 {{q2, q3}}, [{vectors}]",
        "vld1.32 {{q4}}, [{scalars}]",
        "vdup.32 q5, d8[0]", "vmul.f32 q0, q0, q5",
        "vdup.32 q5, d8[1]", "vmul.f32 q1, q1, q5",
        "vadd.f32 q0, q0, q1",
        "vdup.32 q5, d9[0]", "vmul.f32 q2, q2, q5",
        "vadd.f32 q0, q0, q2", "vadd.f32 q0, q0, q3",
        "vdup.32 q5, d9[1]", "vmul.f32 q0, q0, q5",
        "vst1.32 {{q0}}, [{output}]",
        vectors=inout(reg) vectors.as_ptr()=>_, scalars=in(reg) s.as_ptr(), output=in(reg) output.as_mut_ptr(),
        out("q0") _,out("q1") _,out("q2") _,out("q3") _,out("q4") _,out("q5") _,
        options(nostack,preserves_flags));
    Vec4::from_array(output)
}
fn key(p: Params<'_>) -> Vec<u32> {
    let mut k=Vec::with_capacity(p.pose.len()*7+p.palette.len()*12+p.scales.len()+16);
    k.extend([p.pose.len() as u32,p.scales.len() as u32,p.palette.len() as u32]);
    for b in p.pose { k.extend(b.translation.to_array().map(f32::to_bits)); k.extend(b.rotation.to_array().map(f32::to_bits)); }
    k.extend(p.scales.iter().map(|v|v.to_bits()));
    for (r,t) in p.palette {k.extend(r.to_cols_array().map(f32::to_bits));k.extend(t.to_array().map(f32::to_bits));}
    k.extend(p.transform.translation.to_array().map(f32::to_bits));
    k.extend(p.transform.rotation.to_array().map(f32::to_bits));
    k.extend([p.scale.to_bits(),p.alpha.to_bits(),p.flat as u32,p.fullbright as u32]); k
}
/// Cache only the fields the numeric kernel writes. UVs and caller-owned fields stay live.
pub(super) fn cached(surface: &std::sync::Arc<crate::skeletal::SkinSurface>, actor: Option<super::identity::ActorId>,
    output: &mut [Vertex], p: Params<'_>, compute: impl FnOnce(&mut [Vertex])) -> bool {
    let (_,reuse,duplicate)=flags();
    cache_with_flags(surface,actor,output,p,reuse,duplicate,compute)
}
fn cache_with_flags(surface:&std::sync::Arc<crate::skeletal::SkinSurface>,actor:Option<super::identity::ActorId>,
    output:&mut [Vertex],p:Params<'_>,reuse:bool,duplicate:bool,compute:impl FnOnce(&mut [Vertex]))->bool {
    if !reuse && !duplicate { compute(output); return false; }
    let frame=FRAME.load(Ordering::Relaxed); let key=key(p);
    {
        let cache=CACHE.lock().unwrap();
        if let Some(e)=cache.iter().find(|e|std::sync::Arc::ptr_eq(&e.surface,surface) && e.key==key
            && e.output.len()==output.len() && ((reuse && e.actor==actor && e.frame<frame)
                || (duplicate && e.frame==frame))) {
            for (v,&(position,normal,color)) in output.iter_mut().zip(&e.output) {v.position=position;v.normal=normal;v.color=color;}
            return true;
        }
    }
    compute(output);
    let mut cache=CACHE.lock().unwrap();
    // Bounded to 128 surfaces; old entries cannot outlive scene replacement.
    if cache.len()>=128 {let oldest=cache.iter().enumerate().min_by_key(|(_,e)|e.frame).unwrap().0;cache.swap_remove(oldest);}
    cache.push(Entry {surface:surface.clone(),actor,frame,key,
        output:output.iter().map(|v|(v.position,v.normal,v.color)).collect()});
    false
}
/// Four independent lanes, with the reference's multiplication/addition order.
/// The ARM implementation enables NEON locally; the release crate stays -neon.
#[cfg_attr(target_arch="arm", target_feature(enable="neon"))]
#[inline]
pub(super) unsafe fn vector_position(src: &SkinVertex, palette: &[(Mat3,Vec3)]) -> Vec3 {
    let mut sum=Vec4::ZERO;
    for w in &src.weights {
        let (r,t)=palette[w.bone];
        let v=blend(r.x_axis.extend(0.),r.y_axis.extend(0.),r.z_axis.extend(0.),t.extend(0.),
            [w.offset.x,w.offset.y,w.offset.z,w.amount]);
        sum+=v;
    }
    sum.truncate()
}

#[cfg_attr(target_arch="arm", target_feature(enable="neon"))]
#[inline]
pub(super) unsafe fn vector_pose(src:&SkinVertex,pose:&[Transform],scales:&[f32])->Vec3 {
    let mut sum=Vec4::ZERO;
    for w in &src.weights {
        let t=pose[w.bone];
        let v=if scales.is_empty(){w.offset}else{w.offset*scales[w.bone]};
        let b=Vec3::new(t.rotation.x,t.rotation.y,t.rotation.z);
        // Keep scalar reductions and the reference's exact association.
        sum+=blend(v.extend(0.),b.extend(0.),b.cross(v).extend(0.),t.translation.extend(0.),
            [t.rotation.w*t.rotation.w-b.dot(b),v.dot(b)*2.,t.rotation.w*2.,w.amount]);
    }
    sum.truncate()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_checks_all_pose_bits_keeps_live_uv_and_invalidates_on_scene_reset() {
        reset();
        let surface=std::sync::Arc::new(crate::skeletal::SkinSurface {vertices:vec![],indices:vec![],bind_vertices:vec![],
            #[cfg(feature="vita_renderer")] used_bones:vec![]});
        let pose=[Transform {translation:Vec3::ZERO,rotation:Quat::IDENTITY}];
        let p=Params {pose:&pose,scales:&[],palette:&[],transform:pose[0],scale:1.,flat:false,fullbright:false,alpha:1.};
        let actor=Some(super::super::identity::ActorId::default());
        let mut output=[Vertex {position:Vec3::ZERO,normal:Vec4::ZERO,color:[0;4],uv:Vec2::ONE}];
        let fill=|v:&mut [Vertex]|{v[0].position=Vec3::X;v[0].normal=Vec4::Y;v[0].color=[3;4];};
        assert!(!cache_with_flags(&surface,actor,&mut output,p,false,true,fill));
        output[0].position=Vec3::ZERO;output[0].uv=Vec2::splat(7.);
        assert!(cache_with_flags(&surface,None,&mut output,p,false,true,|_|panic!("duplicate recomputed")));
        assert_eq!(output[0].position,Vec3::X);assert_eq!(output[0].uv,Vec2::splat(7.));
        begin();
        assert!(cache_with_flags(&surface,actor,&mut output,p,true,false,|_|panic!("pose recomputed")));
        let changed=[Transform {translation:vec3(-0.,0.,0.),..pose[0]}];
        assert!(!cache_with_flags(&surface,actor,&mut output,Params {pose:&changed,..p},true,false,fill));
        assert_ne!(key(p),key(Params {alpha:-0.,..p}));
        assert_ne!(key(p),key(Params {scale:-0.,..p}));
        assert_ne!(key(p),key(Params {flat:true,..p}));
        assert_ne!(key(p),key(Params {fullbright:true,..p}));
        reset();
        assert!(!cache_with_flags(&surface,actor,&mut output,p,true,true,fill));
        reset();
    }
    #[test]
    fn vector_kernel_keeps_reference_bits_for_non_unit_quaternions_and_scales() {
        let pose:Vec<_>=(0..19).map(|i|Transform {rotation:Quat::from_xyzw(i as f32*0.03,-0.21,0.6,0.77),
            translation:vec3(i as f32,-2.3,0.71)}).collect();
        let scales:Vec<_>=(0..19).map(|i|0.7+i as f32*0.02).collect();
        let palette=super::super::palette(&pose,&scales);
        for i in 0..500 {
            let src=SkinVertex {uv:Vec2::ZERO,weights:(0..4).map(|j|crate::skeletal::Weight {
                bone:(i+j)%19,amount:0.13+j as f32*0.1,offset:vec3(i as f32*0.07,j as f32-2.,-0.31)}).collect()};
            let bits=|v:Vec3|v.to_array().map(f32::to_bits);
            assert_eq!(bits(unsafe{vector_position(&src,&palette)}),bits(position(&src,&palette)));
            for scales in [&[][..],&scales[..]] {
                let reference=if scales.is_empty(){src.position(&pose)}else{
                    src.weights.iter().map(|w|pose[w.bone].point(w.offset*scales[w.bone])*w.amount).sum()};
                assert_eq!(bits(unsafe{vector_pose(&src,&pose,scales)}),bits(reference));
            }
        }
    }
}
