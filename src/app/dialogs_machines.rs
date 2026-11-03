#![allow(clippy::wildcard_imports)]

use super::*;

impl BeamBurinApp {
    pub(super) fn machine_wizard(&mut self, ctx: &egui::Context) {
        let mut create = false;
        let mut skip = false;
        egui::Window::new("Welcome - machine profile")
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(
                "Save the work area of your machine so new projects start with the right size.",
            );
            ui.label(
                "This is preparation data only: BeamBurin never connects to, configures or controls a machine.",
            );
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut self.machine_editor.name);
            });
            ui.horizontal(|ui| {
                ui.label("Work width");
                ui.add(
                    egui::DragValue::new(&mut self.machine_editor.work_width_mm)
                        .speed(1.0)
                        .range(10.0..=100_000.0)
                        .suffix(" mm"),
                );
                ui.label("Height");
                ui.add(
                    egui::DragValue::new(&mut self.machine_editor.work_height_mm)
                        .speed(1.0)
                        .range(10.0..=100_000.0)
                        .suffix(" mm"),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Firmware");
                let mut firmware = self.machine_editor.firmware;
                egui::ComboBox::from_id_salt("wizard_firmware")
                    .selected_text(firmware.label())
                    .show_ui(ui, |ui| {
                        for candidate in crate::machines::Firmware::all() {
                            ui.selectable_value(
                                &mut firmware,
                                candidate,
                                candidate.label(),
                            );
                        }
                    });
                self.machine_editor.firmware = firmware;
            });
            ui.weak("The firmware is a label for the intended G-code dialect; nothing is connected or configured.");
            if let Some(error) = &self.machine_error {
                ui.colored_label(Color32::from_rgb(200, 60, 60), error);
            }
            ui.horizontal(|ui| {
                if ui.button("Create profile").clicked() {
                    create = true;
                }
                if ui.button("Skip for now").clicked() {
                    skip = true;
                }
            });
        });
        if create {
            match self.machines.add(self.machine_editor.clone()) {
                Ok(()) => {
                    self.machines.first_run_done = true;
                    let _ = self.machines.save(&self.machines_path);
                    self.apply_machine_work_area();
                    self.machine_wizard_open = false;
                    self.status = format!("Machine profile '{}' created", self.machine_editor.name);
                }
                Err(error) => self.machine_error = Some(error),
            }
        } else if skip {
            self.machines.first_run_done = true;
            let _ = self.machines.save(&self.machines_path);
            self.machine_wizard_open = false;
        }
    }

    pub(super) fn machines_window(&mut self, ctx: &egui::Context) {
        let mut open = self.machines_open;
        egui::Window::new("Machine profiles")
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([560.0, 440.0])
        .max_height(560.0)
        .show(ctx, |ui| {
            ui.label(
                "Work areas used to prepare projects. No connection or controller settings are stored.",
            );
            ui.horizontal(|ui| {
                if ui.button("Import...").clicked() {
                    self.open_dialog(FileMode::ImportMachines);
                }
                if ui.button("Export...").clicked() {
                    self.open_dialog(FileMode::ExportMachines);
                }
                if ui.button("Add common presets").clicked() {
                    let added = self.machines.add_common_presets();
                    let _ = self.machines.save(&self.machines_path);
                    self.status = format!("Added {added} machine preset(s)");
                }
            });
            ui.separator();
            let machines = self.machines.machines.clone();
            egui::ScrollArea::vertical()
                .id_salt("machines_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.heading("Saved");
                    for machine in &machines {
                        if ui
                            .selectable_label(
                                self.machine_edit_id.as_deref() == Some(&machine.id),
                                format!(
                                    "{} [{}] ({:.0} x {:.0} mm)",
                                    machine.name,
                                    machine.firmware.label(),
                                    machine.work_width_mm,
                                    machine.work_height_mm
                                ),
                            )
                            .clicked()
                        {
                            self.machine_edit_id = Some(machine.id.clone());
                            self.machine_editor = machine.clone();
                            self.machine_error = None;
                        }
                    }
                    if machines.is_empty() {
                        ui.weak("No profiles yet.");
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.heading("Profile");
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(&mut self.machine_editor.name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Work width");
                        ui.add(
                            egui::DragValue::new(&mut self.machine_editor.work_width_mm)
                                .speed(1.0)
                                .range(10.0..=100_000.0)
                                .suffix(" mm"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Work height");
                        ui.add(
                            egui::DragValue::new(&mut self.machine_editor.work_height_mm)
                                .speed(1.0)
                                .range(10.0..=100_000.0)
                                .suffix(" mm"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Firmware");
                        let mut firmware = self.machine_editor.firmware;
                        egui::ComboBox::from_id_salt("machine_firmware")
                            .selected_text(firmware.label())
                            .show_ui(ui, |ui| {
                                for candidate in crate::machines::Firmware::all() {
                                    ui.selectable_value(
                                        &mut firmware,
                                        candidate,
                                        candidate.label(),
                                    );
                                }
                            });
                        self.machine_editor.firmware = firmware;
                    });
                    ui.label("Notes");
                    ui.text_edit_multiline(&mut self.machine_editor.notes);
                    if let Some(error) = &self.machine_error {
                        ui.colored_label(Color32::from_rgb(200, 60, 60), error);
                    }
                    ui.horizontal(|ui| {
                        if ui.button("New").clicked() {
                            self.machine_editor = crate::machines::MachineProfile::default();
                            self.machine_edit_id = None;
                            self.machine_error = None;
                        }
                        if ui.button("Save profile").clicked() {
                            match self.machine_editor.validate() {
                                Ok(()) => {
                                    if let Some(id) = &self.machine_edit_id {
                                        if let Some(index) = self
                                            .machines
                                            .machines
                                            .iter()
                                            .position(|machine| &machine.id == id)
                                        {
                                            self.machines.machines[index] =
                                                self.machine_editor.clone();
                                        }
                                    } else {
                                        match self
                                            .machines
                                            .add(self.machine_editor.clone())
                                        {
                                            Ok(()) => {
                                                self.machine_edit_id =
                                                    Some(self.machine_editor.id.clone());
                                            }
                                            Err(error) => {
                                                self.machine_error = Some(error);
                                            }
                                        }
                                    }
                                    match self.machines.validate() {
                                        Ok(()) => match self.machines.save(&self.machines_path)
                                        {
                                            Ok(()) => {
                                                self.status = "Machine profiles saved"
                                                    .to_string();
                                                self.machine_error = None;
                                            }
                                            Err(error) => {
                                                self.machine_error = Some(error);
                                            }
                                        },
                                        Err(error) => self.machine_error = Some(error),
                                    }
                                }
                                Err(error) => self.machine_error = Some(error),
                            }
                        }
                        if ui.button("Use work area").clicked() {
                            self.apply_machine_work_area();
                        }
                        if ui.button("Delete").clicked() {
                            if let Some(id) = self.machine_edit_id.take() {
                                self.machines.remove(&id);
                                let _ = self.machines.save(&self.machines_path);
                                self.machine_editor =
                                    crate::machines::MachineProfile::default();
                            }
                        }
                    });
                });
            });
                });
        });
        self.machines_open = open;
    }

    pub(super) fn apply_machine_work_area(&mut self) {
        self.editor.record_action("Machine work area");
        self.editor.document.width_mm = self.machine_editor.work_width_mm;
        self.editor.document.height_mm = self.machine_editor.work_height_mm;
        self.needs_fit = true;
        self.status = format!(
            "Work area set to {:.0} x {:.0} mm",
            self.machine_editor.work_width_mm, self.machine_editor.work_height_mm
        );
    }

    pub(super) fn vectorize_selected_image(&mut self) {
        let Some(id) = self.editor.selection.ids().first().cloned() else {
            return;
        };
        let Some(object) = self
            .editor
            .document
            .objects
            .iter()
            .find(|object| object.id == id)
            .cloned()
        else {
            return;
        };
        let crate::document::Shape::Image(image) = &object.shape else {
            return;
        };
        let Some((min, max)) = object.bounds() else {
            return;
        };
        let bitmap = match crate::raster::process_object(std::path::Path::new(&image.path), image) {
            Ok(bitmap) => bitmap,
            Err(error) => {
                self.status = format!("Cannot read the image: {error}");
                return;
            }
        };
        let contours = crate::vectorize::trace(&bitmap, max.x - min.x, max.y - min.y, 0.2);
        if contours.is_empty() {
            self.status = "The image has no burnable areas to vectorize".to_string();
            return;
        }
        self.editor.record_action("Vectorize image");
        let groups = crate::vectorize::group_contours(&contours);
        let mut created = Vec::new();
        let mut holes = 0;
        for group in &groups {
            let Some(outer_index) = group.first() else {
                continue;
            };
            let operation_id = self
                .editor
                .document
                .objects
                .iter()
                .find(|candidate| candidate.id == id)
                .and_then(|candidate| candidate.operation_id.clone());
            let mut outline = crate::document::Object::new(
                object.layer_id.clone(),
                crate::document::Shape::Path(crate::vectorize::to_curved_path(
                    &contours[*outer_index],
                    40.0,
                )),
            );
            outline.name = "Vectorized contour".to_string();
            outline.transform = crate::geometry::Transform2::translate(min.x, min.y);
            outline.operation_id.clone_from(&operation_id);
            outline.style.stroke = Some("#000000".to_string());
            outline.style.fill = None;
            let outer_id = self.editor.document.add_object(outline).id.clone();
            created.push(outer_id.clone());
            for hole_index in &group[1..] {
                let mut hole = crate::document::Object::new(
                    object.layer_id.clone(),
                    crate::document::Shape::Path(crate::vectorize::to_curved_path(
                        &contours[*hole_index],
                        40.0,
                    )),
                );
                hole.name = "Vectorized hole".to_string();
                hole.transform = crate::geometry::Transform2::translate(min.x, min.y);
                hole.operation_id.clone_from(&operation_id);
                hole.hole_of = Some(outer_id.clone());
                hole.style.stroke = Some("#000000".to_string());
                hole.style.fill = None;
                created.push(self.editor.document.add_object(hole).id.clone());
                holes += 1;
            }
        }
        self.editor.selection.clear();
        for new_id in &created {
            self.editor.selection.add(new_id.clone());
        }
        self.status = format!(
            "Vectorized into {} contour(s) with {holes} hole(s)",
            created.len() - holes
        );
    }

    pub(super) fn centerline_selected_image(&mut self) {
        let Some(id) = self.editor.selection.ids().first().cloned() else {
            return;
        };
        let Some(object) = self
            .editor
            .document
            .objects
            .iter()
            .find(|object| object.id == id)
            .cloned()
        else {
            return;
        };
        let crate::document::Shape::Image(image) = &object.shape else {
            return;
        };
        let Some((min, max)) = object.bounds() else {
            return;
        };
        let bitmap = match crate::raster::process_object(std::path::Path::new(&image.path), image) {
            Ok(bitmap) => bitmap,
            Err(error) => {
                self.status = format!("Cannot read the image: {error}");
                return;
            }
        };
        let lines = crate::centerline::trace(&bitmap, max.x - min.x, max.y - min.y, 0.2);
        if lines.is_empty() {
            self.status = "The image has no strokes to trace".to_string();
            return;
        }
        self.editor.record_action("Trace centerline");
        let operation_id = self
            .editor
            .document
            .objects
            .iter()
            .find(|candidate| candidate.id == id)
            .and_then(|candidate| candidate.operation_id.clone());
        let mut created = Vec::new();
        for line in &lines {
            let mut new_object = crate::document::Object::new(
                object.layer_id.clone(),
                crate::document::Shape::Path(crate::centerline::to_path(line)),
            );
            new_object.name = "Centerline".to_string();
            new_object.transform = crate::geometry::Transform2::translate(min.x, min.y);
            new_object.operation_id.clone_from(&operation_id);
            new_object.style.stroke = Some("#000000".to_string());
            new_object.style.fill = None;
            created.push(self.editor.document.add_object(new_object).id.clone());
        }
        self.editor.selection.clear();
        for new_id in &created {
            self.editor.selection.add(new_id.clone());
        }
        self.status = format!("Traced {} centerline(s)", created.len());
    }

    pub(super) fn import_materials_csv(&mut self, path: &Path) {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                self.status = format!("Cannot read the CSV: {error}");
                return;
            }
        };
        match crate::profiles::csv::import_materials_csv(&mut self.profiles, &text) {
            Ok(count) => {
                match crate::profiles::library::save(&self.profiles, &self.profiles_path) {
                    Ok(()) => self.status = format!("Imported {count} material(s) from CSV"),
                    Err(error) => self.status = format!("Cannot save the library: {error}"),
                }
            }
            Err(error) => self.status = format!("CSV import failed: {error}"),
        }
    }

    pub(super) fn export_materials_csv(&mut self, path: &Path) {
        let csv = crate::profiles::csv::export_materials_csv(&self.profiles);
        match std::fs::write(path, csv) {
            Ok(()) => self.status = format!("Exported materials to {}", path.display()),
            Err(error) => self.status = format!("CSV export failed: {error}"),
        }
    }

    pub(super) fn preview_external_gcode(&mut self, path: &Path) {
        match crate::gcode::external::parse_file(path) {
            Ok(program) => {
                self.status = format!(
                    "External G-code: {} cut move(s) in {} line(s)",
                    program.cut_points(),
                    program.lines
                );
                self.external_gcode = Some((path.to_path_buf(), program));
                self.external_gcode_open = true;
            }
            Err(error) => self.status = format!("Cannot read the G-code: {error}"),
        }
    }

    pub(super) fn burn_preview_texture(
        &mut self,
        ctx: &egui::Context,
        id: &str,
        image: &crate::objects::ImageObject,
    ) -> Option<egui::TextureHandle> {
        let current = self.burn_preview.as_ref().is_some_and(|preview| {
            preview.object_id == id
                && preview.settings == image.settings
                && preview.crop == image.crop
        });
        if !current {
            let bitmap =
                crate::raster::process_object(std::path::Path::new(&image.path), image).ok()?;
            let texture = ctx.load_texture(
                format!("burn-preview-{id}"),
                preview_color_image(&bitmap, true),
                egui::TextureOptions::NEAREST,
            );
            self.burn_preview = Some(BurnPreview {
                object_id: id.to_string(),
                settings: image.settings,
                crop: image.crop,
                texture,
            });
        }
        self.burn_preview
            .as_ref()
            .map(|preview| preview.texture.clone())
    }

    pub(super) fn open_image_adjust(&mut self) {
        let Some(id) = self.editor.selection.ids().first().cloned() else {
            return;
        };
        let Some(object) = self
            .editor
            .document
            .objects
            .iter()
            .find(|object| object.id == id)
        else {
            return;
        };
        let crate::document::Shape::Image(image) = &object.shape else {
            return;
        };
        self.image_adjust_snapshot = Some(self.editor.document.clone());
        self.image_adjust = Some(id);
        self.image_target_dpi = 254.0;
        self.image_target_width_mm = image.width_mm();
    }

    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    pub(super) fn original_image_texture(
        &mut self,
        ctx: &egui::Context,
        image: &crate::objects::ImageObject,
    ) -> Option<egui::TextureHandle> {
        let key = format!(
            "{}|{:?}|{}",
            image.path, image.crop, image.settings.rotation_degrees
        );
        if let Some(texture) = self.image_original_previews.get(&key) {
            return Some(texture.clone());
        }
        let mut picture = image::open(&image.path).ok()?;
        if let Some(crop) = image.crop {
            picture = picture.crop_imm(crop.x, crop.y, crop.width, crop.height);
        }
        let rotation = image.settings.rotation_degrees;
        if (rotation - 90.0).abs() < f64::EPSILON {
            picture = picture.rotate90();
        } else if (rotation - 180.0).abs() < f64::EPSILON {
            picture = picture.rotate180();
        } else if (rotation - 270.0).abs() < f64::EPSILON {
            picture = picture.rotate270();
        }
        let width = picture.width().max(1);
        let height = picture.height().max(1);
        let scale = (480.0 / width.max(height) as f32).min(1.0);
        if scale < 1.0 {
            let target_width = ((width as f32) * scale).max(1.0) as u32;
            let target_height = ((height as f32) * scale).max(1.0) as u32;
            picture = picture.resize(
                target_width,
                target_height,
                image::imageops::FilterType::Triangle,
            );
        }
        let rgba = picture.to_rgba8();
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [rgba.width() as usize, rgba.height() as usize],
            rgba.as_raw(),
        );
        let texture = ctx.load_texture(
            format!("image-original-{key}"),
            color,
            egui::TextureOptions::LINEAR,
        );
        self.image_original_previews.insert(key, texture.clone());
        Some(texture)
    }
}
