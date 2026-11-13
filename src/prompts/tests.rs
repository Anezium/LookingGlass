use super::*;

const SINGLE_KEYS: &[&str] = &["W", "S", "D", "E", "Q", "P", "C", "V", "H", "R", "M"];
const ANYWHERE_KEYS: &[&str] = &[
    "Ctrl", "Shift", "Esc", "Backspace", "WASD", "A-D", "1-0", "F1", "F2", "F3", "F4", "F5",
    "F9", "F12", "PgUp", "PgDn", "click", "Click", "Double-click", "mouse", "Mouse", "keyboard",
    "Keyboard", "Wheel", "Arrows", "arrows",
];
/// English words too: a key only when they do not start a sentence.
const INNER_KEYS: &[&str] = &["Space", "Enter", "Escape", "Tab", "Home"];

/// Desktop key names in `text`. Words keep their hyphens, so "D-pad" is not "D".
fn desktop_keys(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut sentence_start = true;
    let mut word = String::new();
    let mut flush = |word: &mut String, sentence_start: &mut bool| {
        if word.is_empty() {
            return;
        }
        let w = word.as_str();
        if SINGLE_KEYS.contains(&w)
            || ANYWHERE_KEYS.contains(&w)
            || (INNER_KEYS.contains(&w) && !*sentence_start)
        {
            found.push(w.to_owned());
        }
        *sentence_start = false;
        word.clear();
    };
    for c in text.chars() {
        if c.is_alphanumeric() || c == '-' || c == '\'' {
            word.push(c);
        } else {
            flush(&mut word, &mut sentence_start);
            if matches!(c, '.' | '!' | '?' | ';') {
                sentence_start = true;
            }
        }
    }
    flush(&mut word, &mut sentence_start);
    found
}

fn vita_preset() -> (Input, Preferences) {
    let mut prefs = Preferences::default();
    prefs.apply_vita_preset();
    (Input::default(), prefs)
}

fn vita_wording(text: &str, input: &Input, prefs: &Preferences) -> Option<String> {
    handheld(text, |action| {
        if action == "skip" {
            format!("{} / Cross", input.label_with(prefs, "Enter", true))
        } else {
            input.label_with(prefs, action, true)
        }
    })
}

#[test]
fn every_template_names_vita_buttons_only() {
    let (input, prefs) = vita_preset();
    for (desktop, template) in HANDHELD {
        assert!(!desktop_keys(desktop).is_empty(), "no desktop key in {desktop:?}");
        let shown = vita_wording(desktop, &input, &prefs).unwrap();
        assert!(!shown.contains(['{', '}']), "unfilled action in {shown:?}");
        assert!(!shown.contains("None"), "unbound action in {shown:?}");
        assert_eq!(desktop_keys(&shown), Vec::<String>::new(), "{desktop:?} -> {shown:?}");
        // Every braced action is a real binding row or the skip label.
        let mut rest = *template;
        while let Some(open) = rest.find('{') {
            let close = open + rest[open..].find('}').unwrap();
            let action = &rest[open + 1..close];
            assert!(
                action == "skip" || crate::preferences::BINDINGS.iter().any(|(_, k)| *k == action),
                "{action} is not a binding"
            );
            rest = &rest[close + 1..];
        }
    }
}

#[test]
fn vita_preset_wording_of_the_swim_banner() {
    let (input, prefs) = vita_preset();
    let cases = [
        (
            "Swimming / Space rise / Ctrl dive / look and W to swim",
            "Swimming / Cross rise / Circle dive / left stick swims where you look",
        ),
        ("E grab rope", "Square grab rope"),
        (
            "E release rope / Space up / Ctrl down / WASD swing",
            "Square release rope / Cross up / Circle down / left stick swing",
        ),
        ("A scene is playing / hold Enter to skip", "A scene is playing / hold Triangle / Cross to skip"),
        ("Fatal fall / Enter to retry", "Fatal fall / Triangle to retry"),
        (
            "Entrance is obstructed / choose another level with Tab",
            "Entrance is obstructed / choose another level with Select",
        ),
    ];
    for (desktop, vita) in cases {
        assert_eq!(vita_wording(desktop, &input, &prefs).as_deref(), Some(vita));
    }
    assert_eq!(vita_wording("Checkpoint saved", &input, &prefs), None);
}

