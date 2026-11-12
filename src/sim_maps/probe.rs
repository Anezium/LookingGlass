//! Explicit host diagnostics. Run from a private working/save directory.
use std::{cell::{Cell,RefCell}, collections::BTreeMap, sync::OnceLock, time::Instant};
use sha2::{Digest, Sha256};
thread_local! {
    static FRAME_NO:Cell<u64>=const {Cell::new(0)};
    static PACKET_NO:Cell<u64>=const {Cell::new(0)};
    static ALLOC_TRACKED:Cell<bool>=const {Cell::new(false)};
    static PREPARE_ALLOC:Cell<[u64;2]>=const {Cell::new([0;2])};
    static SKINS: RefCell<Vec<serde_json::Value>> = RefCell::default();
    static TEXTURES: RefCell<BTreeMap<String,String>> = RefCell::default();
    static PACKETS: RefCell<Vec<serde_json::Value>> = RefCell::default();
    static JOINT_ORDER: RefCell<Vec<(u8,usize,u32)>> = RefCell::default();
    static WORLD_ORDER: RefCell<Vec<(usize,bool)>> = RefCell::default();
    static COUNTS: RefCell<BTreeMap<&'static str, u64>> = RefCell::default();
    static SPANS: RefCell<BTreeMap<&'static str, f64>> = RefCell::default();
    static START: RefCell<Option<Instant>> = const { RefCell::new(None) };
    static PROJECTION: Cell<Option<[u32; 16]>> = const { Cell::new(None) };
    static MOVER_REPLAY: RefCell<Option<serde_json::Value>> = const { RefCell::new(None) };
    static NPC_ACTOR: RefCell<String> = RefCell::default();
    static NPC_DECISIONS: RefCell<BTreeMap<String, (Sha256, u64)>> = RefCell::default();
    /// GL viewport sampled by the host at the scaled 3D stage, once per frame.
    static WORLD_VIEWPORT: Cell<Option<[i32; 4]>> = const { Cell::new(None) };
}
fn decisions_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    enabled() && *ON.get_or_init(|| std::env::var_os("LOOKING_GLASS_NPC_DECISIONS").is_some())
}
pub fn npc_actor(index: usize, name: &str, model: &str) {
    if decisions_enabled() {
        NPC_ACTOR.with(|a| *a.borrow_mut() = format!("{index}:{name}:{model}"));
    }
}
/// Per-actor ordered substep state, emitted feedback and sight decisions. This
/// deliberately expensive host oracle is excluded from native builds/timings.
pub fn npc_decision(label: &str, value: &impl serde::Serialize) {
    if !decisions_enabled() { return; }
    let bytes = serde_json::to_vec(&(label, value)).expect("finite NPC decision");
    NPC_ACTOR.with(|a| NPC_DECISIONS.with(|d| {
        let mut d = d.borrow_mut();
        let (digest, count) = d.entry(a.borrow().clone()).or_default();
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
        *count += 1;
    }));
}
fn npc_decisions() -> serde_json::Value {
    NPC_DECISIONS.with(|d| serde_json::Value::Array(d.borrow().iter().map(|(actor, (hash, count))|
        serde_json::json!({"actor":actor,"observations":count,"sha256":format!("{:x}",hash.clone().finalize())})
    ).collect()))
}
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LOOKING_GLASS_SIM_MAPS_PROBE").is_some())
}
pub fn elapsed(actual: f32) -> f32 {
    if !enabled() { return actual; }
    static DT: OnceLock<f32> = OnceLock::new();
    *DT.get_or_init(|| std::env::var("LOOKING_GLASS_SIM_MAPS_DT").unwrap_or_else(|_| "0.05".into()).parse::<f32>().ok().filter(|t| t.is_finite() && *t > 0. && *t < 1.).expect("finite probe timestep"))
}
pub fn count(name: &'static str, n: u64) {
    static COUNTS_ON: OnceLock<bool> = OnceLock::new();
    if enabled() && *COUNTS_ON.get_or_init(|| std::env::var("LOOKING_GLASS_SIM_MAPS_COUNTERS").as_deref() != Ok("0")) {
        COUNTS.with(|c| *c.borrow_mut().entry(name).or_default() += n);
    }
}
/// The counters so far, for host fixtures that do not use the frame probe.
pub fn counters() -> BTreeMap<&'static str, u64> { COUNTS.with(|c| c.borrow().clone()) }
pub fn uncount(name: &'static str, n: u64) {
    static COUNTS_ON: OnceLock<bool> = OnceLock::new();
    if enabled() && *COUNTS_ON.get_or_init(|| std::env::var("LOOKING_GLASS_SIM_MAPS_COUNTERS").as_deref() != Ok("0")) {
        COUNTS.with(|c| {
            if let Some(slot) = c.borrow_mut().get_mut(name) { *slot = slot.saturating_sub(n); }
        });
    }
}
pub fn set_frame(frame: u64) {
    // Harvest the previous frame before the next one arms: the HUD and upscale
    // batches of frame N are flushed by the framework between the two samples.
    draw_query::set_frame(frame);
    FRAME_NO.with(|c| c.set(frame)); PACKET_NO.with(|c| c.set(0));
}
/// Frame number of the sample currently being collected.
pub fn frame() -> u64 { FRAME_NO.with(Cell::get) }
/// Record the scaled 3D viewport at the specified per-frame stage. This is a GL query of
/// the current viewport at prepare-3d-returned, never the HUD/default framebuffer stage.
pub fn sample_world_viewport() {
    if !enabled() { return; }
    use macroquad::miniquad::gl::glGetIntegerv;
    let mut viewport = [0i32; 4];
    unsafe { glGetIntegerv(0x0BA2, viewport.as_mut_ptr()); }
    WORLD_VIEWPORT.with(|v| v.set(Some(viewport)));
}
pub fn begin() {
    crate::collision::sah_probe::begin();
    if enabled() {
        SKINS.with(|s|s.borrow_mut().clear());
        PACKETS.with(|s|s.borrow_mut().clear());
        JOINT_ORDER.with(|s|s.borrow_mut().clear());
        WORLD_ORDER.with(|s|s.borrow_mut().clear());
        COUNTS.with(|c| c.borrow_mut().clear());
        SPANS.with(|c| c.borrow_mut().clear());
        NPC_DECISIONS.with(|c| c.borrow_mut().clear());
        PROJECTION.with(|p| p.set(None));
        WORLD_VIEWPORT.with(|v| v.set(None));
        crate::light_profile::render_host_reset();
        START.with(|s| *s.borrow_mut() = Some(Instant::now()));
    }
}
pub fn end(frame: u64, map: &str, active: bool, state: &impl serde::Serialize) {
    if !enabled() { return; }
    crate::collision::sah_probe::end(frame,map);
    // Includes snapshot Value construction; excludes byte encoding, hashing and output.
    // Workload counters can be disabled independently for host ranking.
    let cpu_ms = START.with(|s| s.borrow().unwrap().elapsed().as_secs_f64() * 1000.);
    let bytes = serde_json::to_vec(state).expect("serializable simulation state");
    let hash = format!("{:x}", Sha256::digest(&bytes));
    COUNTS.with(|c| println!("SIM_MAPS_FRAME {}", serde_json::json!({
        "actor_skin":SKINS.with(|s|s.borrow().clone()),
        "submitted_packets":PACKETS.with(|s|s.borrow().clone()),
        "frame": frame, "map": map, "active": active,
        "joint_alpha_order": JOINT_ORDER.with(|s|s.borrow().clone()),
        "blend_world_order": WORLD_ORDER.with(|s|s.borrow().clone()),
        "selected_lights_bits": crate::lighting::selected_bits(),
        "state_sha256": hash, "cpu_ms": cpu_ms, "counters": *c.borrow(),
        "world_viewport": WORLD_VIEWPORT.with(Cell::get),
        "npc_decisions": npc_decisions(),
        "spans_ms": SPANS.with(|s| s.borrow().clone()),
        "render": crate::light_profile::render_host_report(),
        "renderer_drops": renderer_drops(),
        "mover_replay": MOVER_REPLAY.with(|m| m.borrow_mut().take()),
    })));
    if std::env::var_os("LOOKING_GLASS_SIM_MAPS_STATE").is_some() {
        std::fs::write(format!("state-{frame:05}.json"), bytes).unwrap();
    }
    #[cfg(feature = "vita_renderer")]
    if frame == 179 { crate::render_fx::actor_publication::emit_eligibility(map); }
}

