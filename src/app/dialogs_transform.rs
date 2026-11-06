#![allow(clippy::wildcard_imports)]

use super::*;

impl BeamBurinApp {
    /// Open the position dialog with the selection's current corner.
    pub(super) fn open_position(&mut self) {
        let Some((min, _)) = self.editor.selected_bounds() else {
            self.status = "Select something to move".to_string();
            return;
        };
        self.position_x = min.x;
        self.position_y = min.y;
        self.position_open = true;
    }

    /// Open the corner-radius dialog with the first selected rectangle's value.
    pub(super) fn open_corner_radius(&mut self) {
        let value = self
            .editor
            .selection
            .ids()
            .iter()
            .filter_map(|id| self.editor.document.object(id))
            .find_map(|object| match object.shape {
                crate::document::Shape::Rect { corner_radius, .. } => Some(corner_radius),
                _ => None,
            })
            .unwrap_or(0.0);
        self.corner_radius_value = value;
        self.corner_radius_open = true;
    }

    pub(super) fn corner_radius_window(&mut self, ctx: &egui::Context) {
        let mut open = self.corner_radius_open;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new("Corner radius")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
                if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    apply = true;
                }
                ui.horizontal(|ui| {
                    ui.label("Radius");
                    ui.add(
                        egui::DragValue::new(&mut self.corner_radius_value)
                            .speed(0.1)
                            .range(0.0..=100_000.0)
                            .suffix(" mm"),
                    );
                });
                ui.weak("Applies to every selected rectangle, up to half the smaller side.");
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        apply = true;
                    }
                    if ui.button("Clear corners").clicked() {
                        self.corner_radius_value = 0.0;
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if apply {
            let count = self.editor.set_corner_radius(self.corner_radius_value);
            if count == 0 {
                self.status = "Select at least one rectangle".to_string();
            } else {
                self.status = format!("Set the corner radius on {count} rectangle(s)");
                self.corner_radius_open = false;
            }
        } else if cancel {
            self.corner_radius_open = false;
        } else {
            self.corner_radius_open = open;
        }
    }

    pub(super) fn barcode_window(&mut self, ctx: &egui::Context) {
        let mut open = self.barcode_open;
        let mut insert = false;
        let mut cancel = false;
        egui::Window::new("Barcode")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
                if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    insert = true;
                }
                ui.horizontal(|ui| {
                    ui.label("Text");
                    ui.add(egui::TextEdit::singleline(&mut self.barcode_text).desired_width(240.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Module (mm)");
                    ui.add(
                        egui::DragValue::new(&mut self.barcode_module_mm)
                            .speed(0.05)
                            .range(0.1..=5.0)
                            .fixed_decimals(2),
                    );
                    ui.label("Height (mm)");
                    ui.add(
                        egui::DragValue::new(&mut self.barcode_height_mm)
                            .speed(0.5)
                            .range(1.0..=200.0)
                            .fixed_decimals(1),
                    );
                });
                ui.weak("Code 39: A-Z, 0-9, space and - . $ / + %. Lower case becomes upper case.");
                ui.horizontal(|ui| {
                    if ui.button("Insert").clicked() {
                        insert = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if insert {
            match self.editor.insert_barcode(
                &self.barcode_text,
                self.barcode_module_mm,
                self.barcode_height_mm,
            ) {
                Ok(count) => {
                    self.status = format!("Inserted a barcode with {count} bars");
                    self.barcode_open = false;
                }
                Err(error) => self.status = format!("Barcode failed: {error}"),
            }
        } else if cancel {
            self.barcode_open = false;
        } else {
            self.barcode_open = open;
        }
    }

    pub(super) fn position_window(&mut self, ctx: &egui::Context) {
        let mut open = self.position_open;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new("Move to position")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
                if ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                    apply = true;
                }
                ui.horizontal(|ui| {
                    ui.label("X");
                    ui.add(
                        egui::DragValue::new(&mut self.position_x)
                            .speed(0.5)
                            .suffix(" mm"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Y");
                    ui.add(
                        egui::DragValue::new(&mut self.position_y)
                            .speed(0.5)
                            .suffix(" mm"),
                    );
                });
                ui.weak("Places the bottom-left corner of the selection at X / Y.");
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        apply = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if apply {
            if self
                .editor
                .move_selection_to(self.position_x, self.position_y)
            {
                self.status = format!(
                    "Moved the selection to {:.1}, {:.1} mm",
                    self.position_x, self.position_y
                );
                self.position_open = false;
            } else {
                self.status =
                    "Move to position failed: select something and change X or Y".to_string();
            }
        } else if cancel {
            self.position_open = false;
        } else {
            self.position_open = open;
        }
    }

    pub(super) fn skew_window(&mut self, ctx: &egui::Context) {
        let mut open = self.skew_open;
        let mut apply = false;
        egui::Window::new("Skew")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Horizontal");
                    ui.add(
                        egui::DragValue::new(&mut self.skew_x)
                            .speed(1.0)
                            .range(-80.0..=80.0)
                            .suffix(" deg"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Vertical");
                    ui.add(
                        egui::DragValue::new(&mut self.skew_y)
                            .speed(1.0)
                            .range(-80.0..=80.0)
                            .suffix(" deg"),
                    );
                });
                ui.weak("The shear is applied around the selection centre.");
                if ui.button("Apply").clicked() {
                    apply = true;
                }
            });
        if apply {
            if self.editor.skew_selection(self.skew_x, self.skew_y) {
                self.status = "Selection skewed".to_string();
                self.skew_open = false;
            } else {
                self.status = "Skew failed: select something and use a non-zero angle".to_string();
            }
        } else {
            self.skew_open = open;
        }
    }

    pub(super) fn repeat_window(&mut self, ctx: &egui::Context) {
        use crate::editor::RepeatMode;

        let mut open = self.repeat_open;
        let mut apply = false;
        egui::Window::new("Repeat selection")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::ComboBox::from_label("Mode")
                    .selected_text(match self.repeat_options.mode {
                        RepeatMode::Linear => "Linear",
                        RepeatMode::Circular => "Circular",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.repeat_options.mode,
                            RepeatMode::Linear,
                            "Linear",
                        );
                        ui.selectable_value(
                            &mut self.repeat_options.mode,
                            RepeatMode::Circular,
                            "Circular",
                        );
                    });
                ui.horizontal(|ui| {
                    ui.label("Copies");
                    ui.add(
                        egui::DragValue::new(&mut self.repeat_options.copies)
                            .speed(1.0)
                            .range(1..=100),
                    );
                });
                match self.repeat_options.mode {
                    RepeatMode::Linear => {
                        ui.horizontal(|ui| {
                            ui.label("Spacing X");
                            ui.add(
                                egui::DragValue::new(&mut self.repeat_options.dx)
                                    .speed(0.5)
                                    .suffix(" mm"),
                            );
                            ui.label("Y");
                            ui.add(
                                egui::DragValue::new(&mut self.repeat_options.dy)
                                    .speed(0.5)
                                    .suffix(" mm"),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("Rotation per copy");
                            ui.add(
                                egui::DragValue::new(&mut self.repeat_options.rotation_degrees)
                                    .speed(1.0)
                                    .suffix(" deg"),
                            );
                        });
                    }
                    RepeatMode::Circular => {
                        ui.horizontal(|ui| {
                            ui.label("Radius");
                            ui.add(
                                egui::DragValue::new(&mut self.repeat_options.radius)
                                    .speed(0.5)
                                    .range(0.0..=10000.0)
                                    .suffix(" mm"),
                            );
                        });
                        ui.checkbox(
                            &mut self.repeat_options.rotate_copies,
                            "Rotate copies along the orbit",
                        );
                    }
                }
                ui.weak("Copies are spaced evenly and stay separate from the original.");
                if ui.button("Apply").clicked() {
                    apply = true;
                }
            });
        if apply {
            match self.editor.repeat_selection(&self.repeat_options) {
                Ok(count) => {
                    self.status = format!("Created {count} copy/copies");
                    self.repeat_open = false;
                }
                Err(error) => self.status = format!("Repeat failed: {error}"),
            }
        } else {
            self.repeat_open = open;
        }
    }

    pub(super) fn open_nest(&mut self) {
        self.nest_open = true;
        self.refresh_nest();
    }

    pub(super) fn refresh_nest(&mut self) {
        match self
            .editor
            .nest_selection(self.nest_spacing, self.nest_margin, self.nest_rotation)
        {
            Ok(proposal) => {
                self.nest_message.clear();
                self.nest_proposal = Some(proposal);
            }
            Err(error) => {
                self.nest_message = error;
                self.nest_proposal = None;
            }
        }
    }

    pub(super) fn nest_window(&mut self, ctx: &egui::Context) {
        let mut open = self.nest_open;
        let mut preview = false;
        let mut apply = false;
        egui::Window::new("Nest selection")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Work area: {:.0} x {:.0} mm",
                    self.editor.document.width_mm, self.editor.document.height_mm
                ));
                ui.horizontal(|ui| {
                    ui.label("Spacing");
                    if ui
                        .add(
                            egui::DragValue::new(&mut self.nest_spacing)
                                .speed(0.5)
                                .range(0.0..=100.0)
                                .suffix(" mm"),
                        )
                        .changed()
                    {
                        preview = true;
                    }
                    ui.label("Margin");
                    if ui
                        .add(
                            egui::DragValue::new(&mut self.nest_margin)
                                .speed(0.5)
                                .range(0.0..=100.0)
                                .suffix(" mm"),
                        )
                        .changed()
                    {
                        preview = true;
                    }
                    if ui
                        .checkbox(&mut self.nest_rotation, "Allow 90 deg rotation")
                        .changed()
                    {
                        preview = true;
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Preview").clicked() {
                        preview = true;
                    }
                    let can_apply = self
                        .nest_proposal
                        .as_ref()
                        .is_some_and(|proposal| !proposal.placements.is_empty());
                    if ui
                        .add_enabled(can_apply, egui::Button::new("Apply"))
                        .on_hover_text("Apply the nesting proposal")
                        .clicked()
                    {
                        apply = true;
                    }
                    if ui
                        .button("Cancel")
                        .on_hover_text("Close without applying the nesting")
                        .clicked()
                    {
                        self.nest_open = false;
                    }
                });
                if !self.nest_message.is_empty() {
                    ui.colored_label(Color32::from_rgb(190, 60, 50), &self.nest_message);
                }
                if let Some(proposal) = &self.nest_proposal {
                    ui.separator();
                    ui.label(format!(
                        "Placed {} of {} parts, {:.0} % of the available area",
                        proposal.placements.len(),
                        proposal.placements.len() + proposal.unplaced.len(),
                        proposal.utilization() * 100.0
                    ));
                    for placement in &proposal.placements {
                        let name = self
                            .editor
                            .document
                            .object(&placement.object_id)
                            .map_or("part", |object| object.name.as_str());
                        ui.weak(format!(
                            "{} -> ({:.1}, {:.1}) mm{}{}",
                            name,
                            placement.min.x,
                            placement.min.y,
                            if placement.rotated { ", rotated" } else { "" },
                            " - preview on the canvas"
                        ));
                    }
                    if !proposal.unplaced.is_empty() {
                        ui.colored_label(
                            Color32::from_rgb(180, 120, 0),
                            format!("{} part(s) do not fit", proposal.unplaced.len()),
                        );
                        for id in &proposal.unplaced {
                            if let Some(object) = self.editor.document.object(id) {
                                ui.colored_label(Color32::from_rgb(180, 120, 0), &object.name);
                            }
                        }
                    }
                }
            });
        if preview {
            self.refresh_nest();
        }
        if apply {
            if let Some(proposal) = self.nest_proposal.clone() {
                let moved = self.editor.apply_nest(&proposal);
                self.status = format!("Nested {moved} part(s)");
                self.nest_open = false;
                self.nest_proposal = None;
            }
        }
        if !self.nest_open {
            self.nest_proposal = None;
        }
        self.nest_open = self.nest_open && open;
    }

    pub(super) fn simplify_window(&mut self, ctx: &egui::Context) {
        let mut open = self.simplify_open;
        let mut apply = false;
        egui::Window::new("Simplify path")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Tolerance");
                ui.add(
                    egui::DragValue::new(&mut self.simplify_tolerance)
                        .speed(0.05)
                        .range(0.001..=5.0)
                        .suffix(" mm"),
                );
                ui.weak("Curves are flattened and straightened within the tolerance.");
                if ui.button("Apply").clicked() {
                    apply = true;
                }
            });
        if apply {
            match self.editor.simplify_selected_path(self.simplify_tolerance) {
                Ok(nodes) => {
                    self.status = format!("Simplified to {nodes} node(s)");
                    self.simplify_open = false;
                }
                Err(error) => self.status = format!("Simplify failed: {error}"),
            }
        } else {
            self.simplify_open = open;
        }
    }

    pub(super) fn offset_window(&mut self, ctx: &egui::Context) {
        let mut open = self.offset_open;
        let mut apply = false;
        egui::Window::new("Offset")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Distance");
                ui.add(
                    egui::DragValue::new(&mut self.offset_distance)
                        .speed(0.1)
                        .range(-100.0..=100.0)
                        .suffix(" mm"),
                );
                ui.weak("Positive expands the outline; negative contracts it.");
                ui.weak("Shapes that collapse are removed.");
                if ui.button("Apply").clicked() {
                    apply = true;
                }
            });
        if apply {
            match self.editor.offset_selection(self.offset_distance) {
                Ok(count) => {
                    self.status = format!("Offset applied to {count} shape(s)");
                    self.offset_open = false;
                }
                Err(error) => self.status = format!("Offset failed: {error}"),
            }
        } else {
            self.offset_open = open;
        }
    }

    pub(super) fn grid_array_window(&mut self, ctx: &egui::Context) {
        let mut open = self.grid_array_open;
        egui::Window::new("Grid array")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let options = &mut self.grid_array;
                ui.horizontal(|ui| {
                    ui.label("Columns");
                    ui.add(
                        egui::DragValue::new(&mut options.columns)
                            .speed(1.0)
                            .range(1..=100),
                    );
                    ui.label("Rows");
                    ui.add(
                        egui::DragValue::new(&mut options.rows)
                            .speed(1.0)
                            .range(1..=100),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Spacing X");
                    ui.add(
                        egui::DragValue::new(&mut options.dx)
                            .speed(0.5)
                            .range(-10000.0..=10000.0)
                            .suffix(" mm"),
                    );
                    ui.label("Y");
                    ui.add(
                        egui::DragValue::new(&mut options.dy)
                            .speed(0.5)
                            .range(-10000.0..=10000.0)
                            .suffix(" mm"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Margin X");
                    ui.add(
                        egui::DragValue::new(&mut options.margin_x)
                            .speed(0.5)
                            .range(-10000.0..=10000.0)
                            .suffix(" mm"),
                    );
                    ui.label("Y");
                    ui.add(
                        egui::DragValue::new(&mut options.margin_y)
                            .speed(0.5)
                            .range(-10000.0..=10000.0)
                            .suffix(" mm"),
                    );
                });
                ui.checkbox(&mut options.column_major, "Fill column by column");
                let options = *options;
                ui.label(format!(
                    "Creates {} copies of the {} selected object(s).",
                    options
                        .columns
                        .saturating_mul(options.rows)
                        .saturating_sub(1),
                    self.editor.selection.len()
                ));
                if ui
                    .add_enabled(
                        !self.editor.selection.is_empty(),
                        egui::Button::new("Create array"),
                    )
                    .clicked()
                {
                    let created = self.editor.duplicate_grid_with(&options);
                    self.status = format!("Created {created} copies");
                    self.grid_array_open = false;
                }
            });
        self.grid_array_open = open && self.grid_array_open;
    }

    pub(super) fn current_workspace(&self, name: &str) -> crate::config::preferences::Workspace {
        crate::config::preferences::Workspace {
            name: name.trim().to_string(),
            layers_panel: self.show_layers_panel,
            properties_panel: self.show_properties_panel,
            rulers: self.editor.show_rulers,
            grid: self.editor.canvas.grid_visible,
            guides: self.editor.guides_visible,
            toolpaths: self.preview_toolpaths,
            tool_panel: self.show_tool_panel,
        }
    }

    pub(super) fn apply_workspace(&mut self, workspace: &crate::config::preferences::Workspace) {
        self.show_layers_panel = workspace.layers_panel;
        self.show_properties_panel = workspace.properties_panel;
        self.editor.show_rulers = workspace.rulers;
        self.editor.canvas.grid_visible = workspace.grid;
        self.editor.guides_visible = workspace.guides;
        self.preview_toolpaths = workspace.toolpaths;
        self.show_tool_panel = workspace.tool_panel;
        self.preferences.drawing.show_rulers = workspace.rulers;
        self.preferences.drawing.guides_visible = workspace.guides;
    }

    pub(super) fn run_palette_action(&mut self, action: PaletteAction) {
        use crate::commands::CommandId as Action;
        self.palette_query.clear();
        match action {
            Action::New => self.request(PendingAction::New),
            Action::Open => self.request(PendingAction::Open),
            Action::Save => self.save(),
            Action::SaveAs => self.open_dialog(FileMode::SaveProject),
            Action::SaveArchive => self.open_dialog(FileMode::SaveArchive),
            Action::Undo => self.undo(),
            Action::Redo => self.redo(),
            Action::Cut => self.cut(),
            Action::Copy => self.copy(),
            Action::Paste => self.paste_from_clipboard(),
            Action::Duplicate => self.editor.duplicate_selection(),
            Action::Delete => {
                if self.editor.is_editing_nodes() && !self.editor.node_selection.is_empty() {
                    let removed = self.editor.delete_selected_nodes();
                    self.status = format!("Deleted {removed} node(s)");
                } else {
                    self.editor.delete_selection();
                }
            }
            Action::SelectAll => self.editor.select_all(),
            Action::SelectNone => self.editor.selection.clear(),
            Action::SelectLayer => {
                let layer = self.editor.active_layer_id.clone();
                let count = self.editor.select_layer_objects(&layer);
                self.status = format!("Selected {count} object(s) on the active layer");
            }
            Action::SelectContained => {
                let count = self.editor.select_contained_shapes();
                self.status = format!("Selected {count} contained object(s)");
            }
            Action::SelectOpenShapes => self.editor.select_open_shapes(),
            Action::SetPosition => self.open_position(),
            Action::ClosePaths => self.apply_close_paths(),
            Action::JoinPaths => self.apply_join_paths(),
            Action::ConvertToPath => {
                let count = self.editor.convert_selection_to_paths();
                self.status = format!("Converted {count} shape(s) to paths");
            }
            Action::CornerRadius => self.open_corner_radius(),
            Action::ClearCornerRadius => {
                let count = self.editor.set_corner_radius(0.0);
                self.status = format!("Squared the corners of {count} rectangle(s)");
            }
            Action::BooleanUnion => self.apply_boolean(crate::geometry::BooleanOp::Union),
            Action::BooleanDifference => self.apply_boolean(crate::geometry::BooleanOp::Difference),
            Action::BooleanIntersection => {
                self.apply_boolean(crate::geometry::BooleanOp::Intersection);
            }
            Action::BooleanExclusion => self.apply_boolean(crate::geometry::BooleanOp::Xor),
            Action::Weld => match self.editor.weld_selection() {
                Ok(count) => self.status = format!("Welded into {count} contour(s)"),
                Err(error) => self.status = format!("Weld failed: {error}"),
            },
            Action::Offset => self.offset_open = true,
            Action::Repeat => self.repeat_open = true,
            Action::Skew => self.skew_open = true,
            Action::Nest => self.open_nest(),
            Action::BringForward => {
                let moved = self.editor.raise_selection();
                self.status = format!("Brought {moved} object(s) forward");
            }
            Action::SendBackward => {
                let moved = self.editor.lower_selection();
                self.status = format!("Sent {moved} object(s) backward");
            }
            Action::ZoomIn => self.editor.zoom_in(),
            Action::ZoomOut => self.editor.zoom_out(),
            Action::FitPage => self.needs_fit = true,
            Action::ToggleGrid => {
                self.editor.canvas.grid_visible = !self.editor.canvas.grid_visible;
            }
            Action::ToggleSnap => self.editor.snap_enabled = !self.editor.snap_enabled,
            Action::ToggleToolpaths => self.preview_toolpaths = !self.preview_toolpaths,
            Action::ToolSelect => self.editor.tool = Tool::Select,
            Action::ToolLine => self.editor.tool = Tool::Line,
            Action::ToolRectangle => self.editor.tool = Tool::Rect,
            Action::ToolEllipse => self.editor.tool = Tool::Ellipse,
            Action::ToolStar => self.editor.tool = Tool::Star,
            Action::ToolText => self.editor.tool = Tool::Text,
            Action::ToolPolygon => self.editor.tool = Tool::Polygon,
            Action::ToolPencil => self.editor.tool = Tool::Pencil,
            Action::Import => self.open_dialog(FileMode::ImportFile),
            Action::ExportSvg => self.open_dialog(FileMode::ExportSvg),
            Action::ExportDxf => self.open_dialog(FileMode::ExportDxf),
            Action::ExportEps => self.open_dialog(FileMode::ExportEps),
            Action::ExportPdf => self.open_dialog(FileMode::ExportPdf),
            Action::ExportImage => self.open_dialog(FileMode::ExportImage),
            Action::ExportGcode => self.open_dialog(FileMode::ExportGcode),
            Action::Cuts => self.open_cuts(),
            Action::Parameters => self.open_parameters(),
            Action::Constraints => self.open_constraints(),
            Action::Preview => self.open_preview(),
            Action::MaterialTest => self.material_test_open = true,
            Action::RegistrationMarks => self.regmarks_open = true,
            Action::DocumentProperties => {
                self.doc_properties_snapshot = Some(self.editor.document.clone());
                self.doc_properties_open = true;
            }
            Action::Preferences => self.preferences_open = true,
            Action::Workspaces => self.workspaces_open = true,
            Action::InterfacePresets => self.interface_presets_open = true,
            Action::History => self.history_open = true,
            Action::MachineProfiles => self.machines_open = true,
            Action::Materials => self.profiles_open = true,
            Action::KeyboardShortcuts => self.help_shortcuts_open = true,
            Action::Assistant => self.assistant_open = true,
        }
    }
}