#[test]
fn rebinding_moves_the_named_button() {
    let (input, mut prefs) = vita_preset();
    let jump = crate::preferences::BINDINGS.iter().position(|(_, k)| *k == "Space").unwrap();
    prefs.bindings[jump] = "Tab".into(); // Select
    let tab = crate::preferences::BINDINGS.iter().position(|(_, k)| *k == "Tab").unwrap();
    prefs.bindings[tab] = "None".into();
    let shown = vita_wording("E release rope / Space up / Ctrl down / WASD swing", &input, &prefs);
    assert_eq!(shown.as_deref(), Some("Square release rope / Select up / Circle down / left stick swing"));
}

#[test]
fn vita_controls_show_the_handheld_text() {
    if !Preferences::vita_emulation() {
        return; // Run with LOOKING_GLASS_VITA_CONTROLS=1 to cover this branch on the host.
    }
    let (input, prefs) = vita_preset();
    for (desktop, _) in HANDHELD {
        let shown = shown(desktop, &input, &prefs);
        assert_eq!(Some(shown.as_ref()), vita_wording(desktop, &input, &prefs).as_deref());
        assert!(desktop_keys(&shown).is_empty(), "{shown:?}");
    }
    assert_eq!(world_prompt("E: use lever", &input, &prefs), "Square: use lever");
    assert_eq!(world_prompt("E grab rope", &input, &prefs), "Square grab rope");
    assert_eq!(shown("Checkpoint saved", &input, &prefs), "Checkpoint saved");
}

#[test]
fn desktop_controls_keep_the_desktop_text() {
    if Preferences::vita_emulation() {
        return; // The host run under LOOKING_GLASS_VITA_CONTROLS covers the other branch.
    }
    let input = Input::default();
    let prefs = Preferences::default();
    for (desktop, _) in HANDHELD {
        assert_eq!(shown(desktop, &input, &prefs), *desktop);
    }
}

#[test]
fn use_key_forms_take_the_use_label() {
    let (input, prefs) = vita_preset();
    let label = input.label_with(&prefs, "E", true);
    assert_eq!(label, "Square");
    for (desktop, vita) in [
        ("E  open door", "Square  open door"),
        ("E / Turn the mirror", "Square / Turn the mirror"),
        ("E: use lever", "Square: use lever"),
        ("E: open door", "Square: open door"),
    ] {
        assert_eq!(use_key(desktop, &label), vita);
        assert!(desktop_keys(&use_key(desktop, &label)).is_empty());
    }
    let rope = vita_wording("E grab rope", &input, &prefs).unwrap();
    assert_eq!(use_key(&rope, &label), "Square grab rope");
}

#[test]
fn fill_leaves_plain_text_and_an_unclosed_brace_alone() {
    assert_eq!(fill("plain", |_| unreachable!()), "plain");
    assert_eq!(fill("{a} and {b}", |a| a.to_uppercase()), "A and B");
    assert_eq!(fill("open {a", |_| unreachable!()), "open {a");
}

