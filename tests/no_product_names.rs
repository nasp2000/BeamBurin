//! Keep names of commercial laser-software products out of the tree.
//!
//! The denylist is stored as bytes so this test does not itself add the names
//! it is meant to catch.

use std::path::Path;

const DENY: &[&[u8]] = &[
    &[108, 105, 103, 104, 116, 98, 117, 114, 110],
    &[114, 100, 119, 111, 114, 107, 115],
    &[101, 122, 99, 97, 100],
    &[108, 97, 115, 101, 114, 99, 117, 116],
    &[108, 97, 115, 101, 114, 119, 101, 98],
    &[103, 108, 111, 119, 102, 111, 114, 103, 101],
    &[120, 116, 111, 111, 108],
    &[111, 114, 116, 117, 114],
    &[116, 114, 111, 99, 101, 110],
    &[114, 117, 105, 100, 97],
];

fn text_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some(
            "rs" | "md"
                | "toml"
                | "yml"
                | "yaml"
                | "txt"
                | "svg"
                | "dxf"
                | "hpgl"
                | "json"
                | "html"
                | "css"
                | "sh"
                | "py"
        )
    )
}

fn walk(dir: &Path, hits: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name.as_ref() == "target" || name.as_ref() == ".git" {
                continue;
            }
            walk(&path, hits);
        } else if text_file(&path) {
            let Ok(content) = std::fs::read(&path) else {
                continue;
            };
            let lower = String::from_utf8_lossy(&content).to_ascii_lowercase();
            for needle in DENY {
                let needle = String::from_utf8_lossy(*needle);
                if lower.contains(needle.as_ref()) {
                    hits.push(format!("{}: {}", path.display(), needle));
                }
            }
        }
    }
}

#[test]
fn no_commercial_product_names_in_the_tree() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut hits = Vec::new();
    walk(root, &mut hits);
    assert!(hits.is_empty(), "commercial product names found: {hits:#?}");
}
