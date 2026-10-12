//! Startup-only, opt-in visual simulation compromises. Defaults preserve sim8.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{path::{Path, PathBuf}, sync::OnceLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PuffCollide {
    #[default]
    All,
    Moving,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub puff_cap: usize,
    pub puff_collide: PuffCollide,
    pub puff_sweep_every: u32,
    pub anim_cull: bool,
    pub anim_far_distance: Option<f32>,
    pub anim_far_every: u32,
    pub mover_far_every: u32,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            puff_cap: 4096,
            puff_collide: PuffCollide::All,
            puff_sweep_every: 1,
            anim_cull: false,
            anim_far_distance: None,
            anim_far_every: 1,
            mover_far_every: 1,
        }
    }
}
impl Config {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let config: Self = serde_json::from_slice(bytes)?;
        ensure!(config.puff_cap <= 4096, "puff_cap must be between 0 and 4096");
        for (name, value) in [
            ("puff_sweep_every", config.puff_sweep_every),
            ("anim_far_every", config.anim_far_every),
            ("mover_far_every", config.mover_far_every),
        ] {
            ensure!((1..=120).contains(&value), "{name} must be between 1 and 120");
        }
        ensure!(config.anim_far_distance.is_none_or(|d| d.is_finite() && d >= 0.),
            "anim_far_distance must be null or a finite nonnegative distance");
        Ok(config)
    }
    fn read(path: &Path) -> Result<Self> {
        match std::fs::read(path) {
            Ok(bytes) => Self::parse(&bytes).with_context(|| format!("Invalid {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("Cannot read {}", path.display())),
        }
    }
}
static CONFIG: OnceLock<Config> = OnceLock::new();
pub fn effective() -> &'static Config {
    CONFIG.get_or_init(Config::default)
}
pub fn path(save_dir: &Path) -> PathBuf {
    #[cfg(target_os = "vita")]
    { let _ = save_dir; PathBuf::from("ux0:data/alice/private/vita_sim.json") }
    #[cfg(not(target_os = "vita"))]
    { save_dir.parent().unwrap_or(Path::new("private")).join("vita_sim.json") }
}
pub fn initialize(save_dir: &Path) {
    CONFIG.get_or_init(|| {
        let config = Config::read(&path(save_dir)).unwrap_or_else(|error| {
            eprintln!("[lg/vita-sim] error {error:#}; using full defaults");
            Config::default()
        });
        println!("[lg/vita-sim] config {}", serde_json::to_string(&config).expect("finite config"));
        config
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_config_defaults_and_partial_override() {
        assert_eq!(Config::parse(b"{}").unwrap(), Config::default());
        let c = Config::parse(br#"{"puff_cap":128}"#).unwrap();
        assert_eq!(c, Config { puff_cap: 128, ..Config::default() });
        assert_eq!(Config::parse(&serde_json::to_vec(&c).unwrap()).unwrap(), c);
    }
    #[test]
    fn invalid_config_rejects_whole_document() {
        for s in [r#"{"unknown":true}"#, r#"{"puff_cap":4097}"#,
            r#"{"puff_cap":-1}"#, r#"{"puff_cap":1.5}"#, r#"{"puff_collide":"fast"}"#,
            r#"{"puff_sweep_every":0}"#, r#"{"anim_far_every":121}"#,
            r#"{"mover_far_every":0}"#, r#"{"anim_far_distance":-1}"#,
            r#"{"anim_far_distance":1e100}"#, r#"{"anim_cull":1}"#,
            r#"{"puff_cap":128,"puff_cap":256}"#, "[]", "null", "{", "{}{}"] {
            assert!(Config::parse(s.as_bytes()).is_err(), "{s}");
        }
    }
    #[test]
    fn file_errors_and_missing_file_use_full_defaults() {
        let dir = std::env::temp_dir().join(format!("lg-sim9-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("vita_sim.json");
        assert_eq!(Config::read(&file).unwrap(), Config::default());
        std::fs::write(&file, br#"{"puff_cap":128,"typo":2}"#).unwrap();
        assert_eq!(Config::read(&file).unwrap_or_default(), Config::default());
        assert!(Config::read(&dir).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn host_config_is_next_to_save_folder() {
        assert_eq!(path(Path::new("private/saves")), PathBuf::from("private/vita_sim.json"));
        assert_eq!(path(Path::new("/tmp/game/private/saves")), PathBuf::from("/tmp/game/private/vita_sim.json"));
    }
}
