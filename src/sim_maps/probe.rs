//! Explicit host diagnostics. Run from a private working/save directory.
use std::{cell::RefCell, collections::BTreeMap, sync::OnceLock, time::Instant};
use sha2::{Digest, Sha256};
thread_local! {
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
    crate::world_input_proof::begin();
    if enabled() {
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
        "frame": frame, "map": map, "active": active,
        "vertex_input_proof": crate::world_input_proof::report(),
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
