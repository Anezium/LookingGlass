use super::*;
fn check(g:&StaticGeometry,a:Vec3,b:Vec3,h:Vec3)->Counts {
    let q=SweepBounds::new(a,b,h);let mut c=Counts::default();
    let expected=g.legacy_trace(a,b,h,&q,&mut Counts::default());
    let actual=g.trace_inner(a,b,h,&q,&mut c);
    assert_eq!(bits(actual),bits(expected),"a={a:?} b={b:?} half={h:?}");c
}
fn accelerated(mut w:World)->World {
    let g=std::sync::Arc::get_mut(&mut w.static_geometry).unwrap();
    g.sah=g.tree.as_ref().map(|t|Hierarchy::build(&g.hulls,t));w
}
#[test]
fn sah_extremes_order_solid_flags_and_fallbacks() {
    let boxes:Vec<_>=(0..300).map(|i|{let p=Vec3::new((i%10)as f32*4.,((i/10)%10)as f32*4.,(i/100)as f32*4.);(p,p+Vec3::ONE*3.)}).collect();
    let w=accelerated(World::fixture(&boxes));let g=&w.static_geometry;
    let values=[0.,-0.,f32::from_bits(1),-f32::from_bits(1),SKIN,-SKIN,3.,4.,1e30,-1e30,f32::MAX,-f32::MAX,f32::INFINITY,f32::NEG_INFINITY,f32::from_bits(0x7fc12345)];
    for &x in &values {for &y in &values {for half in [Vec3::ZERO,Vec3::splat(0.5),PLAYER_HALF,Vec3::splat(-1.),Vec3::splat(f32::MAX)] {
        check(g,Vec3::new(x,y,-0.),Vec3::new(y,x,0.),half);
        check(g,Vec3::new(x,y,3.),Vec3::new(x,y,3.),half);
    }}}
    let c=check(g,Vec3::ZERO,Vec3::ZERO,Vec3::splat(1000.));assert_eq!(c.fallbacks,1);
    assert_eq!(check(g,Vec3::ZERO,Vec3::new(f32::from_bits(1),0.,0.),Vec3::ZERO).fallbacks,1);
    // Equal enter fractions with different normals; negative enter before max(0);
    // an all-solid event preserves the normal contributed earlier in old order.
    let w=accelerated(World::fixture(&[(Vec3::new(0.,-10.,-10.),Vec3::new(1.,10.,10.)),(Vec3::new(-10.,0.,-10.),Vec3::new(10.,1.,10.)),(Vec3::splat(-2.),Vec3::splat(2.))]));
    for a in [Vec3::new(-1.,-1.,0.),Vec3::new(-0.,-0.,0.),Vec3::new(-SKIN/2.,-SKIN/2.,0.),Vec3::ZERO] {
        for b in [Vec3::ONE,Vec3::ZERO,-Vec3::ONE] {check(&w.static_geometry,a,b,Vec3::ZERO);}
    }
    let scope=crate::kernel_alloc::Scope::begin();
    for _ in 0..100 {check(g,Vec3::splat(-1.),Vec3::splat(40.),Vec3::ZERO);}
    assert_eq!(scope.finish(),0);
}
#[test]
fn sah_patches_and_mutable_completion() {
    let mut a=World::fixture(&[(Vec3::splat(50.),Vec3::splat(60.))]);
    let g=std::sync::Arc::get_mut(&mut a.static_geometry).unwrap();
    g.hulls.push(triangle_hull(Vec3::ZERO,Vec3::X*10.,Vec3::Y*10.).unwrap());
    g.tree=Some(Node::build(&g.hulls,&mut[0,1]));
    let mut b=accelerated(World::fixture(&[]));b.static_geometry=std::sync::Arc::new(StaticGeometry::new(g.hulls.clone(),g.tree.clone()));
    let g=std::sync::Arc::get_mut(&mut b.static_geometry).unwrap();g.sah=Some(Hierarchy::build(&g.hulls,g.tree.as_ref().unwrap()));
    let mut changed=0;
    for frame in 0..180 {
        let p=Vec3::new((frame%11)as f32,0.,0.);
        for w in [&mut a,&mut b] {
            w.set_dynamic(vec![Collider::box_bounds(p-Vec3::ONE,p+Vec3::ONE)]);
            w.set_settled_supports(vec![Collider::box_bounds(p-Vec3::splat(0.5),p+Vec3::splat(0.5))]);
            w.set_weapon_obstacles(vec![crate::combat::Target{id:7,center:p,half:Vec3::splat(2.)}]);
        }
        for t in [-SKIN,-0.,0.,0.125,5.,10.,10.+SKIN] {for half in [Vec3::ZERO,Vec3::ONE,PLAYER_HALF] {
            let start=Vec3::new(t,t,20.);let end=Vec3::new(t,t,-20.);
            assert_eq!(bits(a.sweep(start,end,half)),bits(b.sweep(start,end,half)));
            assert_eq!(bits(a.sweep_except(start,end,half,Some(7))),bits(b.sweep_except(start,end,half,Some(7))));
            assert_eq!(bits(a.audio_geometry().sweep(start,end)),bits(b.audio_geometry().sweep(start,end)));
            if bits(a.sweep(start,end,half))!=bits(a.static_geometry.legacy_trace(start,end,half,&SweepBounds::new(start,end,half),&mut Counts::default())){changed+=1;}
        }}
    }
    assert!(changed>0);println!("SAH_MUTABLE changes={changed} frames=180");
}
#[test]
#[ignore="requires local assets and recorded 180-frame corpora"]
fn sah_real_maps_corpus_masks_and_load() -> anyhow::Result<()> {
    let output=std::path::PathBuf::from(std::env::var("SAH_OUTPUT")?);
    let mut assets=crate::assets::Assets::open(&std::path::PathBuf::from(std::env::var("LOOKING_GLASS_TEST_DATA")?))?;
    for name in ["hedge3","pandemonium","gvillage","skool2"] {
        let map=Bsp::parse(&assets.read(&format!("maps/{name}.bsp"))?)?;
        let data=std::fs::read_to_string(output.join("host").join(format!("{name}-A")).join("sah-corpus.jsonl"))?;
        let rows:Vec<serde_json::Value>=data.lines().map(|s|serde_json::from_str(s).unwrap()).collect();assert_eq!(rows.len(),180);
        let mut world=World::from_bsp(&map)?;
        let g=std::sync::Arc::get_mut(&mut world.static_geometry).unwrap();
        crate::kernel_alloc::heap_begin();let build_start=std::time::Instant::now();
        let hierarchy=Hierarchy::build(&g.hulls,g.tree.as_ref().unwrap());
        let build_ns=build_start.elapsed().as_nanos();let heap=crate::kernel_alloc::heap_finish();
        let storage=hierarchy.storage_bytes();
        println!("SAH_LOAD {}",serde_json::json!({"map":name,"hulls":g.hulls.len(),"nodes":hierarchy.nodes.len(),"certificates":hierarchy.legacy.len(),"build_ns":build_ns,"host_request_retained":heap[0],"host_request_peak_upper_bound":heap[1],"newlib32_retained":heap[2],"newlib32_peak_upper_bound":heap[3],"allocation_calls":heap[4],"storage_bytes":storage,"spatial_node_bytes":std::mem::size_of::<SpatialNode>(),"certificate_bytes":std::mem::size_of::<Certificate>(),"original_bytes":std::mem::size_of::<Original>()}));
        g.sah=Some(hierarchy);let g=&world.static_geometry;
        let mut owner_counts=[[Counts::default();2];5];let mut populations=[0usize;5];
        let v=|s:&serde_json::Value|Vec3::new(f32::from_bits(s[0].as_u64().unwrap()as u32),f32::from_bits(s[1].as_u64().unwrap()as u32),f32::from_bits(s[2].as_u64().unwrap()as u32));
        let mut queries=Vec::new();
        for row in &rows {for q in row["queries"].as_array().unwrap() {
            if q["hulls"].as_u64()!=Some(g.hulls.len()as u64){continue;}
            let owner=q["owner"].as_u64().unwrap()as usize;let a=v(&q["a"]);let b=v(&q["b"]);let h=v(&q["half"]);let query=SweepBounds::new(a,b,h);
            let old=g.legacy_trace(a,b,h,&query,&mut owner_counts[owner][0]);
            let new=g.trace_inner(a,b,h,&query,&mut owner_counts[owner][1]);
            let recorded:[u32;6]=std::array::from_fn(|i|q["trace"][i].as_u64().unwrap()as u32);
            assert_eq!(bits(old),recorded,"recorded {name}");assert_eq!(bits(new),recorded,"SAH {name}");
            populations[owner]+=1;queries.push((owner,a,b,h));
        }}
        for owner in 0..5 {
            let mut rounds=Vec::new();
            for sah_on in [false,true,true,false] {let start=std::time::Instant::now();for _ in 0..5 {for &(o,a,b,h) in &queries {if o==owner {let q=SweepBounds::new(a,b,h);let mut c=Counts::default();std::hint::black_box(if sah_on {g.trace_inner(a,b,h,&q,&mut c)}else{g.legacy_trace(a,b,h,&q,&mut c)});}}}rounds.push(start.elapsed().as_nanos()as f64/(5*populations[owner].max(1))as f64);}
            println!("SAH_CORPUS {}",serde_json::json!({"map":name,"owner":owner,"queries":populations[owner],"mismatches":0,"old":owner_counts[owner][0],"sah":owner_counts[owner][1],"abba_ns_per_query":rounds}));
        }
        let mut masks=vec![0,PLAYER_MASK,1|0x20000,-1,LIQUID_MASK];
        masks.extend(map.shaders.iter().map(|s|s.contents));masks.sort_unstable();masks.dedup();
        let mut checked=0;
        for mask in masks.iter().copied(){
            let w=accelerated(World::from_bsp_mask(&map,mask)?);
            for (_,a,b,h) in queries.iter().step_by((queries.len()/200).max(1)) {check(&w.static_geometry,*a,*b,*h);checked+=1;}
            for hull in w.static_geometry.hulls.iter().step_by(100) {
                let p=hull.bounds.min;for h in [Vec3::ZERO,PLAYER_HALF] {check(&w.static_geometry,p,p,h);check(&w.static_geometry,p+Vec3::Z*1000000.,p-Vec3::Z*1000000.,h);checked+=2;}
            }
        }
        println!("SAH_MASKS map={name} distinct_masks={} queries={checked} mismatches=0",masks.len());
    }
    Ok(())
}

