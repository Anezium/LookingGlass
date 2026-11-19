//! Lean fragment programs for actor packets, behind `actor_lean_shader`, default off.
//!
//! The actor program (`render::FRAGMENT` with eight dynamic lights and the fog block) is the
//! longest one the console runs. A model packet with no dynamic light and no appearance effect
//! executes most of it for nothing:
//!
//! - both `gl_FragCoord` discards test `FxAppearance.x > 0` and `FxAppearance.y > 0.5`: false;
//! - `FxModel = 1` fails `FxModel < 0.5`, so the light atlas is never read;
//! - the sixteen packed light vectors are zero (`lighting::packed`), so `dynamicLight` returns
//!   `vec3(0.0)` and `rgb += diffuse.rgb*vec3(0.0)*Gain.rgb` adds zero for a finite gain;
//! - `FxAppearance.z <= 0.5` and `FxAppearance.w == 0`: no tint, no filter fade;
//! - `FogMode <= 3.5` skips the source-colour fog branch, which holds a second copy of `fogged`;
//! - `SkyFade` is zero in every retained payload: no backdrop fetch.
//!
//! The lean source is `render::FRAGMENT` with exactly those lines removed. The texture read,
//! the alpha product, both cutout lines, the vertex lighting, the gain, the unlit colour
//! product and the `fogged` call stay byte for byte, so a packet that can reject fragments
//! still rejects the same ones. The one edited line is the fog call's guard, which loses the
//! `else` of the branch that went. `admitted` reads every condition from the packet's own
//! uniform payload, the words the program would have tested.
//!
//! The second program also drops the two cutout lines. It serves only packets that
//! `actor_discard_free_shader` or `actor_cutout_discard_free` already send to the discard-free
//! program, so it never widens the set of packets drawn without `discard`.
use super::*;

pub(super) const NAMES: [&str; 2] = ["retained-actor-lean", "retained-actor-lean-opaque"];

// Offsets into the retained uniform block (`UNIFORM_FIELDS`).
const GAIN: usize = 39;
const FOG_MODE: usize = 43;
const SKY_FADE: usize = 46;
const FX_MODEL: usize = 47;
const APPEARANCE: usize = 48;
const LIGHTS: usize = 52;

