//! Private CPU controls. Never included in a production/native executable.
//! Immediate primitives are intercepted here, not executed without a GL context.
use super::*;
use std::cell::Cell;

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static IMMEDIATE: RefCell<Vec<Immediate>> = const { RefCell::new(Vec::new()) };
}
#[derive(Debug, PartialEq)]
enum Immediate {
    Line([u32; 3], [u32; 3], [u32; 4]),
    Sphere([u32; 3], u32, Option<Texture2D>, [u32; 4]),
}
fn bits(v: Vec3) -> [u32; 3] { v.to_array().map(f32::to_bits) }
fn color_bits(c: Color) -> [u32; 4] { [c.r,c.g,c.b,c.a].map(f32::to_bits) }
pub(super) fn enabled() -> bool { ENABLED.with(Cell::get) }
pub(super) fn immediate_line(a: Vec3, b: Vec3, c: Color) {
    IMMEDIATE.with(|v| v.borrow_mut().push(Immediate::Line(bits(a),bits(b),color_bits(c))));
}
pub(super) fn immediate_sphere(p: Vec3, r: f32, t: Option<&Texture2D>, c: Color) {
    IMMEDIATE.with(|v| v.borrow_mut().push(Immediate::Sphere(bits(p),r.to_bits(),t.cloned(),color_bits(c))));
}
struct Scope { enabled: bool, frame: Option<Frame>, immediate: Option<Vec<Immediate>> }
impl Scope {
    fn new(enabled: bool, active: bool, direct: bool) -> Self {
        #[cfg(not(feature="vita_renderer"))] let _ = direct;
        Self {
            enabled: ENABLED.with(|s| s.replace(enabled)),
            frame: Some(FRAME.with(|s| s.replace(Frame { active,
                #[cfg(feature="vita_renderer")] direct,
                camera: Vec3::Z*10., direction: -Vec3::Z, ..Default::default() }))),
            immediate: Some(IMMEDIATE.with(|s| s.replace(Vec::new()))),
        }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        FRAME.with(|s| s.replace(self.frame.take().unwrap()));
        IMMEDIATE.with(|s| s.replace(self.immediate.take().unwrap()));
        ENABLED.with(|s| s.set(self.enabled));
    }
}
fn pair(texture: Option<&Texture2D>) {
    legacy_line(vec3(-1.,0.,0.), vec3(1.,0.,0.), Color::new(0.25,0.5,0.75,0.5));
    legacy_sphere(vec3(2.,3.,4.), 0.75, texture, Color::new(0.75,0.5,0.25,1.));
}

#[test]
fn routing_truth_table_and_exact_fallback_arguments() {
    let texture = test_support::texture(417);
    for enabled in [false,true] { for active in [false,true] { for direct in [false,true] {
        let _scope = Scope::new(enabled,active,direct);
        // Noncanonical floats test exact forwarding on the intercepted original path.
        let a=vec3(-0.,f32::from_bits(0x7fc01234),3.);
        let b=vec3(4.,5.,6.); let c=Color::new(-0.,0.25,0.75,1.);
        let queued=cfg!(feature="vita_renderer") && enabled && active && direct;
        if queued { pair(Some(&texture)); }
        else {
            legacy_line(a,b,c);
            legacy_sphere(a,-0.,Some(&texture),c);
        }
        assert_eq!(FRAME.with(|s|s.borrow().groups.len()),if queued {2} else {0},
            "FX_ROUTE_RETAINED_QUEUE enabled={enabled} active={active} direct={direct}");
        IMMEDIATE.with(|s| {
            let s=s.borrow();
            if queued { assert!(s.is_empty(),"FX_ROUTE_NO_IMMEDIATE"); }
            else { assert_eq!(*s,vec![Immediate::Line(bits(a),bits(b),color_bits(c)),
                Immediate::Sphere(bits(a),(-0.0f32).to_bits(),Some(texture.clone()),color_bits(c))],
                "FX_ROUTE_FALLBACK_ARGS"); }
        });
    }}}
}

#[cfg(feature="vita_renderer")]
#[test]
fn retained_adapter_produces_exact_existing_helper_packets() {
    let texture=test_support::texture(418);
    let adapter={let _scope=Scope::new(true,true,true); pair(Some(&texture)); test_support::snapshot()};
    let helper={let _scope=Scope::new(true,true,true);
        line(vec3(-1.,0.,0.),vec3(1.,0.,0.),Color::new(0.25,0.5,0.75,0.5));
        sphere(vec3(2.,3.,4.),0.75,Some(&texture),Color::new(0.75,0.5,0.25,1.));
        test_support::snapshot()};
    assert_eq!(adapter,helper,"FX_ROUTE_HELPER_PACKET_EQUIVALENCE");
    assert_eq!(adapter.groups.len(),2,"FX_ROUTE_RETAINED_QUEUE");
}

