//! Deterministic regressions for `actor_consumer_frustum_cull`.
//!
//! The key releases the availability guard that kept frozen-capable and overlay Puppet
//! bodies prepared. That may remove skinning, output reservation and publication from an
//! off-screen body, but it must not change the shared frame vertex budget: admission of
//! every later group stays identical to the OFF arm, and a body that comes back into view
//! while animating is prepared from its current pose exactly as it would have been.
//!
//! These tests drive `overlap::handle` with the visibility decision `draw_skin_deformed`
//! makes; they skip only its `skin_requires_preservation` texture probe, which needs a GL
//! context. A registered surface's `preserve_skin` is that probe's answer.
use super::*;
use crate::character::skinning::overlap::{self, startup, Frame};
use crate::materials::{Blend, Stage};
use crate::render::vita_renderer::with_config;
use crate::render_fx::{actor_publication as ap, test_support as ts};
use crate::skeletal::Skeleton;
use std::cell::RefCell;

const LAYERS: usize = 2;
const VERTICES: usize = 6;
/// `vertices * layers`, what one published surface reserves from the frame budget.
const BODY_CHARGE: usize = LAYERS * VERTICES;
const FX_CHARGE: usize = 3;

thread_local! { static KEEP: RefCell<Vec<std::rc::Rc<crate::render_fx::Surface>>> = RefCell::default(); }

struct Body { skeleton: Skeleton, pose: Vec<Transform>, texture: Texture2D }

fn stage(environment: bool, dot_alpha: bool) -> Stage {
    Stage { images: vec!["a".into()], environment,
        dot_alpha: if dot_alpha { Some(vec2(0.2, 0.9)) } else { None }, ..Default::default() }
}

/// One two-bone body with one surface. `model` picks distinct textures and a distinct
/// registered surface, so two bodies in one frame never share a publication key.
fn body(model: u32, environment: bool, dot_alpha: bool) -> Body {
    let pose = vec![
        Transform { rotation: Quat::from_rotation_z(0.21), translation: Vec3::ZERO },
        Transform { rotation: Quat::from_rotation_x(-0.37), translation: vec3(0., 1.5, 0.) },
    ];
    let skin = (0..VERTICES).map(|i| {
        let t = i as f32;
        crate::skeletal::SkinVertex { uv: vec2(0.07 * t, 0.31 - 0.02 * t), weights: vec![
            crate::skeletal::Weight { bone: 0, amount: 0.6, offset: vec3(t * 0.25, 0., 0.5) },
            crate::skeletal::Weight { bone: 1, amount: 0.4, offset: vec3(0., -t * 0.4, 0.25) }] }
    }).collect();
    let indices: Vec<u16> = (0..VERTICES as u16 / 3).flat_map(|t| { let b = 3 * t; [b, b + 1, b + 2] }).collect();
    let skeleton = Skeleton {
        bones: vec![crate::skeletal::Bone { parent: None, name: "root".into() },
            crate::skeletal::Bone { parent: Some(0), name: "child".into() }],
        surfaces: vec![crate::skeletal::Surface::new(format!("body{model}"), skin, indices)],
        // world_bounds_from_pose needs one box per bone before it can decide visibility.
        skin_bounds: vec![(Vec3::splat(-0.6), Vec3::splat(0.6)); 2],
    };
    let texture = ts::texture(4000 + 16 * model);
    let layers = (0..LAYERS).map(|l| (stage(environment, dot_alpha), vec![ts::texture(4001 + 16 * model + l as u32)])).collect();
    KEEP.with(|k| k.borrow_mut().push(ts::register(&texture, layers, false)));
    Body { skeleton, pose, texture }
}

fn mesh(body: &Body) -> Option<Mesh> {
    let cpu = body.skeleton.surfaces[0].cpu();
    Some(Mesh { upload_identity: None, surface: ts::surface(&body.texture), vertices: cpu.bind_vertices.clone(), indices: cpu.indices.clone(),
        texture: Some(body.texture.clone()) })
}

