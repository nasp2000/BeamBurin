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

    pub(super) fn layers_panel(&mut self, ui: &mut Ui) {
        egui::Panel::left("layers_panel")
            .default_size(420.0)
            .resizable(true)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.label(egui::RichText::new("Cuts / Layers").strong());
                let before = self.editor.document.clone();
                let mut message: Option<String> = None;
                ui.horizontal(|ui| {
                    if ui.button("+ Layer").clicked() {
                        let name = format!("Layer {}", self.editor.document.layers.len() + 1);
                        let layer = self.editor.document.add_layer(name);
                        self.editor.active_layer_id.clone_from(&layer.id);
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
                    if ui.button("+ Mode").clicked() {
                        let id = self.editor.add_operation(
                            format!("Mode {}", self.editor.document.operations.len() + 1),
                            crate::operations::OperationType::OutlineCutting,
                        );
                        self.active_operation = Some(id);
                    }
                });

                // One row per layer, with the mode and the switches that matter
                // for the job. The remaining settings live in the row menu and
                // in the Cuts / Layers window.
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

                let mut assign: Option<(String, Option<String>)> = None;
                let mut toggle_operation: Option<(String, bool, bool)> = None;
                egui::ScrollArea::horizontal()
                    .id_salt("layers_table_scroll")
                    .show(ui, |ui| {
                        egui::Grid::new("layers_grid")
                            .num_columns(7)
                            .striped(true)
                            .spacing([4.0, 2.0])
                            .show(ui, |ui| {
                                ui.add_sized(
                                    [26.0, 18.0],
                                    egui::Label::new(egui::RichText::new("#").strong()),
                                );
                                ui.horizontal(|ui| {
                                    ui.add_space(20.0);
                                    ui.label(egui::RichText::new("Layer").strong());
                                });
                                for header in ["Mode", "Spd/Pwr", "Out", "Show", "Air"] {
                                    let width = if header == "Mode" {
                                        104.0
                                    } else if header == "Spd/Pwr" {
                                        52.0
                                    } else {
                                        26.0
                                    };
                                    ui.add_sized(
                                        [width, 18.0],
                                        egui::Label::new(egui::RichText::new(header).strong()),
                                    );
                                }
                                ui.end_row();
                                for (index, row) in rows.iter().enumerate() {
                                    let (id, name, color, visible, _locked, mode, mixed, count) =
                                        row;
                                    let selected = self.editor.active_layer_id == *id;
                                    let operation = mode.as_deref().and_then(|mode| {
                                        operations.iter().find(|entry| entry.0 == mode)
                                    });
                                    let mut name_hit: Option<egui::Response> = None;
                                    ui.add_sized(
                                        [26.0, 18.0],
                                        egui::Label::new(format!("L{:02}", index + 1)),
                                    );
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(120.0, 18.0),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            let rgb = parse_hex_color(color).unwrap_or((0, 0, 0));
                                            let mut srgb = [rgb.0, rgb.1, rgb.2];
                                            if ui.color_edit_button_srgb(&mut srgb).changed() {
                                                let colour = to_hex(srgb[0], srgb[1], srgb[2]);
                                                self.set_layer_colour(id, &colour);
                                            }
                                            let name_button = ui
                                                .add_sized(
                                                    [82.0, 18.0],
                                                    egui::Button::selectable(
                                                        selected,
                                                        name.as_str(),
                                                    )
                                                    .frame(false),
                                                )
                                                .on_hover_text(format!("{count} object(s)"));
                                            if name_button.clicked() {
                                                self.editor.active_layer_id.clone_from(id);
                                            }
                                            name_hit = Some(name_button);
                                        },
                                    );
                                    let current = if *mixed {
                                        "Mixed".to_string()
                                    } else {
                                        mode.as_deref()
                                            .and_then(|mode| {
                                                operations
                                                    .iter()
                                                    .find(|entry| entry.0 == mode)
                                                    .map(|entry| entry.1.clone())
                                            })
                                            .unwrap_or_else(|| "—".to_string())
                                    };
                                    let mut chosen = mode.clone();
                                    egui::ComboBox::from_id_salt(format!("layer_mode_{id}"))
                                        .width(100.0)
                                        .selected_text(current)
                                        .show_ui(ui, |ui| {
                                            if ui.selectable_value(&mut chosen, None, "—").clicked()
                                            {
                                                ui.close();
                                            }
                                            for (op_id, op_name, ..) in &operations {
                                                if ui
                                                    .selectable_value(
                                                        &mut chosen,
                                                        Some(op_id.clone()),
                                                        op_name,
                                                    )
                                                    .clicked()
                                                {
                                                    ui.close();
                                                }
                                            }
                                        });
                                    if &chosen != mode {
                                        assign = Some((id.clone(), chosen));
                                    }
                                    let speeds = operation.map_or_else(
                                        || "—".to_string(),
                                        |entry| format!("{:.0} / {:.0}", entry.2, entry.3),
                                    );
                                    ui.add_sized([52.0, 18.0], egui::Label::new(speeds));
                                    let mut output = operation.is_some_and(|entry| entry.4);
                                    let output_response = ui
                                        .push_id(("layer_output", id), |ui| {
                                            ui.add_sized(
                                                [26.0, 18.0],
                                                egui::Checkbox::without_text(&mut output),
                                            )
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
                                                [26.0, 18.0],
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
                                            ui.add_sized(
                                                [26.0, 18.0],
                                                egui::Checkbox::without_text(&mut air),
                                            )
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
                                    ui.end_row();
                                    if let Some(name_button) = name_hit {
                                        name_button.context_menu(|ui| {
                                            self.layer_context_menu(ui, id, mode.as_deref());
                                        });
                                    }
                                }
                            });
                    });
                if let Some((id, operation)) = assign {
                    let changed = self
                        .editor
                        .assign_operation_to_layer(&id, operation.as_deref());
                    self.status = format!("Assigned a mode to {changed} object(s)");
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

                ui.separator();
                let active = self.editor.active_layer_id.clone();
                if let Some(layer) = self.editor.document.layer(&active).cloned() {
                    ui.strong(format!("Layer: {}", layer.name));
                    egui::Grid::new("layer_details")
                        .num_columns(2)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Color");
                            let rgb = parse_hex_color(&layer.color).unwrap_or((0, 0, 0));
                            let mut srgb = [rgb.0, rgb.1, rgb.2];
                            if ui.color_edit_button_srgb(&mut srgb).changed() {
                                self.set_layer_colour(&active, &to_hex(srgb[0], srgb[1], srgb[2]));
                            }
                            ui.end_row();
                            ui.label("Name");
                            if let Some(layer) = self.editor.document.layer_mut(&active) {
                                ui.text_edit_singleline(&mut layer.name);
                            }
                            ui.end_row();
                            let (mode, _) = self.layer_mode(&active);
                            ui.label("Mode");
                            let current = mode
                                .as_deref()
                                .and_then(|mode| {
                                    operations
                                        .iter()
                                        .find(|entry| entry.0 == mode)
                                        .map(|entry| entry.1.clone())
                                })
                                .unwrap_or_else(|| "—".to_string());
                            let mut chosen = mode.clone();
                            egui::ComboBox::from_id_salt("layer_details_mode")
                                .selected_text(current)
                                .show_ui(ui, |ui| {
                                    if ui.selectable_value(&mut chosen, None, "—").clicked() {
                                        ui.close();
                                    }
                                    for (op_id, op_name, ..) in &operations {
                                        if ui
                                            .selectable_value(
                                                &mut chosen,
                                                Some(op_id.clone()),
                                                op_name,
                                            )
                                            .clicked()
                                        {
                                            ui.close();
                                        }
                                    }
                                });
                            if chosen != mode {
                                self.editor
                                    .assign_operation_to_layer(&active, chosen.as_deref());
                            }
                            ui.end_row();
                        });
                    let (mode, _) = self.layer_mode(&active);
                    if let Some(operation) = self
                        .editor
                        .document
                        .operations
                        .iter_mut()
                        .find(|operation| Some(operation.id.as_str()) == mode.as_deref())
                    {
                        egui::Grid::new("layer_operation_details")
                            .num_columns(2)
                            .spacing([8.0, 4.0])
                            .show(ui, |ui| {
                                ui.label("Speed (mm/min)");
                                ui.add(
                                    egui::DragValue::new(&mut operation.params.speed)
                                        .speed(10.0)
                                        .range(1.0..=100_000.0),
                                );
                                ui.end_row();
                                ui.label("Pass count");
                                ui.add(
                                    egui::DragValue::new(&mut operation.params.passes)
                                        .speed(0.2)
                                        .range(1..=100),
                                );
                                ui.end_row();
                                ui.label("Power max (%)");
                                ui.add(
                                    egui::DragValue::new(&mut operation.params.power)
                                        .speed(1.0)
                                        .range(0.0..=100.0),
                                );
                                ui.end_row();
                                ui.label("Interval (mm)");
                                ui.add(
                                    egui::DragValue::new(&mut operation.params.line_interval_mm)
                                        .speed(0.01)
                                        .range(0.01..=10.0),
                                );
                                ui.end_row();
                            });
                        if ui
                            .button("All settings...")
                            .on_hover_text("Passes, leads, kerf, tabs, fill and output")
                            .clicked()
                        {
                            self.active_operation = Some(operation.id.clone());
                            self.cuts_open = true;
                        }
                    } else {
                        ui.weak("Choose a mode to set speed and power.");
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
                ui.separator();
                egui::CollapsingHeader::new("All operations")
                    .default_open(false)
                    .show(ui, |ui| self.operations_section(ui));
            });
    }

    pub(super) fn layer_context_menu(&mut self, ui: &mut Ui, id: &str, mode: Option<&str>) {
        let name = self
            .editor
            .document
            .layer(id)
            .map(|layer| layer.name.clone())
            .unwrap_or_default();
        let operations: Vec<(String, String)> = self
            .editor
            .document
            .operations
            .iter()
            .map(|operation| (operation.id.clone(), operation.name.clone()))
            .collect();
        let mut assign: Option<Option<String>> = None;
        if ui.button("Rename (edit below)").clicked() {
            self.editor.active_layer_id = id.to_string();
            ui.close();
        }
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
        if ui.button("Group objects").clicked() {
            let count = self.editor.group_layer_objects(id);
            self.status = format!("Grouped {count} object(s)");
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
        ui.menu_button("Mode", |ui| {
            if ui.button("None").clicked() {
                assign = Some(None);
                ui.close();
            }
            for (op_id, op_name) in &operations {
                if ui
                    .selectable_label(mode == Some(op_id.as_str()), op_name)
                    .clicked()
                {
                    assign = Some(Some(op_id.clone()));
                    ui.close();
                }
            }
        });
        if let Some(mode) = mode {
            if ui.button("All settings...").clicked() {
                self.active_operation = Some(mode.to_string());
                self.cuts_open = true;
                ui.close();
            }
        }
        if let Some(assignment) = assign {
            let changed = self
                .editor
                .assign_operation_to_layer(id, assignment.as_deref());
            self.status = format!("Assigned a mode to {changed} object(s)");
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
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            format!("Drawing colour {colour}"),
                        )
                    });
                    if response.on_hover_text(format!("Use {colour}")).clicked() {
                        self.apply_drawing_colour(colour);
                    }
                }
            });
        });
    }

    fn apply_drawing_colour(&mut self, colour: &str) {
        let before = self.editor.document.clone();
        let selected = self.editor.selection.ids().to_vec();
        if selected.is_empty() {
            let active = self.editor.active_layer_id.clone();
            if let Some(layer) = self.editor.document.layer_mut(&active) {
                layer.color = colour.to_string();
            }
            if let Some(operation) = self.layer_mode(&active).0.and_then(|id| {
                self.editor
                    .document
                    .operations
                    .iter_mut()
                    .find(|operation| operation.id == id)
            }) {
                operation.params.color = colour.to_string();
            }
            self.status = format!("New objects will use {colour}");
        } else {
            for id in &selected {
                if let Some(object) = self.editor.document.object_mut(id) {
                    object.color = Some(colour.to_string());
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
                    }
                    if let Some(operation) = self.layer_mode(&layer_id).0.and_then(|id| {
                        self.editor
                            .document
                            .operations
                            .iter_mut()
                            .find(|operation| operation.id == id)
                    }) {
                        operation.params.color = colour.to_string();
                    }
                }
            }
            self.status = format!("Colour {colour} applied to {} object(s)", selected.len());
        }
        if self.editor.document != before {
            self.editor.history.record(&before, "Colour");
        }
    }

    fn set_layer_colour(&mut self, layer_id: &str, colour: &str) {
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
        for id in ids {
            if let Some(object) = self.editor.document.object_mut(&id) {
                object.color = Some(colour.to_string());
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

    pub(super) fn operations_section(&mut self, ui: &mut Ui) {
        use crate::operations::OperationType;

        ui.heading("Operations");
        let before = self.editor.document.clone();
        let operations: Vec<_> = self
            .editor
            .document
            .operations
            .iter()
            .map(|operation| {
                (
                    operation.id.clone(),
                    operation.name.clone(),
                    operation.op_type,
                    operation.enabled,
                    operation.params.speed,
                    operation.params.power,
                )
            })
            .collect();
        ui.horizontal(|ui| {
            if ui.button("+ Operation").clicked() {
                let id = self.editor.add_operation(
                    format!("Operation {}", operations.len() + 1),
                    OperationType::OutlineCutting,
                );
                self.active_operation = Some(id);
            }
            let active = self.active_operation.clone().unwrap_or_default();
            if ui
                .add_enabled(
                    operations.iter().any(|(id, ..)| *id == active),
                    egui::Button::new("- Operation"),
                )
                .clicked()
            {
                self.editor.remove_operation(&active);
                self.active_operation = None;
            }
        });
        egui::ScrollArea::horizontal().show(ui, |ui| {
            egui::Grid::new("operations_table")
                .num_columns(6)
                .striped(true)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    for header in ["On", "Operation", "Type", "Spd/Pwr", "Used", ""] {
                        ui.strong(header);
                    }
                    ui.end_row();
                    for (id, name, op_type, enabled, speed, power) in &operations {
                        let mut is_enabled = *enabled;
                        if ui.checkbox(&mut is_enabled, "").changed() {
                            if let Some(operation) = self
                                .editor
                                .document
                                .operations
                                .iter_mut()
                                .find(|operation| operation.id == *id)
                            {
                                operation.enabled = is_enabled;
                            }
                        }
                        let selected = self.active_operation.as_deref() == Some(id.as_str());
                        if ui.selectable_label(selected, name).clicked() {
                            self.active_operation = Some(id.clone());
                        }
                        ui.label(op_type.label());
                        ui.label(format!("{speed:.0} / {power:.0}"));
                        let used = self
                            .editor
                            .document
                            .objects
                            .iter()
                            .filter(|object| object.operation_id.as_deref() == Some(id.as_str()))
                            .count();
                        ui.weak(used.to_string());
                        ui.horizontal(|ui| {
                            if ui
                                .small_button("Up")
                                .on_hover_text("Run this operation earlier")
                                .clicked()
                            {
                                self.editor.move_operation(id, -1);
                            }
                            if ui
                                .small_button("Down")
                                .on_hover_text("Run this operation later")
                                .clicked()
                            {
                                self.editor.move_operation(id, 1);
                            }
                            if ui
                                .small_button("Duplicate")
                                .on_hover_text("Duplicate this operation")
                                .clicked()
                            {
                                if let Some(copy) = self.editor.duplicate_operation(id) {
                                    self.active_operation = Some(copy);
                                }
                            }
                        });
                        ui.end_row();
                    }
                });
        });
        let mut copy_parameters = false;
        let mut paste_parameters = false;
        let mut save_as_profile = false;
        let mut dependency_choice: Option<Option<String>> = None;
        if let Some(active) = self.active_operation.clone() {
            if let Some(operation) = self
                .editor
                .document
                .operations
                .iter_mut()
                .find(|operation| operation.id == active)
            {
                ui.label("Name");
                ui.text_edit_singleline(&mut operation.name);
                let mut op_type = operation.op_type;
                egui::ComboBox::from_label("Type")
                    .selected_text(op_type.label())
                    .show_ui(ui, |ui| {
                        for candidate in OperationType::all() {
                            ui.selectable_value(&mut op_type, candidate, candidate.label());
                        }
                    });
                operation.op_type = op_type;
                ui.horizontal(|ui| {
                    ui.label("Runs after");
                    let selected = operation.depends_on.first().cloned();
                    let selected_text = selected
                        .as_ref()
                        .and_then(|id| {
                            operations
                                .iter()
                                .find(|(operation_id, ..)| operation_id == id)
                                .map(|(_, name, ..)| name.clone())
                        })
                        .unwrap_or_else(|| "None".to_string());
                    egui::ComboBox::from_id_salt("operation_dependency")
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(selected.is_none(), "None").clicked() {
                                dependency_choice = Some(None);
                            }
                            for (id, name, ..) in &operations {
                                if *id == active {
                                    continue;
                                }
                                if ui
                                    .selectable_label(
                                        selected.as_deref() == Some(id.as_str()),
                                        name,
                                    )
                                    .clicked()
                                {
                                    dependency_choice = Some(Some(id.clone()));
                                }
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Speed");
                    ui.add(
                        egui::DragValue::new(&mut operation.params.speed)
                            .speed(10.0)
                            .range(1.0..=20000.0)
                            .suffix(" mm/min"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Power");
                    ui.add(
                        egui::DragValue::new(&mut operation.params.power)
                            .speed(1.0)
                            .range(0.0..=100.0)
                            .suffix(" %"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Passes");
                    ui.add(
                        egui::DragValue::new(&mut operation.params.passes)
                            .speed(1.0)
                            .range(1..=100),
                    );
                    ui.label("Interval");
                    ui.add(
                        egui::DragValue::new(&mut operation.params.line_interval_mm)
                            .speed(0.01)
                            .range(0.01..=10.0)
                            .suffix(" mm"),
                    );
                });
                if matches!(
                    operation.op_type,
                    OperationType::Fill
                        | OperationType::FillWithOutline
                        | OperationType::FillWithOffset
                ) {
                    ui.horizontal(|ui| {
                        ui.label("Fill angle");
                        ui.add(
                            egui::DragValue::new(&mut operation.params.fill_angle_degrees)
                                .speed(1.0)
                                .range(-180.0..=180.0)
                                .suffix(" deg"),
                        );
                        ui.label("Margin");
                        ui.add(
                            egui::DragValue::new(&mut operation.params.fill_margin_mm)
                                .speed(0.1)
                                .range(0.0..=50.0)
                                .suffix(" mm"),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Pattern offset");
                        ui.add(
                            egui::DragValue::new(&mut operation.params.fill_offset_mm)
                                .speed(0.1)
                                .range(-50.0..=50.0)
                                .suffix(" mm"),
                        );
                        ui.checkbox(&mut operation.params.fill_alternate, "Alternate direction");
                    });
                    ui.weak("Fill spacing is the line interval above. Offset fill uses the interval between loops.");
                }
                ui.horizontal(|ui| {
                    if ui.button("Copy parameters").clicked() {
                        copy_parameters = true;
                    }
                    if ui
                        .add_enabled(
                            self.operation_params.is_some(),
                            egui::Button::new("Paste parameters"),
                        )
                        .clicked()
                    {
                        paste_parameters = true;
                    }
                    if ui.button("Save as profile").clicked() {
                        save_as_profile = true;
                    }
                });
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        !self.editor.selection.is_empty(),
                        egui::Button::new("Assign to selection"),
                    )
                    .clicked()
                {
                    let changed = self.editor.assign_operation(Some(&active));
                    self.status = format!("Assigned operation to {changed} object(s)");
                }
                if ui
                    .add_enabled(
                        !self.editor.selection.is_empty(),
                        egui::Button::new("Clear assignment"),
                    )
                    .clicked()
                {
                    let changed = self.editor.assign_operation(None);
                    self.status = format!("Cleared operation on {changed} object(s)");
                }
                if ui
                    .add_enabled(
                        !self.editor.document.objects.is_empty(),
                        egui::Button::new("Assign to active layer"),
                    )
                    .clicked()
                {
                    let layer = self.editor.active_layer_id.clone();
                    let changed = self.editor.assign_operation_to_layer(&layer, Some(&active));
                    self.status = format!("Assigned operation to {changed} layer object(s)");
                }
                if ui
                    .add_enabled(
                        !self.editor.document.objects.is_empty(),
                        egui::Button::new("Clear layer assignment"),
                    )
                    .clicked()
                {
                    let layer = self.editor.active_layer_id.clone();
                    let changed = self.editor.assign_operation_to_layer(&layer, None);
                    self.status = format!("Cleared operation on {changed} layer object(s)");
                }
            });
        }
        if let Some(choice) = dependency_choice {
            if let Some(active) = self.active_operation.clone() {
                self.editor
                    .set_operation_dependency(&active, choice.as_deref());
            }
        }
        if let Some(active) = self.active_operation.clone() {
            if copy_parameters {
                if let Some(operation) = self
                    .editor
                    .document
                    .operations
                    .iter()
                    .find(|operation| operation.id == active)
                {
                    self.operation_params = Some(operation.params.clone());
                    self.status = "Operation parameters copied".to_string();
                }
            }
            if paste_parameters {
                if let Some(params) = self.operation_params.clone() {
                    if let Some(operation) = self
                        .editor
                        .document
                        .operations
                        .iter_mut()
                        .find(|operation| operation.id == active)
                    {
                        operation.params = params;
                        self.status = "Operation parameters pasted".to_string();
                    }
                }
            }
            if save_as_profile {
                if let Some(operation) = self
                    .editor
                    .document
                    .operations
                    .iter()
                    .find(|operation| operation.id == active)
                {
                    let mut profile = crate::profiles::OperationProfile::new(
                        format!("{} profile", operation.name),
                        operation.op_type,
                    );
                    profile.params = operation.params.clone();
                    profile.notes = "Saved from an operation".to_string();
                    self.profiles.add_operation(profile);
                    self.save_profiles();
                    self.status = "Operation saved as a profile".to_string();
                }
            }
        }
        if self.editor.document != before {
            self.editor.history.record(&before, "Edit operation");
        }
    }
}
