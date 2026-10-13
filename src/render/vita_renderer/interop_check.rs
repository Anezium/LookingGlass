//! GPU regressions using the actual retained world and actor shaders, outside measured runs.
use super::*;
use crate::render_fx::{self, Appearance};
use macroquad::camera::Camera;
const RED: Color = Color::new(1., 0., 0., 1.);
const GREEN: Color = Color::new(0., 1., 0., 1.);
const BLUE: Color = Color::new(0., 0., 1., 1.);

/// Native GL parity exercises both legacy and producer-owned publication paths.
fn fixture_effect(mesh: &Mesh, blend: Blend) {
    if config().particle_producer_reuse {
        let publication=std::rc::Rc::new(render_fx::prepared::Geometry::new(render_fx::copy_mesh(mesh)));
        render_fx::effect_geometry(publication,mesh.texture.clone(),blend);
    } else { render_fx::effect(mesh,blend); }
}

struct Plane {
    _texture: Texture2D,
    shader: ShaderId,
    pipeline: Pipeline,
    bindings: Bindings,
    uniforms: shader::Uniforms,
}
impl Plane {
    fn new(camera: &Camera3D) -> Result<Self> { Self::with_opaque(camera, false) }
    fn with_opaque(camera: &Camera3D, opaque: bool) -> Result<Self> {
        let white = crate::texture::upload_rgba8(1, 1, &[255; 4]);
        let recipe = Recipe {
            stage: Stage {
                blend: if opaque { Blend::Opaque } else { Blend::Alpha },
                constant_rgb: Some(if opaque { Vec3::X } else { Vec3::Y }),
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
                    color: if opaque { [255, 0, 0, 255] } else { [0, 255, 0, 127] },
                    lightmap: [0.; 4],
                    normal: [0., 0., 1.],
                }
            });
        let texture = white.raw_miniquad_id();
        let ctx = unsafe { get_internal_gl() }.quad_context;
        crate::frame_profile::shader_creation();
        let _shader_name = crate::shader_cache::Name::new("interop-check");
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
            crate::render::depth_pipeline(crate::render::blend(if opaque { 0 } else { 1 })),
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
            fixture_effect(&quad(z, c), Blend::Alpha);
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
    fixture_effect(&quad(1., Color::new(1., 0., 0., 0.5)), Blend::Alpha);
    fixture_effect(&quad(-1., Color::new(0., 0., 1., 0.5)), Blend::Alpha);
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
    fixture_effect(&quad(1., BLUE), Blend::Alpha);
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
    // Reuse every ring page, then mutate vertices, topology, camera and appearance.
    // Both config arms save all 18 full frames for byte-for-byte PNG comparison.
    let mut hits=0u64;
    for frame in 0..18 {
        let mut camera=Camera3D {position:camera.position,target:camera.target,up:camera.up,
            projection:Projection::Orthographics,fovy:camera.fovy,z_near:camera.z_near,z_far:camera.z_far,..Default::default()};
        if frame>=12 { camera.position.x=0.15; camera.target.x=0.15; }
        clear_background(BLACK); set_camera(&camera);
        render_fx::retained::begin_view(&camera,frame as f32,&atmosphere,true,true);
        let ids=if frame>=15 {[1,0]} else {[0,1]};
        for id in ids {
            let mut m=quad(0., if id==0 {RED} else {GREEN});
            for v in &mut m.vertices {v.position.x=v.position.x*0.45+if id==0 {-0.6} else {0.6};}
            if frame>=6 {m.vertices[0].position.x+=0.1;}
            if frame>=9 {m.indices=vec![0,1,3,1,2,3];}
            let appearance=Appearance {dissolve:if frame>=12 {0.1} else {0.},..Default::default()};
            render_fx::submit(vec![render_fx::pass(m,&Stage::default(),true,false,appearance)?],false);
        }
        render_fx::retained::finish(&mut gpu,&[],true,|_|unreachable!());
        hits+=render_fx::retained::host_report()["vita_fx_prepare_upload_hits"].as_u64().unwrap_or(0);
        crate::viewer::save_capture(&output.join(format!("prepare-{frame:02}.png")))?;
        next_frame().await;
    }
    if config().fx_prepare_cache {ensure!(hits>0,"versioned ring preparation never hit");}
    println!("PASS FX preparation ring/mutation parity fixtures: 18 frames, generation_upload_hits={hits}");
    Ok(())
}

