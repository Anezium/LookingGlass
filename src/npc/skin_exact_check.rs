//! Host-only intermediate-frame regressions; no fixture or observer ships on Vita.
use super::*;
pub(crate) async fn check(assets:&mut Assets,options:&crate::Options)->Result<()> {
    use sha2::{Digest,Sha256};
    let specs=crate::texture::read_materials(assets)?;
    crate::character::skinning::overlap::startup(2,false);
    std::fs::create_dir_all("private/exact-frames")?;
    for (case,model) in [("frozen-card","cardguard_spade"),("cat-fade","c_cheshire"),
        ("funhouse-fade","c_tweedle_dee"),("jabberwock-fade","c_gryphon"),("shared-slots","c_torchgnome3")] {
        let mut scene=crate::render::Scene::load_for(assets,"gvillage",options.difficulty)?;
        let mut puppet=Puppet::load(assets,model,&[],&specs)?;
        if case.ends_with("fade") {puppet=puppet.with_afterimages();}
        let clip=puppet.idle_clip().to_owned();
        for frame in 0..180 {
            crate::sim_maps::probe::begin();
            let time=frame as f32*0.035;
            let view=Camera3D {position:vec3(180.,-260.,130.),target:Vec3::Z*48.,up:Vec3::Z,
                fovy:75_f32.to_radians(),z_near:2.,z_far:30000.,..Default::default()};
            clear_background(BLACK);set_camera(&view);
            scene.begin_fx_view(&view,time,false,true);
            let mut skin=crate::character::skinning::overlap::Frame::begin();
            if case=="frozen-card" {
                for instance in 0..2 {
                    let mut guard=crate::cards::Guard::new(crate::cards::Kind::Spade,
                        vec3(instance as f32*90.-45.,0.,0.),instance as f32*0.65,0.7,instance);
                    guard.frozen=(60..120).contains(&frame);
                    guard.health=if frame>=60 {0.} else {160.};
                    guard.phase=if frame>=60 {crate::cards::Phase::Dead} else {crate::cards::Phase::Idle};
                    guard.time=if frame>=120 {(frame-120) as f32*0.02} else {time*0.1};
                    let electric=if (30..60).contains(&frame) {0.4} else {0.};
                    let consumer=puppet.electric_consumer(electric);
                    puppet.draw_card(&guard,false,view.position,&scene.atmosphere);
                    drop(consumer);puppet.draw_electric(electric);
                }
            } else if case.ends_with("fade") {
                let at=Transform {translation:Vec3::ZERO,rotation:Quat::from_rotation_z(0.9)};
                if frame<60 || frame>=120 {puppet.draw(&clip,time,true,at,1.,false);}
                if frame>=60 {puppet.draw_afterimage(&clip,time+0.1,at,0.6);}
            } else {
                // Both reorder and remove an earlier instance while the surviving
                // instances have exactly unchanged numeric dependencies.
                let order:&[usize]=match (frame/30)%6 {0=>&[0,1,2],1=>&[1,2],2=>&[2,1],3=>&[0,2,1],4=>&[2],_=>&[1,0]};
                for &instance in order {
                    puppet.draw(&clip,0.,true,Transform {translation:vec3(instance as f32*65.-65.,0.,0.),
                        rotation:Quat::from_rotation_z(instance as f32*0.5)},1.,false);
                }
            }
            skin.dispatch();skin.collect();skin.assert_consumed();drop(skin);
            scene.finish_fx(true);set_default_camera();
            let image=get_screen_data();
            let hash=format!("{:x}",Sha256::digest(&image.bytes));
            crate::sim_maps::probe::end(frame,case,true,&(case,frame));
            println!("SKIN_EXACT_IMAGE {}",serde_json::json!({"case":case,"frame":frame,"sha256":hash}));
            if [0,30,59,60,90,119,120,150,179].contains(&frame) {
                crate::viewer::save_capture(std::path::Path::new(&format!("private/exact-frames/{case}-{frame}.png")))?;
            }
            next_frame().await;
        }
    }
    println!("PASS skin exact fixtures: 900 intermediate GPU and image frames");
    Ok(())
}
