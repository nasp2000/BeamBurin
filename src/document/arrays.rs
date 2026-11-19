//! Virtual arrays: one specification and in-memory copies.
//!
//! A project stores the array specification and its base shapes. Copies are
//! derived when the project is opened and removed again when it is saved, so a
//! file can never drift between two material copies.

use serde::{Deserialize, Serialize};

use crate::document::model::{Document, Object};
use crate::geometry::{Point2, Transform2};

pub const MAX_ARRAY_COLUMNS: usize = 100;
pub const MAX_ARRAY_ROWS: usize = 100;
pub const MAX_ARRAY_OBJECTS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArraySpec {
    pub id: String,
    pub name: String,
    /// Base shapes, in the order the copies are painted.
    pub source_ids: Vec<String>,
    pub layout: ArrayLayout,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArrayLayout {
    Grid {
        columns: usize,
        rows: usize,
        spacing_x_mm: f64,
        spacing_y_mm: f64,
        spacing: Spacing,
        #[serde(default)]
        angle_degrees: f64,
        #[serde(default)]
        column_major: bool,
        #[serde(default)]
        mirror_columns: bool,
        #[serde(default)]
        mirror_rows: bool,
        #[serde(default)]
        shift_fraction: f64,
    },
    Circular {
        center_x: f64,
        center_y: f64,
        /// Copies beyond the base shapes; cells = copies + 1.
        copies: usize,
        start_degrees: f64,
        step_degrees: f64,
        #[serde(default = "default_true")]
        rotate_copies: bool,
    },
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Spacing {
    #[default]
    Gap,
    Pitch,
}

impl ArrayLayout {
    pub fn cells(&self) -> usize {
        match self {
            Self::Grid { columns, rows, .. } => columns.saturating_mul(*rows),
            Self::Circular { copies, .. } => copies.saturating_add(1),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Grid {
                columns,
                rows,
                spacing_x_mm,
                spacing_y_mm,
                angle_degrees,
                shift_fraction,
                ..
            } => {
                if !(1..=MAX_ARRAY_COLUMNS).contains(columns)
                    || !(1..=MAX_ARRAY_ROWS).contains(rows)
                {
                    return Err(format!(
                        "columns and rows must be between 1 and {MAX_ARRAY_COLUMNS}"
                    ));
                }
                if columns.saturating_mul(*rows) < 2 {
                    return Err("an array needs at least two cells".to_string());
                }
                if !spacing_x_mm.is_finite()
                    || !spacing_y_mm.is_finite()
                    || spacing_x_mm.abs() > 10_000.0
                    || spacing_y_mm.abs() > 10_000.0
                {
                    return Err("spacing must be finite and at most 10000 mm".to_string());
                }
                if !angle_degrees.is_finite() {
                    return Err("the array angle must be finite".to_string());
                }
                if !(0.0..=1.0).contains(shift_fraction) || !shift_fraction.is_finite() {
                    return Err("the row shift must be between 0 and 1".to_string());
                }
                Ok(())
            }
            Self::Circular {
                copies,
                step_degrees,
                center_x,
                center_y,
                start_degrees,
                ..
            } => {
                if !(1..=100).contains(copies) {
                    return Err("copies must be between 1 and 100".to_string());
                }
                if !step_degrees.is_finite() || *step_degrees == 0.0 || step_degrees.abs() > 360.0 {
                    return Err(
                        "the step must be finite, non-zero and at most 360 degrees".to_string()
                    );
                }
                if !center_x.is_finite() || !center_y.is_finite() || !start_degrees.is_finite() {
                    return Err("the array centre and start must be finite".to_string());
                }
                Ok(())
            }
        }
    }

    fn cell_transform(&self, cell: usize, source_bounds: (Point2, Point2)) -> Option<Transform2> {
        if cell == 0 {
            return Some(Transform2::identity());
        }
        let width = (source_bounds.1.x - source_bounds.0.x).max(0.001);
        let height = (source_bounds.1.y - source_bounds.0.y).max(0.001);
        match self {
            Self::Grid {
                columns,
                rows,
                spacing_x_mm,
                spacing_y_mm,
                spacing,
                angle_degrees,
                column_major,
                mirror_columns,
                mirror_rows,
                shift_fraction,
            } => {
                let cells = columns.saturating_mul(*rows);
                if cell >= cells {
                    return None;
                }
                let (column, row) = if *column_major {
                    (cell / *rows, cell % *rows)
                } else {
                    (cell % *columns, cell / *columns)
                };
                let step_x = spacing_x_mm + if *spacing == Spacing::Gap { width } else { 0.0 };
                let step_y = spacing_y_mm
                    + if *spacing == Spacing::Gap {
                        height
                    } else {
                        0.0
                    };
                #[allow(clippy::cast_precision_loss)]
                let mut x = column as f64 * step_x;
                #[allow(clippy::cast_precision_loss)]
                let mut y = row as f64 * step_y;
                if row % 2 == 1 {
                    x += shift_fraction * step_x;
                }
                if column % 2 == 1 && *shift_fraction > 0.0 {
                    y += shift_fraction * step_y;
                }
                let angle = angle_degrees.to_radians();
                let (sin, cos) = angle.sin_cos();
                let rotated = (x * cos - y * sin, x * sin + y * cos);
                let mut transform = Transform2::translate(rotated.0, rotated.1);
                let center = Point2::new(
                    f64::midpoint(source_bounds.0.x, source_bounds.1.x),
                    f64::midpoint(source_bounds.0.y, source_bounds.1.y),
                );
                if (*mirror_columns && column % 2 == 1) || (*mirror_rows && row % 2 == 1) {
                    let sx = if *mirror_columns && column % 2 == 1 {
                        -1.0
                    } else {
                        1.0
                    };
                    let sy = if *mirror_rows && row % 2 == 1 {
                        -1.0
                    } else {
                        1.0
                    };
                    transform =
                        transform.compose(&Transform2::scale_around(sx, sy, center.x, center.y));
                }
                Some(transform)
            }
            Self::Circular {
                center_x,
                center_y,
                copies,
                start_degrees,
                step_degrees,
                rotate_copies,
            } => {
                if cell > *copies {
                    return None;
                }
                #[allow(clippy::cast_precision_loss)]
                let angle = (start_degrees + cell as f64 * step_degrees).to_radians();
                let center = Point2::new(*center_x, *center_y);
                let rotate = Transform2::rotate_around(angle, center.x, center.y);
                if *rotate_copies {
                    Some(rotate)
                } else {
                    let anchor = Point2::new(
                        f64::midpoint(source_bounds.0.x, source_bounds.1.x),
                        f64::midpoint(source_bounds.0.y, source_bounds.1.y),
                    );
                    let moved = rotate.apply(anchor);
                    Some(Transform2::translate(
                        moved.x - anchor.x,
                        moved.y - anchor.y,
                    ))
                }
            }
        }
    }
}

impl ArraySpec {
    pub fn validate(&self, document: &Document) -> Result<(), String> {
        if self.id.trim().is_empty() || self.name.trim().is_empty() {
            return Err("array id and name must not be empty".to_string());
        }
        self.layout
            .validate()
            .map_err(|error| format!("array '{}': {error}", self.name))?;
        if self.source_ids.is_empty() {
            return Err(format!(
                "array '{}': it needs at least one base shape",
                self.name
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for id in &self.source_ids {
            if !seen.insert(id) {
                return Err(format!("array '{}': duplicate base shape {id}", self.name));
            }
            let Some(object) = document.object(id) else {
                return Err(format!("array '{}': missing base shape {id}", self.name));
            };
            if object.array_id.is_some() {
                return Err(format!(
                    "array '{}': a copy cannot be a base shape",
                    self.name
                ));
            }
        }
        let copies = self
            .source_ids
            .len()
            .saturating_mul(self.layout.cells().saturating_sub(1));
        if copies > MAX_ARRAY_OBJECTS {
            return Err(format!(
                "array '{}': {copies} copies exceed the limit of {MAX_ARRAY_OBJECTS}",
                self.name
            ));
        }
        Ok(())
    }
}

fn source_bounds(document: &Document, source_ids: &[String]) -> Option<(Point2, Point2)> {
    let mut min = Point2::new(f64::INFINITY, f64::INFINITY);
    let mut max = Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut found = false;
    for id in source_ids {
        let Some((low, high)) = document.object(id).and_then(Object::bounds) else {
            continue;
        };
        found = true;
        min.x = min.x.min(low.x);
        min.y = min.y.min(low.y);
        max.x = max.x.max(high.x);
        max.y = max.y.max(high.y);
    }
    found.then_some((min, max))
}

impl Document {
    pub fn array(&self, id: &str) -> Option<&ArraySpec> {
        self.arrays.iter().find(|array| array.id == id)
    }

    pub fn array_mut(&mut self, id: &str) -> Option<&mut ArraySpec> {
        self.arrays.iter_mut().find(|array| array.id == id)
    }

    pub fn array_of_copy(&self, object_id: &str) -> Option<&ArraySpec> {
        let array_id = self.object(object_id)?.array_id.as_deref()?;
        self.array(array_id)
    }

    pub fn array_members(&self, id: &str) -> Vec<&Object> {
        let Some(array) = self.array(id) else {
            return Vec::new();
        };
        self.objects
            .iter()
            .filter(|object| {
                array.source_ids.contains(&object.id)
                    || object.array_id.as_deref() == Some(array.id.as_str())
            })
            .collect()
    }

    /// Remove copies, regenerate them from the specifications, and keep the
    /// old identifiers where the cell still exists.
    pub fn expand_arrays(&mut self) {
        let old: Vec<Object> = self
            .objects
            .iter()
            .filter(|object| object.array_id.is_some())
            .cloned()
            .collect();
        self.objects.retain(|object| object.array_id.is_none());
        let arrays = self.arrays.clone();
        for array in arrays {
            let previous: Vec<Object> = old
                .iter()
                .filter(|object| object.array_id.as_deref() == Some(array.id.as_str()))
                .cloned()
                .collect();
            let Some(bounds) = source_bounds(self, &array.source_ids) else {
                continue;
            };
            let Some(last_index) = self
                .objects
                .iter()
                .rposition(|object| array.source_ids.contains(&object.id))
            else {
                continue;
            };
            let cells = array.layout.cells();
            let mut generated: Vec<Object> = Vec::new();
            for cell in 1..cells {
                let Some(transform) = array.layout.cell_transform(cell, bounds) else {
                    continue;
                };
                #[allow(clippy::cast_possible_truncation)]
                let cell_number = cell as u32;
                let mut cell_ids: Vec<Option<String>> = vec![None; array.source_ids.len()];
                let mut cell_copies: Vec<Object> = Vec::new();
                for (index, source_id) in array.source_ids.iter().enumerate() {
                    let Some(source) = self.object(source_id) else {
                        continue;
                    };
                    let mut copy = source.clone();
                    let ordinal = (cell - 1) * array.source_ids.len() + index;
                    if let Some(previous) = previous.get(ordinal) {
                        copy.id.clone_from(&previous.id);
                        copy.group_id.clone_from(&previous.group_id);
                    } else {
                        copy.id = uuid::Uuid::new_v4().to_string();
                        copy.group_id = None;
                    }
                    copy.name = format!("{} · {} #{}", source.name, array.name, cell);
                    copy.transform = transform.compose(&source.transform);
                    copy.array_id = Some(array.id.clone());
                    copy.array_cell = Some(cell_number);
                    copy.hole_of = None;
                    cell_ids[index] = Some(copy.id.clone());
                    cell_copies.push(copy);
                }
                for (index, source_id) in array.source_ids.iter().enumerate() {
                    let Some(source) = self.object(source_id) else {
                        continue;
                    };
                    let Some(parent) = &source.hole_of else {
                        continue;
                    };
                    let Some(parent_index) = array.source_ids.iter().position(|id| id == parent)
                    else {
                        continue;
                    };
                    if let (Some(Some(target)), Some(copy)) =
                        (cell_ids.get(parent_index), cell_copies.get_mut(index))
                    {
                        copy.hole_of = Some(target.clone());
                    }
                }
                generated.extend(cell_copies);
            }
            let mut tail = self.objects.split_off(last_index + 1);
            self.objects.extend(generated);
            self.objects.append(&mut tail);
        }
    }

    /// Remove every derived copy, leaving only the specifications and bases.
    pub fn strip_array_copies(&mut self) {
        self.objects.retain(|object| object.array_id.is_none());
    }

    /// Remove one specification. With `keep_copies`, the copies become normal
    /// objects; otherwise they are removed too.
    pub fn remove_array(&mut self, id: &str, keep_copies: bool) -> bool {
        let Some(index) = self.arrays.iter().position(|array| array.id == id) else {
            return false;
        };
        self.arrays.remove(index);
        if keep_copies {
            for object in &mut self.objects {
                if object.array_id.as_deref() == Some(id) {
                    object.array_id = None;
                    object.array_cell = None;
                }
            }
        } else {
            self.objects
                .retain(|object| object.array_id.as_deref() != Some(id));
        }
        true
    }

    /// Drop invalid specifications and orphan links, then regenerate.
    pub fn repair_arrays(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        let arrays = self.arrays.clone();
        self.arrays.clear();
        for array in arrays {
            match array.validate(self) {
                Ok(()) => self.arrays.push(array),
                Err(error) => {
                    warnings.push(format!("array '{}' was repaired: {error}", array.name));
                }
            }
        }
        let valid: std::collections::HashSet<String> =
            self.arrays.iter().map(|array| array.id.clone()).collect();
        let cells: std::collections::HashMap<String, usize> = self
            .arrays
            .iter()
            .map(|array| (array.id.clone(), array.layout.cells()))
            .collect();
        for object in &mut self.objects {
            if let Some(array_id) = object.array_id.clone() {
                let valid_cell = cells.get(&array_id).is_some_and(|count| {
                    object
                        .array_cell
                        .is_some_and(|cell| (cell as usize) < *count)
                });
                if !valid.contains(&array_id) || !valid_cell {
                    object.array_id = None;
                    object.array_cell = None;
                }
            }
        }
        self.expand_arrays();
        warnings
    }

    /// Validate every array specification and copy link.
    pub(crate) fn validate_arrays(&self) -> Result<(), String> {
        let mut ids = std::collections::HashSet::new();
        for array in &self.arrays {
            if !ids.insert(array.id.as_str()) {
                return Err(format!("duplicate array id: {}", array.id));
            }
            array.validate(self)?;
        }
        for object in &self.objects {
            if let Some(array_id) = &object.array_id {
                let Some(array) = self.array(array_id) else {
                    return Err(format!(
                        "object {} belongs to missing array {array_id}",
                        object.id
                    ));
                };
                let Some(cell) = object.array_cell else {
                    return Err(format!("object {} has no array cell", object.id));
                };
                if cell as usize >= array.layout.cells() {
                    return Err(format!(
                        "object {} has an invalid array cell {cell}",
                        object.id
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::serialization::{from_json, to_json};
    use crate::document::{Object, Shape};

    fn rect(document: &mut Document, x: f64, y: f64, width: f64, height: f64) -> String {
        let layer = document.layers[0].id.clone();
        let mut object = Object::new(
            layer,
            Shape::Rect {
                width,
                height,
                corner_radius: 0.0,
            },
        );
        object.transform = Transform2::translate(x, y);
        let id = object.id.clone();
        document.add_object(object);
        id
    }

    fn grid(columns: usize, rows: usize, x: f64, y: f64, spacing: Spacing) -> ArrayLayout {
        ArrayLayout::Grid {
            columns,
            rows,
            spacing_x_mm: x,
            spacing_y_mm: y,
            spacing,
            angle_degrees: 0.0,
            column_major: false,
            mirror_columns: false,
            mirror_rows: false,
            shift_fraction: 0.0,
        }
    }

    #[test]
    fn grid_cell_offsets_use_gap_spacing() {
        let mut document = Document::new("Array", 400.0, 400.0);
        let source = rect(&mut document, 10.0, 10.0, 40.0, 20.0);
        document.arrays.push(ArraySpec {
            id: "array".to_string(),
            name: "Array 1".to_string(),
            source_ids: vec![source],
            layout: grid(3, 2, 5.0, 5.0, Spacing::Gap),
        });
        document.expand_arrays();
        let mut xs: Vec<f64> = document
            .objects
            .iter()
            .map(|object| object.bounds().unwrap().0.x)
            .collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        xs.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        assert_eq!(xs, vec![10.0, 55.0, 100.0]);
    }

    #[test]
    fn grid_cell_offsets_use_pitch_spacing() {
        let mut document = Document::new("Array", 400.0, 400.0);
        let source = rect(&mut document, 10.0, 10.0, 40.0, 20.0);
        document.arrays.push(ArraySpec {
            id: "array".to_string(),
            name: "Array 1".to_string(),
            source_ids: vec![source],
            layout: grid(3, 2, 50.0, 30.0, Spacing::Pitch),
        });
        document.expand_arrays();
        let bounds: Vec<f64> = document
            .objects
            .iter()
            .filter(|object| object.array_cell == Some(2))
            .map(|object| object.bounds().unwrap().0.x)
            .collect();
        assert_eq!(bounds, vec![110.0]);
    }

    #[test]
    fn circular_copies_sit_on_start_plus_step() {
        let mut document = Document::new("Array", 400.0, 400.0);
        let layer = document.layers[0].id.clone();
        let mut base = Object::new(
            layer,
            Shape::Ellipse {
                rx: 0.001,
                ry: 0.001,
            },
        );
        base.transform = Transform2::translate(10.0, 0.0);
        let source = base.id.clone();
        document.add_object(base);
        document.arrays.push(ArraySpec {
            id: "array".to_string(),
            name: "Array 1".to_string(),
            source_ids: vec![source],
            layout: ArrayLayout::Circular {
                center_x: 0.0,
                center_y: 0.0,
                copies: 3,
                start_degrees: 0.0,
                step_degrees: 90.0,
                rotate_copies: true,
            },
        });
        document.expand_arrays();
        let mut points: Vec<(f64, f64)> = document
            .objects
            .iter()
            .filter(|object| object.array_id.is_some())
            .map(|object| {
                let bounds = object.bounds().unwrap();
                (
                    f64::midpoint(bounds.0.x, bounds.1.x),
                    f64::midpoint(bounds.0.y, bounds.1.y),
                )
            })
            .collect();
        points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        assert_eq!(points.len(), 3);
        assert!(
            points
                .iter()
                .any(|(x, y)| (*x).abs() < 1e-3 && (*y - 10.0).abs() < 1e-3)
        );
        assert!(
            points
                .iter()
                .any(|(x, y)| (*x + 10.0).abs() < 1e-3 && y.abs() < 1e-3)
        );
        assert!(
            points
                .iter()
                .any(|(x, y)| x.abs() < 1e-3 && (*y + 10.0).abs() < 1e-3)
        );
    }

    #[test]
    fn array_needs_two_cells_and_valid_numbers() {
        assert!(grid(1, 1, 5.0, 5.0, Spacing::Gap).validate().is_err());
        assert!(grid(2, 2, f64::NAN, 5.0, Spacing::Gap).validate().is_err());
        assert!(
            ArrayLayout::Circular {
                center_x: 0.0,
                center_y: 0.0,
                copies: 1,
                start_degrees: 0.0,
                step_degrees: 0.0,
                rotate_copies: true,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn saved_project_has_the_spec_and_no_copies() {
        let mut document = Document::new("Array", 300.0, 200.0);
        let source = rect(&mut document, 0.0, 0.0, 10.0, 10.0);
        document.arrays.push(ArraySpec {
            id: "array".to_string(),
            name: "Array 1".to_string(),
            source_ids: vec![source],
            layout: grid(2, 2, 5.0, 5.0, Spacing::Gap),
        });
        document.expand_arrays();
        assert_eq!(document.objects.len(), 4);
        let json = to_json(&document).unwrap();
        assert!(!json.contains("array_cell"));
        assert!(json.contains("\"arrays\""));
        let loaded = from_json(&json).unwrap();
        assert_eq!(loaded.objects.len(), 4);
        assert_eq!(loaded.format_version, 3);
    }

    #[test]
    fn repair_drops_a_spec_with_missing_sources_and_keeps_objects() {
        let mut document = Document::new("Array", 300.0, 200.0);
        rect(&mut document, 0.0, 0.0, 10.0, 10.0);
        document.arrays.push(ArraySpec {
            id: "array".to_string(),
            name: "Array 1".to_string(),
            source_ids: vec!["missing".to_string()],
            layout: grid(2, 2, 5.0, 5.0, Spacing::Gap),
        });
        let warnings = document.repair_arrays();
        assert_eq!(document.arrays.len(), 0);
        assert_ne!(warnings.len(), 0);
        assert_eq!(document.objects.len(), 1);
    }
}
