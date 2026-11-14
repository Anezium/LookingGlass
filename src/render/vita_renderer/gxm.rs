//! Opaque world draws straight to sceGxm, behind `world_gxm`. Default off. Vita only in effect:
//! the host runs the same records through `world_list_submit`, and tests run this logic on a mock.
//!
//! vitaGL stays the owner of everything but the draw itself. A draw that changes pipeline goes
//! through it, which sets programs, blend, cull, depth, viewport and scene. The draws after it
//! change only what vitaGL would re-issue anyway: fragment texture 0, the vertex streams, the two
//! default uniform buffers, and the draw. Nothing here reads vitaGL's private structs. What a
//! group needs is captured from the sceGxm calls vitaGL makes for it the first time it draws.
//!
//! `world_gxm` modes:
//! - `shadow`: vitaGL draws everything. After each draw, compare the arguments it passed with what
//!   the record predicts, uniform bytes included. Counts mismatches and prints the first.
//! - `live`: direct draws as above.
//! - `proof`: as `live`, and every so often the same frame is drawn twice into the target, through
//!   vitaGL and directly, and the bytes compared. `proof_control` draws both trials through vitaGL
//!   and must always read equal. `proof_sabotage` leaves one draw out and must read different.
#![allow(dead_code)]
use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, Ordering::Relaxed};

pub(crate) const MAX_STREAMS: usize = 8;
pub(crate) const MAX_TEXTURES: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Mode { Off, Shadow, Live, Proof, ProofControl, ProofSabotage }
impl Mode {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "off" => Some(Self::Off), "shadow" => Some(Self::Shadow), "live" => Some(Self::Live),
            "proof" => Some(Self::Proof), "proof_control" => Some(Self::ProofControl), "proof_sabotage" => Some(Self::ProofSabotage),
            _ => None,
        }
    }
    /// A proof arm's own frames still draw directly, so learning goes on between proofs.
    fn frame_way(self) -> Way {
        match self { Self::Off => Way::Vitagl, Self::Shadow => Way::Shadow, _ => Way::Direct }
    }
}
/// How one pass runs on a Vita. A host always runs `Vitagl`, which means GL.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Way { Vitagl, Shadow, Direct }
/// Which pass this is: the frame's own, or one trial of a proof frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Pass { Frame, ProofVitagl, ProofSecond }
/// A proof frame every this many frames, past a warmup that lets records fill, up to a limit.
const PROOF_WARMUP: u64 = 120;
const PROOF_EVERY: u64 = 30;
const PROOF_FRAMES: u64 = 8;
/// The trial draw `proof_sabotage` leaves out. The comparison must see it.
const SABOTAGE_DRAW: u64 = 5;
/// The GPU and CPU side of one renderer's `world_gxm`, past what the mock tests exercise.
pub(crate) struct State {
    mode: Mode,
    records: Vec<Option<Record>>,
    fields: Vec<(String, u16, u16)>,
    /// For the full program and the low one.
    layouts: [Option<Program>; 2],
    /// The vertex and fragment program vitaGL used, per pipeline class and program.
    /// Room for 10 blend classes (`exact_blends` adds classes from 5 on); a slot past the end
    /// is not recorded and that draw is checked without its programs.
    programs: [Option<(usize, usize)>; 64],
    direct: Direct,
    pub(crate) shadow: Shadow,
    pub(crate) direct_draws: u64,
    pub(crate) vitagl_draws: u64,
    frames: u64,
    proofs: Proofs,
    buffers: (Vec<u8>, Vec<u8>),
    /// Draws and direct draws of the second trial in all proof frames, and draws `proof_sabotage` left out.
    pub(crate) second_trial_draws: u64,
    pub(crate) second_trial_direct: u64,
    pub(crate) sabotage_fired: u64,
    sabotaged_this_proof: bool,
    /// Captures nothing was learned from, program pairs refused, proof frames skipped for memory.
    pub(crate) refused_captures: u64,
    layout_refused: [bool; 2],
    pub(crate) proofs_skipped: u64,
}
/// What the proof frames found, for the one line after the window.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Proofs { pub frames: u64, pub equal: u64, pub different: u64, pub first_different_byte: Option<usize>, pub different_bytes: usize, pub bytes: usize }
impl State {
    pub(crate) fn new(mode: Mode, groups: usize, meta: &macroquad::miniquad::ShaderMeta) -> Self {
        Self { mode, records: if mode == Mode::Off { Vec::new() } else { vec![None; groups] }, fields: field_table(meta), layouts: [None, None], programs: [None; 64],
            direct: Direct::default(), shadow: Shadow::default(), direct_draws: 0, vitagl_draws: 0, frames: 0, proofs: Proofs::default(), buffers: (Vec::new(), Vec::new()), second_trial_draws: 0, second_trial_direct: 0, sabotage_fired: 0, sabotaged_this_proof: false, refused_captures: 0, layout_refused: [false; 2], proofs_skipped: 0 }
    }
    /// Boxed so the survey's closing line can find it. Main thread only, like the renderer.
    pub(crate) fn published(mode: Mode, groups: usize, meta: &macroquad::miniquad::ShaderMeta) -> Box<Self> {
        let mut state = Box::new(Self::new(mode, groups, meta));
        CURRENT.store(&mut *state, Relaxed);
        state
    }
    pub(crate) fn way(&self, pass: Pass) -> Way {
        match (pass, self.mode) {
            (Pass::Frame, mode) => mode.frame_way(),
            (Pass::ProofVitagl, _) | (Pass::ProofSecond, Mode::ProofControl) => Way::Shadow,
            (Pass::ProofSecond, _) => Way::Direct,
        }
    }
    pub(crate) fn direct_used(&self) -> bool { self.mode.frame_way() == Way::Direct }
    /// Once per world pass. True when this frame is a proof frame.
    pub(crate) fn proof_due(&mut self) -> bool {
        if !matches!(self.mode, Mode::Proof | Mode::ProofControl | Mode::ProofSabotage) { return false; }
        self.frames += 1;
        self.frames >= PROOF_WARMUP && (self.frames - PROOF_WARMUP) % PROOF_EVERY == 0 && self.proofs.frames < PROOF_FRAMES
    }
    pub(crate) fn take_proof_buffers(&mut self) -> (Vec<u8>, Vec<u8>) { std::mem::take(&mut self.buffers) }
    pub(crate) fn give_proof_buffers(&mut self, a: Vec<u8>, b: Vec<u8>) { self.buffers = (a, b); }
    pub(crate) fn proof_result(&mut self, a: &[u8], b: &[u8]) { self.proofs.observe(a, b); self.sabotaged_this_proof = false; }
    pub(crate) fn proof_skipped(&mut self) { self.proofs_skipped += 1; }
    /// Once per draw of a pass.
    #[inline] pub(crate) fn note_draw(&mut self, pass: Pass) { if pass == Pass::ProofSecond { self.second_trial_draws += 1; } }
    /// Leave out the first draw at or after `SABOTAGE_DRAW` that really went direct, once per proof frame.
    /// A pipeline change or a group with no record at that position would otherwise hide the sabotage.
    pub(crate) fn sabotage_due(&mut self, pass: Pass, drawn: u64) -> bool {
        if self.mode != Mode::ProofSabotage || pass != Pass::ProofSecond || drawn < SABOTAGE_DRAW || self.sabotaged_this_proof { return false; }
        self.sabotaged_this_proof = true;
        self.sabotage_fired += 1;
        true
    }
    pub(crate) fn summary(&self, map: &str) -> Option<String> {
        if self.mode == Mode::Off { return None; }
        let p = &self.proofs;
        Some(format!("{} mode={:?} direct_draws={} vitagl_draws={} vertex_uniform_bytes={} fragment_uniform_bytes={} second_trial_draws={} second_trial_direct={} sabotage_fired={} refused_captures={} layout_refused={} proofs_skipped={} gxm_errors={} proof_frames={} proof_equal={} proof_different={} proof_first_byte={:?} proof_different_bytes={} proof_frame_bytes={}",
            self.shadow.summary(map), self.mode, self.direct_draws, self.vitagl_draws,
            self.layouts.iter().flatten().map(|l| l.vertex.size).max().unwrap_or(0), self.layouts.iter().flatten().map(|l| l.fragment.size).max().unwrap_or(0),
            self.second_trial_draws, self.second_trial_direct, self.sabotage_fired, self.refused_captures, self.layout_refused.iter().filter(|r| **r).count(), self.proofs_skipped, gxm_errors(),
            p.frames, p.equal, p.different, p.first_different_byte, p.different_bytes, p.bytes))
    }
}
static CURRENT: AtomicPtr<State> = AtomicPtr::new(std::ptr::null_mut());
fn gxm_errors() -> u32 {
    #[cfg(target_os = "vita")]
    { vita::errors() }
    #[cfg(not(target_os = "vita"))]
    { 0 }
}
pub(crate) fn withdraw(state: &mut State) {
    let _ = CURRENT.compare_exchange(state, std::ptr::null_mut(), Relaxed, Relaxed);
}
pub(crate) fn summary_line(map: &str) -> Option<String> {
    let state = CURRENT.load(Relaxed);
    if state.is_null() { None } else { unsafe { (*state).summary(map) } }
}
impl Proofs {
    pub(crate) fn observe(&mut self, a: &[u8], b: &[u8]) {
        self.frames += 1;
        self.bytes = a.len();
        if a == b && !a.is_empty() { self.equal += 1; return; }
        self.different += 1;
        let differing = a.iter().zip(b).filter(|(x, y)| x != y).count() + a.len().abs_diff(b.len());
        self.different_bytes = self.different_bytes.max(differing);
        if self.first_different_byte.is_none() { self.first_different_byte = a.iter().zip(b).position(|(x, y)| x != y); }
    }
}

