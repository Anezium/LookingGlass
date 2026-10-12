//! GPU regressions using the actual retained world and actor shaders, outside measured runs.
use super::*;
use crate::render_fx::{self, Appearance};
use macroquad::camera::Camera;
const RED: Color = Color::new(1., 0., 0., 1.);
const GREEN: Color = Color::new(0., 1., 0., 1.);
const BLUE: Color = Color::new(0., 0., 1., 1.);

struct Plane {
    _texture: Texture2D,
    shader: ShaderId,
    pipeline: Pipeline,
    bindings: Bindings,
    uniforms: shader::Uniforms,
}
impl Plane {
    fn new(camera: &Camera3D) -> Result<Self> {
        let white = crate::texture::upload_rgba8(1, 1, &[255; 4]);
        let recipe = Recipe {
            stage: Stage {
                blend: Blend::Alpha,
                constant_rgb: Some(Vec3::Y),
                vertex_alpha: true,
                ..Default::default()
            },
            textures: vec![white.clone()],
            lit: false,
            rank: 0,
            blend: 1,
            opaque: false,
            cull: 0,
            model: None,
            motion: None,
            deform: None,
        };
        let vertices =
            [[-1., -1., 0.], [1., -1., 0.], [1., 1., 0.], [-1., 1., 0.]].map(|position| {
                WorldVertex {
                    position,
                    uv: [0.5; 2],
                    color: [0, 255, 0, 127],
                    lightmap: [0.; 4],
                    normal: [0., 0., 1.],
                }
            });
        let texture = white.raw_miniquad_id();
        let ctx = unsafe { get_internal_gl() }.quad_context;
        crate::frame_profile::shader_creation();
        let shader = ctx.new_shader(
            ShaderSource::Glsl {
                vertex: shader::VERTEX,
                fragment: &crate::environment::fragment(shader::FRAGMENT),
            },
            shader::meta(),
        )?;
        crate::frame_profile::pipeline_creation();
        let pipeline = ctx.new_pipeline(
            &[BufferLayout {
                stride: 52,
                ..Default::default()
            }],
            &[
                VertexAttribute::new("position", VertexFormat::Float3),
                VertexAttribute::new("texcoord", VertexFormat::Float2),
                VertexAttribute::new("color0", VertexFormat::Byte4),
                VertexAttribute::new("lightmap", VertexFormat::Float4),
                VertexAttribute::new("normal", VertexFormat::Float3),
            ],
            shader,
            crate::render::depth_pipeline(crate::render::blend(1)),
        );
        let bindings = Bindings {
            vertex_buffers: vec![ctx.new_buffer(
                BufferType::VertexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(&vertices),
            )],
            index_buffer: ctx.new_buffer(
                BufferType::IndexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(&[0u16, 1, 2, 0, 2, 3]),
            ),
            images: vec![texture, texture],
        };
        let mut uniforms = uniforms(
            &recipe,
            0.,
            camera.position,
            Mat4::IDENTITY,
            camera.matrix(),
            true,
            Vec2::ONE,
        );
        uniforms.atmosphere(&crate::environment::Atmosphere::default(), camera.position);
        // Managed texture lifetime is held by the recipe until this constructor ends;
        // Plane must own it too rather than relying on a stale raw ID.
        Ok(Self {
            _texture: white,
            shader,
            pipeline,
            bindings,
            uniforms,
        })
    }
    fn draw(&self) {
        let ctx = unsafe { get_internal_gl() }.quad_context;
        ctx.apply_pipeline(&self.pipeline);
        ctx.apply_bindings(&self.bindings);
        ctx.apply_uniforms(UniformsSource::table(&self.uniforms));
        ctx.draw(0, 6, 1);
    }
}
impl Drop for Plane {
    fn drop(&mut self) {
        let ctx = unsafe { get_internal_gl() }.quad_context;
        ctx.delete_buffer(self.bindings.vertex_buffers[0]);
        ctx.delete_buffer(self.bindings.index_buffer);
        ctx.delete_pipeline(self.pipeline);
        ctx.delete_shader(self.shader);
    }
}
fn quad(z: f32, color: Color) -> Mesh {
    Mesh {
        vertices: [
            vec3(-1., -1., z),
            vec3(1., -1., z),
            vec3(1., 1., z),
            vec3(-1., 1., z),
        ]
        .map(|position| Vertex {
            position,
            uv: Vec2::splat(0.5),
            color: color.into(),
            normal: Vec4::ZERO,
        })
        .to_vec(),
        indices: vec![0, 1, 2, 0, 2, 3],
        texture: None,
    }
}
fn pixel() -> Color {
    let f = get_screen_data();
    f.get_pixel(f.width as u32 / 2, f.height as u32 / 2)
}
fn check_color(expected: Vec3, label: &str) -> Result<()> {
    let p = pixel();
    ensure!(
        vec3(p.r, p.g, p.b).distance(expected) < 0.035,
        "{label}: {p:?} expected {expected:?}"
    );
    Ok(())
}
pub async fn check() -> Result<()> {
    // Reproduce the milestone-2 weak-cache lifetime without a scene owner.
    crate::frame_profile::begin_creation_frame();
    for _ in 0..3 { drop(crate::render::world_material(12)?); }
    ensure!(crate::frame_profile::current_creations()==[3,3], "weak cache must reproduce per-frame compilation");
    println!("PASS reproduced weak-cache shader/pipeline recreation: 3 / 3");
    let owner:crate::render::WorldMaterialKeepalive=Default::default();
    crate::render::activate_world_materials(&owner);
    for frame in 0..180 {
        crate::frame_profile::begin_creation_frame();
        drop(crate::render::world_material(12)?);
        if frame>0 { ensure!(crate::frame_profile::current_creations()==[0,0], "scene owner recompiled frame {frame}"); }
    }
    println!("PASS scene-owned material reuse: 0 shader / 0 pipeline creations after warmup (180 frames)");
    ensure!(
        config().sort_alpha && config().actor_max_layers == 2,
        "fixture requires sort_alpha=true, actor_max_layers=2"
    );
    let camera = Camera3D {
        position: Vec3::Z * 10.,
        target: Vec3::ZERO,
        up: Vec3::Y,
        projection: Projection::Orthographics,
        fovy: 4.,
        z_near: 0.1,
        z_far: 50.,
        ..Default::default()
    };
    let atmosphere = crate::environment::Atmosphere::default();
    let mut gpu = render_fx::retained::Gpu::new()?;
    let plane = Plane::new(&camera)?;
    let output = std::path::Path::new("private/retained-alpha");
    std::fs::create_dir_all(output)?;
    crate::lighting::fixture_lights(vec![]);
    for reversed in [false, true] {
        clear_background(BLACK);
        set_camera(&camera);
        render_fx::retained::begin_view(&camera, 0., &atmosphere, true, true);
        let mut actors = [
            (-1., Color::new(0., 0., 1., 0.5)),
            (1., Color::new(1., 0., 0., 0.5)),
        ];
        if reversed {
            actors.reverse();
        }
        for (z, c) in actors {
            render_fx::effect(&quad(z, c), Blend::Alpha);
        }
        render_fx::retained::finish(&mut gpu, &[(10., 0)], true, |_| plane.draw());
        check_color(
            vec3(0.5, 0.25, 0.125),
            "world plane between rear/front actor groups",
        )?;
        crate::viewer::save_capture(&output.join(format!("joint-alpha-{reversed}.png")))?;
        next_frame().await;
    }
    println!("PASS retained world/actor/effect common alpha order independent of submission order");
    let counts = render_fx::retained::host_report();
    ensure!(
        counts["vita_3d_interop_flushes"] == 2
            && counts["vita_fx_gpu_draws"] == 2
            && counts["vita_fx_sorted_triangles"] == 0
            && counts["vita_fx_compacted_vertices"] == 0,
        "unexpected joint-pass operations: {counts}"
    );
    println!("[lg/vita-renderer] synthetic joint counts {counts}");
    clear_background(BLACK);
    set_camera(&camera);
    render_fx::retained::begin_view(&camera, 0., &atmosphere, true, true);
    render_fx::effect(&quad(1., Color::new(1., 0., 0., 0.5)), Blend::Alpha);
    render_fx::effect(&quad(-1., Color::new(0., 0., 1., 0.5)), Blend::Alpha);
    render_fx::retained::finish(&mut gpu, &[], true, |_| unreachable!());
    check_color(
        vec3(0.5, 0., 0.25),
        "whole-mesh coalescing preserves blend order",
    )?;
    let counts = render_fx::retained::host_report();
    ensure!(
        counts["vita_fx_gpu_draws"] == if config().merge_fx { 1 } else { 2 },
        "merge toggle draws: {counts}"
    );
    println!("PASS adjacent whole-effect merging; counts {counts}");
    next_frame().await;
    clear_background(BLACK);
    set_camera(&camera);
    render_fx::retained::begin_view(&camera, 0., &atmosphere, true, true);
    render_fx::submit(
        vec![render_fx::pass(
            quad(2., RED),
            &Stage::default(),
            true,
            false,
            Appearance::default(),
        )?],
        false,
    );
    render_fx::effect(&quad(1., BLUE), Blend::Alpha);
    render_fx::retained::finish(&mut gpu, &[], true, |_| unreachable!());
    check_color(Vec3::X, "opaque actor occludes rear effect")?;
    next_frame().await;
    // Readback uses raw texture bindings. A fresh frame resets backend caches;
    // the real viewer captures only after both 3-D layers have finished.
    clear_background(BLACK);set_camera(&camera);
    render_fx::retained::begin_view(&camera,0.,&atmosphere,true,true);
    render_fx::submit(vec![render_fx::pass(quad(2.,RED),&Stage::default(),true,false,Appearance::default())?],false);
    render_fx::retained::finish(&mut gpu,&[],true,|_|unreachable!());
    crate::render::clear_direct_view_depth();
    render_fx::retained::begin_view(&camera, 0., &atmosphere, true, false);
    render_fx::submit(
        vec![render_fx::pass(
            quad(-1., BLUE),
            &Stage::default(),
            true,
            false,
            Appearance::default(),
        )?],
        false,
    );
    render_fx::retained::finish(&mut gpu, &[], false, |_| unreachable!());
    check_color(Vec3::Z, "first-person layer after direct depth clear")?;
    next_frame().await;
    println!("PASS opaque depth and first-person direct depth clear");
    for alpha in [0., 0.12, 0.75, 1.] {
        clear_background(BLUE);
        set_camera(&camera);
        render_fx::retained::begin_view(&camera, 0., &atmosphere, true, true);
        let mut first = render_fx::pass(
            quad(0., WHITE),
            &Stage {
                blend: Blend::Alpha,
                constant_rgb: Some(Vec3::X),
                constant_alpha: Some(alpha),
                ..Default::default()
            },
            true,
            false,
            Appearance::default(),
        )?;
        // RGBA uses white vertices so gain supplies red exactly once.
        first.filter_fade = 0.;
        let mut second = render_fx::pass(
            quad(0., WHITE),
            &Stage {
                blend: Blend::Filter,
                constant_rgb: Some(Vec3::splat(0.5)),
                ..Default::default()
            },
            true,
            false,
            Appearance::default(),
        )?;
        second.filter_fade = 1. - alpha;
        render_fx::submit(vec![first, second], true);
        render_fx::retained::finish(&mut gpu, &[], true, |_| unreachable!());
        let filter = 1. - 0.5 * alpha;
        check_color(
            vec3(alpha * filter, 0., (1. - alpha) * filter),
            "low-alpha multiplicative ghost neutral colour",
        )?;
        next_frame().await;
    }
    println!("PASS retained low-alpha and multiplicative layer order");
    for (label, a, expected_fraction) in [
        (
            "dissolve",
            Appearance {
                dissolve: 0.5,
                ..Default::default()
            },
            0.5,
        ),
        (
            "ghost",
            Appearance {
                ghost: true,
                ..Default::default()
            },
            0.25,
        ),
    ] {
        clear_background(BLUE);
        set_camera(&camera);
        render_fx::retained::begin_view(&camera, 0., &atmosphere, true, true);
        render_fx::submit(
            vec![render_fx::pass(
                quad(0., RED),
                &Stage::default(),
                true,
                false,
                a,
            )?],
            false,
        );
        render_fx::retained::finish(&mut gpu, &[], true, |_| unreachable!());
        let f = get_screen_data();
        let (mut visible, mut total) = (0, 0);
        for y in f.height / 2 - 40..f.height / 2 + 40 {
            for x in f.width / 2 - 40..f.width / 2 + 40 {
                visible += usize::from(f.get_pixel(x as u32, y as u32).r > 0.9);
                total += 1;
            }
        }
        ensure!(
            (visible as f32 / total as f32 - expected_fraction).abs() < 0.04,
            "{label} discard changed: {visible}/{total}"
        );
        crate::viewer::save_capture(&output.join(format!("{label}.png")))?;
        next_frame().await;
        clear_background(BLACK);
        set_camera(&camera);
        render_fx::retained::begin_view(&camera, 0., &atmosphere, true, true);
        render_fx::submit(
            vec![render_fx::pass(
                quad(0., GREEN),
                &Stage::default(),
                true,
                false,
                Appearance::default(),
            )?],
            false,
        );
        render_fx::retained::finish(&mut gpu, &[], true, |_| unreachable!());
        check_color(Vec3::Y, "appearance reset")?;
        next_frame().await;
    }
    println!("PASS retained dissolve/ghost discard and independent appearance state");
    crate::lighting::fixture_lights(vec![crate::lighting::Light {
        position: Vec3::Z * 2.,
        color: Vec3::X * 0.4,
        radius: 10.,
        only_models: true,
        flare: false,
    }]);
    for model in [false, true] {
        clear_background(BLACK);
        set_camera(&camera);
        render_fx::retained::begin_view(&camera, 0., &atmosphere, false, true);
        render_fx::submit(
            vec![render_fx::pass(
                quad(0., Color::new(0.2, 0.2, 0.2, 1.)),
                &Stage::default(),
                model,
                true,
                Appearance::default(),
            )?],
            false,
        );
        render_fx::retained::finish(&mut gpu, &[], true, |_| unreachable!());
        let p = pixel();
        ensure!(
            if model {
                p.r > p.g + 0.25
            } else {
                (p.r - p.g).abs() < 0.025
            },
            "models-only lights: {model}/{p:?}"
        );
        next_frame().await;
    }
    crate::lighting::fixture_lights(vec![]);
    println!("PASS retained selected dynamic light and models-only mask");
    Ok(())
}
