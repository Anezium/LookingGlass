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


HOOK='''                        #[cfg(feature = "native_timing")]
                        timing.with(|r| r.before_install());
'''
REPLACE='''                        crate::transition_memory::replace(&mut scene, next.scene, "chapter-or-console", &maps[current], &maps[selected]);
'''
CHAPTER_ORDER=(HOOK+REPLACE,REPLACE+HOOK)


def main():
    source=(ROOT/'src/viewer.rs').read_text()
    if os.environ.get('ALICE_TIMING_HOOK_MUTANT')=='chapter-order':
        assert source.count(CHAPTER_ORDER[0])==1
        source=source.replace(*CHAPTER_ORDER)
    mutant=os.environ.get('ALICE_TIMING_HOOK_MUTANT')
    if mutant in ('natural_ready_early','chapter_ready_early'):
        reason,marker = ('Natural','"natural-exit"') if mutant=='natural_ready_early' else ('ChapterOrConsole','"chapter-or-console"')
        ready=call(source,'timing_ready!(crate::native_timing::Reason::'+reason+')')
        pattern=r'[ \t]*#\[cfg\(feature = "native_timing"\)\]\n[ \t]*'+re.escape(ready)
        source,n=re.subn(pattern,'',source);assert n==1
        match=re.search(r'crate::transition_memory::replace\([^\n]*'+re.escape(marker)+r'[^\n]*\);',source);assert match
        pos=match.end()
        source=source[:pos]+'\n'+ready+source[pos:]
    if mutant=='recovery-after-reset':
        hook=call(source,'timing.with(|r| r.discontinuity(crate::native_timing::Milestone::Recovery')
        source=source.replace(hook,'',1)
        start=source.index('        if (recover || restart || console_restart) && !in_transport {')
        pos=source.index('recovery.clear();',start)+len('recovery.clear();')
        source=source[:pos]+hook+source[pos:]
    assert re.search(r'let mut timing = crate::native_timing::Session::open\(\)\?;\s*#\[cfg\(feature = "native_timing"\)\]\s*audio.bind_native_counters\(&mut timing\);',source)
    after=source.index('if let Err(error) = timing.export_after_viewer()')
    assert after<source.index('store.write_with_preview(Slot::Auto',after)<source.index('audio.finish()?',after)
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
    # Exact per-site static contracts. Each slice starts at its own branch/macro,
    # so no preceding install can satisfy another site's invalidation assertion.
    loaded=macro(source,'apply_loaded')
    def branch(start):
        return run[start:balanced(run,run.index('{',start),'{','}')]
    sites={
        'Fresh':(branch(run.index('    } else {',run.index('apply_loaded!(prepared.unwrap()'))),'scene = load_scene(',fresh),
        'ChapterOrConsole':(branch(run.index('                    Ok(next) => {')),'crate::transition_memory::replace(',chapter),
        'Natural':(branch(run.index('                Ok((next, arrived_player, arrived_eye, arrived_yaw)) => {')),'crate::transition_memory::replace(',natural),
        'StartSave':(loaded,'current = maps',restore),
        'Load':(loaded,'current = maps',restore),
        'Retry':(loaded,'current = maps',restore),
    }
    site_contracts={}
    static_violations=[]
    for reason,(site,mutation,ready) in sites.items():
        assert site.count(before)==1 and site.count(ready)==1, reason
        if os.environ.get('ALICE_TIMING_HOOK_MUTANT')!='chapter-order':
            assert site.index(before)<site.index(mutation)<site.index(ready), reason
        baseline_positions=[m.start() for m in re.finditer(r'(?<![.\w])(?:environment_clock|pickup_clock|player|pitch)\s*=|\(pos, yaw\)\s*=|clock\.pause\(\)|current = selected;',site)]
        late=all(site.index(ready)>pos for pos in baseline_positions)
        if not late: static_violations.append(reason+' ready before final baseline assignment')
        if mutant not in ('natural_ready_early','chapter_ready_early'): assert late,reason
        site_contracts[reason]=dict(before=site.index(before),first_mutation=site.index(mutation),ready=site.index(ready),baseline_positions=baseline_positions,ready_late=late)
    # apply_loaded invalidates before changing current, then installs resources
    # and observes ready after input suppression/last state assignment.
    assert loaded.index(before)<loaded.index('current = maps')<loaded.index('install_loaded_scene!($install')<loaded.index('save_frame.clear();')<loaded.index(restore)
    assert 'apply_loaded!(prepared.unwrap(), pause_on_load, initial)' in run
    assert 'apply_loaded!(restored);' in run and 'apply_loaded!(restored, false, retry)' in run
    # Pin the actual Recovery branch and its first statement before ANY live reset.
    recovery_start=source.index('        if (recover || restart || console_restart) && !in_transport {')
    recovery_site=source[recovery_start:balanced(source,source.index('{',recovery_start),'{','}')]
    recovery=call(recovery_site,'timing.with(|r| r.discontinuity(crate::native_timing::Milestone::Recovery')
    recovery_order=recovery_site.index(recovery)<recovery_site.index('let entry = interaction::spawn(')<recovery_site.index('recovery.clear();')<recovery_site.index('player = restored;')
    if not recovery_order: static_violations.append('Recovery hook after reset')
    if mutant!='recovery-after-reset': assert recovery_order
    recovery_tokens=[(recovery_site.index(recovery),recovery)]
    for mutation in ('recovery.clear();','player = restored;'):
        recovery_tokens.append((recovery_site.index(mutation),'assert_eq!(timing.r.operation,old_operation+1,"Recovery reset before invalidation");'))
    recovery_code=''.join(text for _,text in sorted(recovery_tokens))
    (OUT/'static-contracts.json').write_text(json.dumps(dict(violations=static_violations,per_site=site_contracts),indent=2)+'\n')
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
struct V {x:f32,y:f32,z:f32} struct T {ticks:u64} impl T {fn pause(&mut self){}}
struct Next {environment_clock:f32,pickup_clock:f32} struct Stats {yes:bool,powers:Powers} impl Stats {fn alive(&self)->bool{self.yes}}
struct Powers;impl Powers{fn world_dt(&self,dt:f32)->f32{dt}}
struct Story(bool);impl Story{fn busy(&self)->bool{self.0}}
struct School(bool);impl School{fn cinematic(&self)->bool{self.0}} struct Interactions {school:Option<School>}
TLS
GETTER
fn main(){
 let mut timing=TestSession{r:Recorder::new(Fake{now:0},Some([5;16]),Metadata::default())};
 let mut current=0;let maps=["tower2","tower2"];let entry_spawn:Option<String>=Some("same-entry".into());
 let mut environment_clock=17.25f32;let mut pickup_clock=23.5f32;let mut clock=T{ticks:47};
 let mut pos=V{x:1.,y:2.,z:3.};let mut yaw=0.3f32;let mut pitch=-0.2f32;let mut scene:i32;let mut player=0;let selected=1;
MACROS
INSTALLS
 assert_eq!(REPLACES.load(Ordering::Relaxed),4);assert_eq!(scene,6);
 let generation=timing.r.generation;FAILED
 assert_eq!(generation,timing.r.generation); // actual failed hook never installs
 // Isolate interval/eligibility checks from install-order probes above.
 timing=TestSession{r:Recorder::new(Fake{now:0},Some([5;16]),Metadata::default())};
 timing.r.ready("tower2",None,Reason::Fresh,State::default());
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
 let old_generation=timing.r.generation;let old_operation=timing.r.operation;
 RECOVERY
 assert_eq!(timing.r.generation,old_generation);
 assert_eq!(timing.r.operation,old_operation+1);
 assert!(timing.r.pending.unwrap().reasons&OPERATION!=0);
 assert!(matches!(timing.r.events.last().unwrap().kind,Milestone::Recovery));
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
    for index,reason in enumerate(['Fresh','StartSave','Load','Retry','ChapterOrConsole','Natural'],1):
        site,mutation,ready=sites[reason]
        if reason in ('StartSave','Load','Retry'):
            mode={'StartSave':'initial','Load':'replace','Retry':'retry'}[reason]
            ready=ready.replace('$install',mode)
        else:
            mode='initial' if reason=='Fresh' else 'replace'
        action=f'install_loaded_scene!({mode}, {index}, &maps[current], &maps[current]);'
        # Execute actual source hook ORDER, replacing only game mutation/resource
        # work with a stub that checks the recorder BEFORE changing the scene.
        tokens=[(site.index(before),before),(site.index(mutation),f'assert!(timing.r.pending.unwrap().reasons & (INSTALL|OPERATION)==(INSTALL|OPERATION), "{reason} mutation before invalidation");'+action),(site.index(sites[reason][2]),ready)]
        if reason in ('Fresh','ChapterOrConsole','Natural'):
            # Clocks come from the ACTUAL late assignments, never the scene stub.
            # Only unavailable game/asset RHS work is stubbed. Ordering and the
            # literal/environment/pickup/pitch/current assignments are real text.
            for field in ('environment_clock','pickup_clock','pitch'):
                for match in re.finditer(r'(?<![.\w])'+field+r'\s*=.*?;',site):
                    tokens.append((match.start(),match.group()+'timing.r.clock.source.now+=1;'))
            for match in re.finditer(r'\(pos, yaw\)\s*=',site):
                tokens.append((match.start(),f'pos=V{{x:{index}.,y:2.,z:3.}};yaw={index}.3;timing.r.clock.source.now+=1;'))
            for match in re.finditer(r'(?<![.\w])player\s*=',site):
                tokens.append((match.start(),f'player={index};timing.r.clock.source.now+=1;'))
            for match in re.finditer(r'clock\.pause\(\);|current = selected;',site):
                tokens.append((match.start(),match.group()+'timing.r.clock.source.now+=1;'))
            setup=f'environment_clock=-17.;pickup_clock=-23.;pitch=-0.2;let next=Next{{environment_clock:{index}.25,pickup_clock:{index}.5}};'
            expected_env=f'{index}.25' if reason=='Natural' else '0.0'
            expected_pick=f'{index}.5' if reason=='Natural' else '0.0'
            expected_pitch='-0.2' if reason=='Fresh' else '0.0'
            checks=f'assert_eq!(timing.r.worlds.last().unwrap().baseline.environment_bits,({expected_env}f32).to_bits());assert_eq!(timing.r.worlds.last().unwrap().baseline.pickup_bits,({expected_pick}f32).to_bits());assert_eq!(timing.r.worlds.last().unwrap().baseline.pose_bits[4],({expected_pitch}f32).to_bits());assert_eq!(timing.r.worlds.last().unwrap().ready_us,Some(timing.r.clock.source.now),"{reason} ready before last baseline assignment");'
        else:
            setup=''
            tokens.append((site.index(mutation)+1,f'environment_clock={index}.25;'))
            checks=f'assert_eq!(timing.r.worlds.last().unwrap().baseline.environment_bits,({index}.25f32).to_bits());'
        installs.append(f'{setup}timing.r.clock.source.now={index}*100;timing.r.begin(State::default());'+''.join(text for _,text in sorted(tokens))+f'assert_eq!(timing.r.generation,{index});'+checks+'assert_eq!(timing.r.worlds.last().unwrap().baseline.physics_ticks,47);')
    dt=re.search(r'let dt = if simulate \{ elapsed\.clamp\(0\., 0\.05\) \} else \{ 0\. \};',source).group()
    worlddt='let world_dt = stats.powers.world_dt(dt);'
    assert worlddt in source
    values={'MODULE':str(ROOT/'src/native_timing/mod.rs'),'TLS':tls,'GETTER':getter,'MACROS':'\n'.join(extracted.values()),
        'INSTALLS':'\n'.join(installs),'FAILED':failed,'BEGIN':begin,'CPU':cpu,'DT':dt,'WORLD_DT':worlddt,
        'SIMULATION':simulation,'END':end,'RECOVERY':recovery_code,'HOOK_SKIP':skip,'HOOK_MODAL':modal,'LIGHT':light,'FULL':full}
    code=re.sub(r'\b('+'|'.join(values)+r')\b',lambda m:values[m.group()],code)
    mutant=os.environ.get('ALICE_TIMING_HOOK_MUTANT')
    if mutant=='retry-ready':
        code=code.replace('timing_ready!(timing_install_reason!(retry));','')
    elif mutant=='pause-reason':
        code=code.replace('if paused { reasons |= nt::PAUSED; }','')
    elif mutant=='dt-cap':
        code=code.replace('elapsed.clamp(0., 0.05)','elapsed.clamp(0., 0.5)')
    elif mutant in ('chapter-order','natural_ready_early','chapter_ready_early','recovery-after-reset'):
        pass  # actual extracted order is compiled; mutation must fail semantically
    elif mutant:
        raise ValueError('unknown mutant')
    src=OUT/'hooks.rs';src.write_text(code)
    target=Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'target')))/'debug'
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
        'extracted_macros':list(extracted),'per_site_order':site_contracts,'recovery_hook':recovery,'compiled_executed':True,'scope':'actual hook text with game stubs, not full rendered/asset/device playthrough'},indent=2)+'\n')
    print(result.stdout,end='')

if __name__=='__main__':main()
