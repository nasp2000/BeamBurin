use super::{BeamBurinApp, Color32, FileMode, LayerRow, Ui, parse_hex_color, to_hex};

/// Drawing colours offered at the bottom of the window. They are also the
/// colours a layer can use for its cut configuration.
const DRAWING_COLOURS: [&str; 20] = [
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
            .default_size(480.0)
            .min_size(380.0)
            .resizable(true)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
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
                                DRAWING_COLOURS[self.editor.document.layers.len()
                                    % DRAWING_COLOURS.len()]
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
                        match self.editor.document.remove_layer(&id) {
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
                egui::ScrollArea::horizontal()
                    .id_salt("layers_table_scroll")
                    .show(ui, |ui| {
                        egui::Grid::new("layers_grid")
                            .num_columns(8)
                            .striped(true)
                            .spacing([3.0, 2.0])
                            .show(ui, |ui| {
                                ui.add_sized(
                                    [20.0, 18.0],
                                    egui::Label::new(egui::RichText::new("#").strong()),
                                );
                                ui.add_sized(
                                    [96.0, 18.0],
                                    egui::Label::new(egui::RichText::new("Layer").strong()),
                                );
                                for header in ["Mode", "Spd/Pwr", "Out", "Show", "Air"] {
                                    let width = if header == "Mode" {
                                        86.0
                                    } else if header == "Spd/Pwr" {
                                        40.0
                                    } else {
                                        21.0
                                    };
                                    ui.add_sized(
                                        [width, 18.0],
                                        egui::Label::new(egui::RichText::new(header).strong()),
                                    );
                                }
                                ui.add_sized(
                                    [36.0, 18.0],
                                    egui::Label::new(egui::RichText::new("Order").strong()),
                                );
                                ui.end_row();
                                for (index, row) in rows.iter().enumerate() {
                                    let (id, name, color, visible, _locked, mode, mixed, count) =
                                        row;
                                    let selected = self.editor.active_layer_id == *id;
                                    let operation = mode.as_deref().and_then(|mode| {
                                        operations.iter().find(|entry| entry.0 == mode)
                                    });
                                    let number = ui.add_sized(
                                        [20.0, 18.0],
                                        egui::Label::new(format!("L{:02}", index + 1))
                                            .sense(egui::Sense::click()),
                                    );
                                    number.context_menu(|ui| {
                                        self.layer_context_menu(ui, id);
                                    });
                                    let mut name_hit: Option<egui::Response> = None;
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(96.0, 18.0),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            let rgb = parse_hex_color(color).unwrap_or((0, 0, 0));
                                            let mut srgb = [rgb.0, rgb.1, rgb.2];
                                            let swatch = ui.color_edit_button_srgb(&mut srgb);
                                            swatch.context_menu(|ui| {
                                                self.layer_context_menu(ui, id);
                                            });
                                            if swatch.changed() {
                                                let colour = to_hex(srgb[0], srgb[1], srgb[2]);
                                                self.set_layer_colour(id, &colour);
                                            }
                                            let name_button = ui
                                                .add_sized(
                                                    [70.0, 18.0],
                                                    egui::Button::selectable(
                                                        selected,
                                                        name.as_str(),
                                                    )
                                                    .frame(false),
                                                )
                                                .on_hover_text(format!(
                                                    "{count} object(s) — right-click for cut settings"
                                                ));
                                            if name_button.clicked() {
                                                self.editor.active_layer_id.clone_from(id);
                                            }
                                            name_hit = Some(name_button);
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
                                        current_type
                                            .map_or_else(|| "—".to_string(), |kind| {
                                                kind.label().to_string()
                                            })
                                    };
                                    let mode_combo =
                                        egui::ComboBox::from_id_salt(format!("layer_mode_{id}"))
                                            .width(80.0)
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
                                        [40.0, 18.0],
                                        egui::Label::new(speeds).sense(egui::Sense::click()),
                                    );
                                    speeds_response.context_menu(|ui| {
                                        self.layer_context_menu(ui, id);
                                    });
                                    let mut output = operation.is_some_and(|entry| entry.4);
                                    let output_response = ui
                                        .push_id(("layer_output", id), |ui| {
                                            ui.add_enabled_ui(operation.is_some(), |ui| {
                                                ui.add_sized(
                                                    [22.0, 18.0],
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
                                            toggle_operation =
                                                Some((entry.0.clone(), output, entry.5));
                                        }
                                    }
                                    output_response.on_hover_text("Include this mode in the job");
                                    let mut shown = *visible;
                                    let show_response = ui
                                        .push_id(("layer_show", id), |ui| {
                                            ui.add_sized(
                                                [22.0, 18.0],
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
                                                    [22.0, 18.0],
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
                                            toggle_operation =
                                                Some((entry.0.clone(), entry.4, air));
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
                                    if let Some(name_button) = name_hit {
                                        name_button.context_menu(|ui| {
                                            self.layer_context_menu(ui, id);
                                        });
                                    }
                                }
                            });
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
                if self.editor.document != before {
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
        let Some((layer_id, colour, operation)) = self.selection_cut_context() else {
            return;
        };
        let before = self.editor.document.clone();
        let operation =
            operation.unwrap_or_else(|| self.editor.ensure_operation_for_colour(&colour));
        if let Some(layer) = self.editor.document.layer_mut(&layer_id) {
            layer.operation_id.get_or_insert_with(|| operation.clone());
        }
        for id in self.editor.selection.ids().to_vec() {
            if let Some(object) = self.editor.document.object_mut(&id) {
                if object.layer_id == layer_id && object.operation_id.is_none() {
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

    /// Compact speed and power fields for the selected object's cut entry,
    /// shown inside the object context menu.
    pub(super) fn object_cut_quick_ui(&mut self, ui: &mut Ui) {
        let Some((_, _, Some(operation_id))) = self.selection_cut_context() else {
            ui.weak("No cut entry for this layer");
            return;
        };
        let Some(entry) = self
            .editor
            .document
            .operations
            .iter()
            .find(|entry| entry.id == operation_id)
        else {
            ui.weak("No cut entry for this layer");
            return;
        };
        use crate::operations::OperationType;

        let name = entry.name.clone();
        let current_mode = entry.op_type;
        let mut mode = current_mode;
        let mut speed = entry.params.speed;
        let mut power = entry.params.power;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("Cut: {name}")).strong());
            egui::ComboBox::from_id_salt(("object_cut_mode", operation_id.as_str()))
                .width(120.0)
                .selected_text(mode.label())
                .show_ui(ui, |ui| {
                    for candidate in OperationType::all() {
                        if ui
                            .selectable_value(&mut mode, candidate, candidate.label())
                            .clicked()
                        {
                            ui.close();
                        }
                    }
                });
        });
        if mode != current_mode {
            self.set_cut_mode(&operation_id, mode);
        }
        let mut changed = false;
        egui::Grid::new(("object_cut_quick", operation_id.as_str()))
            .num_columns(2)
            .spacing([8.0, 3.0])
            .show(ui, |ui| {
                ui.label("Speed (mm/min)");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut speed)
                            .speed(10.0)
                            .range(1.0..=100_000.0),
                    )
                    .changed();
                ui.end_row();
                ui.label("Power (%)");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut power)
                            .speed(1.0)
                            .range(0.0..=100.0),
                    )
                    .changed();
                ui.end_row();
            });
        if changed {
            self.set_cut_speed_power(&operation_id, speed, power);
        }
    }

    pub(super) fn set_cut_mode(
        &mut self,
        operation_id: &str,
        kind: crate::operations::OperationType,
    ) {
        let before = self.editor.document.clone();
        if let Some(entry) = self
            .editor
            .document
            .operations
            .iter_mut()
            .find(|entry| entry.id == operation_id)
        {
            entry.op_type = kind;
        }
        if self.editor.document != before {
            self.editor.history.record(&before, "Cut mode");
        }
    }

    pub(super) fn set_cut_speed_power(&mut self, operation_id: &str, speed: f64, power: f64) {
        let before = self.editor.document.clone();
        if let Some(entry) = self
            .editor
            .document
            .operations
            .iter_mut()
            .find(|entry| entry.id == operation_id)
        {
            entry.params.speed = speed;
            entry.params.power = power;
        }
        if self.editor.document != before {
            self.editor.history.record(&before, "Cut speed and power");
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn layer_context_menu(&mut self, ui: &mut Ui, id: &str) {
        use crate::operations::OperationType;

        let name = self
            .editor
            .document
            .layer(id)
            .map_or_else(String::new, |layer| layer.name.clone());
        ui.label(egui::RichText::new(format!("Layer {name}")).strong());
        egui::Grid::new(("row_layer", id))
            .num_columns(2)
            .spacing([8.0, 3.0])
            .show(ui, |ui| {
                ui.label("Name");
                if let Some(layer) = self.editor.document.layer_mut(id) {
                    ui.text_edit_singleline(&mut layer.name);
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
                    ui.label("Cut");
                    ui.text_edit_singleline(&mut operation.name);
                    ui.end_row();
                    ui.label("Mode");
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
                        });
                    operation.op_type = kind;
                    ui.end_row();
                    ui.label("Speed (mm/min)");
                    ui.add(
                        egui::DragValue::new(&mut operation.params.speed)
                            .speed(10.0)
                            .range(1.0..=100_000.0),
                    );
                    ui.end_row();
                    ui.label("Power (%)");
                    ui.add(
                        egui::DragValue::new(&mut operation.params.power)
                            .speed(1.0)
                            .range(0.0..=100.0),
                    );
                    ui.end_row();
                    ui.label("Passes");
                    ui.add(
                        egui::DragValue::new(&mut operation.params.passes)
                            .speed(0.2)
                            .range(1..=1000),
                    );
                    ui.end_row();
                    ui.label("DPI");
                    let mut dpi = (25.4 / operation.params.line_interval_mm.max(0.001)).round();
                    let response = ui
                        .add(
                            egui::DragValue::new(&mut dpi)
                                .speed(1.0)
                                .range(10.0..=2540.0)
                                .max_decimals(0),
                        )
                        .on_hover_text("Lines per inch; stored as the line interval in mm");
                    if response.changed() {
                        operation.params.line_interval_mm = 25.4 / dpi.max(1.0);
                    }
                    ui.end_row();
                    ui.label("Air assist");
                    ui.checkbox(&mut operation.params.air_assist, "");
                    ui.end_row();
                    ui.label("Output");
                    ui.checkbox(&mut operation.enabled, "");
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
                ui.close();
            }
            if ui
                .button("Delete cut entry")
                .on_hover_text("Remove the cut entry; objects keep their colour")
                .clicked()
            {
                self.editor.remove_operation(&cut_id);
                ui.close();
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
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Duplicate layer").clicked() {
            if let Some(copy) = self.editor.duplicate_layer(id) {
                self.editor.active_layer_id = copy;
            }
            ui.close();
        }
        if ui.button("Delete layer").clicked() {
            if let Err(error) = self.editor.document.remove_layer(id) {
                self.status = error;
            }
            ui.close();
        }
        if ui.button("Select objects").clicked() {
            let count = self.editor.select_layer_objects(id);
            self.status = format!("Selected {count} object(s)");
            ui.close();
        }
        if ui.button("Select this colour").clicked() {
            let colour = self.layer_display_colour(id);
            let count = self.editor.select_by_color(&colour);
            self.status = format!("Selected {count} object(s) of {colour}");
            ui.close();
        }
        if ui.button("Group objects").clicked() {
            let count = self.editor.group_layer_objects(id);
            self.status = format!("Grouped {count} object(s)");
            ui.close();
        }
        if ui.button("Group by colour").clicked() {
            let (groups, objects) = self.editor.group_by_colour();
            self.status = if groups == 0 {
                "No colour has two or more objects".to_string()
            } else {
                format!("Grouped {objects} object(s) into {groups} colour group(s)")
            };
            ui.close();
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
            if let Some(layer) = self.editor.document.layer_mut(id) {
                layer.visible = !visible;
            }
            ui.close();
        }
        if ui
            .button(if locked { "Unlock layer" } else { "Lock layer" })
            .clicked()
        {
            if let Some(layer) = self.editor.document.layer_mut(id) {
                layer.locked = !locked;
            }
            ui.close();
        }
        if name.is_empty() {
            ui.weak("Missing layer");
        }
    }
    /// Compact colour strip: applies the colour to the selection, or makes it
    /// the colour of the active layer so the next shapes use it.
    pub(super) fn color_palette(&mut self, ui: &mut Ui) {
        egui::Panel::bottom("color_palette").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Colour");
                ui.spacing_mut().item_spacing.x = 2.0;
                let active = self
                    .editor
                    .document
                    .layer(&self.editor.active_layer_id)
                    .map(|layer| layer.color.to_ascii_lowercase());
                for colour in DRAWING_COLOURS {
                    let (r, g, b) = parse_hex_color(colour).unwrap_or((0, 0, 0));
                    let selected = active.as_deref() == Some(colour);
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
                            (0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b))
                                / 255.0;
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
            });
        });
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
        if self.editor.document != before {
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
