//! Authored distance atmosphere and bounded fog volumes; the density curve is provisional.
use crate::{assets::Assets, bsp::Bsp, texture::MaterialSpec};
use anyhow::Result;
use macroquad::prelude::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub struct Volume {
    pub min: Vec3,
    pub max: Vec3,
    pub color: Vec4,
}
#[derive(Clone, Default)]
pub struct Atmosphere {
    pub distance: Vec4,
    pub volumes: Vec<Volume>,
    pub liquid: Vec4,
}
impl Atmosphere {
    pub fn load(
        assets: &mut Assets,
        name: &str,
        map: &Bsp,
        specs: &BTreeMap<String, MaterialSpec>,
    ) -> Result<Self> {
        let script = format!("maps/{name}.scr");
        let distance = if assets.contains(&script) {
            literal_farplane(&String::from_utf8_lossy(&assets.read(&script)?))
        } else {
            None
        }
        .unwrap_or(Vec4::ZERO);
        let mut volumes = Vec::new();
        for fog in &map.fogs {
            let Some(color) = specs.get(&fog.shader).and_then(|s| s.fog) else {
                continue;
            };
            let mut min = Vec3::splat(f32::NEG_INFINITY);
            let mut max = Vec3::splat(f32::INFINITY);
            let mut axial = true;
            for &p in &map.side_planes[map.brushes[fog.brush].sides.clone()] {
                let p = map.planes[p];
                let mut found = false;
                for axis in 0..3 {
                    if p.normal[axis].abs() > 0.99999 {
                        if p.normal[axis] > 0. {
                            max[axis] = max[axis].min(p.distance);
                        } else {
                            min[axis] = min[axis].max(-p.distance);
                        }
                        found = true;
                    }
                }
                axial &= found;
            }
            if axial && min.is_finite() && max.is_finite() {
                volumes.push(Volume {
                    min,
                    max,
                    color: Vec4::from_array(color),
                });
            } else {
                eprintln!(
                    "Fog volume {} has non-axial planes; not yet rendered",
                    fog.shader
                );
            }
        }
        anyhow::ensure!(
            volumes.len() <= 2,
            "More than two fog volumes are not supported"
        );
        Ok(Self {
            distance,
            volumes,
            liquid: Vec4::ZERO,
        })
    }
    pub fn apply(&self, material: &Material, camera: Vec3) {
        material.set_uniform("FogEye", camera);
        material.set_uniform("DistanceFog", self.distance);
        material.set_uniform("LiquidFog", self.liquid);
        for (i, prefix) in ["FogA", "FogB"].iter().enumerate() {
            let v = self.volumes.get(i).copied().unwrap_or(Volume {
                min: Vec3::ZERO,
                max: Vec3::ZERO,
                color: Vec4::ZERO,
            });
            material.set_uniform(&format!("{prefix}Min"), v.min);
            material.set_uniform(&format!("{prefix}Max"), v.max);
            material.set_uniform(prefix, v.color);
        }
    }
    pub fn background(&self) -> Color {
        if self.liquid.w > 0. {
            Color::new(self.liquid.x, self.liquid.y, self.liquid.z, 1.)
        } else if self.distance.w > 0. {
            Color::new(self.distance.x, self.distance.y, self.distance.z, 1.)
        } else {
            Color::from_hex(0x202633)
        }
    }
}
/// Read only an unambiguous literal declaration. Conditional/changing settings need the future VM.
pub fn literal_farplane(text: &str) -> Option<Vec4> {
    let mut found = Vec::new();
    for line in crate::materials::lines(text) {
        let s = line.join(" ");
        let Some(rest) = s
            .strip_prefix("setfarplane(")
            .or_else(|| s.strip_prefix("setfarplane ("))
        else {
            continue;
        };
        let values = rest
            .split(|c: char| c.is_whitespace() || "'\",();".contains(c))
            .filter(|s| !s.is_empty())
            .map(str::parse::<f32>)
            .collect::<std::result::Result<Vec<_>, _>>()
            .ok()?;
        if values.len() != 4 || values.iter().any(|v| !v.is_finite()) || values[3] <= 0. {
            return None;
        }
        found.push(Vec4::new(values[0], values[1], values[2], values[3]));
    }
    if found.len() == 1 {
        found.pop()
    } else {
        None
    }
}
pub fn uniforms() -> Vec<UniformDesc> {
    vec![
        UniformDesc::new("FogEye", UniformType::Float3),
        UniformDesc::new("DistanceFog", UniformType::Float4),
        UniformDesc::new("LiquidFog", UniformType::Float4),
        UniformDesc::new("FogAMin", UniformType::Float3),
        UniformDesc::new("FogAMax", UniformType::Float3),
        UniformDesc::new("FogA", UniformType::Float4),
        UniformDesc::new("FogBMin", UniformType::Float3),
        UniformDesc::new("FogBMax", UniformType::Float3),
        UniformDesc::new("FogB", UniformType::Float4),
    ]
}
pub fn fragment(source: &str) -> String {
    // Adapted from NextOs-Ports/Looking-Glass-NextOS 005adaa (felc18-blip).
    #[cfg(feature = "vita_renderer")]
    if crate::render::vita_renderer::config().simple_fog {
        // Legacy volume layers also refer directly to FogA/B and fogLength.
        let declarations = FOG.split_once("float slab(").unwrap().0;
        let simple = format!("{declarations}\nfloat fogLength(vec3 lo,vec3 hi){{return 0.0;}}\nvec3 fogged(vec3 rgb,float additive){{return rgb;}}\n");
        return source.replace("// FOG", &simple);
    }
    source.replace("// FOG", FOG)
}
/// Opaque stages have Alpha.z = 0. Specialize their fog targets and compile
/// out volume slots absent from this map. Keep the original arithmetic,
/// precision qualifiers and order for every live factor and mix.
#[cfg(feature = "vita_renderer")]
pub(crate) fn opaque_fog(source: &str, volume_slots: usize) -> String {
    let mut block = FOG.replace(" vec3 neutral=additive>2.5?vec3(0.5):vec3(1.0);\n", "")
        .replace("vec3 fogged(vec3 rgb,float additive)", "vec3 fogged(vec3 rgb)")
        .replace("additive>1.5?neutral:", "")
        .replace(".rgb*(1.0-additive)", ".rgb");
    if volume_slots < 2 {
        block = block.lines().filter(|line| {
            (volume_slots > 0 || !line.contains("FogA")) && !line.contains("FogB")
        }).collect::<Vec<_>>().join("\n");
    }
    if volume_slots == 0 {
        let start = block.find("float slab(").unwrap();
        let end = block.find("vec3 fogged(").unwrap();
        block.replace_range(start..end, "");
    }
    if volume_slots > 0 {
        // The volume calculation used a highp length independently of the
        // mediump distance/liquid factor. Share the highp result and round
        // dist at the original assignment for those two factors.
        block = block.replace("vec3 lo,vec3 hi){\n highp vec3 d=worldPosition-FogEye;",
                "vec3 lo,vec3 hi,highp vec3 d,highp float rayLength){\n")
            .replace("*length(d)*valid", "*rayLength*valid")
            .replace("float dist=length(worldPosition-FogEye);",
                "highp vec3 d=worldPosition-FogEye;\n highp float rayLength=length(d);\n float dist=rayLength;")
            .replace("fogLength(FogAMin,FogAMax)", "fogLength(FogAMin,FogAMax,d,rayLength)")
            .replace("fogLength(FogBMin,FogBMax)", "fogLength(FogBMin,FogBMax,d,rayLength)");
    }
    source.replace("// FOG", &block).replace("fogged(rgb,Alpha.z)", "fogged(rgb)")
}

