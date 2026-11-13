//! Player-facing text that names desktop keys, reworded for the Vita buttons.
//!
//! Banners, world prompts and objectives are written for the keyboard. Under the
//! Vita controls (the console, or `LOOKING_GLASS_VITA_CONTROLS` on the host) the
//! screen shows the handheld template below in their place. A template names a
//! gameplay action in braces by its desktop default key, so `{Space}` is jump and
//! takes whatever button the live bindings give jump. `{skip}` takes the scene-skip
//! label. Buttons the bindings cannot move (Start, the sticks, Cross and Circle on
//! fixed screens) are plain text.
use crate::{input::Input, preferences::Preferences};
use std::borrow::Cow;

/// Exact desktop text, then its handheld template.
pub const HANDHELD: &[(&str, &str)] = &[
    // HUD banners.
    (
        "Swimming / Space rise / Ctrl dive / look and W to swim",
        "Swimming / {Space} rise / {Ctrl} dive / left stick swims where you look",
    ),
    (
        "Previous save restored / P or click to resume",
        "Previous save restored / Cross or Start to resume",
    ),
    ("Game restored / P or click to resume", "Game restored / Cross or Start to resume"),
    (
        "A saved game could not be read. F9 retries quick load.",
        "A saved game could not be read. Load another save from the Start menu.",
    ),
    ("A scene is playing / hold Enter to skip", "A scene is playing / hold {skip} to skip"),
    (
        "Entrance is obstructed / choose another level with Tab",
        "Entrance is obstructed / choose another level with {Tab}",
    ),
    ("Lost beyond the level / Enter to retry", "Lost beyond the level / {Enter} to retry"),
    ("Fatal fall / Enter to retry", "Fatal fall / {Enter} to retry"),
    ("Game saved / F9 to load", "Game saved / load it from the Start menu"),
    ("Sound muted - M to restore", "Sound muted / unmute in Start, Settings, Audio"),
    (
        "Enter / Cross: retry    Escape / Circle: quit",
        "Cross: retry    Circle: quit",
    ),
    // World prompts: ledge and ropes.
    (
        "Forward or Space pull up / A-D move along edge / Back or Ctrl let go",
        "Forward or {Space} pull up / left stick moves along edge / back or {Ctrl} let go",
    ),
    (
        "E release rope / Space up / Ctrl down / WASD swing",
        "{E} release rope / {Space} up / {Ctrl} down / left stick swing",
    ),
    ("E grab rope", "{E} grab rope"),
    // Objectives: school2 announces its own; the others show in the help panel.
    ("Listen to the Elder Gnome / E advances dialogue.", "Listen to the Elder Gnome / {E} advances dialogue."),
    (
        "Listen to the Gnome's directions / E advances dialogue.",
        "Listen to the Gnome's directions / {E} advances dialogue.",
    ),
    (
        "Explore the east gymnasium. E uses its lever; the Elder Gnome waits above.",
        "Explore the east gymnasium. {E} uses its lever; the Elder Gnome waits above.",
    ),
    (
        "Explore the village; approach its gnomes and press E to talk.",
        "Explore the village; approach its gnomes and press {E} to talk.",
    ),
    (
        "Descend to the Elder Gnome, then follow the path to the minecart. E grabs/releases ropes; Space/Ctrl climbs; WASD swings.",
        "Descend to the Elder Gnome, then follow the path to the minecart. {E} grabs/releases ropes; {Space}/{Ctrl} climbs; left stick swings.",
    ),
];

pub fn template(text: &str) -> Option<&'static str> {
    HANDHELD.iter().find(|(desktop, _)| *desktop == text).map(|(_, t)| *t)
}

/// Replace each `{action}` with `label(action)`.
pub fn fill(template: &str, label: impl Fn(&str) -> String) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else { break };
        out.push_str(&rest[..open]);
        out.push_str(&label(&rest[open + 1..open + close]));
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

/// The handheld wording of `text`, or `None` when the table has no entry for it.
pub fn handheld(text: &str, label: impl Fn(&str) -> String) -> Option<String> {
    template(text).map(|t| fill(t, label))
}

/// What the screen shows for `text`: the handheld wording under the Vita
/// controls when the table has one, otherwise `text` unchanged.
pub fn shown<'a>(text: &'a str, input: &Input, prefs: &Preferences) -> Cow<'a, str> {
    if Preferences::vita_emulation() {
        let label = |action: &str| {
            if action == "skip" { input.film_skip_label(prefs) } else { input.label(prefs, action) }
        };
        if let Some(s) = handheld(text, label) {
            return Cow::Owned(s);
        }
    }
    Cow::Borrowed(text)
}

/// A world prompt as shown: the table wording, else the leading use key in the
/// "E  verb", "E / verb" and "E: verb" forms replaced by the use action's label.
pub fn world_prompt(text: &str, input: &Input, prefs: &Preferences) -> String {
    use_key(&shown(text, input, prefs), &input.label(prefs, "E"))
}

pub fn use_key(text: &str, label: &str) -> String {
    text.replace("E  ", &format!("{label}  "))
        .replace("E /", &format!("{label} /"))
        .replace("E: ", &format!("{label}: "))
}

#[cfg(test)]
mod tests;
