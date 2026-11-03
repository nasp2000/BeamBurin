#![allow(clippy::wildcard_imports)]

use super::*;

impl BeamBurinApp {
    pub(super) fn shortcuts_window(&mut self, ctx: &egui::Context) {
        let mut open = self.help_shortcuts_open;
        egui::Window::new("Keyboard shortcuts")
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(520.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("shortcut_table_scroll")
                .max_height(420.0)
                .show(ui, |ui| {
                    egui::Grid::new("shortcut_table")
                        .num_columns(2)
                        .striped(true)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            for command in Command::ALL {
                                ui.monospace(self.preferences.shortcuts.chord(command));
                                ui.label(command.label());
                                ui.end_row();
                            }
                            ui.monospace("Backspace");
                            ui.label("Also deletes the selection (fixed alias)");
                            ui.end_row();
                            ui.monospace("Equals");
                            ui.label("Also zooms in (fixed alias)");
                            ui.end_row();
                            ui.monospace("Ctrl+Shift+Z");
                            ui.label("Also redoes (fixed alias)");
                            ui.end_row();
                        });
                });
            ui.separator();
            ui.weak(
                "Change these in Preferences → Shortcuts. Shift+Arrows nudge by 0.1 mm; keys are never captured while a text field has focus.",
            );
        });
        self.help_shortcuts_open = open;
    }

    pub(super) fn assistant_window(&mut self, ctx: &egui::Context) {
        let mut open = self.assistant_open;
        let mut copy: Option<String> = None;
        let mut note = None;
        egui::Window::new("Assistant for agents")
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([560.0, 520.0])
        .show(ctx, |ui| {
            ui.label(
                "Hand this project to an agent that speaks MCP. BeamBurin embeds no model: you choose the client.",
            );
            ui.separator();
            ui.heading("Live bridge");
            match &self.bridge {
                Some(bridge) => {
                    ui.label(format!(
                        "Listening on {}:{}",
                        bridge.address(),
                        bridge.port()
                    ));
                    if bridge.is_network() {
                        ui.colored_label(
                            Color32::from_rgb(200, 130, 40),
                            "Reachable from the network: keep the token private and prefer a confined session.",
                        );
                    }
                    let token = crate::bridge::effective_token();
                    ui.horizontal(|ui| {
                        ui.label("Token");
                        ui.monospace(format!(
                            "{}…",
                            token.chars().take(4).collect::<String>()
                        ));
                        if ui
                            .button("Copy token")
                            .on_hover_text(
                                "Anyone with this token can read and change the open project",
                            )
                            .clicked()
                        {
                            note = Some("Token copied to the clipboard".to_string());
                            copy.clone_from(&Some(token));
                        }
                    });
                }
                None => {
                    ui.weak("The live bridge is not running in this window.");
                }
            }
            ui.add_space(4.0);
            ui.label("Launch lines");
            egui::Grid::new("assistant_launch")
                .num_columns(3)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    for (description, line) in crate::assistant::launch_commands() {
                        ui.label(description);
                        ui.monospace(line);
                        if ui.small_button("Copy").clicked() {
                            note = Some(format!("Copied: {line}"));
                            copy = Some(line.to_string());
                        }
                        ui.end_row();
                    }
                });
            ui.horizontal(|ui| {
                if ui.button("Copy client configuration").clicked() {
                    note = Some("Client configuration copied".to_string());
                    copy = Some(crate::assistant::client_config());
                }
                ui.weak("stdio entry for a client that runs BeamBurin itself");
            });
            ui.separator();
            ui.heading("Document");
            let document = &self.editor.document;
            ui.label(format!(
                "{}: {} x {} mm, {} object(s), {} operation(s), {} parameter(s)",
                document.name,
                crate::editor::renderer::format_mm(document.width_mm),
                crate::editor::renderer::format_mm(document.height_mm),
                document.objects.len(),
                document.operations.len(),
                document.parameters.len()
            ));
            if ui
                .button("Copy prompt with the document state")
                .on_hover_text(
                    "Includes the project summary, preflight problems with their codes and the safe workflow",
                )
                .clicked()
            {
                let prompt = crate::assistant::state_prompt(&self.editor.document);
                note = Some("Prompt copied to the clipboard".to_string());
                copy = Some(prompt);
            }
            ui.separator();
            ui.heading("Recipes");
            egui::Grid::new("assistant_recipes")
                .num_columns(2)
                .spacing([10.0, 4.0])
                .show(ui, |ui| {
                    for prompt in crate::mcp_content::prompts() {
                        let name = prompt["name"].as_str().unwrap_or_default();
                        ui.label(prompt["description"].as_str().unwrap_or_default());
                        if ui.small_button(format!("Copy {name}")).clicked() {
                            if let Ok(message) =
                                crate::mcp_content::prompt(name, &serde_json::json!({}))
                            {
                                let text = message["messages"][0]["content"]["text"]
                                    .as_str()
                                    .unwrap_or_default()
                                    .to_string();
                                note = Some(format!("Copied the {name} recipe"));
                                copy = Some(text);
                            }
                        }
                        ui.end_row();
                    }
                });
            ui.separator();
            ui.weak(
                "The agent reads and writes the document through MCP; every batch is atomic and one undo step. Use --read-only or --workspace when the project does not need free access.",
            );
        });
        if let Some(text) = copy {
            ctx.copy_text(text);
        }
        if let Some(note) = note {
            self.status = note;
        }
        self.assistant_open = open;
    }
}
