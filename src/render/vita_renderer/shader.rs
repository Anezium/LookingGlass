//! Fixed GLSL ES 100 world shader. Material animation changes uniforms, never vertices.
use macroquad::{miniquad::*, prelude::*};

pub const VERTEX: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
attribute vec4 lightmap;
attribute vec3 normal;
uniform mat4 Mvp, Model, Uv, UvAfter;
uniform highp vec4 Eye, Surface, Gain, Alpha, DotAlpha, VectorS, VectorT, Turb, Deform, Wave, MoveDir, Atlas;
varying highp vec2 uv;
varying highp vec3 lm;
varying lowp vec4 color;
varying highp vec3 worldPosition;
float wave(float x,float kind){
 x=fract(x);
 if(kind<0.5)return sin(x*6.28318530718);
 if(kind<1.5)return 1.0-4.0*abs(x-0.5);
 if(kind<2.5)return x<0.5?1.0:-1.0;
 if(kind<3.5)return x;
 if(kind<4.5)return 1.0-x;
 return 0.0;
}
void main(){
 vec3 p=position;
 float phase=Wave.z+Deform.z*Wave.w;
 if(Deform.x>0.5&&Deform.x<1.5){
  phase+=(p.x+p.y+p.z)/max(Deform.y,0.001);
  p+=normal*(Wave.x+Wave.y*wave(phase,Deform.w));
 }else if(Deform.x>1.5&&Deform.x<2.5){
  p+=MoveDir.xyz*(Wave.x+Wave.y*wave(phase,Deform.w));
 }else if(Deform.x>2.5){
  p+=normal*sin(texcoord.x*Wave.x+Deform.z*Wave.z)*Wave.y;
 }
 vec4 world=Model*vec4(p,1.0);
 worldPosition=world.xyz;
 vec3 n=(Model*vec4(normal,0.0)).xyz;n/=max(length(n),0.000001);
 vec3 view=Eye.xyz-world.xyz;view/=max(length(view),0.000001);
 vec2 base=texcoord;
 if(VectorS.w>1.5){vec3 r=n*(2.0*dot(n,view))-view;base=vec2(0.5+r.y*0.5,0.5-r.z*0.5);}
 else if(VectorS.w>0.5){base=vec2(dot(world.xyz,VectorS.xyz),dot(world.xyz,VectorT.xyz));}
 uv=(Uv*vec4(base,0.0,1.0)).xy;
 vec2 ripple=sin(vec2((world.x+world.z)/1024.0,world.y/1024.0)*6.28318530718
  +vec2((Turb.y+Turb.z*Turb.w)*6.28318530718))*Turb.x;
 uv+=(UvAfter*vec4(ripple,0.0,0.0)).xy;
 color=color0/255.0;
 color.a=Alpha.y>0.5?color.a:1.0;
 if(DotAlpha.w>0.5){float d=clamp(abs(dot(n,view)),0.0,1.0);if(DotAlpha.z>0.5)d=1.0-d;color.a*=mix(DotAlpha.x,DotAlpha.y,d);}
 color.a=clamp(color.a,0.0,1.0);
 lm=lightmap.xyz;
 gl_Position=Mvp*vec4(p,1.0);
}"#;

pub const FRAGMENT: &str = r#"#version 100
precision mediump float;
varying highp vec2 uv;
varying highp vec3 lm;
varying lowp vec4 color;
uniform sampler2D Diffuse, LightAtlas;
uniform highp vec4 Surface, Gain, Alpha, Atlas;
// FOG
void main(){
 vec2 coord=mix(uv,clamp(uv,vec2(0.001),vec2(0.999)),Surface.x);
 vec4 diffuse=texture2D(Diffuse,coord);
 float a=diffuse.a*Gain.a*color.a;
 float cut=Surface.z;
 if((cut>0.5&&cut<1.5&&a<=0.0)||(cut>1.5&&cut<2.5&&a<0.5)
  ||(cut>2.5&&cut<3.5&&a>=0.5)||(cut>3.5&&cut<4.5&&a<0.35))discard;
 if((cut>4.5&&cut<5.5&&a<Alpha.x)||(cut>5.5&&cut<6.5&&a<=Alpha.x)
  ||(cut>6.5&&cut<7.5&&a>=Alpha.x)||(cut>7.5&&a>Alpha.x))discard;
 vec3 lighting=max(color.rgb,vec3(0.18));
 if(lm.z>0.5){float index=floor(lm.z-1.0+0.1);vec2 tile=vec2(mod(index,Atlas.x),floor(index/Atlas.x));
  vec2 sampleUV=(tile+clamp(lm.xy,vec2(0.5/128.0),vec2(127.5/128.0)))/Atlas.xy;
  lighting=min(vec3(1.0),texture2D(LightAtlas,sampleUV).rgb*2.0);}
 vec3 rgb=diffuse.rgb*mix(vec3(1.0),lighting,Surface.y*(1.0-Surface.w))*Gain.rgb;
 gl_FragColor=vec4(fogged(rgb,Alpha.z),a);
}"#;

