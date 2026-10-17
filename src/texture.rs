use crate::assets::Assets;
use anyhow::{ensure, Context, Result};
use macroquad::prelude::Texture2D;
use std::collections::BTreeMap;

/// Longest side, in pixels, of an RGBA8 texture uploaded on Vita.
/// The Android fork's profiles use 1024, 512 and 256. Vita uses the middle value.
/// Desktop uploads ignore this cap.
#[cfg_attr(not(any(test, target_os = "vita")), allow(dead_code))]
pub const VITA_TEXTURE_MAX_DIM: u16 = 512;

/// Upload immutable game pixels in DXT5 on Vita; dynamic textures use the
/// separate upload helper so ordinary subimage updates remain valid.
pub(crate) fn upload_rgba8(width: u16, height: u16, pixels: &[u8]) -> Texture2D {
    #[cfg(target_os = "vita")]
    {
        return macroquad::miniquad::native::vita::with_dxt5_uploads(|| {
            upload_pixels(width, height, pixels, true)
        });
    }
    #[cfg(not(target_os = "vita"))]
    upload_pixels(width, height, pixels, true)
}

/// Upload RGBA8 pixels. On Vita, images larger than [`VITA_TEXTURE_MAX_DIM`] are
/// halved on the CPU until they fit, then uploaded. The packed game data is not
/// rewritten. Immutable uploads can opt into the exact DXT5 disk cache.
pub(crate) fn upload_dynamic_rgba8(width: u16, height: u16, pixels: &[u8]) -> Texture2D {
    upload_pixels(width, height, pixels, false)
}

fn upload_cap(immutable: bool, requested: Option<u16>, native: bool) -> Option<u16> {
    // Streaming movies update the original-sized texture every frame.
    let cap = requested.filter(|_| immutable);
    if native { Some(cap.unwrap_or(VITA_TEXTURE_MAX_DIM).min(VITA_TEXTURE_MAX_DIM)) } else { cap }
}

fn upload_pixels(width: u16, height: u16, pixels: &[u8], immutable: bool) -> Texture2D {
    // Vita's synchronous Texture2D upload includes backend DXT5 conversion
    // when called through upload_rgba8 (or reads its cached result); this reports
    // that combined cost.
    let _timing = crate::startup::WorkTimer::new(crate::startup::Work::TextureUpload);
    #[cfg(any(target_os = "vita", feature = "vita_renderer"))]
    {
        // Inspired by NextOs-Ports/Looking-Glass-NextOS 0c716a9 (felc18-blip).
        #[cfg(feature = "vita_renderer")]
        let cap = crate::render::vita_renderer::config().max_texture_dimension;
        #[cfg(not(feature = "vita_renderer"))]
        let cap = None;
        let cap = upload_cap(immutable, cap, cfg!(target_os = "vita"));
        let Some(cap) = cap else { return Texture2D::from_rgba8(width, height, pixels); };
        let (dst_w, dst_h, limited) = limit_rgba8(width, height, pixels, cap);
        #[cfg(target_os = "vita")]
        record_reduction(width, height, dst_w, dst_h);
        return Texture2D::from_rgba8(dst_w, dst_h, limited.as_ref());
    }
    #[cfg(all(not(target_os = "vita"), not(feature = "vita_renderer")))]
    {
        let _ = immutable;
        Texture2D::from_rgba8(width, height, pixels)
    }
}

#[cfg(any(test, target_os = "vita", feature = "vita_renderer"))]
fn limit_rgba8<'a>(
    width: u16,
    height: u16,
    pixels: &'a [u8],
    max_dim: u16,
) -> (u16, u16, std::borrow::Cow<'a, [u8]>) {
    use std::borrow::Cow;
    if width == 0 || height == 0 || (width <= max_dim && height <= max_dim) {
        return (width, height, Cow::Borrowed(pixels));
    }
    let need = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    if need == 0 || pixels.len() != need {
        return (width, height, Cow::Borrowed(pixels));
    }
    let mut w = width;
    let mut h = height;
    let mut current = Cow::Borrowed(pixels);
    while (w > max_dim || h > max_dim) && (w > 1 || h > 1) {
        let (nw, nh, next) = downsample_2x(w, h, current.as_ref());
        w = nw;
        h = nh;
        current = Cow::Owned(next);
    }
    (w, h, current)
}

