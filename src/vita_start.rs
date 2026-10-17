//! One-use console test starts. Checkpoints are read in place; player saves stay read-only.
use crate::{assets::Assets, campaign_chain, save, Options};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Visit {
    Number(u32),
    Name(String),
}
impl Visit {
    fn selector(&self) -> String {
        match self {
            Self::Number(n) => n.to_string(),
            Self::Name(s) => s.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub campaign_from: Option<Visit>,
    pub map: Option<String>,
    pub entry: Option<String>,
    pub frames: Option<u64>,
    pub capture: Option<String>,
    #[serde(skip)]
    directory: PathBuf,
}

fn simple_name(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}
impl Request {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.campaign_from.is_some() != self.map.is_some(),
            "vita_start requires exactly one of campaign_from or map"
        );
        ensure!(
            self.campaign_from.is_none() || self.entry.is_none(),
            "entry belongs to a map start, not campaign_from"
        );
        if let Some(v) = &self.campaign_from {
            campaign_chain::parse_visit(&v.selector())?;
        }
        ensure!(
            self.map.as_deref().is_none_or(simple_name),
            "Invalid map name"
        );
        ensure!(
            self.entry.as_deref().is_none_or(simple_name),
            "Invalid entrance name"
        );
        ensure!(self.frames != Some(0), "frames must be positive");
        if let Some(name) = &self.capture {
            ensure!(self.frames.is_some(), "capture requires frames");
            ensure!(
                name.strip_suffix(".png").is_some_and(|stem| {
                    !stem.is_empty()
                        && stem
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
                }),
                "capture must be a simple .png filename in private"
            );
            ensure!(
                !self.directory.join(name).exists(),
                "Capture already exists: {name}"
            );
        }
        Ok(())
    }

    pub fn apply(&self, options: &mut Options) {
        options.load = None;
        options.new_game = false;
        options.direct_map = true;
        options.frames = self.frames;
        options.capture = self.capture.as_ref().map(|s| self.directory.join(s));
        if let Some(map) = &self.map {
            options.map = map.clone();
            options.entry = self.entry.clone();
        }
    }

    pub fn restore(&self, assets: &mut Assets) -> Result<Option<save::Restored>> {
        let Some(visit) = &self.campaign_from else {
            return Ok(None);
        };
        let (checkpoint, hash) =
            campaign_chain::load_start_checkpoint(assets, &self.directory, &visit.selector())?;
        println!(
            "[lg/vita-start] checkpoint_restored visit={} map={} state_hash={hash}",
            visit.selector(),
            checkpoint.level.map
        );
        // Chain entrances have no advanced headless actors to translate. The existing
        // native save-chain bridge supplies Alice and the cast from the same game assets.
        ensure!(
            checkpoint.route.ticks == 0,
            "Only campaign entrance checkpoints are playable"
        );
        let restored = campaign_chain::save_chain::restore_entrance(assets, &checkpoint)?;
        println!(
            "[lg/vita-start] native_restored map={} native_state_hash={}",
            checkpoint.level.map,
            native_hash(&restored)?
        );
        Ok(Some(restored))
    }
}

/// Consume even malformed requests before parsing/loading, so a bad request cannot loop.
/// No directory is created, and a failure to remove the request prevents the test start.
pub fn consume(path: &Path) -> Result<Option<Request>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).context("Opening vita_start request"),
    };
    let mut bytes = Vec::new();
    file.take(16 * 1024 + 1).read_to_end(&mut bytes)?;
    fs::remove_file(path).context("Consuming vita_start request")?;
    println!(
        "[lg/vita-start] consumed path={} sha256={} saves=read-only",
        path.display(),
        format_args!("{:x}", Sha256::digest(&bytes))
    );
    ensure!(bytes.len() <= 16 * 1024, "vita_start exceeds 16 KiB");
    let mut request: Request = serde_json::from_slice(&bytes).context("Invalid vita_start JSON")?;
    request.directory = path
        .parent()
        .context("vita_start has no parent directory")?
        .to_owned();
    request.validate()?;
    println!("[lg/vita-start] selected {request:?}");
    Ok(Some(request))
}

/// Hash the actual restored native objects, before the first simulation tick.
/// Kept separate from Route::state_hash: native Alice/cast replace headless stand-ins.
pub fn native_hash(restored: &save::Restored) -> Result<String> {
    let mut game = restored.game.clone();
    game.character = restored.alice.snapshot();
    let level = game
        .campaign
        .levels
        .get_mut(&game.current)
        .context("Missing current visit")?;
    level.interactions = restored.interactions.snapshot();
    level.npcs = restored.npcs.snapshot();
    level.story = restored.story.snapshot();
    level.hints = restored.hints.snapshot();
    game_hash(&game)
}

pub fn game_hash(game: &save::Game) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&serde_json::to_value(game)?)?)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    fn directory() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "lg-start-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        p
    }
    #[test]
    fn absent_requests_do_nothing_and_valid_requests_are_consumed_once() {
        let dir = directory();
        let path = dir.join("vita_start.json");
        assert!(consume(&path).unwrap().is_none());
        fs::write(
            &path,
            r#"{"campaign_from":30,"frames":1,"capture":"checkpoint-pilot-30.png"}"#,
        )
        .unwrap();
        let r = consume(&path).unwrap().unwrap();
        assert_eq!(r.campaign_from.unwrap().selector(), "30");
        assert!(!path.exists());
        assert!(consume(&path).unwrap().is_none());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn invalid_requests_are_consumed_and_cannot_write_outside_private() {
        let dir = directory();
        let path = dir.join("vita_start.json");
        for body in [
            r#"{}"#,
            r#"{"map":"skool1","campaign_from":6}"#,
            r#"{"map":"skool1","frames":0}"#,
            r#"{"map":"../saves"}"#,
            r#"{"map":"skool1","capture":"auto.json","frames":1}"#,
            r#"{"map":"skool1","capture":"../shot.png","frames":1}"#,
            r#"{"map":"skool1","capture":"shot.png"}"#,
            r#"{"campaign_from":40}"#,
            r#"{"campaign_from":6,"entry":"start"}"#,
            r#"{"map":"skool1","typo":true}"#,
            "broken",
        ] {
            fs::write(&path, body).unwrap();
            assert!(consume(&path).is_err(), "{body}");
            assert!(!path.exists());
        }
        fs::write(dir.join("shot.png"), b"preserve").unwrap();
        fs::write(&path, r#"{"map":"skool1","capture":"shot.png","frames":1}"#).unwrap();
        assert!(consume(&path).is_err());
        assert_eq!(fs::read(dir.join("shot.png")).unwrap(), b"preserve");
        fs::write(&path, vec![b' '; 16385]).unwrap();
        assert!(consume(&path).is_err());
        assert!(!path.exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn named_visits_and_map_entrances_are_accepted() {
        for body in [
            r#"{"campaign_from":"fortress1$return"}"#,
            r#"{"map":"skool1","entry":"skool1_start2"}"#,
        ] {
            let r: Request = serde_json::from_str(body).unwrap();
            r.validate().unwrap();
        }
    }
}
