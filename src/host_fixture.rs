//! Shared recorder for the host-only semantic fixtures (`--potears2-sight-fixture`,
//! `--hatter1-door-fixture`). Never compiled for the console.
//!
//! One process runs one arm, because the levers come from the startup config
//! `private/vita_renderer.json`. `tools/host_fixtures_r22.py` runs both arms and compares
//! the reports this module writes.
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// Selects the deliberately broken sequence of a negative control.
pub fn variant() -> String {
    std::env::var("LOOKING_GLASS_FIXTURE_VARIANT").unwrap_or_else(|_| "nominal".into())
}

pub fn levers() -> serde_json::Value {
    #[cfg(feature = "vita_renderer")]
    let (occurrences, layers) = {
        let c = crate::render::vita_renderer::config();
        (c.actor_publication_occurrences, c.actor_deferred_layers)
    };
    #[cfg(not(feature = "vita_renderer"))]
    let (occurrences, layers) = (false, false);
    serde_json::json!({
        "sim_trigger_facts_reuse": crate::sim_maps::trigger_facts_reuse(),
        "sim_sight_query_reuse": crate::sim_maps::sight_query_reuse(),
        "sim_hatter_door_candidates": crate::sim_maps::hatter_door_candidates(),
        "actor_publication_occurrences": occurrences,
        "actor_deferred_layers": layers,
    })
}

/// Probe counters that prove a lever's path ran; empty without `sim_maps_probe`.
fn counters() -> serde_json::Value {
    #[cfg(feature = "sim_maps_probe")]
    {
        serde_json::Value::Object(crate::sim_maps::probe::counters().into_iter()
            .map(|(k, v)| (k.to_string(), v.into())).collect())
    }
    #[cfg(not(feature = "sim_maps_probe"))]
    { serde_json::json!({}) }
}

pub struct Recorder {
    name: &'static str,
    state: Sha256,
    ticks: u64,
    pub events: Vec<serde_json::Value>,
}

impl Recorder {
    pub fn new(name: &'static str) -> Self {
        Self { name, state: Sha256::new(), ticks: 0, events: Vec::new() }
    }
    /// Hash one tick's complete serialized state into the ordered stream.
    pub fn tick(&mut self, state: &impl serde::Serialize) -> Result<()> {
        let bytes = serde_json::to_vec(state)?;
        self.state.update((bytes.len() as u64).to_le_bytes());
        self.state.update(&bytes);
        self.ticks += 1;
        Ok(())
    }
    pub fn event(&mut self, event: serde_json::Value) {
        self.events.push(event);
    }
    /// Write the report whether or not the assertions passed, then return `outcome`.
    pub fn finish(self, final_state: &impl serde::Serialize, summary: serde_json::Value,
        outcome: Result<()>) -> Result<()> {
        let bytes = serde_json::to_vec(final_state)?;
        let report = serde_json::json!({
            "fixture": self.name,
            "variant": variant(),
            "levers": levers(),
            "counters": counters(),
            "ticks": self.ticks,
            "state_stream_sha256": format!("{:x}", self.state.finalize()),
            "final_state_sha256": format!("{:x}", Sha256::digest(&bytes)),
            "summary": summary,
            "events": self.events,
            "passed": outcome.is_ok(),
            "failure": outcome.as_ref().err().map(|e| format!("{e:#}")),
        });
        std::fs::create_dir_all("private/host-fixtures")?;
        let stem = format!("private/host-fixtures/{}-{}", self.name, variant());
        std::fs::write(format!("{stem}.json"), serde_json::to_vec_pretty(&report)?)
            .context("Fixture report")?;
        std::fs::write(format!("{stem}-final.json"), bytes).context("Fixture final state")?;
        outcome
    }
}