/// This frame's refused groups and capped layers, independent of the counter switch.
fn renderer_drops() -> serde_json::Value {
    #[cfg(all(feature = "vita_renderer", not(target_os = "vita")))]
    {
        // A counter that was never charged has no entry yet; its value is zero.
        let report = crate::render_fx::retained::host_report();
        let value = |key: &str| report.get(key).and_then(serde_json::Value::as_u64).unwrap_or(0);
        serde_json::json!({"groups": value("vita_fx_dropped_groups"), "layers": value("vita_fx_dropped_layers")})
    }
    #[cfg(not(all(feature = "vita_renderer", not(target_os = "vita"))))]
    { serde_json::Value::Null }
}

pub fn span(label: &'static str, ms: f64) {
    if enabled() { SPANS.with(|s| *s.borrow_mut().entry(label).or_default() += ms); }
}

pub fn world_order(id: usize, alpha: bool) {
    if enabled() {WORLD_ORDER.with(|s|s.borrow_mut().push((id,alpha)));}
}

pub fn joint_order(kind: u8, id: usize, depth: f32) {
    if enabled() {JOINT_ORDER.with(|s|s.borrow_mut().push((kind,id,depth.to_bits())));}
}

pub fn force_uploads() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LOOKING_GLASS_SIM_MAPS_FORCE_UPLOAD").is_some())
}
/// Host oracle sabotage: makes a lever wrong on purpose, to prove the comparison sees it.
pub fn negative_control() -> Option<&'static str> {
    static ON: OnceLock<Option<String>> = OnceLock::new();
    ON.get_or_init(|| std::env::var("LOOKING_GLASS_GATHER_NEGATIVE").ok()).as_deref()
}
/// `uv`: one live UV edit nobody announces, in both arms. The lever-on arm must go stale.
pub fn negative_uv_edit(mesh: &mut macroquad::prelude::Mesh) {
    if negative_control() == Some("uv") && frame() >= 90 {
        if let Some(v) = mesh.vertices.first_mut() { v.uv.x = frame() as f32 * 0.125; }
    }
}
/// Mover replay proof, LOOKING_GLASS_MOVER_REPLAY: transforms, collider bits and collision answers
/// after the movers advanced. LOOKING_GLASS_MOVER_RIDE also stands the player on one mover every
/// 24 frames, so the rider branch runs. Both arms get the same teleports.
pub fn mover_replay_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    enabled() && *ON.get_or_init(|| std::env::var_os("LOOKING_GLASS_MOVER_REPLAY").is_some())
}
pub fn mover_ride(tops: Vec<Vec3Probe>, player: &mut crate::movement::Player) {
    static ON: OnceLock<bool> = OnceLock::new();
    if !*ON.get_or_init(|| std::env::var_os("LOOKING_GLASS_MOVER_RIDE").is_some()) || tops.is_empty() || frame() % 24 != 0 { return; }
    let top = tops[(frame() / 24) as usize % tops.len()];
    player.feet = top + macroquad::prelude::Vec3::Z * 0.5;
    player.velocity = macroquad::prelude::Vec3::ZERO;
    count("mover_ride_teleports", 1);
}
type Vec3Probe = macroquad::prelude::Vec3;
pub fn record_movers(transforms: &[(usize, Vec3Probe, macroquad::prelude::Quat)], world: &crate::collision::World, player: &crate::movement::Player) {
    let mut t = Sha256::new();
    for (id, origin, rotation) in transforms {
        t.update((*id as u64).to_le_bytes());
        for f in origin.to_array().into_iter().chain(rotation.to_array()) { t.update(f.to_bits().to_le_bytes()); }
    }
    let (geometry, sweeps) = world.probe_dynamic_bits(player.feet);
    let hash = |bits: &[u32]| { let mut h = Sha256::new(); for b in bits { h.update(b.to_le_bytes()); } format!("{:x}", h.finalize()) };
    let feet = player.feet.to_array().map(f32::to_bits);
    MOVER_REPLAY.with(|m| *m.borrow_mut() = Some(serde_json::json!({
        "transforms": transforms.len(), "transforms_sha256": format!("{:x}", t.finalize()),
        "collider_bits": geometry.len(), "colliders_sha256": hash(&geometry),
        "sweeps": sweeps.len(), "sweeps_sha256": hash(&sweeps), "player_feet_bits": feet, "grounded": player.grounded,
    })));
}
pub fn skin_proof() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    enabled() && *ON.get_or_init(||std::env::var_os("LOOKING_GLASS_SKIN_PROOF").is_some())
}
pub fn reserve_skin(actor:Option<crate::character::skinning::identity::ActorId>,surface:usize)->usize {
    if !skin_proof() {return usize::MAX;}
    SKINS.with(|s| {let mut s=s.borrow_mut();let id=s.len();
        s.push(serde_json::json!({"actor":actor.map(|a|a.census_id()),"surface":surface}));id})
}
fn mesh_hash(vertices:&[macroquad::prelude::Vertex],indices:&[u16])->Sha256 {
    let mut h=Sha256::new();h.update((vertices.len() as u64).to_le_bytes());
    for v in vertices {
        for f in v.position.to_array().into_iter().chain(v.uv.to_array()).chain(v.normal.to_array()) {h.update(f.to_bits().to_le_bytes());}
        h.update(v.color);
    }
    h.update((indices.len() as u64).to_le_bytes());for i in indices {h.update(i.to_le_bytes());}h
}
pub fn record_skin(slot:usize,vertices:&[macroquad::prelude::Vertex],indices:&[u16]) {
    if slot==usize::MAX {return;}
    let hash=format!("{:x}",mesh_hash(vertices,indices).finalize());
    SKINS.with(|s|s.borrow_mut()[slot]["sha256"]=hash.into());
}
/// The serial kernel leaves raw colour alpha in the live mesh, while a worker
/// publication stores it normalized. A serial entry therefore also records the
/// normalized hashes of both alpha variants, so a surface that moves to a worker
/// publication compares with the serial output of the same frame.
pub fn record_serial_publication(slot:usize,vertices:&[macroquad::prelude::Vertex],indices:&[u16],
    normalize:fn(macroquad::prelude::Vertex,bool)->macroquad::prelude::Vertex) {
    if slot==usize::MAX {return;}
    let [opaque,alpha]=[false,true].map(|variant| {
        let published:Vec<_>=vertices.iter().map(|v|normalize(*v,variant)).collect();
        format!("{:x}",mesh_hash(&published,indices).finalize())
    });
    SKINS.with(|s| {let mut s=s.borrow_mut();let entry=&mut s[slot];
        entry["path"]="serial".into();entry["published_opaque_sha256"]=opaque.into();entry["published_alpha_sha256"]=alpha.into();});
}
pub fn record_direct_publication(slot:usize,alpha_variant:bool) {
    if slot==usize::MAX {return;}
    SKINS.with(|s| {let mut s=s.borrow_mut();let entry=&mut s[slot];
        entry["path"]="direct".into();entry["alpha_variant"]=alpha_variant.into();});
}
pub fn packet(gpu_slot:usize,gpu_vertices:&[macroquad::prelude::Vertex],gpu_indices:&[u16],vertices:&[macroquad::prelude::Vertex],indices:&[u16],uniforms:&[f32],policy:&[u32],texture:Option<&macroquad::prelude::Texture2D>) {
    if !skin_proof() {return;}
    let geometry=format!("{:x}",mesh_hash(vertices,indices).finalize());
    let gpu_geometry=format!("{:x}",mesh_hash(gpu_vertices,gpu_indices).finalize());
    assert_eq!(geometry,gpu_geometry,"stale GPU geometry at retained slot {gpu_slot}");
    let mut h=Sha256::new();for f in uniforms {h.update(f.to_bits().to_le_bytes());}
    let uniform_hash=format!("{:x}",h.finalize());
    let texture=texture.map(|t|TEXTURES.with(|cache| {let mut cache=cache.borrow_mut();
        cache.entry(format!("{:?}",t.raw_miniquad_id())).or_insert_with(|| {
            let image=t.get_texture_data();let mut h=Sha256::new();
            h.update(image.width.to_le_bytes());h.update(image.height.to_le_bytes());h.update(&image.bytes);
            format!("{:x}",h.finalize())
        }).clone()
    }));
    PACKETS.with(|s|s.borrow_mut().push(serde_json::json!({"geometry":geometry,"gpu_geometry":gpu_geometry,"gpu_slot":gpu_slot,"vertices":vertices.len(),"indices":indices.len(),"uniforms":uniform_hash,"policy":policy,"texture":texture})));
    // The packet hash stays the comparison key. Bits are a sidecar so a field diff can name
    // the divergent uniform without changing the hash or the SIM_MAPS_FRAME line.
    static BITS: OnceLock<bool> = OnceLock::new();
    if *BITS.get_or_init(|| std::env::var_os("LOOKING_GLASS_SIM_MAPS_UNIFORM_BITS").is_some()) {
        let index = PACKET_NO.with(|c| { let i = c.get(); c.set(i + 1); i });
        let bits: Vec<u32> = uniforms.iter().copied().map(f32::to_bits).collect();
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open("uniform-bits.jsonl") {
            let _ = writeln!(file, "{}", serde_json::json!({
                "frame": FRAME_NO.with(Cell::get), "index": index, "bits": bits,
            }));
        }
    }
}
/// Host-only view motion leaves simulation inputs untouched.
pub fn moving_view(frame:u64,eye:glam::Vec3,target:glam::Vec3)->(glam::Vec3,glam::Vec3) {
    static ON:OnceLock<bool>=OnceLock::new();
    if !enabled() || !*ON.get_or_init(||std::env::var_os("LOOKING_GLASS_SKIN_MOVING_VIEW").is_some()) {return (eye,target);}
    let t=frame as f32*0.037;let offset=glam::vec3(t.sin()*28.,t.cos()*21.,(t*0.7).sin()*12.);
    (eye+offset,target+glam::vec3((t*0.6).sin()*35.,(t*0.5).cos()*24.,0.))
}

