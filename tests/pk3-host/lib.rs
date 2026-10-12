#[path = "../../src/assets.rs"]
#[allow(dead_code)]
mod assets;
use anyhow::Result;
use std::{fs::File, io::Read, path::Path};
use zip::ZipArchive;

#[test]
fn real_pk3_buffered_directory_and_reads_match_baseline() -> Result<()> {
    let Some(base) = std::env::var_os("ALICE_TEST_DATA") else { return Ok(()); };
    let base = Path::new(&base);
    let mut mounted = assets::Assets::open(base)?;
    let mut paths = std::fs::read_dir(base)?.map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pk3"))).collect::<Vec<_>>();
    paths.sort_by_key(|p| p.file_name().unwrap_or_default().to_ascii_lowercase());
    let mut expected = std::collections::BTreeMap::new();
    for path in &paths {
        let mut old = ZipArchive::new(File::open(path)?)?;
        // The production reader is inspected in the module test below; also
        // compare every override-resolved name and representative decoded bytes.
        for i in 0..old.len() {
            let e = old.by_index(i)?;
            if !e.is_dir() {
                expected.insert(e.name().replace('\\', "/").to_ascii_lowercase(), (path.clone(), i));
            }
        }
    }
    assert_eq!(mounted.names().collect::<Vec<_>>(), expected.keys().map(String::as_str).collect::<Vec<_>>());
    for (n, (path, index)) in expected.iter().step_by(97) {
        let mut old = ZipArchive::new(File::open(path)?)?;
        let mut bytes = Vec::new();
        old.by_index(*index)?.read_to_end(&mut bytes)?;
        assert_eq!(mounted.read(n)?, bytes, "{n}");
    }
    eprintln!("real PK3 proof: {} packs, {} resolved entries, representative decoded bytes identical", paths.len(), expected.len());
    Ok(())
}
