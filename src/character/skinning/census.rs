//! Host-only, opt-in actor inventory. Never changes renderer policy.
use super::*;
pub(crate) fn active()->bool {
    std::env::var_os("LOOKING_GLASS_ACTOR_CENSUS").is_some()
        && {
            static FRAMES: std::sync::OnceLock<Option<Vec<u64>>> = std::sync::OnceLock::new();
            let frames=FRAMES.get_or_init(||std::env::var("LOOKING_GLASS_ACTOR_CENSUS_FRAMES").ok()
                .map(|s|s.split(',').map(|n|n.parse().expect("census frame number")).collect()));
            frames.as_ref().map_or_else(||(300..=360).contains(&crate::sim_maps::probe::frame()),
                |f|f.contains(&crate::sim_maps::probe::frame()))
        }
}
use std::cell::{Cell,RefCell};
use std::collections::HashMap;
thread_local! {
    static CURRENT:Cell<Option<(u64,u64)>>=const {Cell::new(None)};
    static NEXT:Cell<(u64,u64)>=const {Cell::new((0,0))};
    static GROUPS:RefCell<HashMap<usize,Option<(u64,u64)>>>=RefCell::default();
}
pub(crate) fn current()->Option<(u64,u64)> {CURRENT.with(Cell::get)}
pub(crate) fn register(group:usize) {if active() {GROUPS.with(|g|g.borrow_mut().insert(group,current()));}}
pub(crate) fn owner(group:usize)->Option<(u64,u64)> {GROUPS.with(|g|g.borrow().get(&group).copied().flatten())}
pub(crate) fn clear() {GROUPS.with(|g|g.borrow_mut().clear());}
pub(crate) struct Actor(Option<(std::time::Instant,u64,u64,Option<(u64,u64)>)>);
pub(crate) fn actor(_fallback:identity::ActorId,path:&str,model:&str,bones:usize,
    attachments:&impl std::fmt::Debug,transform:Transform,scale:f32,puppet:bool,afterimages:bool)->Actor {
    if !active() {return Actor(None);}
    let id=identity::current().unwrap_or(_fallback).census_id();
    let frame=crate::sim_maps::probe::frame();
    let ordinal=NEXT.with(|n| {let (f,i)=n.get();let i=if f==frame{i+1}else{0};n.set((frame,i));i});
    let previous=CURRENT.with(|c|c.replace(Some((id,ordinal))));
    println!("[lg/pose-census] pose={ordinal} frame={} identity={id} path={path} model={model} bones={bones} attachments={attachments:?} transform={transform:?} scale={scale} puppet={puppet} afterimages={afterimages}",crate::sim_maps::probe::frame());
    Actor(Some((std::time::Instant::now(),id,ordinal,previous)))
}
impl Drop for Actor {fn drop(&mut self) {if let Some((t,id,ordinal,previous))=self.0 {
    CURRENT.with(|c|c.set(previous));
    println!("[lg/pose-cpu] pose={ordinal} frame={} identity={id} cpu_ns={}",crate::sim_maps::probe::frame(),t.elapsed().as_nanos());
}}}
