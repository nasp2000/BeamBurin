//! Deterministic property checks over generated drawings.
//!
//! A small xorshift generator builds drawings with random rectangles and
//! ellipses, random transforms and random page sizes. Each case must survive
//! the project JSON round-trip and the SVG export/import round-trip with the
//! same object count and bounds. This is intentionally deterministic: a
//! failure prints the seed and can be reproduced without a proptest runner.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use beamburin::document::{Document, Object, Shape};
use beamburin::geometry::Transform2;
use std::f64::consts::TAU;

fn unit(state: &mut u64) -> f64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 11) as f64 / ((1_u64 << 53) as f64)
}

fn between(state: &mut u64, low: f64, high: f64) -> f64 {
    low + (high - low) * unit(state)
}

fn drawing(state: &mut u64) -> Document {
    let width = between(state, 80.0, 300.0);
    let height = between(state, 80.0, 300.0);
    let mut document = Document {
        width_mm: width,
        height_mm: height,
        ..Document::default()
    };
    let layer_id = document.layers[0].id.clone();
    let count = 1 + (unit(state) * 6.0) as usize;
    for _ in 0..count {
        let shape = if unit(state) < 0.5 {
            Shape::Rect {
                width: between(state, 0.5, 40.0),
                height: between(state, 0.5, 40.0),
                corner_radius: if unit(state) < 0.3 {
                    between(state, 0.0, 5.0)
                } else {
                    0.0
                },
            }
        } else {
            Shape::Ellipse {
                rx: between(state, 0.5, 25.0),
                ry: between(state, 0.5, 25.0),
            }
        };
        let transform = Transform2::translate(
            between(state, -20.0, width + 20.0),
            between(state, -20.0, height + 20.0),
        )
        .compose(&Transform2::rotate(between(state, 0.0, TAU)))
        .compose(&Transform2::scale(
            between(state, 0.2, 2.5),
            between(state, 0.2, 2.5),
        ));
        document.add_object(Object::new(layer_id.clone(), shape).with_transform(transform));
    }
    document
}

fn compare(left: &Document, right: &Document, label: &str) {
    assert_eq!(
        right.objects.len(),
        left.objects.len(),
        "{label}: object count"
    );
    for (index, (a, b)) in left.objects.iter().zip(&right.objects).enumerate() {
        let (a_min, a_max) = a.world_path().bounding_box().expect("left bounds");
        let (b_min, b_max) = b.world_path().bounding_box().expect("right bounds");
        for (name, left, right) in [
            ("min x", a_min.x, b_min.x),
            ("min y", a_min.y, b_min.y),
            ("max x", a_max.x, b_max.x),
            ("max y", a_max.y, b_max.y),
        ] {
            let tolerance = 1e-3_f64.max(1e-6 * left.abs().max(right.abs()));
            assert!(
                (left - right).abs() <= tolerance,
                "{label}: object {index} {name}: {left} vs {right} (tolerance {tolerance})"
            );
        }
    }
}

#[test]
fn generated_drawings_survive_json_and_svg_round_trips() {
    let mut state = 0x5EED_1234_ABCD_9876_u64;
    for case in 0..200 {
        let document = drawing(&mut state);
        let json = beamburin::document::serialization::to_json(&document)
            .unwrap_or_else(|error| panic!("case {case}: JSON write: {error}"));
        let from_json = beamburin::document::serialization::from_json(&json)
            .unwrap_or_else(|error| panic!("case {case}: JSON read: {error}"));
        compare(&document, &from_json, &format!("case {case} JSON"));

        let svg = beamburin::export::export_svg(&document)
            .unwrap_or_else(|error| panic!("case {case}: SVG export: {error}"));
        let imported = beamburin::import::svg::import_svg(&svg)
            .unwrap_or_else(|error| panic!("case {case}: SVG import: {error}"));
        assert!(
            (imported.width_mm - document.width_mm).abs() < 1e-3,
            "case {case}: page width"
        );
        assert!(
            (imported.height_mm - document.height_mm).abs() < 1e-3,
            "case {case}: page height"
        );
        compare(&document, &imported, &format!("case {case} SVG"));
    }
}