pub struct NumericAlloc(Option<crate::kernel_alloc::Scope>);
impl NumericAlloc {
    pub fn begin()->Self {
        static ON:OnceLock<bool>=OnceLock::new();
        let active=enabled() && *ON.get_or_init(||std::env::var_os("LOOKING_GLASS_SKIN_ALLOC").is_some());
        ALLOC_TRACKED.with(|v|v.set(active));PREPARE_ALLOC.with(|v|v.set([0;2]));
        Self(active.then(crate::kernel_alloc::Scope::begin))
    }
}
impl Drop for NumericAlloc { fn drop(&mut self) {if let Some(scope)=self.0.take() {
    let calls=scope.finish();ALLOC_TRACKED.with(|v|v.set(false));
    count("skin_numeric_allocation_calls",calls);count("skin_numeric_draws",1);
    let p=PREPARE_ALLOC.with(Cell::get);count("skin_worker_prepare_allocation_calls",p[0]);count("skin_rejected_prepare_allocation_calls",p[1]);
}}}

/// Deterministic host-only damage witness; gameplay NPCs keep their production hit path.
pub fn script_hits(frame:u64,npcs:&mut crate::npc::Npcs) {
    static ON:OnceLock<bool>=OnceLock::new();
    if !enabled() || !*ON.get_or_init(||std::env::var_os("LOOKING_GLASS_SKIN_DEATHS").is_some()) {return;}
    let (damage,kind)=match frame {70=>(1.,crate::combat::DamageKind::Electric),100=>(10000.,crate::combat::DamageKind::Ice),130=>(10000.,crate::combat::DamageKind::Other),_=>return};
    let targets=npcs.targets();let mut accepted=0;
    for t in &targets {accepted+=usize::from(npcs.hit(crate::combat::Hit {id:t.id,damage,kind,knockback:glam::Vec3::ZERO}).is_some());}
    println!("SIM_SKIN_HITS {}",serde_json::json!({"frame":frame,"targets":targets.len(),"accepted":accepted,"kind":format!("{kind:?}")}));
}

