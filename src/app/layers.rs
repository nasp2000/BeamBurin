use super::{BeamBurinApp, Color32, FileMode, LayerRow, Ui, parse_hex_color, to_hex};

/// Drawing colours offered at the bottom of the window. They are also the
/// colours a layer can use for its cut configuration.
pub(super) const DRAWING_COLOURS: [&str; 20] = [
    "#000000", "#1e88e5", "#e53935", "#43a047", "#fdd835", "#8e24aa", "#00acc1", "#fb8c00",
    "#6d4c41", "#546e7a", "#d81b60", "#7cb342", "#3949ab", "#00897b", "#f4511e", "#c0ca33",
    "#5e35b1", "#039be5", "#ffb300", "#757575",
];

impl BeamBurinApp {
    /// The operation shared by every object of a layer, and whether the
    /// objects disagree. A layer has no operation of its own: this keeps the
    /// existing per-object assignment and presents it as one mode row.
    pub(super) fn layer_mode(&self, layer_id: &str) -> (Option<String>, bool) {
        if let Some(layer) = self.editor.document.layer(layer_id) {
            if let Some(operation_id) = &layer.operation_id {
                if self
                    .editor
                    .document
                    .operations
                    .iter()
                    .any(|operation| operation.id == *operation_id)
                {
                    return (Some(operation_id.clone()), false);
                }
            }
        }
        let mut ids: Vec<&str> = self
            .editor
            .document
            .objects_on_layer(layer_id)
            .iter()
            .filter_map(|object| object.operation_id.as_deref())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        match ids.len() {
            1 => (Some(ids[0].to_string()), false),
            0 => (None, false),
            _ => (None, true),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn layers_panel(&mut self, ui: &mut Ui) {
        use crate::operations::OperationType;

        egui::Panel::left("layers_panel")
            .default_size(310.0)
            .resizable(false)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                self.color_palette(ui);
                self.image_mask_tools(ui);
                ui.separator();
                ui.label(egui::RichText::new("Cuts / Layers").strong());
                let before = self.editor.document.clone();
                let mut message: Option<String> = None;
                ui.horizontal(|ui| {
                    if ui.button("+ Layer").clicked() {
                        let name = format!("Layer {}", self.editor.document.layers.len() + 1);
                        let used: Vec<String> = self
                            .editor
                            .document
                            .layers
                            .iter()
                            .map(|layer| layer.color.to_ascii_lowercase())
                            .collect();
                        let colour = DRAWING_COLOURS
                            .iter()
                            .find(|colour| !used.contains(&colour.to_ascii_lowercase()))
                            .copied()
                            .unwrap_or_else(|| {
                                DRAWING_COLOURS
                                    [self.editor.document.layers.len() % DRAWING_COLOURS.len()]
                            });
                        let layer = self.editor.document.add_layer(name);
                        let id = layer.id.clone();
                        if let Some(layer) = self.editor.document.layer_mut(&id) {
                            layer.color = colour.to_string();
                        }
                        let operation = self.editor.ensure_operation_for_colour(colour);
                        if let Some(layer) = self.editor.document.layer_mut(&id) {
                            layer.operation_id = Some(operation);
                        }
                        self.editor.active_layer_id = id;
                    }
                    if ui.button("- Layer").clicked() {
                        let id = self.editor.active_layer_id.clone();
                        match self.delete_layer(&id) {
                            Ok(()) => {
                                self.editor.active_layer_id = self
                                    .editor
                                    .document
                                    .layers
                                    .last()
                                    .map(|layer| layer.id.clone())
                                    .unwrap_or_default();
                            }
                            Err(error) => message = Some(error),
                        }
                    }
                    ui.weak("Right-click a row for cut settings");
                });

                // One row per layer, with the mode and the switches that matter
                // for the job. The remaining settings live in the row menu.
                let operations: Vec<(String, String, f64, f64, bool, bool)> = self
                    .editor
                    .document
                    .operations
                    .iter()
                    .map(|operation| {
                        (
                            operation.id.clone(),
                            operation.name.clone(),
                            operation.params.speed,
                            operation.params.power,
                            operation.enabled,
                            operation.params.air_assist,
                        )
                    })
                    .collect();
                let operation_types: Vec<(String, OperationType)> = self
                    .editor
                    .document
                    .operations
                    .iter()
                    .map(|operation| (operation.id.clone(), operation.op_type))
                    .collect();
                let rows: Vec<LayerRow> = self
                    .editor
                    .document
                    .layers
                    .iter()
                    .map(|layer| {
                        let (mode, mixed) = self.layer_mode(&layer.id);
                        (
                            layer.id.clone(),
                            layer.name.clone(),
                            self.layer_display_colour(&layer.id),
                            layer.visible,
                            layer.locked,
                            mode,
                            mixed,
                            self.editor.document.objects_on_layer(&layer.id).len(),
                        )
                    })
                    .collect();

                let mut mode_choice: Option<(String, OperationType)> = None;
                let mut toggle_operation: Option<(String, bool, bool)> = None;
                egui::Grid::new("layers_grid")
                    .num_columns(7)
                    .striped(true)
                    .spacing([2.0, 2.0])
                    .show(ui, |ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(46.0, 18.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.label(egui::RichText::new("Colour").strong());
                            },
                        );
                        for (header, width) in [
                            ("Mode", 98.0),
                            ("Spd/Pwr", 52.0),
                            ("Out", 18.0),
                            ("Show", 18.0),
                            ("Air", 18.0),
                            ("Order", 34.0),
                        ] {
                            ui.allocate_ui_with_layout(
                                egui::vec2(width, 18.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.label(egui::RichText::new(header).strong());
                                },
                            );
                        }
                        ui.end_row();
                        for row in &rows {
                            let (id, name, color, visible, _locked, mode, mixed, count) = row;
                            let selected = self.editor.active_layer_id == *id;
                            let operation = mode
                                .as_deref()
                                .and_then(|mode| operations.iter().find(|entry| entry.0 == mode));
                            ui.allocate_ui_with_layout(
                                egui::vec2(28.0, 18.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    let rgb = parse_hex_color(color).unwrap_or((0, 0, 0));
                                    let mut srgb = [rgb.0, rgb.1, rgb.2];
                                    let swatch = ui.color_edit_button_srgb(&mut srgb);
                                    swatch.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::WidgetType::Button,
                                            true,
                                            name.clone(),
                                        )
                                    });
                                    if selected {
                                        ui.painter().rect_stroke(
                                            swatch.rect.expand(1.0),
                                            2.0,
                                            egui::Stroke::new(1.5, Color32::WHITE),
                                            egui::StrokeKind::Outside,
                                        );
                                    }
                                    if swatch.changed() {
                                        let colour = to_hex(srgb[0], srgb[1], srgb[2]);
                                        self.set_layer_colour(id, &colour);
                                    }
                                    if swatch.clicked() {
                                        self.editor.active_layer_id.clone_from(id);
                                    }
                                    if swatch.secondary_clicked() {
                                        self.open_row_settings(id);
                                    }
                                    swatch.on_hover_text(format!(
                                        "{name} — {count} object(s); right-click for cut settings"
                                    ));
                                },
                            );
                            let current_type = if *mixed {
                                None
                            } else {
                                mode.as_deref().and_then(|mode| {
                                    operation_types
                                        .iter()
                                        .find(|entry| entry.0 == mode)
                                        .map(|entry| entry.1)
                                })
                            };
                            let mut chosen = current_type;
                            let text = if *mixed {
                                "Mixed".to_string()
                            } else {
                                current_type.map_or_else(
                                    || "—".to_string(),
                                    |kind| kind.short_label().to_string(),
                                )
                            };
                            let mode_combo =
                                egui::ComboBox::from_id_salt(format!("layer_mode_{id}"))
                                    .width(90.0)
                                    .selected_text(text)
                                    .show_ui(ui, |ui| {
                                        for candidate in OperationType::all() {
                                            if ui
                                                .selectable_value(
                                                    &mut chosen,
                                                    Some(candidate),
                                                    candidate.label(),
                                                )
                                                .clicked()
                                            {
                                                ui.close();
                                            }
                                        }
                                    });
                            mode_combo.response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::ComboBox,
                                    true,
                                    format!("{name} mode"),
                                )
                            });
                            if let Some(entry) = operation {
                                mode_combo.response.on_hover_text(format!(
                                    "{} · {:.0} mm/min · {:.0}%",
                                    entry.1, entry.2, entry.3
                                ));
                            }
                            if chosen != current_type {
                                if let Some(kind) = chosen {
                                    mode_choice = Some((id.clone(), kind));
                                }
                            }
                            let speeds = operation.map_or_else(
                                || "—".to_string(),
                                |entry| format!("{:.0} / {:.0}", entry.2, entry.3),
                            );
                            let speeds_response = ui.add_sized(
                                [52.0, 18.0],
                                egui::Label::new(speeds).sense(egui::Sense::click()),
                            );
                            if speeds_response.secondary_clicked() {
                                self.open_row_settings(id);
                            }
                            let mut output = operation.is_some_and(|entry| entry.4);
                            let output_response = ui
                                .push_id(("layer_output", id), |ui| {
                                    ui.add_enabled_ui(operation.is_some(), |ui| {
                                        ui.add_sized(
                                            [18.0, 18.0],
                                            egui::Checkbox::without_text(&mut output),
                                        )
                                    })
                                    .inner
                                })
                                .inner;
                            output_response.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::Checkbox,
                                    true,
                                    output,
                                    "Output",
                                )
                            });
                            if output_response.changed() {
                                if let Some(entry) = operation {
                                    toggle_operation = Some((entry.0.clone(), output, entry.5));
                                }
                            }
                            output_response.on_hover_text("Include this mode in the job");
                            let mut shown = *visible;
                            let show_response = ui
                                .push_id(("layer_show", id), |ui| {
                                    ui.add_sized(
                                        [18.0, 18.0],
                                        egui::Checkbox::without_text(&mut shown),
                                    )
                                })
                                .inner;
                            show_response.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::Checkbox,
                                    true,
                                    shown,
                                    "Show",
                                )
                            });
                            if show_response
                                .on_hover_text("Show this layer on the canvas")
                                .changed()
                            {
                                if let Some(layer) = self.editor.document.layer_mut(id) {
                                    layer.visible = shown;
                                }
                            }
                            let mut air = operation.is_some_and(|entry| entry.5);
                            let air_response = ui
                                .push_id(("layer_air", id), |ui| {
                                    ui.add_enabled_ui(operation.is_some(), |ui| {
                                        ui.add_sized(
                                            [18.0, 18.0],
                                            egui::Checkbox::without_text(&mut air),
                                        )
                                    })
                                    .inner
                                })
                                .inner;
                            air_response.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::Checkbox,
                                    true,
                                    air,
                                    "Air",
                                )
                            });
                            if air_response.changed() {
                                if let Some(entry) = operation {
                                    toggle_operation = Some((entry.0.clone(), entry.4, air));
                                }
                            }
                            air_response.on_hover_text("Air assist (M8/M9)");
                            ui.allocate_ui_with_layout(
                                egui::vec2(34.0, 18.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.spacing_mut().item_spacing.x = 1.0;
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new("\u{25b2}").monospace(),
                                            )
                                            .small()
                                            .min_size(egui::vec2(14.0, 16.0)),
                                        )
                                        .on_hover_text("Cut this layer earlier")
                                        .clicked()
                                    {
                                        self.editor.move_layer(id, -1);
                                    }
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new("\u{25bc}").monospace(),
                                            )
                                            .small()
                                            .min_size(egui::vec2(14.0, 16.0)),
                                        )
                                        .on_hover_text("Cut this layer later")
                                        .clicked()
                                    {
                                        self.editor.move_layer(id, 1);
                                    }
                                },
                            );
                            ui.end_row();
                        }
                    });
                if let Some((id, kind)) = mode_choice {
                    self.set_layer_mode(&id, kind);
                }
                if let Some((operation, enabled, air)) = toggle_operation {
                    if let Some(entry) = self
                        .editor
                        .document
                        .operations
                        .iter_mut()
                        .find(|entry| entry.id == operation)
                    {
                        entry.enabled = enabled;
                        entry.params.air_assist = air;
                    }
                }
                if let Some(message) = message {
                    ui.colored_label(Color32::from_rgb(200, 60, 60), message);
                }
                if self.row_settings.is_none() && self.editor.document != before {
                    self.editor.history.record(&before, "Edit layer");
                }
                ui.separator();
                ui.label(format!(
                    "{} objects on {} layers",
                    self.editor.document.objects.len(),
                    self.editor.document.layers.len()
                ));
                ui.separator();
                ui.heading("Job");
                ui.horizontal(|ui| {
                    if ui.button("Estimate").clicked() {
                        match crate::gcode::render_job(
                            &self.editor.document,
                            &crate::gcode::JobOptions::default(),
                        ) {
                            Ok((_, report)) => self.job = Some(report),
                            Err(error) => {
                                self.job = None;
                                self.status = format!("Job error: {error}");
                            }
                        }
                    }
                    if ui.button("Export G-code...").clicked() {
                        self.open_dialog(FileMode::ExportGcode);
                    }
                });
                if let Some(report) = &self.job {
                    ui.label(format!("Paths: {}", report.paths));
                    ui.label(format!("Lines: {} ({} bytes)", report.lines, report.bytes));
                    ui.label(format!("Estimated: {:.1} s", report.seconds));
                    for warning in &report.warnings {
                        ui.colored_label(Color32::from_rgb(180, 120, 0), warning);
                    }
                }
            });
    }

    /// Delete a layer and, when nothing else uses its cut entry, the entry too.
    pub(super) fn delete_layer(&mut self, layer_id: &str) -> Result<(), String> {
        let operation = self
            .editor
            .document
            .layer(layer_id)
            .and_then(|layer| layer.operation_id.clone());
        self.editor.document.remove_layer(layer_id)?;
        if let Some(operation) = operation {
            let in_use = self
                .editor
                .document
                .layers
                .iter()
                .filter_map(|layer| layer.operation_id.as_deref())
                .chain(
                    self.editor
                        .document
                        .objects
                        .iter()
                        .filter_map(|object| object.operation_id.as_deref()),
                )
                .any(|id| id == operation.as_str());
            if !in_use {
                self.editor
                    .document
                    .operations
                    .retain(|entry| entry.id != operation);
            }
        }
        Ok(())
    }

    /// Put a layer on a cut mode, creating the cut entry for its colour when
    /// there is none and assigning it to every object of the layer.
    pub(super) fn set_layer_mode(
        &mut self,
        layer_id: &str,
        kind: crate::operations::OperationType,
    ) {
        let before = self.editor.document.clone();
        let current = self.layer_mode(layer_id).0;
        let operation = current.unwrap_or_else(|| {
            let colour = self.layer_display_colour(layer_id);
            self.editor.ensure_operation_for_colour(&colour)
        });
        if let Some(entry) = self
            .editor
            .document
            .operations
            .iter_mut()
            .find(|entry| entry.id == operation)
        {
            entry.op_type = kind;
        }
        if let Some(layer) = self.editor.document.layer_mut(layer_id) {
            layer.operation_id = Some(operation.clone());
        }
        let ids: Vec<String> = self
            .editor
            .document
            .objects_on_layer(layer_id)
            .iter()
            .map(|object| object.id.clone())
            .collect();
        for id in ids {
            if let Some(object) = self.editor.document.object_mut(&id) {
                object.operation_id = Some(operation.clone());
            }
        }
        if self.editor.document != before {
            self.editor.history.record(&before, "Cut mode");
        }
        let name = self
            .editor
            .document
            .layer(layer_id)
            .map_or_else(String::new, |layer| layer.name.clone());
        self.status = format!("{} set to {}", name, kind.label());
    }

    /// Resolve the layer, drawing colour and cut entry of the first selected
    /// object. The entry comes from the object, then the layer, then the colour.
    pub(super) fn selection_cut_context(&self) -> Option<(String, String, Option<String>)> {
        let object_id = self.editor.selection.ids().first()?;
        let object = self.editor.document.object(object_id)?;
        let layer = self.editor.document.layer(&object.layer_id)?;
        let colour = object.color.clone().unwrap_or_else(|| layer.color.clone());
        let operation = object
            .operation_id
            .clone()
            .or_else(|| layer.operation_id.clone())
            .or_else(|| self.editor.operation_for_colour(&colour));
        Some((layer.id.clone(), colour, operation))
    }

    /// Open the full cut window on the cut entry of the selected object's
    /// layer, creating one for its colour when the layer has none.
    pub(super) fn open_cut_settings_for_selection(&mut self) {
        let Some((layer_id, _, _)) = self.selection_cut_context() else {
            return;
        };
        self.open_cut_settings_for_layer(&layer_id);
    }

    /// Open the focused row settings dialog for a layer.
    pub(super) fn open_row_settings(&mut self, layer_id: &str) {
        if self.row_settings.is_some() || self.editor.document.layer(layer_id).is_none() {
            return;
        }
        self.row_settings = Some(layer_id.to_string());
        self.row_settings_snapshot = Some(self.editor.document.clone());
        self.row_settings_done = false;
        self.editor.active_layer_id = layer_id.to_string();
    }

    fn close_row_settings(&mut self) {
        self.row_settings = None;
        self.row_settings_snapshot = None;
        self.row_settings_done = false;
    }

    /// The row settings modal: it blocks the rest of the UI and stays open
    /// until OK or Cancel. Cancel restores the document as it was when opened.
    pub(super) fn row_settings_modal(&mut self, ctx: &egui::Context) {
        let Some(id) = self.row_settings.clone() else {
            return;
        };
        if self.editor.document.layer(&id).is_none() {
            self.close_row_settings();
            return;
        }
        let enter =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        let mut commit = enter;
        let mut cancel = false;
        egui::containers::Modal::new(egui::Id::new("row_settings_modal")).show(ctx, |ui| {
            ui.set_min_width(430.0);
            self.row_settings_ui(ui, &id);
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    commit = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
            if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                cancel = true;
            }
            if self.row_settings_done {
                commit = false;
                cancel = false;
            }
        });
        if self.row_settings_done {
            self.close_row_settings();
            return;
        }
        if cancel {
            if let Some(snapshot) = self.row_settings_snapshot.take() {
                self.editor.document = snapshot;
            }
            self.close_row_settings();
            return;
        }
        if commit {
            if let Some(snapshot) = self.row_settings_snapshot.take() {
                if self.editor.document != snapshot {
                    self.editor.history.record(&snapshot, "Edit layer");
                }
            }
            self.close_row_settings();
        }
    }

    /// Open the full cut window on a layer, creating and binding its cut entry
    /// when the layer has none.
    pub(super) fn open_cut_settings_for_layer(&mut self, layer_id: &str) {
        if self.editor.document.layer(layer_id).is_none() {
            return;
        }
        let colour = self.layer_display_colour(layer_id);
        let stored = self
            .editor
            .document
            .layer(layer_id)
            .and_then(|layer| layer.operation_id.clone());
        let before = self.editor.document.clone();
        let operation = stored
            .or_else(|| self.editor.operation_for_colour(&colour))
            .unwrap_or_else(|| self.editor.ensure_operation_for_colour(&colour));
        if let Some(layer) = self.editor.document.layer_mut(layer_id) {
            layer.operation_id.get_or_insert_with(|| operation.clone());
        }
        let ids: Vec<String> = self
            .editor
            .document
            .objects_on_layer(layer_id)
            .iter()
            .map(|object| object.id.clone())
            .collect();
        for id in ids {
            if let Some(object) = self.editor.document.object_mut(&id) {
                if object.operation_id.is_none() {
                    object.operation_id = Some(operation.clone());
                }
            }
        }
        if self.editor.document != before {
            self.editor.history.record(&before, "Cut entry");
        }
        self.active_operation = Some(operation);
        self.open_cuts();
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn row_settings_ui(&mut self, ui: &mut Ui, id: &str) {
        use crate::operations::OperationType;

        let name = self
            .editor
            .document
            .layer(id)
            .map_or_else(String::new, |layer| layer.name.clone());
        ui.label(egui::RichText::new(&name).strong());
        egui::Grid::new(("row_layer", id))
            .num_columns(2)
            .spacing([8.0, 3.0])
            .show(ui, |ui| {
                let name_label = ui.label("Name");
                if let Some(layer) = self.editor.document.layer_mut(id) {
                    ui.text_edit_singleline(&mut layer.name)
                        .labelled_by(name_label.id);
                }
                ui.end_row();
                ui.label("Colour");
                let colour = self.layer_display_colour(id);
                let rgb = parse_hex_color(&colour).unwrap_or((0, 0, 0));
                let mut srgb = [rgb.0, rgb.1, rgb.2];
                if ui.color_edit_button_srgb(&mut srgb).changed() {
                    self.set_layer_colour(id, &to_hex(srgb[0], srgb[1], srgb[2]));
                }
                ui.end_row();
            });
        ui.separator();
        let cut = self.layer_mode(id).0;
        if let Some(cut_id) = cut.clone() {
            ui.label(egui::RichText::new("Cut settings").strong());
            let shared: Vec<String> = self
                .editor
                .document
                .layers
                .iter()
                .filter(|layer| {
                    layer.id != *id
                        && self.layer_mode(&layer.id).0.as_deref() == Some(cut_id.as_str())
                })
                .map(|layer| layer.name.clone())
                .collect();
            if !shared.is_empty() {
                ui.weak(format!("Shared with {}", shared.join(", ")))
                    .on_hover_text("Changing this entry also changes those layers");
            }
            let mut open_all = false;
            egui::Grid::new(("row_cut", id))
                .num_columns(2)
                .spacing([8.0, 3.0])
                .show(ui, |ui| {
                    let Some(operation) = self
                        .editor
                        .document
                        .operations
                        .iter_mut()
                        .find(|operation| operation.id == cut_id)
                    else {
                        ui.weak("The cut entry is missing");
                        return;
                    };
                    let cut_label = ui.label("Cut");
                    ui.text_edit_singleline(&mut operation.name)
                        .labelled_by(cut_label.id);
                    ui.end_row();
                    let mode_label = ui.label("Mode");
                    let mut kind = operation.op_type;
                    egui::ComboBox::from_id_salt(("row_cut_mode", id))
                        .width(130.0)
                        .selected_text(kind.label())
                        .show_ui(ui, |ui| {
                            for candidate in OperationType::all() {
                                if ui
                                    .selectable_value(&mut kind, candidate, candidate.label())
                                    .clicked()
                                {
                                    ui.close();
                                }
                            }
                        })
                        .response
                        .labelled_by(mode_label.id);
                    operation.op_type = kind;
                    ui.end_row();
                    let speed_label = ui.label("Speed (mm/min)");
                    ui.add(
                        egui::DragValue::new(&mut operation.params.speed)
                            .speed(10.0)
                            .range(1.0..=100_000.0),
                    )
                    .labelled_by(speed_label.id);
                    ui.end_row();
                    let power_label = ui.label("Power (%)");
                    ui.add(
                        egui::DragValue::new(&mut operation.params.power)
                            .speed(1.0)
                            .range(0.0..=100.0),
                    )
                    .labelled_by(power_label.id);
                    ui.end_row();
                    let passes_label = ui.label("Passes");
                    ui.add(
                        egui::DragValue::new(&mut operation.params.passes)
                            .speed(0.2)
                            .range(1..=1000),
                    )
                    .labelled_by(passes_label.id);
                    ui.end_row();
                    let dpi_label = ui.label("DPI");
                    let mut dpi = (25.4 / operation.params.line_interval_mm.max(0.001)).round();
                    let response = ui
                        .add(
                            egui::DragValue::new(&mut dpi)
                                .speed(1.0)
                                .range(10.0..=2540.0)
                                .max_decimals(0),
                        )
                        .labelled_by(dpi_label.id)
                        .on_hover_text("Lines per inch; stored as the line interval in mm");
                    if response.changed() {
                        operation.params.line_interval_mm = 25.4 / dpi.max(1.0);
                    }
                    ui.end_row();
                    ui.label("Air assist");
                    ui.checkbox(&mut operation.params.air_assist, "")
                        .widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::Checkbox,
                                true,
                                operation.params.air_assist,
                                "Air assist",
                            )
                        });
                    ui.end_row();
                    ui.label("Output");
                    ui.checkbox(&mut operation.enabled, "").widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::Checkbox,
                            true,
                            operation.enabled,
                            "Output",
                        )
                    });
                    ui.end_row();
                    if ui
                        .button("All settings...")
                        .on_hover_text("Passes, leads, kerf, tabs, fill and output")
                        .clicked()
                    {
                        open_all = true;
                    }
                });
            if open_all {
                self.active_operation = Some(cut_id.clone());
                self.cuts_open = true;
                self.row_settings_done = true;
            }
            if ui
                .button("Delete cut entry")
                .on_hover_text("Remove the cut entry; objects keep their colour")
                .clicked()
            {
                self.editor.remove_operation(&cut_id);
                self.row_settings_done = true;
            }
        } else {
            ui.weak("This layer has no cut entry.");
            if ui.button("Add cut entry").clicked() {
                let before = self.editor.document.clone();
                let colour = self.layer_display_colour(id);
                let operation = self.editor.ensure_operation_for_colour(&colour);
                if let Some(layer) = self.editor.document.layer_mut(id) {
                    layer.operation_id = Some(operation.clone());
                }
                let ids: Vec<String> = self
                    .editor
                    .document
                    .objects_on_layer(id)
                    .iter()
                    .map(|object| object.id.clone())
                    .collect();
                for object_id in ids {
                    if let Some(object) = self.editor.document.object_mut(&object_id) {
                        object.operation_id = Some(operation.clone());
                    }
                }
                if self.editor.document != before {
                    self.editor.history.record(&before, "Cut entry");
                }
            }
        }
        ui.separator();
        if ui.button("Duplicate layer").clicked() {
            if let Some(copy) = self.editor.duplicate_layer(id) {
                self.editor.active_layer_id = copy;
            }
            self.row_settings_done = true;
        }
        if ui.button("Delete layer").clicked() {
            let before = self.editor.document.clone();
            if let Err(error) = self.delete_layer(id) {
                self.status = error;
            }
            if self.editor.document != before {
                self.editor.history.record(&before, "Delete layer");
            }
            self.row_settings_done = true;
        }
        if ui.button("Select objects").clicked() {
            let count = self.editor.select_layer_objects(id);
            self.status = format!("Selected {count} object(s)");
            self.row_settings_done = true;
        }
        if ui.button("Select this colour").clicked() {
            let colour = self.layer_display_colour(id);
            let count = self.editor.select_by_color(&colour);
            self.status = format!("Selected {count} object(s) of {colour}");
            self.row_settings_done = true;
        }
        if ui.button("Group objects").clicked() {
            let count = self.editor.group_layer_objects(id);
            self.status = format!("Grouped {count} object(s)");
            self.row_settings_done = true;
        }
        if ui.button("Group by colour").clicked() {
            let (groups, objects) = self.editor.group_by_colour();
            self.status = if groups == 0 {
                "No colour has two or more objects".to_string()
            } else {
                format!("Grouped {objects} object(s) into {groups} colour group(s)")
            };
            self.row_settings_done = true;
        }
        let (visible, locked) = self
            .editor
            .document
            .layer(id)
            .map_or((true, false), |layer| (layer.visible, layer.locked));
        if ui
            .button(if visible { "Hide layer" } else { "Show layer" })
            .clicked()
        {
            let before = self.editor.document.clone();
            if let Some(layer) = self.editor.document.layer_mut(id) {
                layer.visible = !visible;
            }
            self.editor
                .history
                .record(&before, if visible { "Hide layer" } else { "Show layer" });
            self.row_settings_done = true;
        }
        if ui
            .button(if locked { "Unlock layer" } else { "Lock layer" })
            .clicked()
        {
            let before = self.editor.document.clone();
            if let Some(layer) = self.editor.document.layer_mut(id) {
                layer.locked = !locked;
            }
            self.editor
                .history
                .record(&before, if locked { "Unlock layer" } else { "Lock layer" });
            self.row_settings_done = true;
        }
        if name.is_empty() {
            ui.weak("Missing layer");
        }
    }
    /// Compact colour strip: applies the colour to the selection, or makes it
    /// the colour of the active layer so the next shapes use it.
    ///
    /// The palette lives at the top of the Cuts / Layers panel in two rows, so
    /// it stays next to the cut entries instead of taking a strip from the
    /// canvas.
    pub(super) fn color_palette(&mut self, ui: &mut Ui) {
        let previous_spacing = ui.spacing().item_spacing.x;
        ui.spacing_mut().item_spacing.x = 2.0;
        let active = self
            .editor
            .document
            .layer(&self.editor.active_layer_id)
            .map(|layer| layer.color.to_ascii_lowercase());
        egui::Grid::new("drawing_colour_grid")
            .num_columns(2)
            .spacing([6.0, 2.0])
            .show(ui, |ui| {
                for (row, colours) in DRAWING_COLOURS.chunks(10).enumerate() {
                    if row == 0 {
                        ui.label("Colour");
                    } else {
                        ui.label("");
                    }
                    ui.horizontal(|ui| {
                        for colour in colours {
                            self.drawing_colour_button(ui, colour, active.as_deref());
                        }
                    });
                    ui.end_row();
                }
            });
        if let Some(active) = &active
            && !DRAWING_COLOURS
                .iter()
                .any(|colour| colour.eq_ignore_ascii_case(active))
        {
            ui.horizontal(|ui| {
                let (r, g, b) = parse_hex_color(active).unwrap_or((0, 0, 0));
                let response = ui.add(
                    egui::Button::new("")
                        .fill(Color32::from_rgb(r, g, b))
                        .min_size(egui::vec2(18.0, 18.0))
                        .stroke(egui::Stroke::new(1.0, Color32::WHITE)),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        true,
                        format!("Custom colour {active}"),
                    )
                });
                ui.weak(active.clone());
                response.on_hover_text("Custom colour outside the palette");
            });
        }
        ui.spacing_mut().item_spacing.x = previous_spacing;
    }

    fn drawing_colour_button(&mut self, ui: &mut Ui, colour: &str, active: Option<&str>) {
        let (r, g, b) = parse_hex_color(colour).unwrap_or((0, 0, 0));
        let selected = active == Some(colour);
        let cut = self.editor.operation_for_colour(colour).and_then(|id| {
            self.editor
                .document
                .operations
                .iter()
                .find(|operation| operation.id == id)
                .map(|operation| {
                    format!(
                        "{} · {:.0} mm/min · {:.0}%",
                        operation.name, operation.params.speed, operation.params.power
                    )
                })
        });
        let response = ui.add(
            egui::Button::new("")
                .fill(Color32::from_rgb(r, g, b))
                .min_size(egui::vec2(18.0, 18.0))
                .stroke(if selected {
                    egui::Stroke::new(2.0, Color32::WHITE)
                } else {
                    egui::Stroke::NONE
                }),
        );
        if cut.is_some() {
            let luminance =
                (0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)) / 255.0;
            let ink = if luminance > 0.6 {
                Color32::BLACK
            } else {
                Color32::WHITE
            };
            ui.painter().circle_filled(
                response.rect.right_bottom() + egui::vec2(-3.5, -3.5),
                2.0,
                ink,
            );
        }
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                true,
                format!("Drawing colour {colour}"),
            )
        });
        let tip = cut.as_ref().map_or_else(
            || format!("Use {colour} — no cut entry"),
            |cut| format!("Use {colour}\nCut {cut}"),
        );
        if response.on_hover_text(tip).clicked() {
            self.apply_drawing_colour(colour);
        }
    }

    /// Four small tools that use selected vector shapes as an image mask.
    fn image_mask_tools(&mut self, ui: &mut Ui) {
        let (image, vectors, has_mask) = self.image_mask_selection();
        ui.horizontal_wrapped(|ui| {
            ui.label("Image mask");
            if ui
                .add_enabled(image && vectors, egui::Button::new("Apply mask").small())
                .on_hover_text("Use the selected closed shapes as the image mask")
                .clicked()
            {
                self.apply_image_vector_mask(false);
            }
            if ui
                .add_enabled(image && vectors, egui::Button::new("Mask + keep").small())
                .on_hover_text("Mask the image and keep the shapes for cutting")
                .clicked()
            {
                self.apply_image_vector_mask(true);
            }
            if ui
                .add_enabled(image && has_mask, egui::Button::new("Invert").small())
                .on_hover_text("Keep the pixels outside the mask instead of inside")
                .clicked()
            {
                self.set_image_mask_inverted(!self.image_mask_is_inverted());
            }
            if ui
                .add_enabled(image && has_mask, egui::Button::new("Clear").small())
                .on_hover_text("Remove the vector mask from the image")
                .clicked()
            {
                self.clear_image_mask();
            }
        });
    }

    fn image_mask_selection(&self) -> (bool, bool, bool) {
        let mut images = 0usize;
        let mut vectors = false;
        let mut has_mask = false;
        for id in self.editor.selection.ids() {
            let Some(object) = self.editor.document.object(id) else {
                continue;
            };
            match &object.shape {
                crate::document::Shape::Image(image) => {
                    images += 1;
                    has_mask = !image.mask_paths.is_empty();
                }
                crate::document::Shape::Text(_) => {}
                _ => {
                    if object.world_path().is_closed() {
                        vectors = true;
                    }
                }
            }
        }
        (images == 1, vectors, has_mask)
    }

    fn selected_image_id(&self) -> Option<String> {
        self.editor.selection.ids().iter().find_map(|id| {
            self.editor
                .document
                .object(id)
                .filter(|object| matches!(&object.shape, crate::document::Shape::Image(_)))
                .map(|_| id.clone())
        })
    }

    fn image_mask_is_inverted(&self) -> bool {
        self.selected_image_id().is_some_and(|id| {
            self.editor
                .document
                .object(&id)
                .and_then(|object| match &object.shape {
                    crate::document::Shape::Image(image) => Some(image.mask_invert),
                    _ => None,
                })
                .unwrap_or(false)
        })
    }

    pub(super) fn apply_image_vector_mask(&mut self, keep_vectors: bool) {
        let ids = self.editor.selection.ids().to_vec();
        let mut image_id: Option<String> = None;
        let mut vector_ids = Vec::new();
        for id in &ids {
            let Some(object) = self.editor.document.object(id) else {
                continue;
            };
            match &object.shape {
                crate::document::Shape::Image(_) => {
                    if image_id.is_some() {
                        self.status = "Select exactly one image and the mask shapes".to_string();
                        return;
                    }
                    image_id = Some(id.clone());
                }
                crate::document::Shape::Text(_) => {}
                _ => {
                    if object.world_path().is_closed() {
                        vector_ids.push(id.clone());
                    }
                }
            }
        }
        let Some(image_id) = image_id else {
            self.status = "Select an image and at least one closed shape".to_string();
            return;
        };
        if vector_ids.is_empty() {
            self.status = "Select at least one closed shape to use as the mask".to_string();
            return;
        }
        let Some(inverse) = self
            .editor
            .document
            .object(&image_id)
            .and_then(|object| object.transform.invert())
        else {
            self.status = "The image transform cannot be inverted".to_string();
            return;
        };
        let paths: Vec<_> = vector_ids
            .iter()
            .filter_map(|id| self.editor.document.object(id))
            .map(|object| object.world_path().transform(&inverse))
            .collect();
        if paths.is_empty() {
            self.status = "No closed shape could be used as the mask".to_string();
            return;
        }
        self.editor.record_action("Mask image");
        if let Some(object) = self.editor.document.object_mut(&image_id)
            && let crate::document::Shape::Image(image) = &mut object.shape
        {
            image.mask_paths = paths;
            image.mask_invert = false;
        }
        if !keep_vectors {
            for id in &vector_ids {
                self.editor.document.remove_object(id);
            }
            self.editor.selection.clear();
            self.editor.selection.add(image_id);
        }
        self.status = if keep_vectors {
            "Masked the image and kept the shapes".to_string()
        } else {
            "Masked the image and baked the shapes in".to_string()
        };
    }

    pub(super) fn set_image_mask_inverted(&mut self, invert: bool) {
        let Some(id) = self.selected_image_id() else {
            return;
        };
        self.editor.record_action("Invert image mask");
        if let Some(object) = self.editor.document.object_mut(&id)
            && let crate::document::Shape::Image(image) = &mut object.shape
        {
            image.mask_invert = invert;
        }
        self.status = if invert {
            "Image mask inverted".to_string()
        } else {
            "Image mask restored".to_string()
        };
    }

    pub(super) fn clear_image_mask(&mut self) {
        let Some(id) = self.selected_image_id() else {
            return;
        };
        self.editor.record_action("Clear image mask");
        if let Some(object) = self.editor.document.object_mut(&id)
            && let crate::document::Shape::Image(image) = &mut object.shape
        {
            image.mask_paths.clear();
            image.mask_invert = false;
        }
        self.status = "Image mask cleared".to_string();
    }

    fn apply_drawing_colour(&mut self, colour: &str) {
        let before = self.editor.document.clone();
        let selected = self.editor.selection.ids().to_vec();
        let created = self.editor.operation_for_colour(colour).is_none();
        let operation = self.editor.ensure_operation_for_colour(colour);
        if selected.is_empty() {
            let active = self.editor.active_layer_id.clone();
            if let Some(layer) = self.editor.document.layer_mut(&active) {
                layer.color = colour.to_string();
                layer.operation_id = Some(operation.clone());
            }
            let ids: Vec<String> = self
                .editor
                .document
                .objects_on_layer(&active)
                .iter()
                .map(|object| object.id.clone())
                .collect();
            for id in ids {
                if let Some(object) = self.editor.document.object_mut(&id) {
                    object.operation_id = Some(operation.clone());
                }
            }
            self.status = format!("New objects will use {colour}{}", cut_entry_note(created));
        } else {
            for id in &selected {
                if let Some(object) = self.editor.document.object_mut(id) {
                    object.color = Some(colour.to_string());
                    object.operation_id = Some(operation.clone());
                }
            }
            let mut layers: Vec<String> = selected
                .iter()
                .filter_map(|id| {
                    self.editor
                        .document
                        .object(id)
                        .map(|object| object.layer_id.clone())
                })
                .collect();
            layers.sort_unstable();
            layers.dedup();
            for layer_id in layers {
                let Some(layer) = self.editor.document.layer(&layer_id) else {
                    continue;
                };
                let colours: Vec<String> = self
                    .editor
                    .document
                    .objects_on_layer(&layer_id)
                    .iter()
                    .map(|object| object.color.clone().unwrap_or_else(|| layer.color.clone()))
                    .collect();
                if colours.iter().all(|value| value == colour) {
                    if let Some(layer) = self.editor.document.layer_mut(&layer_id) {
                        layer.color = colour.to_string();
                        layer.operation_id = Some(operation.clone());
                    }
                }
            }
            self.status = format!(
                "Colour {colour} applied to {} object(s){}",
                selected.len(),
                cut_entry_note(created)
            );
        }
        if self.editor.document != before {
            self.editor.history.record(&before, "Colour");
        }
    }

    fn set_layer_colour(&mut self, layer_id: &str, colour: &str) {
        let before = self.editor.document.clone();
        self.set_layer_colour_inner(layer_id, colour);
        if self.row_settings.is_none() && self.editor.document != before {
            self.editor.history.record(&before, "Colour");
        }
    }

    fn set_layer_colour_inner(&mut self, layer_id: &str, colour: &str) {
        let ids: Vec<String> = self
            .editor
            .document
            .objects_on_layer(layer_id)
            .iter()
            .map(|object| object.id.clone())
            .collect();
        if let Some(layer) = self.editor.document.layer_mut(layer_id) {
            layer.color = colour.to_string();
        }
        let operation = self.editor.ensure_operation_for_colour(colour);
        if let Some(layer) = self.editor.document.layer_mut(layer_id) {
            layer.operation_id = Some(operation.clone());
        }
        for id in ids {
            if let Some(object) = self.editor.document.object_mut(&id) {
                object.color = Some(colour.to_string());
                object.operation_id = Some(operation.clone());
            }
        }
    }

    fn layer_display_colour(&self, layer_id: &str) -> String {
        let Some(layer) = self.editor.document.layer(layer_id) else {
            return "#000000".to_string();
        };
        let objects = self.editor.document.objects_on_layer(layer_id);
        let mut colours: Vec<String> = objects
            .iter()
            .map(|object| object.color.clone().unwrap_or_else(|| layer.color.clone()))
            .collect();
        colours.sort_unstable();
        colours.dedup();
        match colours.as_slice() {
            [colour] => colour.clone(),
            _ => layer.color.clone(),
        }
    }
}

fn cut_entry_note(created: bool) -> &'static str {
    if created { " (new cut entry)" } else { "" }
}
