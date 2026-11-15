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
        let v=((r.x_axis.extend(0.)*w.offset.x+r.y_axis.extend(0.)*w.offset.y)
            +r.z_axis.extend(0.)*w.offset.z+t.extend(0.))*w.amount;
        sum+=v;
    }
    sum.truncate()
}
