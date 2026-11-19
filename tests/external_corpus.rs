//! Import corpus: downloaded real-world files and files committed with the tree.
//!
//! `scripts/fetch-external-corpus.sh` downloads each remote entry of
//! `assets/external/corpus.tsv` into `assets/external/files/` and verifies its
//! SHA-256 before the manifest test runs; that test is ignored in the normal
//! suite because those files are not committed. The small fixtures under
//! `tests/fixtures/corpus` are committed and run with every ordinary test, so a
//! fresh checkout still imports each format and prints one line per file.
//!
//! The manifest has eight tab-separated columns: name, SHA-256, URL, format,
//! bytes, licence, tier and notes. Local fixtures use a `local:` URL and the
//! `smoke` tier; every other entry is downloaded on the scheduled corpus run.

use std::path::PathBuf;
use std::time::{Duration, Instant};

const COLUMNS: usize = 8;
const FORMATS: [&str; 6] = ["svg", "dxf", "hpgl", "plt", "pdf", "bwp"];
const TIERS: [&str; 3] = ["smoke", "full", "heavy"];
const LICENSES: [&str; 16] = [
    "MIT",
    "MIT OR Apache-2.0",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "CC0-1.0",
    "CC-BY-4.0",
    "CC-BY-SA-2.5",
    "CC-BY-SA-3.0",
    "CC-BY-SA-4.0",
    "AGPL-3.0",
    "PD",
    "W3C",
    "NOASSERTION",
    "verify",
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Lowest object counts seen for representative entries. They guard against an
/// importer change that silently drops geometry while the file still imports.
fn minimum_objects(name: &str) -> Option<usize> {
    Some(match name {
        "myst-guy.hpgl" | "ocs-lwpolyline.dxf" | "text-on-path.svg" | "hpgl-pd-pa.hpgl"
        | "hpgl-scaling.plt" | "spline_qcad.dxf" => 1,
        "eagle.hpgl"
        | "minimal-r12.dxf"
        | "r2018-hatch.dxf"
        | "mask-clip.svg"
        | "vector-only.pdf"
        | "leader-mleader.dxf"
        | "resvg-clip-path-on-child.svg"
        | "resvg-color-simple.svg"
        | "Type3WordSpacing.pdf" => 2,
        "hatch-in-fill.dxf"
        | "Embedded_font.pdf"
        | "XiaoBiaoSong.pdf"
        | "resvg-circle-simple.svg" => 3,
        "r2000-blocks.dxf" => 4,
        "stroke-mask.svg" | "mtext_columns_R2007.dxf" => 5,
        "viewport-frozen-layers.dxf" | "LWPOLYLINE-OCS.dxf" => 7,
        "Wikipedia_wordmark.svg" | "pattern-units.svg" => 10,
        "mtext-ocs-reduced.dxf" | "ZapfDingbats.pdf" => 6,
        "ocs2wcs3.dxf" => 8,
        "hatches_2.dxf" => 9,
        "mtext-inline.dxf" => 12,
        "annotation-stamp.pdf" | "svg-logo.svg" | "text-viewports.dxf" => 18,
        "symbols-labels.svg" | "VP4.dxf" => 22,
        "hp.hpgl" => 25,
        "oshw-logo.hpgl" => 26,
        "dimension-in-blocks.dxf" => 28,
        "wipeout-random.dxf" => 30,
        "ocs2wcs1.dxf" => 32,
        "ocs2wcs2.dxf" => 40,
        "s2.pdf" => 41,
        "xclip.dxf" => 45,
        "uncommon.dxf" => 47,
        "valid_lineweights.dxf" => 50,
        "thuluth.pdf" => 51,
        "columns-r2018.dxf" => 52,
        "caret-encoding.dxf" | "tektronix.hpgl" => 60,
        "columns-r2007.dxf" => 67,
        "form-160f.pdf" => 103,
        "escher-lizard.hpgl" => 127,
        "club-mate.hpgl" => 200,
        "text.dxf" => 217,
        "tamreview.pdf" => 270,
        "wipeout-door.dxf" => 278,
        "man-of-war.hpgl" => 296,
        "text-oblique.dxf" => 317,
        "space-shuttle.hpgl" => 226,
        "stacked-shx.dxf" | "stacked-text.dxf" => 439,
        "bricscad-insert.dxf" => 450,
        "world-map.hpgl" => 480,
        "pattern-019.svg" => 509,
        "resvg-gradient-default.svg" => 66,
        "text-mirror.dxf" => 672,
        "text-fonts.dxf" => 1344,
        "Cassini.hpgl" => 1400,
        "tracemonkey.pdf" => 1546,
        "world-map.svg" => 2338,
        "rca-pattern.svg" => 4773,
        "DSN-antenna.hpgl" => 2736,
        _ => return None,
    })
}

/// Files that must fail with a specific message instead of importing.
fn expected_rejection(name: &str) -> Option<&'static str> {
    match name {
        "dummy.pdf" | "text-only.pdf" => Some("no vector paths"),
        _ => None,
    }
}