/// The camera matrix submitted this frame, recorded where the renderer receives it.
pub fn note_projection(matrix: glam::Mat4) {
    if skin_proof() {
        PROJECTION.with(|p| p.set(Some(matrix.to_cols_array().map(f32::to_bits))));
    }
}

/// Per-frame framebuffer hash plus the presentation inputs that produced it:
/// requested window, effective framebuffer, scaled target, GL viewport and the
/// camera matrix. `png` also keeps the frame as an image. A frame without a
/// recorded camera matrix or with an empty viewport/framebuffer is an error.
#[cfg(all(feature = "vita_renderer", feature = "sim_maps_probe", not(target_os = "vita")))]
pub fn record_exact_frame(case: &str, frame: u64, png: bool) -> anyhow::Result<()> {
    if !skin_proof() { return Ok(()); }
    let projection = PROJECTION.with(Cell::get).ok_or_else(|| anyhow::anyhow!("no camera matrix recorded for {case} frame {frame}"))?;
    let mut viewport = [0i32; 4];
    unsafe { macroquad::miniquad::gl::glGetIntegerv(0x0BA2, viewport.as_mut_ptr()); }
    anyhow::ensure!(viewport[2] > 0 && viewport[3] > 0, "empty GL viewport for {case} frame {frame}: {viewport:?}");
    let image = macroquad::prelude::get_screen_data();
    anyhow::ensure!(image.width > 0 && image.height > 0 && !image.bytes.is_empty(), "empty framebuffer for {case} frame {frame}");
    let png = if png {
        let path = std::path::PathBuf::from(format!("exact-frames/{case}-{frame:03}.png"));
        crate::viewer::save_capture(&path)?;
        Some(serde_json::json!({"path": path.display().to_string(), "sha256": format!("{:x}", Sha256::digest(std::fs::read(&path)?))}))
    } else { None };
    let (width, height, high_dpi) = crate::viewer::probe_window_request();
    println!("SKIN_EXACT_PRESENTATION {}", serde_json::json!({
        "case": case, "frame": frame,
        "requested_window": [width, height], "requested_high_dpi": high_dpi,
        "screen_size_bits": [macroquad::prelude::screen_width().to_bits(), macroquad::prelude::screen_height().to_bits()],
        "framebuffer": [image.width, image.height],
        "scaled_target": crate::render::vita_renderer::diagnostic_target(),
        "viewport": viewport,
        "projection_bits": projection,
    }));
    println!("SKIN_EXACT_IMAGE {}", serde_json::json!({
        "case": case, "frame": frame, "sha256": format!("{:x}", Sha256::digest(&image.bytes)),
        "width": image.width, "height": image.height, "png": png,
    }));
    Ok(())
}

