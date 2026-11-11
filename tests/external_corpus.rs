//! Hash-pinned real-world files, imported in the scheduled corpus workflow.
//!
//! `scripts/fetch-external-corpus.sh` downloads each entry into
//! `assets/external/files/` and verifies its SHA-256 before this test runs. The
//! test is ignored in the normal suite because the files are not committed.

use std::path::PathBuf;
use std::time::{Duration, Instant};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Lowest object counts seen for representative entries. They guard against an
/// importer change that silently drops geometry while the file still imports.
fn minimum_objects(name: &str) -> Option<usize> {
    Some(match name {
        "bricscad-insert.dxf" => 450,
        "caret-encoding.dxf" => 60,
        "club-mate.hpgl" => 200,
        "columns-r2007.dxf" => 67,
        "columns-r2018.dxf" => 52,
        "dimension-in-blocks.dxf" => 28,
        "form-160f.pdf" => 103,
        "escher-lizard.hpgl" => 127,
        "man-of-war.hpgl" => 296,
        "mtext-inline.dxf" => 12,
        "stacked-shx.dxf" | "stacked-text.dxf" => 439,
        "tamreview.pdf" => 270,
        "thuluth.pdf" => 51,
        "text-fonts.dxf" => 1344,
        "text-oblique.dxf" => 317,
        "pattern-019.svg" => 509,
        "rca-pattern.svg" => 4773,
        "s2.pdf" => 41,
        "stroke-mask.svg" => 5,
        "symbols-labels.svg" => 22,
        "text-mirror.dxf" => 672,
        "tracemonkey.pdf" => 1546,
        "svg-logo.svg" | "text-viewports.dxf" => 18,
        "text.dxf" => 217,
        "uncommon.dxf" => 47,
        "wipeout-door.dxf" => 278,
        "viewport-frozen-layers.dxf" => 7,
        "wipeout-random.dxf" => 30,
        "world-map.svg" => 2338,
        "world-map.hpgl" => 480,
        _ => return None,
    })
}

struct Entry {
    name: String,
    path: PathBuf,
    hash: String,
    url: String,
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
            let hash = fields.next()?.to_owned();
            let url = fields.next()?.to_owned();
            Some(Entry {
                path: directory.join(&name),
                name,
                hash,
                url,
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
    let extension = entry
        .path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let started = Instant::now();
    let outcome = match beamburin::import::format::import_with_report(&entry.path) {
        Ok(outcome) => outcome,
        Err(error)
            if extension.eq_ignore_ascii_case("pdf")
                && error.to_string().contains("no vector paths") =>
        {
            println!("{}: clearly rejected ({error})", entry.name);
            return;
        }
        Err(error) => panic!("{}: {error}", entry.name),
    };
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
    if let Some(minimum) = minimum_objects(&entry.name) {
        assert!(
            outcome.document.objects.len() >= minimum,
            "{} imported {} object(s), expected at least {minimum}",
            entry.name,
            outcome.document.objects.len()
        );
    }
    if extension.eq_ignore_ascii_case("svg")
        || extension.eq_ignore_ascii_case("dxf")
        || extension.eq_ignore_ascii_case("plt")
        || extension.eq_ignore_ascii_case("hpgl")
        || extension.eq_ignore_ascii_case("pdf")
    {
        assert!(
            !outcome.document.objects.is_empty(),
            "{} imported no objects",
            entry.name
        );
    }
}

#[test]
fn the_corpus_manifest_is_well_formed() {
    let entries = entries();
    assert!(
        entries.len() >= 15,
        "the external corpus shrank to {} file(s)",
        entries.len()
    );
    let mut names = std::collections::HashSet::new();
    let mut hashes = std::collections::HashSet::new();
    for entry in &entries {
        assert!(
            names.insert(entry.name.clone()),
            "duplicate corpus name: {}",
            entry.name
        );
        assert_eq!(
            entry.hash.len(),
            64,
            "{}: the hash is not a SHA-256 digest",
            entry.name
        );
        assert!(
            entry
                .hash
                .chars()
                .all(|character| character.is_ascii_hexdigit()),
            "{}: the hash contains non-hexadecimal characters",
            entry.name
        );
        assert!(
            entry.url.starts_with("https://"),
            "{}: the source URL is not HTTPS",
            entry.name
        );
        assert!(
            hashes.insert(entry.hash.clone()),
            "duplicate corpus hash: {}",
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
