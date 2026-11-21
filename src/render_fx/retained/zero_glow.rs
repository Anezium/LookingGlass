//! Whole additive packets whose preceding skin draw has already written opaque
//! alpha and identical depth. Keep mixed-alpha parts on the original route.
use super::*;
use crate::texture::RgbaImage;
use macroquad::{miniquad::TextureId, prelude::*};
use std::{cell::RefCell, collections::HashMap};

struct TextureProof {
    texture: Texture2D,
    blocks: [u64; 64],
    side: u16,
    glow: bool,
    opaque: bool,
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
            opaque: glow && image.pixels.chunks_exact(4).all(|pixel| pixel[3] == 255),
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

/// An identical CPU stream or GPU skin occurrence supplies the same vertex
/// positions in both pipelines. GPU skinning derives UV from the bind mesh
/// for ordinary layers and emits a constant vertex alpha of one.
fn skin_pair(base: &Pass, glow: &Pass, same_slot: bool) -> bool {
    match (&base.gpu_skin, &glow.gpu_skin) {
        (None, None) => same_slot,
        (Some(base), Some(glow)) => !base.environment && !glow.environment
            && std::rc::Rc::ptr_eq(&base.occurrence, &glow.occurrence)
            && glow.occurrence.bounds().is_some_and(|(lo, hi)| bounded(lo) && bounded(hi)),
        _ => false,
    }
}

fn bounded(position: Vec3) -> bool {
    position.is_finite() && position.abs().max_element() <= 8192.
}

fn compatible_pair(base: &Pass, bu: &[f32; UNIFORM_FLOATS], glow: &Pass, gu: &[f32; UNIFORM_FLOATS]) -> bool {
    !base.effect && !glow.effect
        && base.blend_index == 1 && glow.blend_index == 2 && !base.depth_equal && !glow.depth_equal
        && base.cutout == 1. && glow.cutout == 0. && glow.fog == 1.
        && !base.clamp && !glow.clamp && base.gain.w == 1. && glow.gain == Vec4::ONE
        && bu[..32] == gu[..32] && bu[..32].iter().all(|x| x.is_finite())
        && bu[48..52] == [0.; 4] && gu[48..52] == [0.; 4]
        && actor_lean::admitted(base, bu) && actor_lean::admitted(glow, gu)
}

fn bounded_fog(gu: &[f32; UNIFORM_FLOATS]) -> bool {
    gu[122] <= 0. && gu[126] <= 0.
        && ![gu[136], gu[146]].into_iter().any(|w| w > 0. && w < 1.)
        && gu[116..].iter().all(|v| v.abs() <= 8192.)
}

/// The caller checks that both draws select the same linked lean program.
pub(super) fn certified(base: &Pass, bu: &[f32; UNIFORM_FLOATS], glow: &Pass, gu: &[f32; UNIFORM_FLOATS], same_slot: bool) -> bool {
    if !compatible_pair(base, bu, glow, gu) { return false; }
    let (Some(base_texture), Some(glow_texture)) = (&base.mesh.texture, &glow.mesh.texture) else {
        return false;
    };
    let mesh = glow.geometry();
    // Cap this experiment at the small, fully certified body parts. Larger
    // mixed parts have uncovered base samples and can change depth and alpha.
    if mesh.indices.len() != 44 * 3 || !skin_pair(base, glow, same_slot) {
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
                if !vertex.uv.is_finite()
                    || (glow.gpu_skin.is_none() && (vertex.color[3] != 255 || !bounded(vertex.position))) {
                    return false;
                }
                min = min.min(vertex.uv);
                max = max.max(vertex.uv);
            }
            base_proof.contains(min, max) && glow_proof.contains(min, max)
        })
    })
}

/// The load-time material owns the reduced stream alongside its two textures.
/// The original stream stays available for the base and for a refused pair.
pub(crate) struct Subset {
    buffer: Option<BufferId>,
    original: usize,
    kept: usize,
}

impl Drop for Subset {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer {
            unsafe { get_internal_gl() }.quad_context.delete_buffer(buffer);
        }
    }
}