/// The sceGxm calls vitaGL made for one draw. `0` and null mean it did not make that call.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Capture {
    pub vertex_program: usize,
    pub fragment_program: usize,
    /// Fragment textures by unit.
    pub textures: [usize; MAX_TEXTURES],
    pub stream_mask: u8,
    pub streams: [usize; MAX_STREAMS],
    pub vertex_uniforms: usize,
    pub fragment_uniforms: usize,
    pub primitive: u32,
    pub format: u32,
    pub index: usize,
    pub count: u32,
    pub wrap: u32,
    pub draws: u32,
}
impl Capture {
    /// True when this is one whole draw a direct draw can repeat: exactly one draw call with a real
    /// index pointer, both programs, and every set stream on one buffer. A direct draw sets all
    /// streams to one address, so anything else stays with vitaGL.
    pub(crate) fn learnable(&self) -> bool {
        self.draws == 1 && self.index != 0 && self.count != 0 && self.wrap == self.count
            && self.vertex_program != 0 && self.fragment_program != 0
            && self.stream_mask & 1 != 0 && self.streams[0] != 0
            && (0..MAX_STREAMS).all(|i| self.stream_mask & (1 << i) == 0 || self.streams[i] == self.streams[0])
    }
    pub(crate) const EMPTY: Self = Self { vertex_program: 0, fragment_program: 0, textures: [0; MAX_TEXTURES], stream_mask: 0, streams: [0; MAX_STREAMS],
        vertex_uniforms: 0, fragment_uniforms: 0, primitive: 0, format: 0, index: 0, count: 0, wrap: 0, draws: 0 };
}

