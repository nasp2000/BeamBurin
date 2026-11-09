#![allow(clippy::wildcard_imports)]

use super::*;

impl BeamBurinApp {
    pub(super) fn tour_window(&mut self, ctx: &egui::Context) {
        let steps = tour_steps();
        let mut open = self.tour_open;
        let mut step = self.tour_step.min(steps.len() - 1);
        let mut finish = false;
        let mut close_requested = false;
        egui::Window::new("Welcome to BeamBurin")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                let (title, body) = steps[step];
                ui.heading(title);
                ui.add_space(4.0);
                ui.label(body);
                ui.add_space(8.0);
                ui.weak(format!("Step {} of {}", step + 1, steps.len()));
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(step > 0, egui::Button::new("Back"))
                        .clicked()
                    {
                        step -= 1;
                    }
                    let last = step + 1 == steps.len();
                    if ui.button(if last { "Finish" } else { "Next" }).clicked() {
                        if last {
                            finish = true;
                        } else {
                            step += 1;
                        }
                    }
                    if ui.button("Skip").clicked() {
                        close_requested = true;
                    }
                    ui.weak("Help → Show the tour reopens this.");
                });
            });
        if finish || close_requested {
            open = false;
            self.preferences.general.tour_seen = true;
            if let Err(error) = self.preferences.save(&self.preferences_path) {
                self.status = format!("Cannot save preferences: {error}");
            }
        }
        self.tour_step = step;
        self.tour_open = open;
    }

    pub(super) fn history_window(&mut self, ctx: &egui::Context) {
        let mut open = self.history_open;
        let labels = self.editor.history.labels();
        let redo = self.editor.history.redo_labels();
        let mut jump: Option<usize> = None;
        let mut redo_once = false;
        egui::Window::new("History")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([360.0, 380.0])
            .max_height(460.0)
            .show(ctx, |ui| {
                ui.label(
                    "Click a step to return to the state just before it; that step \
                 and every later one move to redo.",
                );
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("history_steps")
                    .max_height(320.0)
                    .show(ui, |ui| {
                        if labels.is_empty() {
                            ui.weak("No undo steps yet.");
                        }
                        for (index, label) in labels.iter().enumerate() {
                            if ui
                                .add(
                                    egui::Button::new(label)
                                        .frame(false)
                                        .min_size(egui::vec2(ui.available_width(), 0.0)),
                                )
                                .clicked()
                            {
                                jump = Some(labels.len() - index);
                            }
                        }
                        ui.separator();
                        ui.strong("Current state");
                        for label in &redo {
                            if ui
                                .add(
                                    egui::Button::new(format!("Redo {label}"))
                                        .frame(false)
                                        .min_size(egui::vec2(ui.available_width(), 0.0)),
                                )
                                .clicked()
                            {
                                redo_once = true;
                            }
                        }
                    });
            });
        if let Some(steps) = jump {
            if let Some(action) = self.editor.jump_history(steps) {
                self.status = format!("Reverted to before {action}");
                self.needs_fit = true;
            }
        }
        if redo_once {
            if let Some(action) = self.editor.redo() {
                self.status = format!("Redo {action}");
            }
        }
        self.history_open = open;
    }

    pub(super) fn palette_window(&mut self, ctx: &egui::Context) {
        let mut open = self.palette_open;
        let mut chosen: Option<PaletteTarget> = None;
        let mut close_requested = false;
        egui::Window::new("Command palette")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.palette_query)
                        .hint_text("Type a command, object, layer, operation or parameter")
                        .desired_width(f32::INFINITY),
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Command search")
                });
                if self.palette_focus {
                    response.request_focus();
                    self.palette_focus = false;
                }
                let entries = crate::commands::ALL;
                let matches = filter_palette(entries, &self.palette_query);
                if ui.input(|input| input.key_pressed(Key::Enter)) {
                    if let Some(index) = matches.first() {
                        chosen = Some(PaletteTarget::Command(entries[*index].id));
                    }
                }
                if ui.input(|input| input.key_pressed(Key::Escape))
                    || ctx.input_mut(|input| input.consume_key(Modifiers::CTRL, Key::K))
                {
                    close_requested = true;
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("palette_results")
                    .max_height(340.0)
                    .show(ui, |ui| {
                        if matches.is_empty() {
                            ui.weak("No command matches.");
                        }
                        for index in &matches {
                            let command = &entries[*index];
                            let text =
                                format!("{}  ·  {}", command.category.label(), command.label);
                            if ui
                                .add(
                                    egui::Button::new(text)
                                        .frame(false)
                                        .min_size(egui::vec2(ui.available_width(), 0.0)),
                                )
                                .on_hover_text(command.summary)
                                .clicked()
                            {
                                chosen = Some(PaletteTarget::Command(command.id));
                            }
                        }
                        if !self.palette_query.trim().is_empty() {
                            let query = self.palette_query.clone();
                            ui.separator();
                            ui.weak("Objects");
                            let objects = filter_objects(&self.editor.document, &query);
                            if objects.is_empty() {
                                ui.weak("No object matches.");
                            }
                            for object in objects {
                                let text = format!(
                                    "Object  ·  {} ({})",
                                    object.name,
                                    object.shape.kind_name()
                                );
                                if ui
                                    .add(
                                        egui::Button::new(text)
                                            .frame(false)
                                            .min_size(egui::vec2(ui.available_width(), 0.0)),
                                    )
                                    .on_hover_text(format!("Select {}", object.id))
                                    .clicked()
                                {
                                    chosen = Some(PaletteTarget::Object(object.id.clone()));
                                }
                            }
                            ui.weak("Layers");
                            let layers =
                                filter_named(self.editor.document.layers.iter(), &query, |layer| {
                                    layer.name.as_str()
                                });
                            if layers.is_empty() {
                                ui.weak("No layer matches.");
                            }
                            for layer in layers {
                                let text = format!("Layer  ·  {}", layer.name);
                                if ui
                                    .add(
                                        egui::Button::new(text)
                                            .frame(false)
                                            .min_size(egui::vec2(ui.available_width(), 0.0)),
                                    )
                                    .on_hover_text("Make this the active layer")
                                    .clicked()
                                {
                                    chosen = Some(PaletteTarget::Layer(layer.id.clone()));
                                }
                            }
                            ui.weak("Operations");
                            let operations = filter_named(
                                self.editor.document.operations.iter(),
                                &query,
                                |operation| operation.name.as_str(),
                            );
                            if operations.is_empty() {
                                ui.weak("No operation matches.");
                            }
                            for operation in operations {
                                let text = format!("Operation  ·  {}", operation.name);
                                if ui
                                    .add(
                                        egui::Button::new(text)
                                            .frame(false)
                                            .min_size(egui::vec2(ui.available_width(), 0.0)),
                                    )
                                    .on_hover_text("Open this operation in Cuts")
                                    .clicked()
                                {
                                    chosen = Some(PaletteTarget::Operation(operation.id.clone()));
                                }
                            }
                            ui.weak("Parameters");
                            let parameters = filter_named(
                                self.editor.document.parameters.iter(),
                                &query,
                                |parameter| parameter.name.as_str(),
                            );
                            if parameters.is_empty() {
                                ui.weak("No parameter matches.");
                            }
                            for parameter in parameters {
                                let text = format!("Parameter  ·  {}", parameter.name);
                                if ui
                                    .add(
                                        egui::Button::new(text)
                                            .frame(false)
                                            .min_size(egui::vec2(ui.available_width(), 0.0)),
                                    )
                                    .on_hover_text("Open the parameters window")
                                    .clicked()
                                {
                                    chosen = Some(PaletteTarget::Parameter);
                                }
                            }
                        }
                    });
            });
        if close_requested {
            open = false;
        }
        if let Some(target) = chosen {
            match target {
                PaletteTarget::Command(action) => self.run_palette_action(action),
                PaletteTarget::Object(id) => {
                    self.editor.selection.clear();
                    self.editor.selection.add(id);
                    self.status = "Selected from the palette".to_string();
                }
                PaletteTarget::Layer(id) => {
                    self.editor.active_layer_id = id;
                    self.status = "Active layer changed".to_string();
                }
                PaletteTarget::Operation(id) => {
                    self.active_operation = Some(id);
                    self.open_cuts();
                }
                PaletteTarget::Parameter => self.open_parameters(),
            }
            open = false;
        }
        if !open {
            self.palette_query.clear();
        }
        self.palette_open = open;
    }

    pub(super) fn workspaces_window(&mut self, ctx: &egui::Context) {
        let mut open = self.workspaces_open;
        let mut save = false;
        let mut apply: Option<String> = None;
        let mut delete: Option<String> = None;
        let mut reset = false;
        let mut close_requested = false;
        egui::Window::new("Workspaces")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(
                    "A workspace stores which panels and overlays are visible. \
                 The tool paths, layers, properties, rulers, grid and guides are recorded.",
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.workspace_name)
                            .hint_text("Workspace name")
                            .desired_width(220.0),
                    )
                    .widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::TextEdit,
                            true,
                            "Workspace name",
                        )
                    });
                    if ui
                        .add_enabled(
                            !self.workspace_name.trim().is_empty(),
                            egui::Button::new("Save current layout"),
                        )
                        .on_hover_text(
                            "Saves the layout under this name; an existing name is replaced",
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
                ui.separator();
                if self.preferences.workspaces.saved.is_empty() {
                    ui.weak("No workspaces saved yet.");
                }
                for workspace in &self.preferences.workspaces.saved {
                    ui.horizontal(|ui| {
                        ui.label(&workspace.name);
                        if ui.button("Apply").clicked() {
                            apply = Some(workspace.name.clone());
                        }
                        if ui
                            .button("Delete")
                            .on_hover_text(
                                "Removes this workspace; the current layout stays as it is",
                            )
                            .clicked()
                        {
                            delete = Some(workspace.name.clone());
                        }
                    });
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button("Reset layout")
                        .on_hover_text("Restores the initial layout")
                        .clicked()
                    {
                        reset = true;
                    }
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
            });
        let mut stored = false;
        if save {
            let name = self.workspace_name.trim().to_string();
            let workspace = self.current_workspace(&name);
            match self.preferences.workspaces.upsert(workspace) {
                Ok(()) => {
                    self.status = format!("Workspace '{name}' saved");
                    stored = true;
                }
                Err(error) => self.status = error,
            }
        }
        if let Some(name) = apply {
            if let Some(workspace) = self.preferences.workspaces.find(&name).cloned() {
                self.apply_workspace(&workspace);
                self.status = format!("Workspace '{name}' applied");
            }
        }
        if let Some(name) = delete {
            if self.preferences.workspaces.remove(&name) {
                self.status = format!("Workspace '{name}' deleted");
                stored = true;
            }
        }
        if reset {
            self.apply_workspace(&crate::config::preferences::Workspace::initial());
            self.status = "Layout reset to the initial layout".to_string();
        }
        if stored {
            match self.preferences.save(&self.preferences_path) {
                Ok(()) => {}
                Err(error) => self.status = format!("Cannot save preferences: {error}"),
            }
        }
        if close_requested {
            open = false;
        }
        self.workspaces_open = open;
    }

    pub(super) fn apply_interface_preset(
        &mut self,
        preset: &crate::config::preferences::InterfacePreset,
    ) {
        self.preferences.general.theme.clone_from(&preset.theme);
        self.dark_mode = preset.theme == "dark";
        self.preferences.shortcuts = preset.shortcuts.clone();
        self.apply_workspace(&preset.workspace);
    }

    pub(super) fn interface_presets_window(&mut self, ctx: &egui::Context) {
        let mut open = self.interface_presets_open;
        let mut save = false;
        let mut apply: Option<String> = None;
        let mut delete: Option<String> = None;
        let mut close_requested = false;
        egui::Window::new("Interface presets")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(
                    "An interface preset stores the theme, the panel layout and every \
                 keyboard binding together. Applying one replaces all three.",
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.interface_preset_name)
                            .hint_text("Preset name")
                            .desired_width(220.0),
                    )
                    .widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::TextEdit,
                            true,
                            "Interface preset name",
                        )
                    });
                    if ui
                        .add_enabled(
                            !self.interface_preset_name.trim().is_empty(),
                            egui::Button::new("Save current interface"),
                        )
                        .on_hover_text("An existing name is replaced")
                        .clicked()
                    {
                        save = true;
                    }
                });
                ui.separator();
                if self.preferences.interface_presets.saved.is_empty() {
                    ui.weak("No interface presets saved yet.");
                }
                for preset in &self.preferences.interface_presets.saved {
                    ui.horizontal(|ui| {
                        ui.label(&preset.name);
                        ui.weak(preset.theme.clone());
                        if ui.button("Apply").clicked() {
                            apply = Some(preset.name.clone());
                        }
                        if ui.button("Delete").clicked() {
                            delete = Some(preset.name.clone());
                        }
                    });
                }
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
        let mut stored = false;
        if save {
            let name = self.interface_preset_name.trim().to_string();
            let mut workspace = self.current_workspace(&name);
            workspace.name.clone_from(&name);
            let preset = crate::config::preferences::InterfacePreset {
                name: name.clone(),
                theme: self.preferences.general.theme.clone(),
                workspace,
                shortcuts: self.preferences.shortcuts.clone(),
            };
            match self.preferences.interface_presets.upsert(preset) {
                Ok(()) => {
                    self.status = format!("Interface preset '{name}' saved");
                    stored = true;
                }
                Err(error) => self.status = error,
            }
        }
        if let Some(name) = apply {
            if let Some(preset) = self.preferences.interface_presets.find(&name).cloned() {
                self.apply_interface_preset(&preset);
                self.status = format!("Interface preset '{name}' applied");
            }
        }
        if let Some(name) = delete {
            if self.preferences.interface_presets.remove(&name) {
                self.status = format!("Interface preset '{name}' deleted");
                stored = true;
            }
        }
        if stored {
            match self.preferences.save(&self.preferences_path) {
                Ok(()) => {}
                Err(error) => self.status = format!("Cannot save preferences: {error}"),
            }
        }
        if close_requested {
            open = false;
        }
        self.interface_presets_open = open;
    }

    pub(super) fn preferences_window(&mut self, ctx: &egui::Context) {
        use crate::config::preferences::Section;
        let mut open = self.preferences_open;
        let mut changed = false;
        let mut reset_section = false;
        let mut reset_all = false;
        let mut close_requested = false;
        egui::Window::new("Preferences")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([720.0, 440.0])
            .min_size([600.0, 340.0])
            .max_height(500.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(icons::image_at(ui.ctx(), Icon::Search, 16.0));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.preferences_search)
                            .hint_text("Search settings")
                            .desired_width(240.0),
                    )
                    .widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::TextEdit,
                            true,
                            "Search settings",
                        )
                    });
                    if !self.preferences_search.is_empty() && ui.button("Clear").clicked() {
                        self.preferences_search.clear();
                    }
                });
                ui.separator();
                let content_height = (ui.available_height() - 44.0).clamp(200.0, 330.0);
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(190.0, content_height),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("preferences_sections")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    for section in Section::ALL {
                                        if !section.matches(&self.preferences_search) {
                                            continue;
                                        }
                                        if icons::selectable(
                                            ui,
                                            self.preferences_section == section,
                                            section_icon(section),
                                            section.label(),
                                        )
                                        .clicked()
                                        {
                                            self.preferences_section = section;
                                        }
                                    }
                                    if Section::first_match(&self.preferences_search).is_none() {
                                        ui.weak("No settings match that search.");
                                    }
                                });
                        },
                    );
                    ui.separator();
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), content_height),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("preferences_content")
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    if Section::first_match(&self.preferences_search).is_some() {
                                        ui.heading(self.preferences_section.label());
                                        ui.weak(self.preferences_section.hint());
                                        ui.add_space(8.0);
                                        changed |= self.preferences_section_ui(ui);
                                    } else {
                                        ui.heading("No match");
                                        ui.label(
                                            "No setting matches the search. Clear it to see \
                                         every category again.",
                                        );
                                    }
                                });
                        },
                    );
                });
                ui.separator();
                ui.horizontal(|ui| {
                    let resettable = self.preferences_section != Section::Machines;
                    if ui
                        .add_enabled(resettable, egui::Button::new("Reset section"))
                        .on_hover_text("Restore the defaults of this category only")
                        .clicked()
                    {
                        reset_section = true;
                    }
                    if ui
                        .button("Reset all")
                        .on_hover_text("Restore every setting except the recent projects")
                        .clicked()
                    {
                        reset_all = true;
                    }
                    ui.weak("Changes are saved automatically.");
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
            });
        if close_requested {
            open = false;
        }
        if let Some(command) = self.shortcut_capture {
            let pressed = ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::Key {
                        key,
                        modifiers,
                        pressed: true,
                        ..
                    } => Some((*key, *modifiers)),
                    _ => None,
                })
            });
            if let Some((key, modifiers)) = pressed {
                self.shortcut_capture = None;
                if key == Key::Escape {
                    self.status = "Shortcut change cancelled".to_string();
                } else {
                    match self.preferences.shortcuts.assign(command, key, modifiers) {
                        Ok(()) => {
                            changed = true;
                            self.status = format!(
                                "{} is now {}",
                                command.label(),
                                self.preferences.shortcuts.chord(command)
                            );
                        }
                        Err(error) => self.status = error,
                    }
                }
            }
        }
        if let Some(first) = Section::first_match(&self.preferences_search) {
            if !self.preferences_section.matches(&self.preferences_search) {
                self.preferences_section = first;
            }
        }
        if reset_section {
            self.preferences.reset_section(self.preferences_section);
            changed = true;
        }
        if reset_all {
            self.preferences.reset_section(Section::Advanced);
            changed = true;
        }
        if changed {
            self.apply_preferences();
            match self.preferences.save(&self.preferences_path) {
                Ok(()) => self.status = "Preferences saved".to_string(),
                Err(error) => self.status = format!("Cannot save preferences: {error}"),
            }
        }
        self.preferences_open = open;
    }

    /// Copy the stored preferences into the live editor state.
    pub(super) fn apply_preferences(&mut self) {
        self.editor.canvas.grid_spacing_mm = self.preferences.drawing.grid_spacing_mm;
        self.editor.snap_step_mm = self.preferences.drawing.snap_step_mm;
        self.editor.snap_enabled = self.preferences.drawing.snap_enabled;
        self.editor.snap_nodes = self.preferences.drawing.snap_nodes;
        self.editor.snap_endpoints = self.preferences.drawing.snap_endpoints;
        self.editor.snap_centers = self.preferences.drawing.snap_centers;
        self.editor.snap_midpoints = self.preferences.drawing.snap_midpoints;
        self.editor.snap_intersections = self.preferences.drawing.snap_intersections;
        self.editor.snap_limits = self.preferences.drawing.snap_limits;
        self.editor.show_rulers = self.preferences.drawing.show_rulers;
        self.editor.show_selection = self.preferences.drawing.show_selection;
        self.editor
            .selection_color
            .clone_from(&self.preferences.drawing.selection_color);
        self.editor.handle_style =
            crate::editor::state::HandleStyle::from_label(&self.preferences.drawing.handle_style);
        self.editor.zoom_step = self.preferences.drawing.zoom_step;
        self.editor.scroll_zoom_step = self.preferences.drawing.scroll_zoom_step;
        self.editor.select_whole_group = self.preferences.drawing.select_whole_group;
        self.editor.guides_visible = self.preferences.drawing.guides_visible;
        self.editor.snap_guides = self.preferences.drawing.snap_guides;
        self.editor
            .set_history_limit(self.preferences.drawing.undo_steps);
        self.dark_mode = self.preferences.general.theme == "dark";
    }

    /// Draw the controls of the selected preferences category.
    pub(super) fn preferences_section_ui(&mut self, ui: &mut egui::Ui) -> bool {
        use crate::config::preferences::Section;
        let mut changed = false;
        match self.preferences_section {
            Section::General => {
                egui::Grid::new("pref_general")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        let unit_label = ui.label("Default unit");
                        let mut unit = self.preferences.general.default_unit.clone();
                        egui::ComboBox::from_id_salt("pref_default_unit")
                            .selected_text(unit.clone())
                            .show_ui(ui, |ui| {
                                for candidate in ["mm", "cm", "in", "px"] {
                                    ui.selectable_value(
                                        &mut unit,
                                        candidate.to_string(),
                                        candidate,
                                    );
                                }
                            })
                            .response
                            .labelled_by(unit_label.id);
                        if unit != self.preferences.general.default_unit {
                            self.preferences.general.default_unit = unit;
                            changed = true;
                        }
                        ui.end_row();

                        ui.label("Text font");
                        changed |= font_picker(
                            ui,
                            "preferences_text_font",
                            &mut self.preferences.general.text_font,
                        );
                        ui.end_row();
                    });
                changed |= ui
                    .checkbox(
                        &mut self.preferences.general.confirm_destructive,
                        "Confirm before discarding unsaved changes",
                    )
                    .changed();
            }
            Section::Interface => {
                egui::Grid::new("pref_interface")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        let theme_label = ui.label("Theme");
                        let mut dark = self.preferences.general.theme == "dark";
                        egui::ComboBox::from_id_salt("pref_theme")
                            .selected_text(if dark { "Dark" } else { "Light" })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut dark, true, "Dark");
                                ui.selectable_value(&mut dark, false, "Light");
                            })
                            .response
                            .labelled_by(theme_label.id);
                        let wanted = if dark { "dark" } else { "light" };
                        if wanted != self.preferences.general.theme {
                            self.preferences.general.theme = wanted.to_string();
                            changed = true;
                        }
                        ui.end_row();

                        ui.label("Selection colour");
                        let (r, g, b) = parse_hex_color(&self.preferences.drawing.selection_color)
                            .unwrap_or((255, 122, 26));
                        let mut rgb = [r, g, b];
                        if ui.color_edit_button_srgb(&mut rgb).changed() {
                            self.preferences.drawing.selection_color =
                                to_hex(rgb[0], rgb[1], rgb[2]);
                            changed = true;
                        }
                        ui.end_row();

                        let style_label = ui.label("Handle style");
                        let mut style = self.preferences.drawing.handle_style.clone();
                        egui::ComboBox::from_id_salt("pref_handle_style")
                            .selected_text(style.clone())
                            .show_ui(ui, |ui| {
                                for candidate in ["square", "circle"] {
                                    ui.selectable_value(
                                        &mut style,
                                        candidate.to_string(),
                                        candidate,
                                    );
                                }
                            })
                            .response
                            .labelled_by(style_label.id);
                        if style != self.preferences.drawing.handle_style {
                            self.preferences.drawing.handle_style = style;
                            changed = true;
                        }
                        ui.end_row();

                        let zoom_label = ui.label("Zoom step");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut self.preferences.drawing.zoom_step)
                                    .speed(0.01)
                                    .range(1.01..=3.0)
                                    .fixed_decimals(2),
                            )
                            .labelled_by(zoom_label.id)
                            .changed();
                        ui.end_row();

                        let wheel_label = ui.label("Wheel zoom step");
                        changed |= ui
                            .add(
                                egui::DragValue::new(
                                    &mut self.preferences.drawing.scroll_zoom_step,
                                )
                                .speed(0.01)
                                .range(1.01..=2.0)
                                .fixed_decimals(2),
                            )
                            .labelled_by(wheel_label.id)
                            .changed();
                        ui.end_row();
                    });
                changed |= ui
                    .checkbox(&mut self.preferences.drawing.show_rulers, "Show rulers")
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.preferences.drawing.show_selection,
                        "Show selection outlines and handles",
                    )
                    .changed();
                changed |= ui
                    .checkbox(&mut self.preferences.drawing.guides_visible, "Show guides")
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.preferences.drawing.select_whole_group,
                        "Clicking one object selects the whole group",
                    )
                    .changed();
            }
            Section::Drawing => {
                egui::Grid::new("pref_drawing")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        let grid_label = ui.label("Grid spacing");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut self.preferences.drawing.grid_spacing_mm)
                                    .speed(0.5)
                                    .range(0.1..=1000.0)
                                    .suffix(" mm"),
                            )
                            .labelled_by(grid_label.id)
                            .changed();
                        ui.end_row();

                        let snap_label = ui.label("Snap step");
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut self.preferences.drawing.snap_step_mm)
                                    .speed(0.1)
                                    .range(0.1..=100.0)
                                    .suffix(" mm"),
                            )
                            .labelled_by(snap_label.id)
                            .changed();
                        ui.end_row();

                        let tolerance_label = ui.label("Snap tolerance");
                        changed |= ui
                            .add(
                                egui::DragValue::new(
                                    &mut self.preferences.drawing.snap_tolerance_px,
                                )
                                .speed(0.5)
                                .range(0.0..=64.0)
                                .suffix(" px"),
                            )
                            .labelled_by(tolerance_label.id)
                            .changed();
                        ui.end_row();
                    });
                changed |= ui
                    .checkbox(
                        &mut self.preferences.drawing.snap_enabled,
                        "Snap to the grid step",
                    )
                    .changed();
                ui.add_space(6.0);
                ui.strong("Snap targets");
                changed |= ui
                    .checkbox(&mut self.preferences.drawing.snap_nodes, "Path nodes")
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.preferences.drawing.snap_endpoints,
                        "Open-path endpoints",
                    )
                    .changed();
                changed |= ui
                    .checkbox(&mut self.preferences.drawing.snap_centers, "Object centres")
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.preferences.drawing.snap_midpoints,
                        "Segment midpoints",
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut self.preferences.drawing.snap_intersections,
                        "Outline intersections",
                    )
                    .changed();
                changed |= ui
                    .checkbox(&mut self.preferences.drawing.snap_limits, "Object bounds")
                    .changed();
                changed |= ui
                    .checkbox(&mut self.preferences.drawing.snap_guides, "Guides")
                    .changed();
            }
            Section::Images => {
                egui::Grid::new("pref_images")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        let largest_label = ui.label("Largest imported image");
                        let mut megapixels =
                            self.preferences.drawing.max_image_pixels as f64 / 1_000_000.0;
                        if ui
                            .add(
                                egui::DragValue::new(&mut megapixels)
                                    .speed(1.0)
                                    .range(1.0..=500.0)
                                    .suffix(" MP"),
                            )
                            .labelled_by(largest_label.id)
                            .changed()
                        {
                            self.preferences.drawing.max_image_pixels =
                                (megapixels * 1_000_000.0).round() as u64;
                            changed = true;
                        }
                        ui.end_row();
                    });
                ui.weak(
                    "Preparation settings (mode, dithering, tone curve, crop) belong to each \
                 image; open Adjust image from the object context menu.",
                );
            }
            Section::Files => {
                egui::Grid::new("pref_files")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        let directory_label = ui.label("Default directory");
                        changed |= ui
                            .text_edit_singleline(&mut self.preferences.general.default_directory)
                            .labelled_by(directory_label.id)
                            .changed();
                        ui.end_row();
                    });
                changed |= ui
                    .checkbox(
                        &mut self.preferences.general.backup_enabled,
                        "Keep a .bak of the previous project version",
                    )
                    .changed();
                ui.add_space(6.0);
                ui.strong("Recent projects");
                ui.horizontal(|ui| {
                    ui.weak(format!(
                        "{} remembered",
                        self.preferences.general.recent_projects.len()
                    ));
                    let enabled = !self.preferences.general.recent_projects.is_empty();
                    if ui
                        .add_enabled(enabled, egui::Button::new("Clear recent projects"))
                        .clicked()
                    {
                        self.preferences.general.recent_projects.clear();
                        changed = true;
                    }
                });
            }
            Section::Shortcuts => {
                ui.label(
                    "Click Change and press the new combination. Escape cancels. \
                 Delete and Backspace, Equals and Plus, and Ctrl+Shift+Z stay as fixed aliases.",
                );
                ui.add_space(4.0);
                if let Some(command) = self.shortcut_capture {
                    ui.colored_label(
                        crate::theme::ACCENT,
                        format!("Press a key for \"{}\" (Escape cancels)", command.label()),
                    );
                }
                let mut capture: Option<Command> = None;
                let mut reset: Option<Command> = None;
                egui::Grid::new("pref_shortcuts")
                    .num_columns(3)
                    .striped(true)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        for command in Command::ALL {
                            ui.label(command.label());
                            ui.monospace(self.preferences.shortcuts.chord(command));
                            ui.horizontal(|ui| {
                                if ui.button("Change").clicked() {
                                    capture = Some(command);
                                }
                                if ui.button("Reset").clicked() {
                                    reset = Some(command);
                                }
                            });
                            ui.end_row();
                        }
                    });
                if let Some(command) = capture {
                    self.shortcut_capture = Some(command);
                }
                if let Some(command) = reset {
                    self.preferences.shortcuts.reset(command);
                    changed = true;
                }
                if ui.button("Reset all shortcuts").clicked() {
                    self.preferences.shortcuts.reset_all();
                    self.shortcut_capture = None;
                    changed = true;
                }
            }
            Section::Machines => {
                ui.weak(
                    "Profiles hold work areas, firmware labels and reference values. No \
                 connection, port or controller setting is stored.",
                );
                ui.add_space(6.0);
                if icons::button(ui, Icon::Profiles, "Machine profiles...").clicked() {
                    self.machines_open = true;
                }
                if icons::button(ui, Icon::Materials, "Materials and operations...").clicked() {
                    self.profiles_open = true;
                }
                if icons::button(ui, Icon::Materials, "Recipes...").clicked() {
                    self.recipes_open = true;
                }
            }
            Section::Advanced => {
                egui::Grid::new("pref_advanced")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        ui.label("Data directory");
                        let data = crate::config::paths::data_dir().display().to_string();
                        ui.add(
                            egui::Label::new(egui::RichText::new(data.clone()).monospace())
                                .truncate(),
                        )
                        .on_hover_text(data);
                        ui.end_row();
                        let undo_label = ui.label("Undo steps");
                        ui.add(
                        egui::DragValue::new(&mut self.preferences.drawing.undo_steps)
                            .range(5..=200),
                    )
                    .labelled_by(undo_label.id)
                    .on_hover_text(
                        "Snapshots kept in memory; lower values use less memory on old machines",
                    );
                        ui.end_row();
                        ui.label("Preferences file");
                        let file = self.preferences_path.display().to_string();
                        ui.add(
                            egui::Label::new(egui::RichText::new(file.clone()).monospace())
                                .truncate(),
                        )
                        .on_hover_text(file);
                        ui.end_row();
                    });
                ui.weak(
                    "Reset all restores every setting above but keeps the recent-project list.",
                );
            }
        }
        changed
    }

    pub(super) fn profiles_window(&mut self, ctx: &egui::Context) {
        use crate::operations::OperationType;
        use crate::profiles::{MaterialProfile, OperationProfile};

        let mut open = self.profiles_open;
        let mut changed = false;
        egui::Window::new("Materials and operations")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .default_size([560.0, 460.0])
            .max_height(560.0)
            .show(ctx, |ui| {
                ui.label(
                "Values are reference starting points, not guarantees. Test on your own material.",
            );
                ui.horizontal(|ui| {
                    if ui.button("Import profiles...").clicked() {
                        self.open_dialog(FileMode::ImportProfiles);
                    }
                    if ui.button("Export profiles...").clicked() {
                        self.open_dialog(FileMode::ExportProfiles);
                    }
                    ui.menu_button("Materials CSV", |ui| {
                        if ui.button("Import materials CSV...").clicked() {
                            self.open_dialog(FileMode::ImportMaterialsCsv);
                            ui.close();
                        }
                        if ui.button("Export materials CSV...").clicked() {
                            self.open_dialog(FileMode::ExportMaterialsCsv);
                            ui.close();
                        }
                    });
                });
                ui.horizontal(|ui| {
                    let search_label = ui.label("Search");
                    ui.text_edit_singleline(&mut self.profile_search)
                        .labelled_by(search_label.id);
                });
                let query = self.profile_search.trim().to_lowercase();
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.profile_favorites_only, "Favourites only");
                    let category_label = ui.label("Category");
                    let selected = if self.profile_category_filter.is_empty() {
                        "All".to_string()
                    } else {
                        self.profile_category_filter.clone()
                    };
                    egui::ComboBox::from_id_salt("profile_category_filter")
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.profile_category_filter,
                                String::new(),
                                "All",
                            );
                            for category in self.profiles.categories() {
                                ui.selectable_value(
                                    &mut self.profile_category_filter,
                                    category.clone(),
                                    category,
                                );
                            }
                        })
                        .response
                        .labelled_by(category_label.id);
                    ui.checkbox(&mut self.profile_sort_by_date, "Newest first");
                });
                let filter = ProfileFilter {
                    query,
                    favorites_only: self.profile_favorites_only,
                    category: self.profile_category_filter.clone(),
                    sort_by_date: self.profile_sort_by_date,
                };
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("profiles_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.heading("Materials");
                        let materials = profile_rows(
                            &self.profiles.materials,
                            |profile| (&profile.id, &profile.name, profile.builtin, &profile.meta),
                            |profile| format!("{} {}", profile.material, profile.notes),
                            &filter,
                        );
                        for row in &materials {
                            ui.horizontal(|ui| {
                                let selected =
                                    self.selected_material.as_deref() == Some(row.id.as_str());
                                if ui.selectable_label(selected, &row.name).clicked() {
                                    self.selected_material = Some(row.id.clone());
                                }
                                if row.favorite {
                                    ui.weak("\u{2605}");
                                }
                                if !row.category.is_empty() {
                                    ui.weak(row.category.as_str());
                                }
                                if row.builtin {
                                    ui.weak("built-in");
                                }
                            });
                        }
                        ui.horizontal(|ui| {
                            if ui.button("+ Material").clicked() {
                                let profile = self.profiles.add_material(MaterialProfile::new(
                                    "New material",
                                    "Material",
                                    3.0,
                                ));
                                self.selected_material = Some(profile.id.clone());
                                changed = true;
                            }
                            if ui
                                .add_enabled(
                                    self.selected_material.is_some(),
                                    egui::Button::new("Duplicate"),
                                )
                                .clicked()
                            {
                                if let Some(id) = self.selected_material.clone() {
                                    if let Some(copy) = self.profiles.duplicate_material(&id) {
                                        self.selected_material = Some(copy);
                                    }
                                }
                            }
                            let removable = self
                                .selected_material
                                .as_deref()
                                .and_then(|id| self.profiles.material(id))
                                .is_some_and(|profile| !profile.builtin);
                            if ui
                                .add_enabled(removable, egui::Button::new("Delete"))
                                .clicked()
                            {
                                if let Some(id) = self.selected_material.clone() {
                                    match self.profiles.remove_material(&id) {
                                        Ok(()) => {
                                            self.selected_material = None;
                                            changed = true;
                                        }
                                        Err(error) => self.profile_message = error,
                                    }
                                }
                            }
                        });
                        let operation_choices: Vec<(String, String)> = self
                            .profiles
                            .operations
                            .iter()
                            .map(|operation| (operation.id.clone(), operation.name.clone()))
                            .collect();
                        if let Some(id) = self.selected_material.clone() {
                            if let Some(profile) = self
                                .profiles
                                .materials
                                .iter_mut()
                                .find(|profile| profile.id == id)
                            {
                                let editable = !profile.builtin;
                                ui.add_enabled_ui(editable, |ui| {
                                    ui.horizontal(|ui| {
                                        let name_label = ui.label("Name");
                                        changed |= ui
                                            .text_edit_singleline(&mut profile.name)
                                            .labelled_by(name_label.id)
                                            .changed();
                                    });
                                    ui.horizontal(|ui| {
                                        let material_label = ui.label("Material");
                                        changed |= ui
                                            .text_edit_singleline(&mut profile.material)
                                            .labelled_by(material_label.id)
                                            .changed();
                                    });
                                    ui.horizontal(|ui| {
                                        let thickness_label = ui.label("Thickness");
                                        changed |= ui
                                            .add(
                                                egui::DragValue::new(&mut profile.thickness_mm)
                                                    .speed(0.1)
                                                    .range(0.1..=100.0)
                                                    .suffix(" mm"),
                                            )
                                            .labelled_by(thickness_label.id)
                                            .changed();
                                    });
                                    ui.horizontal(|ui| {
                                        let notes_label = ui.label("Notes");
                                        changed |= ui
                                            .text_edit_singleline(&mut profile.notes)
                                            .labelled_by(notes_label.id)
                                            .changed();
                                    });
                                    ui.separator();
                                    ui.label("Recommended operations");
                                    for (operation_id, operation_name) in &operation_choices {
                                        let mut checked = profile
                                            .recommended_operations
                                            .iter()
                                            .any(|recommended| recommended == operation_id);
                                        if ui.checkbox(&mut checked, operation_name).changed() {
                                            if checked {
                                                profile
                                                    .recommended_operations
                                                    .push(operation_id.clone());
                                            } else {
                                                profile.recommended_operations.retain(
                                                    |recommended| recommended != operation_id,
                                                );
                                            }
                                            changed = true;
                                        }
                                    }
                                    ui.weak("Missing operation profiles are ignored.");
                                });
                                changed |= profile_meta_editor(
                                    ui,
                                    &id,
                                    &mut profile.meta,
                                    editable,
                                    &mut self.profile_tag_buffer,
                                    &mut self.profile_tag_owner,
                                );
                                if !editable {
                                    ui.weak("Built-in profile: duplicate it to edit.");
                                }
                            }
                        }
                        ui.separator();
                        ui.heading("Operation profiles");
                        let operations = profile_rows(
                            &self.profiles.operations,
                            |profile| (&profile.id, &profile.name, profile.builtin, &profile.meta),
                            |profile| profile.notes.clone(),
                            &filter,
                        );
                        for row in &operations {
                            ui.horizontal(|ui| {
                                let selected =
                                    self.selected_operation.as_deref() == Some(row.id.as_str());
                                if ui.selectable_label(selected, &row.name).clicked() {
                                    self.selected_operation = Some(row.id.clone());
                                }
                                if row.favorite {
                                    ui.weak("\u{2605}");
                                }
                                if !row.category.is_empty() {
                                    ui.weak(row.category.as_str());
                                }
                                if row.builtin {
                                    ui.weak("built-in");
                                }
                            });
                        }
                        ui.horizontal(|ui| {
                            if ui.button("+ Operation profile").clicked() {
                                let profile = self.profiles.add_operation(OperationProfile::new(
                                    "New operation",
                                    OperationType::OutlineCutting,
                                ));
                                self.selected_operation = Some(profile.id.clone());
                                changed = true;
                            }
                            if ui
                                .add_enabled(
                                    self.selected_operation.is_some(),
                                    egui::Button::new("Duplicate"),
                                )
                                .clicked()
                            {
                                if let Some(id) = self.selected_operation.clone() {
                                    if let Some(copy) = self.profiles.duplicate_operation(&id) {
                                        self.selected_operation = Some(copy);
                                    }
                                }
                            }
                            let removable = self
                                .selected_operation
                                .as_deref()
                                .and_then(|id| self.profiles.operation(id))
                                .is_some_and(|profile| !profile.builtin);
                            if ui
                                .add_enabled(removable, egui::Button::new("Delete"))
                                .clicked()
                            {
                                if let Some(id) = self.selected_operation.clone() {
                                    match self.profiles.remove_operation(&id) {
                                        Ok(()) => {
                                            self.selected_operation = None;
                                            changed = true;
                                        }
                                        Err(error) => self.profile_message = error,
                                    }
                                }
                            }
                        });
                        if let Some(id) = self.selected_operation.clone() {
                            if let Some(profile) = self
                                .profiles
                                .operations
                                .iter_mut()
                                .find(|profile| profile.id == id)
                            {
                                let editable = !profile.builtin;
                                ui.add_enabled_ui(editable, |ui| {
                                    ui.horizontal(|ui| {
                                        let name_label = ui.label("Name");
                                        changed |= ui
                                            .text_edit_singleline(&mut profile.name)
                                            .labelled_by(name_label.id)
                                            .changed();
                                    });
                                    let mut op_type = profile.op_type;
                                    egui::ComboBox::from_label("Type")
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
                                    if op_type != profile.op_type {
                                        profile.op_type = op_type;
                                        changed = true;
                                    }
                                    ui.horizontal(|ui| {
                                        let speed_label = ui.label("Speed");
                                        changed |= ui
                                            .add(
                                                egui::DragValue::new(&mut profile.params.speed)
                                                    .speed(10.0)
                                                    .range(1.0..=20000.0)
                                                    .suffix(" mm/min"),
                                            )
                                            .labelled_by(speed_label.id)
                                            .changed();
                                    });
                                    ui.horizontal(|ui| {
                                        let power_label = ui.label("Power");
                                        changed |= ui
                                            .add(
                                                egui::DragValue::new(&mut profile.params.power)
                                                    .speed(1.0)
                                                    .range(0.0..=100.0)
                                                    .suffix(" %"),
                                            )
                                            .labelled_by(power_label.id)
                                            .changed();
                                    });
                                    ui.horizontal(|ui| {
                                        let passes_label = ui.label("Passes");
                                        changed |= ui
                                            .add(
                                                egui::DragValue::new(&mut profile.params.passes)
                                                    .speed(1.0)
                                                    .range(1..=100),
                                            )
                                            .labelled_by(passes_label.id)
                                            .changed();
                                        let interval_label = ui.label("Interval");
                                        changed |= ui
                                            .add(
                                                egui::DragValue::new(
                                                    &mut profile.params.line_interval_mm,
                                                )
                                                .speed(0.01)
                                                .range(0.01..=10.0)
                                                .suffix(" mm"),
                                            )
                                            .labelled_by(interval_label.id)
                                            .changed();
                                    });
                                });
                                changed |= profile_meta_editor(
                                    ui,
                                    &id,
                                    &mut profile.meta,
                                    editable,
                                    &mut self.profile_tag_buffer,
                                    &mut self.profile_tag_owner,
                                );
                                if !editable {
                                    ui.weak("Built-in profile: duplicate it to edit.");
                                }
                                let profile_clone = profile.clone();
                                if ui
                                    .add_enabled(
                                        !self.editor.selection.is_empty(),
                                        egui::Button::new("Apply to selection"),
                                    )
                                    .clicked()
                                {
                                    let count = self.editor.apply_operation_profile(&profile_clone);
                                    self.status = format!(
                                        "Applied '{}' to {count} object(s)",
                                        profile_clone.name
                                    );
                                }
                                if ui
                                    .add_enabled(
                                        self.active_operation.is_some(),
                                        egui::Button::new("Apply to active operation"),
                                    )
                                    .clicked()
                                {
                                    if let Some(operation) = self.active_operation.clone() {
                                        self.profile_pending =
                                            Some((operation, profile_clone.clone()));
                                        self.profile_confirm_open = true;
                                    }
                                }
                            }
                        }
                        ui.separator();
                        ui.heading("Image profiles");
                        let image_profiles = profile_rows(
                            &self.profiles.images,
                            |profile| (&profile.id, &profile.name, profile.builtin, &profile.meta),
                            |profile| profile.notes.clone(),
                            &filter,
                        );
                        for row in &image_profiles {
                            ui.horizontal(|ui| {
                                let selected =
                                    self.selected_image_profile.as_deref() == Some(row.id.as_str());
                                if ui.selectable_label(selected, &row.name).clicked() {
                                    self.selected_image_profile = Some(row.id.clone());
                                }
                                if row.favorite {
                                    ui.weak("\u{2605}");
                                }
                                if !row.category.is_empty() {
                                    ui.weak(row.category.as_str());
                                }
                                if row.builtin {
                                    ui.weak("built-in");
                                }
                            });
                        }
                        ui.horizontal(|ui| {
                            if ui.button("+ Image profile").clicked() {
                                let profile = self.profiles.add_image(
                                    crate::profiles::ImageProfile::new("New image profile"),
                                );
                                self.selected_image_profile = Some(profile.id.clone());
                                changed = true;
                            }
                            if ui
                                .add_enabled(
                                    self.selected_image_profile.is_some(),
                                    egui::Button::new("Duplicate"),
                                )
                                .clicked()
                            {
                                if let Some(id) = self.selected_image_profile.clone() {
                                    if let Some(copy) = self.profiles.duplicate_image(&id) {
                                        self.selected_image_profile = Some(copy);
                                    }
                                }
                            }
                            let removable = self
                                .selected_image_profile
                                .as_deref()
                                .and_then(|id| self.profiles.image(id))
                                .is_some_and(|profile| !profile.builtin);
                            if ui
                                .add_enabled(removable, egui::Button::new("Delete"))
                                .clicked()
                            {
                                if let Some(id) = self.selected_image_profile.clone() {
                                    match self.profiles.remove_image(&id) {
                                        Ok(()) => {
                                            self.selected_image_profile = None;
                                            changed = true;
                                        }
                                        Err(error) => self.profile_message = error,
                                    }
                                }
                            }
                        });
                        if let Some(id) = self.selected_image_profile.clone() {
                            if let Some(profile) = self
                                .profiles
                                .images
                                .iter_mut()
                                .find(|profile| profile.id == id)
                            {
                                let editable = !profile.builtin;
                                ui.add_enabled_ui(editable, |ui| {
                                    ui.horizontal(|ui| {
                                        let name_label = ui.label("Name");
                                        changed |= ui
                                            .text_edit_singleline(&mut profile.name)
                                            .labelled_by(name_label.id)
                                            .changed();
                                    });
                                    let mut mode = profile.settings.mode;
                                    egui::ComboBox::from_label("Mode")
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
                                    if mode != profile.settings.mode {
                                        profile.settings.mode = mode;
                                        changed = true;
                                    }
                                    changed |= ui
                                        .add(
                                            egui::Slider::new(
                                                &mut profile.settings.threshold,
                                                0..=255,
                                            )
                                            .text("Threshold"),
                                        )
                                        .changed();
                                    changed |= ui
                                        .add(
                                            egui::Slider::new(
                                                &mut profile.settings.brightness,
                                                -100..=100,
                                            )
                                            .text("Brightness"),
                                        )
                                        .changed();
                                    changed |= ui
                                        .add(
                                            egui::Slider::new(
                                                &mut profile.settings.contrast,
                                                0.25..=3.0,
                                            )
                                            .text("Contrast"),
                                        )
                                        .changed();
                                    changed |= ui
                                        .add(
                                            egui::Slider::new(
                                                &mut profile.settings.gamma,
                                                0.2..=3.0,
                                            )
                                            .text("Gamma"),
                                        )
                                        .changed();
                                    changed |= ui
                                        .checkbox(&mut profile.settings.invert, "Invert")
                                        .changed();
                                    let interval = ui.add(
                                        egui::DragValue::new(
                                            &mut profile.settings.line_interval_mm,
                                        )
                                        .speed(0.01)
                                        .range(0.01..=2.0)
                                        .suffix(" mm"),
                                    );
                                    interval.widget_info(|| {
                                        egui::WidgetInfo::labeled(
                                            egui::WidgetType::DragValue,
                                            true,
                                            "Line interval",
                                        )
                                    });
                                    changed |= interval.changed();
                                });
                                changed |= profile_meta_editor(
                                    ui,
                                    &id,
                                    &mut profile.meta,
                                    editable,
                                    &mut self.profile_tag_buffer,
                                    &mut self.profile_tag_owner,
                                );
                                if !editable {
                                    ui.weak("Built-in profile: duplicate it to edit.");
                                }
                                let profile_clone = profile.clone();
                                if ui
                                    .add_enabled(
                                        !self.editor.selection.is_empty(),
                                        egui::Button::new("Apply to selected images"),
                                    )
                                    .clicked()
                                {
                                    let count = self.editor.apply_image_profile(&profile_clone);
                                    self.status =
                                        format!("Applied image profile to {count} object(s)");
                                }
                            }
                        }
                        ui.separator();
                        ui.heading("Project profiles");
                        let project_profiles = profile_rows(
                            &self.profiles.projects,
                            |profile| (&profile.id, &profile.name, profile.builtin, &profile.meta),
                            |profile| profile.notes.clone(),
                            &filter,
                        );
                        for row in &project_profiles {
                            ui.horizontal(|ui| {
                                let selected = self.selected_project_profile.as_deref()
                                    == Some(row.id.as_str());
                                if ui.selectable_label(selected, &row.name).clicked() {
                                    self.selected_project_profile = Some(row.id.clone());
                                }
                                if row.favorite {
                                    ui.weak("\u{2605}");
                                }
                                if !row.category.is_empty() {
                                    ui.weak(row.category.as_str());
                                }
                                if row.builtin {
                                    ui.weak("built-in");
                                }
                            });
                        }
                        ui.horizontal(|ui| {
                            if ui.button("+ Project profile").clicked() {
                                let profile = self.profiles.add_project(
                                    crate::profiles::ProjectProfile::new(
                                        "New project",
                                        300.0,
                                        200.0,
                                    ),
                                );
                                self.selected_project_profile = Some(profile.id.clone());
                                changed = true;
                            }
                            if ui
                                .add_enabled(
                                    self.selected_project_profile.is_some(),
                                    egui::Button::new("Duplicate"),
                                )
                                .clicked()
                            {
                                if let Some(id) = self.selected_project_profile.clone() {
                                    if let Some(copy) = self.profiles.duplicate_project(&id) {
                                        self.selected_project_profile = Some(copy);
                                    }
                                }
                            }
                            let removable = self
                                .selected_project_profile
                                .as_deref()
                                .and_then(|id| self.profiles.project(id))
                                .is_some_and(|profile| !profile.builtin);
                            if ui
                                .add_enabled(removable, egui::Button::new("Delete"))
                                .clicked()
                            {
                                if let Some(id) = self.selected_project_profile.clone() {
                                    match self.profiles.remove_project(&id) {
                                        Ok(()) => {
                                            self.selected_project_profile = None;
                                            changed = true;
                                        }
                                        Err(error) => self.profile_message = error,
                                    }
                                }
                            }
                        });
                        if let Some(id) = self.selected_project_profile.clone() {
                            if let Some(profile) = self
                                .profiles
                                .projects
                                .iter_mut()
                                .find(|profile| profile.id == id)
                            {
                                let editable = !profile.builtin;
                                ui.add_enabled_ui(editable, |ui| {
                                    ui.horizontal(|ui| {
                                        let name_label = ui.label("Name");
                                        changed |= ui
                                            .text_edit_singleline(&mut profile.name)
                                            .labelled_by(name_label.id)
                                            .changed();
                                    });
                                    ui.horizontal(|ui| {
                                        let width_label = ui.label("Width");
                                        changed |= ui
                                            .add(
                                                egui::DragValue::new(&mut profile.width_mm)
                                                    .speed(1.0)
                                                    .range(1.0..=10000.0)
                                                    .suffix(" mm"),
                                            )
                                            .labelled_by(width_label.id)
                                            .changed();
                                        let height_label = ui.label("Height");
                                        changed |= ui
                                            .add(
                                                egui::DragValue::new(&mut profile.height_mm)
                                                    .speed(1.0)
                                                    .range(1.0..=10000.0)
                                                    .suffix(" mm"),
                                            )
                                            .labelled_by(height_label.id)
                                            .changed();
                                    });
                                    ui.horizontal(|ui| {
                                        let grid_label = ui.label("Grid");
                                        changed |= ui
                                            .add(
                                                egui::DragValue::new(&mut profile.grid_spacing_mm)
                                                    .speed(0.5)
                                                    .range(0.1..=100.0)
                                                    .suffix(" mm"),
                                            )
                                            .labelled_by(grid_label.id)
                                            .changed();
                                        let snap_label = ui.label("Snap");
                                        changed |= ui
                                            .add(
                                                egui::DragValue::new(&mut profile.snap_mm)
                                                    .speed(0.1)
                                                    .range(0.1..=100.0)
                                                    .suffix(" mm"),
                                            )
                                            .labelled_by(snap_label.id)
                                            .changed();
                                    });
                                });
                                changed |= profile_meta_editor(
                                    ui,
                                    &id,
                                    &mut profile.meta,
                                    editable,
                                    &mut self.profile_tag_buffer,
                                    &mut self.profile_tag_owner,
                                );
                                if !editable {
                                    ui.weak("Built-in profile: duplicate it to edit.");
                                }
                                let profile_clone = profile.clone();
                                if ui.button("Apply to project").clicked() {
                                    if self.editor.apply_project_profile(&profile_clone) {
                                        self.needs_fit = true;
                                        self.status = format!(
                                            "Applied project profile '{}'",
                                            profile_clone.name
                                        );
                                    } else {
                                        self.status = "Project profile is invalid".to_string();
                                    }
                                }
                            }
                        }
                        ui.separator();
                        ui.horizontal(|ui| {
                            let file_label = ui.label("File");
                            ui.text_edit_singleline(&mut self.profile_io_path)
                                .labelled_by(file_label.id);
                        });
                        ui.horizontal(|ui| {
                            if ui.button("Save profiles").clicked() {
                                self.save_profiles();
                            }
                            if ui.button("Import").clicked() {
                                let path = std::path::PathBuf::from(self.profile_io_path.trim());
                                match crate::profiles::import_into(&mut self.profiles, &path) {
                                    Ok(added) => {
                                        self.profile_message =
                                            format!("Imported {added} profile(s)");
                                        changed = true;
                                    }
                                    Err(error) => {
                                        self.profile_message = format!("Import failed: {error}");
                                    }
                                }
                            }
                            if ui.button("Export").clicked() {
                                let path = std::path::PathBuf::from(self.profile_io_path.trim());
                                match crate::profiles::export(&self.profiles, &path) {
                                    Ok(()) => {
                                        self.profile_message =
                                            format!("Exported profiles to {}", path.display());
                                    }
                                    Err(error) => {
                                        self.profile_message = format!("Export failed: {error}");
                                    }
                                }
                            }
                        });
                        if !self.profile_message.is_empty() {
                            ui.weak(&self.profile_message);
                        }
                    });
            });
        self.profiles_open = open;
        if changed {
            self.save_profiles();
        }
    }
}
