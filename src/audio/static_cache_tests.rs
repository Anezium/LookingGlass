use super::*;
use crate::{assets::Assets, bsp::Bsp};
use macroquad::prelude::vec3;
use std::{hint::black_box, path::PathBuf, time::Instant};

fn bits(t: Trace) -> ([u32;4],bool,bool) {
    ([t.fraction.to_bits(),t.normal.x.to_bits(),t.normal.y.to_bits(),t.normal.z.to_bits()],t.start_solid,t.all_solid)
}
fn moving(world: &mut World, p: Vec3, frame: usize) {
    use crate::collision::Collider;
    world.set_dynamic(vec![Collider::box_bounds(p-Vec3::splat(15.),p+Vec3::splat(15.))]);
    world.set_settled_supports(vec![Collider::box_bounds(p-Vec3::splat(30.),p+Vec3::splat(30.))]);
    world.set_weapon_obstacles(vec![crate::combat::Target {id:frame,center:p,half:Vec3::splat(45.)}]);
}
#[test]
fn independent_rays_float_bits_mutable_completion_and_scene_lifetime() {
    let mut world=World::fixture(&[(vec3(100.,-100.,-100.),vec3(110.,100.,100.))]);
    let mut cache=Cache::new();
    let a=Vec3::ZERO; let b=Vec3::X*200.;
    assert_eq!(bits(cache.sweep(&world,a,b)),bits(world.sweep(a,b,Vec3::ZERO)));
    moving(&mut world,Vec3::X*50.,1);
    assert_eq!(bits(cache.sweep(&world,a,b)),bits(world.sweep(a,b,Vec3::ZERO)));
    assert_eq!(cache.hits,1);
    cache.sweep(&world,a,Vec3::Y*200.); // Changed ray leaves the first reusable.
    cache.sweep(&world,a,b);assert_eq!(cache.hits,2);
    cache.sweep(&world,vec3(-0.,0.,0.),b);assert_eq!(cache.misses,3);
    // Pin survives drop; allocate a different mask/scene before releasing it.
    drop(world);
    let world=World::fixture(&[(vec3(20.,-100.,-100.),vec3(30.,100.,100.))]);
    assert_eq!(bits(cache.sweep(&world,a,b)),bits(world.sweep(a,b,Vec3::ZERO)));
    assert_eq!(cache.misses,4);
    cache.reset();cache.sweep(&world,a,b);assert_eq!(cache.misses,5);
    for (a,b) in [(a,a),(b,a),(Vec3::X*25.,Vec3::X*25.),
        (vec3(f32::INFINITY,0.,0.),b),(vec3(f32::from_bits(0x7fc01234),0.,0.),b)] {
        for _ in 0..2 {assert_eq!(bits(cache.sweep(&world,a,b)),bits(world.sweep(a,b,Vec3::ZERO)));}
    }
    // More unique keys than capacity exercise bounded replacement and exact lookup.
    for i in 0..4096 {
        let end=vec3(200.,i as f32*0.1,0.);
        assert_eq!(bits(cache.sweep(&world,a,end)),bits(world.sweep(a,end,Vec3::ZERO)));
    }
}
#[test]
#[ignore = "requires LOOKING_GLASS_TEST_DATA; real-map randomized parity and host kernel ABBA"]
fn static_cache_randomized_maps_and_kernel_measurement() -> anyhow::Result<()> {
    let mut assets=Assets::open(&PathBuf::from(std::env::var("LOOKING_GLASS_TEST_DATA")?))?;
    for (name,eye) in [("hedge3",vec3(6168.,-7300.,280.15625)),("pandemonium",Vec3::ZERO)] {
        let map=Bsp::parse(&assets.read(&format!("maps/{name}.bsp"))?)?;
        let mut world=World::from_bsp(&map)?;let mut cache=Cache::new();
        let mut seed=0x628371u32;
        let mut random=|| {seed^=seed<<13;seed^=seed>>17;seed^=seed<<5; seed as f32/u32::MAX as f32};
        let mut rays=[(Vec3::ZERO,Vec3::ZERO);32];
        let mut changed=0;
        for frame in 0..320 {
            if frame%80==0 {world=World::from_bsp(&map)?;}
            if frame%53==0 {cache.reset();}
            // Repeated keys straddle moving dynamic, settled and weapon geometry.
            moving(&mut world,eye+vec3((frame%31) as f32*4.,0.,0.),frame);
            for (i,ray) in rays.iter_mut().enumerate() {
                if frame%4==0 || i==frame%32 {
                    let p=eye+vec3(random()*1000.-500.,random()*1000.-500.,random()*400.-200.);
                    *ray=(p,p+vec3(random()*2000.-1000.,random()*2000.-1000.,random()*1000.-500.));
                }
                let (a,b)=*ray;
                let expected=world.sweep(a,b,Vec3::ZERO);
                let got=cache.sweep(&world,a,b);
                assert_eq!(bits(got),bits(expected),"{name} frame={frame} ray={i}");
                if bits(expected)!=bits(world.audio_geometry().sweep(a,b)) {changed+=1;}
            }
        }
        assert!(cache.hits>0 && cache.misses>0 && changed>0);
        println!("STATIC_CACHE_RANDOM_EQUAL map={name} queries=10240 hits={} misses={} mutable_result_changes={changed} scene_replacements=4 explicit_resets=7",cache.hits,cache.misses);
        // Acoustic kernel itself: real room rays + 8 ambient sources. Moving
        // dynamic publication happens outside the measured/allocation interval.
        let mut times=Vec::new();let mut hot_allocs=0;
        for enabled in [false,true,true,false] {
            cache.reset();let h=cache.hits;let m=cache.misses;
            let mut ns=0u128;
            for frame in 0..1000 {
                moving(&mut world,eye+vec3((frame%31) as f32*4.,0.,0.),frame);
                let scope=crate::kernel_alloc::Scope::begin();let t=Instant::now();
                cache.begin(&world);
                let mut sweep=|a,b| if enabled {cache.sweep(&world,a,b)} else {world.sweep(a,b,Vec3::ZERO)};
                black_box(crate::audio::acoustics::room_with(eye,&mut sweep));
                for i in 0..8 {
                    black_box(crate::audio::acoustics::obstruction_with(eye,eye+vec3(i as f32*43.,170.,80.),&mut sweep));
                }
                ns+=t.elapsed().as_nanos();hot_allocs+=scope.finish();
            }
            times.push(ns as f64/1000000./1000.);
            println!("STATIC_CACHE_KERNEL map={name} enabled={enabled} frames=1000 mean_ms={} hits={} misses={} host_only=true",times.last().unwrap(),cache.hits-h,cache.misses-m);
        }
        assert_eq!(hot_allocs,0);
        println!("STATIC_CACHE_ALLOCATION map={name} frames=4000 allocation_calls={hot_allocs} cache_bytes={} oriented_saved_ms={:?}",std::mem::size_of::<Cache>(),[times[0]-times[1],times[3]-times[2]]);
    }
    Ok(())
}