#[cfg(feature="vita_renderer")]
#[test]
fn packets_survive_with_geometry_texture_and_disclosed_alpha_policy() {
    let _scope=Scope::new(true,true,true);
    let texture=test_support::texture(419);
    pair(Some(&texture));
    FRAME.with(|s| {
        let f=s.borrow();
        assert_eq!(f.groups.len(),2,"FX_ROUTE_RETAINED_QUEUE");
        assert_eq!(f.vertices,4+117,"FX_ROUTE_VERTEX_CHARGE");
        assert_eq!(f.dropped,0);
        for g in &f.groups {
            assert!(g.transparent,"FX_ROUTE_ALPHA_POLICY");
            assert_eq!(g.passes.len(),1);
            let p=&g.passes[0];
            assert!(p.effect); assert!(p.material.is_none());
            assert_eq!(p.blend_index,crate::render::blend_index(Blend::Alpha));
        }
        let l=f.groups[0].passes[0].geometry();
        assert_eq!(l.vertices.len(),4); assert_eq!(l.indices,vec![0,1,2,0,2,3]);
        let expected_color: [u8;4] = Color::new(0.25,0.5,0.75,0.5).into();
        assert_eq!(l.vertices[0].color,expected_color);
        let p=&f.groups[1].passes[0]; let mesh=p.geometry();
        assert_eq!(mesh.vertices.len(),117); assert_eq!(mesh.indices.len(),576);
        assert_eq!(mesh.vertices[0].position,vec3(2.,3.,4.75));
        assert_eq!(p.mesh.texture.as_ref(),Some(&texture),"FX_ROUTE_TEXTURE_OWNER");
    });
    assert!(IMMEDIATE.with(|s|s.borrow().is_empty()),"FX_ROUTE_NO_IMMEDIATE");
}

#[cfg(feature="vita_renderer")]
#[test]
fn budget_refusal_is_explicit_and_never_uses_immediate_fallback() {
    let _scope=Scope::new(true,true,true);
    test_support::set_frame_vertices(MAX_VERTICES);
    pair(None);
    FRAME.with(|s| {let f=s.borrow(); assert_eq!(f.dropped,2,"FX_ROUTE_DROP_COUNT");
        assert!(f.groups.is_empty()); assert_eq!(f.vertices,MAX_VERTICES);});
    assert!(IMMEDIATE.with(|s|s.borrow().is_empty()),"FX_ROUTE_NO_BUDGET_FALLBACK");
}

#[cfg(feature="vita_renderer")]
#[test]
fn ant_depth_scope_restores_flags_even_on_unwind() {
    let _scope=Scope::new(true,true,true);
    for prior in [false,true] {
        FRAME.with(|s|s.borrow_mut().depth_read_only=prior);
        let result=std::panic::catch_unwind(||retained_depth_scope(|| {
            assert!(FRAME.with(|s|s.borrow().depth_read_only));
            legacy_sphere(Vec3::ZERO,1.,None,WHITE);
            panic!("FX_CONTROL_EXPECTED_UNWIND");
        }));
        assert!(result.is_err());
        assert_eq!(FRAME.with(|s|s.borrow().depth_read_only),prior,"FX_ROUTE_DEPTH_SCOPE_RESTORE");
    }
    assert_eq!(FRAME.with(|s|s.borrow().groups.len()),2,"FX_ROUTE_RETAINED_QUEUE");
}

#[test]
fn scoped_override_and_frame_restore_on_unwind() {
    let _outer=Scope::new(false,false,false);
    let result=std::panic::catch_unwind(|| {let _inner=Scope::new(true,true,true);
        panic!("FX_CONTROL_EXPECTED_UNWIND");});
    assert!(result.is_err()); assert!(!enabled()); assert!(!active());
    assert!(IMMEDIATE.with(|s|s.borrow().is_empty()));
}

#[test]
fn inactive_after_frame_uses_original_dispatch() {
    let _scope=Scope::new(true,true,true);
    test_support::end_frame();
    pair(None);
    assert_eq!(IMMEDIATE.with(|s|s.borrow().len()),2,"FX_ROUTE_END_FRAME_FALLBACK");
    assert!(FRAME.with(|s|s.borrow().groups.is_empty()));
}

#[cfg(feature="vita_renderer")]
#[test]
fn gather_only_is_suppressed_and_single_preparation_submits_once() {
    use crate::character::skinning::overlap;
    let _scope=Scope::new(true,true,true);
    assert!(!actor_publication::active());
    overlap::with_routing_control_gather(|| {
        assert!(overlap::gather_only()); pair(None);
        assert!(FRAME.with(|s|s.borrow().groups.is_empty()),"FX_ROUTE_GATHER_SUPPRESSION");
    });
    struct End;
    impl Drop for End { fn drop(&mut self) { actor_publication::end(); } }
    actor_publication::begin(); let _end=End;
    assert!(actor_publication::active(),"FX_CONTROL_SINGLE_PREPARATION_PREREQUISITES");
    overlap::with_routing_control_gather(|| {
        assert!(overlap::single_preparing()); assert!(!overlap::gather_only()); pair(None);
    });
    assert_eq!(FRAME.with(|s|s.borrow().groups.len()),2,"FX_ROUTE_SINGLE_PREPARATION_ONCE");
    assert!(IMMEDIATE.with(|s|s.borrow().is_empty()));
}

#[cfg(feature="vita_renderer")]
#[test]
fn option_default_strict_boolean_and_roundtrip() {
    use crate::render::vita_renderer::Config;
    assert!(!serde_json::from_str::<Config>("{}").unwrap().defer_immediate_fx);
    for flag in [false,true] {
        let c:Config=serde_json::from_value(serde_json::json!({"defer_immediate_fx":flag})).unwrap();
        assert_eq!(c.defer_immediate_fx,flag);
        let value=serde_json::to_value(&c).unwrap();
        assert_eq!(value["defer_immediate_fx"],serde_json::json!(flag));
        assert_eq!(serde_json::from_value::<Config>(value).unwrap().defer_immediate_fx,flag);
    }
    for wrong in [serde_json::json!(1),serde_json::json!("true"),serde_json::Value::Null] {
        assert!(serde_json::from_value::<Config>(serde_json::json!({"defer_immediate_fx":wrong})).is_err());
    }
}
