//! No-GL map-path fixtures. Use production census gates/clocks, never force a
//! profiler or Totals active. Wall timestamps are synthetic, not performance data.
use super::*;
use glam::Vec3;
fn world_draws() {
    let _world=span(Region::WorldSubmit);
    let mut totals=crate::frame_profile::Totals::<3>::new();
    for _ in 0..507 {let mut at=totals.start();for i in 0..3 {totals.lap(i,&mut at);}}
    let (ns,calls)=totals.census_values().expect("production census gate");
    world_publish(ns,calls,totals.profile_active());count(3,507);
}
fn npc_path(world:&crate::collision::World) {
    for levels in [false,true] {
        let _npc=span(Region::Npc);let _owner=owner(Owner::Npc);
        let _path=span(if levels {Region::NpcLevels}else{Region::NpcGeneric});
        let _controller=npc_controller(levels);
        for category in [QueryCategory::GravityRecoil,QueryCategory::Navigation,QueryCategory::Sight,QueryCategory::Spawn,QueryCategory::Other] {
            let _category=query_category(category);
            for _ in 0..2 {std::hint::black_box(world.sweep(Vec3::Z*100.,Vec3::ZERO,Vec3::splat(2.)));}
        }
    }
}
fn fx_path() {
    let _fx=span(Region::Fx);
    {let _world=span(Region::FxWorld);path_count(Counter::WorldSets,1);path_count(Counter::WorldLive,64);
     {let _quad=span(Region::FxQuadBuild);path_count(Counter::QuadSets,1);
      let _pair=span(Region::FxPairWait);path_count(Counter::PairJobs,1);}
     let _register=span(Region::FxRegistration);path_count(Counter::Registrations,1);}
    {let _sample=span(Region::FxAttachedSample);path_count(Counter::AttachedSamples,1);path_count(Counter::AttachedLive,128);path_count(Counter::AttachedNew,2);}
    {let _quad=span(Region::FxQuadBuild);path_count(Counter::QuadSets,1);
     let _pair=span(Region::FxPairWait);path_count(Counter::PairJobs,1);}
    let _register=span(Region::FxRegistration);path_count(Counter::Registrations,1);
}
fn player_path() {
    let _player=span(Region::PlayerMovement);
    {let _prepare=span(Region::PlayerPrepare);path_count(Counter::PlayerPrepare,1);
     for _ in 0..2 {let _hook=span(Region::PlayerHooks);path_count(Counter::PlayerHooks,1);}}
    {let _hook=span(Region::PlayerHooks);path_count(Counter::PlayerHooks,1);}
    {let _fixed=span(Region::FixedPhysics);path_count(Counter::PlayerTicks,8);}
    let _checks=span(Region::PlayerChecks);path_count(Counter::TriggerVisits,10);path_count(Counter::TriggerEligible,8);path_count(Counter::FactBuilds,9);
}
fn hud_path() {
    let _hud=span(Region::Hud);
    {let _draw=span(Region::HudDraw);path_count(Counter::HudDraw,1);}
    {let _search=span(Region::HudSearch);path_count(Counter::HudSearch,1);}
    {let _draw=span(Region::HudDraw);path_count(Counter::HudDraw,1);
     let _menu=span(Region::HudMenus);path_count(Counter::HudMenus,1);}
}
#[test]
#[cfg(feature="frame_total_only")]
fn r15_four_map_paths_record_every_leaf_without_test_activation() {
    assert!(!crate::light_profile::full());
    let world=crate::collision::World::fixture(&[(Vec3::splat(-50.),Vec3::splat(50.))]);
    let mut seen=[false;REGIONS];
    for (map,added) in [("centipede1",4),("utemple",10),("wchess2",7),("hatter1",6)] {
        let mut now=Instant::now();begin_survey(now);
        let allocations=crate::kernel_alloc::Scope::begin();
        for frame in 0..180 {
            world_draws();
            match map {"centipede1"=>npc_path(&world),"utemple"=>fx_path(),"wchess2"=>player_path(),_=>hud_path()}
            {let _tail=span(Region::FrameworkTail);drop(span(Region::TailSave));drop(span(Region::TailProfiler));}
            accept(true,frame>=60);now+=std::time::Duration::from_secs(1);survey_presented(now,frame==179);
        }
        assert_eq!(allocations.finish(),0,"census/query observer hot path allocated");
        let document=test_survey_json().unwrap();
        for row in document["rows"].as_array().unwrap() {
            assert_eq!(row["added_brackets"],1522+added);assert_eq!(row["reused_brackets"],0);
            for i in 52..REGIONS {if row["calls"][i].as_u64().unwrap()>0 {seen[i]=true;assert!(row["inclusive_ns"][i].as_u64().unwrap()>0);}}
            assert_eq!(row["exclusive_ns"].as_array().unwrap().iter().map(|v|v.as_u64().unwrap()).sum::<u64>()+row["unbracketed_residual_ns"].as_u64().unwrap(),row["frame_total_ns"].as_u64().unwrap());
        }
        if let Ok(dir)=std::env::var("ALICE_TIMING_TEST_OUTPUT") {
            let dir=std::path::Path::new(&dir);
            std::fs::write(dir.join(format!("r15-{map}-census.json")),serde_json::to_vec(&document).unwrap()).unwrap();
            STATE.with(|s|{let s=s.borrow();let mut out=Vec::new();write_csv(&s,&mut out).unwrap();std::fs::write(dir.join(format!("r15-{map}-census.csv")),out).unwrap();});
        }
        println!("R15_PATH map={map} added_r15={added} added_total={} reused=0 world_draws=507 world_clock_reads=2028 hot_allocations=0",1522+added);
    }
    assert!(seen[52..].iter().all(|&s|s),"a new leaf has no feature-exact proof");
    println!("R15_STORAGE_BYTES state={} row={}",std::mem::size_of::<State>(),std::mem::size_of::<Row>());
    begin(false);
    for region in [Region::Fx,Region::FxWorld,Region::PlayerPrepare,Region::HudDraw] {let timer=span(region);assert!(timer.start.is_none());}
}

