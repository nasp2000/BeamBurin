#![allow(clippy::wildcard_imports)]

use super::*;

impl BeamBurinApp {
    pub(super) fn document_properties_window(&mut self, ctx: &egui::Context) {
        use crate::document::Unit;

        let mut open = self.doc_properties_open;
        let mut changed = false;
        egui::Window::new("Document properties")
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .max_height(520.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("document_properties_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
            egui::Grid::new("document_properties_window")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    let name_label = ui.label("Name");
                    changed |= ui
                        .text_edit_singleline(&mut self.editor.document.name)
                        .labelled_by(name_label.id)
                        .changed();
                    ui.end_row();
                    let width_label = ui.label("Width");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.editor.document.width_mm)
                                .speed(1.0)
                                .range(10.0..=100_000.0)
                                .suffix(" mm"),
                        )
                        .labelled_by(width_label.id)
                        .changed();
                    ui.end_row();
                    let height_label = ui.label("Height");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut self.editor.document.height_mm)
                                .speed(1.0)
                                .range(10.0..=100_000.0)
                                .suffix(" mm"),
                        )
                        .labelled_by(height_label.id)
                        .changed();
                    ui.end_row();
                    let unit_label = ui.label("Unit");
                    let mut unit = self.editor.document.unit;
                    egui::ComboBox::from_id_salt("document_unit")
                        .selected_text(unit.label())
                        .show_ui(ui, |ui| {
                            for candidate in [
                                Unit::Millimeters,
                                Unit::Centimeters,
                                Unit::Inches,
                                Unit::Pixels,
                            ] {
                                if ui
                                    .selectable_value(&mut unit, candidate, candidate.label())
                                    .changed()
                                {
                                    changed = true;
                                }
                            }
                        })
                        .response
                        .labelled_by(unit_label.id);
                    self.editor.document.unit = unit;
                    ui.end_row();
                    ui.label("Origin");
                    ui.label(format!("{:?}", self.editor.document.origin));
                    ui.end_row();
                    ui.label("Layers");
                    ui.label(self.editor.document.layers.len().to_string());
                    ui.end_row();
                    ui.label("Objects");
                    ui.label(self.editor.document.objects.len().to_string());
                    ui.end_row();
                    ui.label("Operations");
                    ui.label(self.editor.document.operations.len().to_string());
                    ui.end_row();
                    ui.label("Format version");
                    ui.label(self.editor.document.format_version.to_string());
                    ui.end_row();
                    ui.label("File");
                    ui.label(self.current_path.as_deref().map_or_else(
                        || "not saved yet".to_string(),
                        |path| path.display().to_string(),
                    ));
                    ui.end_row();
                });
            ui.separator();
            ui.strong("Material area");
            let (page_width, page_height) = (
                self.editor.document.width_mm,
                self.editor.document.height_mm,
            );
            let mut enabled = self.editor.document.stock.is_some();
            if ui
                .checkbox(&mut enabled, "Clip the job to a material area")
                .on_hover_text(
                    "Cuts are shortened or dropped outside this rectangle; travels keep the head moving",
                )
                .changed()
            {
                self.editor.document.stock = if enabled {
                    Some(crate::document::MaterialZone::new(
                        0.0,
                        0.0,
                        page_width,
                        page_height,
                    ))
                } else {
                    None
                };
                changed = true;
            }
            if let Some(stock) = &mut self.editor.document.stock {
                egui::Grid::new("material_area_grid")
                    .num_columns(2)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("X / Y");
                        ui.horizontal(|ui| {
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut stock.x_mm)
                                        .speed(1.0)
                                        .suffix(" mm"),
                                )
                                .changed();
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut stock.y_mm)
                                        .speed(1.0)
                                        .suffix(" mm"),
                                )
                                .changed();
                        });
                        ui.end_row();
                        ui.label("Width / height");
                        ui.horizontal(|ui| {
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut stock.width_mm)
                                        .speed(1.0)
                                        .range(0.1..=100_000.0)
                                        .suffix(" mm"),
                                )
                                .changed();
                            changed |= ui
                                .add(
                                    egui::DragValue::new(&mut stock.height_mm)
                                        .speed(1.0)
                                        .range(0.1..=100_000.0)
                                        .suffix(" mm"),
                                )
                                .changed();
                        });
                        ui.end_row();
                    });
                ui.horizontal(|ui| {
                    if ui.button("Use the page size").clicked() {
                        *stock = crate::document::MaterialZone::new(
                            0.0,
                            0.0,
                            page_width,
                            page_height,
                        );
                        changed = true;
                    }
                    if let Err(error) = stock.validate() {
                        ui.colored_label(egui::Color32::from_rgb(200, 60, 60), error);
                    }
                });
            }
            ui.separator();
            ui.strong("Restricted areas");
            ui.weak("Cuts that enter these rectangles are removed from the job.");
            let mut remove_area: Option<usize> = None;
            for (index, area) in self.editor.document.restricted_areas.iter_mut().enumerate()
            {
                ui.horizontal(|ui| {
                    ui.label(format!("#{}", index + 1));
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut area.x_mm)
                                .speed(1.0)
                                .prefix("x ")
                                .suffix(" mm"),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut area.y_mm)
                                .speed(1.0)
                                .prefix("y ")
                                .suffix(" mm"),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut area.width_mm)
                                .speed(1.0)
                                .range(0.1..=100_000.0)
                                .prefix("w ")
                                .suffix(" mm"),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut area.height_mm)
                                .speed(1.0)
                                .range(0.1..=100_000.0)
                                .prefix("h ")
                                .suffix(" mm"),
                        )
                        .changed();
                    if ui.button("Remove").clicked() {
                        remove_area = Some(index);
                    }
                });
            }
            if let Some(index) = remove_area {
                if index < self.editor.document.restricted_areas.len() {
                    self.editor.document.restricted_areas.remove(index);
                    changed = true;
                }
            }
            if ui.button("Add restricted area").clicked() {
                self.editor.document.restricted_areas.push(
                    crate::document::MaterialZone::new(
                        page_width * 0.25,
                        page_height * 0.25,
                        page_width * 0.5,
                        page_height * 0.5,
                    ),
                );
                changed = true;
            }
                });
        });
        if changed {
            self.needs_fit = true;
        }
        if !open || !self.doc_properties_open {
            self.doc_properties_open = open;
        }
        if !open {
            if let Some(snapshot) = self.doc_properties_snapshot.take() {
                if snapshot != self.editor.document {
                    self.editor.history.record(&snapshot, "Document properties");
                }
            }
            self.doc_properties_open = false;
        }
    }

    pub(super) fn operations_compare_window(&mut self, ctx: &egui::Context) {
        let mut open = self.operations_compare_open;
        egui::Window::new("Compare operations")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([560.0, 340.0])
            .max_height(480.0)
            .show(ctx, |ui| {
                if self.editor.document.operations.is_empty() {
                    ui.weak("No operations yet.");
                    return;
                }
                egui::ScrollArea::both()
                    .id_salt("operations_compare_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Grid::new("operations_compare")
                            .num_columns(7)
                            .striped(true)
                            .spacing([10.0, 4.0])
                            .show(ui, |ui| {
                                for heading in [
                                    "Name", "Type", "Speed", "Power", "Passes", "Interval",
                                    "Enabled",
                                ] {
                                    ui.strong(heading);
                                }
                                ui.end_row();
                                for operation in &self.editor.document.operations {
                                    ui.label(&operation.name);
                                    ui.label(operation.op_type.label());
                                    ui.label(format!("{:.0} mm/min", operation.params.speed));
                                    ui.label(format!("{:.0} %", operation.params.power));
                                    ui.label(operation.params.passes.to_string());
                                    ui.label(format!(
                                        "{:.2} mm",
                                        operation.params.line_interval_mm
                                    ));
                                    ui.label(if operation.enabled { "yes" } else { "no" });
                                    ui.end_row();
                                }
                            });
                    });
            });
        self.operations_compare_open = open;
    }

    pub(super) fn guides_window(&mut self, ctx: &egui::Context) {
        let mut open = self.guides_open;
        egui::Window::new("Guides")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(360.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let position_label = ui.label("Position");
                    ui.add(
                        egui::DragValue::new(&mut self.guide_position)
                            .speed(0.5)
                            .suffix(" mm"),
                    )
                    .labelled_by(position_label.id);
                    if ui.button("Add horizontal").clicked() {
                        self.editor.add_guide(true, self.guide_position);
                    }
                    if ui.button("Add vertical").clicked() {
                        self.editor.add_guide(false, self.guide_position);
                    }
                });
                ui.separator();
                if self.editor.document.guides.is_empty() {
                    ui.weak("No guides yet.");
                }
                let mut remove: Option<String> = None;
                for guide in &self.editor.document.guides {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{} at {:.2} mm",
                            if guide.horizontal {
                                "Horizontal"
                            } else {
                                "Vertical"
                            },
                            guide.position
                        ));
                        if ui.small_button("Remove").clicked() {
                            remove = Some(guide.id.clone());
                        }
                    });
                }
                if let Some(id) = remove {
                    self.editor.remove_guide(&id);
                }
            });
        self.guides_open = open;
    }

    pub(super) fn profile_confirm_window(&mut self, ctx: &egui::Context) {
        let Some((operation_id, profile)) = self.profile_pending.clone() else {
            self.profile_confirm_open = false;
            return;
        };
        let mut open = self.profile_confirm_open;
        let mut apply = false;
        egui::Window::new(format!("Apply '{}'", profile.name))
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                let current = self
                    .editor
                    .document
                    .operations
                    .iter()
                    .find(|operation| operation.id == operation_id)
                    .map(|operation| operation.params.clone());
                let Some(current) = current else {
                    ui.weak("The operation no longer exists.");
                    return;
                };
                let changes = current.changes_to(&profile.params);
                ui.label(format!(
                    "{} parameter(s) will change in this operation:",
                    changes.len()
                ));
                egui::Grid::new("profile_changes")
                    .num_columns(3)
                    .striped(true)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        ui.strong("Parameter");
                        ui.strong("Current");
                        ui.strong("Profile");
                        ui.end_row();
                        for change in &changes {
                            ui.label(change.label);
                            ui.label(&change.from);
                            ui.label(&change.to);
                            ui.end_row();
                        }
                    });
                ui.separator();
                if ui
                    .button("Apply")
                    .on_hover_text("Apply the profile to the active operation")
                    .clicked()
                {
                    apply = true;
                }
                if ui
                    .button("Cancel")
                    .on_hover_text("Close without applying the profile")
                    .clicked()
                {
                    self.profile_confirm_open = false;
                }
            });
        if apply {
            match self
                .editor
                .apply_profile_to_operation(&operation_id, &profile)
            {
                Ok(0) => {
                    self.status = "The operation already has these values".to_string();
                }
                Ok(count) => {
                    self.status = format!("Applied '{}' ({count} parameter(s))", profile.name);
                }
                Err(error) => self.status = format!("Cannot apply profile: {error}"),
            }
            self.profile_confirm_open = false;
            self.profile_pending = None;
        } else {
            self.profile_confirm_open = open;
            if !open {
                self.profile_pending = None;
            }
        }
    }
}
