//! Format corpus: import the samples, export them and compare against goldens.
//!
//! The expected files live in `assets/examples/expected`. Regenerate them
//! deliberately with `CORPUS_UPDATE=1 cargo test --test corpus` and review the
//! diff before committing.

use std::path::{Path, PathBuf};

use beamwright::document::{Document, Shape};
use beamwright::geometry::Transform2;
use beamwright::operations::{Operation, OperationParams, OperationType};

fn samples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/examples")
}

fn expected(name: &str) -> PathBuf {
    samples().join("expected").join(name)
}

/// Replace generated UUIDs so the golden only records geometry and settings.
fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        if text.is_char_boundary(index + 36) {
            let candidate = &text[index..index + 36];
            if is_uuid(candidate) {
                out.push_str("<uuid>");
                index += 36;
                continue;
            }
        }
        let character = text[index..].chars().next().unwrap();
        out.push(character);
        index += character.len_utf8();
    }
    out
}

fn is_uuid(candidate: &str) -> bool {
    let bytes = candidate.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (position, byte) in bytes.iter().enumerate() {
        let separator = matches!(position, 8 | 13 | 18 | 23);
        if separator {
            if *byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn check(name: &str, actual: &str) {
    let actual = normalize(actual).replace("\r\n", "\n");
    let path = expected(name);
    if std::env::var_os("CORPUS_UPDATE").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &actual).unwrap();
        return;
    }
    let golden = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| {
            panic!(
                "missing golden {} ({error}); run CORPUS_UPDATE=1 cargo test --test corpus",
                path.display()
            )
        })
        .replace("\r\n", "\n");
    assert_eq!(actual, golden, "corpus output changed for {name}");
}

fn import(name: &str) -> Document {
    let path = samples().join(name);
    let outcome = beamwright::import::format::import_with_report(&path)
        .unwrap_or_else(|error| panic!("cannot import {name}: {error}"));
    outcome.document
}

fn with_cut(mut document: Document) -> Document {
    document.operations.push(Operation {
        id: "corpus-cut".to_string(),
        name: "Corpus cut".to_string(),
        op_type: OperationType::OutlineCutting,
        params: OperationParams {
            speed: 1200.0,
            power: 40.0,
            passes: 1,
            ..OperationParams::default()
        },
        enabled: true,
        depends_on: Vec::new(),
    });
    for object in &mut document.objects {
        object.operation_id = Some("corpus-cut".to_string());
    }
    document
}

#[test]
fn the_svg_sample_imports_and_exports_stably() {
    let document = import("corpus_shapes.svg");
    assert!(
        document.objects.len() >= 3,
        "objects: {}",
        document.objects.len()
    );
    let svg = beamwright::export::export_svg(&document).unwrap();
    check("shapes.svg", &svg);
}

#[test]
fn the_dxf_sample_imports_and_exports_stably() {
    let document = import("corpus_parts.dxf");
    assert_ne!(document.objects.len(), 0);
    let dxf = beamwright::export::export_dxf(&document).unwrap();
    check("parts.dxf", &dxf);
}

#[test]
fn the_hpgl_sample_imports_and_exports_stably() {
    let document = import("corpus_plot.hpgl");
    assert_ne!(document.objects.len(), 0);
    let svg = beamwright::export::export_svg(&document).unwrap();
    check("plot.svg", &svg);
}

#[test]
fn the_svg_sample_produces_stable_gcode() {
    let document = with_cut(import("corpus_shapes.svg"));
    let (text, report) =
        beamwright::gcode::render_job(&document, &beamwright::gcode::JobOptions::default())
            .unwrap();
    assert!(report.lines > 0);
    check("shapes.nc", &text);
}

#[test]
fn the_exports_stay_inside_the_page() {
    for name in ["corpus_shapes.svg", "corpus_parts.dxf", "corpus_plot.hpgl"] {
        let document = import(name);
        let page = document.width_mm * document.height_mm;
        assert!(page > 0.0);
        for object in &document.objects {
            let bounds = object
                .bounds()
                .unwrap_or_else(|| panic!("{name}: object without bounds"));
            assert!(
                bounds.0.x.is_finite() && bounds.1.y.is_finite(),
                "{name}: non-finite bounds"
            );
        }
    }
}

#[test]
fn a_freehand_square_round_trips_through_svg() {
    // A document built in code, so the corpus also covers export without import.
    let mut document = Document::new("Corpus", 100.0, 100.0);
    let layer_id = document.layers[0].id.clone();
    let mut object = beamwright::document::Object::new(
        layer_id,
        Shape::Path(beamwright::geometry::Path::from_points(
            &[(10.0, 10.0), (50.0, 10.0), (50.0, 40.0), (10.0, 40.0)],
            true,
        )),
    );
    object.transform = Transform2::identity();
    document.add_object(object);
    let svg = beamwright::export::export_svg(&document).unwrap();
    check("square.svg", &svg);
}
