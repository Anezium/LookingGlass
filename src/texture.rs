use crate::assets::Assets;
use anyhow::{ensure, Context, Result};
use macroquad::prelude::Texture2D;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

/// Longest side, in pixels, of an RGBA8 texture uploaded on Vita.
/// The Android fork's profiles use 1024, 512 and 256. Vita uses the middle value.
/// Desktop uploads ignore this cap.
#[cfg_attr(not(any(test, target_os = "vita")), allow(dead_code))]
pub const VITA_TEXTURE_MAX_DIM: u16 = 512;

/// The sampler state an upload is keyed for. Wrap and filter are GPU state, not
/// texels, so a readback cannot see them: a shared upload never changes its
/// sampler after a consumer receives it. Repeat has its own key and is set at
/// upload, before the texture is handed out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sampler {
    Clamp,
    Repeat,
}

impl Sampler {
    /// The cache key for one path at one sampler state. Clamp keeps the bare
    /// path, so the clamp entries read exactly as they did before.
    fn key(self, path: &str) -> String {
        match self {
            Sampler::Clamp => path.to_owned(),
            Sampler::Repeat => format!("{path}\u{1}repeat"),
        }
    }
}

/// The launch's shared uploads for `load_texture_share`, and how they were served.
#[derive(Default)]
struct Shared {
    uploaded: BTreeMap<String, Entry>,
    hits: u32,
    misses: u32,
}

struct Entry {
    texture: Texture2D,
    zero_alpha: bool,
    sampler: Sampler,
}

thread_local! {
    static SHARED: RefCell<Shared> = RefCell::new(Shared::default());
    // Image-only equivalence within one original base upload. Never stores a Surface.
    static MATERIAL_IMAGES: RefCell<BTreeMap<String, SharedTexture>> = RefCell::new(BTreeMap::new());
    /// Loads whose textures may be cached. Outside a window the cache is inert.
    static SHARE_WINDOW: Cell<u32> = const { Cell::new(0) };
}

/// One load's window on the shared-upload cache. Outside a window nothing is
/// retained; dropping the last guard clears every entry. A load holds one from
/// its first upload to its last, so an early `?` or a failed recovery cannot
/// keep strong texture owners across a load, map or retirement boundary.
/// The guard is `#[must_use]`: dropping it at once would cache nothing.
#[must_use = "the load window closes as soon as the guard drops"]
pub(crate) struct LoadScope(());

impl Drop for LoadScope {
    fn drop(&mut self) {
        let closed = SHARE_WINDOW.with(|depth| {
            let left = depth.get().saturating_sub(1);
            depth.set(left);
            left == 0
        });
        if closed {
            release_shared();
        }
    }
}

/// Open a load window. Nested scopes share one window; the outermost drop clears.
pub(crate) fn load_scope() -> LoadScope {
    SHARE_WINDOW.with(|depth| depth.set(depth.get() + 1));
    LoadScope(())
}

fn share_window() -> bool {
    SHARE_WINDOW.with(|depth| depth.get() > 0)
}

/// One uploaded texture and the facts a caller cannot recompute after a reuse.
#[derive(Clone)]
pub(crate) struct SharedTexture {
    pub texture: Texture2D,
    /// Every texel is fully transparent, as `fill_cuts` reads it.
    pub zero_alpha: bool,
    /// Unique per request, even on a cache hit: the private upload OFF would create.
    pub upload_identity: u64,
}

fn zero_alpha(pixels: &[u8]) -> bool {
    !pixels.is_empty() && pixels.chunks_exact(4).all(|p| p[3] == 0)
}

/// `load_texture_share` is on in the RC12 release input and off in every earlier one;
/// without `vita_renderer` the key has no reader, so off.
fn share_enabled() -> bool {
    #[cfg(test)]
    if let Some(on) = TEST_SHARE.with(|v| v.get()) { return on; }
    #[cfg(feature = "vita_renderer")]
    { crate::render::vita_renderer::config().load_texture_share }
    #[cfg(not(feature = "vita_renderer"))]
    { false }
}

#[cfg(test)]
thread_local! { static TEST_SHARE: Cell<Option<bool>> = const { Cell::new(None) }; }
#[cfg(test)]
pub(crate) fn set_share_for_test(on: Option<bool>) { TEST_SHARE.with(|v| v.set(on)); }

/// Whether this upload may be kept and reused. The key alone never retains a
/// texture between loads; the load window is required as well.
fn retains_shared_uploads() -> bool {
    share_enabled() && share_window()
}

/// Decode and upload one asset path, or reuse the copy this load already uploaded.
///
/// Two callers that name the same resolved path and sampler get the same pixels
/// from the same archive entry, so the second read, decode and GPU allocation are
/// pure repetition. The texture is returned as a strong clone, so a shared entry
/// is never freed while any of its users lives. The load guard clears the keys at
/// the end of its window (`release_shared`), on success and every error alike.
pub(crate) fn decode_upload_shared(assets: &mut Assets, path: &str) -> Result<Texture2D> {
    decode_upload_shared_sampled(assets, path, Sampler::Clamp)
}

