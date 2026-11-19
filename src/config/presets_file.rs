//! `.bwp-presets` files: strict, versioned transport for export presets.
//!
//! One file holds one family (G-code, image or vector). Imports are validated
//! completely before the preferences are touched and exports are written
//! atomically.

use std::collections::HashSet;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use super::preferences::{
    ExportPreset, ExportPresetPrefs, ImageExportPreset, ImageExportPresetPrefs, MAX_EXPORT_PRESETS,
    VectorExportPreset, VectorExportPresetPrefs,
};

pub const SCHEMA: &str = "beamburin-presets";
pub const VERSION: u32 = 1;
pub const MAX_PRESET_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PresetKind {
    #[default]
    Gcode,
    Image,
    Vector,
}

impl PresetKind {
    pub fn id(self) -> &'static str {
        match self {
            PresetKind::Gcode => "gcode",
            PresetKind::Image => "image",
            PresetKind::Vector => "vector",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PresetKind::Gcode => "G-code",
            PresetKind::Image => "image",
            PresetKind::Vector => "vector",
        }
    }

    pub fn from_id(value: &str) -> Option<Self> {
        match value {
            "gcode" => Some(PresetKind::Gcode),
            "image" => Some(PresetKind::Image),
            "vector" => Some(PresetKind::Vector),
            _ => None,
        }
    }

    fn allowed_fields(self) -> &'static [&'static str] {
        match self {
            PresetKind::Gcode => &[
                "name",
                "dialect",
                "decimals",
                "comments",
                "travel",
                "crlf",
                "smooth_mm",
                "arcs",
                "origin",
            ],
            PresetKind::Image => &[
                "name",
                "dpi",
                "dpi_custom",
                "transparent",
                "margin_mm",
                "offset_x_mm",
                "offset_y_mm",
                "custom_size",
                "width_px",
                "height_px",
                "keep_aspect",
                "drawing",
                "selection",
                "allow_outside_page",
            ],
            PresetKind::Vector => &[
                "name",
                "format",
                "text_as_paths",
                "arcs",
                "margin_mm",
                "dxf_unit",
                "embed_images",
                "area",
                "layers",
                "allow_outside_page",
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConflictPolicy {
    #[default]
    Skip,
    Replace,
    KeepBoth,
}

impl ConflictPolicy {
    pub fn label(self) -> &'static str {
        match self {
            ConflictPolicy::Skip => "Skip",
            ConflictPolicy::Replace => "Replace",
            ConflictPolicy::KeepBoth => "Keep both",
        }
    }

    pub fn all() -> [ConflictPolicy; 3] {
        [
            ConflictPolicy::Skip,
            ConflictPolicy::Replace,
            ConflictPolicy::KeepBoth,
        ]
    }
}

/// One validated preset file, still as JSON values for a lossless round trip.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetFile {
    pub kind: PresetKind,
    pub presets: Vec<Value>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetPreview {
    pub kind: PresetKind,
    pub names: Vec<String>,
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportOutcome {
    pub imported: usize,
    pub replaced: usize,
    pub skipped: usize,
    pub renamed: Vec<(String, String)>,
    pub warnings: Vec<String>,
}

impl ImportOutcome {
    pub fn applied(&self) -> usize {
        self.imported + self.replaced
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportOutcome {
    pub count: usize,
    pub warnings: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema: String,
    version: u32,
    kind: String,
    presets: Vec<Value>,
}

/// Read and validate a preset file for one family.
pub fn read_file(path: &Path, expected: PresetKind) -> Result<PresetFile, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("Cannot read the preset file: {error}."))?;
    if metadata.len() > MAX_PRESET_FILE_BYTES {
        return Err("Preset file is larger than 1 MiB.".to_string());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("Cannot read the preset file: {error}."))?;
    let envelope: Envelope = serde_json::from_str(&text)
        .map_err(|error| format!("The preset file is not valid JSON: {error}."))?;
    if envelope.schema != SCHEMA {
        return Err("This is not a BeamBurin preset file.".to_string());
    }
    if envelope.version != VERSION {
        return Err(format!(
            "Preset file version {} is not supported; this build reads version {VERSION}.",
            envelope.version
        ));
    }
    let Some(kind) = PresetKind::from_id(&envelope.kind) else {
        return Err("This is not a BeamBurin preset file.".to_string());
    };
    if kind != expected {
        return Err(format!(
            "The file holds {} presets, not {} presets.",
            kind.label(),
            expected.label()
        ));
    }
    if envelope.presets.is_empty() {
        return Err("The preset file contains no presets.".to_string());
    }
    if envelope.presets.len() > MAX_EXPORT_PRESETS {
        return Err(format!(
            "The preset file has {} presets; the limit is {MAX_EXPORT_PRESETS}.",
            envelope.presets.len()
        ));
    }

    let mut warnings = Vec::new();
    let mut names: HashSet<String> = HashSet::new();
    let mut presets = Vec::with_capacity(envelope.presets.len());
    for (index, value) in envelope.presets.into_iter().enumerate() {
        let number = index + 1;
        let object = value
            .as_object()
            .ok_or_else(|| format!("Preset {number} is not an object."))?;
        for key in object.keys() {
            if !kind.allowed_fields().contains(&key.as_str()) {
                return Err(format!("Preset {number} has an unknown field \"{key}\"."));
            }
        }
        let (name, mut value) = match kind {
            PresetKind::Gcode => {
                let preset: ExportPreset = serde_json::from_value(value.clone())
                    .map_err(|error| format!("Preset {number} is invalid: {error}."))?;
                preset
                    .validate()
                    .map_err(|reason| invalid_preset(number, &preset.name, &reason))?;
                (preset.name.trim().to_string(), value)
            }
            PresetKind::Image => {
                let preset: ImageExportPreset = serde_json::from_value(value.clone())
                    .map_err(|error| format!("Preset {number} is invalid: {error}."))?;
                preset
                    .validate()
                    .map_err(|reason| invalid_preset(number, &preset.name, &reason))?;
                (preset.name.trim().to_string(), value)
            }
            PresetKind::Vector => {
                let mut preset: VectorExportPreset = serde_json::from_value(value.clone())
                    .map_err(|error| format!("Preset {number} is invalid: {error}."))?;
                preset
                    .validate()
                    .map_err(|reason| invalid_preset(number, &preset.name, &reason))?;
                if !preset.layers.is_empty() {
                    preset.layers.clear();
                    warnings.push(
                        "Layer selections stay with the document and are not written to preset \
                         files."
                            .to_string(),
                    );
                    value = serde_json::to_value(&preset)
                        .map_err(|error| format!("Preset {number} is invalid: {error}."))?;
                }
                (preset.name.trim().to_string(), value)
            }
        };
        if name.is_empty() {
            return Err(format!(
                "Preset {number} (\"{name}\") is invalid: a preset needs a name."
            ));
        }
        if !names.insert(name.clone()) {
            return Err(format!("The preset file repeats the name \"{name}\"."));
        }
        presets.push(value);
    }
    warnings.dedup();
    Ok(PresetFile {
        kind,
        presets,
        warnings,
    })
}

fn invalid_preset(number: usize, name: &str, reason: &str) -> String {
    format!("Preset {number} (\"{name}\") is invalid: {reason}")
}

/// Names and conflicts for the import preview.
pub fn preview(file: &PresetFile, existing_names: &[String]) -> PresetPreview {
    let names: Vec<String> = file
        .presets
        .iter()
        .filter_map(|value| value.get("name").and_then(Value::as_str))
        .map(|name| name.trim().to_string())
        .collect();
    let conflicts = names
        .iter()
        .filter(|name| existing_names.iter().any(|existing| existing == *name))
        .cloned()
        .collect();
    PresetPreview {
        kind: file.kind,
        names,
        conflicts,
    }
}

pub fn gcode_names(prefs: &ExportPresetPrefs) -> Vec<String> {
    prefs
        .saved
        .iter()
        .map(|preset| preset.name.clone())
        .collect()
}

pub fn image_names(prefs: &ImageExportPresetPrefs) -> Vec<String> {
    prefs
        .saved
        .iter()
        .map(|preset| preset.name.clone())
        .collect()
}

pub fn vector_names(prefs: &VectorExportPresetPrefs) -> Vec<String> {
    prefs
        .saved
        .iter()
        .map(|preset| preset.name.clone())
        .collect()
}

pub fn merge_gcode(
    existing: &ExportPresetPrefs,
    file: &PresetFile,
    policy: ConflictPolicy,
) -> Result<(ExportPresetPrefs, ImportOutcome), String> {
    if file.kind != PresetKind::Gcode {
        return Err(kind_mismatch(file.kind, PresetKind::Gcode));
    }
    let presets: Vec<ExportPreset> = file
        .presets
        .iter()
        .map(|value| serde_json::from_value(value.clone()))
        .collect::<std::result::Result<_, _>>()
        .map_err(|error| format!("The preset file is not valid: {error}."))?;
    let names: Vec<String> = presets
        .iter()
        .map(|preset| preset.name.trim().to_string())
        .collect();
    let targets = plan_names(&gcode_names(existing), &names, policy, PresetKind::Gcode)?;
    let mut merged = existing.clone();
    let mut outcome = ImportOutcome {
        warnings: file.warnings.clone(),
        ..ImportOutcome::default()
    };
    for (mut preset, target) in presets.into_iter().zip(targets) {
        let original = preset.name.trim().to_string();
        let Some(target) = target else {
            outcome.skipped += 1;
            continue;
        };
        if target != original {
            outcome.renamed.push((original, target.clone()));
        }
        preset.name = target.clone();
        preset.validate()?;
        if let Some(existing) = merged.saved.iter_mut().find(|entry| entry.name == target) {
            *existing = preset;
            outcome.replaced += 1;
        } else {
            merged.saved.push(preset);
            outcome.imported += 1;
        }
    }
    Ok((merged, outcome))
}

pub fn merge_image(
    existing: &ImageExportPresetPrefs,
    file: &PresetFile,
    policy: ConflictPolicy,
) -> Result<(ImageExportPresetPrefs, ImportOutcome), String> {
    if file.kind != PresetKind::Image {
        return Err(kind_mismatch(file.kind, PresetKind::Image));
    }
    let presets: Vec<ImageExportPreset> = file
        .presets
        .iter()
        .map(|value| serde_json::from_value(value.clone()))
        .collect::<std::result::Result<_, _>>()
        .map_err(|error| format!("The preset file is not valid: {error}."))?;
    let names: Vec<String> = presets
        .iter()
        .map(|preset| preset.name.trim().to_string())
        .collect();
    let targets = plan_names(&image_names(existing), &names, policy, PresetKind::Image)?;
    let mut merged = existing.clone();
    let mut outcome = ImportOutcome {
        warnings: file.warnings.clone(),
        ..ImportOutcome::default()
    };
    for (mut preset, target) in presets.into_iter().zip(targets) {
        let original = preset.name.trim().to_string();
        let Some(target) = target else {
            outcome.skipped += 1;
            continue;
        };
        if target != original {
            outcome.renamed.push((original, target.clone()));
        }
        preset.name = target.clone();
        preset.validate()?;
        if let Some(existing) = merged.saved.iter_mut().find(|entry| entry.name == target) {
            *existing = preset;
            outcome.replaced += 1;
        } else {
            merged.saved.push(preset);
            outcome.imported += 1;
        }
    }
    Ok((merged, outcome))
}

pub fn merge_vector(
    existing: &VectorExportPresetPrefs,
    file: &PresetFile,
    policy: ConflictPolicy,
) -> Result<(VectorExportPresetPrefs, ImportOutcome), String> {
    if file.kind != PresetKind::Vector {
        return Err(kind_mismatch(file.kind, PresetKind::Vector));
    }
    let presets: Vec<VectorExportPreset> = file
        .presets
        .iter()
        .map(|value| serde_json::from_value(value.clone()))
        .collect::<std::result::Result<_, _>>()
        .map_err(|error| format!("The preset file is not valid: {error}."))?;
    let names: Vec<String> = presets
        .iter()
        .map(|preset| preset.name.trim().to_string())
        .collect();
    let targets = plan_names(&vector_names(existing), &names, policy, PresetKind::Vector)?;
    let mut merged = existing.clone();
    let mut outcome = ImportOutcome {
        warnings: file.warnings.clone(),
        ..ImportOutcome::default()
    };
    for (mut preset, target) in presets.into_iter().zip(targets) {
        let original = preset.name.trim().to_string();
        let Some(target) = target else {
            outcome.skipped += 1;
            continue;
        };
        if target != original {
            outcome.renamed.push((original, target.clone()));
        }
        preset.name = target.clone();
        preset.layers.clear();
        preset.validate()?;
        if let Some(existing) = merged.saved.iter_mut().find(|entry| entry.name == target) {
            *existing = preset;
            outcome.replaced += 1;
        } else {
            merged.saved.push(preset);
            outcome.imported += 1;
        }
    }
    Ok((merged, outcome))
}

fn kind_mismatch(actual: PresetKind, expected: PresetKind) -> String {
    format!(
        "The file holds {} presets, not {} presets.",
        actual.label(),
        expected.label()
    )
}

fn plan_names(
    existing: &[String],
    imported: &[String],
    policy: ConflictPolicy,
    kind: PresetKind,
) -> Result<Vec<Option<String>>, String> {
    let mut taken: HashSet<String> = existing.iter().cloned().collect();
    let mut targets = Vec::with_capacity(imported.len());
    let mut additions = 0;
    for name in imported {
        if taken.contains(name) {
            match policy {
                ConflictPolicy::Skip => {
                    targets.push(None);
                }
                ConflictPolicy::Replace => {
                    targets.push(Some(name.clone()));
                }
                ConflictPolicy::KeepBoth => {
                    let renamed = free_name(name, &mut taken)?;
                    additions += 1;
                    targets.push(Some(renamed));
                }
            }
        } else {
            taken.insert(name.clone());
            additions += 1;
            targets.push(Some(name.clone()));
        }
    }
    if existing.len() + additions > MAX_EXPORT_PRESETS {
        return Err(format!(
            "Importing would reach {} {} presets; the limit is {MAX_EXPORT_PRESETS}. \
             Choose Replace or delete some.",
            existing.len() + additions,
            kind.label()
        ));
    }
    Ok(targets)
}

fn free_name(base: &str, taken: &mut HashSet<String>) -> Result<String, String> {
    for number in 2..=99 {
        let suffix = format!(" ({number})");
        let budget = 60usize.saturating_sub(suffix.chars().count());
        let stem: String = base.chars().take(budget).collect();
        let candidate = format!("{}{}", stem.trim_end(), suffix);
        if taken.insert(candidate.clone()) {
            return Ok(candidate);
        }
    }
    Err(format!("Could not find a free name for \"{base}\"."))
}

pub fn export_gcode(prefs: &ExportPresetPrefs, path: &Path) -> Result<ExportOutcome, String> {
    if prefs.saved.is_empty() {
        return Err("There are no G-code presets to export.".to_string());
    }
    let values = prefs
        .saved
        .iter()
        .map(serde_json::to_value)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| format!("Cannot write the preset file: {error}."))?;
    write_file(PresetKind::Gcode, values, path)
}

pub fn export_image(prefs: &ImageExportPresetPrefs, path: &Path) -> Result<ExportOutcome, String> {
    if prefs.saved.is_empty() {
        return Err("There are no image presets to export.".to_string());
    }
    let values = prefs
        .saved
        .iter()
        .map(serde_json::to_value)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| format!("Cannot write the preset file: {error}."))?;
    write_file(PresetKind::Image, values, path)
}

pub fn export_vector(
    prefs: &VectorExportPresetPrefs,
    path: &Path,
) -> Result<ExportOutcome, String> {
    if prefs.saved.is_empty() {
        return Err("There are no vector presets to export.".to_string());
    }
    let mut warnings = Vec::new();
    let mut portable = prefs.saved.clone();
    for preset in &mut portable {
        if !preset.layers.is_empty() {
            preset.layers.clear();
            warnings.push(
                "Layer selections stay with the document and are not written to preset files."
                    .to_string(),
            );
        }
    }
    warnings.dedup();
    let values = portable
        .iter()
        .map(serde_json::to_value)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| format!("Cannot write the preset file: {error}."))?;
    let mut outcome = write_file(PresetKind::Vector, values, path)?;
    outcome.warnings = warnings;
    Ok(outcome)
}