impl Subset {
    pub(crate) fn load(surface: &super::super::Surface, source: &crate::skeletal::Surface) -> Option<Self> {
        let [base, glow] = surface.layers.as_slice() else { return None; };
        let [base_image] = base.textures.as_slice() else { return None; };
        let [glow_image] = glow.textures.as_slice() else { return None; };
        let simple_opaque_alpha = |layer: &super::super::Layer| {
            super::super::actor_cache::simple(&layer.stage) && !layer.stage.vertex_alpha
        };
        if surface.transparent || base.stage.blend != crate::materials::Blend::Alpha
            || glow.stage.blend != crate::materials::Blend::Add
            || !simple_opaque_alpha(base) || !simple_opaque_alpha(glow)
            || source.indices.len() % 3 != 0 {
            return None;
        }
        let kept = TEXTURES.with(|table| {
            let table = table.borrow();
            let base = table.get(&base_image.raw_miniquad_id())
                .filter(|proof| !proof.glow && proof.texture == base_image.weak_clone())?;
            let glow = table.get(&glow_image.raw_miniquad_id())
                .filter(|proof| proof.glow && proof.opaque && proof.texture == glow_image.weak_clone())?;
            let mut kept = Vec::new();
            for triangle in source.indices.chunks_exact(3) {
                let a = source.vertices.get(usize::from(triangle[0]))?.uv;
                let b = source.vertices.get(usize::from(triangle[1]))?.uv;
                let c = source.vertices.get(usize::from(triangle[2]))?.uv;
                let omit = a.is_finite() && b.is_finite() && c.is_finite() && {
                    let min = a.min(b).min(c);
                    let max = a.max(b).max(c);
                    base.contains(min, max) && glow.contains(min, max)
                };
                if !omit {
                    kept.extend_from_slice(triangle);
                }
            }
            Some(kept)
        })?;
        let original = source.indices.len();
        let omitted = (original - kept.len()) / 3;
        println!("[lg/actor-glow-subset] surface={} triangles={} omitted={omitted}",
            source.name, original / 3);
        if omitted == 0 {
            return None;
        }
        let buffer = (!kept.is_empty()).then(|| {
            unsafe { get_internal_gl() }.quad_context.new_buffer(
                BufferType::IndexBuffer, BufferUsage::Immutable, BufferSource::slice(&kept))
        });
        Some(Self { buffer, original, kept: kept.len() })
    }

    pub(super) fn draw_count(&self, full: BufferId) -> (BufferId, i32) {
        (self.buffer.unwrap_or(full), self.kept as i32)
    }

    pub(super) fn record(&self, gpu: bool) {
        let omitted = (self.original - self.kept) / 3;
        count(if gpu { "vita_actor_glow_subset_gpu_triangles" } else { "vita_actor_glow_subset_cpu_triangles" },
            omitted as u64);
        crate::spike_log::actor_glow_subset(omitted as u32);
    }
}

/// Accept only the live bindings from the same load-time recipe. Equal depth
/// can leave a fractional-alpha base fragment on top of an opaque one. Its
/// matching glow fragment stays in the list and saturates destination alpha.
/// Other deleted fragments have already established their own opaque depth.
pub(super) fn subset<'a>(base: &Pass, bu: &[f32; UNIFORM_FLOATS], glow: &'a Pass,
    gu: &[f32; UNIFORM_FLOATS], same_slot: bool) -> Option<&'a Subset> {
    let surface = glow.mesh.surface.as_ref()?;
    let subset = surface.glow_subset.as_ref()?;
    if !base.mesh.surface.as_ref().is_some_and(|b| std::rc::Rc::ptr_eq(b, surface))
        || !compatible_pair(base, bu, glow, gu)
        || base.geometry().indices.len() != subset.original
        || glow.geometry().indices.len() != subset.original
        || base.mesh.texture.as_ref() != surface.layers[0].textures.first()
        || glow.mesh.texture.as_ref() != surface.layers[1].textures.first()
        || !skin_pair(base, glow, same_slot)
        || !bounded_fog(gu)
        || (glow.gpu_skin.is_none() && (gu[136] > 0. || gu[146] > 0.)) {
        return None;
    }
    Some(subset)
}
