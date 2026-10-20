#!/usr/bin/env python3
"""Compile and execute actual viewer hook/macro text with small host stubs.

This is source-path integration evidence, not a rendered/asset/device playthrough.
The recorder, clock checks and writer are the production Rust modules. Hooks and
macro bodies are extracted without reimplementing their logic. Game dependencies
are stubbed; the separate in-crate test exercises actual FixedClock/Player/powers.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

ROOT=Path(__file__).resolve().parents[1]
OUT=Path(os.environ['ALICE_TIMING_TEST_OUTPUT'])/os.environ.get('ALICE_TIMING_HOOK_RUN','source-hook-integration')


def balanced(source,start,left,right):
    depth=0
    # Skip quoted strings/comments before inspecting delimiter characters.
    token=re.compile(r'"(?:\\.|[^"\\])*"|//[^\n]*|/\*.*?\*/|.',re.S)
    for match in token.finditer(source,start):
        part=match.group()
        if part==left:depth+=1
        if part==right:
            depth-=1
            if depth==0:return match.end()
    raise AssertionError('unbalanced source extraction')


def macro(source,name):
    start=source.index('macro_rules! '+name)
    return source[start:balanced(source,source.index('{',start),'{','}')]


def call(source,needle,offset=0):
    start=source.index(needle,offset)
    end=balanced(source,source.index('(',start),'(',')')
    assert source[end]==';'
    return source[start:end+1]


def main():
    source=(ROOT/'src/viewer.rs').read_text()
    profile=(ROOT/'src/frame_profile.rs').read_text()
    OUT.mkdir(parents=True,exist_ok=False)
    extracted={name:macro(source,name) for name in ('timing_state','timing_ready','timing_install_reason','install_loaded_scene')}
    before=call(source,'timing.with(|r| r.before_install())')
    begin=call(source,'timing.with(|r| r.begin(timing_state!()))')
    cpu=call(source,'timing.with(|r| r.cpu_begin())')
    simulation=call(source,'timing.with(|r| r.simulation(elapsed, dt, world_dt))')
    end=call(source,'timing.with(|r| {\n            use crate::native_timing as nt;')
    failed=call(source,'timing.with(|r| r.event(crate::native_timing::Milestone::LoadFailed')
    skip=call(source,'timing.with(|r| r.discontinuity(crate::native_timing::Milestone::SkipComplete')
    modal=call(source,'timing.with(|r| {\n                r.discontinuity(crate::native_timing::Milestone::ModalEnter')
    fresh=call(source,'timing_ready!(crate::native_timing::Reason::Fresh)')
    chapter=call(source,'timing_ready!(crate::native_timing::Reason::ChapterOrConsole)')
    natural=call(source,'timing_ready!(crate::native_timing::Reason::Natural)')
    restore=call(source,'timing_ready!(timing_install_reason!($install))')
    # Audit the actual run's install call graph, independently of running snippets.
    run=source[source.index('pub async fn run('):source.index('    #[cfg(feature = "native_timing")]\n    if let Err(error) = timing.export_after_viewer()')]
    assert run.count('timing.with(|r| r.before_install())')==4
    assert run.count('crate::transition_memory::replace(')==3
    assert run.count('timing_ready!(')==4
    assert 'apply_loaded!(restored, false, retry)' in run
    for assignment,ready in [('scene = load_scene(',fresh),('"chapter-or-console", &maps[current]',chapter),('"natural-exit", &maps[current]',natural)]:
        a=run.index(assignment);b=run.index(ready,a)
        assert run.rfind(before,0,a)>=0 and b>a
    # Exact production legacy ID bookkeeping is also compiled/executed on host.
    tls=profile[profile.index('thread_local! { static NATIVE_MEASURED:'):]
    tls=tls[:tls.index('\n#[cfg(')]
    getter=profile[profile.index('pub(crate) fn native_timing_measured_id()'):]
    getter=getter[:getter.index('\npub(crate) fn begin_viewer_frame')]
    light_start=profile.index('if measured { s.frames += 1;')
    light=profile[light_start:balanced(profile,profile.index('{',light_start),'{','}')]
    full_start=profile.index('            s.frames += 1;\n            #[cfg(feature = "native_timing")]')
    full=profile[full_start:profile.index('            crate::frame_phases::measured(s.frames);',full_start)]
    code='''#![allow(dead_code,unused_mut,unused_variables)]
#[path="MODULE"] mod native_timing;
use native_timing::*;
use std::sync::atomic::{AtomicUsize,Ordering};
static REPLACES:AtomicUsize=AtomicUsize::new(0);
mod transition_memory { pub fn replace(scene:&mut i32,next:i32,_reason:&str,_from:&str,_to:&str) {super::REPLACES.fetch_add(1,super::Ordering::Relaxed);*scene=next;} }
struct Fake {now:u64} impl clock::Source for Fake {fn domain(&self)->&'static str{"host-injected-fake-us"} fn read_us(&mut self)->Result<u64,clock::Error>{Ok(self.now)}}
struct TestSession {r:Recorder<Fake>} impl TestSession {fn with(&mut self,f:impl FnOnce(&mut Recorder<Fake>)){f(&mut self.r)}}
struct V {x:f32,y:f32,z:f32} struct T {ticks:u64} struct Stats {yes:bool,powers:Powers} impl Stats {fn alive(&self)->bool{self.yes}}
struct Powers;impl Powers{fn world_dt(&self,dt:f32)->f32{dt}}
struct Story(bool);impl Story{fn busy(&self)->bool{self.0}}
struct School(bool);impl School{fn cinematic(&self)->bool{self.0}} struct Interactions {school:Option<School>}
TLS
GETTER
fn main(){
 let mut timing=TestSession{r:Recorder::new(Fake{now:0},Some([5;16]),Metadata::default())};
 let mut current=0;let maps=["tower2","tower2"];let entry_spawn:Option<String>=Some("same-entry".into());
 let mut environment_clock=17.25f32;let mut pickup_clock=23.5f32;let clock=T{ticks:47};
 let pos=V{x:1.,y:2.,z:3.};let yaw=0.3f32;let pitch=-0.2f32;let mut scene:i32;
MACROS
INSTALLS
 assert_eq!(REPLACES.load(Ordering::Relaxed),5);assert_eq!(scene,6);
 let generation=timing.r.generation;FAILED
 assert_eq!(generation,timing.r.generation); // actual failed hook never installs
 let mut paused=false;let mut focused=true;let mut menu=false;let inventory_menu=false;
 let console_input=false;let mut simulate=true;let skip_id:Option<u8>=None;
 let pand_cinematic=false;let scripted_camera:Option<u8>=None;
 let story=Story(false);let interactions=Interactions{school:None};
 let stats=Stats{yes:true,powers:Powers};let mut active=true;let flying=false;
 for index in 0..6 {
   timing.r.clock.source.now=1000+index*100;
   BEGIN CPU
   let elapsed=0.06f32;
   paused=index==1;focused=index!=2;menu=index==3;simulate=index!=4;active=!(paused||!focused||menu);
   DT WORLD_DT SIMULATION
   END
   let row=timing.r.pending.unwrap();
   if index==1{assert!(row.reasons&PAUSED!=0)}
   if index==2{assert!(row.reasons&UNFOCUSED!=0)}
   if index==3{assert!(row.reasons&OVERLAY!=0)}
   if index==4{assert!(row.reasons&FROZEN_SIM!=0)}
   assert_eq!(row.simulation.unwrap().dt_bits,(if simulate {0.05f32}else{0.}).to_bits());
 }
 HOOK_SKIP HOOK_MODAL
 assert!(timing.r.pending.unwrap().reasons&(SKIP|MODAL|OPERATION)!=0);
 assert!(!timing.r.stopped);timing.r.clock.source.now=9000;BEGIN
 assert!(timing.r.stopped);assert!(timing.r.pending.is_none());
 assert_eq!(timing.r.frames.len(),6);assert_eq!(timing.r.viewer_seq,7);
 struct Legacy{frames:usize} let mut s=Legacy{frames:0};
 for measured in [false,true,true,false,true]{NATIVE_MEASURED.with(|id|id.set(None));LIGHT
 assert_eq!(native_timing_measured_id(),measured.then_some(s.frames as u64));}
 assert_eq!(s.frames,3);FULL assert_eq!(native_timing_measured_id(),Some(4));
 println!("HOST Native storage={} Recorder={} auxiliary={} budget={}",STORAGE_BYTES,std::mem::size_of::<Recorder<clock::Native>>(),AUX_RESERVE,BUDGET);
 println!("actual extracted viewer hooks PASS: six same-map install reasons, failed preparation generation, begin/cpu/sim/end, eligibility, skip/modal sealing, exact legacy ID bookkeeping");
}
'''
    installs=[]
    for index,(reason,ready) in enumerate([('Fresh',fresh),('StartSave',restore.replace('$install','initial')),
        ('Load',restore.replace('$install','replace')),('Retry',restore.replace('$install','retry')),
        ('ChapterOrConsole',chapter),('Natural',natural)],1):
        action='install_loaded_scene!(initial, 11, &maps[current], &maps[current]);' if index==1 else 'install_loaded_scene!('+('retry' if reason=='Retry' else 'replace')+', '+str(index)+', &maps[current], &maps[current]);'
        installs.append(before+action+ready+f'assert_eq!(timing.r.generation,{index});assert_eq!(timing.r.worlds.last().unwrap().baseline.physics_ticks,47);')
    dt=re.search(r'let dt = if simulate \{ elapsed\.clamp\(0\., 0\.05\) \} else \{ 0\. \};',source).group()
    worlddt='let world_dt = stats.powers.world_dt(dt);'
    assert worlddt in source
    values={'MODULE':str(ROOT/'src/native_timing/mod.rs'),'TLS':tls,'GETTER':getter,'MACROS':'\n'.join(extracted.values()),
        'INSTALLS':'\n'.join(installs),'FAILED':failed,'BEGIN':begin,'CPU':cpu,'DT':dt,'WORLD_DT':worlddt,
        'SIMULATION':simulation,'END':end,'HOOK_SKIP':skip,'HOOK_MODAL':modal,'LIGHT':light,'FULL':full}
    code=re.sub(r'\b('+'|'.join(values)+r')\b',lambda m:values[m.group()],code)
    mutant=os.environ.get('ALICE_TIMING_HOOK_MUTANT')
    if mutant=='retry-ready':
        code=code.replace('timing_ready!(timing_install_reason!(retry));','')
    elif mutant=='pause-reason':
        code=code.replace('if paused { reasons |= nt::PAUSED; }','')
    elif mutant=='dt-cap':
        code=code.replace('elapsed.clamp(0., 0.05)','elapsed.clamp(0., 0.5)')
    elif mutant:
        raise ValueError('unknown mutant')
    src=OUT/'hooks.rs';src.write_text(code)
    target=ROOT/'target/debug'
    files=list(target.rglob('*.rlib'))+list(target.rglob('*.dylib'))
    cmd=[str(Path.home()/'.cargo/bin/rustc'),'--edition=2021','--cfg','feature="native_timing"',str(src),'-o',str(OUT/'hooks')]
    for parent in sorted({str(p.parent) for p in files}):cmd+=['-L','dependency='+parent]
    for crate in ['serde','serde_json','anyhow','sha2']:
        lib=max((p for p in files if p.name.startswith('lib'+crate+'-') and p.suffix=='.rlib'),key=lambda p:p.stat().st_mtime)
        cmd+=['--extern',crate+'='+str(lib)]
        if lib.with_suffix('.rmeta').exists():cmd+=['--extern',crate+'='+str(lib.with_suffix('.rmeta'))]
    result=subprocess.run(cmd,capture_output=True,text=True)
    (OUT/'compile.log').write_text(result.stdout+result.stderr)
    if result.returncode:raise RuntimeError(result.stderr)
    result=subprocess.run([str(OUT/'hooks')],capture_output=True,text=True)
    (OUT/'run.log').write_text(result.stdout+result.stderr)
    if result.returncode:raise RuntimeError(result.stdout+result.stderr)
    (OUT/'source-evidence.json').write_text(json.dumps({'viewer_sha256':hashlib.sha256(source.encode()).hexdigest(),
        'frame_profile_sha256':hashlib.sha256(profile.encode()).hexdigest(),'generated_harness_sha256':hashlib.sha256(code.encode()).hexdigest(),
        'extracted_macros':list(extracted),'compiled_executed':True,'scope':'actual hook text with game stubs, not full rendered/asset/device playthrough'},indent=2)+'\n')
    print(result.stdout,end='')

if __name__=='__main__':main()