/// Per-draw `GL_SAMPLES_PASSED` census. Host only, never linked into a Vita
/// package: one query brackets the GL draw that a submitted packet produces.
/// A query is issued only on the requested frame, so all other frames run the
/// unmodified path.
#[cfg(all(feature = "sim_maps_probe", not(target_os = "vita")))]
pub mod draw_query {
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;
    use std::sync::OnceLock;

    use macroquad::miniquad::gl;

    const SAMPLES_PASSED: u32 = 0x8914;
    const QUERY_RESULT: u32 = 0x8866;
    const BLEND: u32 = 0x0BE2;
    const DEPTH_TEST: u32 = 0x0B71;
    const DEPTH_WRITEMASK: u32 = 0x0B72;
    const BLEND_SRC_RGB: u32 = 0x80C9;
    const BLEND_DST_RGB: u32 = 0x80C8;
    const TEXTURE_BINDING_2D: u32 = 0x8069;
    const ACTIVE_TEXTURE: u32 = 0x84E0;
    const TEXTURE0: u32 = 0x84C0;
    const CURRENT_PROGRAM: u32 = 0x8B8D;
    const VIEWPORT: u32 = 0x0BA2;
    const DEPTH_FUNC: u32 = 0x0B74;
    const COLOR_WRITEMASK: u32 = 0x0C23;
    const ALWAYS: u32 = 0x0207;
    const MAX_ROWS: usize = 4096;

