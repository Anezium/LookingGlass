//! Host real-asset controls for accepted and rejected NPC input batches.
use super::*;
pub(crate) async fn check(assets: &mut Assets, family: &str) -> Result<()> {
    let map_name = if matches!(family,"snark" | "wildlife") { "potears1" } else { "potears2" };
    let map = Bsp::parse(&assets.read(&format!("maps/{map_name}.bsp"))?)?;
    let mut npcs = Npcs::load(assets,&map,map_name,None,false,false)?;
    npcs.sim.scene_hidden.clear(); npcs.sim.levels.clear();
    let models: &[&str] = match family {
        "wildlife" => &["c_walkrock_large","c_walkrock_small"], "snark" => &["c_snark_biteonly"], "ambient" => &["c_bill"], "ladybug" => &["c_ladybug"],
        "plant" => &["c_bloodrose","c_evilmushroom"], _ => anyhow::bail!("Unknown snapshot family"),
    };
    let atmosphere = Atmosphere::default();
    let view=Camera3D { position:vec3(150.,-190.,130.),target:Vec3::Z*60.,up:Vec3::Z,
        fovy:65_f32.to_radians(),z_near:2.,z_far:30000.,..Default::default() };
    let direction=(view.target-view.position).normalize(); let mut checked=0;
    let original = npcs.sim.actors.clone();
    for model in models {
        let mut template=original.iter().find(|a|a.spawn.model==*model).context("Missing family fixture")?.clone();
        template.spawn.name.clear(); template.spawn.hidden=false; template.spawn.origin=Vec3::ZERO;
        template.footing=None; template.walk=None; template.story_visible=false;
        for case in 0..8 {
            let mut actor=template.clone(); actor.time=0.7; actor.yaw=0.4; actor.mouth=0.6;
            actor.speaking=true; actor.greeting=0.5;
            if let Some(p)=&mut actor.resident {
                p.active=case!=1;
                match &mut p.body {
                    resident::Body::Ladybug(b)=>{
                        b.feet=Vec3::Z*60.; b.time=0.4; b.carrying=case!=3;
                        if case==3 {b.electric=0.4;}
                        if case==4 {b.frozen=true;b.health=0.;}
                        b.acorns=serde_json::from_value(serde_json::json!([{"position":[30.,0.,70.],"velocity":[0.,0.,0.],"age":0.2}]))?;
                        b.bursts=vec![crate::ladybug::Burst {position:Vec3::Z*40.,age:0.2}];
                    }
                    resident::Body::Plant(b)=>{
                        b.feet=Vec3::ZERO;b.grow_for_check();b.phase=crate::plants::Phase::Ready;b.time=0.4;
                        if case==4 {b.frozen=true;b.health=0.;b.phase=crate::plants::Phase::Dead;}
                        b.shots=vec![crate::plants::Shot {serial:77,position:vec3(30.,0.,70.),direction:Vec3::X,age:0.2}];
                    }
                    resident::Body::Snark(b)=>{
                        b.feet=Vec3::Z*60.;b.time=0.4;
                        b.phase=if case==3 {crate::snark::Phase::Bite} else {crate::snark::Phase::Idle};
                        if case==4 {b.frozen=true;b.health=0.;b.phase=crate::snark::Phase::Dead;}
                    }
                    resident::Body::Wildlife(b)=>{
                        b.feet=Vec3::Z*60.;b.time=0.4;
                        b.phase=match case {3=>crate::wildlife::Phase::Wake,4=>crate::wildlife::Phase::Run,_=>crate::wildlife::Phase::Idle};
                    }
                    _=>anyhow::bail!("Unexpected fixture resident"),
                }
            }
            if case==2 {actor.spawn.hidden=true;}
            if case==5 {actor.spawn.origin=Vec3::splat(10000.);actor.footing=Some(actor.spawn.origin);
                if let Some(p)=&mut actor.resident {match &mut p.body {
                    resident::Body::Ladybug(b)=>b.feet=Vec3::splat(10000.),
                    resident::Body::Plant(b)=>b.feet=Vec3::splat(10000.),
                    resident::Body::Snark(b)=>b.feet=Vec3::splat(10000.),
                    resident::Body::Wildlife(b)=>b.feet=Vec3::splat(10000.),_=>{}}}}
            let mut reference=None;
            for captured in [false,true] {
                npcs.sim.actors=vec![actor.clone()]; npcs.clear_actor_inputs(); npcs.art.resources.visual_lod.clear(); npcs.art.resources.spores.clear();
                npcs.art.resources.snapshot_registry.retire();
                if captured {
                    let (_,_,overflow)=npcs.extract_ants(view.position,direction);ensure!(!overflow,"Fixture overflow");
                    ensure!(npcs.art.resources.actor_inputs.as_ref().is_some_and(|i|i.valid_resources(&npcs.art.resources.snapshot_registry,&npcs.art.resources.models)),"Fixture rejected");
                    if case>=6 {
                        let input=npcs.art.resources.actor_inputs.as_mut().unwrap();
                        if case==6 {input.overflow=true;} else {input.generation=0;}
                        ensure!(!input.valid_resources(&npcs.art.resources.snapshot_registry,&npcs.art.resources.models),"Malformed fixture accepted");
                    } else {
                        // Accepted publication must survive complete removal of simulation actors.
                        npcs.sim.actors.clear();
                    }
                }
                clear_background(BLACK);set_camera(&view);crate::render_fx::begin_view(&view,2.5,&atmosphere,false);
                npcs.draw(view.position,direction,&atmosphere,true, captured && case < 6);
                crate::render_fx::finish();set_default_camera();let image=get_screen_data();
                if case==0 {ensure!(image.bytes.chunks_exact(4).filter(|p|p[..3]!=[0,0,0]).count()>20,"Empty active family fixture");}
                if let Some(expected)=&reference {ensure!(*expected==image.bytes,"{family}/{model}/{case}: pixels differ");checked+=1;}
                else {reference=Some(image.bytes);}
                next_frame().await;
            }
        }
        // Cast-size rejection must precede registry and pose allocations.
        npcs.sim.actors=vec![template.clone();1025];npcs.art.resources.snapshot_registry.retire();
        let (_,_,overflow)=npcs.extract_ants(view.position,direction);
        ensure!(overflow && npcs.art.resources.snapshot_registry.slots.is_empty(),"Unbounded cast extraction");
        if family != "ambient" {
            let mut overloaded=template.clone(); overloaded.spawn.hidden=false;
            if let Some(p)=&mut overloaded.resident { p.active=true;
                match &mut p.body {
                    resident::Body::Ladybug(b)=>{b.feet=Vec3::ZERO;b.bursts=vec![crate::ladybug::Burst {position:Vec3::ZERO,age:0.2};16385];}
                    resident::Body::Plant(b)=>{b.feet=Vec3::ZERO;b.shots=vec![crate::plants::Shot {serial:0,position:Vec3::ZERO,direction:Vec3::X,age:0.2};16385];}
                    resident::Body::Snark(b)=>{b.shots=vec![crate::snark::Shot {serial:0,position:Vec3::ZERO,direction:Vec3::X,age:0.2};16385];}
                    resident::Body::Wildlife(b)=>{b.frozen=true;}
                    _=>{}
                }
            }
            npcs.sim.actors=vec![overloaded];npcs.art.resources.snapshot_registry.retire();
            let (_,_,overflow)=npcs.extract_ants(view.position,direction);
            let input=npcs.art.resources.actor_inputs.as_ref().unwrap();
            ensure!(overflow && !input.valid_resources(&npcs.art.resources.snapshot_registry,&npcs.art.resources.models),"Unbounded projectile extraction");
        }
        if matches!(family,"snark" | "wildlife") {
            for bad in 0..3 {
                let mut a=template.clone();
                if bad==0 {a.spawn.model="wrong_resource".into();}
                let p=a.resident.as_mut().unwrap();
                match &mut p.body {
                    resident::Body::Snark(b)=>{if bad==1 {b.phase=crate::snark::Phase::Rise;} if bad==2 {b.variant=usize::MAX;}},
                    resident::Body::Wildlife(b)=>{if bad==1 {b.phase=crate::wildlife::Phase::Melee;} if bad==2 {b.frozen=true;}},
                    _=>unreachable!(),
                }
                npcs.sim.actors=vec![a];npcs.art.resources.snapshot_registry.retire();
                let (_,_,overflow)=npcs.extract_ants(view.position,direction);
                ensure!(overflow && !npcs.art.resources.actor_inputs.as_ref().unwrap().valid_resources(&npcs.art.resources.snapshot_registry,&npcs.art.resources.models),"Malformed resident accepted");
            }
        }
        npcs.sim.actors=vec![template];
    }
    println!("PASS snapshot NPC {family}: {checked} exact active/hidden/cull/frozen/effect/poisoned/rejection comparisons; bounded cast rejection");
    Ok(())
}
