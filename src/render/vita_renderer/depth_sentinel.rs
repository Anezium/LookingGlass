//! Colour-only depth diagnostic. Readback happens after world/FX submission, never between them.
use super::*;
use std::{cell::RefCell, mem::ManuallyDrop};

struct Probe {
    _shader: ShaderId,
    pipelines: [Pipeline; 3],
    bindings: Bindings,
    pending: bool,
    frame: u64,
    last: Option<bool>,
}
// GL owns these until process exit, like the scaled target.
thread_local! { static PROBE: RefCell<ManuallyDrop<Option<Probe>>> = const { RefCell::new(ManuallyDrop::new(None)) }; }
impl Probe {
    fn new() -> Self {
        let ctx = unsafe { get_internal_gl() }.quad_context;
        crate::frame_profile::shader_creation();
        let _name = crate::shader_cache::Name::new("depth-sentinel");
        let shader = ctx.new_shader(ShaderSource::Glsl {
            vertex: "#version 100\nattribute vec2 position; uniform vec4 Rect; uniform float Depth; void main(){gl_Position=vec4(mix(Rect.xy,Rect.zw,position),Depth,1.);}",
            fragment: "#version 100\nprecision mediump float; uniform vec4 Tint; void main(){gl_FragColor=Tint;}",
        }, ShaderMeta {
            images: vec![],
            uniforms: UniformBlockLayout { uniforms: vec![
                UniformDesc::new("Rect", UniformType::Float4),
                UniformDesc::new("Depth", UniformType::Float1),
                UniformDesc::new("Tint", UniformType::Float4),
            ] },
        }).expect("depth sentinel shader");
        let pipelines = [0, 2, 1].map(|blend| {
            crate::frame_profile::pipeline_creation();
            ctx.new_pipeline(&[BufferLayout::default()],
                &[VertexAttribute::new("position", VertexFormat::Float2)], shader,
                crate::render::depth_pipeline(crate::render::blend(blend)))
        });
        let bindings = Bindings {
            vertex_buffers: vec![ctx.new_buffer(BufferType::VertexBuffer, BufferUsage::Immutable,
                BufferSource::slice(&[[0f32,0.], [1.,0.], [1.,1.], [0.,1.]]))],
            index_buffer: ctx.new_buffer(BufferType::IndexBuffer, BufferUsage::Immutable,
                BufferSource::slice(&[0u16,1,2,0,2,3])),
            images: vec![],
        };
        Self { _shader: shader, pipelines, bindings, pending: false, frame: 0, last: None }
    }
    fn draw(&self, cell: Option<usize>, pipeline: usize, depth: f32, tint: [f32;4]) {
        let (left, right) = cell.map_or((-0.96, -0.36), |i| (-0.96 + i as f32 * 0.2, -0.76 + i as f32 * 0.2));
        // Exact packed GLSL uniform layout: Float4, Float1, Float4.
        let u = [left, -0.96, right, -0.76, depth, tint[0], tint[1], tint[2], tint[3]];
        let ctx = unsafe { get_internal_gl() }.quad_context;
        ctx.apply_pipeline(&self.pipelines[pipeline]);
        unsafe { macroquad::miniquad::gl::glDepthMask(u8::from(pipeline == 0)); }
        ctx.apply_bindings(&self.bindings);
        ctx.apply_uniforms(UniformsSource::table(&u));
        ctx.draw(0, 6, 1);
    }
}
pub(super) fn opaque() {
    if !config().depth_sentinel || !crate::render_fx::retained_active() { return; }
    PROBE.with(|p| {
        let mut p = p.borrow_mut();
        let p = p.get_or_insert_with(Probe::new);
        p.draw(None, 0, -0.998, [0.,0.,1.,1.]);
        p.pending = true;
    });
}
pub(super) fn log_fx_state() {
    if !config().depth_sentinel || !PROBE.with(|p| p.borrow().as_ref().is_some_and(|p| p.pending)) { return; }
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| unsafe {
        use macroquad::miniquad::gl::*;
        let mut target = 0; let mut viewport = [0;4]; let mut func = 0;
        glGetIntegerv(0x8CA6, &mut target); let target_error = glGetError();
        glGetIntegerv(0x0BA2, viewport.as_mut_ptr()); let viewport_error = glGetError();
        glGetIntegerv(0x0B74, &mut func); let func_error = glGetError();
        #[cfg(target_os="vita")]
        let (test, mask) = {
            extern "C" { fn glIsEnabled(cap: u32) -> u8; fn glGetBooleanv(p: u32, v: *mut u8); }
            let mut mask = 0; glGetBooleanv(0x0B72, &mut mask);
            (glIsEnabled(0x0B71) as i32, mask as i32)
        };
        #[cfg(not(target_os="vita"))]
        let (test, mask) = {
            let mut test = 0; let mut mask = 0;
            glGetIntegerv(0x0B71, &mut test); glGetIntegerv(0x0B72, &mut mask);
            (test, mask)
        };
        let state_error = glGetError();
        println!("[lg/depth-sentinel] first_world_fx target={target} viewport={viewport:?} test={test} mask={mask} func={func} query_errors={:?}",
            [target_error, viewport_error, func_error, state_error]);
    });
}
pub(super) fn finish() {
    if !config().depth_sentinel { return; }
    PROBE.with(|p| {
        let mut p = p.borrow_mut();
        let Some(p) = (**p).as_mut() else { return; };
        if !p.pending { return; }
        p.pending = false;
        // The rear is wholly contained by the opaque panel. Two front alpha
        // layers are deliberately drawn near then far to catch unwanted writes.
        p.draw(Some(0), 1, -0.996, [1.,0.,0.,1.]);
        p.draw(Some(1), 1, -0.999, [0.,1.,0.,1.]);
        p.draw(Some(2), 2, -0.9995, [0.,1.,0.,0.5]);
        p.draw(Some(2), 2, -0.999, [1.,0.,0.,0.5]);
        let mut pixels = [[0u8;4];3];
        let mut errors = Vec::new();
        unsafe {
            use macroquad::miniquad::gl::*;
            let mut viewport = [0;4]; glGetIntegerv(0x0BA2, viewport.as_mut_ptr());
            errors.push(glGetError());
            for (i, pixel) in pixels.iter_mut().enumerate() {
                let x = viewport[0] + ((0.07 + i as f32 * 0.1) * viewport[2] as f32) as i32;
                let y = viewport[1] + (0.07 * viewport[3] as f32) as i32;
                glReadPixels(x, y, 1, 1, GL_RGBA, GL_UNSIGNED_BYTE, pixel.as_mut_ptr().cast());
                errors.push(glGetError());
            }
        }
        let expected = [[0,0,255], [0,255,255], [128,64,64]];
        let visible: [bool;3] = std::array::from_fn(|i| pixels[i][..3].iter().zip(expected[i])
            .all(|(&a,b)| a.abs_diff(b) <= 4));
        let pass = errors.iter().all(|&e| e == 0) && visible.iter().all(|&v| v);
        p.last = Some(pass); p.frame += 1;
        println!("[lg/depth-sentinel] frame={} scale={} fix={} rear_visible={} front_visible={} alpha_read_only={} rgba={pixels:?} read_errors={errors:?} pass={pass}",
            p.frame, config().render_scale, config().scaled_depth_fix, pixels[0][0] > 4, visible[1], visible[2]);
        // State is re-applied by the next production draw; caller restores mask.
    });
}
#[cfg(not(target_os="vita"))]
pub(super) fn last_pass() -> Option<bool> {
    PROBE.with(|p| p.borrow().as_ref().and_then(|p| p.last))
}