    #[derive(Clone)]
    struct Mark {
        pass: &'static str,
        class: &'static str,
        blend: usize,
        group: Option<usize>,
        alpha_test: u8,
        texture_name: Option<String>,
    }
    #[derive(Clone)]
    struct Row {
        seq: u64,
        pass: String,
        class: String,
        program: i32,
        program_name: String,
        group: Option<usize>,
        texture_name: Option<String>,
        blend: String,
        blend_enabled: bool,
        blend_src: i32,
        blend_dst: i32,
        depth_write: bool,
        depth_test: bool,
        texture: i32,
        triangles: u32,
        samples: u64,
        raster_samples: u64,
        viewport: [i32; 4],
    }
    pub struct RasterState { depth_func: u32, depth_write: i32, color_mask: [i32; 4] }

    thread_local! {
        static ARMED: Cell<bool> = const { Cell::new(false) };
        static MARK: RefCell<Option<Mark>> = const { RefCell::new(None) };
        static PENDING: RefCell<Vec<(u32, Option<u32>, Row)>> = RefCell::default();
        static SEQUENCE: Cell<u64> = const { Cell::new(0) };
        static SAMPLE_FRAME: Cell<u64> = const { Cell::new(0) };
        static PROGRAMS: RefCell<BTreeMap<u32, String>> = RefCell::default();
    }

