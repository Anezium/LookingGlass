//! Synthetic ownership/defensive-delete regression. LG always uses sample_count=1;
//! its resolved targets do not reach the sample_count=0 ownership defect.
use macroquad::prelude::*;

#[macroquad::main("Render target lifetime regression")]
async fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    if mode != "guard" {
        for sample_count in [0, 1] {
            for _ in 0..16 {
                let target = render_target_ex(64, 64, RenderTargetParams { sample_count, depth: true });
                let clone = target.clone();
                set_camera(&Camera2D { render_target: Some(clone.clone()), ..Default::default() });
                clear_background(RED);
                set_default_camera();
                draw_texture(&target.texture, 0., 0., WHITE);
                next_frame().await;
                drop(target);
                drop(clone);
                // Runs Texture2D's deferred garbage collection after pass drop.
                next_frame().await;
            }
        }
        println!("PASS synthetic sample_count=0 and resolved sample_count=1 target cleanup");
    }
    if mode != "ownership" {
        let ctx = unsafe { get_internal_gl() }.quad_context;
        let texture = ctx.new_render_texture(Default::default());
        let pass = ctx.new_render_pass(texture, None);
        ctx.delete_render_pass(pass);
        ctx.delete_texture(texture);
        ctx.delete_texture(texture);
        println!("PASS defensive guard avoids a second delete of a possibly recycled GL name");
    }
}
