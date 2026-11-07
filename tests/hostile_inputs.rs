//! Every public text/byte parser must reject damaged input without panicking.
//!
//! The matrix covers empty files, truncation, hostile dimensions, deeply nested
//! SVG, extreme numbers, malformed PDF objects and a deterministic stream of
//! random bytes. The assertions are deliberately only "does not panic": the
//! parsers may reject an input or return a best-effort document, but the
//! process must survive either way.

use std::fmt::Write as _;

use beamburin::import::{dxf, hpgl, pdf, svg};

fn sample_svg() -> String {
    let mut path = String::from("M0 0");
    for index in 1..=5000 {
        let _ = write!(path, " L{index} {}", index % 37);
    }
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" viewBox="0 0 100 100"><path d="{path}"/></svg>"#
    )
}

fn sample_dxf() -> String {
    let mut entities = String::from("0\nSECTION\n2\nENTITIES\n");
    for index in 0..2000 {
        let _ = write!(
            entities,
            "0\nLINE\n10\n{index}\n20\n0\n11\n{}\n21\n{}\n",
            index + 1,
            index % 17
        );
    }
    entities.push_str("0\nENDSEC\n0\nEOF\n");
    entities
}

fn sample_gcode() -> String {
    let mut program = String::new();
    for index in 0..20000 {
        program.push_str(&format!(
            "G1 X{} Y{} S{}\n",
            index % 300,
            index % 200,
            index % 1000
        ));
    }
    program
}

#[test]
fn svg_inputs_never_panic() {
    let sample = sample_svg();
    let nested = format!(
        r#"<svg width="10" height="10">{}{}</svg>"#,
        "<g>".repeat(300),
        "</g>".repeat(300)
    );
    let inputs = [
        "",
        "<svg",
        "<svg></svg>",
        "<svg width=\"NaN\" height=\"1e309\"><rect width=\"1\" height=\"1\"/></svg>",
        "<svg width=\"10\" height=\"10\"><path d=\"M0 0 L1e309 1e309\"/></svg>",
        "<svg width=\"10\" height=\"10\"><use href=\"#missing\"/></svg>",
        "<svg width=\"10\" height=\"10\"><image href=\"file:///missing.png\"/></svg>",
        nested.as_str(),
        sample.as_str(),
    ];
    for input in inputs {
        let _ = svg::import_svg_report(input);
    }
}

#[test]
fn dxf_inputs_never_panic() {
    let sample = sample_dxf();
    let truncated = "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n90\n5000\n10\n0\n20\n0\n";
    let wrong_types = "0\nSECTION\n2\nENTITIES\n0\nLINE\n10\nnot-a-number\n20\nNaN\n11\n1e309\n21\n-inf\n0\nENDSEC\n0\nEOF\n";
    let huge_count = "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n90\n100000\n0\nENDSEC\n0\nEOF\n";
    let inputs = [
        "",
        "0",
        "0\nSECTION\n2\nENTITIES\n",
        truncated,
        wrong_types,
        huge_count,
        sample.as_str(),
    ];
    for input in inputs {
        let _ = dxf::import_dxf_report(input);
    }
}

#[test]
fn hpgl_inputs_never_panic() {
    let inputs = [
        "",
        "IN;PA;SP1;",
        "IN;PA;SP1;PU0,0;PD",
        "IN;PA;SP1;PU999999999999999999,0;PD0,0;",
        "IN;PA;SP1;PU1e309,-1e309;PDNaN,inf;",
        "IN;PA;SP1;PD0,0,1,1,2,2;",
    ];
    for input in inputs {
        let _ = hpgl::import_hpgl_report(input);
    }
}

#[test]
fn pdf_inputs_never_panic() {
    let inputs: [&[u8]; 7] = [
        b"",
        b"%PDF-1.7\n",
        b"%PDF-1.7\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
        b"%PDF-1.7\n1 0 obj\n<< /Length 999999999 >>\nstream\n",
        b"%PDF-1.7\n1 0 obj\n<< /Type /Page /MediaBox [0 0 1e309 1e309] >>\nendobj\n",
        b"not a pdf at all",
        b"%PDF-\xff\xfe\x00\x01",
    ];
    for input in inputs {
        let _ = pdf::import_pdf_report(input);
    }
}

#[test]
fn gcode_inputs_never_panic() {
    let repeated = "G1 X0 Y0\n".repeat(1000);
    let sample = sample_gcode();
    let inputs = [
        "",
        "G1 X",
        "G0 X1e309 YNaN",
        "G2 I J K",
        repeated.as_str(),
        sample.as_str(),
    ];
    for input in inputs {
        let _ = beamburin::gcode::external::parse(input);
    }
}

#[test]
fn project_json_inputs_never_panic() {
    let inputs = [
        "",
        "{}",
        "null",
        "[]",
        r#"{"format_version": 999999}"#,
        r#"{"format_version": "not a number"}"#,
        r#"{"layers": [{"id": 1}], "objects": [null]}"#,
        r#"{"format_version": 1, "objects": [{"id": "x", "shape": {"Path": {"segments": []}}}]}"#,
    ];
    for input in inputs {
        let _ = beamburin::document::serialization::from_json(input);
    }
}

#[test]
fn deterministic_random_bytes_never_panic() {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    for length in [0, 1, 7, 31, 255, 1024, 4096] {
        for _ in 0..20 {
            let mut bytes = Vec::with_capacity(length);
            for _ in 0..length {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                bytes.push((state & 0xff) as u8);
            }
            let _ = pdf::import_pdf_report(&bytes);
            if let Ok(text) = std::str::from_utf8(&bytes) {
                let _ = svg::import_svg_report(text);
                let _ = dxf::import_dxf_report(text);
                let _ = hpgl::import_hpgl_report(text);
                let _ = beamburin::gcode::external::parse(text);
                let _ = beamburin::document::serialization::from_json(text);
            }
        }
    }
}
