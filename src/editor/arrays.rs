//! Editor commands that create and maintain virtual arrays.

use crate::document::Object;
use crate::document::arrays::{ArrayLayout, ArraySpec};
use crate::geometry::Transform2;

use super::state::EditorState;

impl EditorState {
    fn selected_ids(&self) -> Vec<String> {
        self.selection.ids().to_vec()
    }

    fn select_array(&mut self, id: &str) {
        self.selection.clear();
        for object in self.document.array_members(id) {
            self.selection.add(object.id.clone());
        }
    }

    pub(super) fn array_for_object(&self, id: &str) -> Option<String> {
        if let Some(array_id) = self.document.object(id)?.array_id.clone() {
            return Some(array_id);
        }
        self.document
            .arrays
            .iter()
            .find(|array| array.source_ids.iter().any(|source| source == id))
            .map(|array| array.id.clone())
    }

    pub fn selected_array_id(&self) -> Option<String> {
        self.selection
            .ids()
            .iter()
            .find_map(|id| self.array_for_object(id))
    }

    pub fn selected_array(&self) -> Option<&ArraySpec> {
        let id = self
            .selection
            .ids()
            .iter()
            .find_map(|id| self.array_for_object(id))?;
        self.document.array(&id)
    }

    pub fn create_array(
        &mut self,
        layout: ArrayLayout,
        source_ids: &[String],
    ) -> Result<String, String> {
        let source_ids = if source_ids.is_empty() {
            self.selected_ids()
        } else {
            source_ids.to_vec()
        };
        layout.validate()?;
        if source_ids.is_empty() {
            return Err("select at least one shape first".to_string());
        }
        let number = self.document.arrays.len() + 1;
        let spec = ArraySpec {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("Array {number}"),
            source_ids,
            layout,
        };
        spec.validate(&self.document)?;
        self.record_action("Create array");
        let id = spec.id.clone();
        self.document.arrays.push(spec);
        self.document.expand_arrays();
        self.array_synced_revision = self.geometry_revision;
        self.select_array(&id);
        Ok(id)
    }

    pub fn update_array(&mut self, id: &str, layout: ArrayLayout) -> Result<usize, String> {
        layout.validate()?;
        let spec = self
            .document
            .array_mut(id)
            .ok_or_else(|| format!("unknown array: {id}"))?;
        spec.layout = layout;
        let spec = spec.clone();
        spec.validate(&self.document)?;
        self.record_action("Edit array");
        self.document.expand_arrays();
        self.array_synced_revision = self.geometry_revision;
        self.select_array(id);
        Ok(spec.layout.cells().saturating_sub(1))
    }

    pub fn add_to_array(&mut self, id: &str, ids: &[String]) -> Result<usize, String> {
        let spec = self
            .document
            .array(id)
            .ok_or_else(|| format!("unknown array: {id}"))?
            .clone();
        let mut added = 0;
        for object_id in ids {
            let object = self
                .document
                .object(object_id)
                .ok_or_else(|| format!("unknown object: {object_id}"))?;
            if object.array_id.is_some() {
                return Err("A copy cannot be added to an array; flatten it first".to_string());
            }
            if !spec.source_ids.iter().any(|source| source == object_id) {
                added += 1;
            }
        }
        if added == 0 {
            return Ok(0);
        }
        self.record_action("Add to array");
        if let Some(spec) = self.document.array_mut(id) {
            for object_id in ids {
                if !spec.source_ids.iter().any(|source| source == object_id) {
                    spec.source_ids.push(object_id.clone());
                }
            }
        }
        self.document.expand_arrays();
        self.array_synced_revision = self.geometry_revision;
        self.select_array(id);
        Ok(added)
    }

    pub fn remove_from_array(&mut self, id: &str, ids: &[String]) -> Result<usize, String> {
        let spec = self
            .document
            .array(id)
            .ok_or_else(|| format!("unknown array: {id}"))?
            .clone();
        let remaining: Vec<String> = spec
            .source_ids
            .iter()
            .filter(|source| !ids.iter().any(|id| id == *source))
            .cloned()
            .collect();
        if remaining.is_empty() {
            return Err(
                "An array needs at least one base shape; flatten or delete it instead".to_string(),
            );
        }
        let removed = spec.source_ids.len() - remaining.len();
        if removed == 0 {
            return Ok(0);
        }
        self.record_action("Remove from array");
        if let Some(spec) = self.document.array_mut(id) {
            spec.source_ids = remaining;
        }
        self.document.expand_arrays();
        self.array_synced_revision = self.geometry_revision;
        self.select_array(id);
        Ok(removed)
    }