fn params(body: &Body, at: Transform) -> Params<'_> {
    Params { pose: &body.pose, scales: &[], palette: &[], transform: at, scale: 1.,
        flat: false, fullbright: false, alpha: 1. }
}

fn offscreen() -> Transform { Transform { rotation: Quat::IDENTITY, translation: vec3(4000., 0., -5.) } }
fn inside() -> Transform { Transform { rotation: Quat::IDENTITY, translation: vec3(0., 0., -6.) } }

/// Exactly what `Puppet::draw_pose` wraps around one body (availability guard plus the
/// budget marker the consumer key adds), followed by `draw_skin_deformed`'s visibility
/// decision and the real gather.
fn draw(body: &Body, meshes: &mut [Option<Mesh>], at: Transform, guarded: bool, charge: bool) -> bool {
    let released = consumer_body_culling(false);
    let _visibility = CullGuard::visual_preserve(guarded && !released);
    let _budget = CullBudget::released_by_key(charge && guarded && released);
    let config = crate::render::vita_renderer::config();
    let (cull, _) = visual_options(config.skin_frustum_cull, config.flat_actor_light, meshes);
    let visible = !(cull && outside_skin_bounds(cull, &body.skeleton.skin_bounds, &body.pose, &[],
        rotation(at.rotation), at.translation, 1.));
    assert_eq!(overlap::handle(&body.skeleton, meshes, params(body, at), visible), Some(visible));
    visible
}

fn fx() {
    crate::render_fx::effect(&Mesh { upload_identity: None, surface: None,
        vertices: vec![Vertex::new(0., 0., 0., 0.5, 0.5, WHITE); FX_CHARGE],
        indices: vec![0, 1, 2], texture: None }, Blend::Alpha);
}

fn clean() { overlap::reset_publications(); ts::clear(); KEEP.with(|k| k.borrow_mut().clear()); }

fn begin_frame() {
    ts::begin_direct(Vec3::ZERO, vec3(0., 0., -1.), 0.5, Default::default());
    crate::render_fx::set_projection(Mat4::perspective_rh_gl(std::f32::consts::FRAC_PI_2, 1., 1., 100.));
    crate::render_fx::retained::host_reset();
}

struct Run { frames: Vec<ts::FrameSnapshot>, visible: Vec<bool> }

/// One arm: a guarded body, then a visible body and an FX group, per line. `lines` names
/// whether the guarded body is inside the view and which animated pose it holds.
fn run(charge: bool, budget: usize, lines: &[(bool, usize)]) -> Run {
    startup(1, false);
    clean();
    let mut guarded = body(0, false, false);
    let later = body(1, false, false);
    let mut guarded_meshes = vec![mesh(&guarded)];
    let mut later_meshes = vec![mesh(&later)];
    let mut out = Run { frames: Vec::new(), visible: Vec::new() };
    for &(on_screen, pose_frame) in lines {
        let t = pose_frame as f32 * 0.37;
        guarded.pose = vec![
            Transform { rotation: Quat::from_rotation_z(0.21 + t), translation: vec3(0.1 * t, 0., 0.) },
            Transform { rotation: Quat::from_rotation_x(-0.37 - t), translation: vec3(0., 1.5 + 0.2 * t, 0.3 * t) },
        ];
        begin_frame();
        ts::set_frame_vertices(budget);
        let mut frame = Frame::begin();
        let visible = draw(&guarded, &mut guarded_meshes, if on_screen { inside() } else { offscreen() }, true, charge);
        draw(&later, &mut later_meshes, inside(), false, charge);
        fx();
        frame.dispatch();
        frame.collect();
        drop(frame);
        out.frames.push(ts::snapshot());
        out.visible.push(visible);
        ts::end_frame();
    }
    clean();
    out
}

fn arm(on: bool, charge: bool, budget: usize, lines: &[(bool, usize)]) -> Run {
    with_config(|c| { c.actor_consumer_frustum_cull = on; }, || run(charge, budget, lines))
}

