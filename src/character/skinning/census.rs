//! Host-only, opt-in actor inventory. Never changes renderer policy.
use super::*;
pub(crate) fn active()->bool {
    std::env::var_os("LOOKING_GLASS_ACTOR_CENSUS").is_some()
        && window().contains(&crate::sim_maps::probe::frame())
}
/// Override the inventory window without changing the production renderer.
fn window() -> std::ops::RangeInclusive<u64> {
    static WINDOW: std::sync::OnceLock<(u64,u64)> = std::sync::OnceLock::new();
    let &(first,last)=WINDOW.get_or_init(|| {
        std::env::var("LOOKING_GLASS_ACTOR_CENSUS_WINDOW").ok()
            .map(|v| {let (a,b)=v.split_once(':').expect("census window FIRST:LAST");
                let a=a.parse().expect("census first frame");let b=b.parse().expect("census last frame");
                assert!(a<=b,"ordered census window");(a,b)})
            .unwrap_or((300,360))
    });
    first..=last
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

/// Host-only scoped CPU measurements. Overhead is included; never a Vita clock.
pub(crate) struct Leaf(Option<(std::time::Instant, &'static str)>);
pub(crate) fn leaf(name: &'static str) -> Leaf {
    Leaf(active().then(|| (std::time::Instant::now(),name)))
}
impl Drop for Leaf { fn drop(&mut self) {
    if let Some((start,name))=self.0 {
        println!("[lg/actor-leaf] frame={} leaf={name} cpu_ns={}",
            crate::sim_maps::probe::frame(),start.elapsed().as_nanos());
    }
}}