    pub fn duplicate_array(&mut self, id: &str, dx: f64, dy: f64) -> Result<String, String> {
        let spec = self
            .document
            .array(id)
            .ok_or_else(|| format!("unknown array: {id}"))?
            .clone();
        if !dx.is_finite() || !dy.is_finite() {
            return Err("the duplicate offset must be finite".to_string());
        }
        let sources: Vec<Object> = spec
            .source_ids
            .iter()
            .filter_map(|source| self.document.object(source).cloned())
            .collect();
        if sources.len() != spec.source_ids.len() {
            return Err("the array has a missing base shape".to_string());
        }
        self.record_action("Duplicate array");
        let mut new_ids = Vec::new();
        for mut object in sources {
            object.id = uuid::Uuid::new_v4().to_string();
            object.name = format!("{} copy", object.name);
            object.array_id = None;
            object.array_cell = None;
            object.translate(dx, dy);
            new_ids.push(object.id.clone());
            self.document.add_object(object);
        }
        let number = self.document.arrays.len() + 1;
        let duplicate = ArraySpec {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("Array {number}"),
            source_ids: new_ids,
            layout: spec.layout,
        };
        let new_id = duplicate.id.clone();
        self.document.arrays.push(duplicate);
        self.document.expand_arrays();
        self.array_synced_revision = self.geometry_revision;
        self.select_array(&new_id);
        Ok(new_id)
    }

    pub fn flatten_array(&mut self, id: &str) -> Result<usize, String> {
        if self.document.array(id).is_none() {
            return Err(format!("unknown array: {id}"));
        }
        let count = self.document.array_members(id).len();
        self.record_action("Flatten array");
        self.document.remove_array(id, true);
        self.selection.clear();
        self.array_synced_revision = self.geometry_revision;
        Ok(count)
    }

    pub fn begin_array_edit(&mut self, id: &str) -> Result<(), String> {
        if self.document.array(id).is_none() {
            return Err(format!("unknown array: {id}"));
        }
        self.array_edit = Some(id.to_string());
        self.selection.clear();
        if let Some(array) = self.document.array(id) {
            for source in &array.source_ids {
                self.selection.add(source.clone());
            }
        }
        Ok(())
    }

    pub fn end_array_edit(&mut self) {
        self.array_edit = None;
    }

    /// Regenerate derived copies after a source or the specification changed.
    pub fn sync_arrays(&mut self) {
        if self.array_synced_revision == self.geometry_revision {
            return;
        }
        self.document.expand_arrays();
        self.array_synced_revision = self.geometry_revision;
    }