#[test]
#[cfg(feature="frame_total_only")]
fn r15_real_cpu_paths_keep_outputs_and_record_work() {
    use crate::{particles::tests::{attached_fixture,census_quad_fixture},skeletal::Transform};
    let world=crate::collision::World::fixture(&[(Vec3::splat(-100.),Vec3::ZERO)]);
    let mut attached=attached_fixture(2).fork_cached();
    let pose=|_:f32,_:Option<&str>|Transform{translation:Vec3::Z*10.,rotation:glam::Quat::IDENTITY};
    begin(false);attached.sample(0.3,1.,pose,|_,_,_|true);
    let reference=attached.output_bits(Vec3::Z*100.);
    crate::character::skinning::overlap::startup(2,false);
    begin_survey(Instant::now());
    {let _fx=span(Region::Fx);attached.sample(0.3,1.,pose,|_,_,_|true);census_quad_fixture(&mut attached);}
    assert_eq!(reference,attached.output_bits(Vec3::Z*100.));
    let mut interactions=crate::interaction::Interactions::empty();
    let mut stats=crate::inventory::Stats::default();let mut player=crate::movement::Player::new(Vec3::Z*10.);
    {let _player=span(Region::PlayerMovement);
     {let _prepare=span(Region::PlayerPrepare);interactions.prepare_player(&mut stats,&mut player);interactions.filter_level_controls(&mut crate::movement::Controls::default());}
     let mut clock=crate::movement::FixedClock::default();clock.advance(1./60.,&world,&mut player,crate::movement::Controls::default());
     interactions.triggers(1./60.,player.feet,player.feet);}
    STATE.with(|s|{let s=s.borrow();for region in [Region::FxAttachedSample,Region::FxQuadBuild,Region::FxPairWait,Region::PlayerPrepare,Region::PlayerHooks,Region::FixedPhysics,Region::PlayerChecks] {
        assert!(s.current.calls[region as usize]>0,"missing {:?}",region);
        assert!(s.current.inclusive[region as usize]>0);
    }assert!(s.current.counters[Counter::AttachedLive as usize]>0);assert!(s.current.counters[Counter::PlayerTicks as usize]>0);assert_eq!(s.current.errors,0);});
    begin(false);
}

struct AntTiming;
impl crate::ant::Timing for AntTiming {
    fn duration(&self,_:&str,_:&str)->f32 {1.}
    fn frame(&self,_:&str,_:&str)->f32 {0.1}
    fn speed(&self,_:&str,_:&str)->f32 {120.}
}
#[test]
#[cfg(feature="frame_total_only")]
fn r15_ant_substeps_and_level_entry_use_real_cpu_paths() {
    let world=crate::collision::World::fixture(&[(Vec3::new(-1000.,-1000.,-50.),Vec3::new(1000.,1000.,0.))]);
    let seed=crate::ant::Ant::new(1,false,Vec3::Z*0.1,0.);
    let mut off=seed.clone();let mut on=seed;
    begin(false);off.update(1./60.,&world,Vec3::X*300.,&AntTiming,&mut Default::default());
    begin_survey(Instant::now());
    {let _npc=span(Region::Npc);let _owner=owner(Owner::Npc);let _generic=span(Region::NpcGeneric);let _controller=npc_controller(false);
     on.update(1./60.,&world,Vec3::X*300.,&AntTiming,&mut Default::default());}
    assert_eq!(serde_json::to_value(off).unwrap(),serde_json::to_value(on).unwrap());
    let mut interactions=crate::interaction::Interactions::empty();
    let mut player=crate::movement::Player::new(Vec3::Z*10.);let mut stats=crate::inventory::Stats::default();let mut story=crate::story::Story::default();
    interactions.levels_step(&mut crate::level::Combat{dt:1./60.,world:&world,player:&mut player,stats:&mut stats,story:&mut story,notarget:false,summon:None,threatens:&|_|false});
    STATE.with(|s|{let s=s.borrow();assert_eq!(s.current.counters[Counter::AntSubsteps as usize],2);
        assert_eq!(s.current.calls[Region::NpcLevels as usize],1);
        assert!(s.current.npc_queries[QueryCategory::GravityRecoil as usize][0]>0);
        assert!(s.current.npc_queries[QueryCategory::Sight as usize][0]>0);
        assert_eq!(s.current.errors,0);});
    begin(false);
}