/// One frame of the same draw with an active consumer scope or a normal-driven layer and
/// no budget pressure. Returns the frame output and the visibility the draw decided.
fn one(on: bool, model: u32, environment: bool, dot_alpha: bool, consumer: Option<u32>) -> (ts::FrameSnapshot, bool) {
    with_config(|c| c.actor_consumer_frustum_cull = on, || {
        startup(1, false);
        clean();
        let body = body(model, environment, dot_alpha);
        let mut meshes = vec![mesh(&body)];
        begin_frame();
        ts::set_frame_vertices(0);
        let mut frame = Frame::begin();
        let visible = {
            let _consumer = consumer.map(|reason| CullGuard::reason(true, reason));
            draw(&body, &mut meshes, offscreen(), true, true)
        };
        frame.dispatch(); frame.collect(); drop(frame);
        let out = (ts::snapshot(), visible);
        ts::end_frame();
        clean();
        out
    })
}

#[test]
fn a_culled_body_charges_exactly_what_off_reserves_so_later_admission_matches() {
    // A guarded body, then a visible body and FX that together just fit OFF's accounting.
    let budget = ts::MAX - (BODY_CHARGE + BODY_CHARGE + FX_CHARGE);
    let off = arm(false, true, budget, &[(false, 0)]);
    let on = arm(true, true, budget, &[(false, 0)]);
    assert_eq!(off.frames[0].vertices, ts::MAX, "the OFF arm reserved the whole frame");
    assert_eq!(on.frames[0].vertices, ts::MAX, "the culled body still charged the OFF reservation");
    assert_eq!((off.frames[0].dropped, on.frames[0].dropped), (0, 0));
    assert_eq!(off.frames[0].groups.len(), on.frames[0].groups.len() + 1,
        "the OFF arm published one more group for the body the key culled");
    assert_eq!(off.frames[0].groups[1..], on.frames[0].groups[..],
        "every later group is identical and admitted in both arms");

    // One vertex fewer: the later visible body no longer fits once the culled body's
    // reservation is charged, so ON must drop it too. Charging nothing admits it, which
    // is the divergence this regression exists for.
    let budget = ts::MAX - (BODY_CHARGE + BODY_CHARGE - 1);
    let off = arm(false, true, budget, &[(false, 0)]);
    let on = arm(true, true, budget, &[(false, 0)]);
    let uncharged = arm(true, false, budget, &[(false, 0)]);
    assert_eq!(off.frames[0].vertices, on.frames[0].vertices, "the budget usage differs");
    assert_eq!(off.frames[0].dropped, 1, "the later body is over budget in OFF");
    assert_eq!(on.frames[0].dropped, 1, "the culled body's charge makes ON drop the same group");
    assert_eq!(off.frames[0].groups[1..], on.frames[0].groups[..], "the same later groups survive in both arms");
    assert_eq!(uncharged.frames[0].dropped, 0, "negative control: skipping the charge admits the later group");
    assert_eq!(uncharged.frames[0].groups.len(), 2, "negative control: the later body and FX are published");
    assert_eq!(uncharged.frames[0].groups[0].1.len(), LAYERS, "negative control: the published group is the later body");
}

#[test]
fn an_off_screen_body_that_comes_back_while_animating_matches_the_always_prepared_arm() {
    // Frames 0..3 are outside the view, frames 4..7 inside; the pose changes every frame.
    let lines: Vec<(bool, usize)> = (0..8).map(|frame| (frame >= 4, frame)).collect();
    let off = arm(false, true, 0, &lines);
    let on = arm(true, true, 0, &lines);
    for (frame, (off, on)) in off.frames.iter().zip(&on.frames).enumerate() {
        assert_eq!(off.vertices, on.vertices, "budget admission differs at frame {frame}");
        assert_eq!(off.dropped, on.dropped, "dropped groups differ at frame {frame}");
    }
    for frame in 0..4 {
        assert!(off.visible[frame], "the OFF arm prepared the off-screen body at frame {frame}");
        assert!(!on.visible[frame], "the key did not cull the off-screen body at frame {frame}");
        assert_eq!(off.frames[frame].groups.len(), on.frames[frame].groups.len() + 1,
            "one body group differs at frame {frame}");
        assert_eq!(off.frames[frame].groups[1..], on.frames[frame].groups[..],
            "every later group differs at frame {frame}");
    }
    for frame in 4..8 {
        assert!(on.visible[frame], "the body is visible at frame {frame}");
        assert_eq!(off.frames[frame], on.frames[frame],
            "re-entering while animating differs from the always-prepared arm at frame {frame}");
    }
}