    /// Adjust a specification after a transform applied to the whole array.
    pub fn absorb_array_transform(&mut self, array_id: &str, transform: &Transform2) {
        let Some(array) = self.document.array_mut(array_id) else {
            return;
        };
        let scale = (transform.a * transform.a + transform.b * transform.b).sqrt();
        let angle = transform.b.atan2(transform.a);
        match &mut array.layout {
            ArrayLayout::Grid {
                angle_degrees,
                spacing_x_mm,
                spacing_y_mm,
                ..
            } => {
                *angle_degrees += angle.to_degrees();
                *spacing_x_mm *= scale;
                *spacing_y_mm *= scale;
            }
            ArrayLayout::Circular {
                center_x, center_y, ..
            } => {
                let center = transform.apply(crate::geometry::Point2::new(*center_x, *center_y));
                *center_x = center.x;
                *center_y = center.y;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::arrays::Spacing;
    use crate::document::{Document, Object, Shape};

    fn editor_with_rect() -> (EditorState, String) {
        let mut document = Document::new("Array", 300.0, 200.0);
        let layer = document.layers[0].id.clone();
        let mut object = Object::new(
            layer,
            Shape::Rect {
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        object.transform = Transform2::translate(10.0, 10.0);
        let id = object.id.clone();
        document.add_object(object);
        let mut editor = EditorState::new(document);
        editor.selection.add(id.clone());
        (editor, id)
    }

    fn grid(columns: usize, rows: usize, spacing: f64) -> ArrayLayout {
        ArrayLayout::Grid {
            columns,
            rows,
            spacing_x_mm: spacing,
            spacing_y_mm: spacing,
            spacing: Spacing::Gap,
            angle_degrees: 0.0,
            column_major: false,
            mirror_columns: false,
            mirror_rows: false,
            shift_fraction: 0.0,
        }
    }

    #[test]
    fn create_array_selects_every_member_and_is_undoable() {
        let (mut editor, _) = editor_with_rect();
        let id = editor.create_array(grid(2, 2, 5.0), &[]).unwrap();
        assert_eq!(editor.document.objects.len(), 4);
        assert_eq!(editor.selection.len(), 4);
        assert_eq!(editor.document.array_members(&id).len(), 4);
        assert!(editor.undo().is_some());
        assert_eq!(editor.document.objects.len(), 1);
        assert_eq!(editor.document.arrays.len(), 0);
    }

    #[test]
    fn update_array_reuses_existing_copy_ids() {
        let (mut editor, _) = editor_with_rect();
        let id = editor.create_array(grid(2, 2, 5.0), &[]).unwrap();
        let before: Vec<String> = editor
            .document
            .objects
            .iter()
            .filter(|object| object.array_id.is_some())
            .map(|object| object.id.clone())
            .collect();
        editor.update_array(&id, grid(3, 2, 5.0)).unwrap();
        let after: Vec<String> = editor
            .document
            .objects
            .iter()
            .filter(|object| object.array_id.is_some())
            .map(|object| object.id.clone())
            .collect();
        assert!(after.starts_with(&before));
        assert_eq!(after.len(), 5);
    }

    #[test]
    fn adding_and_removing_sources_repeats_the_cell() {
        let (mut editor, first) = editor_with_rect();
        let second = editor
            .add_rectangle(
                crate::geometry::Point2::new(30.0, 10.0),
                crate::geometry::Point2::new(40.0, 20.0),
            )
            .unwrap();
        editor.selection.clear();
        editor.selection.add(first.clone());
        let id = editor.create_array(grid(2, 2, 5.0), &[]).unwrap();
        editor.selection.clear();
        editor.selection.add(second.clone());
        assert_eq!(
            editor
                .add_to_array(&id, std::slice::from_ref(&second))
                .unwrap(),
            1
        );
        assert_eq!(editor.document.objects.len(), 8);
        assert_eq!(editor.remove_from_array(&id, &[second]).unwrap(), 1);
        assert_eq!(editor.document.objects.len(), 5);
        assert!(editor.document.object(&first).is_some());
    }

    #[test]
    fn removing_the_last_source_is_refused() {
        let (mut editor, first) = editor_with_rect();
        let id = editor.create_array(grid(2, 2, 5.0), &[]).unwrap();
        assert!(editor.remove_from_array(&id, &[first]).is_err());
    }

    #[test]
    fn flatten_and_delete_are_undoable() {
        let (mut editor, _) = editor_with_rect();
        let id = editor.create_array(grid(2, 2, 5.0), &[]).unwrap();
        assert_eq!(editor.flatten_array(&id).unwrap(), 4);
        assert_eq!(editor.document.arrays.len(), 0);
        assert_eq!(editor.document.objects.len(), 4);
        assert!(editor.undo().is_some());
        assert_eq!(editor.document.objects.len(), 4);
    }

    #[test]
    fn deleting_an_array_deletes_every_member_in_one_step() {
        let (mut editor, _) = editor_with_rect();
        let id = editor.create_array(grid(2, 2, 5.0), &[]).unwrap();
        editor.selection.clear();
        for member in editor.document.array_members(&id) {
            editor.selection.add(member.id.clone());
        }
        editor.delete_selection();
        assert_eq!(editor.document.objects.len(), 0);
        assert_eq!(editor.document.arrays.len(), 0);
    }
}
