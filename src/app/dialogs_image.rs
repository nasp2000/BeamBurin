#![allow(clippy::wildcard_imports)]

use super::*;

/// Resolutions offered in the image export options. The numeric field accepts
/// any value between 10 and 2400 dpi; the presets make the common screen and
/// print resolutions one click away.
const EXPORT_DPI_PRESETS: [(u32, &str); 5] = [
    (96, "96 dpi"),
    (150, "150 dpi"),
    (200, "200 dpi"),
    (300, "300 dpi"),
    (600, "600 dpi"),
];

/// The preset label matching `dpi`, if the value is one of [`EXPORT_DPI_PRESETS`].
fn export_dpi_preset(dpi: f64) -> Option<&'static str> {
    EXPORT_DPI_PRESETS
        .iter()
        .find(|(value, _)| (dpi - f64::from(*value)).abs() < 0.5)
        .map(|(_, label)| *label)
}

impl BeamBurinApp {
    fn drawing_area_mm(&self) -> Option<(f64, f64)> {
        let (min, max) = self.editor.visible_bounds()?;
        let x = min.x.max(0.0);
        let y = min.y.max(0.0);
        let max_x = max.x.min(self.editor.document.width_mm);
        let max_y = max.y.min(self.editor.document.height_mm);
        (max_x > x && max_y > y).then_some((max_x - x, max_y - y))
    }

    fn image_area_mm(&self) -> (f64, f64) {
        if self.export_image_selection {
            if let Some((min, max)) = self.editor.selected_bounds() {
                return (max.x - min.x, max.y - min.y);
            }
        }
        if self.export_image_drawing {
            if let Some(size) = self.drawing_area_mm() {
                return size;
            }
        }
        (
            self.editor.document.width_mm,
            self.editor.document.height_mm,
        )
    }

