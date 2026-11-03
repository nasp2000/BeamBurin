//! The shipped notices must name every locked crate and the bundled fonts.

use std::collections::BTreeSet;
use std::path::Path;

fn locked_crates() -> BTreeSet<String> {
    let lock = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock"))
        .expect("cannot read Cargo.lock");
    let mut names = BTreeSet::new();
    let mut in_package = false;
    for line in lock.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            in_package = true;
        } else if in_package {
            if let Some(rest) = line.strip_prefix("name = \"") {
                if let Some(name) = rest.strip_suffix('"') {
                    names.insert(name.to_string());
                }
                in_package = false;
            }
        }
    }
    names
}

#[test]
fn every_locked_crate_is_named_in_the_notices() {
    let notices = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("THIRD_PARTY_NOTICES.md"),
    )
    .expect("cannot read THIRD_PARTY_NOTICES.md");
    for name in locked_crates() {
        assert!(
            notices.contains(&format!("`{name}`")),
            "{name} is missing from THIRD_PARTY_NOTICES.md; run scripts/third_party_notices.py"
        );
    }
}

#[test]
fn the_notices_cover_the_bundled_fonts_and_standard_licences() {
    let notices = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("THIRD_PARTY_NOTICES.md"),
    )
    .expect("cannot read THIRD_PARTY_NOTICES.md");
    for marker in [
        "DejaVu Sans",
        "Hack",
        "Noto Emoji",
        "Ubuntu Light",
        "Emoji Icon Font",
        "SIL OPEN FONT LICENSE",
        "UBUNTU FONT LICENCE",
        "Apache License",
        "MIT License",
        "BSD 2-Clause",
        "BSD 3-Clause",
        "ISC",
        "zlib",
        "Boost Software License",
        "Unicode License 3.0",
    ] {
        assert!(notices.contains(marker), "the notices are missing {marker}");
    }
}