struct Entry {
    name: String,
    path: PathBuf,
    hash: String,
    url: String,
    format: String,
    bytes: u64,
    license: String,
    tier: String,
    notes: String,
    local: bool,
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
            let fields: Vec<&str> = line.split('\t').collect();
            assert_eq!(
                fields.len(),
                COLUMNS,
                "the manifest needs {COLUMNS} columns: {line}"
            );
            let name = fields[0].to_owned();
            let url = fields[2].to_owned();
            let local = url.starts_with("local:");
            let path = if local {
                root().join(url.strip_prefix("local:").unwrap_or_default())
            } else {
                directory.join(&name)
            };
            Some(Entry {
                name,
                path,
                hash: fields[1].to_owned(),
                url,
                format: fields[3].to_owned(),
                bytes: fields[4].parse().expect("manifest bytes"),
                license: fields[5].to_owned(),
                tier: fields[6].to_owned(),
                notes: fields[7].to_owned(),
                local,
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
            if expected_rejection(entry.name.as_str())
                .is_some_and(|text| error.to_string().contains(text)) =>
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
    if let Some(minimum) = minimum_objects(entry.name.as_str()) {
        assert!(
            outcome.document.objects.len() >= minimum,
            "{} imported {} object(s), expected at least {minimum}",
            entry.name,
            outcome.document.objects.len()
        );
    }
    if matches!(extension, "svg" | "dxf" | "plt" | "hpgl" | "pdf") {
        assert!(
            !outcome.document.objects.is_empty(),
            "{} imported no objects",
            entry.name
        );
    }
}

fn check_manifest_entry(
    entry: &Entry,
    names: &mut std::collections::HashSet<String>,
    hashes: &mut std::collections::HashSet<String>,
) {
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
        hashes.insert(entry.hash.clone()),
        "duplicate corpus hash: {}",
        entry.name
    );
    assert!(
        entry.url.starts_with("https://") || entry.url.starts_with("local:tests/fixtures/corpus/"),
        "{}: unsupported source URL",
        entry.name
    );
    assert!(
        FORMATS.contains(&entry.format.as_str()),
        "{}: unknown format {}",
        entry.name,
        entry.format
    );
    assert_eq!(
        entry
            .name
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase(),
        entry.format,
        "{}: format does not match the extension",
        entry.name
    );
    assert!(entry.bytes > 0, "{}: bytes must be positive", entry.name);
    assert!(
        LICENSES.contains(&entry.license.as_str()),
        "{}: unknown licence {}",
        entry.name,
        entry.license
    );
    assert!(
        TIERS.contains(&entry.tier.as_str()),
        "{}: unknown tier {}",
        entry.name,
        entry.tier
    );
    assert!(
        !entry.notes.trim().is_empty(),
        "{}: notes are empty",
        entry.name
    );
    if entry.local {
        assert_eq!(
            entry.tier, "smoke",
            "{}: local fixtures are smoke",
            entry.name
        );
        assert!(
            entry.path.is_file(),
            "{}: local fixture is missing",
            entry.name
        );
    }
}

#[test]
fn the_corpus_manifest_is_well_formed() {
    let manifest = std::fs::read_to_string(root().join("assets/external/corpus.tsv"))
        .expect("read external corpus manifest");
    assert!(
        manifest
            .lines()
            .any(|line| line.trim() == "# manifest-version: 3"),
        "the manifest must declare version 3"
    );
    let entries = entries();
    assert!(
        entries.len() >= 15,
        "the external corpus shrank to {} file(s)",
        entries.len()
    );
    let mut names = std::collections::HashSet::new();
    let mut hashes = std::collections::HashSet::new();
    let mut local = 0usize;
    for entry in &entries {
        check_manifest_entry(entry, &mut names, &mut hashes);
        if entry.local {
            local += 1;
        }
    }
    assert!(
        local >= 11,
        "the local Tier A corpus shrank to {local} file(s)"
    );
}

#[test]
fn the_manifest_matches_the_files_on_disk() {
    let entries = entries();
    let local: Vec<&Entry> = entries.iter().filter(|entry| entry.local).collect();
    for entry in &local {
        let metadata = std::fs::metadata(&entry.path)
            .unwrap_or_else(|error| panic!("{}: {error}", entry.name));
        assert_eq!(
            metadata.len(),
            entry.bytes,
            "{}: manifest bytes do not match the file",
            entry.name
        );
    }

    let fixture_directory = root().join("tests/fixtures/corpus");
    let listing = std::fs::read_dir(&fixture_directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", fixture_directory.display()));
    let mut unlisted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for entry in listing.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if !local.iter().any(|entry| entry.path == path) {
            unlisted.insert(
                path.file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_string(),
            );
        }
    }
    assert!(
        unlisted.is_empty(),
        "tests/fixtures/corpus has files outside the manifest: {unlisted:?}"
    );

    let external = root().join("assets/external/files");
    if external.is_dir() {
        let listing = std::fs::read_dir(&external)
            .unwrap_or_else(|error| panic!("read {}: {error}", external.display()));
        let mut orphans: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for entry in listing.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string();
            if let Some(entry) = entries
                .iter()
                .find(|entry| entry.name == name && !entry.local)
            {
                assert_eq!(
                    path.metadata().map_or(0, |meta| meta.len()),
                    entry.bytes,
                    "{name}: manifest bytes do not match the file"
                );
            } else {
                orphans.insert(name);
            }
        }
        assert!(
            orphans.is_empty(),
            "assets/external/files has files outside the manifest: {orphans:?}"
        );
    }
}

