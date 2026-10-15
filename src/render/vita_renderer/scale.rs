//! One reusable single-sample 3D target, composited before the native-resolution HUD.
use super::*;
use macroquad::camera::Camera;
use std::cell::RefCell;
use std::mem::ManuallyDrop;
struct Target {
    rt: RenderTarget,
    depth: Option<TextureId>,
    width: u32,
    height: u32,
    upscale: Material,
}
// Macroquad's context is process-lived. Miniquad's host GL library/context is
// already gone when main-thread TLS destructors run, so the final target must
// share that process lifetime. Replacing **slot below still drops an old target
// during a live frame (including its separately owned host depth attachment).
thread_local! {static TARGET:RefCell<ManuallyDrop<Option<Target>>>=const{RefCell::new(ManuallyDrop::new(None))};}
impl Drop for Target {
    fn drop(&mut self) {
        if let Some(depth) = self.depth {
            unsafe { get_internal_gl() }
                .quad_context
                .delete_texture(depth);
        }
    }
}
pub(super) fn pass() -> Option<macroquad::miniquad::RenderPass> {
    TARGET.with(|t| {
        t.borrow()
            .as_ref()
            .map(|t| t.rt.render_pass.raw_miniquad_id())
    })
}
/// The completed 3D image survives the default framebuffer's next-frame clear.
/// Readback is requested only after a host survey has flushed its timed rows.
#[cfg(not(target_os = "vita"))]
pub(super) fn verification_image() -> Option<Image> {
    TARGET.with(|t| t.borrow().as_ref().map(|t| t.rt.texture.get_texture_data()))
}
pub(super) fn prepare(bg: Color) {
    crate::render_tail::end();
    log_framebuffer();
    if config().render_scale == 1. {
        // miniquad clear obeys the previous HUD's depth write mask.
        begin(PassAction::Clear {
            color: None,
            depth: Some(1.),
            stencil: None,
        });
        unsafe {
            get_internal_gl().quad_context.end_render_pass();
        }
        return;
    }
    let w = (screen_width() * config().render_scale).round() as u32;
    let h = (screen_height() * config().render_scale).round() as u32;
    TARGET.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.as_ref().is_none_or(|t| t.width != w || t.height != h) {
            let mut rt = render_target_ex(
                w,
                h,
                RenderTargetParams {
                    sample_count: 1,
                    depth: false,
                },
            );
            rt.texture
                .set_filter(macroquad::texture::FilterMode::Linear);
            #[cfg(not(target_os = "vita"))]
            let depth = {
                let ctx = unsafe { get_internal_gl() }.quad_context;
                let depth = ctx.new_texture(
                    TextureAccess::RenderTarget,
                    TextureSource::Empty,
                    TextureParams {
                        width: w,
                        height: h,
                        format: TextureFormat::Depth32,
                        sample_count: 1,
                        ..Default::default()
                    },
                );
                // Camera's public render-pass wrapper has no raw constructor. Attach the
                // owned Depth32 image to its framebuffer, retaining normal wrapper ownership.
                ctx.begin_pass(Some(rt.render_pass.raw_miniquad_id()), PassAction::Nothing);
                let raw = unsafe { ctx.texture_raw_id(depth) };
                if let RawId::OpenGl(id) = raw {
                    unsafe {
                        macroquad::miniquad::gl::glFramebufferTexture2D(
                            0x8D40, 0x8D00, 0x0DE1, id, 0,
                        );
                        assert_eq!(
                            macroquad::miniquad::gl::glCheckFramebufferStatus(0x8D40),
                            0x8CD5,
                            "incomplete scaled depth target"
                        );
                    }
                } else {
                    panic!("scaled renderer requires OpenGL");
                }
                ctx.end_render_pass();
                rt.render_pass.depth_texture = Some(Texture2D::from_miniquad_texture(depth));
                Some(depth)
            };
            #[cfg(target_os = "vita")]
            let depth: Option<TextureId> = None;
            let upscale = crate::frame_profile::load_material(
                ShaderSource::Glsl {
                    vertex: UPSCALE_VERTEX,
                    fragment: UPSCALE_FRAGMENT,
                },
                MaterialParams {
                    pipeline_params: PipelineParams {
                        color_blend: None,
                        alpha_blend: None,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .expect("opaque upscale shader");
            **slot = Some(Target {
                rt,
                depth,
                width: w,
                height: h,
                upscale,
            });
            println!(
                "[lg/vita-renderer] 3d target {w}x{h} RGBA8/depth32 samples=1 scale={} copy_v_inverted={}",
                config().render_scale, cfg!(target_os = "vita")
            );
        }
    });
    begin(PassAction::Clear {
        color: Some((bg.r, bg.g, bg.b, 1.)),
        depth: Some(1.),
        stencil: None,
    });
    unsafe {
        get_internal_gl().quad_context.end_render_pass();
    }
}
pub(super) fn begin(action: PassAction) {
    unsafe {
        if matches!(action, PassAction::Clear { depth: Some(_), .. }) {
            macroquad::miniquad::gl::glDepthMask(1);
        }
        get_internal_gl().quad_context.begin_pass(pass(), action);
    }
}
struct ScaledCamera<'a>(&'a Camera3D);
impl Camera for ScaledCamera<'_> {
    fn matrix(&self) -> Mat4 {
        self.0.matrix()
    }
    fn depth_enabled(&self) -> bool {
        true
    }
    fn render_pass(&self) -> Option<macroquad::texture::RenderPass> {
        TARGET.with(|t| t.borrow().as_ref().map(|t| t.rt.render_pass.clone()))
    }
    fn viewport(&self) -> Option<(i32, i32, i32, i32)> {
        self.0.viewport
    }
}
pub(super) fn camera(camera: &Camera3D) {
    if pass().is_some() {
        set_camera(&ScaledCamera(camera));
    } else {
        set_camera(camera);
    }
}
pub(super) fn present() {
    crate::render_tail::begin();
    let _timer = crate::light_profile::render_timer(crate::light_profile::RenderPhase::Present);
    TARGET.with(|t| {
        let t = t.borrow();
        if let Some(target) = t.as_ref() {
            let texture = &target.rt.texture;
            // The bridge has finished all 3D packets; queue a single upscale before HUD.
            set_default_camera();
            gl_use_material(&target.upscale);
            draw_texture_ex(
                texture,
                0.,
                0.,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(screen_width(), screen_height())),
                    flip_y: true,
                    ..Default::default()
                },
            );
            count("vita_upscale_draws", 1);
            gl_use_default_material();
        }
    });
}
fn log_framebuffer() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(||{
        let values=unsafe{let mut result=Vec::new();for (label,param) in [("red_bits",0x0D52),("green_bits",0x0D53),("blue_bits",0x0D54),("alpha_bits",0x0D55),("depth_bits",0x0D56),("stencil_bits",0x0D57)]{
            let mut value=0;macroquad::miniquad::gl::glGetIntegerv(param,&mut value);let error=macroquad::miniquad::gl::glGetError();result.push(serde_json::json!({"name":label,"value":value,"gl_error":error}));}result};
        println!("[lg/vita-renderer] framebuffer sample_count={} gxm_msaa={} default_queries={} extra_fullscreen_pass={}",config().msaa_samples,match config().msaa_samples{1=>"NONE",2=>"2X",_=>"4X"},serde_json::to_string(&values).unwrap(),if config().render_scale<1.{"one upscale"}else{"none"});
    });
}

