#![allow(clippy::wildcard_imports)]

use super::*;

impl BeamBurinApp {
    pub(super) fn open_parameters(&mut self) {
        if self.parameters_snapshot.is_none() {
            self.parameters_snapshot = Some(self.editor.document.clone());
        }
        self.parameters_open = true;
    }

    pub(super) fn open_cuts(&mut self) {
        self.cuts_snapshot = Some(self.editor.document.clone());
        if self.active_operation.is_none() {
            self.active_operation = self
                .editor
                .document
                .operations
                .first()
                .map(|operation| operation.id.clone());
        }
        self.cuts_open = true;
    }

    pub(super) fn cuts_window(&mut self, ctx: &egui::Context) {
        use crate::operations::OperationType;

        let mut open = self.cuts_open;
        let mut close_requested = false;
        let mut operations = self.editor.document.operations.clone();
        egui::Window::new("Cuts / Layers")
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([760.0, 560.0])
        .max_height(640.0)
        .show(ctx, |ui| {
            ui.label("One row per cut. Output controls the job, Show controls the preview, Air emits M8/M9.");
            ui.horizontal(|ui| {
                ui.label("G-code dialect");
                let mut dialect = self.gcode_dialect;
                egui::ComboBox::from_id_salt("gcode_dialect")
                    .selected_text(dialect.label())
                    .show_ui(ui, |ui| {
                        for candidate in crate::gcode::Dialect::all() {
                            ui.selectable_value(&mut dialect, candidate, candidate.label());
                        }
                    });
                self.gcode_dialect = dialect;
                ui.label("Smooth paths (mm)");
                ui.add(
                    egui::DragValue::new(&mut self.gcode_smooth_mm)
                        .speed(0.05)
                        .range(0.0..=5.0),
                )
                .on_hover_text(
                    "Rounds the corners of cut runs before writing the file; zero keeps the exact geometry",
                );
            });
            ui.horizontal(|ui| {
                ui.label("Decimals");
                ui.add(
                    egui::DragValue::new(&mut self.gcode_decimals).range(0..=6),
                )
                .on_hover_text("Decimals written for coordinates");
                ui.checkbox(&mut self.gcode_comments, "Comments");
                ui.checkbox(&mut self.gcode_travel, "Travel moves");
                ui.checkbox(&mut self.gcode_crlf, "CRLF line endings");
                ui.checkbox(&mut self.gcode_arcs, "Arcs (G2/G3)")
                    .on_hover_text(
                        "Writes circular cut runs as G2/G3 arcs; leave it off if the sender \
                         does not accept arcs",
                    );
                if ui
                    .button("Export presets...")
                    .on_hover_text("Save and reuse these output options")
                    .clicked()
                {
                    self.export_presets_open = true;
                }
            });
            ui.separator();
            let mut row_centers: Vec<(usize, f32)> = Vec::new();
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .show(ui, |ui| {
                    egui::Grid::new("cuts_table")
                        .num_columns(7)
                        .striped(true)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            for header in ["#", "Layer", "Mode", "Spd/Pwr", "Output", "Show", "Air"] {
                                ui.strong(header);
                            }
                            ui.end_row();
                            for (index, operation) in operations.iter_mut().enumerate() {
                                let number = ui.add(
                                    egui::Label::new(format!("{:02}", index + 1))
                                        .sense(egui::Sense::drag()),
                                );
                                row_centers.push((index, number.rect.center().y));
                                if number.drag_started() {
                                    self.cuts_drag = Some(index);
                                }
                                ui.horizontal(|ui| {
                                    let rgb = parse_hex_color(&operation.params.color)
                                        .unwrap_or((0, 0, 0));
                                    let mut srgb = [rgb.0, rgb.1, rgb.2];
                                    if ui.color_edit_button_srgb(&mut srgb).changed() {
                                        operation.params.color =
                                            to_hex(srgb[0], srgb[1], srgb[2]);
                                    }
                                    let selected = self.active_operation.as_deref()
                                        == Some(operation.id.as_str());
                                    if ui
                                        .selectable_label(selected, &operation.name)
                                        .clicked()
                                    {
                                        self.active_operation =
                                            Some(operation.id.clone());
                                    }
                                });
                                let mut op_type = operation.op_type;
                                egui::ComboBox::from_id_salt(("cut_mode", index))
                                    .selected_text(op_type.label())
                                    .show_ui(ui, |ui| {
                                        for candidate in OperationType::all() {
                                            ui.selectable_value(
                                                &mut op_type,
                                                candidate,
                                                candidate.label(),
                                            );
                                        }
                                    });
                                operation.op_type = op_type;
                                ui.label(format!(
                                    "{:.0} / {:.0}",
                                    operation.params.speed, operation.params.power
                                ));
                                ui.checkbox(&mut operation.enabled, "");
                                ui.checkbox(&mut operation.params.show_in_preview, "");
                                ui.checkbox(&mut operation.params.air_assist, "");
                                ui.end_row();
                            }
                        });
                });
            if self.cuts_drag.is_some() {
                if let Some(pos) =
                    ui.ctx().input(|input| input.pointer.interact_pos())
                {
                    self.cuts_drag_target = row_centers
                        .iter()
                        .min_by(|left, right| {
                            (left.1 - pos.y).abs().total_cmp(&(right.1 - pos.y).abs())
                        })
                        .map(|(index, _)| *index);
                }
            }
            if ui.ctx().input(|input| input.pointer.any_released()) {
                if let (Some(from), Some(to)) =
                    (self.cuts_drag.take(), self.cuts_drag_target.take())
                {
                    if from != to {
                        let operation = operations.remove(from);
                        let target = to.min(operations.len());
                        operations.insert(target, operation);
                    }
                }
            }
            ui.separator();
            if let Some(id) = self.active_operation.clone() {
                if let Some(index) = operations
                    .iter()
                    .position(|operation| operation.id == id)
                {
                    let operation = &mut operations[index];
                    ui.heading("Cut settings");
                    egui::Grid::new("cut_settings")
                        .num_columns(2)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("Name");
                            ui.text_edit_singleline(&mut operation.name);
                            ui.end_row();
                            ui.label("Colour");
                            let rgb = parse_hex_color(&operation.params.color)
                                .unwrap_or((0, 0, 0));
                            let mut srgb = [rgb.0, rgb.1, rgb.2];
                            if ui.color_edit_button_srgb(&mut srgb).changed() {
                                operation.params.color =
                                    to_hex(srgb[0], srgb[1], srgb[2]);
                            }
                            ui.end_row();
                            ui.label("Mode");
                            let mut op_type = operation.op_type;
                            egui::ComboBox::from_id_salt("cut_settings_mode")
                                .selected_text(op_type.label())
                                .show_ui(ui, |ui| {
                                    for candidate in OperationType::all() {
                                        ui.selectable_value(
                                            &mut op_type,
                                            candidate,
                                            candidate.label(),
                                        );
                                    }
                                });
                            operation.op_type = op_type;
                            ui.end_row();
                            ui.label("Output");
                            ui.checkbox(&mut operation.enabled, "");
                            ui.end_row();
                            ui.label("Air assist");
                            ui.checkbox(&mut operation.params.air_assist, "");
                            ui.end_row();
                            ui.label("Show in preview");
                            ui.checkbox(&mut operation.params.show_in_preview, "");
                            ui.end_row();
                            ui.label("Speed (mm/min)");
                            ui.add(
                                egui::DragValue::new(&mut operation.params.speed)
                                    .speed(10.0)
                                    .range(1.0..=100_000.0),
                            );
                            ui.end_row();
                            ui.label("Max power (%)");
                            ui.add(
                                egui::DragValue::new(&mut operation.params.power)
                                    .speed(1.0)
                                    .range(0.0..=100.0),
                            );
                            ui.end_row();
                            ui.label("Number of passes");
                            ui.add(
                                egui::DragValue::new(&mut operation.params.passes)
                                    .range(1..=1000),
                            );
                            ui.end_row();
                            ui.label("Line interval (mm)");
                            ui.add(
                                egui::DragValue::new(
                                    &mut operation.params.line_interval_mm,
                                )
                                .speed(0.01)
                                .range(0.01..=2.0),
                            );
                            ui.end_row();
                            ui.label("Lines per inch");
                            ui.weak(format!(
                                "{:.2}",
                                25.4 / operation.params.line_interval_mm.max(0.001)
                            ));
                            ui.end_row();
                            ui.label("Scan angle (deg)");
                            ui.add(
                                egui::DragValue::new(
                                    &mut operation.params.fill_angle_degrees,
                                )
                                .speed(1.0)
                                .range(-180.0..=180.0),
                            );
                            ui.end_row();
                            ui.label("Bi-directional fill");
                            let alternate =
                                ui.checkbox(&mut operation.params.fill_alternate, "");
                            alternate.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::Checkbox,
                                    true,
                                    operation.params.fill_alternate,
                                    "Bi-directional fill",
                                )
                            });
                            ui.end_row();
                            ui.label("Cross-hatch");
                            let cross = ui.checkbox(&mut operation.params.cross_hatch, "");
                            cross.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::Checkbox,
                                    true,
                                    operation.params.cross_hatch,
                                    "Cross-hatch",
                                )
                            });
                            ui.end_row();
                            ui.label("Overscan (mm)");
                            ui.add(
                                egui::DragValue::new(&mut operation.params.overscan_mm)
                                    .speed(0.1)
                                    .range(0.0..=50.0),
                            );
                            ui.end_row();
                            ui.label("Kerf (mm)");
                            ui.add(
                                egui::DragValue::new(&mut operation.params.kerf_mm)
                                    .speed(0.01)
                                    .range(0.0..=10.0),
                            );
                            ui.end_row();
                            ui.label("Holding tabs");
                            ui.add(
                                egui::DragValue::new(&mut operation.params.tabs)
                                    .range(0..=20),
                            );
                            ui.end_row();
                            ui.label("Tab width (mm)");
                            ui.add(
                                egui::DragValue::new(
                                    &mut operation.params.tab_width_mm,
                                )
                                .speed(0.1)
                                .range(0.0..=20.0),
                            );
                            ui.end_row();
                            ui.label("Tab spacing (mm)");
                            ui.add(
                                egui::DragValue::new(
                                    &mut operation.params.tab_spacing_mm,
                                )
                                .speed(0.5)
                                .range(0.0..=1000.0),
                            )
                            .on_hover_text(
                                "Distance between tabs along each contour; zero uses the tab count above",
                            );
                            ui.end_row();
                            ui.label("Lead-in / out (mm)");
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::DragValue::new(
                                        &mut operation.params.lead_in_mm,
                                    )
                                    .speed(0.1)
                                    .range(0.0..=50.0),
                                );
                                ui.add(
                                    egui::DragValue::new(
                                        &mut operation.params.lead_out_mm,
                                    )
                                    .speed(0.1)
                                    .range(0.0..=50.0),
                                );
                            });
                            ui.end_row();
                            ui.label("Curved leads");
                            ui.checkbox(
                                &mut operation.params.curved_leads,
                                "Quarter-circle entry and exit",
                            );
                            ui.end_row();
                            ui.label("Constant power mode");
                            let constant =
                                ui.checkbox(&mut operation.params.constant_power, "");
                            constant.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::Checkbox,
                                    true,
                                    operation.params.constant_power,
                                    "Constant power",
                                )
                            });
                            ui.end_row();
                        });
                    ui.add_space(4.0);
                    egui::CollapsingHeader::new("Expressions (optional)")
                        .default_open(false)
                        .show(ui, |ui| {
                            ui.weak(
                                "Override a numeric field with an expression over the document parameters. Empty fields use the value above.",
                            );
                            let environment = self.editor.document.parameters.clone();
                            egui::Grid::new("cut_expressions")
                                .num_columns(3)
                                .spacing([10.0, 6.0])
                                .show(ui, |ui| {
                                    for field in crate::operations::model::EXPRESSION_FIELDS {
                                        let mut expression = operation
                                            .params
                                            .expressions
                                            .get(field)
                                            .cloned()
                                            .unwrap_or_default();
                                        ui.label(field);
                                        let response = ui.add(
                                            egui::TextEdit::singleline(&mut expression)
                                                .desired_width(160.0)
                                                .hint_text("number or expression"),
                                        );
                                        if response.changed() {
                                            if expression.trim().is_empty() {
                                                operation.params.expressions.remove(field);
                                            } else {
                                                operation.params.expressions.insert(
                                                    field.to_string(),
                                                    expression,
                                                );
                                            }
                                        }
                                        match operation.params.expressions.get(field) {
                                            Some(current) => {
                                                match crate::parameters::evaluate(
                                                    current,
                                                    &environment,
                                                ) {
                                                    Ok(value) => {
                                                        ui.weak(format!("= {value:.3}"));
                                                    }
                                                    Err(error) => {
                                                        ui.colored_label(
                                                            Color32::from_rgb(190, 60, 50),
                                                            error,
                                                        );
                                                    }
                                                }
                                            }
                                            None => {
                                                ui.weak("");
                                            }
                                        }
                                        ui.end_row();
                                    }
                                });
                        });
                }
            } else {
                ui.weak("No cut selected. Add one below.");
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("+ Cut").clicked() {
                    let operation = crate::operations::Operation::new(
                        format!("Cut {}", operations.len() + 1),
                        OperationType::OutlineCutting,
                    );
                    self.active_operation = Some(operation.id.clone());
                    operations.push(operation);
                }
                if ui.button("Duplicate").clicked() {
                    if let Some(id) = self.active_operation.clone() {
                        if let Some(index) = operations
                            .iter()
                            .position(|operation| operation.id == id)
                        {
                            let mut copy = operations[index].clone();
                            copy.id = uuid::Uuid::new_v4().to_string();
                            copy.name = format!("{} copy", copy.name);
                            self.active_operation = Some(copy.id.clone());
                            operations.insert(index + 1, copy);
                        }
                    }
                }
                if ui.button("Delete").clicked() {
                    if let Some(id) = self.active_operation.clone() {
                        operations.retain(|operation| operation.id != id);
                        self.active_operation = operations
                            .first()
                            .map(|operation| operation.id.clone());
                    }
                }
                if ui.button("Up").clicked() {
                    if let Some(id) = self.active_operation.clone() {
                        if let Some(index) = operations
                            .iter()
                            .position(|operation| operation.id == id)
                        {
                            if index > 0 {
                                operations.swap(index, index - 1);
                            }
                        }
                    }
                }
                if ui.button("Down").clicked() {
                    if let Some(id) = self.active_operation.clone() {
                        if let Some(index) = operations
                            .iter()
                            .position(|operation| operation.id == id)
                        {
                            if index + 1 < operations.len() {
                                operations.swap(index, index + 1);
                            }
                        }
                    }
                }
                if ui.button("Close").on_hover_text("Close this window").clicked() {
                    close_requested = true;
                }
            });
        });
        self.editor.document.operations = operations;
        if close_requested {
            open = false;
        }
        self.cuts_open = open;
        if !open {
            if let Some(snapshot) = self.cuts_snapshot.take() {
                if snapshot != self.editor.document {
                    self.editor.history.record(&snapshot, "Cuts and layers");
                }
            }
            self.cuts_open = false;
        }
    }

    pub(super) fn parameters_window(&mut self, ctx: &egui::Context) {
        let mut open = self.parameters_open;
        let mut close_requested = false;
        let mut remove: Option<usize> = None;
        let mut add = false;
        egui::Window::new("Parameters")
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([560.0, 360.0])
        .show(ctx, |ui| {
            ui.label(
                "Named values for cut expressions, for example feed = 1200 and twice = feed * 2.",
            );
            ui.weak(
                "Units are labels for the reader; the arithmetic itself ignores them. Names use letters, digits and underscores.",
            );
            ui.separator();
            let mut parameters = std::mem::take(&mut self.editor.document.parameters);
            let environment = parameters.clone();
            egui::Grid::new("parameters_grid")
                .num_columns(5)
                .spacing([10.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Name");
                    ui.strong("Expression");
                    ui.strong("Unit");
                    ui.strong("Value");
                    ui.label("");
                    ui.end_row();
                    for (index, parameter) in parameters.iter_mut().enumerate() {
                        ui.add(
                            egui::TextEdit::singleline(&mut parameter.name)
                                .desired_width(120.0),
                        )
                        .on_hover_text("Unique name; letters, digits and underscores");
                        ui.add(
                            egui::TextEdit::singleline(&mut parameter.expression)
                                .desired_width(160.0),
                        )
                        .on_hover_text(
                            "A number, another parameter, or arithmetic with + - * / and parentheses",
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut parameter.unit)
                                .desired_width(52.0),
                        )
                        .on_hover_text("Label such as mm, mm/min or %; not used in arithmetic");
                        match crate::parameters::evaluate(&parameter.expression, &environment)
                        {
                            Ok(value) => {
                                ui.weak(format_parameter_value(value, &parameter.unit));
                            }
                            Err(error) => {
                                ui.colored_label(
                                    Color32::from_rgb(190, 60, 50),
                                    "invalid",
                                )
                                .on_hover_text(error);
                            }
                        }
                        if ui
                            .small_button("x")
                            .on_hover_text("Delete this parameter")
                            .clicked()
                        {
                            remove = Some(index);
                        }
                        ui.end_row();
                    }
                });
            if let Some(index) = remove {
                parameters.remove(index);
            }
            if let Err(error) = crate::parameters::validate(&parameters) {
                ui.colored_label(Color32::from_rgb(190, 60, 50), error);
            }
            let environment = parameters.clone();
            for operation in &self.editor.document.operations {
                for (field, expression) in &operation.params.expressions {
                    if let Err(error) =
                        crate::parameters::evaluate(expression, &environment)
                    {
                        ui.colored_label(
                            Color32::from_rgb(190, 60, 50),
                            format!("{} field '{field}': {error}", operation.name),
                        );
                    }
                }
            }
            if add {
                let mut index = parameters.len() + 1;
                while parameters
                    .iter()
                    .any(|parameter| parameter.name == format!("parameter{index}"))
                {
                    index += 1;
                }
                parameters.push(crate::parameters::Parameter::new(
                    format!("parameter{index}"),
                    "1",
                ));
            }
            self.editor.document.parameters = parameters;
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .button("+ Parameter")
                    .on_hover_text("Add a named value")
                    .clicked()
                {
                    add = true;
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
        self.parameters_open = open;
        if !open {
            if let Some(snapshot) = self.parameters_snapshot.take() {
                if snapshot != self.editor.document {
                    self.editor.history.record(&snapshot, "Parameters");
                }
            }
        }
    }

    pub(super) fn open_constraints(&mut self) {
        if self.constraints_snapshot.is_none() {
            self.constraints_snapshot = Some(self.editor.document.clone());
        }
        self.constraint_message.clear();
        self.constraints_open = true;
    }

    pub(super) fn add_constraint_from_selection(&mut self) {
        use crate::constraints::ConstraintKind;
        let Some(object_id) = self.editor.node_object.clone() else {
            self.constraint_message =
                "Enter node editing (double-click a path) and select the nodes first".to_string();
            return;
        };
        let selection = self.editor.node_selection.clone();
        let node =
            |index: usize| crate::constraints::NodeRef::new(object_id.clone(), selection[index]);
        let key = self.constraint_kind_key.as_str();
        let needed = if key == "angle" { 3 } else { 2 };
        if selection.len() < needed {
            self.constraint_message = format!(
                "Select {needed} node(s) on the path; {} selected",
                selection.len()
            );
            return;
        }
        let kind = match key {
            "coincident" => ConstraintKind::Coincident {
                a: node(0),
                b: node(1),
            },
            "vertical" => ConstraintKind::Vertical {
                a: node(0),
                b: node(1),
            },
            "distance" => ConstraintKind::Distance {
                a: node(0),
                b: node(1),
                value_mm: self.constraint_value.max(0.001),
            },
            "angle" => ConstraintKind::Angle {
                a: node(0),
                b: node(1),
                c: node(2),
                degrees: self.constraint_value,
            },
            "parallel" => ConstraintKind::Parallel {
                a: node(0),
                b: node(1),
            },
            "perpendicular" => ConstraintKind::Perpendicular {
                a: node(0),
                b: node(1),
            },
            "tangent" => ConstraintKind::Tangent {
                a: node(0),
                b: node(1),
            },
            _ => ConstraintKind::Horizontal {
                a: node(0),
                b: node(1),
            },
        };
        let constraint = crate::constraints::Constraint::new(kind);
        let mut candidate = self.editor.document.clone();
        candidate.constraints.push(constraint);
        if let Err(error) = candidate.validate() {
            self.constraint_message = error;
            return;
        }
        self.editor.record_action("Add constraint");
        self.editor.document = candidate;
        self.constraint_message = "Constraint added".to_string();
    }

    pub(super) fn solve_constraints(&mut self) {
        match crate::constraints::solve(&self.editor.document, None) {
            Ok((solved, report)) if report.constraints > 0 => {
                self.editor.record_action("Solve constraints");
                self.editor.document = solved;
                self.constraint_message = format!(
                    "Solved {} constraint(s): moved {} node(s), residual {:.1e} after {} iteration(s)",
                    report.constraints, report.moved_nodes, report.residual, report.iterations
                );
            }
            Ok(_) => {
                self.constraint_message = "No enabled constraints to solve".to_string();
            }
            Err(error) => {
                self.constraint_message = error.to_string();
            }
        }
    }

    pub(super) fn constraints_window(&mut self, ctx: &egui::Context) {
        let mut open = self.constraints_open;
        let mut solve = false;
        let mut add = false;
        let mut close_requested = false;
        let mut remove: Option<usize> = None;
        egui::Window::new("Constraints")
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([620.0, 480.0])
        .show(ctx, |ui| {
            ui.label(
                "Constraints keep path nodes in place while you edit. They are solved on demand, in document millimetres.",
            );
            ui.weak(
                "Double-click a path to edit its nodes, select the nodes in the order the constraint needs, then add it here.",
            );
            ui.separator();
            ui.heading("Add");
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("constraint_kind")
                    .selected_text(constraint_kind_label(&self.constraint_kind_key))
                    .show_ui(ui, |ui| {
                        for key in CONSTRAINT_KINDS {
                            ui.selectable_value(
                                &mut self.constraint_kind_key,
                                key.to_string(),
                                constraint_kind_label(key),
                            );
                        }
                    });
                if matches!(
                    self.constraint_kind_key.as_str(),
                    "distance" | "angle"
                ) {
                    ui.add(
                        egui::DragValue::new(&mut self.constraint_value)
                            .speed(0.5)
                            .range(-3600.0..=3600.0)
                            .suffix(if self.constraint_kind_key == "angle" {
                                " deg"
                            } else {
                                " mm"
                            }),
                    );
                }
                let needed = if self.constraint_kind_key == "angle" {
                    3
                } else {
                    2
                };
                let ready = self.editor.node_object.is_some()
                    && self.editor.node_selection.len() >= needed;
                if ui
                    .add_enabled(ready, egui::Button::new("Add from selected nodes"))
                    .on_hover_text(match needed {
                        3 => "Select three nodes: first arm, vertex, second arm",
                        _ => "Select two nodes; for parallel, perpendicular and tangent each node starts the segment",
                    })
                    .clicked()
                {
                    add = true;
                }
            });
            if let Some(object_id) = &self.editor.node_object {
                let name = self
                    .editor
                    .document
                    .object(object_id)
                    .map_or("(missing)".to_string(), |object| object.name.clone());
                ui.weak(format!(
                    "Editing '{}': {} node(s) selected",
                    name,
                    self.editor.node_selection.len()
                ));
            } else {
                ui.weak("Not editing nodes.");
            }
            if !self.constraint_message.is_empty() {
                ui.label(self.constraint_message.clone());
            }
            ui.separator();
            ui.heading("Stored");
            let mut removal = None;
            egui::ScrollArea::vertical()
                .id_salt("constraint_list")
                .max_height(260.0)
                .show(ui, |ui| {
                    if self.editor.document.constraints.is_empty() {
                        ui.weak("No constraints yet.");
                    }
                    for (index, constraint) in
                        self.editor.document.constraints.iter_mut().enumerate()
                    {
                        ui.horizontal(|ui| {
                            let mut enabled = constraint.enabled;
                            if ui.checkbox(&mut enabled, "").changed() {
                                constraint.enabled = enabled;
                            }
                            ui.label(constraint.kind.label());
                            ui.weak(constraint.describe());
                            if let Some(value) = constraint.kind.value() {
                                let mut changed = value;
                                ui.add(
                                    egui::DragValue::new(&mut changed)
                                        .speed(0.5)
                                        .range(-3600.0..=3600.0),
                                );
                                if (changed - value).abs() > 1e-9 {
                                    constraint.kind.set_value(changed);
                                }
                            }
                            if ui
                                .small_button("x")
                                .on_hover_text("Delete this constraint")
                                .clicked()
                            {
                                removal = Some(index);
                            }
                        });
                    }
                });
            remove = removal;
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .button("Solve")
                    .on_hover_text("Move the nodes so every enabled constraint holds")
                    .clicked()
                {
                    solve = true;
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
        if add {
            self.add_constraint_from_selection();
        }
        if let Some(index) = remove {
            let mut candidate = self.editor.document.clone();
            if index < candidate.constraints.len() {
                candidate.constraints.remove(index);
                self.editor.record_action("Remove constraint");
                self.editor.document = candidate;
                self.constraint_message = "Constraint removed".to_string();
            }
        }
        if solve {
            self.solve_constraints();
        }
        self.constraints_open = open;
        if !open {
            if let Some(snapshot) = self.constraints_snapshot.take() {
                if snapshot != self.editor.document {
                    self.editor.history.record(&snapshot, "Constraints");
                }
            }
        }
    }

    pub(super) fn save_active_cut_as_recipe(&mut self) {
        let name = self.recipe_name.trim().to_string();
        if name.is_empty() {
            self.recipe_message = "Give the recipe a name first".to_string();
            return;
        }
        if self.recipe_material.is_empty() {
            self.recipe_message = "Choose the material this recipe is for".to_string();
            return;
        }
        let Some(active) = self.active_operation.clone() else {
            self.recipe_message = "Select a cut in Cuts / Layers first".to_string();
            return;
        };
        let Some(operation) = self
            .editor
            .document
            .operations
            .iter()
            .find(|operation| operation.id == active)
        else {
            self.recipe_message = "The active cut no longer exists".to_string();
            return;
        };
        let mut profile = crate::profiles::OperationProfile::new(&name, operation.op_type);
        profile.params = operation.params.clone();
        profile.notes = format!("Saved from '{}'", operation.name);
        let material_id = self.recipe_material.clone();
        let mut recipe = crate::recipes::Recipe::new(name, material_id, profile);
        recipe.machine_id.clone_from(&self.recipe_machine);
        recipe.head = self.recipe_head.trim().to_string();
        match self.profiles.add_recipe(recipe) {
            Ok(()) => {
                self.save_profiles();
                self.recipe_message =
                    format!("Saved '{}' for this combination", self.recipe_name.trim());
                self.recipe_name.clear();
            }
            Err(error) => self.recipe_message = error,
        }
    }

    pub(super) fn apply_recipe(&mut self, id: &str) {
        let Some(recipe) = self.profiles.recipe(id).cloned() else {
            self.recipe_message = "That recipe no longer exists".to_string();
            return;
        };
        let Some(active) = self.active_operation.clone() else {
            self.recipe_message = "Open Cuts / Layers and select a cut first".to_string();
            return;
        };
        let mut candidate = self.editor.document.clone();
        let Some(operation) = candidate
            .operations
            .iter_mut()
            .find(|operation| operation.id == active)
        else {
            self.recipe_message = "The active cut no longer exists".to_string();
            return;
        };
        recipe.apply_to(operation);
        if let Err(error) = candidate.validate() {
            self.recipe_message = error;
            return;
        }
        self.editor.record_action("Apply recipe");
        self.editor.document = candidate;
        self.recipe_message = format!("Applied '{}' to the active cut", recipe.name);
    }

    pub(super) fn recipes_window(&mut self, ctx: &egui::Context) {
        let mut open = self.recipes_open;
        let mut save = false;
        let mut close_requested = false;
        let mut apply: Option<String> = None;
        let mut delete: Option<String> = None;
        let recipes = self.profiles.recipes.clone();
        let materials: Vec<(String, String)> = self
            .profiles
            .materials
            .iter()
            .map(|material| (material.id.clone(), material.name.clone()))
            .collect();
        let machines: Vec<(String, String)> = self
            .machines
            .machines
            .iter()
            .map(|machine| (machine.id.clone(), machine.name.clone()))
            .collect();
        let material_name = |id: &str| {
            materials
                .iter()
                .find(|(candidate, _)| candidate == id)
                .map_or_else(
                    || "(unknown material)".to_string(),
                    |(_, name)| name.clone(),
                )
        };
        let machine_name = |id: &str| {
            if id.is_empty() {
                "Any machine".to_string()
            } else {
                machines
                    .iter()
                    .find(|(candidate, _)| candidate == id)
                    .map_or_else(|| id.to_string(), |(_, name)| name.clone())
            }
        };
        egui::Window::new("Recipes")
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([640.0, 540.0])
        .show(ctx, |ui| {
            ui.label(
                "A recipe stores the active cut for one material, machine and head. Values are reference starting points: test on your own material.",
            );
            ui.separator();
            ui.heading("Save the active cut");
            egui::Grid::new("recipe_save_grid")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut self.recipe_name);
                    ui.end_row();
                    ui.label("Material");
                    egui::ComboBox::from_id_salt("recipe_save_material")
                        .selected_text(if self.recipe_material.is_empty() {
                            "Choose a material".to_string()
                        } else {
                            material_name(&self.recipe_material)
                        })
                        .show_ui(ui, |ui| {
                            for (id, name) in &materials {
                                ui.selectable_value(
                                    &mut self.recipe_material,
                                    id.clone(),
                                    name,
                                );
                            }
                        });
                    ui.end_row();
                    ui.label("Machine");
                    egui::ComboBox::from_id_salt("recipe_save_machine")
                        .selected_text(machine_name(&self.recipe_machine))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.recipe_machine,
                                String::new(),
                                "Any machine",
                            );
                            for (id, name) in &machines {
                                ui.selectable_value(
                                    &mut self.recipe_machine,
                                    id.clone(),
                                    name,
                                );
                            }
                        });
                    ui.end_row();
                    ui.label("Head");
                    ui.text_edit_singleline(&mut self.recipe_head);
                    ui.end_row();
                });
            if ui
                .button("Save active cut as a recipe")
                .on_hover_text(
                    "Copies the mode, speed, power, passes and the rest from the selected cut",
                )
                .clicked()
            {
                save = true;
            }
            if !self.recipe_message.is_empty() {
                ui.label(self.recipe_message.clone());
            }
            ui.separator();
            ui.heading("Stored recipes");
            ui.horizontal(|ui| {
                ui.label("Search");
                ui.text_edit_singleline(&mut self.recipe_search);
                ui.label("Material");
                egui::ComboBox::from_id_salt("recipe_filter_material")
                    .selected_text(if self.recipe_filter_material.is_empty() {
                        "All".to_string()
                    } else {
                        material_name(&self.recipe_filter_material)
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.recipe_filter_material,
                            String::new(),
                            "All",
                        );
                        for (id, name) in &materials {
                            ui.selectable_value(
                                &mut self.recipe_filter_material,
                                id.clone(),
                                name,
                            );
                        }
                    });
                ui.label("Machine");
                egui::ComboBox::from_id_salt("recipe_filter_machine")
                    .selected_text(machine_name(&self.recipe_filter_machine))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.recipe_filter_machine,
                            String::new(),
                            "All machines",
                        );
                        for (id, name) in &machines {
                            ui.selectable_value(
                                &mut self.recipe_filter_machine,
                                id.clone(),
                                name,
                            );
                        }
                    });
            });
            let query = self.recipe_search.trim().to_lowercase();
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("recipe_list")
                .max_height(240.0)
                .show(ui, |ui| {
                    let mut shown = 0;
                    for recipe in &recipes {
                        if !self.recipe_filter_material.is_empty()
                            && recipe.material_id != self.recipe_filter_material
                        {
                            continue;
                        }
                        if !self.recipe_filter_machine.is_empty()
                            && recipe.machine_id != self.recipe_filter_machine
                        {
                            continue;
                        }
                        let combination = format!(
                            "{} · {} · {}",
                            material_name(&recipe.material_id),
                            machine_name(&recipe.machine_id),
                            if recipe.head.trim().is_empty() {
                                "any head"
                            } else {
                                recipe.head.trim()
                            }
                        );
                        if !query.is_empty()
                            && !recipe.name.to_lowercase().contains(&query)
                            && !combination.to_lowercase().contains(&query)
                        {
                            continue;
                        }
                        shown += 1;
                        ui.horizontal(|ui| {
                            ui.strong(&recipe.name);
                            ui.weak(&combination);
                        });
                        ui.weak(recipe.summary());
                        if !recipe.notes.trim().is_empty() {
                            ui.weak(recipe.notes.clone());
                        }
                        ui.horizontal(|ui| {
                            if ui
                                .button("Apply to active cut")
                                .on_hover_text("Replaces the settings of the selected cut")
                                .clicked()
                            {
                                apply = Some(recipe.id.clone());
                            }
                            if ui
                                .small_button("Delete")
                                .on_hover_text("Remove this recipe")
                                .clicked()
                            {
                                delete = Some(recipe.id.clone());
                            }
                        });
                        ui.separator();
                    }
                    if shown == 0 {
                        ui.weak("No recipe matches.");
                    }
                });
            ui.separator();
            if ui
                .button("Close")
                .on_hover_text("Close this window")
                .clicked()
            {
                close_requested = true;
            }
        });
        if close_requested {
            open = false;
        }
        if save {
            self.save_active_cut_as_recipe();
        }
        if let Some(id) = apply {
            self.apply_recipe(&id);
        }
        if let Some(id) = delete {
            match self.profiles.remove_recipe(&id) {
                Ok(()) => {
                    self.save_profiles();
                    self.recipe_message = "Recipe removed".to_string();
                }
                Err(error) => self.recipe_message = error,
            }
        }
        self.recipes_open = open;
    }
}