#[test]
#[ignore = "downloads real-world files; run by the scheduled corpus workflow"]
fn the_external_corpus_imports() {
    let entries = entries();
    assert!(!entries.is_empty(), "the external corpus manifest is empty");
    for entry in &entries {
        if entry.local {
            continue;
        }
        assert_importable(entry);
    }
}

/// Files committed with the repository, so a fresh checkout still exercises
/// every importer without downloading the external manifest.
fn bundled_fixtures() -> Vec<PathBuf> {
    let directory = root().join("tests/fixtures/corpus");
    let listing = std::fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()));
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in listing.flatten() {
        let path = entry.path();
        if path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    files
}

#[test]
fn the_bundled_corpus_imports_and_reports_each_file() {
    let fixtures = bundled_fixtures();
    assert!(
        fixtures.len() >= 6,
        "the bundled corpus has only {} file(s)",
        fixtures.len()
    );
    for path in fixtures {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        let started = Instant::now();
        let imported = beamburin::import::format::import_with_report(&path);
        let elapsed = started.elapsed();
        if let Some(expected) = expected_rejection(name.as_str()) {
            let error = imported
                .err()
                .unwrap_or_else(|| panic!("{name}: expected a rejection"));
            assert!(
                error.to_string().contains(expected),
                "{name}: unexpected rejection: {error}"
            );
            println!("{name}: clearly rejected ({error}), {elapsed:?}");
            continue;
        }
        let outcome = imported.unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(
            outcome.document.width_mm.is_finite() && outcome.document.width_mm > 0.0,
            "{name}: invalid page width"
        );
        assert!(
            outcome.document.height_mm.is_finite() && outcome.document.height_mm > 0.0,
            "{name}: invalid page height"
        );
        assert!(
            !outcome.document.objects.is_empty(),
            "{name} imported no objects"
        );
        if let Some(minimum) = minimum_objects(name.as_str()) {
            assert!(
                outcome.document.objects.len() >= minimum,
                "{name} imported {} object(s), expected at least {minimum}",
                outcome.document.objects.len()
            );
        }
        assert!(elapsed < Duration::from_secs(10), "{name} took {elapsed:?}");
        println!(
            "{name}: {} object(s), {} layer(s), {} warning(s), {elapsed:?}",
            outcome.document.objects.len(),
            outcome.document.layers.len(),
            outcome.warnings.len()
        );
    }
}
