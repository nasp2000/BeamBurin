#![allow(clippy::wildcard_imports)]

use super::*;

fn layer_rows(document: &crate::document::Document) -> Vec<(String, String, bool)> {
    document
        .layers
        .iter()
        .map(|layer| (layer.id.clone(), layer.name.clone(), layer.visible))
        .collect()
}

fn refresh_layer_selection(selected: &mut Vec<String>, layers: &[(String, String, bool)]) {
    selected.retain(|id| layers.iter().any(|(value, _, _)| value == id));
    if selected.is_empty() {
        selected.extend(layers.iter().map(|(id, _, _)| id.clone()));
    }
}

fn layer_picker(
    ui: &mut egui::Ui,
    layers: &[(String, String, bool)],
    selected: &mut Vec<String>,
    active_id: &str,
) {
    let all: Vec<String> = layers.iter().map(|(id, _, _)| id.clone()).collect();
    ui.horizontal(|ui| {
        if ui.small_button("All").clicked() {
            selected.clone_from(&all);
        }
        if ui
            .add_enabled(
                !active_id.is_empty(),
                egui::Button::new("Only active").small(),
            )
            .clicked()
        {
            *selected = vec![active_id.to_string()];
        }
    });
    egui::ScrollArea::vertical()
        .max_height(120.0)
        .show(ui, |ui| {
            for (id, name, visible) in layers {
                let mut checked = selected.iter().any(|value| value == id);
                let label = if *visible {
                    name.clone()
                } else {
                    format!("{name} (hidden)")
                };
                if ui.checkbox(&mut checked, label).changed() {
                    if checked {
                        selected.push(id.clone());
                    } else {
                        selected.retain(|value| value != id);
                    }
                }
            }
        });
}

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

    pub(super) fn apply_image_export_preset(
        &mut self,
        preset: &crate::config::preferences::ImageExportPreset,
    ) {
        self.export_image_dpi = preset.dpi;
        self.export_image_transparent = preset.transparent;
        self.export_image_margin_mm = preset.margin_mm;
        self.export_image_offset_x_mm = preset.offset_x_mm;
        self.export_image_offset_y_mm = preset.offset_y_mm;
        self.export_image_custom_size = preset.custom_size;
        self.export_image_width_px = preset.width_px;
        self.export_image_height_px = preset.height_px;
        self.export_image_keep_aspect = preset.keep_aspect;
    }

    /// Region a vector export writes, including the selection margin.
    pub(super) fn vector_export_region(
        &self,
        selection: bool,
        has_selection: bool,
    ) -> crate::export::Area {
        use crate::export::Area;

        let document = &self.editor.document;
        let page = Area {
            x: 0.0,
            y: 0.0,
            width: document.width_mm,
            height: document.height_mm,
        };
        let (area, margin) = if selection && has_selection {
            match self.editor.selected_bounds() {
                Some((min, max)) => (
                    Area {
                        x: min.x,
                        y: min.y,
                        width: (max.x - min.x).max(0.01),
                        height: (max.y - min.y).max(0.01),
                    },
                    self.export_vector_margin_mm,
                ),
                None => (page, 0.0),
            }
        } else {
            (page, 0.0)
        };
        crate::export::expand_area(area, margin, document.width_mm, document.height_mm)
    }

    pub(super) fn apply_vector_export_preset(
        &mut self,
        preset: &crate::config::preferences::VectorExportPreset,
    ) {
        self.export_vector_margin_mm = preset.margin_mm;
        match preset.format.as_str() {
            "svg" => self.export_svg_text_paths = preset.text_as_paths,
            "dxf" => {
                self.export_dxf_text_paths = preset.text_as_paths;
                self.export_dxf_arcs = preset.arcs;
                self.export_dxf_unit =
                    crate::export::DxfUnit::from_id(&preset.dxf_unit).unwrap_or_default();
            }
            _ => self.export_vector_text_paths = preset.text_as_paths,
        }
    }

    /// Open the DXF export options; `selection` preselects the origin crop.
    pub(super) fn open_export_dxf(&mut self, selection: bool) {
        self.export_dxf_selection = selection && !self.editor.selection.is_empty();
        let layers = layer_rows(&self.editor.document);
        refresh_layer_selection(&mut self.export_dxf_layer_ids, &layers);
        self.export_dxf_open = true;
    }

    pub(super) fn export_dxf_window(&mut self, ctx: &egui::Context) {
        let mut open = self.export_dxf_open;
        let mut start = false;
        let mut cancel = false;
        let has_selection = !self.editor.selection.is_empty();
        let layers = layer_rows(&self.editor.document);
        let active_layer_id = self.editor.active_layer_id.clone();
        let region = self.vector_export_region(self.export_dxf_selection, has_selection);
        let (width_mm, height_mm) = (region.width, region.height);
        egui::Window::new("Export DXF")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
                ui.label("Origin");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.export_dxf_selection, false, "Whole page");
                    if ui
                        .add_enabled(
                            has_selection,
                            egui::RadioButton::new(
                                self.export_dxf_selection && has_selection,
                                "Selection",
                            ),
                        )
                        .clicked()
                    {
                        self.export_dxf_selection = true;
                    }
                });
                if self.export_dxf_selection && !has_selection {
                    self.export_dxf_selection = false;
                }
                vector_margin_row(
                    ui,
                    self.export_dxf_selection && has_selection,
                    &mut self.export_vector_margin_mm,
                );
                ui.checkbox(&mut self.export_dxf_text_paths, "Text as paths");
                ui.checkbox(&mut self.export_dxf_arcs, "Circles and ellipses as curves");
                ui.horizontal(|ui| {
                    ui.label("Units");
                    egui::ComboBox::from_id_salt("dxf_export_unit")
                        .selected_text(self.export_dxf_unit.label())
                        .show_ui(ui, |ui| {
                            for unit in [
                                crate::export::DxfUnit::Millimetres,
                                crate::export::DxfUnit::Inches,
                            ] {
                                ui.selectable_value(&mut self.export_dxf_unit, unit, unit.label());
                            }
                        });
                });
                ui.label("Layers");
                layer_picker(
                    ui,
                    &layers,
                    &mut self.export_dxf_layer_ids,
                    &active_layer_id,
                );
                ui.weak(format!("{width_mm:.1} x {height_mm:.1} mm"));
                ui.weak("Selected layers only; curves and text outlines use straight segments.");
                let can_export = !self.export_dxf_layer_ids.is_empty();
                if !can_export {
                    ui.weak("Select at least one layer.");
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(can_export, egui::Button::new("Export"))
                        .clicked()
                    {
                        start = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if start {
            self.export_dxf_open = false;
            self.export_subset = self.export_dxf_selection && has_selection;
            self.open_dialog(FileMode::ExportDxf);
        } else if cancel {
            self.export_dxf_open = false;
        } else {
            self.export_dxf_open = open;
        }
    }

    /// Open the PDF, EPS or HPGL options; `mode` picks the format.
    pub(super) fn open_export_vector(&mut self, mode: FileMode, selection: bool) {
        self.export_vector_mode = mode;
        self.export_vector_selection = selection && !self.editor.selection.is_empty();
        let layers = layer_rows(&self.editor.document);
        refresh_layer_selection(&mut self.export_vector_layer_ids, &layers);
        self.export_vector_open = true;
    }

    pub(super) fn export_vector_window(&mut self, ctx: &egui::Context) {
        let mut open = self.export_vector_open;
        let mut start = false;
        let mut cancel = false;
        let mode = self.export_vector_mode;
        let title = match mode {
            FileMode::ExportEps => "Export EPS",
            FileMode::ExportHpgl => "Export HPGL",
            _ => "Export PDF",
        };
        let hpgl = mode == FileMode::ExportHpgl;
        let has_selection = !self.editor.selection.is_empty();
        let layers = layer_rows(&self.editor.document);
        let active_layer_id = self.editor.active_layer_id.clone();
        let region = self.vector_export_region(self.export_vector_selection, has_selection);
        let (width_mm, height_mm) = (region.width, region.height);
        egui::Window::new(title)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
                ui.label(if hpgl { "Origin" } else { "Area" });
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.export_vector_selection, false, "Whole page");
                    if ui
                        .add_enabled(
                            has_selection,
                            egui::RadioButton::new(
                                self.export_vector_selection && has_selection,
                                "Selection",
                            ),
                        )
                        .clicked()
                    {
                        self.export_vector_selection = true;
                    }
                });
                if self.export_vector_selection && !has_selection {
                    self.export_vector_selection = false;
                }
                vector_margin_row(
                    ui,
                    self.export_vector_selection && has_selection,
                    &mut self.export_vector_margin_mm,
                );
                ui.checkbox(&mut self.export_vector_text_paths, "Text as paths");
                ui.label("Layers");
                layer_picker(
                    ui,
                    &layers,
                    &mut self.export_vector_layer_ids,
                    &active_layer_id,
                );
                ui.weak(format!("{width_mm:.1} x {height_mm:.1} mm"));
                if hpgl {
                    ui.weak(
                        "Selected layers only; curves and text outlines are flattened and the region starts at 0,0.",
                    );
                } else {
                    ui.weak(
                        "Selected layers only; the page is the region and curves are flattened.",
                    );
                }
                let can_export = !self.export_vector_layer_ids.is_empty();
                if !can_export {
                    ui.weak("Select at least one layer.");
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(can_export, egui::Button::new("Export")).clicked() {
                        start = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if start {
            self.export_vector_open = false;
            self.export_subset = self.export_vector_selection && has_selection;
            self.open_dialog(mode);
        } else if cancel {
            self.export_vector_open = false;
        } else {
            self.export_vector_open = open;
        }
    }

    /// Open the SVG export options; `selection` preselects the crop.
    pub(super) fn open_export_svg(&mut self, selection: bool) {
        self.export_svg_selection = selection && !self.editor.selection.is_empty();
        let layers = layer_rows(&self.editor.document);
        refresh_layer_selection(&mut self.export_svg_layer_ids, &layers);
        self.export_svg_open = true;
    }

    pub(super) fn export_svg_window(&mut self, ctx: &egui::Context) {
        let mut open = self.export_svg_open;
        let mut start = false;
        let mut cancel = false;
        let has_selection = !self.editor.selection.is_empty();
        let layers = layer_rows(&self.editor.document);
        let active_layer_id = self.editor.active_layer_id.clone();
        let region = self.vector_export_region(self.export_svg_selection, has_selection);
        let (width_mm, height_mm) = (region.width, region.height);
        egui::Window::new("Export SVG")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
                ui.label("Area");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.export_svg_selection, false, "Whole page");
                    if ui
                        .add_enabled(
                            has_selection,
                            egui::RadioButton::new(
                                self.export_svg_selection && has_selection,
                                "Selection",
                            ),
                        )
                        .clicked()
                    {
                        self.export_svg_selection = true;
                    }
                });
                if self.export_svg_selection && !has_selection {
                    self.export_svg_selection = false;
                }
                vector_margin_row(
                    ui,
                    self.export_svg_selection && has_selection,
                    &mut self.export_vector_margin_mm,
                );
                ui.label("Layers");
                layer_picker(
                    ui,
                    &layers,
                    &mut self.export_svg_layer_ids,
                    &active_layer_id,
                );
                ui.checkbox(&mut self.export_svg_text_paths, "Text as paths");
                ui.weak(format!("{width_mm:.1} x {height_mm:.1} mm"));
                ui.weak("Selected layers only; hidden layers are skipped.");
                let can_export = !self.export_svg_layer_ids.is_empty();
                if !can_export {
                    ui.weak("Select at least one layer.");
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(can_export, egui::Button::new("Export"))
                        .clicked()
                    {
                        start = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if start {
            self.export_svg_open = false;
            self.export_subset = self.export_svg_selection && has_selection;
            self.open_dialog(FileMode::ExportSvg);
        } else if cancel {
            self.export_svg_open = false;
        } else {
            self.export_svg_open = open;
        }
    }

    pub(super) fn export_presets_window(&mut self, ctx: &egui::Context) {
        let mut open = self.export_presets_open;
        let mut close_requested = false;
        let mut stored = false;
        egui::Window::new("Export presets")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(
                    "A preset stores output options only; the document, the selection and \
                 the layer set stay with each export.",
                );
                egui::CollapsingHeader::new("G-code")
                    .default_open(true)
                    .show(ui, |ui| stored |= self.gcode_presets_section(ui));
                egui::CollapsingHeader::new("Image (PNG)")
                    .default_open(true)
                    .show(ui, |ui| stored |= self.image_presets_section(ui));
                egui::CollapsingHeader::new("Vector (SVG, DXF, PDF, EPS, HPGL)")
                    .default_open(true)
                    .show(ui, |ui| stored |= self.vector_presets_section(ui));
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

    fn gcode_presets_section(&mut self, ui: &mut egui::Ui) -> bool {
        let mut save = false;
        let mut apply: Option<String> = None;
        let mut delete: Option<String> = None;
        ui.label(
            "Dialect, decimals, comments, travel moves, line endings, arcs, smoothing \
             and the job origin.",
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
                    egui::Button::new("Save G-code options"),
                )
                .on_hover_text("An existing name is replaced")
                .clicked()
            {
                save = true;
            }
        });
        if self.preferences.export_presets.saved.is_empty() {
            ui.weak("No G-code presets saved yet.");
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
        stored
    }

    fn image_presets_section(&mut self, ui: &mut egui::Ui) -> bool {
        let mut save = false;
        let mut apply: Option<String> = None;
        let mut delete: Option<String> = None;
        ui.label(
            "Resolution, transparency, margin, offset and a custom pixel size. \
             The area (page, selection or drawing) is chosen per export.",
        );
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.image_export_preset_name)
                    .hint_text("Preset name")
                    .desired_width(200.0),
            );
            if ui
                .add_enabled(
                    !self.image_export_preset_name.trim().is_empty(),
                    egui::Button::new("Save image options"),
                )
                .on_hover_text("An existing name is replaced")
                .clicked()
            {
                save = true;
            }
        });
        if self.preferences.image_export_presets.saved.is_empty() {
            ui.weak("No image presets saved yet.");
        }
        for preset in &self.preferences.image_export_presets.saved {
            ui.horizontal(|ui| {
                ui.label(&preset.name);
                ui.weak(format!(
                    "{:.0} dpi · {:.1} mm margin · {} x {} px{}{}",
                    preset.dpi,
                    preset.margin_mm,
                    preset.width_px,
                    preset.height_px,
                    if preset.transparent {
                        " · transparent"
                    } else {
                        ""
                    },
                    if preset.custom_size {
                        if preset.keep_aspect {
                            " · fit"
                        } else {
                            " · stretch"
                        }
                    } else {
                        ""
                    }
                ));
                if ui.button("Apply").clicked() {
                    apply = Some(preset.name.clone());
                }
                if ui.button("Delete").clicked() {
                    delete = Some(preset.name.clone());
                }
            });
        }
        let mut stored = false;
        if save {
            let name = self.image_export_preset_name.trim().to_string();
            let preset = crate::config::preferences::ImageExportPreset {
                name: name.clone(),
                dpi: self.export_image_dpi,
                transparent: self.export_image_transparent,
                margin_mm: self.export_image_margin_mm,
                offset_x_mm: self.export_image_offset_x_mm,
                offset_y_mm: self.export_image_offset_y_mm,
                custom_size: self.export_image_custom_size,
                width_px: self.export_image_width_px,
                height_px: self.export_image_height_px,
                keep_aspect: self.export_image_keep_aspect,
            };
            match self.preferences.image_export_presets.upsert(preset) {
                Ok(()) => {
                    self.status = format!("Image preset '{name}' saved");
                    stored = true;
                }
                Err(error) => self.status = error,
            }
        }
        if let Some(name) = apply {
            if let Some(preset) = self.preferences.image_export_presets.find(&name).cloned() {
                self.apply_image_export_preset(&preset);
                self.status = format!("Image preset '{name}' applied");
            }
        }
        if let Some(name) = delete {
            if self.preferences.image_export_presets.remove(&name) {
                self.status = format!("Image preset '{name}' deleted");
                stored = true;
            }
        }
        stored
    }

    fn vector_presets_section(&mut self, ui: &mut egui::Ui) -> bool {
        let mut save = false;
        let mut apply: Option<String> = None;
        let mut delete: Option<String> = None;
        ui.label(
            "Text as paths and, for DXF, circles and ellipses as curves. \
             The area and the layers are chosen per export.",
        );
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("vector_preset_format")
                .selected_text(self.vector_export_preset_format.to_uppercase())
                .show_ui(ui, |ui| {
                    for format in ["svg", "dxf", "pdf", "eps", "hpgl"] {
                        ui.selectable_value(
                            &mut self.vector_export_preset_format,
                            format.to_string(),
                            format.to_uppercase(),
                        );
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.vector_export_preset_name)
                    .hint_text("Preset name")
                    .desired_width(200.0),
            );
            if ui
                .add_enabled(
                    !self.vector_export_preset_name.trim().is_empty(),
                    egui::Button::new("Save vector options"),
                )
                .on_hover_text("An existing name is replaced")
                .clicked()
            {
                save = true;
            }
        });
        if self.preferences.vector_export_presets.saved.is_empty() {
            ui.weak("No vector presets saved yet.");
        }
        for preset in &self.preferences.vector_export_presets.saved {
            ui.horizontal(|ui| {
                ui.label(&preset.name);
                ui.weak(format!(
                    "{} · {}{}{}",
                    preset.format.to_uppercase(),
                    if preset.text_as_paths {
                        "text as paths"
                    } else {
                        "text as text"
                    },
                    if preset.arcs { " · arcs" } else { "" },
                    if preset.margin_mm > 0.0 {
                        format!(" · {:.1} mm margin", preset.margin_mm)
                    } else {
                        String::new()
                    }
                ));
                if ui.button("Apply").clicked() {
                    apply = Some(preset.name.clone());
                }
                if ui.button("Delete").clicked() {
                    delete = Some(preset.name.clone());
                }
            });
        }
        let mut stored = false;
        if save {
            let name = self.vector_export_preset_name.trim().to_string();
            let format = self.vector_export_preset_format.clone();
            let (text_as_paths, arcs) = match format.as_str() {
                "svg" => (self.export_svg_text_paths, false),
                "dxf" => (self.export_dxf_text_paths, self.export_dxf_arcs),
                _ => (self.export_vector_text_paths, false),
            };
            let preset = crate::config::preferences::VectorExportPreset {
                name: name.clone(),
                format,
                text_as_paths,
                arcs,
                margin_mm: self.export_vector_margin_mm,
                dxf_unit: self.export_dxf_unit.id().to_string(),
            };
            match self.preferences.vector_export_presets.upsert(preset) {
                Ok(()) => {
                    self.status = format!("Vector preset '{name}' saved");
                    stored = true;
                }
                Err(error) => self.status = error,
            }
        }
        if let Some(name) = apply {
            if let Some(preset) = self.preferences.vector_export_presets.find(&name).cloned() {
                self.apply_vector_export_preset(&preset);
                self.status = format!("Vector preset '{name}' applied");
            }
        }
        if let Some(name) = delete {
            if self.preferences.vector_export_presets.remove(&name) {
                self.status = format!("Vector preset '{name}' deleted");
                stored = true;
            }
        }
        stored
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

/// Vector export margin control; only meaningful for a selection export.
fn vector_margin_row(ui: &mut egui::Ui, enabled: bool, margin: &mut f64) {
    ui.add_enabled_ui(enabled, |ui| {
        ui.horizontal(|ui| {
            ui.label("Margin");
            ui.add(
                egui::DragValue::new(margin)
                    .range(0.0..=100.0)
                    .speed(0.1)
                    .suffix(" mm"),
            );
            ui.weak("blank space around the selection, up to the page edge");
        });
    });
}