const UPSCALE_VERTEX: &str = r#"#version 100
attribute vec3 position; attribute vec2 texcoord; attribute vec4 color0; attribute vec4 normal;
varying mediump vec2 uv; uniform mat4 Model; uniform mat4 Projection;
void main(){gl_Position=Projection*Model*vec4(position,1.);uv=texcoord;}"#;
#[cfg(not(target_os = "vita"))]
const UPSCALE_FRAGMENT: &str = r#"#version 100
varying mediump vec2 uv; uniform sampler2D Texture;
void main(){gl_FragColor=vec4(texture2D(Texture,uv).rgb,1.);}"#;

// The pinned VitaGL build uses HAVE_UNFLIPPED_FBOS. Invert sampled V once
// relative to the existing Macroquad copy quad; the desktop FBO convention
// differs. This shader exists only for scaled targets, before the native HUD.
#[cfg(target_os = "vita")]
const UPSCALE_FRAGMENT: &str = r#"#version 100
varying mediump vec2 uv; uniform sampler2D Texture;
void main(){gl_FragColor=vec4(texture2D(Texture,vec2(uv.x,1.-uv.y)).rgb,1.);}"#;

pub(super) fn log_depth(opaque: bool) {
    static OPAQUE: OnceLock<()> = OnceLock::new();
    static ALPHA: OnceLock<()> = OnceLock::new();
    let once = if opaque { &OPAQUE } else { &ALPHA };
    once.get_or_init(||{
        let (enabled,write,func)=unsafe{
            #[cfg(target_os="vita")]
            {extern "C"{fn glIsEnabled(cap:u32)->u8;fn glGetBooleanv(p:u32,v:*mut u8);}
                let mut write=0;let mut func=0;glGetBooleanv(0x0B72,&mut write);macroquad::miniquad::gl::glGetIntegerv(0x0B74,&mut func);(glIsEnabled(0x0B71) as i32,write as i32,func)}
            #[cfg(not(target_os="vita"))]
            {let mut enabled=0;let mut write=0;let mut func=0;macroquad::miniquad::gl::glGetIntegerv(0x0B71,&mut enabled);macroquad::miniquad::gl::glGetIntegerv(0x0B72,&mut write);macroquad::miniquad::gl::glGetIntegerv(0x0B74,&mut func);(enabled,write,func)}
        };
        println!("[lg/vita-renderer] effective depth opaque={opaque} enabled={enabled} write={write} func={func} order={} early_z=hardware-unverified",config().opaque_order);
    });
}
