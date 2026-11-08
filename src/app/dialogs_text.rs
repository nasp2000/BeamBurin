#![allow(clippy::wildcard_imports)]

use super::*;

impl BeamBurinApp {
    /// Open the text editor on the first selected text object.
    pub(super) fn open_text_editor(&mut self) {
        let Some(id) = self.editor.selection.ids().first().cloned() else {
            return;
        };
        let Some(object) = self.editor.document.object(&id) else {
            return;
        };
        let crate::document::Shape::Text(text) = &object.shape else {
            self.status = "Select a text object to edit".to_string();
            return;
        };
        self.text_editor_draft = text.clone();
        self.text_editor_object = Some(id);
        self.text_editor_open = true;
    }

    pub(super) fn text_editor_window(&mut self, ctx: &egui::Context) {
        if !self.text_editor_open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new("Edit text")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
                ui.label("Content");
                ui.add(
                    egui::TextEdit::multiline(&mut self.text_editor_draft.content)
                        .desired_rows(4)
                        .desired_width(f32::INFINITY),
                );
                egui::Grid::new("text_editor_grid")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Font");
                        let mut font = self.text_editor_draft.font.clone();
                        if super::font_picker(ui, "text_editor_font", &mut font) {
                            self.text_editor_draft.font = font;
                        }
                        ui.end_row();
                        ui.label("Size");
                        ui.add(
                            egui::DragValue::new(&mut self.text_editor_draft.size)
                                .speed(0.1)
                                .range(0.1..=10_000.0),
                        );
                        ui.end_row();
                        ui.label("Letter spacing");
                        ui.add(
                            egui::DragValue::new(&mut self.text_editor_draft.letter_spacing)
                                .speed(0.05)
                                .range(-10.0..=50.0)
                                .suffix(" mm"),
                        );
                        ui.end_row();
                        ui.label("Line spacing");
                        ui.add(
                            egui::Slider::new(&mut self.text_editor_draft.line_spacing, 0.5..=5.0)
                                .fixed_decimals(2),
                        );
                        ui.end_row();
                        ui.label("Align");
                        let mut align = self.text_editor_draft.align;
                        egui::ComboBox::from_id_salt("text_editor_align")
                            .selected_text(align.label())
                            .show_ui(ui, |ui| {
                                for candidate in [
                                    crate::objects::TextAlign::Left,
                                    crate::objects::TextAlign::Center,
                                    crate::objects::TextAlign::Right,
                                ] {
                                    ui.selectable_value(&mut align, candidate, candidate.label());
                                }
                            });
                        self.text_editor_draft.align = align;
                        ui.end_row();
                        ui.label("Upper case");
                        ui.checkbox(&mut self.text_editor_draft.uppercase, "Force upper case");
                        ui.end_row();
                    });
                ui.weak("Shift+Enter starts a new line.");
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("OK").clicked() {
                            apply = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if apply {
            self.apply_text_editor();
        } else if cancel || !open {
            self.text_editor_open = false;
            self.text_editor_object = None;
        }
    }

    fn apply_text_editor(&mut self) {
        let Some(id) = self.text_editor_object.clone() else {
            self.text_editor_open = false;
            return;
        };
        if let Err(error) = self.text_editor_draft.validate() {
            self.status = error;
            return;
        }
        self.editor.record_action("Edit text");
        if let Some(object) = self.editor.document.object_mut(&id)
            && let crate::document::Shape::Text(text) = &mut object.shape
        {
            *text = self.text_editor_draft.clone();
        }
        self.text_editor_open = false;
        self.text_editor_object = None;
        self.status = "Text updated".to_string();
    }
}
