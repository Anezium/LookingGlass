//! Explicitly enabled gameplay qualification. No input or simulation changes.
//! Bounded RAM recording; JSON encoding and file IO occur outside windows.
use std::{cell::RefCell, sync::atomic::{AtomicBool, Ordering::Relaxed}, time::Instant};
use macroquad::prelude::Vec3;
use serde::{Deserialize, Serialize};

static ENABLED: AtomicBool = AtomicBool::new(false);
const CAPACITY: usize = 8192;
pub(crate) fn enabled() -> bool { ENABLED.load(Relaxed) }
pub(crate) fn configure(value: bool) { ENABLED.store(value, Relaxed); }
#[derive(Default, Serialize)]
struct Row {
    us: u64, prepare_ns: u64, heap: u64, scene: usize, eye: [f32;3],
    queries: [u64;3], cache: [[u64;3];3], checks: u64, mismatches: u64,
}
#[derive(Serialize)]
struct Window { id:u64, label:String, map:String, check_stride:u64, rows:Vec<Row>, overflow:bool, end_us:u64 }
struct State {
    epoch:Instant, poll:Instant, command:u64, map:String, class:usize,
    row:Row, windows:Vec<Window>, events:Vec<serde_json::Value>, recording:bool, hits:u64, gameplay:bool, heap_failed:bool,
}
impl State {
    fn new()->Self { Self { epoch:Instant::now(), poll:Instant::now(), command:0,
        map:String::new(), class:0, row:Row::default(), windows:Vec::new(),
        events:Vec::new(), recording:false, hits:0, gameplay:false, heap_failed:false } }
    fn us(&self)->u64 { self.epoch.elapsed().as_micros() as u64 }
}
thread_local! { static STATE:RefCell<State> = RefCell::new(State::new()); }
pub(crate) fn heap()->u64 {
    #[cfg(target_os="vita")]
    { #[repr(C)] struct Info { values:[usize;10] }
      extern "C" { fn mallinfo()->Info; }
      unsafe { mallinfo().values[7] as u64 } }
    #[cfg(not(target_os="vita"))] { 0 }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Command { id:u64, action:String, label:String, #[serde(default)] check_stride:u64 }
fn command_path()->std::path::PathBuf {
    #[cfg(target_os="vita")] { "ux0:data/alice/private/cache_qual_command.json".into() }
    #[cfg(not(target_os="vita"))] { "cache_qual_command.json".into() }
}
fn export_path()->std::path::PathBuf {
    #[cfg(target_os="vita")] { "ux0:data/alice/private/frame_profile/cache-qual.json".into() }
    #[cfg(not(target_os="vita"))] { "cache-qual.json".into() }
}
// Called from the viewer, including paused/menu frames. No polling IO while a
// window is running: stop is requested by the existing pause/menu path instead.
pub(crate) fn poll(paused:bool)->anyhow::Result<()> {
    if !enabled() || !paused { return Ok(()); }
    let due=STATE.with(|s| { let mut s=s.borrow_mut();
        if s.poll.elapsed().as_secs_f32()<0.5 {false} else {s.poll=Instant::now();true} });
    if !due {return Ok(());}
    let Ok(bytes)=std::fs::read(command_path()) else {return Ok(());};
    // An in-flight serial FTP upload may briefly expose incomplete JSON.
    let Ok(c)=serde_json::from_slice::<Command>(&bytes) else {return Ok(());};
    apply(c)?;
    Ok(())
}
fn apply(c:Command)->anyhow::Result<()> {
    let stop=c.action=="stop";
    STATE.with(|s|->anyhow::Result<()> { let mut s=s.borrow_mut();
        if c.id<=s.command {return Ok(());}
        anyhow::ensure!(c.label.len()<=64 && c.label.bytes().all(|b|b.is_ascii_alphanumeric() || b==b'-'),"invalid qualification label");
        anyhow::ensure!(s.windows.len()<16,"qualification window limit");
        match c.action.as_str() {
            "start" => {
                anyhow::ensure!(!s.recording,"qualification already recording");
                let map=s.map.clone();
                s.windows.push(Window{id:c.id,label:c.label.clone(),map,check_stride:c.check_stride,
                    rows:Vec::with_capacity(CAPACITY),overflow:false,end_us:0});
                s.hits=0; s.recording=true;
                eprintln!("[lg/cache-qual] start id={} label={} check_stride={}",c.id,c.label,c.check_stride);
            },
            "stop" => {
                anyhow::ensure!(s.recording,"qualification is not recording");
                let us=s.us();let w=s.windows.last_mut().unwrap();
                anyhow::ensure!(w.label==c.label,"qualification stop label mismatch");
                w.end_us=us;s.recording=false;
                eprintln!("[lg/cache-qual] stop id={} label={}",c.id,c.label);
            },
            _=>anyhow::bail!("unknown qualification action"),
        }
        s.command=c.id; Ok(())
    })?;
    if stop {export()} else {Ok(())}
}
pub(crate) fn export()->anyhow::Result<()> {
    if !enabled() {return Ok(());}
    STATE.with(|s|->anyhow::Result<()> {let s=s.borrow();
        anyhow::ensure!(!s.recording,"qualification requires a stopped window before export");
        let path=export_path();let tmp=path.with_extension("tmp");
        let mut out=std::io::BufWriter::new(std::fs::File::create(&tmp)?);
        // Stream borrowed rows. Materializing a Value for thousands of rows
        // would duplicate their data and can exhaust the device heap at export.
        #[derive(Serialize)]
        struct Export<'a> { schema:u32, capacity:usize, query_classes:[&'static str;3],
            cache_columns:[&'static str;3], windows:&'a [Window], events:&'a [serde_json::Value],
            heap_failed:bool, clock:&'static str }
        serde_json::to_writer(&mut out,&Export{schema:1,capacity:CAPACITY,
            query_classes:["room","ambient","effects"],cache_columns:["hit","miss","eviction"],
            windows:&s.windows,events:&s.events,heap_failed:s.heap_failed,
            clock:"monotonic microseconds since qualification state creation"})?;
        use std::io::Write;out.flush()?;drop(out);std::fs::rename(tmp,path)?;
        eprintln!("[lg/cache-qual] exported windows={} events={}",s.windows.len(),s.events.len());Ok(())
    })
}
pub(crate) fn frame(scene:usize, eye:Vec3) {
    if enabled() {STATE.with(|s| {let mut s=s.borrow_mut();
        let heap=heap();
        if s.recording && heap>180*1048576 && !s.heap_failed {
            s.heap_failed=true;
            eprintln!("[lg/cache-qual] error=heap_limit heap_uordblks_bytes={heap}");
        }
        s.row=Row{us:s.us(),heap,scene,eye:eye.to_array(),..Row::default()};});}
}
pub(crate) fn gameplay(active:bool) {
    if enabled(){STATE.with(|s|s.borrow_mut().gameplay=active);}
}
pub(crate) fn class(class:usize) {
    if enabled() {STATE.with(|s| {let mut s=s.borrow_mut();s.class=class;s.row.queries[class]+=1;});}
}
pub(crate) fn result(hit:bool, evicted:bool) {
    if enabled() {STATE.with(|s| {let mut s=s.borrow_mut();let class=s.class;
        s.row.cache[class][if hit {0}else{1}]+=1;s.row.cache[class][2]+=u64::from(evicted);});}
}
pub(crate) fn should_check()->bool {
    if !enabled(){return false;}
    STATE.with(|s|{let mut s=s.borrow_mut();if !s.recording{return false;}
        let stride=s.windows.last().unwrap().check_stride;s.hits+=1;
        stride>0 && (s.hits-1)%stride==0})
}
pub(crate) fn checked(equal:bool) {
    if !equal {eprintln!("[lg/cache-qual] error=self_check_mismatch");}
    STATE.with(|s|{let mut s=s.borrow_mut();s.row.checks+=1;s.row.mismatches+=u64::from(!equal);});
}
pub(crate) struct AcousticSpan(Option<Instant>);
pub(crate) fn acoustics()->AcousticSpan { AcousticSpan(enabled().then(Instant::now)) }
impl Drop for AcousticSpan {fn drop(&mut self) {if let Some(start)=self.0 {
    let ns=start.elapsed().as_nanos() as u64;
    STATE.with(|s|{let mut s=s.borrow_mut();if !s.recording || !s.gameplay{return;}
        let mut row=std::mem::take(&mut s.row);row.prepare_ns=ns;
        let w=s.windows.last_mut().unwrap();if w.rows.len()<CAPACITY {w.rows.push(row);}else{w.overflow=true;}
    });
}}}
// These event allocations/encoding are only at loads/resets, never moving windows.
pub(crate) fn reset(reason:&str, old_scene:usize, before:usize, after:usize, pin_after:bool, refs_before:usize) {
    if enabled(){STATE.with(|s|{let mut s=s.borrow_mut();let us=s.us();
        s.events.push(serde_json::json!({"kind":"reset","reason":reason,"us":us,"old_scene":old_scene,
            "entries_before":before,"entries_after":after,"pin_after":pin_after,
            "old_refs_before":refs_before,"heap":heap()}));});}
}
pub(crate) fn loaded(map:&str, cache_on:bool) {
    if enabled(){STATE.with(|s|{let mut s=s.borrow_mut();s.map=map.to_string();let us=s.us();
        s.events.push(serde_json::json!({"kind":"load","map":map,"cache_on":cache_on,"us":us,"heap":heap()}));});}
}
pub(crate) fn transition(kind:&str, from:&str, to:&str, stage:&str) {
    if enabled(){STATE.with(|s|{let mut s=s.borrow_mut();let us=s.us();
        s.events.push(serde_json::json!({"kind":"transition","operation":kind,"from":from,"to":to,
            "stage":stage,"us":us,"heap":heap()}));});}
}
pub(crate) fn trace_equal(a:crate::collision::Trace,b:crate::collision::Trace)->bool {
    a.fraction.to_bits()==b.fraction.to_bits() && a.normal.to_array().map(f32::to_bits)==b.normal.to_array().map(f32::to_bits)
        && a.start_solid==b.start_solid && a.all_solid==b.all_solid
}

#[cfg(test)]
pub(crate) fn test_start(stride:u64) {
    configure(true);
    STATE.with(|s| { let mut s=s.borrow_mut();*s=State::new();s.recording=true;s.gameplay=true;
        s.windows.push(Window{id:1,label:"test".into(),map:"test".into(),check_stride:stride,
            rows:Vec::with_capacity(CAPACITY),overflow:false,end_us:0}); });
}
#[cfg(test)]
pub(crate) fn test_finish()->(u64,u64) {
    configure(false);
    STATE.with(|s| {let mut s=s.borrow_mut();let mut result=(s.row.checks,s.row.mismatches);
        for r in s.windows.iter().flat_map(|w|&w.rows){result.0+=r.checks;result.1+=r.mismatches;}
        *s=State::new();result})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_qualification_checks_bits_and_stays_opt_in() {
        assert!(!enabled());
        let a=crate::collision::Trace::default();let mut b=a;
        assert!(trace_equal(a,b));b.normal.x=-0.;assert!(!trace_equal(a,b));
        b=a;b.start_solid=true;assert!(!trace_equal(a,b));
        let c:Command=serde_json::from_str(r#"{"id":1,"action":"start","label":"hedge3-move"}"#).unwrap();
        assert_eq!(c.check_stride,0);
    }
}
