//! Real-resource controls for the Ladybug-only encounter batch.
use super::*;
pub(crate) async fn check(assets: &mut Assets) -> Result<()> {
    fn assert_send<T: Send>() {}
    assert_send::<RenderInput>();
    let map = Bsp::parse(&assets.read("maps/potears1.bsp")?)?;
    let mut art = Art::load(assets)?;
    let atmosphere = crate::environment::Atmosphere::default();
    let view = Camera3D { position: vec3(150., -190., 130.), target: Vec3::Z * 60., up: Vec3::Z,
        fovy: 65_f32.to_radians(), z_near: 2., z_far: 30000., ..Default::default() };
    let mut checked = 0;
    for case in 0..10 {
        let mut reference = None;
        for owned in [false, true] {
            art.clear_snapshot();
            let mut s = Encounters::load(assets, &map, "potears1", None)?;
            anyhow::ensure!(s.ladybug.is_some(), "Missing timing");
            s.actors = vec![Actor { enabled: case != 2, name: "fixture".into(), active: case != 1,
                enemy: Enemy::Ladybug(crate::ladybug::Ladybug::new(Vec3::Z * 60., 0.4, 1., Vec::new())) }];
            let Enemy::Ladybug(b) = &mut s.actors[0].enemy else { unreachable!() };
            b.time = 0.4;
            b.carrying = !matches!(case, 3 | 9);
            b.electric = if case == 4 { 0.4 } else { 0. };
            if case == 5 { b.health = 0.; b.state = crate::ladybug::State::Dead; b.frozen = true; }
            if case == 6 { b.health = 0.; b.state = crate::ladybug::State::Dead; }
            if case == 7 { b.state = crate::ladybug::State::Attack; }
            if matches!(case, 8 | 9) { b.state = crate::ladybug::State::Pain; }
            b.acorns = serde_json::from_value(serde_json::json!([
                {"position":[30.,0.,70.],"velocity":[0.,0.,0.],"age":0.2}]))?;
            if owned {
                art.extract_snapshot(&s);
                anyhow::ensure!(art.snapshot_ready(), "Rejected fixture {case}");
                anyhow::ensure!(art.snapshot_bytes() > 0, "Empty byte accounting");
                // Accepted publication must survive complete removal of simulation actors.
                s.actors.clear();
            }
            clear_background(BLACK); set_camera(&view);
            crate::render_fx::begin_view(&view, 2.5, &atmosphere, false);
            if owned {
                anyhow::ensure!(art.draw_snapshot(true, &atmosphere, view.position), "Owned draw rejected");
            } else { art.draw(&s, true, &atmosphere, view.position); }
            crate::render_fx::finish(); set_default_camera();
            let image = get_screen_data();
            if case == 0 {
                anyhow::ensure!(image.bytes.chunks_exact(4).filter(|p| p[..3] != [0, 0, 0]).count() > 20,
                    "Empty active encounter fixture");
            }
            if let Some(expected) = &reference {
                anyhow::ensure!(*expected == image.bytes, "Encounter {case} pixels differ");
                checked += 1;
            } else { reference = Some(image.bytes); }
            next_frame().await;
        }
    }
    // A stale visit can never draw the retained batch.
    let mut s = Encounters::load(assets, &map, "potears1", None)?;
    s.actors = vec![Actor { enabled: true, name: "fixture".into(), active: true,
        enemy: Enemy::Ladybug(crate::ladybug::Ladybug::new(Vec3::Z * 60., 0., 1., Vec::new())) }];
    art.extract_snapshot(&s);
    anyhow::ensure!(art.snapshot_ready(), "Stale-visit fixture rejected");
    let old = art.input.as_ref().unwrap().visit;
    art.input.as_mut().unwrap().visit = 0;
    anyhow::ensure!(!art.draw_snapshot(true, &atmosphere, view.position), "Accepted stale visit");
    art.input.as_mut().unwrap().visit = old;
    art.clear_snapshot();
    anyhow::ensure!(!art.snapshot_ready() && art.snapshot_bytes() == 0, "Cleared batch retained");
    // Cast, acorn and shape bounds reject before publication.
    let a = Actor { enabled: true, name: "fixture".into(), active: true,
        enemy: Enemy::Ladybug(crate::ladybug::Ladybug::new(Vec3::ZERO, 0., 1., Vec::new())) };
    s.actors = vec![a.clone(); 1025];
    art.extract_snapshot(&s);
    anyhow::ensure!(!art.snapshot_ready() && art.input.as_ref().unwrap().actors.is_empty(),
        "Unbounded actor extraction");
    let mut a = a;
    let Enemy::Ladybug(b) = &mut a.enemy else { unreachable!() };
    b.acorns = serde_json::from_value(serde_json::json!([
        {"position":[0.,0.,0.],"velocity":[0.,0.,0.],"age":0.2}]))?;
    b.acorns = vec![b.acorns[0].clone(); 16385];
    s.actors = vec![a];
    art.extract_snapshot(&s);
    anyhow::ensure!(!art.snapshot_ready() && art.input.as_ref().unwrap().actors.is_empty(),
        "Unbounded acorn extraction");
    let Enemy::Ladybug(b) = &mut s.actors[0].enemy else { unreachable!() };
    b.acorns.clear(); b.time = f32::NAN;
    art.extract_snapshot(&s);
    anyhow::ensure!(!art.snapshot_ready(), "Nonfinite input accepted");
    let village = Bsp::parse(&assets.read("maps/gvillage.bsp")?)?;
    let mut other = Encounters::load(assets, &village, "gvillage", None)?;
    art.extract_snapshot(&other);
    anyhow::ensure!(!art.snapshot_ready(), "Unsupported species accepted");
    other.actors.clear();
    println!("PASS snapshot encounters: {checked} exact comparisons; stale visit, cast, acorn, nonfinite and unsupported-species rejection");
    Ok(())
}
