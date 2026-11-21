#![allow(clippy::wildcard_imports)]

use super::*;

impl BeamBurinApp {
    /// Open the trace options for one image, or for the selected image.
    pub(super) fn open_trace_image(&mut self, id: Option<String>) {
        let target = id.or_else(|| {
            self.editor.selection.ids().iter().find_map(|id| {
                self.editor
                    .document
                    .object(id)
                    .filter(|object| matches!(object.shape, Shape::Image(_)))
                    .map(|_| id.clone())
            })
        });
        let Some(target) = target else {
            self.status = "Select one image to trace".to_string();
            return;
        };
        self.trace_target = Some(target);
        self.trace_open = true;
    }

    /// Options for tracing one image into outlines or centrelines.
    pub(super) fn trace_window(&mut self, ctx: &egui::Context) {
        let mut open = self.trace_open;
        let mut trace = false;
        let mut cancel = false;
        let mut options = self.trace_options;
        let mut delete_source = self.trace_delete_source;
        let mut use_image_settings = !matches!(
            options.threshold,
            crate::trace::ThresholdSource::Custom { .. }
        );
        let (mut low, mut high, mut invert) = match options.threshold {
            crate::trace::ThresholdSource::Custom { low, high, invert } => (low, high, invert),
            crate::trace::ThresholdSource::ImageSettings => (0, 255, false),
        };
        egui::Window::new("Trace image")
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
                ui.label(
                    "Convert the selected image into vector paths. Outlines follow the edges; \
                     centreline traces along the strokes.",
                );
                egui::Grid::new("trace_mode")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Mode");
                        ui.horizontal(|ui| {
                            let mut outline_mode = options.mode == crate::trace::TraceMode::Outline;
                            ui.selectable_value(&mut outline_mode, true, "Outline")
                                .on_hover_text("Follow the edge of every filled area");
                            ui.selectable_value(&mut outline_mode, false, "Centerline")
                                .on_hover_text("Trace along the middle of the strokes");
                            options.mode = if outline_mode {
                                crate::trace::TraceMode::Outline
                            } else {
                                crate::trace::TraceMode::Centerline
                            };
                        });
                        ui.end_row();
                        ui.label("Brightness");
                        ui.checkbox(&mut use_image_settings, "Use the image settings")
                            .on_hover_text("Trace the tones the image already burns");
                        ui.end_row();
                    });
                if !use_image_settings {
                    egui::Grid::new("trace_band")
                        .num_columns(2)
                        .spacing([12.0, 6.0])
                        .show(ui, |ui| {
                            let low_label = ui.label("Lowest brightness");
                            ui.add(egui::Slider::new(&mut low, 0..=255))
                                .labelled_by(low_label.id)
                                .on_hover_text("Darkest tone to include in the trace");
                            ui.end_row();
                            let high_label = ui.label("Highest brightness");
                            ui.add(egui::Slider::new(&mut high, 0..=255))
                                .labelled_by(high_label.id)
                                .on_hover_text("Brightest tone to include in the trace");
                            ui.end_row();
                            ui.label("Negative");
                            ui.checkbox(&mut invert, "")
                                .on_hover_text("Swap the traced and the ignored tones")
                                .widget_info(|| {
                                    egui::WidgetInfo::selected(
                                        egui::WidgetType::Checkbox,
                                        true,
                                        invert,
                                        "Negative",
                                    )
                                });
                            ui.end_row();
                        });
                    if low > high {
                        std::mem::swap(&mut low, &mut high);
                    }
                }
                options.threshold = if use_image_settings {
                    crate::trace::ThresholdSource::ImageSettings
                } else {
                    crate::trace::ThresholdSource::Custom { low, high, invert }
                };
                let outline = options.mode == crate::trace::TraceMode::Outline;
                egui::Grid::new("trace_shape")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Smoothing");
                        ui.checkbox(&mut options.smooth, "Smooth shapes")
                            .on_hover_text("Fit curves through the traced points");
                        ui.end_row();
                        let tolerance_label = ui.label("Curve tolerance (mm)");
                        ui.add(
                            egui::DragValue::new(&mut options.tolerance_mm)
                                .speed(0.01)
                                .range(0.02..=2.0)
                                .fixed_decimals(2)
                                .suffix(" mm"),
                        )
                        .labelled_by(tolerance_label.id)
                        .on_hover_text("Distance the traced points may move");
                        ui.end_row();
                        if outline {
                            let corner_label = ui.label("Corner angle");
                            ui.add(
                                egui::DragValue::new(&mut options.corner_degrees)
                                    .speed(1.0)
                                    .range(10.0..=90.0)
                                    .suffix(" deg"),
                            )
                            .labelled_by(corner_label.id)
                            .on_hover_text("Turns sharper than this stay as corners");
                            ui.end_row();
                            let area_label = ui.label("Ignore areas under (mm2)");
                            ui.add(
                                egui::DragValue::new(&mut options.min_area_mm2)
                                    .speed(0.5)
                                    .range(0.0..=1000.0)
                                    .fixed_decimals(2),
                            )
                            .labelled_by(area_label.id)
                            .on_hover_text("Drop speckles smaller than this");
                            ui.end_row();
                            let holes_label = ui.label("Ignore holes under (mm2)");
                            ui.add(
                                egui::DragValue::new(&mut options.min_hole_mm2)
                                    .speed(0.5)
                                    .range(0.0..=1000.0)
                                    .fixed_decimals(2),
                            )
                            .labelled_by(holes_label.id)
                            .on_hover_text("Fill in holes smaller than this");
                            ui.end_row();
                        }
                        ui.label("Transparency");
                        ui.checkbox(&mut options.transparency, "Trace the alpha channel")
                            .on_hover_text(
                                "Trace a transparent PNG by its alpha channel instead of its tones",
                            );
                        ui.end_row();
                        ui.label("Source");
                        ui.checkbox(&mut delete_source, "Delete the image after tracing")
                            .on_hover_text("Remove the source image once the paths exist");
                        ui.end_row();
                    });
                if let Err(error) = options.validate() {
                    ui.colored_label(egui::Color32::from_rgb(200, 60, 60), error);
                }
                ui.horizontal(|ui| {
                    let valid = options.validate().is_ok();
                    if ui
                        .add_enabled(valid, egui::Button::new("Trace"))
                        .on_hover_text("Create the vector paths")
                        .clicked()
                    {
                        trace = true;
                    }
                    if ui
                        .button("Cancel")
                        .on_hover_text("Close without tracing")
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });
        self.trace_options = options;
        self.trace_delete_source = delete_source;
        if cancel {
            self.trace_open = false;
            return;
        }
        if trace {
            let Some(id) = self.trace_target.clone() else {
                self.trace_open = false;
                return;
            };
            match self
                .editor
                .trace_image(Some(&id), None, &options, delete_source)
            {
                Ok(report) => {
                    self.status = match options.mode {
                        crate::trace::TraceMode::Outline => format!(
                            "Traced {} contour(s) with {} hole(s)",
                            report.contours, report.holes
                        ),
                        crate::trace::TraceMode::Centerline => {
                            format!("Traced {} centreline(s)", report.contours)
                        }
                    };
                    self.trace_open = false;
                }
                Err(error) => self.status = error,
            }
        }
        self.trace_open = open && self.trace_open;
    }
}
