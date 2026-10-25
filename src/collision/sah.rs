//! Ordered static SAH. See tools/sah_r8/CONTRACT.md for the pruning proof.
use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static BUILDS: AtomicU64 = AtomicU64::new(0);
pub(super) fn builds() -> u64 { BUILDS.load(Ordering::Relaxed) }
const LEAF: u32 = 1 << 31;
const NONE: u32 = u32::MAX;
const CANDIDATES: usize = 256;
const CERT_CACHE: usize = 128;
#[derive(Default,Clone,Copy,Debug,serde::Serialize)]
pub(super) struct Counts {
    pub nodes:u64, pub tested:u64, pub clipped:u64, pub planes:u64,
    pub certificates:u64, pub fallbacks:u64,
}
#[derive(Debug,Clone,Copy)]
#[repr(C)]
struct SpatialNode {bounds:Bounds,first:u32,second:u32}
#[derive(Debug,Clone,Copy)]
#[repr(C)]
struct Certificate {bounds:Bounds,parent:u32}
#[derive(Debug,Clone,Copy)]
#[repr(C)]
struct Original {hull:u32,leaf:u32}
#[derive(Debug)]
pub(super) struct Hierarchy {
    nodes:Box<[SpatialNode]>, indices:Box<[u32]>,
    legacy:Box<[Certificate]>, order:Box<[Original]>,
    finite:bool,
}
impl Hierarchy {
    pub fn build(hulls:&[Hull],tree:&Node)->Self {
        BUILDS.fetch_add(1, Ordering::Relaxed);
        let start=std::time::Instant::now();
        assert!(hulls.len()<(LEAF as usize)/2);
        let mut legacy=Vec::new();let mut order=Vec::with_capacity(hulls.len());
        fn certificate(n:&Node,parent:u32,nodes:&mut Vec<Certificate>,order:&mut Vec<Original>) {
            let id=nodes.len() as u32;
            let bounds=match n {Node::Leaf{bounds,..}|Node::Branch{bounds,..}=>*bounds};
            nodes.push(Certificate{bounds,parent});
            match n {
                Node::Leaf{hulls,..}=>order.extend(hulls.iter().map(|&h|Original{hull:h as u32,leaf:id})),
                Node::Branch{left,right,..}=>{certificate(left,id,nodes,order);certificate(right,id,nodes,order);}
            }
        }
        certificate(tree,NONE,&mut legacy,&mut order);
        let mut ranks:Vec<u32>=(0..order.len() as u32).collect();
        let mut nodes=Vec::new();let mut indices=Vec::with_capacity(hulls.len());
        Self::split(hulls,&order,&mut ranks,&mut nodes,&mut indices,0);
        let finite=hulls.iter().all(|h|h.bounds.min.is_finite()&&h.bounds.max.is_finite()&&h.bounds.min.cmple(h.bounds.max).all());
        let out=Self{nodes:nodes.into_boxed_slice(),indices:indices.into_boxed_slice(),legacy:legacy.into_boxed_slice(),order:order.into_boxed_slice(),finite};
        drop(ranks);
        let bytes=out.storage_bytes();
        // One load-time record, outside the measured frame window.
        println!("COLLISION_SAH_LOAD hulls={} nodes={} certificates={} bytes={} build_us={}",hulls.len(),out.nodes.len(),out.legacy.len(),bytes,start.elapsed().as_micros());
        out
    }
    pub fn storage_bytes(&self)->usize {
        self.nodes.len()*std::mem::size_of::<SpatialNode>()+self.indices.len()*4+
        self.legacy.len()*std::mem::size_of::<Certificate>()+self.order.len()*8
    }
    fn split(hulls:&[Hull],order:&[Original],ranks:&mut[u32],nodes:&mut Vec<SpatialNode>,indices:&mut Vec<u32>,depth:usize)->u32 {
        let bound=|rank:u32|hulls[order[rank as usize].hull as usize].bounds;
        let bounds=ranks.iter().map(|&r|bound(r)).reduce(Bounds::union).unwrap();
        let id=nodes.len() as u32;nodes.push(SpatialNode{bounds,first:0,second:0});
        if ranks.len()<=8 || depth==48 {
            nodes[id as usize].first=indices.len() as u32;nodes[id as usize].second=LEAF|ranks.len() as u32;
            indices.extend_from_slice(ranks);return id;
        }
        let area=|b:Bounds| {let d=(b.max.as_dvec3()-b.min.as_dvec3()).max(glam::DVec3::ZERO);2.*(d.x*d.y+d.x*d.z+d.y*d.z)};
        let center=|r:u32,a:usize| {let b=bound(r);(b.min[a] as f64+b.max[a] as f64)*0.5};
        let mut best=(f64::INFINITY,0usize,0usize,0f64,1f64);
        for axis in 0..3 {
            let mn=ranks.iter().map(|&r|center(r,axis)).fold(f64::INFINITY,f64::min);
            let mx=ranks.iter().map(|&r|center(r,axis)).fold(f64::NEG_INFINITY,f64::max);
            if !mn.is_finite()||!mx.is_finite()||mx<=mn {continue;}
            let bin=|r|(((center(r,axis)-mn)/(mx-mn)*16.) as usize).min(15);
            let mut bs=[None::<Bounds>;16];let mut ns=[0usize;16];
            for &r in ranks.iter(){let i=bin(r);ns[i]+=1;bs[i]=Some(bs[i].map_or(bound(r),|a|a.union(bound(r))));}
            for cut in 1..16 {
                let nl:usize=ns[..cut].iter().sum();let nr=ranks.len()-nl;if nl==0||nr==0{continue;}
                let a=bs[..cut].iter().flatten().copied().reduce(Bounds::union).unwrap();
                let b=bs[cut..].iter().flatten().copied().reduce(Bounds::union).unwrap();
                let cost=area(a)*nl as f64+area(b)*nr as f64;
                if cost<best.0 {best=(cost,axis,cut,mn,mx);}
            }
        }
        let mut middle=0;
        if best.0.is_finite() {
            for i in 0..ranks.len() {
                let bin=(((center(ranks[i],best.1)-best.3)/(best.4-best.3)*16.)as usize).min(15);
                if bin<best.2 {ranks.swap(i,middle);middle+=1;}
            }
        }
        if middle==0||middle==ranks.len(){middle=ranks.len()/2;}
        let (a,b)=ranks.split_at_mut(middle);
        let left=Self::split(hulls,order,a,nodes,indices,depth+1);
        let right=Self::split(hulls,order,b,nodes,indices,depth+1);
        nodes[id as usize].first=left;nodes[id as usize].second=right;id
    }
    fn gather(&self,id:u32,g:&StaticGeometry,q:&SweepBounds,out:&mut[u32;CANDIDATES],len:&mut usize,c:&mut Counts)->bool {
        c.nodes+=1;let n=&self.nodes[id as usize];if !q.overlaps(n.bounds){return true;}
        if n.second&LEAF!=0 {
            for &rank in &self.indices[n.first as usize..n.first as usize+(n.second&!LEAF)as usize] {
                c.tested+=1;
                if q.overlaps(g.hulls[self.order[rank as usize].hull as usize].bounds) {
                    if *len==out.len(){return false;}out[*len]=rank;*len+=1;
                }
            }true
        }else{self.gather(n.first,g,q,out,len,c)&&self.gather(n.second,g,q,out,len,c)}
    }
    fn admitted(&self,mut id:u32,q:&SweepBounds,cache:&mut[u32;CERT_CACHE],c:&mut Counts)->bool {
        // Direct-mapped pure-predicate cache, local to this query and thread.
        // Cache collisions only repeat work. Never infer acceptance from a hash.
        let mut path=[0u32;64];let mut len=0;let mut accepted=true;
        while id!=NONE {
            let entry=cache[id as usize&(CERT_CACHE-1)];
            if entry&!LEAF==id {accepted=entry&LEAF==0;break;}
            let n=&self.legacy[id as usize];c.certificates+=1;
            path[len]=id;len+=1;
            if !q.overlaps(n.bounds){accepted=false;break;}id=n.parent;
        }
        // A rejection certifies every descendant on this path as rejected too.
        for &id in &path[..len] {cache[id as usize&(CERT_CACHE-1)]=id|if accepted{0}else{LEAF};}
        accepted
    }
    pub fn trace(&self,g:&StaticGeometry,a:Vec3,b:Vec3,h:Vec3,q:&SweepBounds,c:&mut Counts)->Option<Trace> {
        // Non-finite inverse includes tiny nonzero delta whose reciprocal overflows.
        // Delta overflow can produce inverse zero; require finite endpoints/delta too.
        if !self.finite||!a.is_finite()||!b.is_finite()||!h.is_finite()||h.cmplt(Vec3::ZERO).any()||!(b-a).is_finite()||
            !q.inverse.is_finite()||!q.padding.is_finite()||!q.bounds.min.is_finite()||!q.bounds.max.is_finite(){return None;}
        let mut candidates=[0u32;CANDIDATES];let mut len=0;
        if !self.gather(0,g,q,&mut candidates,&mut len,c){return None;}
        candidates[..len].sort_unstable();
        let mut cache=[NONE;CERT_CACHE];let mut result=Trace::default();
        for &rank in &candidates[..len] {
            let original=self.order[rank as usize];let hull=&g.hulls[original.hull as usize];
            c.clipped+=1;let mut proposed=result;
            clip_hull_counted(hull,a,b,h,&mut proposed,c);
            if bits(proposed)!=bits(result)&&self.admitted(original.leaf,q,&mut cache,c){result=proposed;}
        }
        Some(result)
    }
}
pub(super) fn bits(t:Trace)->[u32;6] {
    [t.fraction.to_bits(),t.normal.x.to_bits(),t.normal.y.to_bits(),t.normal.z.to_bits(),t.start_solid as u32,t.all_solid as u32]
}
#[cfg(test)]
mod tests;
