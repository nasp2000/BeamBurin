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
        let mut target_message: Option<String> = None;
        let mut profile_saved: Option<String> = None;
        egui::Window::new(format!("Adjust image - {object_name}"))
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .collapsible(false)
            .resizable(true)
            .default_size([980.0, 760.0])
            .max_height(820.0)
            .show(ctx, |ui| {
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
                let preview_width = ((ui.available_width() - 24.0) / 2.0).clamp(220.0, 420.0);
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.label("Original");
                        if let Some(texture) = &original_texture {
                            ui.add(
                                egui::Image::new(texture)
                                    .max_size(egui::vec2(preview_width, 340.0))
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
                            let label = if (zoom - 1.0).abs() < f32::EPSILON {
                                "1:1".to_string()
                            } else if zoom > 0.0 {
                                format!("{zoom:.0}×")
                            } else {
                                "Fit".to_string()
                            };
                            egui::ComboBox::from_id_salt("image_preview_zoom")
                                .selected_text(label)
                                .width(70.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut zoom, 0.0, "Fit");
                                    ui.selectable_value(&mut zoom, 1.0, "1:1");
                                    ui.selectable_value(&mut zoom, 2.0, "2×");
                                    ui.selectable_value(&mut zoom, 4.0, "4×");
                                });
                        });
                        if let Some(texture) = burn_texture.as_ref().or(processed_texture.as_ref())
                        {
                            if zoom > 0.0 {
                                egui::ScrollArea::both()
                                    .id_salt("image_preview_zoom_scroll")
                                    .max_width(preview_width)
                                    .max_height(340.0)
                                    .auto_shrink([false, false])
                                    .show(ui, |ui| {
                                        ui.add(
                                            egui::Image::new(texture)
                                                .fit_to_exact_size(texture.size_vec2() * zoom),
                                        );
                                    });
                            } else {
                                ui.add(
                                    egui::Image::new(texture)
                                        .max_size(egui::vec2(320.0, 220.0))
                                        .shrink_to_fit(),
                                );
                            }
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
                                if let Some((_, _, seconds)) = crate::raster::estimate_engraving(
                                    preview.stats,
                                    image.width_mm(),
                                    *speed,
                                ) {
                                    let seconds = seconds * f64::from(*passes);
                                    let minutes = seconds / 60.0;
                                    let time = if minutes >= 1.5 {
                                        format!("{minutes:.1} min")
                                    } else {
                                        format!("{seconds:.0} s")
                                    };
                                    ui.weak(format!("Estimated: {time} at this cut entry"));
                                }
                            }
                        }
                    });
                });
                draw_histogram(ui, &histogram, &image.settings);
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
                        ui.horizontal(|ui| {
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
                                                )
                                                .on_hover_text(candidate.hint());
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
                                    ui.label("Enhance denoise");
                                    ui.add(
                                        egui::DragValue::new(&mut image.settings.enhance_denoise)
                                            .speed(0.1)
                                            .range(0.0..=10.0)
                                            .fixed_decimals(1),
                                    )
                                    .on_hover_text(
                                        "Keep low-contrast noise in smooth areas from being sharpened",
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
                                        let selected = self.selected_image_profile.as_deref()
                                            == Some(profile.id.as_str());
                                        if ui.selectable_label(selected, &profile.name).clicked() {
                                            preset = Some(profile.settings);
                                            self.selected_image_profile =
                                                Some(profile.id.clone());
                                        }
                                    }
                                });
                            let selected_profile = self.selected_image_profile.clone();
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
                                profile_saved = Some(format!("Saved \"{name}\" as an image profile"));
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
            if cache.object_id == id && cache.settings == image.settings && cache.crop == image.crop
            {
                return cache.bins;
            }
        }
        let bins = crate::raster::histogram(std::path::Path::new(&image.path), image)
            .unwrap_or([0u64; 256]);
        self.image_histogram = Some(ImageHistogram {
            object_id: id.to_string(),
            settings: image.settings,
            crop: image.crop,
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