    fn target_frame() -> u64 {
        static TARGET: OnceLock<u64> = OnceLock::new();
        *TARGET.get_or_init(|| std::env::var("LOOKING_GLASS_DRAW_QUERY_FRAME")
            .ok().and_then(|value| value.parse().ok()).unwrap_or(120))
    }
    /// The census is opt-in by environment variable, independent of the counter probe.
    pub fn active() -> bool {
        static ON: OnceLock<bool> = OnceLock::new();
        *ON.get_or_init(|| std::env::var_os("LOOKING_GLASS_DRAW_QUERY").is_some())
    }
    /// Count every rasterized fragment too, by drawing the packet a second time
    /// with depth test forced to always and colour/depth writes masked. The
    /// framebuffer cannot change: colour and depth writes are off for that draw.
    pub fn raster() -> bool {
        static ON: OnceLock<bool> = OnceLock::new();
        *ON.get_or_init(|| std::env::var_os("LOOKING_GLASS_DRAW_QUERY_RASTER").is_some())
    }
    /// The control arm selects the same GL observer table without issuing a query.
    pub fn table_active() -> bool {
        static ON: OnceLock<bool> = OnceLock::new();
        *ON.get_or_init(|| active() || std::env::var_os("LOOKING_GLASS_DRAW_QUERY_CONTROL").is_some())
    }
    /// True only while the requested frame is being submitted.
    pub fn armed() -> bool { ARMED.with(Cell::get) }
    pub fn set_frame(frame: u64) {
        if !active() { return; }
        if ARMED.with(Cell::get) { finish_previous(); }
        SEQUENCE.with(|value| value.set(0));
        static LAST: OnceLock<u64> = OnceLock::new();
        let last = *LAST.get_or_init(|| std::env::var("LOOKING_GLASS_DRAW_QUERY_LAST")
            .ok().and_then(|value| value.parse().ok()).unwrap_or_else(target_frame));
        let armed = frame >= target_frame() && frame <= last;
        if armed {
            SAMPLE_FRAME.with(|value| value.set(frame));
            println!("DRAW_QUERY armed frame={frame} target={}", target_frame());
        }
        ARMED.with(|value| value.set(armed));
    }
    /// Shader object identity, recorded when the program is linked.
    pub fn program(id: u32, name: &str) {
        if active() { PROGRAMS.with(|map| { map.borrow_mut().insert(id, name.to_string()); }); }
    }
    fn blend_name(index: usize) -> String {
        match index {
            0 => "Opaque".into(), 1 => "Alpha".into(), 2 => "Add".into(), 3 => "Filter".into(),
            4 => "DoubleFilter".into(), 5 => "AlphaAdd".into(), 6 => "ColorAdd".into(),
            7 => "Invisible".into(), 8 => "AlphaColor".into(), _ => format!("Factors({index})"),
        }
    }
    /// Attach the submitting pass to the draw that follows. Never allocates unless armed.
    pub fn mark(pass: &'static str, class: &'static str, blend: usize, group: Option<usize>,
                alpha_test: u8, texture_name: Option<&str>) {
        if armed() { MARK.with(|slot| *slot.borrow_mut() = Some(Mark {
            pass, class, blend, group, alpha_test,
            texture_name: texture_name.map(str::to_string),
        })); }
    }
    /// Deferred framework batches have no submission site: classify by the program.
    pub fn deferred_class(present_program: u32, current_program: u32) -> (&'static str, &'static str) {
        if present_program != 0 && current_program == present_program { ("upscale", "upscale") } else { ("hud", "hud") }
    }
    fn state_i32(name: u32) -> i32 {
        let mut value = 0i32;
        unsafe { gl::glGetIntegerv(name, &mut value); }
        value
    }
    /// Begin one query on the next draw. `count` and `instances` come from the GL call.
    pub fn begin(count: i32, instances: i32, deferred: Option<(&'static str, &'static str)>) -> Option<u32> {
        if !armed() { return None; }
        let mut id = 0u32;
        unsafe { gl::glGenQueries(1, &mut id); }
        if id == 0 { println!("DRAW_QUERY glGenQueries returned 0"); return None; }
        unsafe { gl::glBeginQuery(SAMPLES_PASSED, id); }
        let mark = MARK.with(|slot| slot.borrow_mut().take());
        let (pass, class, blend, group, texture_name) = match mark {
            Some(m) => (m.pass.to_string(), if m.class == "world_full" && m.alpha_test > 0 {
                format!("world_full_at{}", m.alpha_test)
            } else { m.class.to_string() }, blend_name(m.blend), m.group, m.texture_name),
            None => {
                let (pass, class) = deferred.unwrap_or(("other", "other"));
                (pass.to_string(), class.to_string(), "gl".to_string(), None, None)
            }
        };
        let mut viewport = [0i32; 4];
        unsafe { gl::glGetIntegerv(VIEWPORT, viewport.as_mut_ptr()); }
        // The last bound image unit is not the diffuse texture; sample unit 0.
        let active = state_i32(ACTIVE_TEXTURE) as u32;
        unsafe { gl::glActiveTexture(TEXTURE0); }
        let texture = state_i32(TEXTURE_BINDING_2D);
        unsafe { gl::glActiveTexture(active); }
        let program = state_i32(CURRENT_PROGRAM);
        let program_name = PROGRAMS.with(|map| map.borrow().get(&(program as u32)).cloned()).unwrap_or_default();
        let row = Row {
            seq: SEQUENCE.with(|value| { let n = value.get(); value.set(n + 1); n }),
            pass, class, program, program_name, group, texture_name,
            blend, blend_enabled: state_i32(BLEND) != 0,
            blend_src: state_i32(BLEND_SRC_RGB), blend_dst: state_i32(BLEND_DST_RGB),
            depth_write: state_i32(DEPTH_WRITEMASK) != 0,
            depth_test: state_i32(DEPTH_TEST) != 0,
            texture,
            triangles: if count > 0 && instances > 0 {
                (count as u32 / 3).saturating_mul(instances as u32)
            } else { 0 },
            samples: 0,
            raster_samples: 0,
            viewport,
        };
        PENDING.with(|rows| rows.borrow_mut().push((id, None, row)));
        if PENDING.with(|rows| rows.borrow().len() > MAX_ROWS) {
            panic!("draw query pool exceeded {MAX_ROWS} draws in one frame");
        }
        Some(id)
    }
    pub fn end(_id: Option<u32>) {
        if !armed() { return; }
        unsafe { gl::glEndQuery(SAMPLES_PASSED); }
    }
    fn state_i32_array(name: u32) -> [i32; 4] {
        let mut value = [0i32; 4];
        unsafe { gl::glGetIntegerv(name, value.as_mut_ptr()); }
        value
    }
    /// Start the raster-area query and mask every write of the duplicate draw.
    pub fn begin_raster() -> Option<RasterState> {
        if !armed() || !raster() { return None; }
        let state = RasterState {
            depth_func: state_i32(DEPTH_FUNC) as u32,
            depth_write: state_i32(DEPTH_WRITEMASK),
            color_mask: state_i32_array(COLOR_WRITEMASK),
        };
        unsafe {
            gl::glDepthFunc(ALWAYS);
            gl::glDepthMask(0);
            gl::glColorMask(0, 0, 0, 0);
        }
        let mut id = 0u32;
        unsafe { gl::glGenQueries(1, &mut id); gl::glBeginQuery(SAMPLES_PASSED, id); }
        PENDING.with(|rows| { if let Some(last) = rows.borrow_mut().last_mut() { last.1 = Some(id); } });
        Some(state)
    }
    pub fn end_raster(state: Option<RasterState>) {
        let Some(state) = state else { return; };
        unsafe {
            gl::glEndQuery(SAMPLES_PASSED);
            gl::glColorMask(state.color_mask[0] as u8, state.color_mask[1] as u8,
                state.color_mask[2] as u8, state.color_mask[3] as u8);
            gl::glDepthFunc(state.depth_func);
            gl::glDepthMask(u8::from(state.depth_write != 0));
        }
    }
    fn finish_previous() {
        let rows = PENDING.with(|slot| std::mem::take(&mut *slot.borrow_mut()));
        if rows.is_empty() { return; }
        let draws = rows.len() as u64;
        let frame = SAMPLE_FRAME.with(Cell::get);
        let mut total = 0u64;
        for (id, raster_id, mut row) in rows {
            let mut samples = 0u64;
            unsafe { gl::glGetQueryObjectui64v(id, QUERY_RESULT, &mut samples); }
            row.samples = samples;
            if let Some(raster_id) = raster_id {
                let mut raster_samples = 0u64;
                unsafe { gl::glGetQueryObjectui64v(raster_id, QUERY_RESULT, &mut raster_samples); }
                row.raster_samples = raster_samples;
                unsafe { gl::glDeleteQueries(1, &raster_id); }
            }
            total += samples;
            println!("SAMPLES_DRAW {}", serde_json::json!({
                "frame": frame, "seq": row.seq, "pass": row.pass, "class": row.class,
                "program": row.program, "program_name": row.program_name, "group": row.group,
                "texture_name": row.texture_name, "blend": row.blend, "blend_enabled": row.blend_enabled,
                "blend_src": row.blend_src, "blend_dst": row.blend_dst,
                "depth_write": row.depth_write, "depth_test": row.depth_test,
                "texture": row.texture, "triangles": row.triangles, "samples": row.samples,
                "raster_samples": row.raster_samples,
                "viewport": row.viewport,
            }));
            unsafe { gl::glDeleteQueries(1, &id); }
        }
        let error = unsafe { gl::glGetError() };
        let target = crate::render::vita_renderer::diagnostic_target();
        println!("SAMPLES_QUERY_SUMMARY {}", serde_json::json!({
            "frame": frame, "draws": draws, "samples": total, "raster": raster(),
            "host_target": target, "host_display": [macroquad::prelude::screen_width() as i32, macroquad::prelude::screen_height() as i32],
            "gl_error": error,
        }));
        ARMED.with(|value| value.set(false));
    }
}
pub fn prepare_checkpoint()->Option<u64> {
    ALLOC_TRACKED.with(Cell::get).then(crate::kernel_alloc::Scope::calls_so_far)
}
pub fn prepare_allocations(start:Option<u64>,accepted:bool) {
    if let Some(start)=start {let calls=crate::kernel_alloc::Scope::calls_so_far()-start;
        PREPARE_ALLOC.with(|p|{let mut v=p.get();v[usize::from(!accepted)]+=calls;p.set(v);});}
}

