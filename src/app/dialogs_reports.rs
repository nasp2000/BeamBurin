#![allow(clippy::wildcard_imports)]

use super::*;

impl BeamBurinApp {
    pub(super) fn task_window(&mut self, ctx: &egui::Context) {
        let Some(task) = &self.task else {
            return;
        };
        let mut cancel = false;
        egui::Window::new("Working")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(task.label());
                });
                ui.weak("You can stop waiting; the result will be discarded.");
                if ui
                    .button("Cancel")
                    .on_hover_text("Stop waiting; the result is discarded")
                    .clicked()
                {
                    cancel = true;
                }
            });
        if cancel {
            if let Some(task) = &self.task {
                task.cancel();
                self.status = format!("Cancelled: {}", task.label());
            }
            self.task = None;
        }
    }
    pub(super) fn preview_window(&mut self, ctx: &egui::Context) {
        let mut open = self.preview_open;
        let (total, duration, position) = match &self.playback {
            Some(playback) => (
                playback.total_length,
                playback.duration_seconds,
                playback.position_at(self.playback_progress),
            ),
            None => (0.0, 0.0, None),
        };
        egui::Window::new("Preview and G-code")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .show(ctx, |ui| {
                if let Some(error) = self.playback_error.clone() {
                    ui.colored_label(
                        Color32::from_rgb(190, 60, 50),
                        format!("Cannot build the job: {error}"),
                    );
                    if ui.button("Refresh").clicked() {
                        self.refresh_preview();
                    }
                    return;
                }
                ui.horizontal(|ui| {
                    let play_label = if self.playback_playing {
                        "Pause"
                    } else {
                        "Play"
                    };
                    if ui.button(play_label).clicked() {
                        if !self.playback_playing && self.playback_progress >= total && total > 0.0
                        {
                            self.playback_progress = 0.0;
                        }
                        self.playback_playing = !self.playback_playing;
                    }
                    if ui.button("Stop").clicked() {
                        self.playback_playing = false;
                        self.playback_progress = 0.0;
                    }
                    ui.add(
                        egui::Slider::new(&mut self.playback_progress, 0.0..=total.max(0.001))
                            .show_value(false),
                    );
                    ui.label(format!("{:.1} / {:.1} mm", self.playback_progress, total));
                });
                ui.horizontal(|ui| {
                    ui.label("Speed");
                    ui.add(
                        egui::Slider::new(&mut self.playback_speed, 0.25..=16.0)
                            .logarithmic(true)
                            .suffix("x"),
                    );
                    if ui.button("Refresh").clicked() {
                        self.refresh_preview();
                    }
                });
                if let Some(position) = position {
                    ui.label(format!(
                        "Pass {} - {} - S{:.0} - ({:.2}, {:.2}) mm",
                        position.pass,
                        match position.kind {
                            crate::gcode::MoveKind::Cut => "cut",
                            crate::gcode::MoveKind::Travel => "travel",
                        },
                        position.power,
                        position.point.x,
                        position.point.y
                    ));
                } else {
                    ui.weak("The document has no toolpaths yet.");
                }
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.preview_line_numbers, "Line numbers");
                    ui.checkbox(&mut self.preview_order, "Processing order on canvas");
                });
                ui.weak(format!("Estimated job time: {duration:.1} s"));
                ui.separator();
                ui.heading("G-code");
                let mut text = if self.preview_line_numbers {
                    crate::gcode::number_lines(&self.gcode_text)
                } else {
                    self.gcode_text.clone()
                };
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut text)
                                .code_editor()
                                .desired_rows(18)
                                .desired_width(f32::INFINITY),
                        );
                    });
                if ui.button("Copy G-code").clicked() {
                    ui.ctx().copy_text(self.gcode_text.clone());
                }
            });
        self.preview_open = open;
    }

    pub(super) fn show_report(&mut self, report: FileReport) {
        self.file_report = Some(report);
        self.report_open = true;
    }
    pub(super) fn report_window(&mut self, ctx: &egui::Context) {
        let Some(report) = self.file_report.clone() else {
            self.report_open = false;
            return;
        };
        let mut open = self.report_open;
        egui::Window::new(format!("{} report", report.action))
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label(&report.path);
                egui::Grid::new("file_report_summary")
                    .num_columns(2)
                    .spacing([12.0, 4.0])
                    .show(ui, |ui| {
                        for (label, value) in &report.summary {
                            ui.label(label);
                            ui.label(value);
                            ui.end_row();
                        }
                    });
                ui.separator();
                if report.is_clean() {
                    ui.colored_label(Color32::from_rgb(40, 140, 60), "Nothing was skipped.");
                } else {
                    ui.heading(format!("Warnings ({})", report.warning_count()));
                    egui::ScrollArea::vertical()
                        .max_height(220.0)
                        .show(ui, |ui| {
                            for warning in &report.warnings {
                                ui.colored_label(Color32::from_rgb(180, 120, 0), warning);
                            }
                        });
                }
                if ui
                    .button("Close")
                    .on_hover_text("Close this window")
                    .clicked()
                {
                    self.report_open = false;
                }
            });
        self.report_open = open && self.report_open;
        if !self.report_open {
            self.file_report = None;
        }
    }

    pub(super) fn report_duplicates(&mut self) {
        let groups = self.editor.duplicate_groups();
        let mut report = FileReport::new("Duplicate geometry", "current document")
            .with_summary("Groups", groups.len().to_string());
        let mut total = 0;
        for (index, group) in groups.iter().enumerate() {
            total += group.len();
            let names: Vec<String> = group
                .iter()
                .filter_map(|id| self.editor.document.object(id))
                .map(|object| object.name.clone())
                .collect();
            report
                .warnings
                .push(format!("Group {}: {}", index + 1, names.join(", ")));
        }
        report
            .summary
            .push(("Objects".to_string(), total.to_string()));
        self.status = if groups.is_empty() {
            "No duplicate geometry".to_string()
        } else {
            format!("{} duplicate group(s)", groups.len())
        };
        self.show_report(report);
    }

    pub(super) fn report_intersections(&mut self) {
        let hits = self.editor.intersections();
        let mut report = FileReport::new("Path intersections", "current document")
            .with_summary("Intersections", hits.len().to_string());
        let name = |id: &str| {
            self.editor
                .document
                .object(id)
                .map_or_else(|| id.to_string(), |object| object.name.clone())
        };
        for (index, hit) in hits.iter().enumerate() {
            if index >= 200 {
                report.warnings.push(format!(
                    "{} more intersection(s) not listed",
                    hits.len() - index
                ));
                break;
            }
            report.warnings.push(format!(
                "{} x {} at ({:.2}, {:.2}) mm",
                name(&hit.first),
                name(&hit.second),
                hit.point.x,
                hit.point.y
            ));
        }
        if hits.is_empty() {
            report
                .summary
                .push(("Pairs".to_string(), "none".to_string()));
            self.status = "No outline intersections".to_string();
        } else {
            self.status = format!("{} outline intersection(s)", hits.len());
        }
        self.show_report(report);
    }

    pub(super) fn finish_pen(&mut self, closed: bool) {
        if self.pen_nodes.len() >= 2
            && self
                .editor
                .add_bezier_nodes(&self.pen_nodes, closed)
                .is_some()
        {
            self.status = if closed {
                "Closed Bézier path created".to_string()
            } else {
                "Bézier path created".to_string()
            };
        }
        self.pen_nodes.clear();
        self.pen_drag_start = None;
    }

    pub(super) fn apply_text_on_path(&mut self) {
        let ids = self.editor.selection.ids().to_vec();
        let text = ids.iter().find(|id| {
            self.editor
                .document
                .object(id)
                .is_some_and(|object| matches!(object.shape, crate::document::Shape::Text(_)))
        });
        let Some(text) = text else {
            self.status = "Select a text object and a path".to_string();
            return;
        };
        let path = ids.iter().find(|id| *id != text);
        let Some(path) = path else {
            self.status = "Select a text object and a path".to_string();
            return;
        };
        match self.editor.fit_text_to_path(text, path) {
            Ok(outlines) => {
                self.status = format!("Fitted text to path ({} contour(s))", outlines.len());
            }
            Err(error) => self.status = format!("Fit failed: {error}"),
        }
    }

    pub(super) fn apply_text_outlines(&mut self) {
        let Some(id) = self.editor.selection.ids().first().cloned() else {
            return;
        };
        let font = self.text_font_of(&id);
        let data = crate::textfont::read(&font);
        match self
            .editor
            .convert_text_to_outlines_with_font(&id, Some(&data))
        {
            Ok(outlines) => {
                self.status = format!("Converted text to {} outline(s)", outlines.len());
            }
            Err(error) => self.status = format!("Text conversion failed: {error}"),
        }
    }

    pub(super) fn text_font_of(&self, id: &str) -> String {
        self.editor
            .document
            .objects
            .iter()
            .find(|object| object.id == id)
            .and_then(|object| match &object.shape {
                Shape::Text(text) => Some(text.font.clone()),
                _ => None,
            })
            .unwrap_or_else(|| crate::textfont::BUNDLED_NAME.to_string())
    }

    pub(super) fn apply_trim(&mut self) {
        match self.editor.trim_selection() {
            Ok(count) => self.status = format!("Trimmed {count} object(s)"),
            Err(error) => self.status = format!("Trim failed: {error}"),
        }
    }

    pub(super) fn apply_delete_overlaps(&mut self) {
        match self.editor.delete_overlapping_segments() {
            Ok(count) => self.status = format!("Removed overlaps in {count} object(s)"),
            Err(error) => self.status = format!("Overlap removal failed: {error}"),
        }
    }

    pub(super) fn apply_outline_stroke(&mut self) {
        match self.editor.outline_selection() {
            Ok(count) => self.status = format!("Created {count} outline contour(s)"),
            Err(error) => self.status = format!("Outline failed: {error}"),
        }
    }

    pub(super) fn apply_join_paths(&mut self) {
        match self.editor.join_paths() {
            Ok(_) => self.status = "Paths joined".to_string(),
            Err(error) => self.status = format!("Join failed: {error}"),
        }
    }

    pub(super) fn apply_close_paths(&mut self) {
        match self.editor.close_paths() {
            Ok(count) => self.status = format!("Closed {count} path(s)"),
            Err(error) => self.status = format!("Close failed: {error}"),
        }
    }

    pub(super) fn apply_boolean(&mut self, op: crate::geometry::BooleanOp) {
        match self.editor.boolean_selection(op) {
            Ok(count) => {
                self.status = if count == 1 {
                    format!("{} applied", op.label())
                } else {
                    format!("{} applied ({count} shapes)", op.label())
                };
            }
            Err(error) => self.status = format!("{} failed: {error}", op.label()),
        }
    }
}