#[repr(C)]
pub struct Uniforms {
    pub mvp: [f32; 16],
    pub model: [f32; 16],
    pub uv: [f32; 16],
    pub uv_after: [f32; 16],
    pub eye: [f32; 4],
    pub surface: [f32; 4],
    pub gain: [f32; 4],
    pub alpha: [f32; 4],
    pub dot_alpha: [f32; 4],
    pub vector_s: [f32; 4],
    pub vector_t: [f32; 4],
    pub turb: [f32; 4],
    pub deform: [f32; 4],
    pub wave: [f32; 4],
    pub move_dir: [f32; 4],
    pub atlas: [f32; 4],
    pub fog_eye: [f32; 3],
    pub distance_fog: [f32; 4],
    pub liquid_fog: [f32; 4],
    pub fog_a_min: [f32; 3],
    pub fog_a_max: [f32; 3],
    pub fog_a: [f32; 4],
    pub fog_b_min: [f32; 3],
    pub fog_b_max: [f32; 3],
    pub fog_b: [f32; 4],
}
impl Uniforms {
    pub fn atmosphere(&mut self, atmosphere: &crate::environment::Atmosphere, eye: Vec3) {
        self.fog_eye = eye.to_array();
        self.distance_fog = atmosphere.distance.to_array();
        self.liquid_fog = atmosphere.liquid.to_array();
        for (i, (lo, hi, color)) in [
            (&mut self.fog_a_min, &mut self.fog_a_max, &mut self.fog_a),
            (&mut self.fog_b_min, &mut self.fog_b_max, &mut self.fog_b),
        ]
        .into_iter()
        .enumerate()
        {
            if let Some(volume) = atmosphere.volumes.get(i) {
                *lo = volume.min.to_array();
                *hi = volume.max.to_array();
                *color = volume.color.to_array();
            } else {
                *lo = [0.; 3];
                *hi = [0.; 3];
                *color = [0.; 4];
            }
        }
    }
}
pub fn meta() -> ShaderMeta {
    let mut uniforms = ["Mvp", "Model", "Uv", "UvAfter"]
        .into_iter()
        .map(|name| UniformDesc::new(name, UniformType::Mat4))
        .collect::<Vec<_>>();
    uniforms.extend(
        [
            "Eye", "Surface", "Gain", "Alpha", "DotAlpha", "VectorS", "VectorT", "Turb", "Deform",
            "Wave", "MoveDir", "Atlas",
        ]
        .into_iter()
        .map(|name| UniformDesc::new(name, UniformType::Float4)),
    );
    uniforms.extend(crate::environment::uniforms());
    ShaderMeta {
        images: vec!["Diffuse".into(), "LightAtlas".into()],
        uniforms: UniformBlockLayout { uniforms },
    }
}

pub const SKY_VERTEX: &str = r#"#version 100
attribute vec2 position; varying lowp float height;
void main(){height=position.y*0.5+0.5;gl_Position=vec4(position,0.999,1.0);}"#;
pub const SKY_FRAGMENT: &str = r#"#version 100
precision mediump float; varying lowp float height; uniform vec4 Bottom, Top;
void main(){gl_FragColor=mix(Bottom,Top,clamp(height,0.0,1.0));}"#;
