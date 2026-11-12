//! First-start check of the game data folder on the Vita.
//!
//! `Assets::open` mounts whatever `*.pk3` it finds and checks no name or size, so a missing
//! or truncated pack fails later, far from the cause. This check runs before that and
//! compares names and sizes only: hashing 660 MB at every start would cost seconds.
//! The table comes from `docs/validation/ALICE-DATA-VALIDATION.md` in the port repository
//! (2011 PC release, hashed on 2026-10-11).
use std::path::Path;

/// The six archives the Vita build needs, with their exact sizes in bytes.
pub const REQUIRED_PACKS: [(&str, u64); 6] = [
    ("pak0.pk3", 289_276_327),
    ("pak1_large.pk3", 157_856_770),
    ("pak2.pk3", 102_269_403),
    ("pak3.pk3", 93_026_431),
    ("pak4_english.pk3", 49_731),
    ("pak5_mod.pk3", 18_133_775),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    Missing(&'static str),
    WrongSize { name: &'static str, expected: u64, found: u64 },
}

/// Returns one entry per required file that is absent, not a regular file, or of the wrong size.
/// An empty list means the folder holds the expected set.
pub fn check(base: &Path, required: &[(&'static str, u64)]) -> Vec<Problem> {
    required
        .iter()
        .filter_map(|&(name, expected)| match std::fs::metadata(base.join(name)) {
            Ok(meta) if meta.is_file() && meta.len() == expected => None,
            Ok(meta) if meta.is_file() => Some(Problem::WrongSize { name, expected, found: meta.len() }),
            _ => Some(Problem::Missing(name)),
        })
        .collect()
}

/// Only a missing file stops the start. A size that differs from the checked edition is reported
/// in the log and in the dialog when one is shown, but another edition of the game may be valid.
pub fn blocks_start(problems: &[Problem]) -> bool {
    problems.iter().any(|problem| matches!(problem, Problem::Missing(_)))
}

/// Text for the on-screen dialog. The system message dialog takes at most 512 bytes,
/// including the terminating NUL; the worst case (six wrong sizes) stays under 450.
pub fn message(base: &Path, problems: &[Problem]) -> String {
    let missing: Vec<&str> = problems
        .iter()
        .filter_map(|p| match p { Problem::Missing(name) => Some(*name), _ => None })
        .collect();
    let wrong: Vec<String> = problems
        .iter()
        .filter_map(|p| match p {
            Problem::WrongSize { name, expected, .. } => Some(format!("{name} (expected {expected} bytes)")),
            _ => None,
        })
        .collect();
    let mut text = format!(
        "Alice game data is missing or incomplete in {}/\n",
        base.display().to_string().trim_end_matches('/')
    );
    if !missing.is_empty() {
        text.push_str(&format!("Missing: {}\n", missing.join(", ")));
    }
    if !wrong.is_empty() {
        text.push_str(&format!("Wrong size: {}\n", wrong.join(", ")));
    }
    text.push_str("Copy the six .pk3 files from the base folder of your own copy of the game.");
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: [(&str, u64); 3] = [("a.pk3", 3), ("b.pk3", 5), ("c.pk3", 0)];

    fn folder(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lg-data-check-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn put(dir: &Path, name: &str, len: usize) {
        std::fs::write(dir.join(name), vec![0u8; len]).unwrap();
    }

    #[test]
    fn complete_folder_passes() {
        let dir = folder("complete");
        put(&dir, "a.pk3", 3);
        put(&dir, "b.pk3", 5);
        put(&dir, "c.pk3", 0);
        put(&dir, "extra.pk3", 9);
        assert!(check(&dir, &SMALL).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_folder_reports_every_file() {
        let dir = folder("absent").join("nothing-here");
        assert_eq!(
            check(&dir, &SMALL),
            vec![Problem::Missing("a.pk3"), Problem::Missing("b.pk3"), Problem::Missing("c.pk3")]
        );
    }

    #[test]
    fn wrong_size_and_missing_are_named() {
        let dir = folder("partial");
        put(&dir, "a.pk3", 4);
        put(&dir, "c.pk3", 0);
        let problems = check(&dir, &SMALL);
        assert_eq!(
            problems,
            vec![Problem::WrongSize { name: "a.pk3", expected: 3, found: 4 }, Problem::Missing("b.pk3")]
        );
        let text = message(Path::new("ux0:data/alice/base"), &problems);
        assert!(text.starts_with("Alice game data is missing or incomplete in ux0:data/alice/base/\n"));
        assert!(text.contains("Missing: b.pk3\n"));
        assert!(text.contains("Wrong size: a.pk3 (expected 3 bytes)\n"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_folder_with_the_pack_name_is_missing() {
        let dir = folder("dir");
        std::fs::create_dir_all(dir.join("b.pk3")).unwrap();
        put(&dir, "a.pk3", 3);
        put(&dir, "c.pk3", 0);
        assert_eq!(check(&dir, &SMALL), vec![Problem::Missing("b.pk3")]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn only_a_missing_file_blocks_the_start() {
        let wrong = Problem::WrongSize { name: "pak0.pk3", expected: 2, found: 1 };
        assert!(!blocks_start(&[]));
        assert!(!blocks_start(&[wrong.clone()]));
        assert!(blocks_start(&[wrong, Problem::Missing("pak2.pk3")]));
    }

    #[test]
    fn worst_case_message_fits_the_dialog() {
        let problems: Vec<Problem> = REQUIRED_PACKS
            .iter()
            .map(|&(name, expected)| Problem::WrongSize { name, expected, found: u64::MAX })
            .collect();
        let text = message(Path::new("ux0:data/alice/base"), &problems);
        assert!(text.len() < 450, "{} bytes", text.len());
    }

    #[test]
    fn host_copy_of_the_game_matches_the_table_when_present() {
        // Skipped where the private host data folder is absent; it checks the table, not the code.
        let Ok(home) = std::env::var("HOME") else { return };
        let dir = Path::new(&home).join("vita-work/alice-data/Alice1/bin/base");
        if dir.is_dir() {
            assert!(check(&dir, &REQUIRED_PACKS).is_empty(), "{}", dir.display());
        }
    }
}