/// String literals of non-test code: (file, line, text).
fn literals(root: &std::path::Path) -> Vec<(String, usize, String)> {
    fn walk(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, files);
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(root, &mut files);
    files.sort();
    let mut out = Vec::new();
    for path in files {
        let name = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
        if name.ends_with("tests.rs") || name.ends_with("_test.rs") || name == "prompts.rs" {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        // Drop a trailing test module.
        let source = match source.find("#[cfg(test)]\nmod tests {") {
            Some(i) => &source[..i],
            None => &source[..],
        };
        let b = source.as_bytes();
        let (mut i, mut line) = (0, 1);
        while i < b.len() {
            match b[i] {
                b'\n' => line += 1,
                b'/' if b.get(i + 1) == Some(&b'/') => {
                    while i < b.len() && b[i] != b'\n' {
                        i += 1;
                    }
                    continue;
                }
                b'\'' if b.get(i + 2) == Some(&b'\'') || b.get(i + 1) == Some(&b'\\') => {
                    // A char literal such as '"' or '\''.
                    i += 1;
                    while i < b.len() && b[i] != b'\'' {
                        i += if b[i] == b'\\' { 2 } else { 1 };
                    }
                }
                b'"' => {
                    let (start, start_line) = (i + 1, line);
                    i += 1;
                    while i < b.len() && b[i] != b'"' {
                        if b[i] == b'\n' {
                            line += 1;
                        }
                        i += if b[i] == b'\\' { 2 } else { 1 };
                    }
                    let text = source[start..i.min(b.len())].to_owned();
                    // The literal's line and the two before it, for multi-line macro calls.
                    let line_text = source[..start].rsplitn(4, '\n').take(3).collect::<Vec<_>>().join("\n");
                    out.push((format!("{name}:{start_line}"), start_line, format!("{line_text}\u{0}{text}")));
                }
                _ => {}
            }
            i += 1;
        }
    }
    out
}

/// Literals that name a desktop key but never reach a Vita screen as written, with the
/// reason. Matched on the exact literal text.
const NOT_ON_VITA: &[(&str, &str)] = &[
    // Play tape line formats (src/replay/tape.rs): file records, never shown to a player.
    ("C {} {} {} {} {:.2},{:.2},{:.2} {:.1} {}", "play tape check record, not player text"),
    ("H {}", "play tape header record, not player text"),
    ("E {step} {kind}", "play tape event record, not player text"),
    // Desktop branch of a choice that already has a Vita branch.
    ("{} or click to resume", "pause panel; Vita branch says Cross or Start"),
    ("P or click to resume", "HUD presentation check page, host only"),
    ("P to resume film", "film pause; Vita branch says Start"),
    ("Esc - load a game or change options", "death panel; Vita branch says Start"),
    ("Arrows / click: select    {} / Esc: return", "inventory; Vita branch names the D-pad"),
    ("D / click", "chapter chooser; Vita branch says touch"),
    ("Click: select   Double-click: begin   Wheel / arrows: browse", "chapter chooser; Vita branch"),
    ("Begin / Enter", "chapter chooser; Vita branch says Cross"),
    ("Return / Tab / Esc", "chapter chooser; Vita branch says Circle"),
    ("Press a button   Start / Esc cancel   Delete clear", "binding page; Vita branch"),
    ("Press a key, mouse button or wheel   Esc cancels", "binding page; Vita branch"),
    ("Arrows adjust   Enter select   Esc cancel", "menu footer; Vita branch"),
    ("Tab: chapters", "menu footer; Vita branch says Select"),
    ("Tab: chapters   Esc: back to game", "menu footer; Vita branch"),
    ("Mouse / look keys", "help panel; Vita branch says Right stick"),
    ("W A S D move   Q / E down / up   Shift faster", "help panel; Vita branch builds it from labels"),
    ("W A S D swing   Space / Ctrl climb   E release", "help panel; Vita branch"),
    ("W A S D swim   Space rise   Ctrl dive   E use", "help panel; Vita branch"),
    ("W A S D move   Space jump   Shift run   E interact", "help panel; Vita branch"),
    // Settings page: names of the desktop device, shown only in the desktop layout.
    ("Invert Mouse", "desktop setting name"),
    ("Keyboard / Mouse", "desktop controls page title"),
    // Never shown on the Vita: no XInput pad there, so no disconnect.
    ("Controller disconnected / reconnect or use keyboard and mouse", "XInput only (Windows)"),
    // Staged previews started from host command-line flags.
    ("Weapon preview / all toys equipped with 1-0", "host preview flag"),
    ("Combat preview / 1 Blade, 2 Cards, 3 Mallet / Home retry", "host preview flag"),
    ("Shelf crossing preview / WASD move, Space jump", "host preview flag"),
    ("Larva encounter / WASD move, mouse attack / Home retry", "host preview flag"),
    // Developer console, opened with the backtick key only.
    ("Console   ~ / Esc to close", "developer console"),
    ("Type help. Enter runs; Tab completes; Up/Down history; PgUp/PgDn scroll.", "developer console"),
    ("save / F5  quick save    load / F9  quick load    load auto", "developer console"),
    // Engine checks and logs, not player text.
    ("Wheel Up", "binding name"),
    ("Wheel Down", "binding name"),
    ("Mouse 1", "binding name"),
    ("Mouse 2", "binding name"),
    ("Mouse 3", "binding name"),
    ("R Shift", "key name table"),
    ("R Ctrl", "key name table"),
    ("Wheel geometry orbits through the cart bed", "check failure text"),
    ("Wheel blocked/free crush differs: {}", "check failure text"),
    ("Wheel left Alice embedded", "check failure text"),
    ("Reviewed script exits: {} adapters in Rust, 0 registered exit specifications (F4 pending)", "campaign graph report"),
    ("{id} is owned by a legacy controller and takes no registration (F1.4a)", "registration error text"),
];

/// Lines that print to a log, a terminal or an error, not to the screen.
fn developer_line(line: &str) -> bool {
    [
        "println!", "eprintln!", "print!(", "ensure!", "bail!", "anyhow!", "assert", "panic!",
        "context(", "with_context", "console.print", "Err(", "expect(", "format_err",
        "log::", "writeln!", "debug!", "unreachable!",
    ]
    .iter()
    .any(|m| line.contains(m))
}

/// Files that only drive host checks, routes and fixtures.
fn developer_file(name: &str) -> bool {
    let stem = name.rsplit('/').next().unwrap_or(name);
    ["check", "route", "fixture", "probe", "census", "survey", "timing", "bench", "registry"]
        .iter()
        .any(|m| stem.contains(m))
        || name == "main.rs"
}

#[test]
fn every_key_naming_literal_has_vita_wording_or_a_reason() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let all = literals(&root);
    let mut missing = Vec::new();
    let mut table_seen = vec![false; HANDHELD.len()];
    let mut excuse_seen = vec![false; NOT_ON_VITA.len()];
    for (place, _, packed) in &all {
        let (line, text) = packed.split_once('\u{0}').unwrap();
        if let Some(i) = HANDHELD.iter().position(|(d, _)| *d == text) {
            table_seen[i] = true;
            continue;
        }
        if let Some(i) = NOT_ON_VITA.iter().position(|(d, _)| *d == text) {
            excuse_seen[i] = true;
            continue;
        }
        let file = place.rsplit_once(':').unwrap().0;
        if text.len() < 4 || !text.contains(' ') || developer_file(file) || developer_line(line) {
            continue;
        }
        // World prompts in the "E  verb" and "E / verb" forms take the live label at the
        // prompt sink; key-name tables keep their own spelling.
        if text.starts_with("E  ") || text.starts_with("E / ") || text.starts_with("E: ") || place.starts_with("input.rs") || place.starts_with("preferences.rs") {
            continue;
        }
        let keys = desktop_keys(text);
        if !keys.is_empty() {
            missing.push(format!("{place}: {text:?} names {keys:?}"));
        }
    }
    for (i, seen) in table_seen.iter().enumerate() {
        assert!(*seen, "stale table entry, no such literal: {:?}", HANDHELD[i].0);
    }
    for (i, seen) in excuse_seen.iter().enumerate() {
        assert!(*seen, "stale excuse, no such literal: {:?}", NOT_ON_VITA[i].0);
    }
    assert!(missing.is_empty(), "key-naming text without Vita wording:\n{}", missing.join("\n"));
}

/// The before and after list for the report: `cargo test prompts::tests::print_list -- --nocapture --ignored`.
#[test]
#[ignore]
fn print_list() {
    let (input, prefs) = vita_preset();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let all = literals(&root);
    for (desktop, _) in HANDHELD {
        let places: Vec<_> = all
            .iter()
            .filter(|(_, _, p)| p.split_once('\u{0}').unwrap().1 == *desktop)
            .map(|(place, _, _)| place.as_str())
            .collect();
        println!("{}\t{desktop}\t{}", places.join(" "), vita_wording(desktop, &input, &prefs).unwrap());
    }
    for (text, reason) in NOT_ON_VITA {
        let places: Vec<_> = all
            .iter()
            .filter(|(_, _, p)| p.split_once('\u{0}').unwrap().1 == *text)
            .map(|(place, _, _)| place.as_str())
            .collect();
        println!("EXCUSED\t{}\t{text}\t{reason}", places.join(" "));
    }
}
