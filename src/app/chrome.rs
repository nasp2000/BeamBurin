use super::{
    AlignMode, BeamBurinApp, Command, DragState, FileMode, Icon, Key, Modifiers, PendingAction,
    Tool, Ui, boolean_icon, format_mm, icons, parse_chord, tool_help, tool_icon,
};

impl BeamBurinApp {
    pub(super) fn shortcuts(&mut self, ui: &Ui) {
        let ctx = ui.ctx();
        if ctx.text_edit_focused()
            || self.dialog.is_some()
            || self.confirm.is_some()
            || self.preferences_open
            || self.machine_wizard_open
            || self.machines_open
            || self.cuts_open
            || self.row_settings.is_some()
            || self.image_adjust.is_some()
            || self.help_shortcuts_open
            || self.assistant_open
            || self.constraints_open
            || self.recipes_open
            || self.workspaces_open
            || self.interface_presets_open
            || self.external_gcode_open
            || self.palette_open
            || self.export_presets_open
            || self.history_open
            || self.tour_open
            || self.licenses_open
        {
            return;
        }
        let keymap = self.preferences.shortcuts.clone();
        let mut pressed = std::collections::HashSet::new();
        ctx.input_mut(|input| {
            for command in Command::ALL {
                if let Ok((modifiers, key)) = parse_chord(keymap.chord(command)) {
                    if input.consume_key(modifiers, key) {
                        pressed.insert(command);
                    }
                }
            }
            // Fixed aliases that stay available whatever the binding is.
            if input.consume_key(Modifiers::NONE, Key::Backspace) {
                pressed.insert(Command::Delete);
            }
            if input.consume_key(Modifiers::NONE, Key::Equals) {
                pressed.insert(Command::ZoomIn);
            }
            if input.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Z) {
                pressed.insert(Command::Redo);
            }
        });
        if pressed.contains(&Command::CommandPalette) {
            self.palette_open = true;
            self.palette_focus = true;
            self.palette_query.clear();
        }
        if pressed.contains(&Command::New) {
            self.request(PendingAction::New);
        }
        if pressed.contains(&Command::Open) {
            self.request(PendingAction::Open);
        }
        if pressed.contains(&Command::SaveAs) {
            self.open_dialog(FileMode::SaveProject);
        } else if pressed.contains(&Command::Save) {
            self.save();
        }
        if pressed.contains(&Command::Undo) {
            self.undo();
        }
        if pressed.contains(&Command::Redo) {
            self.redo();
        }
        if pressed.contains(&Command::SelectAll) {
            self.editor.select_all();
        }
        if pressed.contains(&Command::Cut) {
            self.cut();
        }
        if pressed.contains(&Command::Copy) {
            self.copy();
        }
        if pressed.contains(&Command::Paste) {
            self.paste_from_clipboard();
        }
        if pressed.contains(&Command::Duplicate) {
            self.editor.duplicate_selection();
        }
        if pressed.contains(&Command::Delete) {
            if self.editor.is_editing_nodes() && !self.editor.node_selection.is_empty() {
                let removed = self.editor.delete_selected_nodes();
                self.status = format!("Deleted {removed} node(s)");
            } else {
                self.editor.delete_selection();
            }
        }
        if pressed.contains(&Command::Group) && self.editor.group_selection() {
            self.status = "Objects grouped".to_string();
        }
        if pressed.contains(&Command::Ungroup) && self.editor.ungroup_selection() {
            self.status = "Objects ungrouped".to_string();
        }
        if pressed.contains(&Command::Escape) {
            self.drag = None;
            if self.editor.tool == Tool::Bezier {
                self.pen_nodes.clear();
                self.pen_drag_start = None;
            }
            if self.editor.is_editing_nodes() {
                self.editor.end_node_edit();
            }
            self.editor.selection.clear();
        }
        if pressed.contains(&Command::ToolSelect) {
            self.editor.tool = Tool::Select;
        }
        if pressed.contains(&Command::ToolLine) {
            self.editor.tool = Tool::Line;
        }
        if pressed.contains(&Command::ToolRectangle) {
            self.editor.tool = Tool::Rect;
        }
        if pressed.contains(&Command::ToolEllipse) {
            self.editor.tool = Tool::Ellipse;
        }
        if pressed.contains(&Command::ToolStar) {
            self.editor.tool = Tool::Star;
        }
        if pressed.contains(&Command::ToolText) {
            self.editor.tool = Tool::Text;
        }
        if pressed.contains(&Command::Preview) {
            self.open_preview();
        }
        if pressed.contains(&Command::ToggleLayers) {
            self.show_layers_panel = !self.show_layers_panel;
        }
        if pressed.contains(&Command::ToggleProperties) {
            self.show_properties_panel = !self.show_properties_panel;
        }
        if pressed.contains(&Command::Enter) && self.editor.tool == Tool::Bezier {
            self.finish_pen(false);
        }
        if pressed.contains(&Command::Fit) {
            self.needs_fit = true;
        }
        if pressed.contains(&Command::Grid) {
            self.editor.canvas.grid_visible = !self.editor.canvas.grid_visible;
        }
        if pressed.contains(&Command::PolygonTool) {
            self.editor.tool = Tool::Polygon;
        }
        if pressed.contains(&Command::PencilTool) {
            self.editor.tool = Tool::Pencil;
        }
        if pressed.contains(&Command::ZoomIn) {
            self.editor.zoom_in();
        }
        if pressed.contains(&Command::ZoomOut) {
            self.editor.zoom_out();
        }
        let step = if ctx.input(|input| input.modifiers.shift) {
            0.1
        } else {
            1.0
        };
        let editing_nodes =
            self.editor.is_editing_nodes() && !self.editor.node_selection.is_empty();
        if pressed.contains(&Command::NudgeLeft) {
            if editing_nodes {
                self.editor.nudge_selected_nodes(-step, 0.0);
            } else {
                self.editor.nudge_selection(-step, 0.0);
            }
        }
        if pressed.contains(&Command::NudgeRight) {
            if editing_nodes {
                self.editor.nudge_selected_nodes(step, 0.0);
            } else {
                self.editor.nudge_selection(step, 0.0);
            }
        }
        if pressed.contains(&Command::NudgeUp) {
            if editing_nodes {
                self.editor.nudge_selected_nodes(0.0, step);
            } else {
                self.editor.nudge_selection(0.0, step);
            }
        }
        if pressed.contains(&Command::NudgeDown) {
            if editing_nodes {
                self.editor.nudge_selected_nodes(0.0, -step);
            } else {
                self.editor.nudge_selection(0.0, -step);
            }
        }
    }

    pub(super) fn menu_bar(&mut self, ui: &mut Ui) {
        egui::Panel::top("menu_bar").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.menu_button(icons::menu_label(ui, Icon::File, "File"), |ui| {
                if icons::button(ui, Icon::New, "New").clicked() {
                    self.request(PendingAction::New);
                    ui.close();
                }
                if icons::button(ui, Icon::Open, "Open...").clicked() {
                    self.request(PendingAction::Open);
                    ui.close();
                }
                let recent = self.preferences.general.recent_projects.clone();
                ui.add_enabled_ui(!recent.is_empty(), |ui| {
                    ui.menu_button(icons::menu_label(ui, Icon::Recover, "Recent projects"), |ui| {
                        for entry in &recent {
                            let path = std::path::PathBuf::from(entry);
                            let label = path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or(entry);
                            if ui.button(label).on_hover_text(entry).clicked() {
                                self.start_import(&path);
                                ui.close();
                            }
                        }
                    });
                });
                if icons::button(ui, Icon::Save, "Save").clicked() {
                    self.save();
                    ui.close();
                }
                if icons::button(ui, Icon::Save, "Save as...").clicked() {
                    self.open_dialog(FileMode::SaveProject);
                    ui.close();
                }
                if icons::button(ui, Icon::Save, "Save project with images...").clicked() {
                    self.open_dialog(FileMode::SaveArchive);
                    ui.close();
                }
                ui.separator();
                if icons::button(ui, Icon::Import, "Import SVG or DXF...").clicked() {
                    self.open_dialog(FileMode::ImportFile);
                    ui.close();
                }
                if icons::button(ui, Icon::Toolpaths, "Preview external G-code...").clicked()
                {
                    self.open_dialog(FileMode::PreviewGcode);
                    ui.close();
                }
                if icons::button(ui, Icon::Export, "Export SVG...").clicked() {
                    self.open_dialog(FileMode::ExportSvg);
                    ui.close();
                }
                if icons::button(ui, Icon::Export, "Export DXF...").clicked() {
                    self.open_dialog(FileMode::ExportDxf);
                    ui.close();
                }
                if icons::button(ui, Icon::Export, "Export HPGL...").clicked() {
                    self.open_dialog(FileMode::ExportHpgl);
                    ui.close();
                }
                if icons::button(ui, Icon::Export, "Export EPS...").clicked() {
                    self.open_dialog(FileMode::ExportEps);
                    ui.close();
                }
                if icons::button(ui, Icon::Export, "Export PDF...").clicked() {
                    self.open_dialog(FileMode::ExportPdf);
                    ui.close();
                }
                if icons::button(ui, Icon::Export, "Export image...").clicked() {
                    self.open_dialog(FileMode::ExportImage);
                    ui.close();
                }
                if icons::button(ui, Icon::Export, "Export G-code...").clicked() {
                    self.open_dialog(FileMode::ExportGcode);
                    ui.close();
                }
                ui.menu_button(icons::menu_label(ui, Icon::Export, "Export selection"), |ui| {
                    let enabled = !self.editor.selection.is_empty();
                    if ui
                        .add_enabled(enabled, icons::widget(ui, Icon::Export, "As SVG..."))
                        .clicked()
                    {
                        self.export_subset = true;
                        self.open_dialog(FileMode::ExportSvg);
                        ui.close();
                    }
                    if ui
                        .add_enabled(enabled, icons::widget(ui, Icon::Export, "As DXF..."))
                        .clicked()
                    {
                        self.export_subset = true;
                        self.open_dialog(FileMode::ExportDxf);
                        ui.close();
                    }
                    if ui
                        .add_enabled(enabled, icons::widget(ui, Icon::Export, "As G-code..."))
                        .clicked()
                    {
                        self.export_subset = true;
                        self.open_dialog(FileMode::ExportGcode);
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            enabled,
                            icons::widget(
                                ui,
                                Icon::Export,
                                "As G-code at origin...",
                            ),
                        )
                        .clicked()
                    {
                        self.export_subset = true;
                        self.export_offset_origin = true;
                        self.open_dialog(FileMode::ExportGcode);
                        ui.close();
                    }
                });
                ui.separator();
                if icons::button(ui, Icon::Page, "Document properties...").clicked() {
                    self.open_document_properties();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        self.current_path.is_some()
                            && self.editor.document.objects.iter().any(|object| {
                                matches!(object.shape, crate::document::Shape::Image(_))
                            }),
                        icons::widget(ui, Icon::Image, "Collect images..."),
                    )
                    .clicked()
                {
                    self.collect_images();
                    ui.close();
                }
                if icons::button(ui, Icon::Recover, "Recover project...").clicked() {
                    self.recover_next = true;
                    self.open_dialog(FileMode::OpenProject);
                    ui.close();
                }
                ui.separator();
                if icons::button(ui, Icon::Gear, "Machine profiles...").clicked() {
                    self.machines_open = true;
                    ui.close();
                }
                if icons::button(ui, Icon::Exit, "Exit").clicked() {
                    self.request(PendingAction::Quit);
                    ui.close();
                }
            });
            ui.menu_button(icons::menu_label(ui, Icon::Edit, "Edit"), |ui| {
                if ui
                    .add_enabled(
                        self.editor.history.can_undo(),
                        icons::widget(ui, Icon::Undo, self.undo_label()),
                    )
                    .clicked()
                {
                    self.undo();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        self.editor.history.can_redo(),
                        icons::widget(ui, Icon::Redo, self.redo_label()),
                    )
                    .clicked()
                {
                    self.redo();
                    ui.close();
                }
                ui.separator();
                if icons::button(ui, Icon::SelectAll, "Select all").clicked() {
                    self.editor.select_all();
                    ui.close();
                }
                if icons::button(ui, Icon::SelectAll, "Select open shapes").clicked() {
                    self.editor.select_open_shapes();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        !self.editor.selection.is_empty(),
                        icons::widget(ui, Icon::Cut, "Cut"),
                    )
                    .clicked()
                {
                    self.cut();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        !self.editor.selection.is_empty(),
                        icons::widget(ui, Icon::Copy, "Copy"),
                    )
                    .clicked()
                {
                    self.copy();
                    ui.close();
                }
                if ui
                    .add_enabled(!self.clipboard.is_empty(), icons::widget(ui, Icon::Paste, "Paste"))
                    .clicked()
                {
                    self.paste();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        !self.editor.selection.is_empty(),
                        icons::widget(ui, Icon::Duplicate, "Duplicate"),
                    )
                    .clicked()
                {
                    self.editor.duplicate_selection();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        !self.editor.selection.is_empty(),
                        icons::widget(ui, Icon::Delete, "Delete"),
                    )
                    .clicked()
                {
                    self.editor.delete_selection();
                    ui.close();
                }
                if icons::button(ui, Icon::Invert, "Invert selection").clicked() {
                    let count = self.editor.invert_selection();
                    self.status = format!("Selected {count} object(s)");
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        !self.editor.selection.is_empty(),
                        icons::widget(ui, Icon::Lock, "Lock"),
                    )
                    .clicked()
                {
                    let count = self.editor.lock_selection();
                    self.status = format!("Locked {count} object(s)");
                    ui.close();
                }
                if ui
                    .add_enabled(
                        !self.editor.selection.is_empty(),
                        icons::widget(ui, Icon::Unlock, "Unlock"),
                    )
                    .clicked()
                {
                    let count = self.editor.unlock_selection();
                    self.status = format!("Unlocked {count} object(s)");
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(self.editor.selection.len() >= 2, icons::widget(ui, Icon::Group, "Group"))
                    .clicked()
                {
                    self.editor.group_selection();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        !self.editor.selection.is_empty(),
                        icons::widget(ui, Icon::Ungroup, "Ungroup"),
                    )
                    .clicked()
                {
                    self.editor.ungroup_selection();
                    ui.close();
                }
            });
            ui.menu_button(icons::menu_label(ui, Icon::Draw, "Draw"), |ui| {
                for (tool, icon, label) in [
                    (Tool::Select, Icon::Select, "Selection"),
                    (Tool::Line, Icon::Line, "Line"),
                    (Tool::Rect, Icon::Rectangle, "Rectangle"),
                    (Tool::Ellipse, Icon::Ellipse, "Ellipse"),
                    (Tool::Polygon, Icon::Polygon, "Polygon"),
                    (Tool::Star, Icon::Star, "Star"),
                    (Tool::Text, Icon::Text, "Text"),
                    (Tool::Measure, Icon::Measure, "Measure"),
                    (Tool::Bezier, Icon::Bezier, "Bézier"),
                    (Tool::Pencil, Icon::Freehand, "Freehand"),
                ] {
                    if icons::selectable(ui, self.editor.tool == tool, icon, label).clicked() {
                        self.editor.tool = tool;
                        ui.close();
                    }
                }
            });
            ui.menu_button(icons::menu_label(ui, Icon::Operations, "Operations"), |ui| {
                if icons::button(ui, Icon::Intersections, "Registration marks...").clicked() {
                    self.regmarks_open = true;
                    ui.close();
                }
                if icons::button(ui, Icon::GridArray, "Material test...").clicked() {
                    self.material_test_open = true;
                    ui.close();
                }
                if icons::button(ui, Icon::Layers, "Cuts / Layers...").clicked() {
                    self.open_cuts();
                    ui.close();
                }
                if icons::button(ui, Icon::Gear, "Parameters...").clicked() {
                    self.open_parameters();
                    ui.close();
                }
                if icons::button(ui, Icon::Join, "Constraints...").clicked() {
                    self.open_constraints();
                    ui.close();
                }
                if icons::button(ui, Icon::Plus, "New operation").clicked() {
                    let id = self.editor.add_operation(
                        format!(
                            "Operation {}",
                            self.editor.document.operations.len() + 1
                        ),
                        crate::operations::OperationType::OutlineCutting,
                    );
                    self.active_operation = Some(id);
                    ui.close();
                }
                if icons::button(ui, Icon::Compare, "Compare operations...").clicked() {
                    self.operations_compare_open = true;
                    ui.close();
                }
                if icons::button(ui, Icon::Play, "Preview and G-code...").clicked() {
                    self.open_preview();
                    ui.close();
                }
                if icons::button(ui, Icon::Clock, "Estimate duration").clicked() {
                    match crate::gcode::render_job(
                        &self.editor.document,
                        &crate::gcode::JobOptions::default(),
                    ) {
                        Ok((_, report)) => {
                            self.status = format!(
                                "Paths: {} - {:.1} s estimated",
                                report.paths, report.seconds
                            );
                        }
                        Err(error) => self.status = format!("Estimate failed: {error}"),
                    }
                    ui.close();
                }
                if ui
                    .add_enabled(
                        self.active_operation.is_some(),
                        icons::widget(ui, Icon::Duplicate, "Duplicate operation"),
                    )
                    .clicked()
                {
                    if let Some(operation) = self.active_operation.clone() {
                        if let Some(copy) = self.editor.duplicate_operation(&operation) {
                            self.active_operation = Some(copy);
                            self.status = "Operation duplicated".to_string();
                        }
                    }
                    ui.close();
                }
                if ui
                    .add_enabled(
                        self.active_operation.is_some(),
                        icons::widget(ui, Icon::Layers, "Assign operation to layer"),
                    )
                    .clicked()
                {
                    if let Some(operation) = self.active_operation.clone() {
                        let layer = self.editor.active_layer_id.clone();
                        let changed = self
                            .editor
                            .assign_operation_to_layer(&layer, Some(&operation));
                        self.status =
                            format!("Assigned operation to {changed} layer object(s)");
                    }
                    ui.close();
                }
            });
            ui.menu_button(icons::menu_label(ui, Icon::Materials, "Materials and profiles"), |ui| {
                if icons::button(ui, Icon::Materials, "Materials and operations...").clicked() {
                    self.profiles_open = true;
                    ui.close();
                }
                if icons::button(ui, Icon::Materials, "Recipes...").clicked() {
                    self.recipes_open = true;
                    ui.close();
                }
                if icons::button(ui, Icon::Import, "Import profiles...").clicked() {
                    self.open_dialog(FileMode::ImportProfiles);
                    ui.close();
                }
                if icons::button(ui, Icon::Export, "Export profiles...").clicked() {
                    self.open_dialog(FileMode::ExportProfiles);
                    ui.close();
                }
            });
            ui.menu_button(icons::menu_label(ui, Icon::Arrange, "Arrange"), |ui| {
                let enabled = !self.editor.selection.is_empty();
                if ui
                    .add_enabled(enabled, icons::widget(ui, Icon::ToFront, "Bring to front"))
                    .clicked()
                {
                    let count = self.editor.bring_selection_to_front();
                    self.status = format!("Brought {count} object(s) to the front");
                    ui.close();
                }
                if ui
                    .add_enabled(enabled, icons::widget(ui, Icon::ToBack, "Send to back"))
                    .clicked()
                {
                    let count = self.editor.send_selection_to_back();
                    self.status = format!("Sent {count} object(s) to the back");
                    ui.close();
                }
                if ui
                    .add_enabled(enabled, icons::widget(ui, Icon::ToFront, "Bring forward"))
                    .clicked()
                {
                    let count = self.editor.raise_selection();
                    self.status = format!("Brought {count} object(s) forward");
                    ui.close();
                }
                if ui
                    .add_enabled(enabled, icons::widget(ui, Icon::ToBack, "Send backward"))
                    .clicked()
                {
                    let count = self.editor.lower_selection();
                    self.status = format!("Sent {count} object(s) backward");
                    ui.close();
                }
                if ui
                    .add_enabled(enabled, icons::widget(ui, Icon::Center, "Centre on page"))
                    .clicked()
                {
                    self.editor.center_selection_on_page();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        enabled,
                        icons::widget(ui, Icon::TransformCenter, "Move to position..."),
                    )
                    .clicked()
                {
                    self.open_position();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        self.editor.selection.len() >= 2,
                        icons::widget(ui, Icon::DistributeH, "Distribute on page horizontally"),
                    )
                    .clicked()
                {
                    self.editor.distribute_selection_on_page(true);
                    ui.close();
                }
                if ui
                    .add_enabled(
                        self.editor.selection.len() >= 2,
                        icons::widget(ui, Icon::DistributeV, "Distribute on page vertically"),
                    )
                    .clicked()
                {
                    self.editor.distribute_selection_on_page(false);
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(enabled, icons::widget(ui, Icon::RotateLeft, "Rotate 90° left"))
                    .clicked()
                {
                    self.editor.rotate_selection(std::f64::consts::FRAC_PI_2);
                    ui.close();
                }
                if ui
                    .add_enabled(enabled, icons::widget(ui, Icon::RotateRight, "Rotate 90° right"))
                    .clicked()
                {
                    self.editor.rotate_selection(-std::f64::consts::FRAC_PI_2);
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(enabled, icons::widget(ui, Icon::FlipH, "Flip horizontal"))
                    .clicked()
                {
                    self.editor.flip_selection(true);
                    ui.close();
                }
                if ui
                    .add_enabled(enabled, icons::widget(ui, Icon::FlipV, "Flip vertical"))
                    .clicked()
                {
                    self.editor.flip_selection(false);
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(enabled, icons::widget(ui, Icon::ScaleUp, "Scale up 10 %"))
                    .clicked()
                {
                    self.editor.scale_selection(1.1, 1.1);
                    ui.close();
                }
                if ui
                    .add_enabled(enabled, icons::widget(ui, Icon::ScaleDown, "Scale down 10 %"))
                    .clicked()
                {
                    self.editor.scale_selection(1.0 / 1.1, 1.0 / 1.1);
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        self.editor.selection.len() == 1,
                        icons::widget(ui, Icon::TextOutline, "Text to outlines"),
                    )
                    .clicked()
                {
                    self.apply_text_outlines();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        self.editor.selection.len() == 2,
                        icons::widget(ui, Icon::TextOnPath, "Fit text to path"),
                    )
                    .clicked()
                {
                    self.apply_text_on_path();
                    ui.close();
                }
                ui.menu_button(icons::menu_label(ui, Icon::TransformCenter, "Transform around"), |ui| {
                    use crate::editor::state::TransformCenter;
                    for (mode, label) in [
                        (TransformCenter::Selection, "Selection centre"),
                        (TransformCenter::Page, "Page centre"),
                        (TransformCenter::Object, "Each object's centre"),
                    ] {
                        icons::selectable_value(
                            ui,
                            &mut self.editor.transform_center,
                            mode,
                            Icon::TransformCenter,
                            label,
                        );
                    }
                });
                if ui
                    .add_enabled(enabled, icons::widget(ui, Icon::Recover, "Reset transforms"))
                    .clicked()
                {
                    let count = self.editor.reset_transforms();
                    self.status = format!("Reset {count} object(s)");
                    ui.close();
                }
                if ui
                    .add_enabled(enabled, icons::widget(ui, Icon::Skew, "Skew..."))
                    .clicked()
                {
                    self.skew_open = true;
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(enabled, icons::widget(ui, Icon::GridArray, "Grid array..."))
                    .clicked()
                {
                    self.grid_array_open = true;
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        self.editor.selection.len() >= 2,
                        icons::widget(ui, Icon::Join, "Join paths"),
                    )
                    .clicked()
                {
                    self.apply_join_paths();
                    ui.close();
                }
                if ui
                    .add_enabled(enabled, icons::widget(ui, Icon::ClosePath, "Close paths"))
                    .clicked()
                {
                    self.apply_close_paths();
                    ui.close();
                }
                ui.separator();
                if self.editor.is_editing_nodes() {
                    if icons::button(ui, Icon::Nodes, "Exit node editing").clicked() {
                        self.editor.end_node_edit();
                        ui.close();
                    }
                    if icons::button(ui, Icon::Simplify, "Simplify path...").clicked() {
                        self.simplify_open = true;
                        ui.close();
                    }
                } else if ui
                    .add_enabled(
                        self.editor.selection.len() == 1,
                        icons::widget(ui, Icon::Nodes, "Edit nodes"),
                    )
                    .clicked()
                {
                    if let Some(id) = self.editor.selection.ids().first().cloned() {
                        match self.editor.begin_node_edit(&id) {
                            Ok(()) => {
                                self.status = "Editing nodes: drag to move, double-click a segment to insert"
                                    .to_string();
                            }
                            Err(error) => {
                                self.status = format!("Cannot edit nodes: {error}");
                            }
                        }
                    }
                    ui.close();
                }
                if ui
                    .add_enabled(
                        !self.editor.selection.is_empty(),
                        icons::widget(ui, Icon::Repeat, "Repeat..."),
                    )
                    .clicked()
                {
                    self.repeat_open = true;
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        !self.editor.selection.is_empty(),
                        icons::widget(ui, Icon::Nest, "Nest selection..."),
                    )
                    .clicked()
                {
                    self.open_nest();
                    ui.close();
                }
                ui.separator();
                if icons::button(ui, Icon::FindDuplicates, "Find duplicates...").clicked() {
                    self.report_duplicates();
                    ui.close();
                }
                if icons::button(ui, Icon::Intersections, "Find intersections...").clicked() {
                    self.report_intersections();
                    ui.close();
                }
                ui.separator();
                let can_combine = self.editor.selection.len() == 2;
                ui.menu_button(icons::menu_label(ui, Icon::Boolean, "Boolean"), |ui| {
                    use crate::geometry::BooleanOp;
                    for op in BooleanOp::all() {
                        if ui
                            .add_enabled(can_combine, icons::widget(ui, boolean_icon(op), op.label()))
                            .clicked()
                        {
                            self.apply_boolean(op);
                            ui.close();
                        }
                    }
                });
                if ui
                    .add_enabled(enabled, icons::widget(ui, Icon::OutlineStroke, "Outline stroke"))
                    .clicked()
                {
                    self.apply_outline_stroke();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        self.editor.selection.len() >= 2,
                        icons::widget(ui, Icon::Trim, "Trim segments"),
                    )
                    .clicked()
                {
                    self.apply_trim();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        self.editor.selection.len() >= 2,
                        icons::widget(ui, Icon::Trim, "Delete overlapping segments"),
                    )
                    .clicked()
                {
                    self.apply_delete_overlaps();
                    ui.close();
                }
                if ui
                    .add_enabled(enabled, icons::widget(ui, Icon::Offset, "Offset..."))
                    .clicked()
                {
                    self.offset_open = true;
                    ui.close();
                }
            });
            ui.menu_button(icons::menu_label(ui, Icon::View, "View"), |ui| {
                if icons::button(ui, Icon::ZoomIn, "Zoom in").clicked() {
                    self.editor.zoom_in();
                    ui.close();
                }
                if icons::button(ui, Icon::ZoomOut, "Zoom out").clicked() {
                    self.editor.zoom_out();
                    ui.close();
                }
                if icons::button(ui, Icon::ZoomFit, "Fit page").clicked() {
                    self.needs_fit = true;
                    ui.close();
                }
                if ui
                    .add_enabled(
                        !self.editor.selection.is_empty(),
                        icons::widget(ui, Icon::ZoomSelection, "Zoom to selection"),
                    )
                    .clicked()
                {
                    self.zoom_to_selection_request = true;
                    ui.close();
                }
                ui.separator();
                icons::checkbox(ui, &mut self.editor.canvas.grid_visible, Icon::Grid, "Show grid");
                if icons::checkbox(
                    ui,
                    &mut self.editor.show_selection,
                    Icon::Eye,
                    "Show selection",
                )
                .changed()
                {
                    self.preferences.drawing.show_selection = self.editor.show_selection;
                }
                icons::checkbox(ui, &mut self.editor.show_rulers, Icon::Rulers, "Show rulers");
                icons::checkbox(ui, &mut self.editor.guides_visible, Icon::Guides, "Show guides");
                icons::checkbox(ui, &mut self.editor.snap_guides, Icon::Snap, "Snap to guides");
                if icons::button(ui, Icon::Guides, "Guides...").clicked() {
                    self.guides_open = true;
                    ui.close();
                }
                ui.menu_button(icons::menu_label(ui, Icon::Snap, "Snapping"), |ui| {
                    icons::checkbox(ui, &mut self.editor.snap_enabled, Icon::Grid, "Grid");
                    icons::checkbox(ui, &mut self.editor.snap_nodes, Icon::Nodes, "Nodes");
                    icons::checkbox(ui, &mut self.editor.snap_endpoints, Icon::Nodes, "Endpoints");
                    icons::checkbox(ui, &mut self.editor.snap_centers, Icon::SnapCenter, "Centres");
                    icons::checkbox(ui, &mut self.editor.snap_midpoints, Icon::SnapMidpoint, "Midpoints");
                    icons::checkbox(ui, &mut self.editor.snap_intersections, Icon::Intersections, "Intersections");
                    icons::checkbox(ui, &mut self.editor.snap_limits, Icon::SnapLimits, "Object limits");
                    ui.add(
                        egui::DragValue::new(&mut self.editor.snap_step_mm)
                            .speed(0.1)
                            .range(0.1..=100.0)
                            .suffix(" mm grid"),
                    );
                });
                icons::checkbox(ui, &mut self.preview_toolpaths, Icon::Toolpaths, "Show toolpaths");
                icons::checkbox(ui, &mut self.show_layers_panel, Icon::Panels, "Layers panel (F9)");
                icons::checkbox(ui, &mut self.show_properties_panel, Icon::Panels, "Properties panel (F10)");
                if icons::button(ui, Icon::Panels, "Workspaces...").clicked() {
                    self.workspaces_open = true;
                    ui.close();
                }
                if icons::button(ui, Icon::Clock, "History...").clicked() {
                    self.history_open = true;
                    ui.close();
                }
                if icons::button(ui, Icon::Palette, "Interface presets...").clicked() {
                    self.interface_presets_open = true;
                    ui.close();
                }
                if icons::button(ui, Icon::Search, "Command palette...").clicked() {
                    self.palette_open = true;
                    self.palette_focus = true;
                    self.palette_query.clear();
                    ui.close();
                }
                icons::checkbox(ui, &mut self.dark_mode, Icon::DarkMode, "Dark mode");
                ui.separator();
                if icons::button(ui, Icon::Preferences, "Preferences...").clicked() {
                    self.preferences_open = true;
                    ui.close();
                }
            });
            ui.menu_button(icons::menu_label(ui, Icon::Help, "Help"), |ui| {
                if icons::button(ui, Icon::Text, "Keyboard shortcuts").clicked() {
                    self.help_shortcuts_open = true;
                    ui.close();
                }
                if icons::button(ui, Icon::Info, "Show the tour").clicked() {
                    self.tour_open = true;
                    self.tour_step = 0;
                    ui.close();
                }
                if icons::button(ui, Icon::Info, "Assistant for agents...").clicked() {
                    self.assistant_open = true;
                    ui.close();
                }
                if icons::button(ui, Icon::Info, "Licenses...").clicked() {
                    self.licenses_open = true;
                    ui.close();
                }
                if icons::button(ui, Icon::Info, "About BeamBurin").clicked() {
                    self.about_open = true;
                    ui.close();
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(self.title());
            });
        });
    });
    }

    pub(super) fn tool_strip(&mut self, ui: &mut Ui) {
        egui::Panel::left("tool_strip")
        .resizable(false)
        .exact_size(56.0)
        .show(ui, |ui| {
            // A tighter vertical rhythm keeps every tool, boolean and mode
            // visible at 1280x800 without a scrollbar.
            ui.spacing_mut().item_spacing.y = 2.0;
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    for tool in [
                        Tool::Select,
                        Tool::Pencil,
                        Tool::Rect,
                        Tool::Ellipse,
                        Tool::Polygon,
                        Tool::Star,
                        Tool::Line,
                        Tool::Bezier,
                        Tool::Text,
                        Tool::Measure,
                        Tool::Pan,
                    ] {
                        if icons::icon_button(
                            ui,
                            self.editor.tool == tool,
                            tool_icon(tool),
                            22.0,
                            tool.label(),
                        )
                        .on_hover_text(tool_help(tool))
                        .clicked()
                        {
                            self.editor.tool = tool;
                        }
                    }
                    ui.separator();
                    let has_selection = !self.editor.selection.is_empty();
                    if icons::icon_button_enabled(
                        ui,
                        has_selection,
                        Icon::Duplicate,
                        18.0,
                        "Duplicate",
                    )
                    .on_hover_text("Duplicate the selection")
                    .clicked()
                    {
                        self.editor.duplicate_selection();
                    }
                    if icons::icon_button_enabled(
                        ui,
                        has_selection,
                        Icon::Delete,
                        18.0,
                        "Delete",
                    )
                    .on_hover_text("Delete the selection")
                    .clicked()
                    {
                        self.editor.delete_selection();
                    }
                    ui.separator();
                    let two_selected = self.editor.selection.len() == 2;
                    for op in crate::geometry::BooleanOp::all() {
                        if icons::icon_button_enabled(
                            ui,
                            two_selected,
                            boolean_icon(op),
                            18.0,
                            op.label(),
                        )
                        .on_hover_text(format!("{} the two selected objects", op.label()))
                        .clicked()
                        {
                            self.apply_boolean(op);
                        }
                    }
                    ui.separator();
                    if icons::icon_button_enabled(
                        ui,
                        has_selection,
                        Icon::Nodes,
                        18.0,
                        "Edit nodes",
                    )
                    .on_hover_text("Edit the nodes of one path")
                    .clicked()
                    {
                        if let Some(id) = self.editor.selection.ids().first().cloned() {
                            match self.editor.begin_node_edit(&id) {
                                Ok(()) => {
                                    self.status =
                                        "Editing nodes: drag to move, double-click a segment to insert"
                                            .to_string();
                                }
                                Err(error) => {
                                    self.status = format!("Cannot edit nodes: {error}");
                                }
                            }
                        }
                    }
                    if icons::icon_button_enabled(
                        ui,
                        has_selection,
                        Icon::GridArray,
                        18.0,
                        "Grid array",
                    )
                    .on_hover_text("Copy the selection in a grid")
                    .clicked()
                    {
                        self.grid_array_open = true;
                    }
                    if icons::icon_button_enabled(
                        ui,
                        has_selection,
                        Icon::Repeat,
                        18.0,
                        "Circular array",
                    )
                    .on_hover_text("Copy the selection around a circle")
                    .clicked()
                    {
                        self.repeat_open = true;
                    }
                    if icons::icon_button_enabled(
                        ui,
                        has_selection,
                        Icon::Offset,
                        18.0,
                        "Offset",
                    )
                    .on_hover_text("Offset the selection by the radius")
                    .clicked()
                    {
                        self.offset_open = true;
                    }
                    if icons::icon_button_enabled(
                        ui,
                        has_selection,
                        Icon::OutlineStroke,
                        18.0,
                        "Outline stroke",
                    )
                    .on_hover_text("Convert the stroke into an outline")
                    .clicked()
                    {
                        self.apply_outline_stroke();
                    }
                    ui.separator();
                    ui.label("Radius");
                    ui.add(
                        egui::DragValue::new(&mut self.offset_distance)
                            .speed(0.1)
                            .range(0.1..=50.0)
                            .suffix(" mm"),
                    );
                });
            });
        });
    }

    pub(super) fn tool_options(&mut self, ui: &mut Ui) {
        if self.editor.tool == Tool::Polygon {
            ui.add(
                egui::DragValue::new(&mut self.polygon_sides)
                    .speed(1.0)
                    .range(3..=64),
            );
            ui.label("sides");
        }
        if matches!(self.editor.tool, Tool::Rect | Tool::Ellipse) {
            icons::checkbox_at(ui, &mut self.exact_size, Icon::Measure, "Exact size", 18.0);
            if self.exact_size {
                ui.add(
                    egui::DragValue::new(&mut self.exact_width)
                        .speed(0.5)
                        .range(0.01..=10000.0)
                        .suffix(" mm W"),
                );
                ui.add(
                    egui::DragValue::new(&mut self.exact_height)
                        .speed(0.5)
                        .range(0.01..=10000.0)
                        .suffix(" mm H"),
                );
            }
        }
        if matches!(self.editor.tool, Tool::Rect | Tool::Ellipse | Tool::Line) {
            ui.weak("Shift: square/circle or 45° steps");
        }
        if self.editor.tool == Tool::Line {
            ui.checkbox(&mut self.line_angle_snap, "Snap 45°")
                .on_hover_text("Constrain every line to 45° steps without holding Shift");
        }
        if self.editor.tool == Tool::Text {
            ui.add(
                egui::DragValue::new(&mut self.text_size_mm)
                    .speed(0.5)
                    .range(1.0..=500.0)
                    .suffix(" mm"),
            )
            .on_hover_text("Font size of new text objects");
            ui.label("text size");
        }
        match self.editor.tool {
            Tool::Bezier => {
                ui.weak("Click to add nodes; Enter finishes; Escape cancels");
            }
            Tool::Measure => {
                ui.weak("Drag to measure; Shift constrains the angle");
            }
            Tool::Pencil => {
                ui.weak("Drag to draw; Arrange → Simplify path smooths it");
            }
            _ => {}
        }
        if self.editor.tool == Tool::Star {
            ui.add(
                egui::DragValue::new(&mut self.star_points)
                    .speed(1.0)
                    .range(3..=64),
            );
            ui.label("tips");
            ui.add(
                egui::Slider::new(&mut self.star_inner, 0.05..=1.0)
                    .text("inner")
                    .fixed_decimals(2),
            );
        }
    }

    pub(super) fn toolbar(&mut self, ui: &mut Ui) {
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        self.editor.history.can_undo(),
                        icons::widget_at(ui, Icon::Undo, self.undo_label(), 20.0),
                    )
                    .clicked()
                {
                    self.undo();
                }
                if ui
                    .add_enabled(
                        self.editor.history.can_redo(),
                        icons::widget_at(ui, Icon::Redo, self.redo_label(), 20.0),
                    )
                    .clicked()
                {
                    self.redo();
                }

                self.tool_options(ui);
                ui.separator();
                if ui.button("-").clicked() {
                    self.editor.zoom_out();
                }
                ui.label(format!("{:.0}%", self.editor.canvas.zoom * 100.0));
                if ui.button("+").clicked() {
                    self.editor.zoom_in();
                }
                if ui.button("Fit").clicked() {
                    self.needs_fit = true;
                }
                ui.separator();
                ui.checkbox(&mut self.editor.canvas.grid_visible, "Grid");
                ui.checkbox(&mut self.editor.snap_enabled, "Snap");
                ui.checkbox(&mut self.preview_toolpaths, "Toolpaths");
                ui.separator();
                if icons::button(ui, Icon::Open, "Open").clicked() {
                    self.request(PendingAction::Open);
                }
                if icons::button(ui, Icon::Save, "Save").clicked() {
                    self.save();
                }
                ui.separator();
                let has_selection = !self.editor.selection.is_empty();
                for (mode, icon, label) in [
                    (AlignMode::Left, Icon::AlignLeft, "Align left"),
                    (
                        AlignMode::CenterH,
                        Icon::AlignCenterH,
                        "Align centre horizontally",
                    ),
                    (AlignMode::Right, Icon::AlignRight, "Align right"),
                    (AlignMode::Top, Icon::AlignTop, "Align top"),
                    (
                        AlignMode::CenterV,
                        Icon::AlignCenterV,
                        "Align centre vertically",
                    ),
                    (AlignMode::Bottom, Icon::AlignBottom, "Align bottom"),
                ] {
                    if icons::icon_button_enabled(ui, has_selection, icon, 16.0, label)
                        .on_hover_text(label)
                        .clicked()
                    {
                        self.editor.align_selection(mode);
                    }
                }
                ui.separator();
                let can_distribute = self.editor.selection.len() >= 3;
                if icons::icon_button_enabled(
                    ui,
                    can_distribute,
                    Icon::DistributeH,
                    16.0,
                    "Distribute horizontally",
                )
                .on_hover_text("Distribute horizontally")
                .clicked()
                {
                    self.editor.distribute_selection(true);
                }
                if icons::icon_button_enabled(
                    ui,
                    can_distribute,
                    Icon::DistributeV,
                    16.0,
                    "Distribute vertically",
                )
                .on_hover_text("Distribute vertically")
                .clicked()
                {
                    self.editor.distribute_selection(false);
                }
            });
        });
    }

    /// The fixed tool panel: options, live readout and the pending path.
    /// The fixed tool panel: options, live readout and the pending path.
    pub(super) fn tool_contents(&mut self, ui: &mut Ui) {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 3.0);
        ui.label(tool_help(self.editor.tool));
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("tool_contents_scroll")
            .show(ui, |ui| {
                self.tool_options(ui);
                ui.separator();
                ui.strong("In progress");
                let drag = self.drag.clone();
                match drag {
                    Some(DragState::Draw {
                        tool, start, end, ..
                    }) => {
                        ui.monospace(tool.label());
                        ui.monospace(format!("from {:.2}, {:.2}", start.x, start.y));
                        ui.monospace(format!("to   {:.2}, {:.2}", end.x, end.y));
                        ui.weak(format!(
                            "{:.2} x {:.2} mm",
                            (end.x - start.x).abs(),
                            (end.y - start.y).abs()
                        ));
                        if ui
                            .button("Cancel drawing")
                            .on_hover_text("Discard the shape being dragged")
                            .clicked()
                        {
                            self.drag = None;
                            self.status = "Drawing cancelled".to_string();
                        }
                    }
                    Some(DragState::Marquee { start, end, .. }) => {
                        ui.monospace(format!(
                            "Marquee {:.2} x {:.2} mm",
                            (end.x - start.x).abs(),
                            (end.y - start.y).abs()
                        ));
                    }
                    Some(DragState::Sketch { points }) => {
                        ui.monospace(format!("{} freehand point(s)", points.len()));
                    }
                    Some(DragState::Move { .. } | DragState::NodeMove { .. }) => {
                        ui.weak("Dragging; release to apply.");
                    }
                    Some(_) => {
                        ui.weak("Transforming; release to apply.");
                    }
                    None => {
                        ui.weak("Nothing in progress.");
                    }
                }
                if self.editor.tool == Tool::Bezier {
                    ui.add_space(4.0);
                    ui.monospace(format!("Bézier node(s): {}", self.pen_nodes.len()));
                    ui.horizontal(|ui| {
                        let can_apply = self.pen_nodes.len() >= 2;
                        if ui
                            .add_enabled(can_apply, egui::Button::new("Apply path"))
                            .on_hover_text("Finish the path with the nodes placed so far")
                            .clicked()
                        {
                            self.finish_pen(false);
                        }
                        if ui
                            .add_enabled(
                                !self.pen_nodes.is_empty(),
                                egui::Button::new("Cancel path"),
                            )
                            .on_hover_text("Discard the nodes without creating an object")
                            .clicked()
                        {
                            self.pen_nodes.clear();
                            self.status = "Bézier path cancelled".to_string();
                        }
                    });
                }
                ui.separator();
                ui.strong("Selection");
                if let Some((min, max)) = self.editor.selected_bounds() {
                    ui.monospace(format!("{} object(s)", self.editor.selection.len()));
                    ui.weak(format!("from {:.2}, {:.2}", min.x, min.y));
                    ui.weak(format!("{:.2} x {:.2} mm", max.x - min.x, max.y - min.y));
                } else {
                    ui.weak("No selection.");
                }
            });
    }

    pub(super) fn status_bar(&mut self, ui: &mut Ui) {
        egui::Panel::bottom("status_bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Tool: {}", self.editor.tool.label()));
                ui.separator();
                if let Some(point) = self.cursor_doc {
                    ui.label(format!(
                        "X {}  Y {} mm",
                        format_mm(point.x),
                        format_mm(point.y)
                    ));
                } else {
                    ui.label("X --  Y -- mm");
                }
                ui.separator();
                ui.label(format!("Zoom {:.0}%", self.editor.canvas.zoom * 100.0));
                ui.separator();
                ui.label(format!(
                    "{} x {} mm",
                    format_mm(self.editor.document.width_mm),
                    format_mm(self.editor.document.height_mm)
                ));
                ui.separator();
                ui.label(format!(
                    "{} selected / {} objects",
                    self.editor.selection.len(),
                    self.editor.document.objects.len()
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.status.is_empty() {
                        ui.label(&self.status);
                    }
                });
            });
        });
    }
}