/// A `render::FRAGMENT` line that cannot run, or adds zero, for an admitted packet.
fn unreachable_line(line: &str) -> bool {
    ["FxAppearance.x", "FxAppearance.y", "LightAtlas,sampleUV", "dynamicLight(", "FxAppearance.z",
     "FxAppearance.w", "// Multiplicative actor layers", "// LIGHTS",
     "uniform sampler2D SkyBackdrop;"].iter().any(|marker| line.contains(marker))
}
/// The lean source: `discard_free` also removes the two cutout lines.
pub(super) fn fragment(discard_free: bool) -> String {
    let full = crate::render::FRAGMENT;
    // The backdrop block is the only multi-line statement that goes: two comment lines, the
    // test and its four body lines, up to the output write.
    let (head, rest) = full.split_once(" // The remastered Fortress exterior").expect("backdrop block");
    let (_, tail) = rest.split_once(" gl_FragColor=").expect("output write");
    let source = format!("{head} gl_FragColor={tail}");
    // The source-colour fog branch (`FogMode > 3.5`) goes with its comments; the plain fog
    // call keeps its own guard.
    let (head, rest) = source.split_once(" if(FogMode>3.5){").expect("source-colour fog branch");
    let (_, tail) = rest.split_once(" }else if(FogMode>=0.0)").expect("fog call");
    let source = format!("{head} if(FogMode>=0.0){tail}");
    source.lines().filter(|line| !unreachable_line(line) && !(discard_free && line.contains("discard;")))
        .map(|line| line.replace(" uniform sampler2D LightAtlas; uniform vec2 AtlasGrid;", "")
            .replace(" uniform vec4 FxAppearance;", ""))
        .collect::<Vec<_>>().join("\n")
}
/// The two program sources, `[lean, lean without discard]`. A host probe build can read
/// replacements from an absolute folder (`lean.frag`, `lean-opaque.frag`) while a pixel
/// difference is being attributed; a missing file stops the run. Never compiled for the console.
pub(super) fn sources() -> [String; 2] {
    #[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
    if let Some(folder) = std::env::var_os("LOOKING_GLASS_ACTOR_LEAN_SOURCES") {
        let read = |name: &str| std::fs::read_to_string(std::path::Path::new(&folder).join(name))
            .unwrap_or_else(|e| panic!("LOOKING_GLASS_ACTOR_LEAN_SOURCES {name}: {e}"));
        return [read("lean.frag"), read("lean-opaque.frag")];
    }
    [fragment(false), fragment(true)]
}
/// Every line the lean source leaves out is a no-op under payload `u`. Each test is the
/// negation of the shader's own guard, so NaN fails the guard here as it does there.
pub(super) fn admitted(p: &Pass, u: &[f32; UNIFORM_FLOATS]) -> bool {
    let a = &u[APPEARANCE..APPEARANCE + 4];
    !p.effect
        && !(a[0] > 0.) && !(a[1] > 0.5) && !(a[2] > 0.5) && !(a[3] > 0.) && !(a[3] < 0.)
        && !(u[FX_MODEL] < 0.5)
        && !(u[FOG_MODE] > 3.5)
        && !(u[SKY_FADE] > 0.5)
        && (0..8).all(|light| !(u[LIGHTS + light * 8 + 7] > 0.5))
        && u[GAIN..GAIN + 3].iter().all(|gain| gain.is_finite())
}
/// Host census line for one actor draw: the payload facts the selector reads and the route.
#[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
pub(super) fn census(p: &Pass, u: &[f32; UNIFORM_FLOATS], depth_write: bool, route: Route) {
    if p.effect || !crate::character::skinning::census::active() { return; }
    let surface = p.mesh.surface.as_ref();
    let mesh = p.geometry();
    println!("ACTOR_GPU2 {}", serde_json::json!({
        "frame": crate::sim_maps::probe::frame(), "label": surface.map(|s| s.label.clone()),
        "route": format!("{route:?}"), "admitted": admitted(p, u),
        "blend": p.blend_index, "equal": p.depth_equal, "depth_write": depth_write,
        "lit": p.lit, "clamp": p.clamp, "cutout": p.cutout, "cutout_reference": p.cutout_reference,
        "gain": p.gain.to_array(), "fog_mode": p.fog, "model": p.model, "fullbright": u[34],
        "dissolve": p.appearance.dissolve, "ghost": p.appearance.ghost, "power": p.appearance.power,
        "filter_fade": p.filter_fade,
        "lights_on": (0..8).filter(|light| u[LIGHTS + light * 8 + 7] > 0.5).count(),
        "fog_w": [u[122], u[126], u[136], u[146]], "fog_free": add_program::fog_free(p, u),
        "layers": surface.map(|s| s.layers.iter().map(|l| (l.stage.environment, l.stage.alpha_test)).collect::<Vec<_>>()),
        "vertex_alpha_opaque": mesh.vertices.iter().all(|v| v.color[3] == 255),
        "triangles": mesh.indices.len() / 3, "vertices": mesh.vertices.len(),
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pass() -> Pass {
        Pass {
            gpu_skin: None,
            _game_thread: crate::game_thread::GameThread::default(), geometry: None, effect: false,
            blend_index: 0, depth_equal: false,
            #[cfg(target_os = "vita")] world_static: None,
            #[cfg(target_os = "vita")] world_geometry_counted: false,
            mesh: Mesh { upload_identity: None, surface: None,
                vertices: vec![Vertex::new(0., 0., 0., 0., 0., WHITE); 3], indices: vec![0, 1, 2], texture: None },
            material: None, lit: true, clamp: false, cutout: 4., cutout_reference: 0.5, gain: Vec4::ONE,
            fog: 0., model: true, appearance: Default::default(), filter_fade: 0.,
        }
    }
    /// The payload words `uniforms` writes for `p`, without a retained frame.
    fn payload(p: &Pass) -> [f32; UNIFORM_FLOATS] {
        let mut u = [0.; UNIFORM_FLOATS];
        u[35] = f32::from(p.lit); u[36] = f32::from(p.clamp); u[37] = p.cutout; u[38] = p.cutout_reference;
        u[GAIN..GAIN + 4].copy_from_slice(&p.gain.to_array()); u[43] = p.fog; u[FX_MODEL] = f32::from(p.model);
        u[APPEARANCE..APPEARANCE + 4].copy_from_slice(
            &[p.appearance.dissolve, f32::from(p.appearance.ghost), p.appearance.power, p.filter_fade]);
        u
    }
    #[test]
    fn offsets_name_the_fields_of_the_retained_block() {
        let end = |name: &str| UNIFORM_FIELDS.iter().find(|f| f.0 == name).unwrap().1;
        assert_eq!(end("Gain"), GAIN + 4);
        assert_eq!(end("FogMode"), FOG_MODE + 1);
        assert_eq!(end("SkyFade"), SKY_FADE + 1);
        assert_eq!(end("FxModel"), FX_MODEL + 1);
        assert_eq!(end("FxAppearance"), APPEARANCE + 4);
        assert_eq!(end("FxAppearance"), LIGHTS);
        assert_eq!(end("Lights"), LIGHTS + 64);
        // `lighting::packed` writes position+radius then colour+1 per light: the on flag is word 7.
        assert_eq!(crate::lighting::uniforms().len(), 16);
    }
    #[test]
    fn only_packets_whose_removed_lines_are_no_ops_are_admitted() {
        let p = pass();
        assert!(admitted(&p, &payload(&p)));
        for (name, adjust) in [
            ("effect", (|p: &mut Pass| p.effect = true) as fn(&mut Pass)),
            ("dissolve", |p: &mut Pass| p.appearance.dissolve = 0.001),
            ("ghost", |p: &mut Pass| p.appearance.ghost = true),
            ("power tint", |p: &mut Pass| p.appearance.power = 1.),
            ("rage tint", |p: &mut Pass| p.appearance.power = 2.),
            ("filter fade to white", |p: &mut Pass| p.filter_fade = 0.25),
            ("filter fade to grey", |p: &mut Pass| p.filter_fade = -0.25),
            ("world model reads the light atlas", |p: &mut Pass| p.model = false),
            ("source-colour fog", |p: &mut Pass| p.fog = 4.),
            ("non-finite gain", |p: &mut Pass| p.gain.x = f32::INFINITY),
        ] {
            let mut p = pass();
            adjust(&mut p);
            assert!(!admitted(&p, &payload(&p)), "{name} was admitted");
        }
        // Blend, depth state, lighting, clamping, cutout mode and fog mode are all still
        // evaluated by the kept lines, so none of them decides admission.
        for adjust in [(|p: &mut Pass| p.blend_index = 1) as fn(&mut Pass), |p: &mut Pass| p.depth_equal = true,
            |p: &mut Pass| p.lit = false, |p: &mut Pass| p.clamp = true, |p: &mut Pass| p.cutout = 1.,
            |p: &mut Pass| p.cutout = 0., |p: &mut Pass| p.fog = 1., |p: &mut Pass| p.fog = 3., |p: &mut Pass| p.fog = -1.,
            |p: &mut Pass| p.gain.w = 0.3] {
            let mut p = pass();
            adjust(&mut p);
            assert!(admitted(&p, &payload(&p)));
        }
        // Any dynamic light, in any of the eight slots, keeps the full program.
        for light in 0..8 {
            let mut u = payload(&p);
            u[LIGHTS + light * 8 + 7] = 1.;
            assert!(!admitted(&p, &u), "light {light}");
        }
        let mut u = payload(&p);
        u[SKY_FADE] = 1.;
        assert!(!admitted(&p, &u), "backdrop fade");
    }
    #[test]
    fn the_lean_sources_are_the_full_program_minus_the_unreachable_lines() {
        let full = crate::render::FRAGMENT;
        let [lean, opaque] = [fragment(false), fragment(true)];
        assert_eq!(sources(), [lean.clone(), opaque.clone()]);
        // Every lean line is a full-program line, in the full program's order, except the two
        // uniform declarations that lose unused names and the fog call that loses its `else`.
        let fog_call = " if(FogMode>=0.0)rgb=fogged(rgb,FogMode);";
        assert!(full.contains("\n }else if(FogMode>=0.0)rgb=fogged(rgb,FogMode);\n"));
        let mut cursor = full.lines();
        for line in lean.lines() {
            if line == "uniform sampler2D Texture;" || line == "uniform float FxModel;" || line == fog_call { continue; }
            assert!(cursor.any(|candidate| candidate == line), "not a full-program line in order: {line}");
        }
        assert!(full.contains("uniform sampler2D Texture; uniform sampler2D LightAtlas; uniform vec2 AtlasGrid;"));
        assert!(full.contains("uniform float FxModel; uniform vec4 FxAppearance;"));
        // Exactly these statements are gone.
        let gone: Vec<&str> = full.lines().filter(|line| !lean.lines().any(|kept| kept == *line)
            && !line.starts_with("uniform sampler2D Texture;") && !line.starts_with("uniform float FxModel;")).collect();
        let markers = [
            "uniform sampler2D SkyBackdrop;", "// LIGHTS", "FxAppearance.x>0.0", "FxAppearance.y>0.5",
            "if(lm.z>0.5&&FxModel<0.5)", "rgb+=diffuse.rgb*dynamicLight(worldPosition)*Gain.rgb;",
            "if(FxAppearance.z>1.5)", "// Multiplicative actor layers", "if(FxAppearance.w>0.0)",
            " if(FogMode>3.5){", "// Source-alpha/source-colour", "// fully fogged destination",
            "float visible=fogged(vec3(1.0),1.0).r;", "rgb=mix(vec3(1.0),rgb,visible);alpha*=visible;",
            " }else if(FogMode>=0.0)rgb=fogged(rgb,FogMode);",
            "// The remastered Fortress exterior", "// sky, preserving", "if(SkyFade>0.5",
            "float a=FogA.w>0.0", "float b=FogB.w>0.0", "rgb+=texture2D(SkyBackdrop", " }"];
        assert_eq!(gone.len(), markers.len(), "{gone:#?}");
        for (line, marker) in gone.iter().zip(markers) {
            assert!(line.contains(marker), "{line} / {marker}");
        }
        assert!(!lean.contains("SkyBackdrop") && full.contains("rgb+=texture2D(SkyBackdrop"));
        for kept in [
            " vec2 coords=mix(uv,clamp(uv,vec2(0.001),vec2(0.999)),ClampUV);vec4 diffuse=texture2D(Texture,coords);",
            " float alpha=diffuse.a*Gain.a*color.a;",
            " vec3 lighting=max(color.rgb,vec3(0.18));",
            " vec3 rgb=diffuse.rgb*mix(vec3(1.0),lighting,Lit*(1.0-Fullbright))*Gain.rgb;",
            " if(FxModel>0.5&&Lit<0.5)rgb*=color.rgb;",
            fog_call, " gl_FragColor=vec4(rgb,alpha);", "// FOG",
        ] {
            for source in [&lean, &opaque] { assert_eq!(source.lines().filter(|l| *l == kept).count(), 1, "{kept}"); }
        }
        for source in [&lean, &opaque] {
            for absent in ["LightAtlas", "AtlasGrid", "SkyBackdrop", "SkySize", "SkyFade", "dynamicLight", "FxAppearance", "gl_FragCoord", "visible", "else"] {
                assert!(!source.contains(absent), "{absent}");
            }
            assert!(source.ends_with(" gl_FragColor=vec4(rgb,alpha);\n}"));
        }
        // The lean program keeps both cutout lines byte for byte; the opaque one differs from
        // it by exactly those two lines.
        let cutouts: Vec<&str> = full.lines().filter(|l| l.contains("Cutout>") && l.contains("discard;")).collect();
        assert_eq!(cutouts.len(), 2);
        assert_eq!(lean.matches("discard").count(), 2);
        assert_eq!(opaque, lean.lines().filter(|l| !cutouts.contains(l)).collect::<Vec<_>>().join("\n"));
        assert!(!opaque.contains("discard"));
        // The interpolant declarations match the full program's, in its order.
        let varyings = |source: &str| source.split(';').map(str::trim).filter(|d| d.contains("varying "))
            .map(|d| d.rsplit(' ').next().unwrap().to_owned()).collect::<Vec<_>>();
        let expected = varyings(&crate::environment::fragment(full));
        assert_eq!(expected, ["uv", "lm", "color", "worldPosition"]);
        for source in [&lean, &opaque] { assert_eq!(varyings(&crate::environment::fragment(source)), expected); }
    }
    #[test]
    fn the_key_is_off_by_default_and_strict() {
        let c = crate::render::vita_renderer::Config::default();
        assert!(!c.actor_lean_shader);
        assert!(serde_json::from_value::<crate::render::vita_renderer::Config>(serde_json::json!({"actor_lean_shader": 1})).is_err());
        assert!(serde_json::from_value::<crate::render::vita_renderer::Config>(serde_json::json!({"actor_lean_shader": true})).unwrap().actor_lean_shader);
    }
}
