//! One reusable single-sample 3D target, composited before the native-resolution HUD.
use super::*;
use macroquad::camera::Camera;
use std::cell::RefCell;
struct Target {rt:RenderTarget,depth:TextureId,width:u32,height:u32,upscale:Material}
thread_local! {static TARGET:RefCell<Option<Target>>=const{RefCell::new(None)};}
impl Drop for Target {fn drop(&mut self){unsafe{get_internal_gl()}.quad_context.delete_texture(self.depth);}}
pub(super) fn pass()->Option<macroquad::miniquad::RenderPass>{TARGET.with(|t|t.borrow().as_ref().map(|t|t.rt.render_pass.raw_miniquad_id()))}
pub(super) fn prepare(bg:Color){
    log_framebuffer();
    if config().render_scale==1. {return;}
    let w=(screen_width()*config().render_scale).round() as u32;let h=(screen_height()*config().render_scale).round() as u32;
    TARGET.with(|slot|{let mut slot=slot.borrow_mut();
        if slot.as_ref().is_none_or(|t|t.width!=w||t.height!=h){
            let mut rt=render_target_ex(w,h,RenderTargetParams{sample_count:1,depth:false});
            rt.texture.set_filter(macroquad::texture::FilterMode::Linear);
            let ctx=unsafe{get_internal_gl()}.quad_context;
            let depth=ctx.new_texture(TextureAccess::RenderTarget,TextureSource::Empty,TextureParams{width:w,height:h,format:TextureFormat::Depth32,sample_count:1,..Default::default()});
            // Camera's public render-pass wrapper has no raw constructor. Attach the
            // owned Depth32 image to its framebuffer, retaining normal wrapper ownership.
            ctx.begin_pass(Some(rt.render_pass.raw_miniquad_id()),PassAction::Nothing);
            let raw=unsafe{ctx.texture_raw_id(depth)};
            if let RawId::OpenGl(id)=raw {unsafe{
                macroquad::miniquad::gl::glFramebufferTexture2D(0x8D40,0x8D00,0x0DE1,id,0);
                assert_eq!(macroquad::miniquad::gl::glCheckFramebufferStatus(0x8D40),0x8CD5,"incomplete scaled depth target");
            }}else{panic!("scaled renderer requires OpenGL");}
            ctx.end_render_pass();
            rt.render_pass.depth_texture=Some(Texture2D::from_miniquad_texture(depth));
            let upscale=crate::frame_profile::load_material(ShaderSource::Glsl{vertex:UPSCALE_VERTEX,fragment:UPSCALE_FRAGMENT},MaterialParams{pipeline_params:PipelineParams{color_blend:None,alpha_blend:None,..Default::default()},..Default::default()}).expect("opaque upscale shader");
            *slot=Some(Target{rt,depth,width:w,height:h,upscale});
            println!("[lg/vita-renderer] 3d target {w}x{h} RGBA8/Depth32 samples=1 scale={}",config().render_scale);
        }
    });
    begin(PassAction::Clear{color:Some((bg.r,bg.g,bg.b,1.)),depth:Some(1.),stencil:None});
    unsafe{get_internal_gl().quad_context.end_render_pass();}
}
pub(super) fn begin(action:PassAction){unsafe{get_internal_gl().quad_context.begin_pass(pass(),action);}}
struct ScaledCamera<'a>(&'a Camera3D);
impl Camera for ScaledCamera<'_>{
    fn matrix(&self)->Mat4{self.0.matrix()}
    fn depth_enabled(&self)->bool{true}
    fn render_pass(&self)->Option<macroquad::texture::RenderPass>{TARGET.with(|t|t.borrow().as_ref().map(|t|t.rt.render_pass.clone()))}
    fn viewport(&self)->Option<(i32,i32,i32,i32)>{self.0.viewport}
}
pub(super) fn camera(camera:&Camera3D){if pass().is_some(){set_camera(&ScaledCamera(camera));}else{set_camera(camera);}}
pub(super) fn present(){
    TARGET.with(|t| {let t=t.borrow();if let Some(target)=t.as_ref(){
        let texture=&target.rt.texture;
        // The bridge has finished all 3D packets; queue a single upscale before HUD.
        set_default_camera();gl_use_material(&target.upscale);
        draw_texture_ex(texture,0.,0.,WHITE,DrawTextureParams{dest_size:Some(vec2(screen_width(),screen_height())),flip_y:true,..Default::default()});
        count("vita_upscale_draws",1);gl_use_default_material();
    }});
}
fn log_framebuffer(){
    static ONCE:OnceLock<()>=OnceLock::new();ONCE.get_or_init(||{
        let values=unsafe{let mut result=Vec::new();for (label,param) in [("red_bits",0x0D52),("green_bits",0x0D53),("blue_bits",0x0D54),("alpha_bits",0x0D55),("depth_bits",0x0D56),("stencil_bits",0x0D57)]{
            let mut value=0;macroquad::miniquad::gl::glGetIntegerv(param,&mut value);let error=macroquad::miniquad::gl::glGetError();result.push(serde_json::json!({"name":label,"value":value,"gl_error":error}));}result};
        println!("[lg/vita-renderer] framebuffer sample_count={} gxm_msaa={} default_queries={} extra_fullscreen_pass={}",config().msaa_samples,match config().msaa_samples{1=>"NONE",2=>"2X",_=>"4X"},serde_json::to_string(&values).unwrap(),if config().render_scale<1.{"one upscale"}else{"none"});
    });
}

const UPSCALE_VERTEX:&str=r#"#version 100
attribute vec3 position; attribute vec2 texcoord; attribute vec4 color0; attribute vec4 normal;
varying mediump vec2 uv; uniform mat4 Model; uniform mat4 Projection;
void main(){gl_Position=Projection*Model*vec4(position,1.);uv=texcoord;}"#;
const UPSCALE_FRAGMENT:&str=r#"#version 100
varying mediump vec2 uv; uniform sampler2D Texture;
void main(){gl_FragColor=vec4(texture2D(Texture,uv).rgb,1.);}"#;
