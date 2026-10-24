//! Host-only command/effective-input oracle; absent from the Vita SELF.
use std::{cell::RefCell, collections::BTreeMap, sync::OnceLock};
use sha2::{Digest, Sha256};
#[derive(Clone, Default)]
struct Attribute { enabled: bool, words: [u64; 7], divisor: u64 }
#[derive(Clone, Default)]
struct Input { attributes: [Attribute; 16], index: u64 }
#[derive(Default)]
struct Trace {
    vao: u64, vertex: u64, inputs: BTreeMap<u64, Input>,
    world: bool, commands: [Sha256; 2], draws: [Sha256; 2],
    counts: [BTreeMap<String, u64>; 2],
}
thread_local! { static TRACE: RefCell<Trace> = RefCell::default(); }
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LOOKING_GLASS_WORLD_INPUT_TRACE").is_some())
}
pub fn begin() {
    if enabled() { TRACE.with(|t| {let mut t=t.borrow_mut();t.commands=Default::default();t.draws=Default::default();t.counts=Default::default();}); }
}
pub fn world(active: bool) { if enabled() {TRACE.with(|t| t.borrow_mut().world=active);} }
pub fn report() -> serde_json::Value {
    if !enabled() {return serde_json::Value::Null;}
    TRACE.with(|t| {let t=t.borrow();serde_json::json!({"command_sha256":t.commands.iter().map(|h|format!("{:x}",h.clone().finalize())).collect::<Vec<_>>(),
        "effective_draw_input_sha256":t.draws.iter().map(|h|format!("{:x}",h.clone().finalize())).collect::<Vec<_>>(),"counts":t.counts})})
}
/// Explicit host camera fixture, read once before submission. These values are
/// saved with the proof and never participate in a Vita build.
pub fn view(eye: glam::Vec3, target: glam::Vec3, frame: u64) -> (glam::Vec3, glam::Vec3) {
    static VIEW: OnceLock<Option<serde_json::Value>> = OnceLock::new();
    let fixture=VIEW.get_or_init(|| std::env::var_os("LOOKING_GLASS_WORLD_INPUT_VIEW")
        .map(|p| serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()));
    let (mut eye, mut target)=(eye,target);
    if let Some(v)=fixture {
        let vector=|key:&str| v.get(key).map(|a| glam::Vec3::new(a[0].as_f64().unwrap() as f32,a[1].as_f64().unwrap() as f32,a[2].as_f64().unwrap() as f32));
        eye=vector("eye").unwrap_or(eye);target=vector("target").unwrap_or(target);
        let angle=v.get("radians_per_frame").and_then(|n|n.as_f64()).unwrap_or(0.) as f32*frame as f32;
        let direction=target-eye;
        target=eye+glam::Quat::from_rotation_z(angle)*direction;
        let offset=glam::Vec3::new(0.,(angle*0.7).sin()*16.,(angle*0.5).sin()*8.);
        eye+=offset;target+=offset;
    }
    if frame==0 || frame==60 || frame==120 || frame==179 {
        println!("WORLD_INPUT_VIEW {}",serde_json::json!({"frame":frame,"eye":eye.to_array(),"target":target.to_array()}));
    }
    (eye,target)
}
pub unsafe fn command(name: &str, words: &[u64]) {
    if !enabled() {return;}
    TRACE.with(|t| {
        let mut t=t.borrow_mut();let vao=t.vao;
        let vertex=t.vertex;
        match name {
            "glBindVertexArray" => {t.vao=words[0];},
            "glBindBuffer" if words[0]==0x8892 => {t.vertex=words[1];},
            "glBindBuffer" if words[0]==0x8893 => {t.inputs.entry(vao).or_default().index=words[1];},
            "glVertexAttribPointer" => {let mut a=[0;7];a[..6].copy_from_slice(words);a[6]=vertex;t.inputs.entry(vao).or_default().attributes[words[0] as usize].words=a;},
            "glEnableVertexAttribArray" => {t.inputs.entry(vao).or_default().attributes[words[0] as usize].enabled=true;},
            "glDisableVertexAttribArray" => {t.inputs.entry(vao).or_default().attributes[words[0] as usize].enabled=false;},
            "glVertexAttribDivisor" => {t.inputs.entry(vao).or_default().attributes[words[0] as usize].divisor=words[1];},
            _ => {},
        }
        let vertex_setup=name.contains("VertexAttrib") || name=="glBindVertexArray" || (name=="glBindBuffer" && words[0]==0x8892);
        let mut input_words=[0u64;145];let mut used=0;
        if name=="glDrawElementsInstanced" {
            let input=t.inputs.get(&vao).expect("observed input before draw");
            input_words[0]=input.index;used=1;
            for (i,a) in input.attributes.iter().enumerate() {
                if a.enabled {input_words[used]=i as u64;input_words[used+1..used+8].copy_from_slice(&a.words);input_words[used+8]=a.divisor;used+=9;}
            }
        }
        for scope in 0..=usize::from(t.world) {
            *t.counts[scope].entry(name.to_string()).or_default()+=1;
            if !vertex_setup {
                let h=&mut t.commands[scope];h.update(name.as_bytes());
                let pointer=if name.starts_with("glUniform") && name!="glUniform1i" {Some(words.len()-1)}
                    else if matches!(name,"glBufferData"|"glBufferSubData") {Some(if name=="glBufferData" {2}else{3})}else{None};
                for (i,w) in words.iter().enumerate() {if Some(i)!=pointer {h.update(w.to_le_bytes());}}
                if let Some(p)=pointer {
                    let len=if name.starts_with("glUniform") {
                        let components=match name {"glUniform1fv"=>1,"glUniform2fv"=>2,"glUniform3fv"=>3,"glUniform4fv"=>4,_=>16};
                        words[1] as usize*components*4
                    } else {words[if name=="glBufferData" {1}else{2}] as usize};
                    if words[p]!=0 {h.update(std::slice::from_raw_parts(words[p] as *const u8,len));}
                }
                static DETAIL: OnceLock<bool> = OnceLock::new();
                if *DETAIL.get_or_init(||std::env::var_os("LOOKING_GLASS_WORLD_INPUT_TRACE_DETAIL").is_some()) {
                    let mut arguments=words.to_vec();
                    if let Some(p)=pointer {arguments[p]=0;}
                    let payload=if name.starts_with("glUniform") && name!="glUniform1i" {
                        let n=words[1] as usize*match name {"glUniform1fv"=>1,"glUniform2fv"=>2,"glUniform3fv"=>3,"glUniform4fv"=>4,_=>16};
                        std::slice::from_raw_parts(*words.last().unwrap() as *const u32,n).to_vec()
                    } else {Vec::new()};
                    println!("WORLD_GL_EVENT {}",serde_json::json!({"scope":scope,"name":name,"args":arguments,"payload":payload,"prefix_sha256":format!("{:x}",h.clone().finalize())}));
                }
            }
            if used>0 {for w in &input_words[..used] {t.draws[scope].update(w.to_le_bytes());}}
        }
    });
}
