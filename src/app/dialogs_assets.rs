#![allow(clippy::wildcard_imports)]

use super::*;

impl BeamBurinApp {
    pub(super) fn material_test_window(&mut self, ctx: &egui::Context) {
        let mut open = self.material_test_open;
        let mut generate = false;
        let mut close_requested = false;
        egui::Window::new("Material test")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(
                    "One outline cell per combination. Each cell gets its own speed and power.",
                );
                ui.separator();
                egui::Grid::new("material_test_grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Columns / rows");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.material_test_options.columns)
                                    .range(1..=20),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.material_test_options.rows)
                                    .range(1..=20),
                            );
                        });
                        ui.end_row();
                        ui.label("Speed range (mm/min)");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.material_test_options.speed_min)
                                    .speed(10.0)
                                    .range(1.0..=100_000.0),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.material_test_options.speed_max)
                                    .speed(10.0)
                                    .range(1.0..=100_000.0),
                            );
                        });
                        ui.end_row();
                        ui.label("Power range (%)");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.material_test_options.power_min)
                                    .range(0.0..=100.0),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.material_test_options.power_max)
                                    .range(0.0..=100.0),
                            );
                        });
                        ui.end_row();
                        ui.label("Cell width / height (mm)");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.material_test_options.cell_width_mm)
                                    .speed(0.5)
                                    .range(1.0..=500.0),
                            );
                            ui.add(
                                egui::DragValue::new(
                                    &mut self.material_test_options.cell_height_mm,
                                )
                                .speed(0.5)
                                .range(1.0..=500.0),
                            );
                        });
                        ui.end_row();
                        ui.label("Gap / margin (mm)");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.material_test_options.gap_mm)
                                    .speed(0.5)
                                    .range(0.0..=100.0),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.material_test_options.margin_mm)
                                    .speed(0.5)
                                    .range(0.0..=100.0),
                            );
                        });
                        ui.end_row();
                        ui.label("Passes");
                        ui.add(
                            egui::DragValue::new(&mut self.material_test_options.passes)
                                .range(1..=100),
                        );
                        ui.end_row();
                    });
                ui.weak(format!(
                    "Grid: {:.1} x {:.1} mm, {} cells",
                    self.material_test_options.width_mm(),
                    self.material_test_options.height_mm(),
                    self.material_test_options.columns * self.material_test_options.rows
                ));
                if let Err(error) = self.material_test_options.validate() {
                    ui.colored_label(egui::Color32::from_rgb(200, 60, 60), error);
                }
                ui.horizontal(|ui| {
                    if ui.button("Generate").clicked() {
                        generate = true;
                    }
                    if ui
                        .button("Close")
                        .on_hover_text("Close this window")
                        .clicked()
                    {
                        close_requested = true;
                    }
                });
            });
        if close_requested {
            open = false;
        }
        if generate {
            self.editor.record_action("Material test");
            match crate::material_test::generate(
                &mut self.editor.document,
                &self.material_test_options,
            ) {
                Ok(cells) => {
                    self.status = format!("Material test: {cells} cell(s) created");
                    self.material_test_open = false;
                }
                Err(error) => self.status = format!("Material test failed: {error}"),
            }
        } else {
            self.material_test_open = open;
        }
    }

    pub(super) fn image_test_window(&mut self, ctx: &egui::Context) {
        let mut open = self.image_test_open;
        let mut generate = false;
        let mut close_requested = false;
        egui::Window::new("Image test grid")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(
                    "Copies the adjusted image once per speed (columns) and power (rows). \
                 Every copy keeps the source file, crop and preparation settings, and \
                 gets its own outline operation.",
                );
                ui.separator();
                egui::Grid::new("image_test_grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Columns / rows");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.image_test_options.columns)
                                    .range(1..=20),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.image_test_options.rows)
                                    .range(1..=20),
                            );
                        });
                        ui.end_row();
                        ui.label("Speed range (mm/min)");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.image_test_options.speed_min)
                                    .speed(10.0)
                                    .range(1.0..=100_000.0),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.image_test_options.speed_max)
                                    .speed(10.0)
                                    .range(1.0..=100_000.0),
                            );
                        });
                        ui.end_row();
                        ui.label("Power range (%)");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.image_test_options.power_min)
                                    .range(0.0..=100.0),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.image_test_options.power_max)
                                    .range(0.0..=100.0),
                            );
                        });
                        ui.end_row();
                        ui.label("Gap (mm)");
                        ui.add(
                            egui::DragValue::new(&mut self.image_test_options.gap_mm)
                                .speed(0.5)
                                .range(0.0..=100.0),
                        );
                        ui.end_row();
                    });
                ui.weak(format!(
                    "{} copies in a {} x {} grid",
                    self.image_test_options.cell_count(),
                    self.image_test_options.columns,
                    self.image_test_options.rows
                ));
                if let Err(error) = self.image_test_options.validate() {
                    ui.colored_label(egui::Color32::from_rgb(200, 60, 60), error);
                }
                ui.horizontal(|ui| {
                    if ui.button("Generate").clicked() {
                        generate = true;
                    }
                    if ui
                        .button("Close")
                        .on_hover_text("Close this window")
                        .clicked()
                    {
                        close_requested = true;
                    }
                });
            });
        if close_requested {
            open = false;
        }
        if generate {
            match self.image_test_target.clone() {
                Some(id) => {
                    let before = self.editor.document.clone();
                    match crate::image_test::generate(
                        &mut self.editor.document,
                        &id,
                        &self.image_test_options,
                    ) {
                        Ok(copies) => {
                            self.editor.history.record(&before, "Image test grid");
                            self.status = format!("Image test grid: {copies} copy/copies created");
                            open = false;
                        }
                        Err(error) => self.status = format!("Image test grid failed: {error}"),
                    }
                }
                None => self.status = "Image test grid: select an image first".to_string(),
            }
        }
        self.image_test_open = open;
    }

    pub(super) fn external_gcode_window(&mut self, ctx: &egui::Context) {
        let mut open = self.external_gcode_open;
        egui::Window::new("External G-code")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([640.0, 470.0])
            .show(ctx, |ui| {
                let Some((path, program)) = &self.external_gcode else {
                    return;
                };
                egui::Grid::new("external_gcode_summary")
                    .num_columns(2)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("File");
                        ui.label(path.display().to_string());
                        ui.end_row();
                        ui.label("Lines");
                        ui.label(program.lines.to_string());
                        ui.end_row();
                        ui.label("Cut moves");
                        ui.label(program.cut_points().to_string());
                        ui.end_row();
                        ui.label("Cut length");
                        ui.label(format!("{:.1} mm", program.cut_length_mm));
                        ui.end_row();
                        ui.label("Travel length");
                        ui.label(format!("{:.1} mm", program.travel_length_mm));
                        ui.end_row();
                        ui.label("Estimated time");
                        ui.label(
                            program
                                .estimated_seconds
                                .map_or_else(|| "unknown".to_string(), format_seconds),
                        );
                        ui.end_row();
                        ui.label("Power");
                        ui.label(match (program.power_min, program.power_max) {
                            (Some(min), Some(max)) => format!("{min:.0} to {max:.0}"),
                            _ => "not set".to_string(),
                        });
                        ui.end_row();
                        ui.label("Feed rate");
                        ui.label(match (program.feed_min, program.feed_max) {
                            (Some(min), Some(max)) => format!("{min:.0} to {max:.0} mm/min"),
                            _ => "not set".to_string(),
                        });
                        ui.end_row();
                        ui.label("Size");
                        ui.label(match program.bounds() {
                            Some((min, max)) => {
                                format!("{:.1} x {:.1} mm", max.x - min.x, max.y - min.y)
                            }
                            None => "no motion".to_string(),
                        });
                        ui.end_row();
                    });
                if !program.warnings.is_empty() {
                    ui.separator();
                    ui.strong("Notes");
                    egui::ScrollArea::vertical()
                        .id_salt("external_gcode_warnings")
                        .max_height(90.0)
                        .show(ui, |ui| {
                            for warning in &program.warnings {
                                ui.colored_label(
                                    egui::Color32::from_rgb(210, 140, 60),
                                    format!("- {warning}"),
                                );
                            }
                        });
                }
                ui.separator();
                let size = egui::vec2(ui.available_width(), ui.available_height().max(200.0));
                let (response, painter) = ui.allocate_painter(size, egui::Sense::hover());
                painter.rect_filled(response.rect, 6.0, egui::Color32::from_rgb(16, 17, 19));
                let Some((min, max)) = program.bounds() else {
                    return;
                };
                let width = (max.x - min.x).max(1e-6);
                let height = (max.y - min.y).max(1e-6);
                let available = response.rect.shrink(12.0);
                let scale = (f64::from(available.width()) / width)
                    .min(f64::from(available.height()) / height);
                let to_screen = |point: crate::geometry::Point2| {
                    egui::Pos2::new(
                        available.left() + ((point.x - min.x) * scale) as f32,
                        available.bottom() - ((point.y - min.y) * scale) as f32,
                    )
                };
                for polyline in &program.travels {
                    let points: Vec<egui::Pos2> =
                        polyline.iter().map(|point| to_screen(*point)).collect();
                    painter.add(egui::Shape::line(
                        points,
                        egui::Stroke::new(1.0, egui::Color32::from_gray(90)),
                    ));
                }
                for polyline in &program.cuts {
                    let points: Vec<egui::Pos2> =
                        polyline.iter().map(|point| to_screen(*point)).collect();
                    painter.add(egui::Shape::line(
                        points,
                        egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 122, 26)),
                    ));
                }
            });
        self.external_gcode_open = open;
    }

    pub(super) fn apply_export_preset(
        &mut self,
        preset: &crate::config::preferences::ExportPreset,
    ) {
        if let Some(dialect) = crate::gcode::Dialect::from_id(&preset.dialect) {
            self.gcode_dialect = dialect;
        }
        self.gcode_decimals = preset.decimals;
        self.gcode_comments = preset.comments;
        self.gcode_travel = preset.travel;
        self.gcode_crlf = preset.crlf;
        self.gcode_smooth_mm = preset.smooth_mm;
        self.gcode_arcs = preset.arcs;
        self.gcode_origin = preset.origin;
    }

    pub(super) fn export_presets_window(&mut self, ctx: &egui::Context) {
        let mut open = self.export_presets_open;
        let mut save = false;
        let mut apply: Option<String> = None;
        let mut delete: Option<String> = None;
        let mut close_requested = false;
        egui::Window::new("Export presets")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(
                    "An export preset stores the G-code dialect, decimals, comments, \
                 travel moves, line endings, arcs and smoothing.",
                );
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.export_preset_name)
                            .hint_text("Preset name")
                            .desired_width(200.0),
                    );
                    if ui
                        .add_enabled(
                            !self.export_preset_name.trim().is_empty(),
                            egui::Button::new("Save current options"),
                        )
                        .on_hover_text("An existing name is replaced")
                        .clicked()
                    {
                        save = true;
                    }
                });
                ui.separator();
                if self.preferences.export_presets.saved.is_empty() {
                    ui.weak("No export presets saved yet.");
                }
                for preset in &self.preferences.export_presets.saved {
                    ui.horizontal(|ui| {
                        ui.label(&preset.name);
                        ui.weak(format!(
                            "{} · {} decimals · {} smooth{}",
                            preset.dialect,
                            preset.decimals,
                            preset.smooth_mm,
                            if preset.arcs { " · arcs" } else { "" }
                        ));
                        if ui.button("Apply").clicked() {
                            apply = Some(preset.name.clone());
                        }
                        if ui.button("Delete").clicked() {
                            delete = Some(preset.name.clone());
                        }
                    });
                }
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button("Close")
                        .on_hover_text("Close this window")
                        .clicked()
                    {
                        close_requested = true;
                    }
                });
            });
        let mut stored = false;
        if save {
            let name = self.export_preset_name.trim().to_string();
            let preset = crate::config::preferences::ExportPreset {
                name: name.clone(),
                dialect: self.gcode_dialect.id().to_string(),
                decimals: self.gcode_decimals,
                comments: self.gcode_comments,
                travel: self.gcode_travel,
                crlf: self.gcode_crlf,
                smooth_mm: self.gcode_smooth_mm,
                arcs: self.gcode_arcs,
                origin: self.gcode_origin,
            };
            match self.preferences.export_presets.upsert(preset) {
                Ok(()) => {
                    self.status = format!("Export preset '{name}' saved");
                    stored = true;
                }
                Err(error) => self.status = error,
            }
        }
        if let Some(name) = apply {
            if let Some(preset) = self.preferences.export_presets.find(&name).cloned() {
                self.apply_export_preset(&preset);
                self.status = format!("Export preset '{name}' applied");
            }
        }
        if let Some(name) = delete {
            if self.preferences.export_presets.remove(&name) {
                self.status = format!("Export preset '{name}' deleted");
                stored = true;
            }
        }
        if stored {
            match self.preferences.save(&self.preferences_path) {
                Ok(()) => {}
                Err(error) => self.status = format!("Cannot save preferences: {error}"),
            }
        }
        if close_requested {
            open = false;
        }
        self.export_presets_open = open;
    }

    pub(super) fn registration_marks_window(&mut self, ctx: &egui::Context) {
        let mut open = self.regmarks_open;
        let mut generate = false;
        let mut close_requested = false;
        egui::Window::new("Registration marks")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(
                    "Adds four marks around the selection, or around the whole drawing \
                 when nothing is selected. They are ordinary paths and stay outside \
                 the artwork; alignment itself is still manual.",
                );
                ui.add_space(4.0);
                egui::Grid::new("registration_marks_grid")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Size (mm)");
                        ui.add(
                            egui::DragValue::new(&mut self.regmarks_options.size_mm)
                                .speed(0.5)
                                .range(0.5..=100.0),
                        );
                        ui.end_row();
                        ui.label("Margin (mm)");
                        ui.add(
                            egui::DragValue::new(&mut self.regmarks_options.margin_mm)
                                .speed(0.5)
                                .range(0.0..=200.0),
                        );
                        ui.end_row();
                        ui.label("Style");
                        let mut style = self.regmarks_options.style;
                        egui::ComboBox::from_id_salt("registration_mark_style")
                            .selected_text(style.label())
                            .show_ui(ui, |ui| {
                                for candidate in crate::regmarks::MarkStyle::all() {
                                    ui.selectable_value(&mut style, candidate, candidate.label());
                                }
                            });
                        self.regmarks_options.style = style;
                        ui.end_row();
                    });
                if let Err(error) = self.regmarks_options.validate() {
                    ui.colored_label(egui::Color32::from_rgb(200, 60, 60), error);
                }
                ui.horizontal(|ui| {
                    if ui.button("Add marks").clicked() {
                        generate = true;
                    }
                    if ui
                        .button("Close")
                        .on_hover_text("Close this window")
                        .clicked()
                    {
                        close_requested = true;
                    }
                });
            });
        if generate {
            let bounds = self.editor.selected_bounds().or_else(|| {
                self.editor.document.bounds().or_else(|| {
                    Some((
                        Point2::new(0.0, 0.0),
                        Point2::new(
                            self.editor.document.width_mm,
                            self.editor.document.height_mm,
                        ),
                    ))
                })
            });
            match bounds {
                Some(bounds) => match crate::regmarks::marks(bounds, &self.regmarks_options) {
                    Ok(outlines) => {
                        self.editor.record_action("Registration marks");
                        let layer_id = self
                            .editor
                            .document
                            .add_layer("Registration marks")
                            .id
                            .clone();
                        let operation = self.active_operation.clone();
                        let mut added = 0;
                        for outline in &outlines {
                            let mut object = crate::document::Object::new(
                                layer_id.clone(),
                                crate::document::Shape::Path(crate::centerline::to_path(outline)),
                            );
                            object.name = "Registration mark".to_string();
                            object.operation_id.clone_from(&operation);
                            object.style.stroke = Some("#000000".to_string());
                            object.style.fill = None;
                            self.editor.document.add_object(object);
                            added += 1;
                        }
                        self.status = format!("Added {added} registration mark(s)");
                        open = false;
                    }
                    Err(error) => self.status = error,
                },
                None => self.status = "There is nothing to put marks around".to_string(),
            }
        }
        if close_requested {
            open = false;
        }
        self.regmarks_open = open;
    }

    pub(super) fn handle_live_request(&mut self, request: &str) -> String {
        let request: serde_json::Value = match serde_json::from_str(request) {
            Ok(value) => value,
            Err(error) => {
                return serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {"code": -32700, "message": format!("invalid JSON: {error}")}
                })
                .to_string();
            }
        };
        let placeholder = crate::editor::EditorState::new(Document::default());
        let app_editor = std::mem::replace(&mut self.editor, placeholder);
        let old = self.live_server.replace_editor(app_editor);
        let response = self.live_server.handle(&request);
        let app_editor = self.live_server.replace_editor(old);
        self.editor = app_editor;
        response.to_string()
    }
}
