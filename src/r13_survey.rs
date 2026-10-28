//! Offline-prepared, explicit console diagnostics. Missing/disabled config is inert.
use std::{sync::atomic::{AtomicBool,Ordering::Relaxed}, time::Instant};
static CENSUS:AtomicBool=AtomicBool::new(false);
pub(crate) fn census_active()->bool {CENSUS.load(Relaxed)}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {enabled:bool,map:String,allocation:bool,census:bool,tag:String}
pub(crate) struct Survey {
    config:Config,started:Instant,first:bool,ready:bool,armed:bool,pending_gameplay:bool,
    all_gameplay:bool,frames:usize,rows:Vec<(u64,u64)>,previous:Option<Instant>,arm_time:Option<Instant>,
    snapshot:bool,pub(crate) done:bool,
}
fn validate(c:&Config)->anyhow::Result<()> {
    anyhow::ensure!(!c.map.is_empty() && c.map.bytes().all(|b|b.is_ascii_lowercase()||b.is_ascii_digit()),"invalid survey map");
    anyhow::ensure!(["memory","off","on"].contains(&c.tag.as_str()) && !(c.allocation && c.census),"invalid survey mode");
    Ok(())
}
impl Survey {
    #[cfg(target_os="vita")]
    pub(crate) fn open(options:&mut crate::Options)->anyhow::Result<Option<Box<Self>>> {
        let raw=match std::fs::read("ux0:data/alice/private/r13b-survey.json") {
            Ok(r)=>r,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(None),Err(e)=>return Err(e.into())};
        let c:Config=serde_json::from_slice(&raw)?;if !c.enabled{return Ok(None);}validate(&c)?;
        anyhow::ensure!(crate::frame_profile::Mode::read()==crate::frame_profile::Mode::Full,"survey requires full profile mode");
        if c.allocation {crate::allocation_census::enable();}
        if c.census {crate::phase_census::initialize_survey();}
        options.map=c.map.clone();options.entry=None;options.direct_map=true;options.new_game=false;options.load=None;
        println!("[lg/r13b-survey] entry map={} entry=first tag={} input=disabled saves=disabled",c.map,c.tag);
        let capacity=if c.allocation {8192}else{180};
        Ok(Some(Box::new(Self{config:c,started:Instant::now(),first:false,ready:false,armed:false,
            pending_gameplay:false,all_gameplay:true,frames:0,rows:Vec::with_capacity(capacity),previous:None,
            arm_time:None,snapshot:false,done:false})))
    }
    #[cfg(target_os="vita")]
    pub(crate) fn accept(&mut self,gameplay:bool) {
        self.pending_gameplay=gameplay;
        if self.armed && !self.done {
            self.all_gameplay &= gameplay;
            // Store attempted frames, retaining any loss of gameplay as exclusion.
            crate::phase_census::accept(true,self.frames>=60);
        }
    }
    pub(crate) fn take_snapshot(&mut self)->bool {std::mem::take(&mut self.snapshot)}
    #[cfg(target_os="vita")]
    pub(crate) fn presented(&mut self,map:&str,alive:bool,arm:bool)->anyhow::Result<()> {
        if self.done{return Ok(());}let now=Instant::now();
        if !self.first {self.first=true;self.snapshot=true;
            println!("[lg/r13b-survey] first_frame map={map} load_us={} alive={alive} gameplay={}",self.started.elapsed().as_micros(),self.pending_gameplay);}
        if !self.armed {
            if self.pending_gameplay && !self.ready {self.ready=true;
                println!("[lg/r13b-survey] ready map={map} gameplay=true select_to_arm=true");}
            if self.ready && self.pending_gameplay && arm {
                self.armed=true;self.previous=Some(now);self.arm_time=Some(now);
                CENSUS.store(self.config.census,Relaxed);
                println!("[lg/r13b-survey] window_begin map={map} warmup_frames=60 measured_frames=120");
            }
            return Ok(());
        }
        let previous=self.previous.replace(now).unwrap();
        self.frames+=1;
        anyhow::ensure!(self.rows.len()<self.rows.capacity(),"survey sample capacity exceeded");
        self.rows.push((now.duration_since(self.arm_time.unwrap()).as_micros() as u64,now.duration_since(previous).as_micros() as u64));
        if if self.config.allocation {now.duration_since(self.arm_time.unwrap()).as_secs()<40}else{self.frames<180} {return Ok(());}
        if self.config.census {crate::phase_census::export_survey(map)?;}
        CENSUS.store(false,Relaxed);
        self.snapshot=true;
        println!("[lg/r13b-survey] window_state map={map} all_gameplay={}",self.all_gameplay);
        println!("[lg/frame-total-heap] heap_uordblks_bytes={}",crate::allocation_census::used());
        use std::io::Write;
        let dir=std::path::Path::new("ux0:data/alice/private/frame_profile");anyhow::ensure!(dir.is_dir(),"existing frame_profile required");
        let mut out=std::io::BufWriter::new(std::fs::File::create(dir.join(format!("{map}-r13b-{}.csv",self.config.tag)))?);
        writeln!(out,"frame,since_arm_us,wall_us,measured")?;
        for (i,(end,wall)) in self.rows.iter().enumerate(){writeln!(out,"{},{end},{wall},{}",i+1,i>=60)?;}out.flush()?;
        println!("[lg/r13b-survey] done map={map} samples={} warmup_frames=60 measured_frames={} died={}",self.rows.len(),self.rows.len()-60,!alive);
        self.done=true;Ok(())
    }
}
#[cfg(test)] mod tests {use super::*;
    #[test]fn rejects_path_injection_and_simultaneous_observers(){
        for (map,a,c,ok) in [("potears1",true,false,true),("../x",false,false,false),("hatter1",true,true,false)] {
            assert_eq!(validate(&Config{enabled:true,map:map.into(),allocation:a,census:c,tag:"memory".into()}).is_ok(),ok);}
    }
}
