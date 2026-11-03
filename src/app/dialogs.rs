#![allow(clippy::wildcard_imports)]

use super::*;

impl BeamBurinApp {
    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        if self.machine_wizard_open {
            self.machine_wizard(ctx);
        }
        if self.machines_open {
            self.machines_window(ctx);
        }
        if self.cuts_open {
            self.cuts_window(ctx);
        }
        if self.parameters_open {
            self.parameters_window(ctx);
        }
        if self.constraints_open {
            self.constraints_window(ctx);
        }
        if self.recipes_open {
            self.recipes_window(ctx);
        }
        if self.material_test_open {
            self.material_test_window(ctx);
        }
        if self.help_shortcuts_open {
            self.shortcuts_window(ctx);
        }
        if self.assistant_open {
            self.assistant_window(ctx);
        }
        if self.image_adjust.is_some() {
            self.image_adjust_window(ctx);
        }
        if self.image_test_open {
            self.image_test_window(ctx);
        }
        if self.regmarks_open {
            self.registration_marks_window(ctx);
        }
        if self.export_presets_open {
            self.export_presets_window(ctx);
        }
        if self.external_gcode_open {
            self.external_gcode_window(ctx);
        }
        if let Some(dialog) = &mut self.dialog {
            let result = dialog.show(ctx);
            match result {
                DialogResult::Pending => {}
                DialogResult::Cancelled => self.dialog = None,
                DialogResult::Accepted(path) => {
                    let mode = dialog.mode;
                    self.dialog = None;
                    let overwrites = path.exists()
                        && !matches!(
                            mode,
                            FileMode::OpenProject | FileMode::ImportFile | FileMode::RelinkImage
                        );
                    if overwrites {
                        self.pending_file = Some((mode, path));
                    } else {
                        self.handle_file(mode, &path);
                    }
                }
            }
        }
        if self.task.is_some() {
            self.task_window(ctx);
        }
        if self.recovery_prompt_open {
            let mut open_draft = false;
            let mut discard = false;
            egui::Window::new("Recover unsaved work")
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(ctx.content_rect().center())
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(if self.recovery_timestamp.is_empty() {
                        "An autosaved draft was found.".to_string()
                    } else {
                        format!(
                            "An autosaved draft from {} was found.",
                            self.recovery_timestamp
                        )
                    });
                    ui.label("It was written because the application closed with unsaved changes.");
                    ui.horizontal(|ui| {
                        if ui.button("Open draft").clicked() {
                            open_draft = true;
                        }
                        if ui.button("Discard").clicked() {
                            discard = true;
                        }
                    });
                });
            if open_draft {
                match crate::document::serialization::load_from_file(&self.recovery_path) {
                    Ok(document) => {
                        self.editor.set_document(document);
                        self.current_path = None;
                        self.saved_snapshot = None;
                        self.needs_fit = true;
                        self.status = "Recovered the autosaved draft".to_string();
                        self.recovery_prompt_open = false;
                    }
                    Err(error) => {
                        self.status = format!("Cannot read the draft: {error}");
                    }
                }
            } else if discard {
                let _ = std::fs::remove_file(&self.recovery_path);
                self.recovery_prompt_open = false;
                self.status = "Discarded the autosaved draft".to_string();
            }
        }
        if let Some((mode, path)) = self.pending_file.clone() {
            let mut replace = false;
            let mut cancel = false;
            egui::Window::new("Replace file?")
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(ctx.content_rect().center())
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!("{} already exists.", path.display()));
                    ui.label("Replacing it cannot be undone.");
                    ui.horizontal(|ui| {
                        if ui.button("Replace").clicked() {
                            replace = true;
                        }
                        if ui
                            .button("Cancel")
                            .on_hover_text("Keep the current file")
                            .clicked()
                        {
                            cancel = true;
                        }
                    });
                });
            if replace {
                self.pending_file = None;
                self.handle_file(mode, &path);
            } else if cancel {
                self.pending_file = None;
            }
        }
        if self.confirm.is_some() {
            let mut action = None;
            let mut cancel = false;
            egui::Window::new("Unsaved changes")
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(ctx.content_rect().center())
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("This document has unsaved changes.");
                    ui.label("Discard them and continue?");
                    ui.horizontal(|ui| {
                        if ui.button("Discard").clicked() {
                            action = self.confirm;
                        }
                        if ui
                            .button("Cancel")
                            .on_hover_text("Keep the project open")
                            .clicked()
                        {
                            cancel = true;
                        }
                    });
                });
            if let Some(action) = action {
                self.confirm = None;
                self.perform(action);
            } else if cancel {
                self.confirm = None;
            }
        }
        if self.profiles_open {
            self.profiles_window(ctx);
        }
        if self.preferences_open {
            self.preferences_window(ctx);
        }
        if self.workspaces_open {
            self.workspaces_window(ctx);
        }
        if self.palette_open {
            self.palette_window(ctx);
        }
        if self.history_open {
            self.history_window(ctx);
        }
        if self.tour_open {
            self.tour_window(ctx);
        }
        if self.interface_presets_open {
            self.interface_presets_window(ctx);
        }
        if self.grid_array_open {
            self.grid_array_window(ctx);
        }
        if self.offset_open {
            self.offset_window(ctx);
        }
        if self.simplify_open {
            self.simplify_window(ctx);
        }
        if self.nest_open {
            self.nest_window(ctx);
        }
        if self.repeat_open {
            self.repeat_window(ctx);
        }
        if self.skew_open {
            self.skew_window(ctx);
        }
        if self.doc_properties_open {
            self.document_properties_window(ctx);
        }
        if self.operations_compare_open {
            self.operations_compare_window(ctx);
        }
        if self.profile_confirm_open {
            self.profile_confirm_window(ctx);
        }
        if self.guides_open {
            self.guides_window(ctx);
        }
        if self.report_open {
            self.report_window(ctx);
        }
        if self.preview_open {
            self.preview_window(ctx);
        }
        if self.about_open {
            let mut open = true;
            egui::Window::new("About BeamBurin")
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(ctx.content_rect().center())
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.heading("BeamBurin");
                    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.label("Vector drawing and laser job preparation.");
                    ui.label("This build prepares jobs only; it does not control machines.");
                    ui.label("MIT or Apache-2.0 licensed.");
                    if ui.button("Third-party licences").clicked() {
                        self.licenses_open = true;
                    }
                });
            self.about_open = open;
        }
        if self.licenses_open {
            let mut open = true;
            egui::Window::new("Licenses")
                .pivot(egui::Align2::CENTER_CENTER)
                .default_pos(ctx.content_rect().center())
                .collapsible(false)
                .resizable(true)
                .default_size([760.0, 560.0])
                .open(&mut open)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("licenses_scroll")
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(crate::licenses::NOTICES).monospace(),
                                )
                                .wrap(),
                            );
                        });
                });
            self.licenses_open = open;
        }
    }
}
