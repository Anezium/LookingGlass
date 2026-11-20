//! Whole additive packets whose preceding skin draw has already written opaque
//! alpha and identical depth. Keep mixed-alpha parts on the original route.
use super::{actor_lean, Pass, UNIFORM_FLOATS};
use crate::texture::RgbaImage;
use macroquad::{miniquad::TextureId, prelude::*};
use std::{cell::RefCell, collections::HashMap};

struct TextureProof {
    texture: Texture2D,
    blocks: [u64; 64],
    side: u16,
    glow: bool,
}

thread_local! {
    static TEXTURES: RefCell<HashMap<TextureId, TextureProof>> = RefCell::default();
}

pub(crate) fn note_texture(path: &str, texture: &Texture2D, image: &RgbaImage) {
    let (side, glow) = match path {
        "models/characters/jabberwock/skin01.ftx" => (256, false),
        "models/characters/jabberwock/skin01g.ftx" => (128, true),
        _ => return,
    };
    if image.width != side || image.height != side ||
        texture.width() != f32::from(side) || texture.height() != f32::from(side) {
        return;
    }

    // Certify complete aligned compression blocks. The pinned DXT5 encoder
    // preserves constant 255 alpha and constant black colour exactly.
    let side_px = usize::from(side);
    let mut blocks = [0; 64];
    for (y, row) in blocks.iter_mut().enumerate().take(side_px / 4) {
        for x in 0..side_px / 4 {
            let all_neutral = (y*4..y*4+4).all(|py| (x*4..x*4+4).all(|px| {
                let i = (py * side_px + px) * 4;
                image.pixels[i+3] == 255 && (!glow || image.pixels[i..i+3] == [0; 3])
            }));
            *row |= u64::from(all_neutral) << x;
        }
    }
    TEXTURES.with(|table| {
        let mut table = table.borrow_mut();
        if table.len() >= 16 {
            table.clear();
        }
        table.insert(texture.raw_miniquad_id(), TextureProof {
            texture: texture.weak_clone(), blocks, side, glow,
        });
    });
}

impl TextureProof {
    fn contains(&self, min: Vec2, max: Vec2) -> bool {
        // Stay off the texture edges with an apron for linear filtering and
        // coordinate rounding. Test all blocks in the padded UV rectangle.
        let side = f32::from(self.side);
        let lo = (min * side - Vec2::ONE) * 0.25;
        let hi = (max * side + Vec2::ONE) * 0.25;
        if lo.min_element() < 0. || hi.max_element() >= side * 0.25 {
            return false;
        }
        let (left, right) = (lo.x as u32, hi.x as u32);
        let required = (u64::MAX << left) & (u64::MAX >> (63 - right));
        self.blocks[lo.y as usize..=hi.y as usize].iter().all(|row| row & required == required)
    }
}

/// Slots must bind the same CPU-skinned vertex/index stream. The caller also
/// checks that both pipelines use the very same linked lean program.
pub(super) fn certified(base: &Pass, bu: &[f32; UNIFORM_FLOATS], glow: &Pass, gu: &[f32; UNIFORM_FLOATS]) -> bool {
    if base.effect || glow.effect || base.gpu_skin.is_some() || glow.gpu_skin.is_some()
        || base.blend_index != 1 || glow.blend_index != 2 || base.depth_equal || glow.depth_equal
        || base.cutout != 1. || glow.cutout != 0. || glow.fog != 1.
        || base.clamp || glow.clamp || base.gain.w != 1. || glow.gain != Vec4::ONE
        || bu[..32] != gu[..32] || !bu[..32].iter().all(|x| x.is_finite())
        || bu[48..52] != [0.; 4] || gu[48..52] != [0.; 4]
        || !actor_lean::admitted(base, bu) || !actor_lean::admitted(glow, gu) {
        return false;
    }
    let (Some(base_texture), Some(glow_texture)) = (&base.mesh.texture, &glow.mesh.texture) else {
        return false;
    };
    let mesh = glow.geometry();
    // Cap this experiment at the small, fully certified body parts. Larger
    // mixed parts have uncovered base samples and can change depth and alpha.
    if mesh.indices.len() != 44 * 3 {
        return false;
    }

    TEXTURES.with(|table| {
        let table = table.borrow();
        let get = |texture: &Texture2D, glow: bool| {
            table.get(&texture.raw_miniquad_id())
                .filter(|proof| proof.glow == glow && proof.texture == texture.weak_clone())
        };
        let (Some(base_proof), Some(glow_proof)) = (get(base_texture, false), get(glow_texture, true)) else {
            return false;
        };
        // Within these coordinate bounds, all live fog factors are defined,
        // including at mediump precision. At mode 1 they mix black with black.
        if gu[122] > 0. || gu[126] > 0. ||
            [gu[136], gu[146]].into_iter().any(|w| w > 0. && w < 1.) ||
            !gu[116..].iter().all(|v| v.abs() <= 8192.) {
            return false;
        }
        mesh.indices.chunks_exact(3).all(|triangle| {
            let mut min = Vec2::splat(f32::INFINITY);
            let mut max = Vec2::splat(f32::NEG_INFINITY);
            for &index in triangle {
                let Some(vertex) = mesh.vertices.get(usize::from(index)) else { return false; };
                if vertex.color[3] != 255 || !vertex.uv.is_finite() || !vertex.position.is_finite() ||
                    !(vertex.position.abs().max_element() <= 8192.) {
                    return false;
                }
                min = min.min(vertex.uv);
                max = max.max(vertex.uv);
            }
            base_proof.contains(min, max) && glow_proof.contains(min, max)
        })
    })
}