/// Where a program wants a uniform field: `words` of the 143 go to byte `offset` of its default buffer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct FieldSlot { pub first: u16, pub words: u16, pub offset: u16 }
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub(crate) struct Layout { pub slots: Vec<FieldSlot>, pub size: u32 }
impl Layout {
    /// Same bytes vitaGL's `unif_*buffer` holds once every field has been written. Words the program
    /// has no parameter for stay out of it, as they do there. `dst` must be `size` bytes.
    #[inline] pub(crate) unsafe fn write(&self, dst: *mut u8, bits: &[u32; 143]) {
        std::ptr::write_bytes(dst, 0, self.size as usize);
        for s in &self.slots {
            std::ptr::copy_nonoverlapping(bits.as_ptr().add(s.first as usize).cast::<u8>(), dst.add(s.offset as usize), s.words as usize * 4);
        }
    }
    /// A safe view for checks: the bytes `write` produces.
    pub(crate) fn image(&self, bits: &[u32; 143]) -> Vec<u8> {
        let mut out = vec![0; self.size as usize];
        unsafe { self.write(out.as_mut_ptr(), bits); }
        out
    }
}
/// What the direct path needs of one program pair.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub(crate) struct Program { pub vertex: Layout, pub fragment: Layout, pub diffuse_unit: u32 }
impl Layout {
    /// Every slot lies inside the buffer, so `write` and `image` stay inside it.
    pub(crate) fn fits(&self) -> bool {
        self.slots.iter().all(|s| u32::from(s.offset) + u32::from(s.words) * 4 <= self.size)
    }
}
impl Program {
    pub(crate) fn valid(&self) -> bool {
        self.vertex.fits() && self.fragment.fits() && (self.diffuse_unit as usize) < MAX_TEXTURES
    }
}
/// `shader::meta()` fields as (name, first word, word count). Pure, so a test can tie it to the struct.
pub(crate) fn field_table(meta: &macroquad::miniquad::ShaderMeta) -> Vec<(String, u16, u16)> {
    let mut first = 0u16;
    meta.uniforms.uniforms.iter().map(|u| {
        let words = (u.uniform_type.size() / 4 * u.array_count) as u16;
        let row = (u.name.clone(), first, words);
        first += words;
        row
    }).collect()
}
/// `resource` gives a field's resource index in the program, in 4-byte units, or None if the
/// program has no such parameter: the rule of `glLinkProgram` in vitaGL.
pub(crate) fn layout(fields: &[(String, u16, u16)], size: u32, mut resource: impl FnMut(&str) -> Option<u32>) -> Layout {
    let mut slots = Vec::new();
    for (name, first, words) in fields {
        if let Some(index) = resource(name) { slots.push(FieldSlot { first: *first, words: *words, offset: u16::try_from(index * 4).unwrap_or(u16::MAX) }); }
    }
    Layout { slots, size }
}

/// What the direct path calls. The real one is `sceGxm` and the vitaGL symbols, a test's is a tape.
pub(crate) trait Backend {
    fn texture(&mut self, unit: u32, texture: usize);
    fn stream(&mut self, index: u32, data: usize);
    fn reserve_uniforms(&mut self, size: u32) -> *mut u8;
    fn vertex_uniforms(&mut self, buffer: *mut u8);
    fn fragment_uniforms(&mut self, buffer: *mut u8);
    fn draw(&mut self, primitive: u32, format: u32, index: usize, count: u32, wrap: u32);
}
/// A group as vitaGL drew it. `index` is the index pointer it passed, offset included. `texture` is
/// the diffuse unit's, the one a direct draw sets. `textures` is every unit, for the shadow check.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Record { pub texture: usize, pub stream: usize, pub index: usize, pub textures: [usize; MAX_TEXTURES] }
/// What the GXM context holds, as far as this pass knows. All of it is invalid after vitaGL draws,
/// except what that draw itself set, which `after_vitagl` records.
#[derive(Default)]
pub(crate) struct Direct { known: bool, unit: u32, texture: usize, stream: usize, uniforms: Option<(u32, bool)>, streams: u32, primitive: u32, format: u32 }
impl Direct {
    /// The last vitaGL draw told nothing usable. The next draw goes through vitaGL as well.
    pub(crate) fn forget(&mut self) { self.known = false; }
    pub(crate) fn known(&self) -> bool { self.known }
    pub(crate) fn after_vitagl(&mut self, capture: &Capture, program: &Program, class: u32, low: bool) {
        self.known = true;
        self.unit = program.diffuse_unit;
        self.texture = capture.textures[program.diffuse_unit as usize];
        self.stream = capture.streams[0];
        self.streams = capture.stream_mask.count_ones();
        self.primitive = capture.primitive;
        self.format = capture.format;
        self.uniforms = Some((class, low));
    }
    /// One draw. `class` names the uniform payload. Set only what differs from the context.
    #[inline] pub(crate) fn draw(&mut self, b: &mut impl Backend, record: &Record, count: u32, class: u32, low: bool, program: &Program, bits: &[u32; 143]) {
        if record.texture != self.texture { b.texture(self.unit, record.texture); self.texture = record.texture; }
        if record.stream != self.stream {
            for i in 0..self.streams { b.stream(i, record.stream); }
            self.stream = record.stream;
        }
        if self.uniforms != Some((class, low)) {
            let buffer = b.reserve_uniforms(program.vertex.size);
            unsafe { program.vertex.write(buffer, bits); }
            b.vertex_uniforms(buffer);
            let buffer = b.reserve_uniforms(program.fragment.size);
            unsafe { program.fragment.write(buffer, bits); }
            b.fragment_uniforms(buffer);
            self.uniforms = Some((class, low));
        }
        b.draw(self.primitive, self.format, record.index, count, count);
    }
}
/// What a shadow comparison found, for the one line after the window.
#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Shadow {
    pub draws: u64, pub learned: u64, pub mismatches: u64,
    pub texture: u64, pub stream: u64, pub index: u64, pub count: u64, pub draw_args: u64, pub vertex_bytes: u64, pub fragment_bytes: u64, pub program: u64,
    pub first: Option<String>,
}
impl Shadow {
    /// A vitaGL draw that nothing can be learned from. Only a shadow arm counts it as wrong.
    pub(crate) fn refused(&mut self, group: usize, capture: &Capture) {
        self.draws += 1;
        self.draw_args += 1;
        self.mismatches += 1;
        if self.first.is_none() { self.first = Some(format!("group={group} refused capture={capture:?}")); }
    }
    /// `known` holds what this group's first draw captured; `seen` is this draw's. Returns the
    /// record to keep.
    pub(crate) fn check(&mut self, group: usize, known: Option<Record>, seen: Record, capture: &Capture, count: u32, programs: Option<(usize, usize)>,
                        vertex: (&[u8], &[u8]), fragment: (&[u8], &[u8])) -> Record {
        self.draws += 1;
        let mut wrong = 0;
        if let Some(record) = known {
            if record.texture != seen.texture || record.textures != seen.textures { self.texture += 1; wrong += 1; }
            if record.stream != seen.stream { self.stream += 1; wrong += 1; }
            if record.index != seen.index { self.index += 1; wrong += 1; }
        } else { self.learned += 1; }
        if capture.count != count { self.count += 1; wrong += 1; }
        if capture.draws != 1 || capture.wrap != capture.count { self.draw_args += 1; wrong += 1; }
        // All set streams must point at the one buffer. A direct draw sets them as one.
        for i in 0..MAX_STREAMS {
            if capture.stream_mask & (1 << i) != 0 && capture.streams[i] != seen.stream { self.stream += 1; wrong += 1; break; }
        }
        if let Some((v, f)) = programs {
            if capture.vertex_program != v || capture.fragment_program != f { self.program += 1; wrong += 1; }
        }
        if vertex.0 != vertex.1 { self.vertex_bytes += 1; wrong += 1; }
        if fragment.0 != fragment.1 { self.fragment_bytes += 1; wrong += 1; }
        if wrong > 0 {
            self.mismatches += 1;
            if self.first.is_none() {
                let first_byte = |a: &[u8], b: &[u8]| a.iter().zip(b).position(|(x, y)| x != y);
                self.first = Some(format!("group={group} seen={seen:?} known={known:?} count={}/{count} draws={} wrap={} vertex_byte={:?} fragment_byte={:?}",
                    capture.count, capture.draws, capture.wrap, first_byte(vertex.0, vertex.1), first_byte(fragment.0, fragment.1)));
            }
        }
        known.unwrap_or(seen)
    }
    pub(crate) fn summary(&self, map: &str) -> String {
        format!("[lg/world-gxm] map={map} draws={} learned={} mismatches={} texture={} stream={} index={} count={} draw_args={} vertex_bytes={} fragment_bytes={} program={} first={}",
            self.draws, self.learned, self.mismatches, self.texture, self.stream, self.index, self.count, self.draw_args,
            self.vertex_bytes, self.fragment_bytes, self.program, self.first.as_deref().unwrap_or("none"))
    }
}