/// Synchronous test-only readback. Native builds contain none of this observer.
#[cfg(target_os="macos")]
pub fn gpu_geometry(vertex_count:usize,index_count:usize)->(Vec<macroquad::prelude::Vertex>,Vec<u16>) {
    use macroquad::prelude::*;
    #[link(name="OpenGL",kind="framework")]
    extern "C" {
        fn glFlush();
        fn glGetIntegerv(pname:u32,data:*mut i32);
        fn glGetVertexAttribiv(index:u32,pname:u32,data:*mut i32);
        fn glBindBuffer(target:u32,buffer:u32);
        fn glGetBufferSubData(target:u32,offset:isize,size:isize,data:*mut std::ffi::c_void);
    }
    let mut vertices=vec![Vertex {position:Vec3::ZERO,uv:Vec2::ZERO,normal:Vec4::ZERO,color:[0;4]};vertex_count];
    let mut indices=vec![0_u16;index_count];
    unsafe {
        // Without this the readback's own flush point depends on whether the buffer has a
        // pending upload, and those boundaries move host pixels by 1/255.
        glFlush();
        let mut old_array=0;let mut attribute_buffer=0;let mut index_buffer=0;
        glGetIntegerv(0x8894,&mut old_array); // ARRAY_BUFFER_BINDING
        glGetVertexAttribiv(0,0x889F,&mut attribute_buffer); // ATTRIB_ARRAY_BUFFER_BINDING
        glGetIntegerv(0x8895,&mut index_buffer); // ELEMENT_ARRAY_BUFFER_BINDING
        assert!(attribute_buffer>0 && index_buffer>0,"GPU oracle requires GL vertex/index bindings");
        glBindBuffer(0x8892,attribute_buffer as u32);
        glGetBufferSubData(0x8892,0,(vertices.len()*std::mem::size_of::<Vertex>()) as isize,vertices.as_mut_ptr().cast());
        glBindBuffer(0x8892,old_array as u32);
        glGetBufferSubData(0x8893,0,(indices.len()*2) as isize,indices.as_mut_ptr().cast());
    }
    (vertices,indices)
}
