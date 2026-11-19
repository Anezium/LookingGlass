//! Buffered actor CPU service times, not additive frame wall times with workers.
use std::{cell::Cell, sync::Mutex, time::Instant, fmt::Write};
#[derive(Default)]
struct Row { kind: usize, bones: usize, calls: u64, vertices: u64, hits: u64, ns: u64 }
static ROWS: Mutex<Vec<Row>> = Mutex::new(Vec::new());
static KINDS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static LEAVES: Mutex<[u64;6]> = Mutex::new([0;6]);
thread_local! {static KIND:Cell<usize>=const {Cell::new(0)};}
pub(crate) fn enabled()->bool {
    #[cfg(feature="vita_renderer")]
    {crate::render::vita_renderer::config().diag_actor_timing && crate::spike_log::on()}
    #[cfg(not(feature="vita_renderer"))]
    {false}
}
pub(crate) struct Kind(usize);
pub(crate) fn kind(name:&str)->Kind {
    if !enabled() {return Kind(0);}
    let mut kinds=KINDS.lock().unwrap();
    let id=if let Some(i)=kinds.iter().position(|n|n==name) {i+1}
        else {kinds.push(name.to_owned());kinds.len()};
    Kind(KIND.with(|k|k.replace(id)))
}
impl Drop for Kind {fn drop(&mut self) {if enabled(){KIND.with(|k|k.set(self.0));}}}
pub(crate) fn current()->usize {if enabled(){KIND.with(Cell::get)}else{0}}
pub(crate) fn start()->Option<Instant> {enabled().then(Instant::now)}
pub(crate) fn skin(start:Option<Instant>,kind:usize,bones:usize,vertices:usize,hit:bool) {
    if let Some(start)=start {
        let ns=start.elapsed().as_nanos() as u64;
        let mut rows=ROWS.lock().unwrap();
        let index=rows.iter().position(|r|r.kind==kind && r.bones==bones).unwrap_or_else(|| {
            rows.push(Row{kind,bones,..Row::default()});rows.len()-1});
        let r=&mut rows[index];r.calls+=1;r.vertices+=vertices as u64;r.hits+=u64::from(hit);r.ns+=ns;
    }
}
pub(crate) struct Leaf(Option<Instant>,usize);
pub(crate) fn leaf(name:&str)->Leaf {
    let index=match name {"upload"=>0,"uniform_build"=>1,"state"=>2,"bindings"=>3,"uniform_apply"=>4,_=>5};
    Leaf(start(),index)
}
impl Drop for Leaf {fn drop(&mut self){if let Some(t)=self.0 {LEAVES.lock().unwrap()[self.1]+=t.elapsed().as_nanos() as u64;}}}
pub(crate) fn append(out:&mut String) {
    if !enabled(){return;}
    let rows=ROWS.lock().unwrap(); let kinds=KINDS.lock().unwrap();
    out.push_str(" actor_skin=");
    for (i,r) in rows.iter().enumerate() {
        let name=if r.kind==0 {"unscoped"} else {&kinds[r.kind-1]};
        let _=write!(out,"{}{name}:{}:{}:{}:{}:{:.3}",if i==0{""}else{","},r.bones,r.calls,r.vertices,r.hits,r.ns as f64/1e6);
    }
    let ns=*LEAVES.lock().unwrap();
    let _=write!(out," actor_leaf={:.3}/{:.3}/{:.3}/{:.3}/{:.3}/{:.3}",ns[0] as f64/1e6,ns[1] as f64/1e6,ns[2] as f64/1e6,ns[3] as f64/1e6,ns[4] as f64/1e6,ns[5] as f64/1e6);
}
pub(crate) fn begin(){if enabled(){ROWS.lock().unwrap().clear();*LEAVES.lock().unwrap()=[0;6];}}