#[test]
fn active_mutable_consumers_keep_the_body_prepared_and_change_nothing() {
    for (name, model, reason) in [("frozen", 10, crate::phase_census::FROZEN), ("electric", 11, crate::phase_census::ELECTRIC)] {
        let off = one(false, model, false, false, Some(reason));
        let on = one(true, model, false, false, Some(reason));
        assert!(on.1, "{name}: an active consumer keeps the body visible");
        assert_eq!(off.0, on.0, "{name}: an active consumer restores the OFF output exactly");
    }
}

#[test]
fn env_or_dot_alpha_layers_keep_the_body_prepared_and_change_nothing() {
    for (model, name, environment, dot_alpha) in [(20, "environment", true, false), (21, "dot_alpha", false, true)] {
        let off = one(false, model, environment, dot_alpha, None);
        let on = one(true, model, environment, dot_alpha, None);
        assert!(on.1, "{name}: a normal-driven layer keeps the inherited visibility policy");
        assert_eq!(off.0, on.0, "{name}: the ON output matches OFF exactly");
    }
}

#[test]
fn the_key_requires_every_prerequisite_and_yields_to_mutable_consumers() {
    assert!(!consumer_body_culling(false), "no single preparation, no key decision");
    ts::begin_direct(Vec3::ZERO, vec3(0., 0., -1.), 0., Default::default());
    struct End;
    impl Drop for End { fn drop(&mut self) { ap::end(); ts::end_frame(); } }
    ap::begin();
    let _end = End;
    overlap::with_routing_control_gather(|| {
        assert!(overlap::single_preparing());
        assert!(!consumer_body_culling(false), "disabled prerequisite: the key defaults off");
        with_config(|c| c.actor_consumer_frustum_cull = true, || {
            assert!(consumer_body_culling(false), "key on and nothing consumes the live mesh");
            with_config(|c| c.skin_actual_consumers = false, ||
                assert!(!consumer_body_culling(false), "actually-declared consumers are required"));
            with_config(|c| c.flat_actor_light = true, ||
                assert!(!consumer_body_culling(false), "the flat-light override keeps the old policy"));
            assert!(consumer_body_culling(false), "nested prerequisites restore on unwind");
            assert!(!consumer_body_culling(true), "afterimage normal inheritance keeps the old path");
            {
                let _frozen = CullGuard::publication_reason(true, crate::phase_census::FROZEN);
                assert!(!consumer_body_culling(false), "an active frozen consumer keeps the body prepared");
            }
            {
                let _electric = CullGuard::reason(true, crate::phase_census::ELECTRIC);
                assert!(!consumer_body_culling(false), "an active electric consumer keeps the body prepared");
            }
            assert!(consumer_body_culling(false), "consumer scopes unwind");
            assert!(!cull_owes_budget(), "the budget marker is scoped to one body");
            {
                let _mark = CullBudget::released_by_key(true);
                assert!(cull_owes_budget());
                {
                    let _inner = CullBudget::released_by_key(true);
                    assert!(cull_owes_budget());
                }
                assert!(cull_owes_budget(), "an inner marker restores the outer value");
            }
            assert!(!cull_owes_budget());
        });
        assert!(!consumer_body_culling(false), "the config override is scoped");
    });
}