/// Exercise scaled depth, transparent interop, opaque upscale and native HUD.
pub async fn scale_check()->Result<()> {
    let camera=Camera3D{position:Vec3::Z*10.,target:Vec3::ZERO,up:Vec3::Y,projection:Projection::Orthographics,fovy:4.,z_near:0.1,z_far:50.,..Default::default()};
    let mut gpu=render_fx::retained::Gpu::new()?;
    crate::lighting::fixture_lights(vec![]);
    let red=crate::texture::upload_rgba8(1,1,&[255,0,0,255]);
    let world = Plane::with_opaque(&camera, true)?;
    if config().depth_sentinel {
        clear_background(BLACK); prepare_3d(BLACK); set_3d_camera(&camera);
        render_fx::retained::begin_view(&camera,0.,&Default::default(),true,true);
        begin_3d_pass(PassAction::Nothing); world.draw(); end_world_pass();
        // Negative control: deliberately destroy depth after the opaque sentinel.
        begin_3d_pass(PassAction::Clear { color: None, depth: Some(1.), stencil: None });
        render_fx::retained::finish(&mut gpu,&[],true,|_|unreachable!());
        ensure!(depth_sentinel::last_pass() == Some(false), "sentinel missed deliberate depth loss");
        next_frame().await;
        println!("PASS colour sentinel detects deliberate depth loss");
    }
    for frame in 0..120 {
        clear_background(BLACK); prepare_3d(BLACK); set_3d_camera(&camera);
        render_fx::retained::begin_view(&camera,0.,&Default::default(),true,true);
        begin_3d_pass(PassAction::Nothing);
        unsafe { macroquad::miniquad::gl::glDepthMask(1); }
        world.draw();
        end_world_pass(); // Exactly the production retained world-to-FX exit.
        render_fx::effect(&quad(-1., BLUE), Blend::Add);
        render_fx::effect(&quad(1., Color::new(0.,1.,0.,0.5)), Blend::Alpha);
        render_fx::retained::finish(&mut gpu,&[],true,|_|unreachable!());
        if config().depth_sentinel {
            ensure!(depth_sentinel::last_pass() == Some(true), "colour sentinel failed at frame {frame}");
        }
        present_3d();
        check_color(vec3(0.5,0.5,0.), "separate opaque world hides rear additive FX, front alpha survives")?;
        next_frame().await;
    }
    println!("PASS separate retained world-to-FX depth boundary: 120 frames scale={} fix={} sentinel={}",
        config().render_scale, config().scaled_depth_fix, config().depth_sentinel);
    for frame in 0..120 {
        crate::frame_profile::begin_creation_frame();
        clear_background(BLACK);prepare_3d(BLACK);set_3d_camera(&camera);
        render_fx::retained::begin_view(&camera,0.,&Default::default(),true,true);
        let mut actor=quad(if frame%2==0{1.}else{-1.},WHITE);
        actor.texture=Some(red.clone());
        render_fx::skin(&actor);
        fixture_effect(&quad(-2.,Color::new(0.,0.,1.,0.5)),Blend::Alpha);
        render_fx::retained::finish(&mut gpu,&[],true,|_|unreachable!());
        present_3d();check_color(Vec3::X,"scaled opaque depth hides rear effect")?;
        set_default_camera();draw_rectangle(screen_width()/2.-4.,screen_height()/2.-4.,8.,8.,GREEN);
        check_color(Vec3::Y,"native HUD after opaque upscale")?;
        if frame>0 {ensure!(crate::frame_profile::current_creations()==[0,0],"scaled path recompiled at frame {frame}");}
        if frame==119 {crate::viewer::save_capture(std::path::Path::new("private/retained-alpha/scaled-depth-hud.png"))?;}
        next_frame().await;
    }
    println!("PASS scaled depth, alpha interop, native HUD and zero program creation after startup (120 frames)");
    Ok(())
}
