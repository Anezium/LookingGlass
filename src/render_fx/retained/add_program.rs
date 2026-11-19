//! Lean fragment programs for additive effect packets, each behind its own key, default off.
//!
//! An additive effect packet (`render_fx::effect`, world puffs) is unlit, unclamped and uncut,
//! with no dissolve, ghost, power tint or filter fade, `FxModel = 1` and `FogMode = 1`. For such
//! a packet most of `render::FRAGMENT` cannot run or multiplies by one:
//!
//! - both `gl_FragCoord` discards test `FxAppearance.x > 0` and `FxAppearance.y > 0.5`: false;
//! - `ClampUV = 0` makes the coordinate `mix` return `uv` (line kept, see below);
//! - `Cutout = 0` fails every cutout range, so neither cutout `discard` runs;
//! - `FxModel = 1` skips the light atlas fetch, and `Lit = 0` makes the lighting factor one;
//! - the packed dynamic lights are zero (`lighting::packed`), so that term adds zero;
//! - `FxAppearance.z` and `.w` are zero: no tint, no filter fade;
//! - `SkyFade` is zero in every retained payload: no backdrop fetch.
//!
//! `fx_add_lean` keeps the coordinate line, the texture fetch, the alpha product, one cutout
//! test, `diffuse.rgb*Gain.rgb`, the vertex colour product and the unchanged `fogged` call.
//! Nothing else is compiled.
//!
//! The texture read is the full program's line, so texturing is untouched. The cutout test is
//! `if(Cutout>0.5&&alpha<=0.0)discard;`, the first clause of the full program's chain without
//! its upper bound. An admitted packet has `Cutout = 0`, so it never runs, but it has to be
//! there: a program with no alpha-dependent `discard` makes the host GPU shade tiny sprites
//! through another path. A first version without any `discard` failed the strict pixel oracle
//! on both potears1 tapes (waterfall drops a few pixels wide, up to 9/255 on about a hundred
//! pixels, same state, packets and per-draw fragment counts). Bisected on the host with
//! `LOOKING_GLASS_FX_ADD_SOURCES`: any variant with an alpha-dependent `discard` is exact,
//! with or without the coordinate line; any variant without one is not.
//!
//! `fx_add_fog_free` (needs `fx_add_lean`) also drops the `fogged` call when it returns its
//! argument for every fragment of the packet: distance and liquid fog are off, and each fog
//! volume is either off or separated from every eye-to-triangle hull by an axis plane, so
//! `fogLength` is zero and `mix(rgb, x, 0)` is `rgb`.
use super::*;

pub(super) const LEAN: &str = r#"#version 100
precision mediump float;
varying highp vec2 uv; varying highp vec3 lm; varying lowp vec4 color;
uniform sampler2D Texture; uniform float ClampUV; uniform float Cutout; uniform vec4 Gain; uniform float FogMode;
// FOG
void main(){
 vec2 coords=mix(uv,clamp(uv,vec2(0.001),vec2(0.999)),ClampUV);vec4 diffuse=texture2D(Texture,coords);
 float alpha=diffuse.a*Gain.a*color.a;
 if(Cutout>0.5&&alpha<=0.0)discard;
 vec3 rgb=diffuse.rgb*Gain.rgb;
 rgb*=color.rgb;
 rgb=fogged(rgb,FogMode);
 gl_FragColor=vec4(rgb,alpha);
}"#;
pub(super) const FOG_FREE: &str = r#"#version 100
precision mediump float;
varying highp vec2 uv; varying highp vec3 lm; varying lowp vec4 color;
varying highp vec3 worldPosition;
uniform sampler2D Texture; uniform float ClampUV; uniform float Cutout; uniform vec4 Gain;
void main(){
 vec2 coords=mix(uv,clamp(uv,vec2(0.001),vec2(0.999)),ClampUV);vec4 diffuse=texture2D(Texture,coords);
 float alpha=diffuse.a*Gain.a*color.a;
 if(Cutout>0.5&&alpha<=0.0)discard;
 vec3 rgb=diffuse.rgb*Gain.rgb;
 rgb*=color.rgb;
 gl_FragColor=vec4(rgb,alpha);
}"#;
/// The two program sources. A host probe build can read replacements from a folder
/// (`lean.frag`, `fog-free.frag`) while a pixel difference is being attributed. The folder
/// must be absolute (the game runs from another directory); a missing file stops the run,
/// because a silent fallback would report the built-in programs under the variant's name.
/// Never compiled for the console.
pub(super) fn sources() -> [String; 2] {
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    if let Some(folder) = std::env::var_os("LOOKING_GLASS_FX_ADD_SOURCES") {
        let read = |name: &str| std::fs::read_to_string(std::path::Path::new(&folder).join(name))
            .unwrap_or_else(|e| panic!("LOOKING_GLASS_FX_ADD_SOURCES {name}: {e}"));
        return [read("lean.frag"), read("fog-free.frag")];
    }
    [LEAN.to_owned(), FOG_FREE.to_owned()]
}
pub(super) const NAMES: [&str; 2] = ["retained-fx-add-lean", "retained-fx-add-fog-free"];