#[cfg(target_os = "vita")]
pub(crate) mod vita {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU32};
    extern "C" {
        static gxm_context: *mut c_void;
        static mut dirty_shader_vert_unifs: u8;
        static mut dirty_shader_frag_unifs: u8;
        static mut vgl_def_vert_buf: *mut c_void;
        static mut vgl_def_frag_buf: *mut c_void;
        fn vglReserveUniformCircularPoolBuffer(size: u32) -> *mut c_void;
        fn __real_sceGxmSetFragmentTexture(context: *mut c_void, unit: u32, texture: *const c_void) -> i32;
        fn __real_sceGxmSetVertexStream(context: *mut c_void, index: u32, data: *const c_void) -> i32;
        fn __real_sceGxmSetVertexProgram(context: *mut c_void, program: *const c_void);
        fn __real_sceGxmSetFragmentProgram(context: *mut c_void, program: *const c_void);
        fn __real_sceGxmSetVertexDefaultUniformBuffer(context: *mut c_void, buffer: *const c_void) -> i32;
        fn __real_sceGxmSetFragmentDefaultUniformBuffer(context: *mut c_void, buffer: *const c_void) -> i32;
        fn __real_sceGxmDrawInstanced(context: *mut c_void, primitive: u32, format: u32, index: *const c_void, count: u32, wrap: u32) -> i32;
        fn sceGxmVertexProgramGetProgram(program: *const c_void) -> *const c_void;
        fn sceGxmFragmentProgramGetProgram(program: *const c_void) -> *const c_void;
        fn sceGxmProgramFindParameterByName(program: *const c_void, name: *const std::ffi::c_char) -> *const c_void;
        fn sceGxmProgramParameterGetResourceIndex(parameter: *const c_void) -> u32;
        fn sceGxmProgramParameterGetType(parameter: *const c_void) -> u32;
        fn sceGxmProgramGetDefaultUniformBufferSize(program: *const c_void) -> u32;
        fn glFinish();
    }
    static ARMED: AtomicBool = AtomicBool::new(false);
    /// Non-zero returns on the direct path. vitaGL ignores them too, but this path has no other signal.
    static ERRORS: AtomicU32 = AtomicU32::new(0);
    pub(super) fn errors() -> u32 { ERRORS.load(Relaxed) }
    #[inline(always)] fn checked(rc: i32) { if rc != 0 { ERRORS.store(ERRORS.load(Relaxed).wrapping_add(1), Relaxed); } }
    /// Main thread only, and only between `arm` and `take`.
    static mut CAPTURED: Capture = Capture::EMPTY;
    macro_rules! captured { () => { &mut *std::ptr::addr_of_mut!(CAPTURED) }; }
    pub(crate) fn arm() { unsafe { *captured!() = Capture::EMPTY; } ARMED.store(true, Relaxed); }
    pub(crate) fn take() -> Capture { ARMED.store(false, Relaxed); unsafe { *captured!() } }

    // Each wrapper tests one atomic and tail-calls when disarmed, as in `gpu_timing`.
    #[no_mangle] pub unsafe extern "C" fn __wrap_sceGxmSetFragmentTexture(context: *mut c_void, unit: u32, texture: *const c_void) -> i32 {
        if ARMED.load(Relaxed) && (unit as usize) < MAX_TEXTURES { captured!().textures[unit as usize] = texture as usize; }
        __real_sceGxmSetFragmentTexture(context, unit, texture)
    }
    #[no_mangle] pub unsafe extern "C" fn __wrap_sceGxmSetVertexStream(context: *mut c_void, index: u32, data: *const c_void) -> i32 {
        if ARMED.load(Relaxed) && (index as usize) < MAX_STREAMS {
            captured!().streams[index as usize] = data as usize;
            captured!().stream_mask |= 1 << index;
        }
        __real_sceGxmSetVertexStream(context, index, data)
    }
    #[no_mangle] pub unsafe extern "C" fn __wrap_sceGxmSetVertexProgram(context: *mut c_void, program: *const c_void) {
        if ARMED.load(Relaxed) { captured!().vertex_program = program as usize; }
        __real_sceGxmSetVertexProgram(context, program)
    }
    #[no_mangle] pub unsafe extern "C" fn __wrap_sceGxmSetFragmentProgram(context: *mut c_void, program: *const c_void) {
        if ARMED.load(Relaxed) { captured!().fragment_program = program as usize; }
        __real_sceGxmSetFragmentProgram(context, program)
    }
    #[no_mangle] pub unsafe extern "C" fn __wrap_sceGxmSetVertexDefaultUniformBuffer(context: *mut c_void, buffer: *const c_void) -> i32 {
        if ARMED.load(Relaxed) { captured!().vertex_uniforms = buffer as usize; }
        __real_sceGxmSetVertexDefaultUniformBuffer(context, buffer)
    }
    #[no_mangle] pub unsafe extern "C" fn __wrap_sceGxmSetFragmentDefaultUniformBuffer(context: *mut c_void, buffer: *const c_void) -> i32 {
        if ARMED.load(Relaxed) { captured!().fragment_uniforms = buffer as usize; }
        __real_sceGxmSetFragmentDefaultUniformBuffer(context, buffer)
    }
    #[no_mangle] pub unsafe extern "C" fn __wrap_sceGxmDrawInstanced(context: *mut c_void, primitive: u32, format: u32, index: *const c_void, count: u32, wrap: u32) -> i32 {
        if ARMED.load(Relaxed) {
            let c = captured!();
            c.primitive = primitive; c.format = format; c.index = index as usize; c.count = count; c.wrap = wrap; c.draws += 1;
        }
        __real_sceGxmDrawInstanced(context, primitive, format, index, count, wrap)
    }

    /// The real thing. Reserves and pointers are what `vglReserve*UniformBuffer` does inline, so
    /// `vglRestore*UniformBuffer` after a clear still points at live memory.
    pub(crate) struct Real { pub context: *mut c_void, vertex_buffer: *mut c_void, fragment_buffer: *mut c_void }
    impl Real {
        pub(crate) fn new() -> Self { Self { context: unsafe { gxm_context }, vertex_buffer: std::ptr::null_mut(), fragment_buffer: std::ptr::null_mut() } }
    }
    impl Backend for Real {
        #[inline] fn texture(&mut self, unit: u32, texture: usize) { checked(unsafe { __real_sceGxmSetFragmentTexture(self.context, unit, texture as _) }); }
        #[inline] fn stream(&mut self, index: u32, data: usize) { checked(unsafe { __real_sceGxmSetVertexStream(self.context, index, data as _) }); }
        #[inline] fn reserve_uniforms(&mut self, size: u32) -> *mut u8 { unsafe { vglReserveUniformCircularPoolBuffer(size).cast() } }
        #[inline] fn vertex_uniforms(&mut self, buffer: *mut u8) { checked(unsafe { vgl_def_vert_buf = buffer.cast(); __real_sceGxmSetVertexDefaultUniformBuffer(self.context, buffer as _) }); }
        #[inline] fn fragment_uniforms(&mut self, buffer: *mut u8) { checked(unsafe { vgl_def_frag_buf = buffer.cast(); __real_sceGxmSetFragmentDefaultUniformBuffer(self.context, buffer as _) }); }
        #[inline] fn draw(&mut self, primitive: u32, format: u32, index: usize, count: u32, wrap: u32) {
            checked(unsafe { __real_sceGxmDrawInstanced(self.context, primitive, format, index as _, count, wrap) });
        }
    }
    /// vitaGL keeps uniform storage it believes the GPU has. A direct draw moved the default buffer
    /// pointers behind its back, so its next draw must reserve and copy whatever it believes.
    #[inline] pub(crate) fn uniforms_are_stale() { unsafe { dirty_shader_vert_unifs = 1; dirty_shader_frag_unifs = 1; } }
    /// Direct draws do not touch `last_frame`, so vitaGL could free a buffer or texture the GPU still reads.
    pub(crate) fn wait_for_gpu() { unsafe { glFinish(); } }
    /// What the direct path needs of the program pair a capture holds, or None if a field is not
    /// plain 32-bit floats, a slot falls outside its buffer, or the diffuse sampler is out of range.
    /// `layouts` stays empty then, and every draw of that program goes through vitaGL.
    pub(crate) fn program(capture: &Capture, fields: &[(String, u16, u16)]) -> Option<Program> {
        const F32: u32 = 0; // SCE_GXM_PARAMETER_TYPE_F32, the first value of SceGxmParameterType.
        let mut typed = true;
        let parameter = |program: *const c_void, name: &str| unsafe {
            let name = std::ffi::CString::new(name).ok()?;
            let parameter = sceGxmProgramFindParameterByName(program, name.as_ptr());
            (!parameter.is_null()).then_some(parameter)
        };
        let (vertex, fragment) = unsafe { (sceGxmVertexProgramGetProgram(capture.vertex_program as _), sceGxmFragmentProgramGetProgram(capture.fragment_program as _)) };
        if vertex.is_null() || fragment.is_null() { return None; }
        let mut build = |program: *const c_void| layout(fields, unsafe { sceGxmProgramGetDefaultUniformBufferSize(program) }, |name| {
            let found = parameter(program, name)?;
            typed &= unsafe { sceGxmProgramParameterGetType(found) } == F32;
            Some(unsafe { sceGxmProgramParameterGetResourceIndex(found) })
        });
        let (vertex_layout, fragment_layout) = (build(vertex), build(fragment));
        let diffuse = parameter(fragment, "Diffuse")?;
        let program = Program { vertex: vertex_layout, fragment: fragment_layout, diffuse_unit: unsafe { sceGxmProgramParameterGetResourceIndex(diffuse) } };
        (typed && program.valid()).then_some(program)
    }
    /// The bytes behind the default buffers vitaGL last set. It may have left one from an earlier
    /// draw if nothing changed, which is also what the GPU reads.
    pub(crate) fn uniform_bytes(vertex: u32, fragment: u32) -> (&'static [u8], &'static [u8]) {
        // Null until vitaGL first reserves. Empty slices then read as a mismatch.
        unsafe fn view(buffer: *mut c_void, size: u32) -> &'static [u8] {
            if buffer.is_null() { &[] } else { std::slice::from_raw_parts(buffer.cast(), size as usize) }
        }
        unsafe { (view(vgl_def_vert_buf, vertex), view(vgl_def_frag_buf, fragment)) }
    }
}

