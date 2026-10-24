//! Opt-in host corpus recorder. No code from this module enters a Vita build.
use super::*;
use std::cell::{Cell,RefCell};
thread_local! {
    static ACTIVE:Cell<bool>=const{Cell::new(false)};
    static OWNER:Cell<usize>=const{Cell::new(4)};
    static QUERIES:RefCell<Vec<serde_json::Value>>=RefCell::default();
}
fn enabled()->bool {std::env::var_os("LOOKING_GLASS_SAH_CORPUS").is_some()}
pub(crate) fn owner(owner:usize)->usize {OWNER.with(|v|v.replace(owner))}
pub(crate) fn begin(){if enabled(){ACTIVE.with(|v|v.set(true));QUERIES.with(|v|v.borrow_mut().clear());}}
pub(crate) fn end(frame:u64,map:&str){
    if !ACTIVE.with(Cell::get){return;}
    ACTIVE.with(|v|v.set(false));
    use std::io::Write;
    let mut out=std::fs::OpenOptions::new().create(true).append(true).open("sah-corpus.jsonl").unwrap();
    QUERIES.with(|v|writeln!(out,"{}",serde_json::json!({"frame":frame,"map":map,"queries":*v.borrow()})).unwrap());
}
pub(super) fn record(g:&StaticGeometry,a:Vec3,b:Vec3,h:Vec3,t:Trace,c:sah::Counts) {
    if !ACTIVE.with(Cell::get){return;}
    QUERIES.with(|v|v.borrow_mut().push(serde_json::json!({"owner":OWNER.with(Cell::get),"hulls":g.hulls.len(),"a":a.to_array().map(f32::to_bits),"b":b.to_array().map(f32::to_bits),"half":h.to_array().map(f32::to_bits),"trace":sah::bits(t),"counts":c})));
}
