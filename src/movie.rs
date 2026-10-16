//! Bounded RoQ video reader for the user's original movies. No external player required.
//! Format facts were cross-checked against FFmpeg's RoQ decoder; this implementation
//! owns two RGBA frames and decodes the small vector codebooks directly.
use crate::{assets::Assets, audio::Audio, input::Input, preferences::Preferences, ui::Ui};
use anyhow::{ensure, Context, Result};
use macroquad::prelude::*;
use std::io::{self, Read};
use std::time::{Duration, Instant};

fn word(b: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        b.get(at..at + 2)
            .context("Truncated movie word")?
            .try_into()?,
    ))
}
fn header(reader: &mut impl Read) -> Result<u16> {
    let mut h = [0; 8];
    reader
        .read_exact(&mut h)
        .context("Truncated movie header")?;
    ensure!(
        word(&h, 0)? == 0x1084 && h[2..6] == [255; 4],
        "Not a RoQ movie"
    );
    let fps = word(&h, 6)?;
    ensure!((1..=60).contains(&fps), "Invalid movie rate");
    Ok(fps)
}
fn chunk(reader: &mut impl Read) -> Result<Option<(u16, usize, u16)>> {
    let mut h = [0; 8];
    // A clean end is allowed only between chunks, never inside their headers.
    if reader.read(&mut h[..1])? == 0 {
        return Ok(None);
    }
    reader
        .read_exact(&mut h[1..])
        .context("Truncated movie chunk")?;
    let size = u32::from_le_bytes(h[2..6].try_into()?) as usize;
    ensure!(size <= 16 * 1024 * 1024, "Invalid movie chunk size");
    Ok(Some((word(&h, 0)?, size, word(&h, 6)?)))
}
struct Codes<'a> {
    bytes: &'a [u8],
    at: usize,
    flags: u16,
    left: u8,
}
impl Codes<'_> {
    fn byte(&mut self) -> Result<u8> {
        let value = *self.bytes.get(self.at).context("Truncated movie vector")?;
        self.at += 1;
        Ok(value)
    }
    fn next(&mut self) -> Result<u16> {
        if self.left == 0 {
            self.flags = word(self.bytes, self.at)?;
            self.at += 2;
            self.left = 8;
        }
        self.left -= 1;
        Ok((self.flags >> (self.left * 2)) & 3)
    }
}
pub struct Video {
    chunk: Vec<u8>,
    started: bool,
    pub width: u16,
    pub height: u16,
    pub fps: u16,
    pub frames: usize,
    pub decoded: usize,
    // Conversion is paid once per changed codebook, never per drawn pixel.
    cells: [[[u8; 4]; 4]; 256],
    pub last_convert: Duration,
    groups: [[u8; 4]; 256],
    previous: Vec<u8>,
    working: Vec<u8>,
}
impl Video {
    /// First pass: validate the stream and count frames without retaining chunk payloads.
    /// Reopen the same entry and pass that reader to next for the sequential decode.
    pub fn new(mut reader: impl Read) -> Result<Self> {
        let fps = header(&mut reader)?;
        let (mut width, mut height, mut frames, mut total) = (0, 0, 0, 8);
        while let Some((kind, size, _)) = chunk(&mut reader)? {
            total += 8 + size;
            ensure!(total <= 128 * 1024 * 1024, "Invalid movie size");
            match kind {
                0x1001 => {
                    ensure!(width == 0 && size == 8, "Repeated/invalid movie dimensions");
                    let mut info = [0; 8];
                    reader
                        .read_exact(&mut info)
                        .context("Truncated movie dimensions")?;
                    width = word(&info, 0)?;
                    height = word(&info, 2)?;
                    ensure!(
                        (16..=2048).contains(&width)
                            && (16..=2048).contains(&height)
                            && width % 16 == 0
                            && height % 16 == 0,
                        "Invalid movie dimensions"
                    );
                    ensure!(
                        word(&info, 4)? == 8 && word(&info, 6)? == 4,
                        "Unsupported movie block sizes"
                    );
                    continue;
                }
                0x1002 => ensure!(width > 0, "Codebook before movie dimensions"),
                0x1011 => {
                    ensure!(width > 0, "Frame before dimensions");
                    frames += 1;
                }
                _ => anyhow::bail!("Unsupported movie chunk {kind:x}"),
            }
            let copied = io::copy(&mut reader.by_ref().take(size as u64), &mut io::sink())?;
            ensure!(copied == size as u64, "Truncated movie chunk payload");
        }
        ensure!(
            frames > 0 && frames <= fps as usize * 600,
            "Invalid movie duration"
        );
        let mut blank = vec![0; width as usize * height as usize * 4];
        for p in blank.chunks_exact_mut(4) {
            p[3] = 255;
        }
        Ok(Self {
            chunk: Vec::new(),
            started: false,
            width,
            height,
            fps,
            frames,
            decoded: 0,
            cells: [[[0, 0, 0, 255]; 4]; 256],
            last_convert: Duration::ZERO,
            groups: [[0; 4]; 256],
            previous: blank.clone(),
            working: blank,
        })
    }
    pub fn pixels(&self) -> &[u8] {
        &self.previous
    }
    fn cell(&mut self, index: usize, x: usize, y: usize, scale: usize) {
        let c = self.cells[index];
        for row in 0..2 * scale {
            for col in 0..2 * scale {
                let pixel = c[row / scale * 2 + col / scale];
                let start = ((y + row) * self.width as usize + x + col) * 4;
                self.working[start..start + 4].copy_from_slice(&pixel);
            }
        }
    }
    fn block(
        &mut self,
        codes: &mut Codes<'_>,
        x: usize,
        y: usize,
        size: usize,
        mean: (i32, i32),
    ) -> Result<()> {
        match codes.next()? {
            0 => {} // Retain the corresponding block from two frames earlier.
            1 => {
                let mv = codes.byte()?;
                let sx = x as i32 + 8 - (mv >> 4) as i32 - mean.0;
                let sy = y as i32 + 8 - (mv & 15) as i32 - mean.1;
                ensure!(
                    sx >= 0
                        && sy >= 0
                        && sx + size as i32 <= self.width as i32
                        && sy + size as i32 <= self.height as i32,
                    "Movie motion outside frame"
                );
                for row in 0..size {
                    let a = ((sy as usize + row) * self.width as usize + sx as usize) * 4;
                    let b = ((y + row) * self.width as usize + x) * 4;
                    self.working[b..b + size * 4].copy_from_slice(&self.previous[a..a + size * 4]);
                }
            }
            2 => {
                let group = self.groups[codes.byte()? as usize];
                let half = size / 2;
                for (i, index) in group.into_iter().enumerate() {
                    self.cell(index as usize, x + i % 2 * half, y + i / 2 * half, half / 2);
                }
            }
            3 if size == 8 => {
                for i in 0..4 {
                    self.block(codes, x + i % 2 * 4, y + i / 2 * 4, 4, mean)?;
                }
            }
            3 => {
                for i in 0..4 {
                    self.cell(codes.byte()? as usize, x + i % 2 * 2, y + i / 2 * 2, 1);
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    }
    pub fn next(&mut self, reader: &mut impl Read) -> Result<bool> {
        self.last_convert = Duration::ZERO;
        if !self.started {
            ensure!(
                header(reader)? == self.fps,
                "Movie rate changed between passes"
            );
            self.started = true;
        }
        let mut buffer = std::mem::take(&mut self.chunk);
        let result = (|| {
            while let Some((kind, size, arg)) = chunk(reader)? {
                buffer.resize(size, 0);
                reader
                    .read_exact(&mut buffer)
                    .context("Truncated movie chunk payload")?;
                let bytes = buffer.as_slice();
                if kind == 0x1002 {
                    let cells = if arg >> 8 == 0 {
                        256
                    } else {
                        (arg >> 8) as usize
                    };
                    let groups = if arg & 255 == 0 && size > cells * 6 {
                        256
                    } else {
                        (arg & 255) as usize
                    };
                    ensure!(
                        size == cells * 6 + groups * 4,
                        "Invalid movie codebook length"
                    );
                    let converting = Instant::now();
                    for (i, cell) in bytes[..cells * 6].chunks_exact(6).enumerate() {
                        let cb = cell[4] as f32 - 128.;
                        let cr = cell[5] as f32 - 128.;
                        for (pixel, &l) in self.cells[i].iter_mut().zip(&cell[..4]) {
                            let l = l as f32;
                            *pixel = [
                                (l + 1.402 * cr).round().clamp(0., 255.) as u8,
                                (l - 0.344136 * cb - 0.714136 * cr).round().clamp(0., 255.) as u8,
                                (l + 1.772 * cb).round().clamp(0., 255.) as u8,
                                255,
                            ];
                        }
                    }
                    self.last_convert += converting.elapsed();
                    for (i, group) in bytes[cells * 6..].chunks_exact(4).enumerate() {
                        self.groups[i].copy_from_slice(group);
                    }
                } else if kind == 0x1011 {
                    let mut codes = Codes {
                        bytes,
                        at: 0,
                        flags: 0,
                        left: 0,
                    };
                    for y in (0..self.height as usize).step_by(16) {
                        for x in (0..self.width as usize).step_by(16) {
                            for i in 0..4 {
                                self.block(
                                    &mut codes,
                                    x + i % 2 * 8,
                                    y + i / 2 * 8,
                                    8,
                                    ((arg >> 8) as u8 as i8 as i32, arg as u8 as i8 as i32),
                                )?;
                            }
                        }
                    }
                    // The encoder may leave one unused flag word after the last block.
                    ensure!(
                        bytes.len() - codes.at <= 2,
                        "Trailing movie frame data: {} bytes after frame {}",
                        bytes.len() - codes.at,
                        self.decoded
                    );
                    std::mem::swap(&mut self.previous, &mut self.working);
                    self.decoded += 1;
                    if self.decoded == 1 {
                        self.working.copy_from_slice(&self.previous);
                    }
                    return Ok(true);
                }
            }
            Ok(false)
        })();
        self.chunk = buffer;
        result
    }
}

/// Per-film timing samples are bounded by the validated 600-second frame limit.
#[derive(Default)]
struct Samples(Vec<f64>);
impl Samples {
    fn push(&mut self, duration: Duration) { self.0.push(duration.as_secs_f64() * 1000.); }
    fn summary(&mut self) -> String {
        self.0.sort_by(f64::total_cmp);
        let n = self.0.len();
        if n == 0 { return "n=0".into(); }
        format!("n={n},p50={:.3},p95={:.3},max={:.3}",
            self.0[n / 2], self.0[(n - 1) * 95 / 100], self.0[n - 1])
    }
}
struct MovieEndLog {
    name: String,
    frames: usize,
    duration: f64,
    started: Instant,
    played: f64,
    reason: &'static str,
    decode: Samples,
    convert: Samples,
    upload: Samples,
    dropped: usize,
    stalls: usize,
    #[cfg(target_os = "vita")]
    peak: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
impl MovieEndLog {
    fn decoded(&mut self, total: Duration, convert: Duration) {
        self.decode.push(total.saturating_sub(convert));
        self.convert.push(convert);
    }
}
impl Drop for MovieEndLog {
    fn drop(&mut self) {
        eprintln!("[lg/film] id={} owner=roq end={} frames={} nominal_s={:.3} wall_s={:.3} soundtrack_s={:.3} dropped_presentations={} backpressure={} decode_read_ms=[{}] convert_ms=[{}] upload_ms=[{}]",
            self.name, self.reason, self.frames, self.duration, self.started.elapsed().as_secs_f64(), self.played,
            self.dropped, self.stalls, self.decode.summary(), self.convert.summary(), self.upload.summary());
        #[cfg(target_os = "vita")]
        eprintln!("[lg/film] id={} reader_peak={}", self.name, self.peak.load(std::sync::atomic::Ordering::Relaxed));
    }
}
// RoQ motion references require every frame to be decoded. Drop intermediate
// presentations, not dependencies. If even this budget cannot catch the clock,
// pause the soundtrack while draining debt across input/presentation boundaries.
const DECODE_BUDGET: usize = 2;
fn decode_target(decoded: usize, wanted: usize, frames: usize) -> usize {
    wanted.min(frames).min(decoded.saturating_add(DECODE_BUDGET))
}

pub fn bypass_requested() -> bool {
    #[cfg(feature = "vita_renderer")]
    { crate::render::vita_renderer::config().story_film_bypass }
    #[cfg(not(feature = "vita_renderer"))]
    { false }
}

/// Returns false when the window was closed; true when watched or deliberately skipped.
pub async fn play(
    assets: &mut Assets,
    audio: &mut Audio,
    ui: &Ui,
    input: &mut Input,
    preferences: &Preferences,
    name: &str,
) -> Result<bool> {
    let mut skip = crate::cinematic::Skip::film(name, input.film_skip_label(preferences));
    ensure!(matches!(name, "opening" | "ending"), "Unknown story movie");
    // Bench, renderbench and fullbench restore a save. They must not open a film.
    #[cfg(target_os = "vita")]
    if crate::frame_profile::story_movies_blocked() {
        skip.event("bypass reason=benchmark");
        skip.handoff("benchmark-bypass");
        return Ok(true);
    }
    if bypass_requested() {
        skip.event("bypass reason=explicit-config validation_started=false");
        skip.handoff("config-bypass");
        return Ok(true);
    }
    let path = format!("video/{name}.roq");
    let mut video = Video::new(assets.open_stream(&path)?)?;
    let mut reader = assets.open_stream(&path)?;
    let mut movie_log = MovieEndLog {
        name: name.to_owned(), frames: video.frames,
        duration: video.frames as f64 / video.fps as f64,
        started: Instant::now(), played: 0., reason: "error",
        decode: Samples::default(), convert: Samples::default(), upload: Samples::default(),
        dropped: 0, stalls: 0,
        #[cfg(target_os = "vita")]
        peak: reader.peak_counter(),
    };
    let decoding = Instant::now();
    video.next(&mut reader)?;
    movie_log.decoded(decoding.elapsed(), video.last_convert);
    let uploading = Instant::now();
    let texture = crate::texture::upload_dynamic_rgba8(video.width, video.height, video.pixels());
    movie_log.upload.push(uploading.elapsed());
    eprintln!("[lg/film] id={name} owner=roq start frames={} nominal_s={:.3} decode_budget={DECODE_BUDGET}", video.frames, movie_log.duration);
    texture.set_filter(FilterMode::Linear);
    audio.update(0., Vec3::ZERO, 0., true, true);
    let soundtrack = audio.movie_sound(assets, &format!("video/{name}.mp3"))?;
    let mut elapsed = 0_f64;
    let mut paused = false;
    let mut clock_at = Instant::now();
    let mut backpressure = false;
    let mut clock_active = false;
    input.suppress();
    // Clear the menu confirmation press before arming the movie's skip control.
    next_frame().await;
    loop {
        let focused = crate::look::window_focused();
        input.update(preferences, focused);
        if !focused {
            paused = true;
        }
        if is_quit_requested() {
            if let Some(s) = &soundtrack {
                s.stop();
            }
            audio.flush_movie();
            movie_log.played = elapsed;
            movie_log.reason = "quit";
            skip.event("decoder_exit reason=quit soundtrack_stop=true");
            skip.handoff("quit");
            return Ok(false);
        }
        if focused && (input.ui(KeyCode::P) || input.pad_pressed("Start")
            || (crate::preferences::Preferences::vita_emulation() && is_key_pressed(KeyCode::Escape))) {
            paused = !paused;
        }
        let active = focused && !paused;
        if let Some(s) = &soundtrack {
            if active && !backpressure {
                s.play();
            } else {
                s.pause();
            }
        }
        let now = Instant::now();
        let wall_dt = if clock_active { now.duration_since(clock_at).as_secs_f64() } else { 0. };
        clock_at = now;
        clock_active = active && !backpressure;
        skip.note_input(input.film_skip(preferences), is_key_down(KeyCode::Enter), is_key_down(KeyCode::E));
        if skip.update(
            Some("story-movie"),
            active,
            input.film_skip(preferences),
        ) {
            movie_log.reason = "skipped";
            break;
        }
        if active {
            if !backpressure { elapsed += wall_dt; }
            if let Some(s) = soundtrack.as_ref().filter(|s| !s.empty()) {
                elapsed = audio.movie_position(s).as_secs_f64();
            }
            let wanted = (elapsed * video.fps as f64) as usize + 1;
            movie_log.played = elapsed;
            let target = decode_target(video.decoded, wanted, video.frames);
            backpressure = target < wanted.min(video.frames);
            if backpressure {
                movie_log.stalls += 1;
                if let Some(s) = &soundtrack { s.pause(); }
            }
            let before = video.decoded;
            while video.decoded < target {
                let decoding = Instant::now();
                ensure!(video.next(&mut reader)?, "Incomplete movie decode");
                movie_log.decoded(decoding.elapsed(), video.last_convert);
            }
            if video.decoded != before {
                movie_log.dropped += (video.decoded - before).saturating_sub(1);
                let uploading = Instant::now();
                texture.update_from_bytes(video.width as u32, video.height as u32, video.pixels());
                movie_log.upload.push(uploading.elapsed());
            }
            if wanted > video.frames && video.decoded == video.frames {
                movie_log.reason = "complete";
                break;
            }
        }
        clear_background(BLACK);
        gl_use_default_material();
        set_default_camera();
        let scale =
            (screen_width() / video.width as f32).min(screen_height() / video.height as f32);
        let size = vec2(video.width as f32, video.height as f32) * scale;
        draw_texture_ex(
            &texture,
            (screen_width() - size.x) * 0.5,
            (screen_height() - size.y) * 0.5,
            WHITE,
            DrawTextureParams {
                dest_size: Some(size),
                ..Default::default()
            },
        );
        skip.draw(ui, &input.film_skip_label(preferences));
        if !active {
            ui.paused(if input.desktop_pad() || crate::preferences::Preferences::vita_emulation() {
                "Start to resume film"
            } else {
                "P to resume film"
            });
        }
        next_frame().await;
    }
    skip.event(&format!("decoder_exit reason={} decoded={}", movie_log.reason, video.decoded));
    if let Some(s) = &soundtrack { s.stop(); }
    audio.flush_movie();
    skip.event("soundtrack_stop=true pcm_prefetch_flushed=true");
    skip.handoff(movie_log.reason);
    input.suppress();
    next_frame().await;
    movie_log.played = elapsed;
    Ok(true)
}

pub fn check(assets: &mut Assets) -> Result<()> {
    use rodio::Source;
    for name in ["opening", "ending"] {
        let path = format!("video/{name}.roq");
        let mut video = Video::new(assets.open_stream(&path)?)?;
        let mut reader = assets.open_stream(&path)?;
        while video.next(&mut reader)? {
            if [1, 301, 901].contains(&video.decoded) {
                image::save_buffer(
                    format!("private/movie-{name}-{}.png", video.decoded),
                    video.pixels(),
                    video.width as u32,
                    video.height as u32,
                    image::ColorType::Rgba8,
                )?;
            }
        }
        ensure!(video.decoded == video.frames, "Incomplete movie decode");
        drop(reader);
        let sound = crate::audio::movie_soundtrack(assets, &format!("video/{name}.mp3"))?;
        let rate = sound.sample_rate();
        let channels = sound.channels();
        let samples = sound.count();
        let duration = samples as f32 / rate as f32 / channels as f32;
        ensure!(
            (duration - video.frames as f32 / video.fps as f32).abs() < 10.,
            "Movie audio/video length mismatch: audio {duration}, video {}",
            video.frames as f32 / video.fps as f32
        );
        println!(
            "PASS {name}: {} frames, {}x{}, {}fps; original soundtrack decoded ({duration:.2}s including padding)",
            video.frames, video.width, video.height, video.fps
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Vec<u8> {
        let mut b = vec![
            0x84, 0x10, 255, 255, 255, 255, 30, 0, 1, 0x10, 8, 0, 0, 0, 0, 0, 16, 0, 16, 0, 8, 0,
            4, 0,
        ];
        b.extend_from_slice(&[0x11, 0x10, 2, 0, 0, 0, 0, 0, 0, 0]);
        b
    }
    // Deliberately has no Seek and splits headers/payloads across short reads.
    struct ShortReader<'a>(&'a [u8]);
    impl Read for ShortReader<'_> {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            let count = output.len().min(1);
            self.0.read(&mut output[..count])
        }
    }
    #[test]
    fn streaming_passes_accept_short_reads_and_preserve_frames() -> Result<()> {
        let mut bytes = sample();
        bytes.extend_from_slice(&[0x11, 0x10, 2, 0, 0, 0, 0, 0, 0, 0]);
        let mut video = Video::new(ShortReader(&bytes))?;
        assert_eq!(video.frames, 2);
        let mut reader = ShortReader(&bytes);
        for _ in 0..2 {
            assert!(video.next(&mut reader)?);
            assert!(video.pixels().chunks_exact(4).all(|p| p == [0, 0, 0, 255]));
        }
        assert!(!video.next(&mut reader)?);
        assert_eq!(video.decoded, video.frames);
        // Truncation that appears only in the reopened stream is also rejected.
        let mut video = Video::new(bytes.as_slice())?;
        let mut truncated = &bytes[..bytes.len() - 1];
        assert!(video.next(&mut truncated)?);
        assert!(video.next(&mut truncated).is_err());
        Ok(())
    }
    #[test]
    fn catchup_budget_preserves_dependencies_and_caps_each_poll() {
        let mut decoded = 1;
        for _ in 0..100 {
            let next = decode_target(decoded, 3000, 3731);
            assert!(next - decoded <= 2);
            decoded = next;
        }
        assert_eq!(decoded, 201);
        assert_eq!(decode_target(3730, usize::MAX, 3731), 3731);
        assert_eq!(decode_target(10, 5, 3731), 5);
    }
    #[test]
    fn movie_bounds_and_truncation_are_checked() {
        let b = sample();
        for n in 0..b.len() {
            assert!(Video::new(&b[..n]).is_err());
        }
        let mut invalid = b.clone();
        invalid[16] = 255;
        assert!(Video::new(invalid.as_slice()).is_err());
        let mut v = Video::new(b.as_slice()).unwrap();
        let mut reader = b.as_slice();
        assert!(v.next(&mut reader).unwrap());
        assert!(!v.next(&mut reader).unwrap());
    }
    #[test]
    fn invalid_motion_is_rejected_without_reading_outside_frames() {
        let mut b = sample();
        b[26] = 3;
        b[32] = 0;
        b[33] = 0x40;
        b.push(0xff);
        let mut v = Video::new(b.as_slice()).unwrap();
        let mut reader = b.as_slice();
        assert!(v.next(&mut reader).is_err());
    }
}
