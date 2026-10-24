//! Explicit host diagnostics. Run from a private working/save directory.
use std::{cell::{Cell,RefCell}, collections::BTreeMap, sync::OnceLock, time::Instant};
use sha2::{Digest, Sha256};
thread_local! {
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
    static NPC_ACTOR: RefCell<String> = RefCell::default();
    static NPC_DECISIONS: RefCell<BTreeMap<String, (Sha256, u64)>> = RefCell::default();
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
pub fn begin() {
    if enabled() {
        SKINS.with(|s|s.borrow_mut().clear());
        PACKETS.with(|s|s.borrow_mut().clear());
        JOINT_ORDER.with(|s|s.borrow_mut().clear());
        WORLD_ORDER.with(|s|s.borrow_mut().clear());
        COUNTS.with(|c| c.borrow_mut().clear());
        SPANS.with(|c| c.borrow_mut().clear());
        NPC_DECISIONS.with(|c| c.borrow_mut().clear());
        crate::light_profile::render_host_reset();
        START.with(|s| *s.borrow_mut() = Some(Instant::now()));
    }
}
pub fn end(frame: u64, map: &str, active: bool, state: &impl serde::Serialize) {
    if !enabled() { return; }
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
        "npc_decisions": npc_decisions(),
        "spans_ms": SPANS.with(|s| s.borrow().clone()),
        "render": crate::light_profile::render_host_report()
    })));
    if std::env::var_os("LOOKING_GLASS_SIM_MAPS_STATE").is_some() {
        std::fs::write(format!("state-{frame:05}.json"), bytes).unwrap();
    }
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
pub fn packet(vertices:&[macroquad::prelude::Vertex],indices:&[u16],uniforms:&[f32],policy:&[u32],texture:Option<&macroquad::prelude::Texture2D>) {
    if !skin_proof() {return;}
    let geometry=format!("{:x}",mesh_hash(vertices,indices).finalize());
    let mut h=Sha256::new();for f in uniforms {h.update(f.to_bits().to_le_bytes());}
    let uniforms=format!("{:x}",h.finalize());
    let texture=texture.map(|t|TEXTURES.with(|cache| {let mut cache=cache.borrow_mut();
        cache.entry(format!("{:?}",t.raw_miniquad_id())).or_insert_with(|| {
            let image=t.get_texture_data();let mut h=Sha256::new();
            h.update(image.width.to_le_bytes());h.update(image.height.to_le_bytes());h.update(&image.bytes);
            format!("{:x}",h.finalize())
        }).clone()
    }));
    PACKETS.with(|s|s.borrow_mut().push(serde_json::json!({"geometry":geometry,"uniforms":uniforms,"policy":policy,"texture":texture})));
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

pub fn prepare_checkpoint()->Option<u64> {
    ALLOC_TRACKED.with(Cell::get).then(crate::kernel_alloc::Scope::calls_so_far)
}
pub fn prepare_allocations(start:Option<u64>,accepted:bool) {
    if let Some(start)=start {let calls=crate::kernel_alloc::Scope::calls_so_far()-start;
        PREPARE_ALLOC.with(|p|{let mut v=p.get();v[usize::from(!accepted)]+=calls;p.set(v);});}
}