/// Average each 2x2 block into one pixel. Edges of odd sizes repeat the last texel.
#[cfg(any(test, target_os = "vita", feature = "vita_renderer"))]
fn downsample_2x(width: u16, height: u16, rgba: &[u8]) -> (u16, u16, Vec<u8>) {
    let nw = (width / 2).max(1);
    let nh = (height / 2).max(1);
    let w = width as usize;
    let h = height as usize;
    let mut out = vec![0u8; nw as usize * nh as usize * 4];
    for y in 0..nh as usize {
        let y0 = (y * 2).min(h - 1);
        let y1 = (y * 2 + 1).min(h - 1);
        for x in 0..nw as usize {
            let x0 = (x * 2).min(w - 1);
            let x1 = (x * 2 + 1).min(w - 1);
            let i00 = (y0 * w + x0) * 4;
            let i10 = (y0 * w + x1) * 4;
            let i01 = (y1 * w + x0) * 4;
            let i11 = (y1 * w + x1) * 4;
            let dst = (y * nw as usize + x) * 4;
            for c in 0..4 {
                out[dst + c] = ((rgba[i00 + c] as u16
                    + rgba[i10 + c] as u16
                    + rgba[i01 + c] as u16
                    + rgba[i11 + c] as u16
                    + 2)
                    >> 2) as u8;
            }
        }
    }
    (nw, nh, out)
}

#[cfg(target_os = "vita")]
fn record_reduction(src_w: u16, src_h: u16, dst_w: u16, dst_h: u16) {
    let before = src_w as u64 * src_h as u64 * 4;
    let after = dst_w as u64 * dst_h as u64 * 4;
    if after >= before {
        return;
    }
    let mut totals = REDUCED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    totals.0 += 1;
    totals.1 += before - after;
    eprintln!(
        "vita texture limit: reduced {} textures, saved {} bytes ({}x{} -> {}x{})",
        totals.0, totals.1, src_w, src_h, dst_w, dst_h
    );
}

#[cfg(target_os = "vita")]
static REDUCED: std::sync::Mutex<(u64, u64)> = std::sync::Mutex::new((0, 0));

pub struct RgbaImage {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
}

pub fn decode_ftx(b: &[u8]) -> Result<RgbaImage> {
    ensure!(b.len() >= 12, "Truncated FTX header");
    let w = u32::from_le_bytes(b[0..4].try_into()?);
    let h = u32::from_le_bytes(b[4..8].try_into()?);
    ensure!(
        w > 0 && h > 0 && w <= 4096 && h <= 4096,
        "Invalid or oversized FTX dimensions: {w}x{h}"
    );
    let bytes = (w as usize)
        .checked_mul(h as usize)
        .and_then(|n| n.checked_mul(4))
        .context("FTX size overflow")?;
    ensure!(
        b.len() == 12 + bytes,
        "FTX payload size mismatch: {} versus {}",
        b.len(),
        12 + bytes
    );
    Ok(RgbaImage {
        width: w as u16,
        height: h as u16,
        pixels: b[12..].to_vec(),
    })
}

#[derive(Clone, Default, Debug)]
pub struct MaterialSpec {
    pub face_side: crate::materials::FaceSide,
    pub candidates: Vec<String>,
    pub hidden: bool,
    pub unlit: bool,
    pub transparent: bool,
    pub sky: bool,
    pub portal_sky: bool,
    pub camera_portal: bool,
    pub cloud_height: f32,
    pub deforms: Vec<crate::materials::Deform>,
    pub fog: Option<[f32; 4]>,
    pub stages: Vec<crate::materials::Stage>,
}

pub fn read_materials(assets: &mut Assets) -> Result<std::rc::Rc<BTreeMap<String, MaterialSpec>>> {
    #[cfg(not(target_os="vita"))]
    let reuse = std::env::var_os("LOOKING_GLASS_MATERIAL_CACHE_OFF").is_none();
    #[cfg(target_os="vita")]
    let reuse = true;
    if reuse { if let Some(specs) = &assets.material_specs { return Ok(specs.clone()); } }
    let _timing = crate::startup::WorkTimer::new(crate::startup::Work::MaterialParse);
    let files = assets
        .names()
        .filter(|n| n.ends_with(".shader"))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut result = BTreeMap::new();
    for name in files {
        let bytes = assets.read(&name)?;
        let text = String::from_utf8_lossy(&bytes);
        result.extend(
            crate::materials::parse(&text).with_context(|| format!("Material file {name}"))?,
        );
    }
    let result = std::rc::Rc::new(result);
    if reuse { assets.material_specs = Some(result.clone()); }
    Ok(result)
}