/// The same, keeping the zero-alpha verdict for `fill_cuts`.
pub(crate) fn decode_upload_shared_full(assets: &mut Assets, path: &str) -> Result<SharedTexture> {
    decode_upload_keyed(assets, path, Sampler::Clamp)
}

/// The same, for a caller that samples with repeat wrap. Repeat and clamp cannot
/// share a GPU texture without changing sampling, so they use distinct keys and
/// the repeat upload sets its wrap once, before any consumer sees it.
pub(crate) fn decode_upload_shared_tiling(assets: &mut Assets, path: &str) -> Result<Texture2D> {
    decode_upload_shared_sampled(assets, path, Sampler::Repeat)
}

/// The same, for a caller that names the sampler its base texture needs.
pub(crate) fn decode_upload_shared_sampled(
    assets: &mut Assets,
    path: &str,
    sampler: Sampler,
) -> Result<Texture2D> {
    decode_upload_keyed(assets, path, sampler).map(|s| s.texture)
}

/// RC11 registered several surfaces against a locally cached base image. Its
/// last-registration lookup gave those surfaces the same stage images. Preserve
/// that image equivalence without selecting anyone else's material recipe.
/// The original base upload, layer slot and animation frame fence independent
/// uploads/frames, even if load_texture_share gives all of them one GPU binding.
pub(crate) fn material_image(
    assets: &mut Assets, base: &Texture2D, base_identity: Option<u64>,
    layer: usize, frame: usize, path: &str, sampler: Sampler,
) -> Result<SharedTexture> {
    let owner = base_identity.map_or_else(
        || format!("private:{:?}", base.raw_miniquad_id()), |id| format!("upload:{id}"));
    let key = format!("{owner}:{layer}:{frame}:{}", sampler.key(path));
    if share_window() {
        if let Some(image) = MATERIAL_IMAGES.with(|images| images.borrow().get(&key).cloned()) {
            return Ok(image);
        }
    }
    let image = decode_upload_keyed(assets, path, sampler)?;
    if share_window() {
        MATERIAL_IMAGES.with(|images| images.borrow_mut().insert(key, image.clone()));
    }
    Ok(image)
}

pub(crate) fn decode_upload_keyed(assets: &mut Assets, path: &str, sampler: Sampler) -> Result<SharedTexture> {
    thread_local! { static NEXT_UPLOAD: Cell<u64> = const { Cell::new(1) }; }
    let upload_identity = NEXT_UPLOAD.with(|n| { let v = n.get(); n.set(v.checked_add(1).expect("upload identity overflow")); v });
    let share = retains_shared_uploads();
    let key = sampler.key(path);
    if share {
        let hit = SHARED.with(|s| {
            let mut s = s.borrow_mut();
            match s.uploaded.get(&key) {
                Some(entry) => {
                    let found = (entry.texture.clone(), entry.zero_alpha, entry.sampler);
                    s.hits += 1;
                    Some(found)
                }
                None => { s.misses += 1; None }
            }
        });
        if let Some((texture, zero_alpha, stored)) = hit {
            ensure!(
                stored == sampler,
                "shared upload {path} was reused with a different sampler"
            );
            let shared = SharedTexture { texture, zero_alpha, upload_identity };
            verify_hit(assets, path, &shared)?;
            return Ok(shared);
        }
    }
    let image = decode(assets, path)?;
    let texture = upload_rgba8(image.width, image.height, &image.pixels);
    texture.set_filter(macroquad::texture::FilterMode::Linear);
    #[cfg(feature = "vita_renderer")]
    if crate::render::vita_renderer::config().actor_zero_glow {
        crate::render_fx::retained::zero_glow::note_texture(path, &texture, &image);
    }
    if sampler == Sampler::Repeat {
        set_wrap_repeat(&texture);
    }
    let shared = SharedTexture { texture, zero_alpha: zero_alpha(&image.pixels), upload_identity };
    if share {
        SHARED.with(|s| {
            let mut s = s.borrow_mut();
            s.uploaded.insert(key, Entry {
                texture: shared.texture.clone(),
                zero_alpha: shared.zero_alpha,
                sampler,
            });
        });
    }
    Ok(shared)
}

/// The one place a shared upload's wrap is set: at upload, before it is handed
/// out. No caller mutates a shared texture's sampler afterwards, because wrap is
/// not part of the pixel bytes the reuse check compares.
fn set_wrap_repeat(texture: &Texture2D) {
    unsafe {
        macroquad::prelude::get_internal_gl().quad_context.texture_set_wrap(
            texture.raw_miniquad_id(),
            macroquad::miniquad::TextureWrap::Repeat,
            macroquad::miniquad::TextureWrap::Repeat,
        );
    }
}