/// Blend indices of `Blend::Add` and `Blend::AlphaAdd`, the `fx_add` class of `gpu_timing`.
pub(super) fn additive(blend_index: usize) -> bool { blend_index == 2 || blend_index == 5 }

/// A packet for which every line `LEAN` leaves out of `render::FRAGMENT` is a no-op.
pub(super) fn lean_pass(p: &Pass) -> bool {
    additive(p.blend_index) && lite_pass(p) && p.model && p.fog == 1.
}

// Offsets into the retained uniform block (`UNIFORM_FIELDS`).
const EYE: usize = 116;
const DISTANCE_W: usize = 122;
const LIQUID_W: usize = 126;
const VOLUMES: [(usize, usize, usize); 2] = [(127, 130, 136), (137, 140, 146)];
/// Scanning more triangles than this costs more than it can save; the packet keeps `LEAN`.
const MAX_SCAN_INDICES: usize = 6144;

/// A triangle and the eye lie strictly on one side of an axis plane of the box, with `margin`.
fn hull_misses(eye: Vec3, corners: [Vec3; 3], lo: Vec3, hi: Vec3) -> bool {
    (0..3).any(|axis| {
        let (e, l, h) = (eye[axis], lo[axis], hi[axis]);
        let c = corners.map(|c| c[axis]);
        let scale = c.iter().fold(e.abs().max(l.abs()).max(h.abs()), |m, v| m.max(v.abs()));
        let margin = 4. + scale * 1e-3;
        (e < l - margin && c.iter().all(|&v| v < l - margin))
            || (e > h + margin && c.iter().all(|&v| v > h + margin))
    })
}