#[cfg(target_os = "vita")]
impl State {
    /// One direct draw, or false if this group has no record yet and vitaGL must draw it first.
    #[inline] pub(crate) fn direct_draw(&mut self, real: &mut vita::Real, group: usize, count: u32, class: u32, low: bool, bits: &[u32; 143], pass: Pass, drawn: u64) -> bool {
        if !self.direct.known() { return false; }
        let Some(record) = self.records[group] else { return false; };
        if self.layouts[usize::from(low)].is_none() { return false; }
        self.direct_draws += 1;
        if pass == Pass::ProofSecond { self.second_trial_direct += 1; }
        if self.sabotage_due(pass, drawn) { return true; }
        let program = self.layouts[usize::from(low)].as_ref().unwrap();
        self.direct.draw(real, &record, count, class, low, program, bits);
        true
    }
    /// After vitaGL drew under capture: learn, and in shadow mode compare.
    pub(crate) fn after_vitagl(&mut self, capture: &Capture, group: usize, animated: bool, pipeline_class: u8, low: bool, count: u32, class: u32, bits: &[u32; 143], way: Way) {
        self.vitagl_draws += 1;
        if !capture.learnable() {
            self.refused_captures += 1;
            if way == Way::Shadow { self.shadow.refused(group, capture); }
            self.direct.forget();
            return;
        }
        let which = usize::from(low);
        if self.layouts[which].is_none() && !self.layout_refused[which] {
            self.layouts[which] = vita::program(capture, &self.fields);
            self.layout_refused[which] = self.layouts[which].is_none();
        }
        if self.layouts[which].is_none() {
            if way == Way::Shadow { self.shadow.refused(group, capture); }
            self.direct.forget();
            return;
        }
        let slot = usize::from(pipeline_class) * 2 + usize::from(low);
        let programs = self.programs.get(slot).copied().flatten();
        if let Some(entry) = self.programs.get_mut(slot) { *entry = Some((capture.vertex_program, capture.fragment_program)); }
        let program = self.layouts[usize::from(low)].as_ref().unwrap();
        let seen = Record { texture: capture.textures[program.diffuse_unit as usize], stream: capture.streams[0], index: capture.index, textures: capture.textures };
        // An animated group's texture changes by design. It always draws through vitaGL.
        let known = if animated { None } else { self.records[group] };
        if way == Way::Shadow {
            let (seen_v, seen_f) = vita::uniform_bytes(program.vertex.size, program.fragment.size);
            let (want_v, want_f) = (program.vertex.image(bits), program.fragment.image(bits));
            let record = self.shadow.check(group, known, seen, capture, count, programs, (seen_v, &want_v), (seen_f, &want_f));
            if !animated { self.records[group] = Some(record); }
        } else if !animated && known.is_none() {
            self.records[group] = Some(seen);
        }
        self.direct.after_vitagl(capture, program, class, low);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Tape { calls: Vec<String>, memory: Vec<Box<[u8]>> }
    impl Backend for Tape {
        fn texture(&mut self, unit: u32, texture: usize) { self.calls.push(format!("texture {unit} {texture}")); }
        fn stream(&mut self, index: u32, data: usize) { self.calls.push(format!("stream {index} {data}")); }
        fn reserve_uniforms(&mut self, size: u32) -> *mut u8 {
            self.calls.push(format!("reserve {size}"));
            self.memory.push(vec![0xee; size as usize].into_boxed_slice());
            self.memory.last_mut().unwrap().as_mut_ptr()
        }
        fn vertex_uniforms(&mut self, buffer: *mut u8) { assert_eq!(buffer, self.memory.last_mut().unwrap().as_mut_ptr()); self.calls.push(format!("vertex {:?}", self.memory.last().unwrap())); }
        fn fragment_uniforms(&mut self, buffer: *mut u8) { assert_eq!(buffer, self.memory.last_mut().unwrap().as_mut_ptr()); self.calls.push(format!("fragment {:?}", self.memory.last().unwrap())); }
        fn draw(&mut self, primitive: u32, format: u32, index: usize, count: u32, wrap: u32) { self.calls.push(format!("draw {primitive} {format} {index} {count} {wrap}")); }
    }
    fn program() -> Program {
        // One word of padding inside, as a resource index allows, and a field past the middle.
        Program { vertex: Layout { slots: vec![FieldSlot { first: 0, words: 2, offset: 0 }, FieldSlot { first: 142, words: 1, offset: 12 }], size: 16 },
                  fragment: Layout { slots: vec![FieldSlot { first: 2, words: 1, offset: 4 }], size: 8 }, diffuse_unit: 1 }
    }
    fn record(texture: usize, stream: usize, index: usize) -> Record { Record { texture, stream, index, textures: [99, texture, 0, 0] } }
    fn bits(a: u32) -> [u32; 143] { let mut b = [0; 143]; b[0] = a; b[1] = a + 1; b[2] = a + 2; b[142] = a + 3; b }
    fn capture(texture: usize, stream: usize, index: usize, count: u32) -> Capture {
        Capture { vertex_program: 7, fragment_program: 8, textures: [99, texture, 0, 0], stream_mask: 0b11111, streams: [stream; MAX_STREAMS],
            vertex_uniforms: 1, fragment_uniforms: 2, primitive: 3, format: 4, index, count, wrap: count, draws: 1 }
    }
    #[test]
    fn only_a_whole_draw_on_one_buffer_is_learnable() {
        assert!(capture(10, 500, 600, 12).learnable());
        assert!(!Capture::EMPTY.learnable(), "a masked or skipped draw");
        let cases: [fn(&mut Capture); 9] = [
            |c| c.draws = 0, |c| c.draws = 2, |c| c.index = 0, |c| c.count = 0, |c| c.wrap = 1, |c| c.vertex_program = 0, |c| c.fragment_program = 0,
            |c| c.stream_mask = 0, |c| c.streams[3] = 501,
        ];
        for (i, change) in cases.iter().enumerate() {
            let mut c = capture(10, 500, 600, 12);
            change(&mut c);
            assert!(!c.learnable(), "case {i}");
        }
        // A stream nobody set does not count against it.
        let mut c = capture(10, 500, 600, 12);
        c.stream_mask = 0b00011; c.streams[3] = 999;
        assert!(c.learnable());
        c.streams[0] = 0; c.streams[1] = 0;
        assert!(!c.learnable(), "a null buffer");
    }
    #[test]
    fn a_layout_must_fit_and_the_diffuse_unit_must_exist() {
        let mut p = program();
        assert!(p.valid());
        p.vertex.slots.push(FieldSlot { first: 0, words: 1, offset: 12 });
        assert!(p.valid(), "ends exactly at the end of the buffer");
        p.vertex.slots.push(FieldSlot { first: 0, words: 1, offset: 13 });
        assert!(!p.valid(), "one byte past it");
        let mut p = program();
        p.fragment.slots.push(FieldSlot { first: 0, words: 3, offset: 0 });
        assert!(!p.valid());
        let mut p = program();
        p.diffuse_unit = MAX_TEXTURES as u32;
        assert!(!p.valid());
        // An index too large for a slot's offset field cannot wrap into range.
        let fields = vec![("A".to_string(), 0, 1)];
        assert!(!layout(&fields, 64, |_| Some(0x8000)).fits());
    }
    #[test]
    fn after_a_refused_capture_nothing_goes_direct_until_vitagl_tells_again() {
        let p = program();
        let mut direct = Direct::default();
        assert!(!direct.known(), "nothing is known at the start");
        direct.after_vitagl(&capture(10, 500, 600, 12), &p, 1, true);
        assert!(direct.known());
        direct.forget();
        assert!(!direct.known());
        direct.after_vitagl(&capture(10, 500, 600, 12), &p, 1, true);
        assert!(direct.known());
    }
    #[test]
    fn a_refused_capture_is_a_mismatch_in_the_shadow_line() {
        let mut s = Shadow::default();
        s.refused(4, &Capture::EMPTY);
        assert_eq!((s.draws, s.mismatches, s.draw_args), (1, 1, 1));
        assert!(s.first.as_deref().unwrap().starts_with("group=4 refused"));
    }
    #[test]
    fn sabotage_takes_the_first_direct_draw_at_or_after_its_position_once_per_proof() {
        let meta = super::super::shader::meta();
        let mut state = State::new(Mode::ProofSabotage, 4, &meta);
        assert!(!state.sabotage_due(Pass::Frame, 9), "a frame's own pass is left alone");
        assert!(!state.sabotage_due(Pass::ProofVitagl, 9));
        assert!(!state.sabotage_due(Pass::ProofSecond, SABOTAGE_DRAW - 1));
        assert!(state.sabotage_due(Pass::ProofSecond, SABOTAGE_DRAW + 3), "position 5 may have been a pipeline change");
        assert!(!state.sabotage_due(Pass::ProofSecond, SABOTAGE_DRAW + 4), "once");
        state.proof_result(&[1], &[1]);
        assert!(state.sabotage_due(Pass::ProofSecond, SABOTAGE_DRAW));
        assert_eq!(state.sabotage_fired, 2);
        let mut honest = State::new(Mode::Proof, 4, &meta);
        assert!(!honest.sabotage_due(Pass::ProofSecond, 9));
    }
    #[test]
    fn layout_writes_the_words_its_slots_name_and_zero_elsewhere() {
        let Program { vertex: v, fragment: f, .. } = program();
        let b = bits(0x0101);
        let mut expected = vec![0u8; 16];
        expected[0..4].copy_from_slice(&0x0101u32.to_le_bytes());
        expected[4..8].copy_from_slice(&0x0102u32.to_le_bytes());
        expected[12..16].copy_from_slice(&0x0104u32.to_le_bytes());
        assert_eq!(v.image(&b), expected);
        let mut expected = vec![0u8; 8];
        expected[4..8].copy_from_slice(&0x0103u32.to_le_bytes());
        assert_eq!(f.image(&b), expected);
    }
    #[test]
    fn layout_follows_the_field_table_and_skips_parameters_a_program_lacks() {
        let fields = vec![("A".to_string(), 0, 16), ("B".to_string(), 16, 4), ("C".to_string(), 20, 3)];
        let l = layout(&fields, 64, |name| match name { "A" => Some(4), "C" => Some(0), _ => None });
        assert_eq!(l.slots, vec![FieldSlot { first: 0, words: 16, offset: 16 }, FieldSlot { first: 20, words: 3, offset: 0 }]);
        assert_eq!(l.size, 64);
    }
    #[test]
    fn field_table_covers_every_word_of_the_world_uniforms_in_order() {
        let table = field_table(&super::super::shader::meta());
        let mut next = 0;
        for (_, first, words) in &table { assert_eq!(*first, next); next += *words; }
        assert_eq!(next, 143);
        assert_eq!(table[0], ("Mvp".to_string(), 0, 16));
    }
    #[test]
    fn direct_draw_sets_only_what_differs_from_the_context() {
        let p = program();
        let mut direct = Direct::default();
        direct.after_vitagl(&capture(10, 500, 600, 12), &p, 1, true);
        let mut tape = Tape::default();
        // Same texture, stream and class as the vitaGL draw: the draw call alone.
        direct.draw(&mut tape, &record(10, 500, 600), 12, 1, true, &p, &bits(1));
        assert_eq!(tape.calls, ["draw 3 4 600 12 12"]);
        tape.calls.clear();
        // The diffuse unit, whichever the program gave it. Never the other unit.
        direct.draw(&mut tape, &record(11, 500, 700), 6, 1, true, &p, &bits(1));
        assert_eq!(tape.calls, ["texture 1 11", "draw 3 4 700 6 6"]);
        tape.calls.clear();
        // A new buffer sets every stream the program uses, no more.
        direct.draw(&mut tape, &record(11, 501, 800), 6, 1, true, &p, &bits(1));
        assert_eq!(tape.calls, ["stream 0 501", "stream 1 501", "stream 2 501", "stream 3 501", "stream 4 501", "draw 3 4 800 6 6"]);
        tape.calls.clear();
        // New payload: both buffers, each with its own bytes, before the draw.
        direct.draw(&mut tape, &record(11, 501, 900), 6, 2, true, &p, &bits(5));
        assert_eq!(tape.calls, ["reserve 16", "vertex [5, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 8, 0, 0, 0]", "reserve 8", "fragment [0, 0, 0, 0, 7, 0, 0, 0]", "draw 3 4 900 6 6"]);
        tape.calls.clear();
        direct.draw(&mut tape, &record(11, 501, 900), 6, 2, true, &p, &bits(5));
        assert_eq!(tape.calls, ["draw 3 4 900 6 6"]);
        tape.calls.clear();
        // Equal class on the other program is a different payload image.
        direct.draw(&mut tape, &record(11, 501, 900), 6, 2, false, &p, &bits(5));
        assert_eq!(tape.calls[0], "reserve 16");
    }
    #[test]
    fn a_vitagl_draw_resets_what_the_direct_path_assumes() {
        let p = program();
        let mut direct = Direct::default();
        direct.after_vitagl(&capture(10, 500, 600, 12), &p, 1, true);
        let mut tape = Tape::default();
        direct.draw(&mut tape, &record(20, 501, 1), 3, 2, true, &p, &bits(1));
        tape.calls.clear();
        // vitaGL drew again and left texture 10, stream 500, its own class.
        direct.after_vitagl(&capture(10, 500, 600, 12), &p, 7, true);
        direct.draw(&mut tape, &record(20, 501, 1), 3, 2, true, &p, &bits(1));
        assert_eq!(tape.calls[0], "texture 1 20");
        assert_eq!(tape.calls[1], "stream 0 501");
        assert!(tape.calls.iter().any(|c| c == "reserve 16"));
    }
    #[test]
    fn shadow_learns_once_then_names_each_kind_of_difference() {
        let v = [1u8, 2, 3];
        let f = [4u8, 5];
        let good = capture(10, 500, 600, 12);
        let known = record(10, 500, 600);
        let seen = |c: &Capture| Record { texture: c.textures[1], stream: c.streams[0], index: c.index, textures: c.textures };
        let mut s = Shadow::default();
        assert_eq!(s.check(9, None, seen(&good), &good, 12, None, (&v, &v), (&f, &f)), known);
        assert_eq!((s.draws, s.learned, s.mismatches), (1, 1, 0));
        s.check(9, Some(known), seen(&good), &good, 12, Some((7, 8)), (&v, &v), (&f, &f));
        assert_eq!((s.draws, s.learned, s.mismatches), (2, 1, 0));
        let mut other_unit = good; other_unit.textures[0] = 98;
        let cases: [(&str, Capture, u32, Option<(usize, usize)>, &[u8], &[u8]); 9] = [
            ("texture", capture(11, 500, 600, 12), 12, None, &v, &f),
            ("other unit", other_unit, 12, None, &v, &f),
            ("stream", capture(10, 501, 600, 12), 12, None, &v, &f),
            ("index", capture(10, 500, 601, 12), 12, None, &v, &f),
            ("count", good, 13, None, &v, &f),
            ("program", good, 12, Some((7, 9)), &v, &f),
            ("vertex", good, 12, None, &[1, 2, 9], &f),
            ("fragment", good, 12, None, &v, &[4, 9]),
            ("args", Capture { draws: 2, ..good }, 12, None, &v, &f),
        ];
        for (name, c, count, programs, vb, fb) in cases {
            let mut s = Shadow::default();
            s.check(9, Some(known), seen(&c), &c, count, programs, (vb, &v), (fb, &f));
            assert_eq!(s.mismatches, 1, "{name}");
            let by_kind = [s.texture, s.stream, s.index, s.count, s.program, s.vertex_bytes, s.fragment_bytes, s.draw_args];
            assert_eq!(by_kind.iter().sum::<u64>(), 1, "{name}");
            assert!(s.first.as_ref().unwrap().contains("group=9"), "{name}");
        }
        // Streams set by the draw must all be the same buffer: one that is not is a mismatch.
        let mut split = good; split.streams[3] = 777;
        let mut s = Shadow::default();
        s.check(9, Some(known), seen(&split), &split, 12, None, (&v, &v), (&f, &f));
        assert_eq!((s.mismatches, s.stream), (1, 1));
        // A stream the program does not use does not count.
        let mut unused = good; unused.streams[6] = 777;
        let mut s = Shadow::default();
        s.check(9, Some(known), seen(&unused), &unused, 12, None, (&v, &v), (&f, &f));
        assert_eq!(s.mismatches, 0);
        assert!(s.summary("skool2").contains("map=skool2 draws=1"));
    }
    #[test]
    fn each_mode_runs_each_pass_the_way_its_name_says() {
        let meta = super::super::shader::meta();
        let way = |mode, pass| State::new(mode, 3, &meta).way(pass);
        assert_eq!(way(Mode::Off, Pass::Frame), Way::Vitagl);
        assert_eq!(way(Mode::Shadow, Pass::Frame), Way::Shadow);
        assert_eq!(way(Mode::Live, Pass::Frame), Way::Direct);
        assert_eq!(way(Mode::Proof, Pass::Frame), Way::Direct);
        assert_eq!(way(Mode::Proof, Pass::ProofVitagl), Way::Shadow);
        assert_eq!(way(Mode::Proof, Pass::ProofSecond), Way::Direct);
        assert_eq!(way(Mode::ProofControl, Pass::ProofSecond), Way::Shadow);
        assert_eq!(way(Mode::ProofSabotage, Pass::ProofSecond), Way::Direct);
        assert!(State::new(Mode::Off, 3, &meta).records.is_empty());
        assert!(!State::new(Mode::Off, 3, &meta).direct_used() && !State::new(Mode::Shadow, 3, &meta).direct_used() && State::new(Mode::Live, 3, &meta).direct_used());
    }
    #[test]
    fn proof_frames_come_after_the_warmup_at_their_period_up_to_the_limit() {
        let meta = super::super::shader::meta();
        let mut off = State::new(Mode::Live, 1, &meta);
        assert!(!(0..1000).any(|_| off.proof_due()), "only proof modes");
        let mut s = State::new(Mode::Proof, 1, &meta);
        let mut due = Vec::new();
        for frame in 1..=1000u64 {
            if s.proof_due() { due.push(frame); s.proof_result(&[1], &[1]); }
        }
        assert_eq!(due, (0..PROOF_FRAMES).map(|i| PROOF_WARMUP + i * PROOF_EVERY).collect::<Vec<_>>());
        assert_eq!((s.proofs.equal, s.proofs.different), (PROOF_FRAMES, 0));
    }
    #[test]
    fn proofs_count_equal_frames_and_name_a_difference() {
        let mut p = Proofs::default();
        p.observe(&[1, 2, 3], &[1, 2, 3]);
        assert_eq!((p.frames, p.equal, p.different), (1, 1, 0));
        p.observe(&[1, 2, 3], &[1, 9, 9]);
        assert_eq!((p.frames, p.equal, p.different, p.first_different_byte, p.different_bytes), (2, 1, 1, Some(1), 2));
        p.observe(&[1, 2, 3], &[9, 2, 3]);
        assert_eq!((p.different, p.first_different_byte, p.different_bytes), (2, Some(1), 2), "the first difference and the largest count stay");
        // An empty read is not a proof.
        p.observe(&[], &[]);
        assert_eq!(p.different, 3);
    }
    #[test]
    fn modes_parse_and_only_the_named_modes_exist() {
        assert_eq!(Mode::parse("off"), Some(Mode::Off));
        assert_eq!(Mode::parse("shadow"), Some(Mode::Shadow));
        assert_eq!(Mode::parse("live"), Some(Mode::Live));
        assert_eq!(Mode::parse("proof"), Some(Mode::Proof));
        assert_eq!(Mode::parse("proof_control"), Some(Mode::ProofControl));
        assert_eq!(Mode::parse("proof_sabotage"), Some(Mode::ProofSabotage));
        assert_eq!(Mode::parse("on"), None);
        assert_eq!(Mode::parse(""), None);
    }
}