/// Host check only: a reused upload must read back exactly like a fresh one.
/// Off unless `LOOKING_GLASS_TEXTURE_SHARE_VERIFY` is set; the Vita has no readback.
fn verify_hit(assets: &mut Assets, path: &str, shared: &SharedTexture) -> Result<()> {
    #[cfg(not(target_os = "vita"))]
    if std::env::var_os("LOOKING_GLASS_TEXTURE_SHARE_VERIFY").is_some() {
        let image = decode(assets, path)?;
        let fresh = upload_rgba8(image.width, image.height, &image.pixels);
        let (a, b) = (fresh.get_texture_data(), shared.texture.get_texture_data());
        ensure!(
            a.width == b.width && a.height == b.height && a.bytes == b.bytes,
            "shared upload differs from a fresh one: {path}"
        );
    }
    #[cfg(target_os = "vita")]
    let _ = (assets, path, shared);
    Ok(())
}

/// End one load's sharing window and report what it saved, as one line. Also
/// called directly before a retirement sequence and after a rebuild attempt.
pub(crate) fn release_shared() {
    MATERIAL_IMAGES.with(|images| images.borrow_mut().clear());
    SHARED.with(|s| {
        let mut s = s.borrow_mut();
        if (s.hits, s.misses) != (0, 0) {
            eprintln!("[lg/texture-share] hits={} misses={} uploaded={}", s.hits, s.misses, s.uploaded.len());
        }
        *s = Shared::default();
    });
}

/// Upload immutable game pixels in DXT5 on Vita; dynamic textures use the
/// separate upload helper so ordinary subimage updates remain valid.
pub(crate) fn upload_rgba8(width: u16, height: u16, pixels: &[u8]) -> Texture2D {
    #[cfg(target_os = "vita")]
    let texture = macroquad::miniquad::native::vita::with_dxt5_uploads(|| upload_pixels(width,height,pixels,true));
    #[cfg(not(target_os = "vita"))]
    let texture = upload_pixels(width,height,pixels,true);
    #[cfg(feature="vita_renderer")]
    crate::render_fx::corpse_opaque::note_texture(&texture,pixels);
    texture
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
        let _r13_memory = crate::allocation_census::Scope::new(2);
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
        let _r13_memory = crate::allocation_census::Scope::new(2);
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
    struct ShareReset;
    impl Drop for ShareReset {
        fn drop(&mut self) { set_share_for_test(None); }
    }
    /// Cached uploads, which need a GPU to create in a test.
    fn cached_entries() -> usize {
        SHARED.with(|s| s.borrow().uploaded.len())
    }
    #[test]
    fn tiling_and_clamped_paths_never_share_a_key() {
        let p = "models/fx/cap.ftx";
        assert_ne!(Sampler::Clamp.key(p), Sampler::Repeat.key(p));
        assert_eq!(Sampler::Clamp.key(p), p);
        assert_eq!(Sampler::Repeat.key(p), Sampler::Repeat.key(p));
        assert_ne!(Sampler::Repeat.key("a"), Sampler::Repeat.key("b"));
        // One key can only ever hold one sampler, which is what a hit re-checks.
        for path in ["a", "models/fx/cap.ftx", "textures/special/purple.ftx"] {
            assert_ne!(Sampler::Clamp.key(path), Sampler::Repeat.key(path), "{path}");
        }
    }
    #[test]
    fn the_share_cache_retains_only_inside_a_load_window() {
        let _reset = ShareReset;
        set_share_for_test(Some(true));
        assert!(!retains_shared_uploads(), "a boundary keeps no texture owners");
        let load = load_scope();
        assert!(retains_shared_uploads(), "a load's guard opens the window");
        let nested = load_scope();
        drop(nested);
        assert!(retains_shared_uploads(), "an inner scope keeps the outer load open");
        drop(load);
        assert!(!retains_shared_uploads(), "the last guard closes and clears");
        set_share_for_test(None);
        assert!(!retains_shared_uploads(), "the compiled default is off");
    }
    #[test]
    fn a_failed_load_closes_its_window_like_a_successful_one() {
        fn failing() -> Result<()> {
            let _load = load_scope();
            anyhow::bail!("destination build failed")
        }
        fn working() -> Result<()> {
            let _load = load_scope();
            Ok(())
        }
        let _reset = ShareReset;
        set_share_for_test(Some(true));
        assert!(failing().is_err());
        assert!(!retains_shared_uploads(), "the error path dropped the guard");
        assert_eq!(cached_entries(), 0);
        working().unwrap();
        assert!(!retains_shared_uploads());
        // A panic unwinds through the same guard, so a load cannot strand owners.
        let panicked = std::panic::catch_unwind(|| {
            let _load = load_scope();
            panic!("load failed hard");
        });
        assert!(panicked.is_err());
        assert!(!retains_shared_uploads());
        assert_eq!(cached_entries(), 0);
    }
    #[test]
    fn zero_alpha_reads_every_texel() {
        assert!(zero_alpha(&[0, 0, 0, 0, 9, 9, 9, 0]));
        assert!(!zero_alpha(&[0, 0, 0, 0, 9, 9, 9, 1]));
        assert!(!zero_alpha(&[]));
    }
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