pub fn resolve(
    assets: &Assets,
    name: &str,
    specs: &BTreeMap<String, MaterialSpec>,
) -> Option<String> {
    let key = name.trim_start_matches('/').to_ascii_lowercase();
    let candidates = specs
        .get(&key)
        // Model declarations often name a shader using its image extension.
        .or_else(|| key.rsplit_once('.').and_then(|(stem, _)| specs.get(stem)))
        .map(|s| s.candidates.as_slice())
        .unwrap_or(&[]);
    for candidate in candidates
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(key.as_str()))
    {
        let candidate = candidate.trim_start_matches('/');
        let stem = candidate
            .rsplit_once('.')
            .map(|(s, _)| s)
            .unwrap_or(candidate);
        for ext in ["ftx", "tga", "jpg", "png"] {
            let path = format!("{stem}.{ext}");
            if assets.contains(&path) {
                return Some(path);
            }
        }
    }
    None
}

pub fn decode(assets: &mut Assets, name: &str) -> Result<RgbaImage> {
    let read_timing = crate::startup::WorkTimer::new(crate::startup::Work::TextureRead);
    let bytes = assets.read(name)?;
    drop(read_timing);
    let _decode_timing = crate::startup::WorkTimer::new(crate::startup::Work::TextureDecode);
    if name.ends_with(".ftx") {
        return decode_ftx(&bytes).with_context(|| name.to_owned());
    }
    let format = if name.ends_with(".tga") {
        image::ImageFormat::Tga
    } else if name.ends_with(".jpg") {
        image::ImageFormat::Jpeg
    } else {
        image::ImageFormat::Png
    };
    let mut reader = image::io::Reader::with_format(std::io::Cursor::new(bytes), format);
    let mut limits = image::io::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode()?.to_rgba8();
    ensure!(image.width() > 0 && image.height() > 0, "Empty image");
    Ok(RgbaImage {
        width: image.width() as u16,
        height: image.height() as u16,
        pixels: image.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ftx_preserves_rgba_and_rejects_bad_sizes() {
        let mut b = Vec::new();
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&[17, 23, 42, 255]);
        let image = decode_ftx(&b).unwrap();
        assert_eq!(image.pixels, [17, 23, 42, 255]);
        b.pop();
        assert!(decode_ftx(&b).is_err());
        b[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_ftx(&b).is_err());
        assert!(decode_ftx(&[]).is_err());
    }

    #[test]
    fn quality_caps_leave_streaming_movie_updates_at_their_original_dimensions() {
        assert_eq!(upload_cap(false, Some(128), false), None);
        assert_eq!(upload_cap(false, Some(128), true), Some(512));
        assert_eq!(upload_cap(true, Some(128), false), Some(128));
        assert_eq!(upload_cap(true, Some(128), true), Some(128));
        assert_eq!(upload_cap(true, None, false), None);
        assert_eq!(upload_cap(true, None, true), Some(512));
    }
    #[test]
    fn vita_cap_is_the_middle_android_profile() {
        assert_eq!(VITA_TEXTURE_MAX_DIM, 512);
    }

    #[test]
    fn images_inside_the_cap_are_not_copied() {
        let pixels = vec![7u8; 8 * 4 * 4];
        let (w, h, out) = limit_rgba8(8, 4, &pixels, VITA_TEXTURE_MAX_DIM);
        assert_eq!((w, h), (8, 4));
        assert_eq!(out.as_ref(), pixels.as_slice());
    }

    #[test]
    fn repeated_halving_stops_at_the_cap() {
        let pixels = vec![9u8; 8 * 4 * 4];
        let (w, h, out) = limit_rgba8(8, 4, &pixels, 3);
        assert_eq!((w, h), (2, 1));
        assert_eq!(out.len(), 8);
        assert!(out.iter().all(|p| *p == 9));
        let (w, h, _) = limit_rgba8(1024, 256, &vec![0u8; 1024 * 256 * 4], 512);
        assert_eq!((w, h), (512, 128));
        let (w, h, _) = limit_rgba8(638, 479, &vec![0u8; 638 * 479 * 4], 512);
        assert_eq!((w, h), (319, 239));
    }

    #[test]
    fn two_by_two_average_rounds_to_nearest() {
        // R (0+4+0+4+2)>>2 = 2, G (10+2)>>2 = 3, B (20+2)>>2 = 5, A stays 255.
        let pixels = [0, 10, 20, 255, 4, 0, 0, 255, 0, 0, 0, 255, 4, 0, 0, 255];
        let (w, h, out) = limit_rgba8(2, 2, &pixels, 1);
        assert_eq!((w, h), (1, 1));
        assert_eq!(out.as_ref(), &[2, 3, 5, 255]);
    }

    #[test]
    fn a_short_buffer_is_left_unchanged() {
        let pixels = [0, 1, 2, 3];
        let (w, h, out) = limit_rgba8(8, 8, &pixels, 2);
        assert_eq!((w, h), (8, 8));
        assert_eq!(out.as_ref(), &pixels);
    }
}
