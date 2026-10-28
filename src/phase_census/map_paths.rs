//! Schema-6 map-path counters. All scratch is bounded and initialized before arm.
use super::*;
const EXACT_TABLE:usize=1024;
#[derive(Clone,Copy,PartialEq,Eq)]
pub(super) struct ExactEntry {
    frame:u64,epoch:u64,world:usize,geometry:usize,bits:[u32;9],mask:u32,ignore:Option<usize>,
}
impl ExactEntry {pub(super) const EMPTY:Self=Self{frame:0,epoch:0,world:0,geometry:0,bits:[0;9],mask:0,ignore:None};}
pub(super) fn parent_bit(row:&mut Row,region:usize,parent:usize) {
    if parent<64 {row.parents[region]|=1u64<<parent;}
    else if parent<128 {row.parents_hi[region]|=1u64<<(parent-64);}
    else {row.errors+=1;}
}
#[derive(Clone,Copy)]
#[repr(usize)]
pub(crate) enum QueryCategory {GravityRecoil,Navigation,Sight,Spawn,Other}
pub(crate) struct QueryScope(Option<usize>);
pub(crate) fn query_category(category:QueryCategory)->QueryScope {
    QueryScope(active().then(||STATE.with(|s|{let mut s=s.borrow_mut();std::mem::replace(&mut s.category,category as usize)})))
}
impl Drop for QueryScope {fn drop(&mut self){if let Some(old)=self.0 {STATE.with(|s|s.borrow_mut().category=old);}}}
pub(crate) struct ControllerScope(Option<usize>);
pub(crate) fn npc_controller(levels:bool)->ControllerScope {
    ControllerScope(active().then(||STATE.with(|s|{let mut s=s.borrow_mut();std::mem::replace(&mut s.controller,usize::from(levels))})))
}
impl Drop for ControllerScope {fn drop(&mut self){if let Some(old)=self.0 {STATE.with(|s|s.borrow_mut().controller=old);}}}
/// Deliberately conservative: any solid publication invalidates every world's
/// opportunities, even if its bits are unchanged. Does not change a game cache.
pub(crate) fn solid_publication() {
    if active(){STATE.with(|s|{let mut s=s.borrow_mut();s.epoch=s.epoch.wrapping_add(1);
        if s.epoch==0 {s.current.errors+=1;}
    });}
}
impl State {
    fn exact_query(&mut self,mut key:ExactEntry) {
        if self.owner as usize!=Owner::Npc as usize {return;}
        key.frame=self.sequence;key.epoch=self.epoch;
        let bucket=self.controller*5+self.category;
        self.current.npc_queries[bucket][0]+=1;
        let mut hash=(key.world as u64)^(key.geometry as u64).rotate_left(19)^key.epoch;
        for word in key.bits.into_iter().chain([key.mask]) {hash=(hash^u64::from(word)).wrapping_mul(0x100000001b3);}
        hash^=key.ignore.map_or(0,|v|(v as u64).wrapping_add(1));
        let first=hash as usize&(EXACT_TABLE-1);
        let mut slot=first;let mut vacant=false;
        for probe in 0..PROBES {
            let i=(first+probe)&(EXACT_TABLE-1);let e=self.exact[i];
            if e==key {self.current.npc_queries[bucket][10]+=1;return;}
            if !vacant && (e.frame!=key.frame || e.epoch!=key.epoch) {slot=i;vacant=true;}
        }
        if !vacant {self.current.npc_queries[bucket][11]+=1;}
        self.exact[slot]=key;
    }
}
pub(crate) fn query_key(world:&crate::collision::World,a:glam::Vec3,b:glam::Vec3,half:glam::Vec3,mask:u32,ignore:Option<usize>) {
    if !active(){return;}
    STATE.with(|s|{let mut s=s.borrow_mut();if s.sweep_depth!=0 {return;}
        s.exact_query(ExactEntry{world:world as *const _ as usize,geometry:world.acoustic_scene_key(),
            bits:[a.x.to_bits(),a.y.to_bits(),a.z.to_bits(),b.x.to_bits(),b.y.to_bits(),b.z.to_bits(),half.x.to_bits(),half.y.to_bits(),half.z.to_bits()],
            mask,ignore,..ExactEntry::EMPTY});
    });
}
/// The existing sweep timer remains the only clock pair for each query.
pub(crate) fn query_sweep(world:&crate::collision::World,a:glam::Vec3,b:glam::Vec3,half:glam::Vec3,mask:u32,ignore:Option<usize>)->Sweep {
    query_key(world,a,b,half,mask,ignore);sweep()
}
pub(crate) fn path_count(counter:Counter,value:u64) {count(counter as usize,value);}
pub(crate) fn count_under(parent:Region,counter:Counter,value:u64) {
    if active(){STATE.with(|s|{let mut s=s.borrow_mut();
        if s.stack[..s.depth].contains(&(parent as usize)) {s.current.counters[counter as usize]+=value;}
    });}
}

#[cfg(test)]
mod tests {
use super::*;
#[test]
fn r15_repeat_key_bits_mask_world_epoch_and_eviction_are_conservative() {
    let mut s=State::boxed();s.sequence=1;s.owner=Owner::Npc;
    let key=ExactEntry{world:1,..ExactEntry::EMPTY};
    s.exact_query(key);s.exact_query(key);assert_eq!(s.current.npc_queries[14][10],1);
    for kind in 0..5 {let mut k=key;match kind {0=>k.bits[0]=0x80000000,1=>k.bits[6]=1,2=>k.mask=2,3=>k.ignore=Some(0),_=>k.world=2} s.exact_query(k);}
    assert_eq!(s.current.npc_queries[14][10],1);
    s.epoch+=1;s.exact_query(key);assert_eq!(s.current.npc_queries[14][10],1);
    s.sequence+=1;s.exact_query(key);assert_eq!(s.current.npc_queries[14][10],1);
    for i in 0..5000 {let mut k=key;k.bits[1]=i;s.exact_query(k);}
    assert!(s.current.npc_queries[14][11]>0);
}

}