fn write_file(kind: PresetKind, presets: Vec<Value>, path: &Path) -> Result<ExportOutcome, String> {
    let count = presets.len();
    let envelope = serde_json::json!({
        "schema": SCHEMA,
        "version": VERSION,
        "kind": kind.id(),
        "presets": presets
    });
    let text = serde_json::to_string_pretty(&envelope)
        .map_err(|error| format!("Cannot write the preset file: {error}."))?;
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "presets".to_string());
    let temporary = path.with_file_name(format!("{file_name}.tmp"));
    if let Err(error) = std::fs::write(&temporary, text) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("Cannot write the preset file: {error}."));
    }
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("Cannot write the preset file: {error}."));
    }
    Ok(ExportOutcome {
        count,
        warnings: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("beamburin_presets_{}_{name}", std::process::id()))
    }

    fn gcode(name: &str) -> ExportPreset {
        ExportPreset {
            name: name.to_string(),
            dialect: "grbl".to_string(),
            decimals: 3,
            comments: true,
            travel: false,
            crlf: false,
            smooth_mm: 0.05,
            arcs: true,
            origin: crate::gcode::JobOrigin::default(),
        }
    }

    fn image(name: &str) -> ImageExportPreset {
        ImageExportPreset {
            name: name.to_string(),
            dpi: 300.0,
            dpi_custom: false,
            transparent: false,
            margin_mm: 2.0,
            offset_x_mm: 0.0,
            offset_y_mm: 0.0,
            custom_size: false,
            width_px: 100,
            height_px: 100,
            keep_aspect: true,
            drawing: false,
            selection: false,
            allow_outside_page: false,
        }
    }

    fn vector(name: &str) -> VectorExportPreset {
        VectorExportPreset {
            name: name.to_string(),
            format: "svg".to_string(),
            text_as_paths: true,
            arcs: false,
            margin_mm: 1.0,
            dxf_unit: "mm".to_string(),
            embed_images: false,
            area: "drawing".to_string(),
            layers: vec!["layer-1".to_string()],
            allow_outside_page: true,
        }
    }

    #[test]
    fn the_three_families_round_trip() {
        let path = temp_path("roundtrip.bwp-presets");
        let gcode_prefs = ExportPresetPrefs {
            saved: vec![gcode("Fast")],
        };
        let image_prefs = ImageExportPresetPrefs {
            saved: vec![image("Photo")],
        };
        let vector_prefs = VectorExportPresetPrefs {
            saved: vec![vector("Outline")],
        };
        export_gcode(&gcode_prefs, &path).unwrap();
        let file = read_file(&path, PresetKind::Gcode).unwrap();
        assert_eq!(file.presets.len(), 1);
        let (merged, outcome) =
            merge_gcode(&ExportPresetPrefs::default(), &file, ConflictPolicy::Skip).unwrap();
        assert_eq!(merged.saved[0].name, "Fast");
        assert_eq!(outcome.imported, 1);

        export_image(&image_prefs, &path).unwrap();
        let file = read_file(&path, PresetKind::Image).unwrap();
        let (merged, _) = merge_image(
            &ImageExportPresetPrefs::default(),
            &file,
            ConflictPolicy::Skip,
        )
        .unwrap();
        assert!((merged.saved[0].dpi - 300.0).abs() < f64::EPSILON);

        export_vector(&vector_prefs, &path).unwrap();
        let file = read_file(&path, PresetKind::Vector).unwrap();
        assert!(!file.warnings.is_empty());
        let (merged, _) = merge_vector(
            &VectorExportPresetPrefs::default(),
            &file,
            ConflictPolicy::Skip,
        )
        .unwrap();
        assert!(merged.saved[0].layers.is_empty());
        assert!(merged.saved[0].allow_outside_page);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_file_with_the_wrong_schema_version_or_kind_is_rejected() {
        let path = temp_path("bad.bwp-presets");
        std::fs::write(
            &path,
            r#"{"schema":"other","version":1,"kind":"image","presets":[]}"#,
        )
        .unwrap();
        assert!(
            read_file(&path, PresetKind::Image)
                .unwrap_err()
                .contains("not a BeamBurin preset")
        );
        std::fs::write(
            &path,
            r#"{"schema":"beamburin-presets","version":2,"kind":"image","presets":[]}"#,
        )
        .unwrap();
        assert!(
            read_file(&path, PresetKind::Image)
                .unwrap_err()
                .contains("version 2")
        );
        std::fs::write(
            &path,
            r#"{"schema":"beamburin-presets","version":1,"kind":"vector","presets":[]}"#,
        )
        .unwrap();
        assert!(
            read_file(&path, PresetKind::Image)
                .unwrap_err()
                .contains("vector presets")
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn empty_duplicate_unknown_and_invalid_presets_are_rejected() {
        let path = temp_path("bad2.bwp-presets");
        std::fs::write(
            &path,
            r#"{"schema":"beamburin-presets","version":1,"kind":"image","presets":[]}"#,
        )
        .unwrap();
        assert!(
            read_file(&path, PresetKind::Image)
                .unwrap_err()
                .contains("no presets")
        );
        std::fs::write(
            &path,
            r#"{"schema":"beamburin-presets","version":1,"kind":"image","presets":[
                {"name":"A","dpi":300,"dpi_custom":false,"transparent":false,"margin_mm":0,
                 "offset_x_mm":0,"offset_y_mm":0,"custom_size":false,"width_px":10,
                 "height_px":10,"keep_aspect":true,"drawing":false,"selection":false,
                 "allow_outside_page":false,"extra":1}]}"#,
        )
        .unwrap();
        assert!(
            read_file(&path, PresetKind::Image)
                .unwrap_err()
                .contains("unknown field \"extra\"")
        );
        std::fs::write(
            &path,
            r#"{"schema":"beamburin-presets","version":1,"kind":"image","presets":[
                {"name":"A","dpi":300,"dpi_custom":false,"transparent":false,"margin_mm":0,
                 "offset_x_mm":0,"offset_y_mm":0,"custom_size":false,"width_px":10,
                 "height_px":10,"keep_aspect":true,"drawing":false,"selection":false,
                 "allow_outside_page":false},
                {"name":"A","dpi":300,"dpi_custom":false,"transparent":false,"margin_mm":0,
                 "offset_x_mm":0,"offset_y_mm":0,"custom_size":false,"width_px":10,
                 "height_px":10,"keep_aspect":true,"drawing":false,"selection":false,
                 "allow_outside_page":false}]}"#,
        )
        .unwrap();
        assert!(
            read_file(&path, PresetKind::Image)
                .unwrap_err()
                .contains("repeats the name")
        );
        std::fs::write(
            &path,
            r#"{"schema":"beamburin-presets","version":1,"kind":"image","presets":[
                {"name":"A","dpi":300,"dpi_custom":false,"transparent":false,"margin_mm":500,
                 "offset_x_mm":0,"offset_y_mm":0,"custom_size":false,"width_px":10,
                 "height_px":10,"keep_aspect":true,"drawing":false,"selection":false,
                 "allow_outside_page":false}]}"#,
        )
        .unwrap();
        assert!(
            read_file(&path, PresetKind::Image)
                .unwrap_err()
                .contains("margin must be")
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn conflicts_follow_skip_replace_and_keep_both() {
        let existing = ImageExportPresetPrefs {
            saved: vec![image("Same")],
        };
        let file = PresetFile {
            kind: PresetKind::Image,
            presets: vec![serde_json::to_value(image("Same")).unwrap()],
            warnings: Vec::new(),
        };
        let (merged, outcome) = merge_image(&existing, &file, ConflictPolicy::Skip).unwrap();
        assert_eq!(merged.saved.len(), 1);
        assert_eq!(outcome.skipped, 1);

        let mut changed = image("Same");
        changed.dpi = 600.0;
        let file = PresetFile {
            kind: PresetKind::Image,
            presets: vec![serde_json::to_value(changed).unwrap()],
            warnings: Vec::new(),
        };
        let (merged, outcome) = merge_image(&existing, &file, ConflictPolicy::Replace).unwrap();
        assert_eq!(merged.saved.len(), 1);
        assert!((merged.saved[0].dpi - 600.0).abs() < f64::EPSILON);
        assert_eq!(outcome.replaced, 1);

        let (merged, outcome) = merge_image(&existing, &file, ConflictPolicy::KeepBoth).unwrap();
        assert_eq!(merged.saved.len(), 2);
        assert_eq!(merged.saved[1].name, "Same (2)");
        assert_eq!(
            outcome.renamed,
            vec![("Same".to_string(), "Same (2)".to_string())]
        );
    }

    #[test]
    fn keep_both_truncates_and_can_fail_when_names_run_out() {
        let long = "x".repeat(60);
        let existing = ImageExportPresetPrefs {
            saved: vec![image(&long)],
        };
        let file = PresetFile {
            kind: PresetKind::Image,
            presets: vec![serde_json::to_value(image(&long)).unwrap()],
            warnings: Vec::new(),
        };
        let (merged, _) = merge_image(&existing, &file, ConflictPolicy::KeepBoth).unwrap();
        assert_eq!(merged.saved[1].name.chars().count(), 60);
        assert!(merged.saved[1].name.ends_with(" (2)"));

        let mut taken = HashSet::new();
        taken.insert("Same".to_string());
        for number in 2..=99 {
            taken.insert(format!("Same ({number})"));
        }
        assert!(free_name("Same", &mut taken).is_err());
    }

    #[test]
    fn importing_past_the_limit_is_refused() {
        let existing = ImageExportPresetPrefs {
            saved: (0..MAX_EXPORT_PRESETS)
                .map(|index| image(&format!("P{index}")))
                .collect(),
        };
        let file = PresetFile {
            kind: PresetKind::Image,
            presets: vec![serde_json::to_value(image("New")).unwrap()],
            warnings: Vec::new(),
        };
        let error = merge_image(&existing, &file, ConflictPolicy::Skip).unwrap_err();
        assert!(error.contains("limit is 20"), "{error}");
    }

    #[test]
    fn exporting_an_empty_family_is_refused() {
        let path = temp_path("empty.bwp-presets");
        assert!(
            export_image(&ImageExportPresetPrefs::default(), &path)
                .unwrap_err()
                .contains("no image presets")
        );
        assert!(
            export_gcode(&ExportPresetPrefs::default(), &path)
                .unwrap_err()
                .contains("no G-code presets")
        );
        assert!(
            export_vector(&VectorExportPresetPrefs::default(), &path)
                .unwrap_err()
                .contains("no vector presets")
        );
    }

    #[test]
    fn export_is_atomic_and_leaves_no_temporary_file() {
        let path = temp_path("atomic.bwp-presets");
        let prefs = ImageExportPresetPrefs {
            saved: vec![image("One")],
        };
        export_image(&prefs, &path).unwrap();
        assert!(path.is_file());
        assert!(
            !path
                .with_file_name(format!(
                    "{}.tmp",
                    path.file_name().unwrap().to_string_lossy()
                ))
                .exists()
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_large_file_is_rejected_before_parsing() {
        let path = temp_path("large.bwp-presets");
        std::fs::write(&path, vec![b' '; (MAX_PRESET_FILE_BYTES + 1) as usize]).unwrap();
        assert!(
            read_file(&path, PresetKind::Gcode)
                .unwrap_err()
                .contains("1 MiB")
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_non_json_file_is_rejected() {
        let path = temp_path("text.bwp-presets");
        std::fs::write(&path, "not json").unwrap();
        assert!(
            read_file(&path, PresetKind::Gcode)
                .unwrap_err()
                .contains("not valid JSON")
        );
        let _ = std::fs::remove_file(path);
    }
}