#[test]
fn sah_reorders_candidates_but_preserves_raw_enter_and_all_solid_normal() {
    let mut world=accelerated(World::fixture(&[
        (Vec3::new(0.,-10.,-10.),Vec3::new(1.,10.,10.)),
        (Vec3::new(-10.,0.,-10.),Vec3::new(10.,1.,10.)),
        (Vec3::splat(-2.),Vec3::splat(2.)),
    ]));
    let g=std::sync::Arc::get_mut(&mut world.static_geometry).unwrap();
    // Force discovery in the opposite order. The required rank sort, rather
    // than a coincidentally ordered tiny SAH leaf, must establish exactness.
    g.sah.as_mut().unwrap().indices.reverse();
    let a=Vec3::new(-1.,-1.,0.);let b=Vec3::new(1.,1.,0.);
    let mut wrong=Trace::default();
    for i in [1,0,2] {clip_hull(&g.hulls[i],a,b,Vec3::ZERO,&mut wrong);}
    let expected=g.legacy_trace(a,b,Vec3::ZERO,&SweepBounds::new(a,b,Vec3::ZERO),&mut Counts::default());
    assert_eq!(expected.fraction.to_bits(),0f32.to_bits());assert!(expected.all_solid);
    assert_eq!(expected.normal,-Vec3::X);assert_eq!(wrong.normal,-Vec3::Y);
    assert_ne!(bits(expected),bits(wrong));check(g,a,b,Vec3::ZERO);
    let a=Vec3::new(-SKIN/2.,-SKIN/2.,0.);
    let trace=g.legacy_trace(a,b,Vec3::ZERO,&SweepBounds::new(a,b,Vec3::ZERO),&mut Counts::default());
    // Both negative raw enters compare below the already-clamped zero, so the
    // second hull replaces the first normal. Comparing final fractions fails.
    assert_eq!(trace.fraction.to_bits(),0f32.to_bits());assert_eq!(trace.normal,-Vec3::Y);
    check(g,a,b,Vec3::ZERO);
}

#[cfg(feature="vita_renderer")]
#[test]
fn sah_config_is_one_strict_default_off_boolean() {
    use crate::render::vita_renderer::Config;
    for c in [Config::default(),serde_json::from_str("{}").unwrap()] {
        assert!(!c.collision_sah);assert!(!c.audio_static_cache);assert!(!c.phase_census);
    }
    let c:Config=serde_json::from_str(r#"{"collision_sah":true}"#).unwrap();
    assert!(c.collision_sah);assert!(!c.audio_static_cache);assert!(!c.phase_census);
    for v in [serde_json::json!(1),serde_json::json!("on"),serde_json::Value::Null] {
        assert!(serde_json::from_value::<Config>(serde_json::json!({"collision_sah":v})).is_err());
    }
}