    /// Open the PNG export options; `selection` preselects the crop.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub(super) fn open_export_image(&mut self, selection: bool) {
        self.export_image_selection = selection && !self.editor.selection.is_empty();
        let (width_mm, height_mm) = self.image_area_mm();
        let scale = self.export_image_dpi / 25.4;
        let margin = self.export_image_margin_mm.max(0.0);
        self.export_image_width_px = (((width_mm + margin * 2.0) * scale).round()).max(1.0) as u32;
        self.export_image_height_px =
            (((height_mm + margin * 2.0) * scale).round()).max(1.0) as u32;
        self.export_image_open = true;
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub(super) fn export_image_window(&mut self, ctx: &egui::Context) {
        let mut open = self.export_image_open;
        let mut start = false;
        let mut cancel = false;
        let has_selection = !self.editor.selection.is_empty();
        let drawing_available = self.drawing_area_mm().is_some();
        let dpi = self.export_image_dpi;
        let (width_mm, height_mm) = self.image_area_mm();
        let margin = self.export_image_margin_mm.max(0.0);
        let frame_width_mm = width_mm + margin * 2.0;
        let frame_height_mm = height_mm + margin * 2.0;
        let auto_width_px = ((frame_width_mm * dpi / 25.4).round()).max(1.0) as u32;
        let auto_height_px = ((frame_height_mm * dpi / 25.4).round()).max(1.0) as u32;
        let (width_px, height_px) = if self.export_image_custom_size {
            (
                self.export_image_width_px.max(1),
                self.export_image_height_px.max(1),
            )
        } else {
            (auto_width_px, auto_height_px)
        };
        egui::Window::new("Export image")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
                egui::Grid::new("export_image_options")
                    .num_columns(2)
                    .spacing([10.0, 6.0])
                    .show(ui, |ui| {
                        let dpi_preset = export_dpi_preset(self.export_image_dpi);
                        let custom_dpi = self.export_image_dpi_custom || dpi_preset.is_none();
                        let resolution_label = ui.label("Resolution");
                        egui::ComboBox::from_id_salt("export_image_resolution")
                            .selected_text(if custom_dpi {
                                "Custom..."
                            } else {
                                dpi_preset.unwrap_or_default()
                            })
                            .width(110.0)
                            .show_ui(ui, |ui| {
                                for (value, label) in EXPORT_DPI_PRESETS {
                                    let selected = !self.export_image_dpi_custom
                                        && (self.export_image_dpi - f64::from(value)).abs() < 0.5;
                                    if ui.selectable_label(selected, label).clicked() {
                                        self.export_image_dpi = f64::from(value);
                                        self.export_image_dpi_custom = false;
                                        ui.close();
                                    }
                                }
                                if ui.selectable_label(custom_dpi, "Custom...").clicked() {
                                    self.export_image_dpi_custom = true;
                                    ui.close();
                                }
                            })
                            .response
                            .labelled_by(resolution_label.id)
                            .on_hover_text(crate::ui::icons::field("Resolution"));
                        ui.end_row();
                        if custom_dpi {
                            let exact_label = ui.label("Exact DPI");
                            ui.add(
                                egui::DragValue::new(&mut self.export_image_dpi)
                                    .speed(1.0)
                                    .range(10.0..=2400.0)
                                    .suffix(" dpi"),
                            )
                            .labelled_by(exact_label.id)
                            .on_hover_text(crate::ui::icons::field("Exact DPI"));
                            ui.end_row();
                        }
                        ui.label("Background");
                        ui.checkbox(&mut self.export_image_transparent, "Transparent").on_hover_text(crate::ui::icons::description("Transparent"));
                        ui.end_row();
                        let margin_label = ui.label("Margin");
                        ui.add(
                            egui::DragValue::new(&mut self.export_image_margin_mm)
                                .speed(0.5)
                                .range(0.0..=100.0)
                                .suffix(" mm"),
                        )
                        .labelled_by(margin_label.id).on_hover_text(crate::ui::icons::field("Margin"));
                        ui.end_row();
                        let offset_label = ui.label("Offset");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut self.export_image_offset_x_mm)
                                    .speed(0.5)
                                    .range(-1000.0..=1000.0)
                                    .prefix("X ")
                                    .suffix(" mm"),
                            )
                            .labelled_by(offset_label.id).on_hover_text(crate::ui::icons::field("Offset"));
                            ui.add(
                                egui::DragValue::new(&mut self.export_image_offset_y_mm)
                                    .speed(0.5)
                                    .range(-1000.0..=1000.0)
                                    .prefix("Y ")
                                    .suffix(" mm"),
                            )
                            .labelled_by(offset_label.id).on_hover_text(crate::ui::icons::field("Offset"));
                        });
                        ui.end_row();
                        let size_label = ui.label("Size");
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut self.export_image_custom_size, "Custom").on_hover_text(crate::ui::icons::description("Custom"));
                            if self.export_image_custom_size {
                                let width = ui
                                    .add(
                                        egui::DragValue::new(&mut self.export_image_width_px)
                                            .speed(1.0)
                                            .range(1..=100_000)
                                            .suffix(" px"),
                                    )
                                    .labelled_by(size_label.id).on_hover_text(crate::ui::icons::field("Size"));
                                ui.label("x");
                                let height = ui
                                    .add(
                                        egui::DragValue::new(&mut self.export_image_height_px)
                                            .speed(1.0)
                                            .range(1..=100_000)
                                            .suffix(" px"),
                                    )
                                    .labelled_by(size_label.id);
                                ui.checkbox(&mut self.export_image_keep_aspect, "Keep aspect").on_hover_text(crate::ui::icons::description("Keep aspect"));
                                if self.export_image_keep_aspect
                                    && width_mm > 0.0
                                    && height_mm > 0.0
                                {
                                    if width.changed() {
                                        self.export_image_height_px =
                                            ((f64::from(self.export_image_width_px) * height_mm
                                                / width_mm)
                                                .round())
                                            .max(1.0)
                                                as u32;
                                    } else if height.changed() {
                                        self.export_image_width_px =
                                            ((f64::from(self.export_image_height_px) * width_mm
                                                / height_mm)
                                                .round())
                                            .max(1.0)
                                                as u32;
                                    }
                                }
                            }
                        });
                        ui.end_row();
                        ui.label("Area");
                        ui.horizontal(|ui| {
                            if ui
                                .radio(
                                    !self.export_image_selection && !self.export_image_drawing,
                                    "Whole page",
                                )
                                .clicked()
                            {
                                self.export_image_selection = false;
                                self.export_image_drawing = false;
                            }
                            if ui
                                .add_enabled(
                                    has_selection,
                                    egui::RadioButton::new(
                                        self.export_image_selection && has_selection,
                                        "Selection",
                                    ),
                                )
                                .clicked()
                            {
                                self.export_image_selection = true;
                                self.export_image_drawing = false;
                            }
                            if ui
                                .add_enabled(
                                    drawing_available,
                                    egui::RadioButton::new(
                                        self.export_image_drawing && drawing_available,
                                        "Drawing",
                                    ),
                                )
                                .clicked()
                            {
                                self.export_image_drawing = true;
                                self.export_image_selection = false;
                            }
                        });
                        ui.end_row();
                    });
                if self.export_image_selection && !has_selection {
                    self.export_image_selection = false;
                }
                if self.export_image_drawing && !drawing_available {
                    self.export_image_drawing = false;
                }
                ui.weak(format!(
                    "{frame_width_mm:.1} x {frame_height_mm:.1} mm, {width_px} x {height_px} px"
                ));
                ui.weak("Positive offsets move the frame right and up; the drawing moves left and down.");
                ui.horizontal(|ui| {
                    crate::ui::help::button(ui, "export_image");
                    if ui.button("Export").on_hover_text(crate::ui::icons::description("Export")).clicked() {
                        start = true;
                    }
                    if ui.button("Cancel").on_hover_text(crate::ui::icons::description("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        if start {
            self.export_image_open = false;
            self.export_subset = self.export_image_selection && has_selection;
            self.open_dialog(FileMode::ExportImage);
        } else if cancel {
            self.export_image_open = false;
        } else {
            self.export_image_open = open;
        }
    }

    pub(super) fn image_adjust_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.image_adjust.clone() else {
            return;
        };
        let Some(index) = self
            .editor
            .document
            .objects
            .iter()
            .position(|object| object.id == id)
        else {
            self.image_adjust_edit = None;
            self.image_compare = false;
            self.image_adjust = None;
            return;
        };
        let crate::document::Shape::Image(image) = &self.editor.document.objects[index].shape
        else {
            self.image_adjust_edit = None;
            self.image_adjust = None;
            return;
        };
        let mut image = image.clone();
        // The dialog edits a working copy so "Compare with original" can show
        // the opening settings on the canvas without losing the edits.
        if let Some(edit) = self.image_adjust_edit {
            image.settings = edit;
        }
        let object_name = self.editor.document.objects[index].name.clone();
        let cut_entry = self
            .editor
            .document
            .object(&id)
            .and_then(|object| object.operation_id.as_deref())
            .and_then(|operation_id| {
                self.editor
                    .document
                    .operations
                    .iter()
                    .find(|operation| operation.id == operation_id)
            })
            .map(|operation| {
                (
                    operation.name.clone(),
                    operation.params.speed,
                    operation.params.power,
                    operation.params.passes.max(1),
                )
            });
        let original_settings = self
            .image_adjust_snapshot
            .as_ref()
            .and_then(|document| document.object(&id))
            .and_then(|object| match &object.shape {
                crate::document::Shape::Image(image) => Some(image.settings),
                _ => None,
            })
            .unwrap_or(image.settings);
        let mut burn_map = self.image_burn_map;
        let mut invert_display = self.image_invert_display;
        let mut zoom = self.image_preview_zoom;
        let mut target_dpi = self.image_target_dpi;
        let mut target_width_mm = self.image_target_width_mm;
        let original_texture = self.original_image_texture(ctx, &image);
        let processed_texture = self
            .image_previews
            .get(&id)
            .map(|preview| preview.texture.clone());
        let burn_texture = if burn_map {
            self.burn_preview_texture(ctx, &id, &image)
        } else {
            None
        };
        let invert_texture = if invert_display {
            self.invert_preview_texture(ctx, &id, &image)
        } else {
            None
        };
        let mut histogram_image = image.clone();
        if self.image_compare {
            histogram_image.settings = original_settings;
        }
        let histogram = self.image_histogram_bins(&id, &histogram_image);

        let mut close = false;
        let mut cancel = false;
        let mut reset = false;
        let mut export_prepared = false;
        let mut apply_all = false;
        let mut compare = self.image_compare;
        let mut open_test_grid = false;
        let mut preset: Option<crate::objects::ImageSettings> = None;
        let mut auto_levels = false;
        let mut mask = image.crop.is_some();
        let mut mask_x = image.crop.map_or(0, |crop| crop.x);
        let mut mask_y = image.crop.map_or(0, |crop| crop.y);
        let mut mask_w = image.crop.map_or(image.width_px, |crop| crop.width);
        let mut mask_h = image.crop.map_or(image.height_px, |crop| crop.height);
        let mut target_message: Option<String> = None;
        let mut profile_saved: Option<String> = None;
        egui::Window::new(format!("Adjust image - {object_name}"))
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .collapsible(false)
            .resizable(false)
            .fixed_size([960.0, 712.0])
            .show(ctx, |ui| {
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
                ui.set_min_height(660.0);
                let more_width = 300.0;
                let preview_width =
                    ((ui.available_width() - more_width - 88.0) / 2.0).clamp(200.0, 340.0);
                let preview_height = 340.0;
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(preview_width, preview_height + 28.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(preview_width);
                        ui.allocate_ui(egui::vec2(preview_width, preview_height), |ui| {
                            ui.vertical_centered(|ui| {
                                if let Some(texture) = &original_texture {
                                    ui.add(
                                        egui::Image::new(texture)
                                            .max_size(egui::vec2(preview_width, preview_height))
                                            .shrink_to_fit(),
                                    );
                                } else {
                                    ui.weak("Cannot read the image file");
                                }
                            });
                        });
                        ui.vertical_centered(|ui| {
                            ui.weak(format!("{} x {}", image.width_px, image.height_px));
                        });
                    });
                    ui.separator();
                    ui.allocate_ui_with_layout(
                        egui::vec2(preview_width, preview_height + 28.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(preview_width);
                        ui.allocate_ui(egui::vec2(preview_width, preview_height), |ui| {
                            ui.vertical_centered(|ui| {
                                let texture = burn_texture
                                    .as_ref()
                                    .or(invert_texture.as_ref())
                                    .or(processed_texture.as_ref());
                                if let Some(texture) = texture {
                                    if zoom > 0.0 {
                                        egui::ScrollArea::both()
                                            .id_salt("image_preview_zoom_scroll")
                                            .max_width(preview_width)
                                            .max_height(preview_height)
                                            .auto_shrink([false, false])
                                            .show(ui, |ui| {
                                                ui.add(
                                                    egui::Image::new(texture).fit_to_exact_size(
                                                        texture.size_vec2() * zoom,
                                                    ),
                                                );
                                            });
                                    } else {
                                        ui.add(
                                            egui::Image::new(texture)
                                                .max_size(egui::vec2(
                                                    preview_width,
                                                    preview_height,
                                                ))
                                                .shrink_to_fit(),
                                        );
                                    }
                                } else {
                                    ui.weak("No preview available");
                                }
                            });
                        });
                        ui.horizontal(|ui| {
                            let (work_width, work_height) = image.effective_size_px();
                            ui.weak(format!("{work_width} x {work_height}"));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.checkbox(&mut invert_display, "Invert display").on_hover_text(crate::ui::icons::description("Invert display"));
                                },
                            );
                        });
                    });
                    ui.separator();
                    ui.allocate_ui_with_layout(
                        egui::vec2(more_width, preview_height + 28.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(more_width);
                egui::CollapsingHeader::new("More options")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("image_more_scroll")
                            .max_height(preview_height - 36.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                // The previews row is narrow: keep the long
                                // status lines inside the column.
                                ui.set_max_width(more_width - 20.0);
                                ui.horizontal_wrapped(|ui| {
                                    ui.checkbox(&mut burn_map, "Burn map").on_hover_text(crate::ui::icons::description("Burn map"));
                                    let label = if (zoom - 1.0).abs() < f32::EPSILON {
                                        "1:1".to_string()
                                    } else if zoom > 0.0 {
                                        format!("{zoom:.0}×")
                                    } else {
                                        "Fit".to_string()
                                    };
                                    let zoom_label = ui.label("Zoom");
                                    egui::ComboBox::from_id_salt("image_preview_zoom")
                                        .selected_text(label)
                                        .width(70.0)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut zoom, 0.0, "Fit").on_hover_text(crate::ui::icons::description("Fit"));
                                            ui.selectable_value(&mut zoom, 1.0, "1:1").on_hover_text(crate::ui::icons::description("1:1"));
                                            ui.selectable_value(&mut zoom, 2.0, "2×").on_hover_text(crate::ui::icons::description("2×"));
                                            ui.selectable_value(&mut zoom, 4.0, "4×").on_hover_text(crate::ui::icons::description("4×"));
                                        })
                                        .response
                                        .labelled_by(zoom_label.id);
                                    ui.checkbox(&mut compare, "Compare with original").on_hover_text(crate::ui::icons::description("Compare with original"));
                                });
                                draw_histogram(ui, &histogram, &image.settings);
                                ui.separator();
                                ui.vertical(|ui| {
                                    egui::Grid::new("image_target_resolution")
                                        .num_columns(2)
                                        .spacing([12.0, 6.0])
                                        .show(ui, |ui| {
                                            let dpi_label = ui.label("Target DPI");
                                            ui.add(
                                                egui::DragValue::new(&mut target_dpi)
                                                    .speed(1.0)
                                                    .range(50.0..=1000.0)
                                                    .fixed_decimals(0),
                                            )
                                            .labelled_by(dpi_label.id).on_hover_text(crate::ui::icons::field("Target DPI"));
                                            ui.end_row();
                                            let width_label = ui.label("Target width (mm)");
                                            ui.add(
                                                egui::DragValue::new(&mut target_width_mm)
                                                    .speed(0.5)
                                                    .range(1.0..=10000.0)
                                                    .fixed_decimals(1)
                                                    .suffix(" mm"),
                                            )
                                            .labelled_by(width_label.id).on_hover_text(crate::ui::icons::field("Target width (mm)"));
                                            ui.end_row();
                                        });
                                    let current_px = image.effective_size_px().0;
                                    match crate::objects::target_resolution(
                                        target_width_mm,
                                        target_dpi,
                                        current_px,
                                    ) {
                                        Some(target) => {
                                            ui.weak(format!(
                                                "Needs {} px wide; the current {} px reach {:.0} dpi at {:.1} mm",
                                                target.required_width_px,
                                                current_px,
                                                target.effective_dpi,
                                                target_width_mm
                                            ));
                                            if target.low_resolution {
                                                let warning = ui.visuals().warn_fg_color;
                                                ui.colored_label(
                                                    warning,
                                                    format!(
                                                        "Below {:.0} dpi: detail may be lost.",
                                                        crate::objects::LOW_DPI_THRESHOLD
                                                    ),
                                                );
                                            }
                                        }
                                        None => {
                                            ui.weak("Enter a positive width and DPI");
                                        }
                                    }
                                });
                                if ui
                                    .button("Apply target")
                                    .on_hover_text(
                                        "Set the working size and line interval from the target width and DPI",
                                    )
                                    .clicked()
                                {
                                    let (work_width, work_height) = image.effective_size_px();
                                    let aspect =
                                        f64::from(work_height) / f64::from(work_width.max(1));
                                    match crate::objects::engraving_resolution(
                                        target_width_mm,
                                        target_dpi,
                                        aspect,
                                    ) {
                                        Some((width, height, interval)) => {
                                            image.settings.target_width_px = width;
                                            image.settings.target_height_px = height;
                                            image.settings.line_interval_mm = interval;
                                            target_message = Some(format!(
                                                "Target applied: {width} x {height} px, {interval:.3} mm line interval"
                                            ));
                                        }
                                        None => {
                                            target_message = Some(
                                                "The target leaves the supported working range."
                                                    .to_string(),
                                            );
                                        }
                                    }
                                }
                                ui.separator();
                                ui.horizontal_wrapped(|ui| {
                                    ui.checkbox(&mut mask, "Mask").on_hover_text(crate::ui::icons::description("Mask"));
                                    ui.weak("Limit the engraved area (source pixels)");
                                    if ui.button("Full image").on_hover_text(crate::ui::icons::description("Full image")).clicked() {
                                        mask = false;
                                    }
                                });
                                if mask {
                                    ui.horizontal_wrapped(|ui| {
                                        let x_label = ui.label("Mask X");
                                        ui.add(egui::DragValue::new(&mut mask_x).range(
                                            0..=image.width_px.saturating_sub(1),
                                        ))
                                        .labelled_by(x_label.id).on_hover_text(crate::ui::icons::field("Mask X"));
                                        let y_label = ui.label("Mask Y");
                                        ui.add(egui::DragValue::new(&mut mask_y).range(
                                            0..=image.height_px.saturating_sub(1),
                                        ))
                                        .labelled_by(y_label.id).on_hover_text(crate::ui::icons::field("Mask Y"));
                                        let w_label = ui.label("Mask W");
                                        ui.add(
                                            egui::DragValue::new(&mut mask_w)
                                                .range(1..=image.width_px.max(1)),
                                        )
                                        .labelled_by(w_label.id).on_hover_text(crate::ui::icons::field("Mask W"));
                                        let h_label = ui.label("Mask H");
                                        ui.add(
                                            egui::DragValue::new(&mut mask_h)
                                                .range(1..=image.height_px.max(1)),
                                        )
                                        .labelled_by(h_label.id).on_hover_text(crate::ui::icons::field("Mask H"));
                                    });
                                }
                                ui.horizontal_wrapped(|ui| {
                                    ui.checkbox(&mut image.mask_alpha, "Transparent pixels").on_hover_text(crate::ui::icons::description("Transparent pixels"));
                                    ui.add_enabled(
                                        image.mask_alpha,
                                        egui::DragValue::new(&mut image.mask_alpha_threshold)
                                            .range(1..=255)
                                            .speed(1),
                                    )
                                    .on_hover_text("Alpha values at or below this become transparent")
                                    .widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::WidgetType::DragValue,
                                            true,
                                            "Alpha threshold",
                                        )
                                    });
                                    ui.weak("alpha below this is background");
                                });
                                ui.horizontal_wrapped(|ui| {
                                    ui.strong("Colour mask");
                                    ui.weak("Treat these colours as background");
                                    if !image.mask_colours.is_empty()
                                        && ui.button("Clear colours").on_hover_text(crate::ui::icons::description("Clear colours")).clicked()
                                    {
                                        image.mask_colours.clear();
                                    }
                                });
                                for row in super::layers::DRAWING_COLOURS.chunks(10) {
                                    ui.horizontal(|ui| {
                                        for colour in row {
                                            image_mask_colour_button(ui, &mut image, colour);
                                        }
                                    });
                                }
                                ui.horizontal_wrapped(|ui| {
                                    let tolerance_label = ui.label("Tolerance");
                                    ui.add(
                                        egui::DragValue::new(&mut image.mask_tolerance)
                                            .range(0..=255)
                                            .speed(1),
                                    )
                                    .labelled_by(tolerance_label.id).on_hover_text(crate::ui::icons::field("Tolerance"));
                                    ui.weak("per channel, 0 matches only the exact colour");
                                });
                                ui.separator();
                                egui::Grid::new("image_more_settings")
                                    .num_columns(3)
                                    .spacing([12.0, 6.0])
                                    .show(ui, |ui| {
                                        let shadows_label = ui.label("Shadows");
                                        ui.add(
                                            egui::Slider::new(
                                                &mut image.settings.shadow_lift,
                                                -1.0..=1.0,
                                            )
                                            .fixed_decimals(2)
                                            .show_value(false),
                                        )
                                        .labelled_by(shadows_label.id).on_hover_text(crate::ui::icons::field("Shadows"));
                                        ui.add(
                                            egui::DragValue::new(&mut image.settings.shadow_lift)
                                                .speed(0.01)
                                                .range(-1.0..=1.0)
                                                .fixed_decimals(2),
                                        )
                                        .labelled_by(shadows_label.id).on_hover_text(crate::ui::icons::field("Shadows"));
                                        ui.end_row();
                                        let midtones_label = ui.label("Midtones");
                                        ui.add(
                                            egui::Slider::new(
                                                &mut image.settings.midtone_shift,
                                                -1.0..=1.0,
                                            )
                                            .fixed_decimals(2)
                                            .show_value(false),
                                        )
                                        .labelled_by(midtones_label.id).on_hover_text(crate::ui::icons::field("Midtones"));
                                        ui.add(
                                            egui::DragValue::new(
                                                &mut image.settings.midtone_shift,
                                            )
                                            .speed(0.01)
                                            .range(-1.0..=1.0)
                                            .fixed_decimals(2),
                                        )
                                        .labelled_by(midtones_label.id).on_hover_text(crate::ui::icons::field("Midtones"));
                                        ui.end_row();
                                        let highlights_label = ui.label("Highlights");
                                        ui.add(
                                            egui::Slider::new(
                                                &mut image.settings.highlight_lift,
                                                -1.0..=1.0,
                                            )
                                            .fixed_decimals(2)
                                            .show_value(false),
                                        )
                                        .labelled_by(highlights_label.id).on_hover_text(crate::ui::icons::field("Highlights"));
                                        ui.add(
                                            egui::DragValue::new(
                                                &mut image.settings.highlight_lift,
                                            )
                                            .speed(0.01)
                                            .range(-1.0..=1.0)
                                            .fixed_decimals(2),
                                        )
                                        .labelled_by(highlights_label.id).on_hover_text(crate::ui::icons::field("Highlights"));
                                        ui.end_row();
                                        ui.label("Tone curve");
                                        if ui.button("Reset curve").on_hover_text(crate::ui::icons::description("Reset curve")).clicked() {
                                            image.settings.shadow_lift = 0.0;
                                            image.settings.midtone_shift = 0.0;
                                            image.settings.highlight_lift = 0.0;
                                        }
                                        ui.end_row();
                                        let clip_label = ui.label("Black / white clip");
                                        ui.horizontal(|ui| {
                                            ui.add(
                                                egui::DragValue::new(
                                                    &mut image.settings.black_clip,
                                                )
                                                .range(0..=254),
                                            )
                                            .labelled_by(clip_label.id).on_hover_text(crate::ui::icons::field("Black / white clip"));
                                            ui.add(
                                                egui::DragValue::new(
                                                    &mut image.settings.white_clip,
                                                )
                                                .range(1..=255),
                                            )
                                            .labelled_by(clip_label.id).on_hover_text(crate::ui::icons::field("Black / white clip"));
                                        });
                                        ui.end_row();
                                        let denoise_label = ui.label("Enhance denoise");
                                        ui.add(
                                            egui::DragValue::new(
                                                &mut image.settings.enhance_denoise,
                                            )
                                            .speed(0.1)
                                            .range(0.0..=10.0)
                                            .fixed_decimals(1),
                                        )
                                        .labelled_by(denoise_label.id)
                                        .on_hover_text(
                                            "Keep low-contrast noise in smooth areas from being sharpened",
                                        );
                                        ui.end_row();
                                        let grey_label = ui.label("Grey mix");
                                        ui.horizontal(|ui| {
                                            ui.add(
                                                egui::DragValue::new(
                                                    &mut image.settings.gray_red,
                                                )
                                                .speed(0.01)
                                                .range(0.0..=1.0)
                                                .fixed_decimals(2),
                                            )
                                            .labelled_by(grey_label.id)
                                            .on_hover_text("Red weight");
                                            ui.add(
                                                egui::DragValue::new(
                                                    &mut image.settings.gray_green,
                                                )
                                                .speed(0.01)
                                                .range(0.0..=1.0)
                                                .fixed_decimals(2),
                                            )
                                            .labelled_by(grey_label.id)
                                            .on_hover_text("Green weight");
                                            ui.add(
                                                egui::DragValue::new(
                                                    &mut image.settings.gray_blue,
                                                )
                                                .speed(0.01)
                                                .range(0.0..=1.0)
                                                .fixed_decimals(2),
                                            )
                                            .labelled_by(grey_label.id)
                                            .on_hover_text("Blue weight");
                                        });
                                        ui.end_row();
                                        ui.label("Despeckle");
                                        ui.checkbox(&mut image.settings.despeckle, "3×3 median").on_hover_text(crate::ui::icons::description("3×3 median"));
                                        ui.end_row();
                                    });
                                ui.separator();
                                ui.horizontal_wrapped(|ui| {
                                    if ui.button("Photo").on_hover_text(crate::ui::icons::description("Photo")).clicked() {
                                        preset = Some(crate::objects::ImageSettings {
                                            mode: crate::objects::ImageMode::Jarvis,
                                            contrast: 1.15,
                                            gamma: 0.9,
                                            enhance_radius_px: 1,
                                            enhance_amount: 0.6,
                                            ..crate::objects::ImageSettings::default()
                                        });
                                    }
                                    if ui.button("Wood").on_hover_text(crate::ui::icons::description("Wood")).clicked() {
                                        preset = Some(crate::objects::ImageSettings {
                                            mode: crate::objects::ImageMode::Stucki,
                                            contrast: 1.25,
                                            gamma: 1.1,
                                            ..crate::objects::ImageSettings::default()
                                        });
                                    }
                                    if ui.button("Metal").on_hover_text(crate::ui::icons::description("Metal")).clicked() {
                                        preset = Some(crate::objects::ImageSettings {
                                            mode: crate::objects::ImageMode::Atkinson,
                                            contrast: 1.4,
                                            gamma: 1.2,
                                            ..crate::objects::ImageSettings::default()
                                        });
                                    }
                                    if ui.button("Acrylic").on_hover_text(crate::ui::icons::description("Acrylic")).clicked() {
                                        preset = Some(crate::objects::ImageSettings {
                                            mode: crate::objects::ImageMode::FloydSteinberg,
                                            contrast: 1.1,
                                            gamma: 0.95,
                                            ..crate::objects::ImageSettings::default()
                                        });
                                    }
                                    if ui.button("Newsprint").on_hover_text(crate::ui::icons::description("Newsprint")).clicked() {
                                        preset = Some(crate::objects::ImageSettings {
                                            mode: crate::objects::ImageMode::Newsprint,
                                            contrast: 1.2,
                                            ..crate::objects::ImageSettings::default()
                                        });
                                    }
                                    if ui.button("Sketch").on_hover_text(crate::ui::icons::description("Sketch")).clicked() {
                                        preset = Some(crate::objects::ImageSettings {
                                            mode: crate::objects::ImageMode::Sketch,
                                            contrast: 1.3,
                                            ..crate::objects::ImageSettings::default()
                                        });
                                    }
                                });
                                ui.horizontal_wrapped(|ui| {
                                    if ui.button("Auto levels").on_hover_text(crate::ui::icons::description("Auto levels")).clicked() {
                                        auto_levels = true;
                                    }
                                    if ui.button("Export prepared PNG").on_hover_text(crate::ui::icons::description("Export prepared PNG")).clicked() {
                                        export_prepared = true;
                                    }
                                    if ui.button("Test grid...").on_hover_text(crate::ui::icons::description("Test grid...")).clicked() {
                                        open_test_grid = true;
                                    }
                                    if ui.button("Apply to selected images").on_hover_text(crate::ui::icons::description("Apply to selected images")).clicked() {
                                        apply_all = true;
                                    }
                                    if ui
                                        .button("Save as profile")
                                        .on_hover_text(
                                            "Store the current settings as a reusable image profile",
                                        )
                                        .clicked()
                                    {
                                        let mut index = self.profiles.images.len() + 1;
                                        let name = loop {
                                            let candidate = format!("Image profile {index}");
                                            if !self
                                                .profiles
                                                .images
                                                .iter()
                                                .any(|profile| profile.name == candidate)
                                            {
                                                break candidate;
                                            }
                                            index += 1;
                                        };
                                        let mut profile =
                                            crate::profiles::ImageProfile::new(name.clone());
                                        profile.settings = image.settings;
                                        profile.meta.category = "Custom".to_string();
                                        let id = self.profiles.add_image(profile).id.clone();
                                        self.selected_image_profile = Some(id);
                                        self.save_profiles();
                                        profile_saved =
                                            Some(format!("Saved \"{name}\" as an image profile"));
                                    }
                                });
                                ui.separator();
                                if let Some(preview) = self.image_previews.get(&id) {
                                    ui.weak(preview.stats.coverage_text());
                                    if let Some((name, speed, power, passes)) = &cut_entry {
                                        let passes_text = if *passes == 1 {
                                            "1 pass".to_string()
                                        } else {
                                            format!("{passes} passes")
                                        };
                                        ui.weak(format!(
                                            "{name}: {speed:.0} mm/min, {power:.0}%, {passes_text}"
                                        ));
                                        if let Some((_, _, seconds)) =
                                            crate::raster::estimate_engraving(
                                                preview.stats,
                                                image.width_mm(),
                                                *speed,
                                            )
                                        {
                                            let seconds = seconds * f64::from(*passes);
                                            let minutes = seconds / 60.0;
                                            let time = if minutes >= 1.5 {
                                                format!("{minutes:.1} min")
                                            } else {
                                                format!("{seconds:.0} s")
                                            };
                                            ui.weak(format!(
                                                "Estimated: {time} at this cut entry"
                                            ));
                                        }
                                    }
                                }
                            });
                    });
                        },
                    );
                });
                ui.horizontal(|ui| {
                    if ui.button("Reset All").on_hover_text(crate::ui::icons::description("Reset All")).clicked() {
                        reset = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let selected_profile = self.selected_image_profile.clone();
                        let removable = selected_profile
                            .as_deref()
                            .and_then(|id| self.profiles.image(id))
                            .is_some_and(|profile| !profile.builtin);
                        if ui
                            .add_enabled(removable, egui::Button::new("Delete preset"))
                            .on_hover_text("Remove the selected image profile")
                            .clicked()
                        {
                            if let Some(id) = &selected_profile {
                                match self.profiles.remove_image(id) {
                                    Ok(()) => {
                                        self.selected_image_profile = None;
                                        self.save_profiles();
                                        self.status = "Image profile deleted".to_string();
                                    }
                                    Err(error) => self.status = error,
                                }
                            }
                        }
                        if ui
                            .add_enabled(
                                selected_profile.is_some(),
                                egui::Button::new("Duplicate preset"),
                            )
                            .on_hover_text("Copy the selected image profile")
                            .clicked()
                        {
                            if let Some(id) = &selected_profile {
                                if let Some(copy) = self.profiles.duplicate_image(id) {
                                    self.selected_image_profile = Some(copy);
                                    self.save_profiles();
                                    self.status = "Image profile duplicated".to_string();
                                }
                            }
                        }
                        let preset_label = self
                            .selected_image_profile
                            .as_deref()
                            .and_then(|id| self.profiles.image(id))
                            .map_or_else(|| "Presets".to_string(), |profile| profile.name.clone());
                        let presets_label = ui.label("Preset");
                        egui::ComboBox::from_id_salt("image_adjust_presets")
                            .selected_text(preset_label)
                            .width(180.0)
                            .show_ui(ui, |ui| {
                                for profile in &self.profiles.images {
                                    let selected = self.selected_image_profile.as_deref()
                                        == Some(profile.id.as_str());
                                    if ui
                                        .selectable_label(selected, &profile.name)
                                        .on_hover_text("Apply this image profile")
                                        .clicked()
                                    {
                                        preset = Some(profile.settings);
                                        self.selected_image_profile = Some(profile.id.clone());
                                    }
                                }
                            })
                            .response
                            .labelled_by(presets_label.id).on_hover_text(crate::ui::icons::field("Preset"));
                    });
                });
                ui.add_space(4.0);
                ui.horizontal_top(|ui| {
                    let group_width = ((ui.available_width() - 40.0) / 2.0).clamp(320.0, 460.0);
                    ui.group(|ui| {
                        ui.set_width(group_width);
                        ui.strong("Layer Settings");
                        egui::Grid::new("image_layer_settings")
                            .num_columns(2)
                            .spacing([12.0, 6.0])
                            .show(ui, |ui| {
                                let mode_label = ui.label("Image mode");
                                let mut mode = image.settings.mode;
                                egui::ComboBox::from_id_salt("image_adjust_mode")
                                    .selected_text(mode.label())
                                    .show_ui(ui, |ui| {
                                        for candidate in crate::objects::ImageMode::all() {
                                            ui.selectable_value(
                                                &mut mode,
                                                candidate,
                                                candidate.label(),
                                            )
                                            .on_hover_text(candidate.hint());
                                        }
                                    })
                                    .response
                                    .labelled_by(mode_label.id).on_hover_text(crate::ui::icons::field("Image mode"));
                                image.settings.mode = mode;
                                ui.end_row();
                                ui.label("Negative image");
                                let negative = ui
                                    .checkbox(&mut image.settings.invert, "")
                                    .on_hover_text("Invert the image tones");
                                negative.widget_info(|| {
                                    egui::WidgetInfo::selected(
                                        egui::WidgetType::Checkbox,
                                        true,
                                        image.settings.invert,
                                        "Negative image",
                                    )
                                });
                                ui.end_row();
                                ui.label("Cells per inch");
                                ui.weak(format!(
                                    "{:.1}",
                                    25.4 / image.settings.line_interval_mm.max(0.001)
                                ));
                                ui.end_row();
                                let interval_label = ui.label("Line interval (mm)");
                                ui.add(
                                    egui::DragValue::new(&mut image.settings.line_interval_mm)
                                        .speed(0.01)
                                        .range(0.01..=2.0),
                                )
                                .labelled_by(interval_label.id).on_hover_text(crate::ui::icons::field("Line interval (mm)"));
                                ui.end_row();
                                let angle_label = ui.label("Halftone angle");
                                ui.add(
                                    egui::DragValue::new(
                                        &mut image.settings.halftone_angle_degrees,
                                    )
                                    .speed(0.5)
                                    .range(-360.0..=360.0)
                                    .suffix(" deg"),
                                )
                                .labelled_by(angle_label.id).on_hover_text(crate::ui::icons::field("Halftone angle"));
                                ui.end_row();
                                ui.label("DPI");
                                ui.weak(format!("{:.1}", image.effective_dpi()));
                                ui.end_row();
                            });
                    });
                    ui.group(|ui| {
                        ui.set_width(group_width);
                        ui.strong("Image Settings");
                        egui::Grid::new("image_adjust_settings")
                            .num_columns(3)
                            .spacing([8.0, 6.0])
                            .show(ui, |ui| {
                                let contrast_label = ui.label("Contrast");
                                ui.add(
                                    egui::Slider::new(&mut image.settings.contrast, 0.25..=3.0)
                                        .show_value(false),
                                )
                                .labelled_by(contrast_label.id).on_hover_text(crate::ui::icons::field("Contrast"));
                                ui.add(
                                    egui::DragValue::new(&mut image.settings.contrast)
                                        .speed(0.01)
                                        .range(0.25..=3.0)
                                        .fixed_decimals(2),
                                )
                                .labelled_by(contrast_label.id).on_hover_text(crate::ui::icons::field("Contrast"));
                                ui.end_row();
                                let brightness_label = ui.label("Brightness");
                                ui.add(
                                    egui::Slider::new(&mut image.settings.brightness, -100..=100)
                                        .show_value(false),
                                )
                                .labelled_by(brightness_label.id).on_hover_text(crate::ui::icons::field("Brightness"));
                                ui.add(
                                    egui::DragValue::new(&mut image.settings.brightness)
                                        .speed(1.0)
                                        .range(-100..=100),
                                )
                                .labelled_by(brightness_label.id).on_hover_text(crate::ui::icons::field("Brightness"));
                                ui.end_row();
                                let radius_label = ui.label("Enhance Radius");
                                ui.add(
                                    egui::Slider::new(
                                        &mut image.settings.enhance_radius_px,
                                        0..=32,
                                    )
                                    .show_value(false),
                                )
                                .labelled_by(radius_label.id).on_hover_text(crate::ui::icons::field("Enhance Radius"));
                                ui.add(
                                    egui::DragValue::new(&mut image.settings.enhance_radius_px)
                                        .range(0..=32),
                                )
                                .labelled_by(radius_label.id).on_hover_text(crate::ui::icons::field("Enhance Radius"));
                                ui.end_row();
                                let amount_label = ui.label("Enhance Amount");
                                ui.add(
                                    egui::Slider::new(
                                        &mut image.settings.enhance_amount,
                                        0.0..=5.0,
                                    )
                                    .show_value(false),
                                )
                                .labelled_by(amount_label.id).on_hover_text(crate::ui::icons::field("Enhance Amount"));
                                ui.add(
                                    egui::DragValue::new(&mut image.settings.enhance_amount)
                                        .speed(0.05)
                                        .range(0.0..=5.0)
                                        .fixed_decimals(2),
                                )
                                .labelled_by(amount_label.id).on_hover_text(crate::ui::icons::field("Enhance Amount"));
                                ui.end_row();
                                let gamma_label = ui.label("Gamma");
                                ui.add(
                                    egui::Slider::new(&mut image.settings.gamma, 0.2..=3.0)
                                        .show_value(false),
                                )
                                .labelled_by(gamma_label.id).on_hover_text(crate::ui::icons::field("Gamma"));
                                ui.add(
                                    egui::DragValue::new(&mut image.settings.gamma)
                                        .speed(0.01)
                                        .range(0.2..=3.0)
                                        .fixed_decimals(2),
                                )
                                .labelled_by(gamma_label.id).on_hover_text(crate::ui::icons::field("Gamma"));
                                ui.end_row();
                            });
                    });
                });
                ui.add_space(4.0);
                ui.separator();
                ui.horizontal(|ui| {
                    crate::ui::help::button(ui, "image_adjust");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("OK")
                            .on_hover_text("Apply these image settings and close")
                            .clicked()
                        {
                            close = true;
                        }
                        if ui
                            .button("Cancel")
                            .on_hover_text("Close without applying the changes")
                            .clicked()
                        {
                            cancel = true;
                        }
                    });
                });
            });

        image.crop = if mask {
            let x = mask_x.min(image.width_px.saturating_sub(1));
            let y = mask_y.min(image.height_px.saturating_sub(1));
            let width = mask_w.clamp(1, image.width_px.saturating_sub(x).max(1));
            let height = mask_h.clamp(1, image.height_px.saturating_sub(y).max(1));
            Some(crate::objects::ImageCrop {
                x,
                y,
                width,
                height,
            })
        } else {
            None
        };
        self.image_burn_map = burn_map;
        self.image_invert_display = invert_display;
        self.image_preview_zoom = zoom;
        self.image_target_dpi = target_dpi;
        if let Some(message) = target_message {
            self.status = message;
        }
        if let Some(message) = profile_saved {
            self.status = message;
        }
        self.image_target_width_mm = target_width_mm;
        if open_test_grid {
            self.image_test_target = Some(id.clone());
            self.image_test_open = true;
        }

        if export_prepared {
            match crate::raster::process_object(std::path::Path::new(&image.path), &image) {
                Ok(bitmap) => {
                    let mut path = std::path::PathBuf::from(&image.path);
                    let stem = path
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or("image");
                    path.set_file_name(format!("{stem}_prepared.png"));
                    let picture = image::GrayImage::from_fn(
                        bitmap.width as u32,
                        bitmap.height as u32,
                        |x, y| {
                            if bitmap.get(x as usize, y as usize) {
                                image::Luma([0])
                            } else {
                                image::Luma([255])
                            }
                        },
                    );
                    match picture.save(&path) {
                        Ok(()) => {
                            self.status = format!("Prepared image saved to {}", path.display());
                        }
                        Err(error) => {
                            self.status = format!("Cannot save the prepared image: {error}");
                        }
                    }
                }
                Err(error) => self.status = format!("Cannot prepare the image: {error}"),
            }
        }
        if let Some(preset) = preset {
            image.settings = preset;
        }
        if reset {
            image.settings = crate::objects::ImageSettings::default();
        }
        if auto_levels {
            match crate::raster::auto_levels_preview(
                std::path::Path::new(&image.path),
                &image,
                crate::raster::PREVIEW_MAX_SIDE,
            ) {
                Ok((black, white)) => {
                    image.settings.black_clip = black;
                    image.settings.white_clip = white;
                    self.status = format!("Auto levels: black {black}, white {white}");
                }
                Err(error) => self.status = format!("Cannot analyse the image: {error}"),
            }
        }
        if cancel {
            // Restore every image to the settings it had when the dialog
            // opened: "Apply to all images" may have touched more than the
            // edited object, and the snapshot holds the document as it was.
            if let Some(snapshot) = self.image_adjust_snapshot.take() {
                for object in &mut self.editor.document.objects {
                    let Some(before) = snapshot.object(&object.id) else {
                        continue;
                    };
                    let (
                        crate::document::Shape::Image(current),
                        crate::document::Shape::Image(original),
                    ) = (&mut object.shape, &before.shape)
                    else {
                        continue;
                    };
                    current.settings = original.settings;
                    current.crop = original.crop;
                    current.mask_colours.clone_from(&original.mask_colours);
                    current.mask_tolerance = original.mask_tolerance;
                }
            } else if let Some(current) = self
                .editor
                .document
                .objects
                .iter_mut()
                .find(|object| object.id == id)
            {
                if let crate::document::Shape::Image(current) = &mut current.shape {
                    current.settings = original_settings;
                    current.crop = image.crop;
                }
            }
            self.image_adjust_edit = None;
            self.image_compare = false;
            self.image_adjust = None;
            return;
        }
        self.image_compare = compare;
        let settings_changed = self.image_adjust_edit != Some(image.settings);
        self.image_adjust_edit = Some(image.settings);
        let mut document_changed = false;
        if let Some(current) = self
            .editor
            .document
            .objects
            .iter_mut()
            .find(|object| object.id == id)
        {
            if let crate::document::Shape::Image(current) = &mut current.shape {
                let shown = if compare && !close {
                    original_settings
                } else {
                    image.settings
                };
                if current.settings != shown {
                    current.settings = shown;
                    document_changed = true;
                }
                current.crop = image.crop;
                if current.mask_colours != image.mask_colours
                    || current.mask_tolerance != image.mask_tolerance
                {
                    current.mask_colours.clone_from(&image.mask_colours);
                    current.mask_tolerance = image.mask_tolerance;
                    document_changed = true;
                }
            }
        }
        if apply_all {
            let settings = image.settings;
            let ids = self.editor.selection.ids().to_vec();
            let mut applied = 0;
            for object in &mut self.editor.document.objects {
                if ids.iter().any(|id| id == &object.id) {
                    if let crate::document::Shape::Image(target) = &mut object.shape {
                        target.settings = settings;
                        applied += 1;
                    }
                }
            }
            self.status = format!("Applied the image settings to {applied} image(s)");
            document_changed |= applied > 0;
        }
        if settings_changed || document_changed {
            ctx.request_repaint();
        }
        if close {
            if let Some(snapshot) = self.image_adjust_snapshot.take() {
                if snapshot != self.editor.document {
                    self.editor.history.record(&snapshot, "Adjust image");
                }
            }
            self.image_adjust_edit = None;
            self.image_compare = false;
            self.image_adjust = None;
        }
    }

    /// Histogram of the prepared image for the image dialog, cached by settings.
    fn image_histogram_bins(
        &mut self,
        id: &str,
        image: &crate::objects::ImageObject,
    ) -> [u64; 256] {
        if let Some(cache) = &self.image_histogram {
            if cache.object_id == id
                && cache.settings == image.settings
                && cache.crop == image.crop
                && cache.mask_colours == image.mask_colours
                && cache.mask_tolerance == image.mask_tolerance
                && cache.mask_paths == image.mask_paths
                && cache.mask_invert == image.mask_invert
                && cache.mask_alpha == image.mask_alpha
                && cache.mask_alpha_threshold == image.mask_alpha_threshold
            {
                return cache.bins;
            }
        }
        let bins = crate::raster::histogram_preview(
            std::path::Path::new(&image.path),
            image,
            crate::raster::PREVIEW_MAX_SIDE,
        )
        .unwrap_or([0u64; 256]);
        self.image_histogram = Some(ImageHistogram {
            object_id: id.to_string(),
            settings: image.settings,
            crop: image.crop,
            mask_colours: image.mask_colours.clone(),
            mask_tolerance: image.mask_tolerance,
            mask_paths: image.mask_paths.clone(),
            mask_invert: image.mask_invert,
            mask_alpha: image.mask_alpha,
            mask_alpha_threshold: image.mask_alpha_threshold,
            bins,
        });
        bins
    }
}

