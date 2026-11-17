//! Opt-in device screenshots at fixed replay steps. Empty config is inert.
//!
//! The capture runs on the presented frame of the matching replay step and writes
//! `private/<tag>-<step>.png` through the ordinary `save_capture` path. It is not
//! compiled into anything without `vita_renderer`, and the default empty keys keep
//! every existing config at zero cost.
use std::sync::OnceLock;

struct Plan {
    tag: String,
    frames: Vec<u64>,
}

fn plan() -> Option<&'static Plan> {
    static PLAN: OnceLock<Option<Plan>> = OnceLock::new();
    PLAN.get_or_init(|| {
        let config = crate::render::vita_renderer::config();
        parse(&config.capture_frames, &config.capture_tag).ok().flatten()
    })
    .as_ref()
}

/// Validate both keys before a run: a tag is required with frames, and the tag is
/// a safe path segment so an arm can never escape `private/`.
pub(crate) fn validate(frames: &str, tag: &str) -> anyhow::Result<()> {
    let frames = frames.trim();
    if frames.is_empty() {
        anyhow::ensure!(tag.trim().is_empty(), "capture_tag requires capture_frames");
        return Ok(());
    }
    let parsed = parse(frames, tag)?;
    anyhow::ensure!(parsed.is_some(), "capture_frames is empty");
    Ok(())
}

fn parse(frames: &str, tag: &str) -> anyhow::Result<Option<Plan>> {
    let frames = frames.trim();
    if frames.is_empty() {
        return Ok(None);
    }
    let tag = tag.trim();
    anyhow::ensure!(!tag.is_empty(), "capture_tag is required when capture_frames is set");
    anyhow::ensure!(tag.len() <= 32 &&
        tag.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
        "capture_tag must be 1..32 lowercase letters, digits or '-'");
    let mut list = frames.split(',').map(|part| {
        let part = part.trim();
        anyhow::ensure!(!part.is_empty(), "capture_frames has an empty entry");
        let step: u64 = part.parse()?;
        anyhow::ensure!((1..=9_999_999).contains(&step), "capture_frames step out of range");
        Ok(step)
    }).collect::<anyhow::Result<Vec<_>>>()?;
    anyhow::ensure!(!list.is_empty() && list.len() <= 16, "capture_frames takes 1..16 steps");
    list.sort_unstable();
    anyhow::ensure!(list.windows(2).all(|w| w[0] != w[1]), "capture_frames repeats a step");
    Ok(Some(Plan { tag: tag.into(), frames: list }))
}

/// True when this replay step must save a picture.
pub(crate) fn wanted(step: u64) -> bool {
    plan().is_some_and(|plan| plan.frames.binary_search(&step).is_ok())
}

/// Save the current presented frame for this step, once.
pub(crate) fn save(step: u64) -> anyhow::Result<()> {
    let Some(plan) = plan() else { return Ok(()); };
    if plan.frames.binary_search(&step).is_err() { return Ok(()); }
    crate::viewer::save_capture(std::path::Path::new(&format!(
        "private/{}-{step:04}.png", plan.tag)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_keys_are_off_and_frames_require_a_safe_tag() {
        assert!(parse("", "").unwrap().is_none());
        assert!(validate("", "").is_ok());
        assert!(validate("850", "").is_err());
        assert!(validate("", "tag").is_err());
        assert!(validate("850", "../x").is_err());
        assert!(validate("850", "Tag").is_err());
        assert!(validate("850,850", "a").is_err());
        assert!(validate("0", "a").is_err());
        let plan = parse(" 962, 850 ", "wchess1-rc16").unwrap().unwrap();
        assert_eq!(plan.frames, vec![850, 962]);
        assert_eq!(plan.tag, "wchess1-rc16");
        assert!(plan.frames.binary_search(&850).is_ok());
    }
}
