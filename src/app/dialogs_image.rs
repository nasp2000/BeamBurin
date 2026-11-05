#![allow(clippy::wildcard_imports)]

use super::*;

impl BeamBurinApp {
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
            self.image_adjust = None;
            return;
        };
        let crate::document::Shape::Image(image) = &self.editor.document.objects[index].shape
        else {
            self.image_adjust = None;
            return;
        };
        let mut image = image.clone();
        let object_name = self.editor.document.objects[index].name.clone();
        let original = image.clone();
        let mut burn_map = self.image_burn_map;
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

        let mut close = false;
        let mut cancel = false;
        let mut reset = false;
        let mut export_prepared = false;
        let mut apply_all = false;
        let mut compare = false;
        let mut open_test_grid = false;
        let mut preset: Option<crate::objects::ImageSettings> = None;
        let mut auto_levels = false;
        egui::Window::new(format!("Adjust image - {object_name}"))
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .collapsible(false)
            .resizable(true)
            .default_size([880.0, 600.0])
            .max_height(640.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Original");
                        if let Some(texture) = &original_texture {
                            ui.add(
                                egui::Image::new(texture)
                                    .max_size(egui::vec2(320.0, 220.0))
                                    .shrink_to_fit(),
                            );
                        } else {
                            ui.weak("Cannot read the image file");
                        }
                        ui.weak(format!("{} x {} px", image.width_px, image.height_px));
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("Preview");
                            ui.checkbox(&mut burn_map, "Burn map");
                        });
                        if let Some(texture) = burn_texture.as_ref().or(processed_texture.as_ref())
                        {
                            ui.add(
                                egui::Image::new(texture)
                                    .max_size(egui::vec2(320.0, 220.0))
                                    .shrink_to_fit(),
                            );
                        } else {
                            ui.weak("No preview available");
                        }
                        let (work_width, work_height) = image.effective_size_px();
                        ui.weak(format!(
                            "{work_width} x {work_height} px at {:.0} dpi",
                            image.effective_dpi()
                        ));
                        ui.weak(format!(
                            "{:.1} x {:.1} mm at {:.0} dpi",
                            image.width_mm(),
                            image.height_mm(),
                            image.effective_dpi()
                        ));
                    });
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("image_adjust_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            egui::Grid::new("image_target_resolution")
                                .num_columns(2)
                                .spacing([12.0, 6.0])
                                .show(ui, |ui| {
                                    ui.label("Target DPI");
                                    ui.add(
                                        egui::DragValue::new(&mut target_dpi)
                                            .speed(1.0)
                                            .range(50.0..=1000.0)
                                            .fixed_decimals(0),
                                    );
                                    ui.end_row();
                                    ui.label("Target width (mm)");
                                    ui.add(
                                        egui::DragValue::new(&mut target_width_mm)
                                            .speed(0.5)
                                            .range(1.0..=10000.0)
                                            .fixed_decimals(1)
                                            .suffix(" mm"),
                                    );
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
                        ui.separator();
                        ui.horizontal(|ui| {
                            egui::Grid::new("image_adjust_layers")
                                .num_columns(2)
                                .spacing([12.0, 6.0])
                                .show(ui, |ui| {
                                    ui.label("Image mode");
                                    let mut mode = image.settings.mode;
                                    egui::ComboBox::from_id_salt("image_adjust_mode")
                                        .selected_text(mode.label())
                                        .show_ui(ui, |ui| {
                                            for candidate in crate::objects::ImageMode::all() {
                                                ui.selectable_value(
                                                    &mut mode,
                                                    candidate,
                                                    candidate.label(),
                                                );
                                            }
                                        });
                                    image.settings.mode = mode;
                                    ui.end_row();
                                    ui.label("Negative image");
                                    let negative = ui.checkbox(&mut image.settings.invert, "");
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
                                    ui.label("Line interval (mm)");
                                    ui.add(
                                        egui::DragValue::new(&mut image.settings.line_interval_mm)
                                            .speed(0.01)
                                            .range(0.01..=2.0),
                                    );
                                    ui.end_row();
                                    ui.label("Halftone angle");
                                    ui.add(
                                        egui::DragValue::new(
                                            &mut image.settings.halftone_angle_degrees,
                                        )
                                        .speed(0.5)
                                        .range(-360.0..=360.0)
                                        .suffix(" deg"),
                                    );
                                    ui.end_row();
                                    ui.label("DPI");
                                    ui.weak(format!("{:.0}", image.effective_dpi()));
                                    ui.end_row();
                                });
                            ui.separator();
                            egui::Grid::new("image_adjust_settings")
                                .num_columns(2)
                                .spacing([12.0, 6.0])
                                .show(ui, |ui| {
                                    ui.label("Contrast");
                                    ui.add(
                                        egui::Slider::new(&mut image.settings.contrast, 0.25..=3.0)
                                            .fixed_decimals(2),
                                    );
                                    ui.end_row();
                                    ui.label("Brightness");
                                    ui.add(
                                        egui::Slider::new(
                                            &mut image.settings.brightness,
                                            -100..=100,
                                        )
                                        .fixed_decimals(0),
                                    );
                                    ui.end_row();
                                    ui.label("Shadows");
                                    ui.add(
                                        egui::Slider::new(
                                            &mut image.settings.shadow_lift,
                                            -1.0..=1.0,
                                        )
                                        .fixed_decimals(2),
                                    );
                                    ui.end_row();
                                    ui.label("Midtones");
                                    ui.add(
                                        egui::Slider::new(
                                            &mut image.settings.midtone_shift,
                                            -1.0..=1.0,
                                        )
                                        .fixed_decimals(2),
                                    );
                                    ui.end_row();
                                    ui.label("Highlights");
                                    ui.add(
                                        egui::Slider::new(
                                            &mut image.settings.highlight_lift,
                                            -1.0..=1.0,
                                        )
                                        .fixed_decimals(2),
                                    );
                                    ui.end_row();
                                    ui.label("Tone curve");
                                    if ui.button("Reset curve").clicked() {
                                        image.settings.shadow_lift = 0.0;
                                        image.settings.midtone_shift = 0.0;
                                        image.settings.highlight_lift = 0.0;
                                    }
                                    ui.end_row();
                                    ui.label("Gamma");
                                    ui.add(
                                        egui::Slider::new(&mut image.settings.gamma, 0.2..=3.0)
                                            .fixed_decimals(2),
                                    );
                                    ui.end_row();
                                    ui.label("Enhance radius");
                                    ui.add(
                                        egui::DragValue::new(&mut image.settings.enhance_radius_px)
                                            .range(0..=32)
                                            .suffix(" px"),
                                    );
                                    ui.end_row();
                                    ui.label("Black / white clip");
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::DragValue::new(&mut image.settings.black_clip)
                                                .range(0..=254),
                                        );
                                        ui.add(
                                            egui::DragValue::new(&mut image.settings.white_clip)
                                                .range(1..=255),
                                        );
                                    });
                                    ui.end_row();
                                    ui.label("Enhance amount");
                                    ui.add(
                                        egui::DragValue::new(&mut image.settings.enhance_amount)
                                            .speed(0.05)
                                            .range(0.0..=5.0)
                                            .fixed_decimals(2),
                                    );
                                    ui.end_row();
                                    ui.label("Grey mix");
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::DragValue::new(&mut image.settings.gray_red)
                                                .speed(0.01)
                                                .range(0.0..=1.0)
                                                .fixed_decimals(2),
                                        )
                                        .on_hover_text("Red weight");
                                        ui.add(
                                            egui::DragValue::new(&mut image.settings.gray_green)
                                                .speed(0.01)
                                                .range(0.0..=1.0)
                                                .fixed_decimals(2),
                                        )
                                        .on_hover_text("Green weight");
                                        ui.add(
                                            egui::DragValue::new(&mut image.settings.gray_blue)
                                                .speed(0.01)
                                                .range(0.0..=1.0)
                                                .fixed_decimals(2),
                                        )
                                        .on_hover_text("Blue weight");
                                    });
                                    ui.end_row();
                                    ui.label("Despeckle");
                                    ui.checkbox(&mut image.settings.despeckle, "3×3 median");
                                    ui.end_row();
                                });
                        });
                        ui.separator();
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("image_adjust_presets")
                                .selected_text("Presets")
                                .width(180.0)
                                .show_ui(ui, |ui| {
                                    for profile in &self.profiles.images {
                                        if ui.selectable_label(false, &profile.name).clicked() {
                                            preset = Some(profile.settings);
                                        }
                                    }
                                });
                            if ui.button("Reset all").clicked() {
                                reset = true;
                            }
                            if ui
                                .button("Auto levels")
                                .on_hover_text(
                                    "Set the black and white clip from the image histogram",
                                )
                                .clicked()
                            {
                                auto_levels = true;
                            }
                            if ui.button("Export prepared PNG").clicked() {
                                export_prepared = true;
                            }
                            if ui.button("Test grid...").clicked() {
                                open_test_grid = true;
                            }
                            if ui.button("Apply to selected images").clicked() {
                                apply_all = true;
                            }
                            ui.checkbox(&mut compare, "Compare with original");
                            if ui.button("Photo").clicked() {
                                preset = Some(crate::objects::ImageSettings {
                                    mode: crate::objects::ImageMode::Jarvis,
                                    contrast: 1.15,
                                    gamma: 0.9,
                                    enhance_radius_px: 1,
                                    enhance_amount: 0.6,
                                    ..crate::objects::ImageSettings::default()
                                });
                            }
                            if ui.button("Wood").clicked() {
                                preset = Some(crate::objects::ImageSettings {
                                    mode: crate::objects::ImageMode::Stucki,
                                    contrast: 1.25,
                                    gamma: 1.1,
                                    ..crate::objects::ImageSettings::default()
                                });
                            }
                            if ui.button("Metal").clicked() {
                                preset = Some(crate::objects::ImageSettings {
                                    mode: crate::objects::ImageMode::Atkinson,
                                    contrast: 1.4,
                                    gamma: 1.2,
                                    ..crate::objects::ImageSettings::default()
                                });
                            }
                            if ui.button("Acrylic").clicked() {
                                preset = Some(crate::objects::ImageSettings {
                                    mode: crate::objects::ImageMode::FloydSteinberg,
                                    contrast: 1.1,
                                    gamma: 0.95,
                                    ..crate::objects::ImageSettings::default()
                                });
                            }
                            if ui.button("Newsprint").clicked() {
                                preset = Some(crate::objects::ImageSettings {
                                    mode: crate::objects::ImageMode::Newsprint,
                                    contrast: 1.2,
                                    ..crate::objects::ImageSettings::default()
                                });
                            }
                            if ui.button("Sketch").clicked() {
                                preset = Some(crate::objects::ImageSettings {
                                    mode: crate::objects::ImageMode::Sketch,
                                    contrast: 1.3,
                                    ..crate::objects::ImageSettings::default()
                                });
                            }
                        });
                    });
                ui.separator();
                ui.horizontal(|ui| {
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

        self.image_burn_map = burn_map;
        self.image_target_dpi = target_dpi;
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
            match crate::raster::auto_levels(std::path::Path::new(&image.path), &image) {
                Ok((black, white)) => {
                    image.settings.black_clip = black;
                    image.settings.white_clip = white;
                    self.status = format!("Auto levels: black {black}, white {white}");
                }
                Err(error) => self.status = format!("Cannot analyse the image: {error}"),
            }
        }
        if cancel {
            self.image_adjust_snapshot = None;
            if let Some(current) = self
                .editor
                .document
                .objects
                .iter_mut()
                .find(|object| object.id == id)
            {
                if let crate::document::Shape::Image(current) = &mut current.shape {
                    current.settings = original.settings;
                }
            }
            self.image_adjust = None;
            return;
        }
        let settings_changed = image.settings != original.settings;
        if let Some(current) = self
            .editor
            .document
            .objects
            .iter_mut()
            .find(|object| object.id == id)
        {
            if let crate::document::Shape::Image(current) = &mut current.shape {
                current.settings = if compare {
                    original.settings
                } else {
                    image.settings
                };
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
        }
        if settings_changed {
            ctx.request_repaint();
        }
        if close {
            if let Some(snapshot) = self.image_adjust_snapshot.take() {
                if snapshot != self.editor.document {
                    self.editor.history.record(&snapshot, "Adjust image");
                }
            }
            self.image_adjust = None;
        }
    }
}