/// `fogged` returns its argument for every fragment of this packet under payload `u`.
pub(super) fn fog_free(p: &Pass, u: &[f32; UNIFORM_FLOATS]) -> bool {
    // `if(X.w>0.0)` guards each fog term; NaN fails the guard as it fails this test.
    if u[DISTANCE_W] > 0. || u[LIQUID_W] > 0. { return false; }
    let eye = Vec3::from_slice(&u[EYE..EYE + 3]);
    if !eye.is_finite() { return false; }
    let mut scanned = false;
    for (lo, hi, w) in VOLUMES {
        if !(u[w] > 0.) { continue; }
        let (lo, hi) = (Vec3::from_slice(&u[lo..lo + 3]), Vec3::from_slice(&u[hi..hi + 3]));
        if !lo.is_finite() || !hi.is_finite() { return false; }
        let mesh = p.geometry();
        if mesh.indices.len() > MAX_SCAN_INDICES { return false; }
        scanned = true;
        let position = |i: u16| mesh.vertices.get(i as usize).map(|v| v.position);
        for t in mesh.indices.chunks_exact(3) {
            let (Some(a), Some(b), Some(c)) = (position(t[0]), position(t[1]), position(t[2])) else { return false; };
            if !(a.is_finite() && b.is_finite() && c.is_finite()) || !hull_misses(eye, [a, b, c], lo, hi) { return false; }
        }
    }
    if scanned { count("vita_fx_add_fog_volume_scans", 1); }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    // Built field by field: `render_fx::pass` asks for a world material outside a retained frame.
    fn pass() -> Pass {
        Pass {
            gpu_skin: None,
            _game_thread: crate::game_thread::GameThread::default(), geometry: None, effect: true,
            blend_index: crate::render::blend_index(crate::materials::Blend::AlphaAdd), depth_equal: false,
            #[cfg(target_os = "vita")] world_static: None,
            #[cfg(target_os = "vita")] world_geometry_counted: false,
            mesh: Mesh { upload_identity: None, surface: None,
                vertices: vec![Vertex::new(0., 0., 0., 0., 0., WHITE), Vertex::new(10., 0., 0., 1., 0., WHITE),
                    Vertex::new(10., 10., 0., 1., 1., WHITE), Vertex::new(0., 10., 0., 0., 1., WHITE)],
                indices: vec![0, 1, 2, 0, 2, 3], texture: None },
            material: None, lit: false, clamp: false, cutout: 0., cutout_reference: 0.5, gain: Vec4::ONE,
            fog: crate::render_fx::fog(crate::materials::Blend::AlphaAdd), model: true,
            appearance: Default::default(), filter_fade: 0.,
        }
    }
    fn payload(eye: Vec3) -> [f32; UNIFORM_FLOATS] {
        let mut u = [0.; UNIFORM_FLOATS];
        u[EYE..EYE + 3].copy_from_slice(&eye.to_array());
        u
    }
    fn volume(u: &mut [f32; UNIFORM_FLOATS], which: usize, lo: Vec3, hi: Vec3, w: f32) {
        let (l, h, at) = VOLUMES[which];
        u[l..l + 3].copy_from_slice(&lo.to_array());
        u[h..h + 3].copy_from_slice(&hi.to_array());
        u[at] = w;
    }
    #[test]
    fn offsets_name_the_fog_fields_of_the_retained_block() {
        let end = |name: &str| UNIFORM_FIELDS.iter().find(|f| f.0 == name).unwrap().1;
        assert_eq!(end("FogEye"), EYE + 3);
        assert_eq!(end("DistanceFog"), DISTANCE_W + 1);
        assert_eq!(end("LiquidFog"), LIQUID_W + 1);
        assert_eq!((end("FogAMin"), end("FogAMax"), end("FogA")), (VOLUMES[0].0 + 3, VOLUMES[0].1 + 3, VOLUMES[0].2 + 1));
        assert_eq!((end("FogBMin"), end("FogBMax"), end("FogB")), (VOLUMES[1].0 + 3, VOLUMES[1].1 + 3, VOLUMES[1].2 + 1));
    }
    #[test]
    fn only_plain_additive_effect_packets_are_admitted() {
        assert!(lean_pass(&pass()));
        for (name, adjust) in [
            ("alpha blend", (|p: &mut Pass| p.blend_index = 1) as fn(&mut Pass)),
            ("colour add", |p: &mut Pass| p.blend_index = 6),
            ("opaque", |p: &mut Pass| p.blend_index = 0),
            ("actor", |p: &mut Pass| p.effect = false),
            ("lit", |p: &mut Pass| p.lit = true),
            ("clamp", |p: &mut Pass| p.clamp = true),
            ("cutout", |p: &mut Pass| p.cutout = 1.),
            ("dissolve", |p: &mut Pass| p.appearance.dissolve = 0.001),
            ("ghost", |p: &mut Pass| p.appearance.ghost = true),
            ("power", |p: &mut Pass| p.appearance.power = 1.),
            ("filter fade", |p: &mut Pass| p.filter_fade = 0.25),
            ("world model", |p: &mut Pass| p.model = false),
            ("filter fog", |p: &mut Pass| p.fog = 2.),
            ("no fog", |p: &mut Pass| p.fog = -1.),
        ] {
            let mut p = pass();
            adjust(&mut p);
            assert!(!lean_pass(&p), "{name} was admitted");
        }
        let mut add = pass();
        add.blend_index = 2;
        assert!(lean_pass(&add), "Blend::Add is the other fx_add index");
    }
    #[test]
    fn the_lean_sources_keep_the_full_program_arithmetic_and_nothing_else() {
        let full = crate::render::FRAGMENT;
        for kept in ["float alpha=diffuse.a*Gain.a*color.a;", "rgb=fogged(rgb,FogMode);", "gl_FragColor=vec4(rgb,alpha);"] {
            assert!(full.contains(kept), "{kept}");
            assert!(LEAN.contains(kept), "{kept}");
        }
        // The texture read and the alpha product are the full program's lines, byte for byte.
        let verbatim: Vec<&str> = full.lines().filter(|l| l.contains("texture2D(Texture,coords)")
            || l.contains("float alpha=")).collect();
        assert_eq!(verbatim.len(), 2);
        // One alpha-dependent cutout test stays: the first clause of the full chain. `lean_pass`
        // admits only `Cutout < 0.5`, so it cannot run.
        assert!(full.contains("if((Cutout>0.5&&Cutout<1.5&&alpha<=0.0)||"));
        for source in [LEAN, FOG_FREE] {
            for line in &verbatim { assert_eq!(source.lines().filter(|l| l == line).count(), 1, "{line}"); }
            assert_eq!(source.matches("discard").count(), 1);
            assert!(source.contains("\n if(Cutout>0.5&&alpha<=0.0)discard;\n"));
            let (read, test) = (source.find("float alpha=").unwrap(), source.find("discard").unwrap());
            assert!(read < test, "the test reads the sampled alpha");
        }
        let mut cut = pass();
        cut.cutout = 0.5;
        assert!(!lean_pass(&cut), "a packet that could reach the kept test is not admitted");
        assert!(full.contains("rgb=diffuse.rgb*mix(vec3(1.0),lighting,Lit*(1.0-Fullbright))*Gain.rgb;"));
        assert!(full.contains("if(FxModel>0.5&&Lit<0.5)rgb*=color.rgb;"));
        for source in [LEAN, FOG_FREE] {
            assert!(source.contains("vec3 rgb=diffuse.rgb*Gain.rgb;") && source.contains(" rgb*=color.rgb;"));
            for gone in ["LightAtlas", "SkyBackdrop", "dynamicLight", "FxAppearance", "gl_FragCoord", "lighting", "Fullbright", "CutoutReference"] {
                assert!(!source.contains(gone), "{gone}");
            }
        }
        assert_eq!(FOG_FREE, LEAN.replace(" uniform float FogMode;", "").replace("\n// FOG", "")
            .replace("varying lowp vec4 color;\n", "varying lowp vec4 color;\nvarying highp vec3 worldPosition;\n")
            .replace(" rgb=fogged(rgb,FogMode);\n", ""));
        assert_eq!(sources(), [LEAN.to_owned(), FOG_FREE.to_owned()]);
        assert!(!FOG_FREE.contains("fog"));
        // The interpolant declarations match the full program's, in its order.
        let varyings = |source: &str| source.split(';').map(str::trim).filter(|d| d.contains("varying "))
            .map(|d| d.rsplit(' ').next().unwrap().to_owned()).collect::<Vec<_>>();
        let expected = varyings(&crate::environment::fragment(full));
        assert_eq!(expected, ["uv", "lm", "color", "worldPosition"]);
        assert_eq!(varyings(&crate::environment::fragment(LEAN)), expected);
        assert_eq!(varyings(FOG_FREE), expected);
    }
    #[test]
    fn fog_free_needs_every_term_off_or_every_triangle_clear_of_the_volume() {
        let p = pass();
        let eye = vec3(5., 5., 100.);
        assert!(fog_free(&p, &payload(eye)), "no fog term is on");
        let mut u = payload(eye);
        u[DISTANCE_W] = 2500.;
        assert!(!fog_free(&p, &u), "distance fog reads every fragment");
        let mut u = payload(eye);
        u[LIQUID_W] = 400.;
        assert!(!fog_free(&p, &u), "liquid fog reads every fragment");
        // A volume well below the quad and the eye: every ray stays above its top face.
        let mut u = payload(eye);
        volume(&mut u, 0, vec3(-500., -500., -300.), vec3(500., 500., -100.), 800.);
        assert!(fog_free(&p, &u));
        // The same box as volume B, and both together.
        let mut b = payload(eye);
        volume(&mut b, 1, vec3(-500., -500., -300.), vec3(500., 500., -100.), 800.);
        assert!(fog_free(&p, &b));
        volume(&mut u, 1, vec3(-500., -500., 20.), vec3(500., 500., 60.), 800.);
        assert!(!fog_free(&p, &u), "volume B lies between the eye and the quad");
        // Inside the margin is not a proof.
        let mut u = payload(eye);
        volume(&mut u, 0, vec3(-500., -500., -300.), vec3(500., 500., -2.), 800.);
        assert!(!fog_free(&p, &u));
        // The eye inside the volume, the quad outside.
        let mut u = payload(vec3(5., 5., -200.));
        volume(&mut u, 0, vec3(-500., -500., -300.), vec3(500., 500., -100.), 800.);
        assert!(!fog_free(&p, &u));
        // A volume that is off is ignored whatever its box.
        let mut u = payload(eye);
        volume(&mut u, 0, vec3(-500., -500., -500.), vec3(500., 500., 500.), 0.);
        assert!(fog_free(&p, &u));
        // One triangle through the volume spoils the packet.
        let mut q = pass();
        q.mesh.vertices[2].position.z = -200.;
        let mut u = payload(eye);
        volume(&mut u, 0, vec3(-500., -500., -300.), vec3(500., 500., -100.), 800.);
        assert!(!fog_free(&q, &u));
        // Non-finite input is never a proof.
        let mut u = payload(vec3(f32::NAN, 0., 0.));
        volume(&mut u, 0, vec3(-500., -500., -300.), vec3(500., 500., -100.), 800.);
        assert!(!fog_free(&p, &u));
    }
    #[test]
    fn both_keys_are_off_by_default_and_strict() {
        let c = crate::render::vita_renderer::Config::default();
        assert!(!c.fx_add_lean && !c.fx_add_fog_free);
        for key in ["fx_add_lean", "fx_add_fog_free"] {
            assert!(serde_json::from_value::<crate::render::vita_renderer::Config>(serde_json::json!({key: 1})).is_err());
        }
    }
}
