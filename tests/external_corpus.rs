//! Hash-pinned real-world files, imported in the scheduled corpus workflow.
//!
//! `scripts/fetch-external-corpus.sh` downloads each entry into
//! `assets/external/files/` and verifies its SHA-256 before this test runs. The
//! test is ignored in the normal suite because the files are not committed.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

struct Entry {
    name: String,
    path: PathBuf,
}

fn entries() -> Vec<Entry> {
    let manifest = std::fs::read_to_string(root().join("assets/external/corpus.tsv"))
        .expect("read external corpus manifest");
    let directory = root().join("assets/external/files");
    manifest
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let mut fields = line.split('\t');
            let name = fields.next()?.to_owned();
            let _hash = fields.next()?;
            let _url = fields.next()?;
            Some(Entry {
                path: directory.join(&name),
                name,
            })
        })
        .collect()
}

fn assert_importable(entry: &Entry) {
    assert!(
        entry.path.is_file(),
        "{} is missing; run scripts/fetch-external-corpus.sh",
        entry.name
    );
    let started = Instant::now();
    let outcome = beamburin::import::format::import_with_report(&entry.path)
        .unwrap_or_else(|error| panic!("{}: {error}", entry.name));
    let elapsed = started.elapsed();
    assert!(
        outcome.document.width_mm.is_finite() && outcome.document.width_mm > 0.0,
        "{}: invalid page width",
        entry.name
    );
    assert!(
        outcome.document.height_mm.is_finite() && outcome.document.height_mm > 0.0,
        "{}: invalid page height",
        entry.name
    );
    assert!(
        elapsed < Duration::from_secs(30),
        "{} took {elapsed:?}",
        entry.name
    );
    println!(
        "{}: {} object(s), {} layer(s), {} warning(s), {elapsed:?}",
        entry.name,
        outcome.document.objects.len(),
        outcome.document.layers.len(),
        outcome.warnings.len()
    );
    if entry.name.ends_with(".svg") || entry.name.ends_with(".dxf") {
        assert!(
            !outcome.document.objects.is_empty(),
            "{} imported no objects",
            entry.name
        );
    }
}

#[test]
#[ignore = "downloads real-world files; run by the scheduled corpus workflow"]
fn the_external_corpus_imports() {
    let entries = entries();
    assert!(!entries.is_empty(), "the external corpus manifest is empty");
    for entry in &entries {
        assert_importable(entry);
    }
}