/// Draw the prepared-tone histogram under the previews.
///
/// Heights use a square-root scale so the long tail of a photo stays visible
/// instead of collapsing under one dominant tone.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn draw_histogram(ui: &mut egui::Ui, bins: &[u64; 256], settings: &crate::objects::ImageSettings) {
    ui.label("Histogram");
    let width = ui.available_width().clamp(200.0, 720.0);
    let (response, painter) = ui.allocate_painter(egui::vec2(width, 64.0), egui::Sense::hover());
    let rect = response.rect;
    let visuals = ui.visuals();
    painter.rect_filled(rect, 2.0, visuals.extreme_bg_color);
    let Some(max) = bins.iter().copied().max().filter(|value| *value > 0) else {
        return;
    };
    let scale = (max as f64).sqrt();
    let slot = rect.width() / 256.0;
    let colour = visuals.selection.bg_fill;
    for (index, count) in bins.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        let height = ((*count as f64).sqrt() / scale) as f32 * rect.height();
        let x = rect.left() + index as f32 * slot;
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x, rect.bottom() - height),
                egui::pos2(x + slot.max(1.0), rect.bottom()),
            ),
            0.0,
            colour,
        );
    }
    // Shade the tones the clip window discards, so the level settings are
    // visible against the actual histogram.
    let position = |value: u8| rect.left() + f32::from(value) / 255.0 * rect.width();
    if settings.black_clip > 0 {
        painter.rect_filled(
            egui::Rect::from_min_max(
                rect.min,
                egui::pos2(position(settings.black_clip), rect.bottom()),
            ),
            0.0,
            egui::Color32::from_black_alpha(96),
        );
    }
    if settings.white_clip < 255 {
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(position(settings.white_clip), rect.top()),
                rect.max,
            ),
            0.0,
            egui::Color32::from_black_alpha(96),
        );
    }
    for value in [settings.black_clip, settings.white_clip] {
        let x = position(value);
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, visuals.text_color()),
        );
    }
}

/// One palette swatch that toggles a colour in the image colour mask.
fn image_mask_colour_button(
    ui: &mut egui::Ui,
    image: &mut crate::objects::ImageObject,
    colour: &str,
) {
    let (r, g, b) = crate::color::parse_hex_color(colour).unwrap_or((0, 0, 0));
    let selected = image
        .mask_colours
        .iter()
        .any(|value| value.eq_ignore_ascii_case(colour));
    let response = ui.add(
        egui::Button::new("")
            .fill(egui::Color32::from_rgb(r, g, b))
            .min_size(egui::vec2(18.0, 18.0))
            .stroke(if selected {
                egui::Stroke::new(2.0, egui::Color32::WHITE)
            } else {
                egui::Stroke::NONE
            }),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            format!("Mask colour {colour}"),
        )
    });
    let tip = if selected {
        format!("Stop masking {colour}")
    } else {
        format!("Mask {colour}")
    };
    if response.on_hover_text(tip).clicked() {
        if selected {
            image
                .mask_colours
                .retain(|value| !value.eq_ignore_ascii_case(colour));
        } else {
            image.mask_colours.push(colour.to_string());
        }
    }
}
