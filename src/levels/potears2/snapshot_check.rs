//! Real-asset body/effect and rejection controls for the complete potears2 art batch.
use super::*;
pub(super) async fn check(assets: &mut Assets) -> Result<()> {
    fn assert_send<T:Send>() {} assert_send::<ArtInput>();
    // The viewer's path: the capture source reads the controller and story numbers, the art
    // publishes the value. The source is taken before the call so it borrows no art.
    fn capture(art:&mut Art,o:&PoolTwo,story:&Story)->(usize,bool) {
        let source=art.capture_source().unwrap();let pose=source.story(story);
        art.publish_snapshot(source.capture(o,&pose))
    }
    let map=Bsp::parse(&assets.read("maps/potears2.bsp")?)?;
    let atmosphere=crate::environment::Atmosphere::default();let mut art=Art::load(assets)?;let mut checked=0;
    for case in 0..16 {
        let mut reference=None;
        for captured in [false,true] {
            art.clear_snapshot();
            let mut i=Interactions::load(&map)?;i.set_entry(assets,&map,"potears2",None)?;
            let o=super::super::owner(&mut i)?;
            let at=o.bill;let target=at+Vec3::Z*42.;
            let view=Camera3D {position:target+vec3(180.,-260.,120.),target,up:Vec3::Z,
                fovy:65_f32.to_radians(),z_near:2.,z_far:30000.,..Default::default()};
            o.saved.scene=Presentation::default();o.saved.scene.time=0.8;
            match case {
                1=>{o.saved.scene.phase=Phase::Approach;o.saved.scene.time=0.05;}
                2=>o.saved.scene.phase=Phase::Approach,
                3..=6=>{o.saved.scene.phase=Phase::Conversation;o.saved.scene.line=[0,1,6,12][case-3];o.saved.scene.acting=Some(13.3);o.saved.scene.run_time=20.;}
                7|8=>{o.saved.scene.phase=Phase::Ending;let mut e=SceneState::new(&ENDING);e.time=if case==7 {4.6}else{6.3};o.saved.scene.ending=Some(e);}
                9=>{o.saved.scene.phase=Phase::Conversation;o.saved.scene.skipping=true;}
                10=>o.saved.scene.phase=Phase::Done,
                11..=15=>{o.saved.fish.attack=Some([0.2,0.5,0.7,1.2,1.8][case-11]);o.saved.fish.at=at+Vec3::Z*35.;}
                _=>{}
            }
            for (n,g) in o.saved.guards.iter_mut().enumerate() {
                g.feet=at+Vec3::X*(n as f32*90.-40.);g.yaw=0.4;g.time=0.4;
                if case==0 && n==0 {g.frozen=true;g.health=0.;g.phase=crate::ant::Phase::Dead;}
                g.shots=vec![crate::ant::Bullet {at:at+vec3(10.,n as f32*20.,45.),grenade:n==1,direction:Vec3::X,age:0.2}];
                g.blasts=vec![crate::ant::Blast {at:at+vec3(25.,0.,25.),time:0.2}];
            }
            let mut story=Story::default();
            story.summon_hint(crate::story::Line {watches:Default::default(),actor:if case%2==0 {"billthelizard"}else{"fakeplayer"}.into(),
                path:"test.wav".into(),text:"Test".into(),duration:1.,
                lips:crate::facial::LipTrack::parse(b"1 8 128 128 128 128 128 128 128 128")?});
            story.tick(0.,false);story.tick(0.25,false);
            ensure!(story.mouth(&[if case%2==0 {"billthelizard"}else{"fakeplayer"}])>0.4,"Missing active lips");
            art.story_pose(&story);
            if captured {
                let (_,overflow)=capture(&mut art,o,&story);ensure!(!overflow && art.snapshot_ready(),"Fixture rejected {case}");
                let old=art.input.as_ref().unwrap().visit;art.input.as_mut().unwrap().visit=0;
                ensure!(!art.snapshot_ready() && !art.draw_snapshot(&atmosphere,view.position,true)
                    && !art.effects_snapshot(view.position,&atmosphere),"Stale visit accepted");
                art.input.as_mut().unwrap().visit=old;
                let index=art.input.as_ref().unwrap().draws.iter().position(|d|matches!(d,DrawInput::Puppet(..))).unwrap();
                let DrawInput::Puppet(slot,_,_)=&mut art.input.as_mut().unwrap().draws[index] else {unreachable!()};
                let saved=*slot;*slot=usize::MAX;ensure!(!art.snapshot_ready(),"Wrong resource accepted");
                let DrawInput::Puppet(slot,_,_)=&mut art.input.as_mut().unwrap().draws[index] else {unreachable!()};*slot=saved;
                art.bill.mouth(0.99);art.alice.mouth(0.99);art.bill.mouth_angle(77.);art.clock=999.;
                // Destroy every controller, including its collision and Story inputs.
                i.levels.clear();story=Story::default();let _=story;
            }
            clear_background(BLACK);set_camera(&view);crate::render_fx::begin_view(&view,2.5,&atmosphere,false);
            if captured {ensure!(art.draw_snapshot(&atmosphere,view.position,true),"Owned body rejected");ensure!(art.effects_snapshot(view.position,&atmosphere),"Owned effect rejected");}
            else {let o=super::super::owner(&mut i)?;art.draw(o,&atmosphere,view.position,true);art.effects(o,view.position,&atmosphere);}
            crate::render_fx::finish();set_default_camera();let image=get_screen_data();
            ensure!(image.bytes.chunks_exact(4).filter(|p|p[..3]!=[0,0,0]).count()>20,"Empty fixture {case}");
            if let Some(expected)=&reference {ensure!(*expected==image.bytes,"Potears2 pixels differ case {case}");checked+=1;}
            else {reference=Some(image.bytes);}
            next_frame().await;
        }
    }
    let mut i=Interactions::load(&map)?;i.set_entry(assets,&map,"potears2",None)?;let o=super::super::owner(&mut i)?;
    let guard=o.saved.guards[0].clone();o.saved.guards=vec![guard.clone();1025];
    ensure!(capture(&mut art,o,&Story::default())==(0,true) && art.input.is_none(),"Unbounded guard capture");
    o.saved.guards=vec![guard];o.saved.guards[0].shots=vec![crate::ant::Bullet {at:Vec3::ZERO,grenade:false,direction:Vec3::X,age:0.2};16385];
    ensure!(capture(&mut art,o,&Story::default())==(0,true) && art.input.is_none(),"Unbounded projectile capture");
    println!("PASS snapshot potears2: {checked} exact body/effect/lips/poisoned-controller controls; stale/resource/cast/projectile rejection");Ok(())
}