/// Same fog factors as the fragment path, evaluated at the submitted
/// (possibly deformed) vertices and interpolated through clipping/rasterization.
#[cfg(feature = "vita_renderer")]
pub(crate) fn vertex_fog(source: &str) -> String {
    let helpers = FOG.split_once("vec3 fogged(").unwrap().0
        .replace("varying highp vec3 worldPosition;\n", "");
    let block = format!(r#"precision mediump float;
varying mediump vec4 FogFactors;
{helpers}
vec4 fogFactors(){{
 float dist=length(worldPosition-FogEye);
 vec4 factors=vec4(0.0);
 if(DistanceFog.w>0.0)factors.x=clamp((dist-DistanceFog.w*0.35)/(DistanceFog.w*0.65),0.0,1.0);
 if(FogA.w>0.0)factors.y=clamp(fogLength(FogAMin,FogAMax)/FogA.w,0.0,1.0);
 if(FogB.w>0.0)factors.z=clamp(fogLength(FogBMin,FogBMax)/FogB.w,0.0,1.0);
 if(LiquidFog.w>0.0)factors.w=clamp(0.12+dist/LiquidFog.w,0.0,0.96);
 return factors;
}}
precision highp float;
void main(){{"#);
    source.replace("void main(){", &block)
        .replace(" worldPosition=world.xyz;", " worldPosition=world.xyz;\n FogFactors=fogFactors();")
}

#[cfg(feature = "vita_renderer")]
pub(crate) fn interpolated_fog(source: &str, volume_slots: usize) -> String {
    let a = if volume_slots >= 1 { " rgb=mix(rgb,FogA.rgb,FogFactors.y);\n" } else { "" };
    let b = if volume_slots >= 2 { " rgb=mix(rgb,FogB.rgb,FogFactors.z);\n" } else { "" };
    let block = format!(r#"varying mediump vec4 FogFactors;
uniform vec4 DistanceFog, FogA, FogB, LiquidFog;
vec3 fogged(vec3 rgb){{
 rgb=mix(rgb,DistanceFog.rgb,FogFactors.x);
{a}{b} rgb=mix(rgb,LiquidFog.rgb,FogFactors.w);
 return rgb;
}}"#);
    source.replace("// FOG", &block).replace("fogged(rgb,Alpha.z)", "fogged(rgb)")
}

const FOG: &str = r#"
varying highp vec3 worldPosition;
uniform highp vec3 FogEye;
uniform vec4 DistanceFog;
uniform vec4 LiquidFog;
uniform highp vec3 FogAMin; uniform highp vec3 FogAMax; uniform vec4 FogA;
uniform highp vec3 FogBMin; uniform highp vec3 FogBMax; uniform vec4 FogB;
float slab(float p,float d,float lo,float hi,inout float enter,inout float leave){
 if(abs(d)<0.00001){if(p<lo||p>hi)return 0.0;return 1.0;}
 float a=(lo-p)/d;float b=(hi-p)/d;enter=max(enter,min(a,b));leave=min(leave,max(a,b));return 1.0;
}
float fogLength(vec3 lo,vec3 hi){
 highp vec3 d=worldPosition-FogEye;float a=0.0;float b=1.0;
 float valid=slab(FogEye.x,d.x,lo.x,hi.x,a,b)*slab(FogEye.y,d.y,lo.y,hi.y,a,b)*slab(FogEye.z,d.z,lo.z,hi.z,a,b);
 return max(0.0,b-a)*length(d)*valid;
}
vec3 fogged(vec3 rgb,float additive){
 // Multiplicative layers fade toward their blend's neutral value, never a
 // second layer of coloured fog. Additive light fades toward black.
 vec3 neutral=additive>2.5?vec3(0.5):vec3(1.0);
 float dist=length(worldPosition-FogEye);
 if(DistanceFog.w>0.0){float a=clamp((dist-DistanceFog.w*0.35)/(DistanceFog.w*0.65),0.0,1.0);rgb=mix(rgb,additive>1.5?neutral:DistanceFog.rgb*(1.0-additive),a);}
 if(FogA.w>0.0)rgb=mix(rgb,additive>1.5?neutral:FogA.rgb*(1.0-additive),clamp(fogLength(FogAMin,FogAMax)/FogA.w,0.0,1.0));
 if(FogB.w>0.0)rgb=mix(rgb,additive>1.5?neutral:FogB.rgb*(1.0-additive),clamp(fogLength(FogBMin,FogBMax)/FogB.w,0.0,1.0));
 if(LiquidFog.w>0.0)rgb=mix(rgb,additive>1.5?neutral:LiquidFog.rgb*(1.0-additive),clamp(0.12+dist/LiquidFog.w,0.0,0.96));
 return rgb;
}
"#;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_settings_ignore_comments_and_refuse_ambiguous_state() {
        assert_eq!(
            literal_farplane("//setfarplane('1 1 1',1);\nsetfarplane( '.33 .23 .1', 2500 );")
                .unwrap()
                .w,
            2500.
        );
        assert!(literal_farplane("setfarplane('0 0 0', 3);\nsetfarplane('0 0 0', 4);").is_none());
        assert!(literal_farplane("setfarplane( color, distance );").is_none());
    }
}
