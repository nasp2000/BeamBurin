#![allow(clippy::wildcard_imports)]

use super::*;

impl BeamBurinApp {
    pub(super) fn side_panel(&mut self, ui: &mut Ui) {
        egui::Panel::right("side_panel")
            .default_size(300.0)
            .resizable(true)
            .show(ui, |ui| {
                egui::TopBottomPanel::bottom("side_tabs")
                    .exact_height(28.0)
                    .show_inside(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            if icons::selectable(
                                ui,
                                self.side_panel_tab == SidePanelTab::Properties,
                                Icon::Preferences,
                                "Properties",
                            )
                            .clicked()
                            {
                                self.side_panel_tab = SidePanelTab::Properties;
                            }
                            if icons::selectable(
                                ui,
                                self.side_panel_tab == SidePanelTab::Tool,
                                Icon::Pencil,
                                "Tool",
                            )
                            .clicked()
                            {
                                self.side_panel_tab = SidePanelTab::Tool;
                            }
                        });
                    });
                ui.separator();
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 3.0);
                match self.side_panel_tab {
                    SidePanelTab::Properties => self.properties_contents(ui),
                    SidePanelTab::Tool => self.tool_contents(ui),
                }
            });
    }

    pub(super) fn properties_contents(&mut self, ui: &mut Ui) {
        let before = self.editor.document.clone();
        egui::ScrollArea::vertical().show(ui, |ui| {
            if self.editor.selection.is_empty() {
                self.document_properties(ui);
            } else if self.editor.selection.len() == 1 {
                let id = self.editor.selection.ids()[0].clone();
                self.object_properties(ui, &id);
            } else {
                self.multi_selection_properties(ui);
            }
            if self.editor.is_editing_nodes() {
                ui.separator();
                self.node_properties(ui);
            }
        });
        if self.editor.document != before {
            self.editor.history.record(&before, "Edit properties");
        }
    }

    pub(super) fn document_properties(&mut self, ui: &mut Ui) {
        ui.label("Document");
        egui::Grid::new("document_properties")
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut self.editor.document.name);
                ui.end_row();

                ui.label("Width (mm)");
                let mut width = self.editor.document.width_mm;
                if ui
                    .add(
                        egui::DragValue::new(&mut width)
                            .speed(0.5)
                            .range(0.1..=10000.0),
                    )
                    .changed()
                {
                    self.editor.document.width_mm = width;
                }
                ui.end_row();

                ui.label("Height (mm)");
                let mut height = self.editor.document.height_mm;
                if ui
                    .add(
                        egui::DragValue::new(&mut height)
                            .speed(0.5)
                            .range(0.1..=10000.0),
                    )
                    .changed()
                {
                    self.editor.document.height_mm = height;
                }
                ui.end_row();

                ui.label("Unit");
                egui::ComboBox::from_id_salt("document_unit")
                    .selected_text(self.editor.document.unit.label())
                    .show_ui(ui, |ui| {
                        for unit in [
                            Unit::Millimeters,
                            Unit::Centimeters,
                            Unit::Inches,
                            Unit::Pixels,
                        ] {
                            ui.selectable_value(&mut self.editor.document.unit, unit, unit.label());
                        }
                    });
                ui.end_row();
            });
        ui.separator();
        ui.label("No object selected.");
        ui.label("Drag a rectangle, ellipse or line on the canvas.");
    }

    pub(super) fn object_properties(&mut self, ui: &mut Ui, id: &str) {
        let mut relink_request = false;
        let mut select_same_color: Option<String> = None;
        let layers: Vec<(String, String)> = self
            .editor
            .document
            .layers
            .iter()
            .map(|layer| (layer.id.clone(), layer.name.clone()))
            .collect();
        let Some(object) = self.editor.document.object_mut(id) else {
            return;
        };
        ui.label(object.shape.kind_name());
        egui::Grid::new("object_properties")
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                ui.label("Name");
                ui.text_edit_singleline(&mut object.name);
                ui.end_row();

                ui.label("Layer");
                let current = layers
                    .iter()
                    .find(|(layer_id, _)| layer_id == &object.layer_id)
                    .map_or("?", |(_, name)| name.as_str());
                egui::ComboBox::from_id_salt("object_layer")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        for (layer_id, name) in &layers {
                            ui.selectable_value(&mut object.layer_id, layer_id.clone(), name);
                        }
                    });
                ui.end_row();

                ui.label("Visible");
                ui.checkbox(&mut object.visible, "");
                ui.end_row();

                ui.label("Locked");
                ui.checkbox(&mut object.locked, "");
                ui.end_row();
            });
        ui.separator();

        if let Some((min, max)) = object.bounds() {
            let link_proportions = self.link_proportions;
            ui.label("Position (mm)");
            egui::Grid::new("object_position")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    let step = |ui: &egui::Ui| {
                        if ui.input(|input| input.modifiers.shift) {
                            0.1
                        } else {
                            1.0
                        }
                    };
                    ui.label("X");
                    ui.horizontal(|ui| {
                        let step = step(ui);
                        if ui
                            .small_button("-")
                            .on_hover_text("Move left 1 mm (Shift: 0.1 mm)")
                            .clicked()
                        {
                            object.translate(-step, 0.0);
                        }
                        let mut x = min.x;
                        if ui.add(egui::DragValue::new(&mut x).speed(0.1)).changed() {
                            object.translate(x - min.x, 0.0);
                        }
                        if ui
                            .small_button("+")
                            .on_hover_text("Move right 1 mm (Shift: 0.1 mm)")
                            .clicked()
                        {
                            object.translate(step, 0.0);
                        }
                    });
                    ui.end_row();
                    ui.label("Y");
                    ui.horizontal(|ui| {
                        let step = step(ui);
                        if ui
                            .small_button("-")
                            .on_hover_text("Move down 1 mm (Shift: 0.1 mm)")
                            .clicked()
                        {
                            object.translate(0.0, -step);
                        }
                        let mut y = min.y;
                        if ui.add(egui::DragValue::new(&mut y).speed(0.1)).changed() {
                            object.translate(0.0, y - min.y);
                        }
                        if ui
                            .small_button("+")
                            .on_hover_text("Move up 1 mm (Shift: 0.1 mm)")
                            .clicked()
                        {
                            object.translate(0.0, step);
                        }
                    });
                    ui.end_row();
                    ui.label("Width");
                    ui.horizontal(|ui| {
                        let step = step(ui);
                        let resize = |object: &mut crate::document::Object, delta: f64| {
                            let old = (max.x - min.x).max(1e-9);
                            let width = (old + delta).max(0.01);
                            let factor = width / old;
                            let vertical = if link_proportions { factor } else { 1.0 };
                            let center = Point2::new(
                                f64::midpoint(min.x, max.x),
                                f64::midpoint(min.y, max.y),
                            );
                            object.transform = egui_transform_scale(factor, vertical, center)
                                .compose(&object.transform);
                        };
                        if ui
                            .small_button("-")
                            .on_hover_text("Narrower 1 mm (Shift: 0.1 mm)")
                            .clicked()
                        {
                            resize(object, -step);
                        }
                        let mut width = max.x - min.x;
                        if ui
                            .add(
                                egui::DragValue::new(&mut width)
                                    .speed(0.1)
                                    .range(0.01..=100_000.0),
                            )
                            .changed()
                        {
                            let factor = width / (max.x - min.x).max(1e-9);
                            let vertical = if link_proportions { factor } else { 1.0 };
                            let center = Point2::new(
                                f64::midpoint(min.x, max.x),
                                f64::midpoint(min.y, max.y),
                            );
                            object.transform = egui_transform_scale(factor, vertical, center)
                                .compose(&object.transform);
                        }
                        if ui
                            .small_button("+")
                            .on_hover_text("Wider 1 mm (Shift: 0.1 mm)")
                            .clicked()
                        {
                            resize(object, step);
                        }
                    });
                    ui.end_row();
                    ui.label("Height");
                    ui.horizontal(|ui| {
                        let step = step(ui);
                        let resize = |object: &mut crate::document::Object, delta: f64| {
                            let old = (max.y - min.y).max(1e-9);
                            let height = (old + delta).max(0.01);
                            let factor = height / old;
                            let horizontal = if link_proportions { factor } else { 1.0 };
                            let center = Point2::new(
                                f64::midpoint(min.x, max.x),
                                f64::midpoint(min.y, max.y),
                            );
                            object.transform = egui_transform_scale(horizontal, factor, center)
                                .compose(&object.transform);
                        };
                        if ui
                            .small_button("-")
                            .on_hover_text("Shorter 1 mm (Shift: 0.1 mm)")
                            .clicked()
                        {
                            resize(object, -step);
                        }
                        let mut height = max.y - min.y;
                        if ui
                            .add(
                                egui::DragValue::new(&mut height)
                                    .speed(0.1)
                                    .range(0.01..=100_000.0),
                            )
                            .changed()
                        {
                            let factor = height / (max.y - min.y).max(1e-9);
                            let horizontal = if link_proportions { factor } else { 1.0 };
                            let center = Point2::new(
                                f64::midpoint(min.x, max.x),
                                f64::midpoint(min.y, max.y),
                            );
                            object.transform = egui_transform_scale(horizontal, factor, center)
                                .compose(&object.transform);
                        }
                        if ui
                            .small_button("+")
                            .on_hover_text("Taller 1 mm (Shift: 0.1 mm)")
                            .clicked()
                        {
                            resize(object, step);
                        }
                    });
                    ui.end_row();
                    ui.label("Proportions");
                    ui.checkbox(&mut self.link_proportions, "Link width and height");
                    ui.end_row();
                });
            ui.separator();
        }

        match &mut object.shape {
            Shape::Rect {
                width,
                height,
                corner_radius,
            } => {
                ui.label("Rectangle");
                egui::Grid::new("object_rect")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Width");
                        ui.add(
                            egui::DragValue::new(width)
                                .speed(0.1)
                                .range(0.01..=100_000.0),
                        );
                        ui.end_row();
                        ui.label("Height");
                        ui.add(
                            egui::DragValue::new(height)
                                .speed(0.1)
                                .range(0.01..=100_000.0),
                        );
                        ui.end_row();
                        ui.label("Corner radius");
                        ui.add(
                            egui::DragValue::new(corner_radius)
                                .speed(0.1)
                                .range(0.0..=100_000.0),
                        );
                        ui.end_row();
                    });
            }
            Shape::Ellipse { rx, ry } => {
                ui.label("Ellipse");
                egui::Grid::new("object_ellipse")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Radius X");
                        ui.add(egui::DragValue::new(rx).speed(0.1).range(0.005..=100_000.0));
                        ui.end_row();
                        ui.label("Radius Y");
                        ui.add(egui::DragValue::new(ry).speed(0.1).range(0.005..=100_000.0));
                        ui.end_row();
                    });
            }
            Shape::Path(path) => {
                ui.label(format!("Path ({} segments)", path.segment_count()));
            }
            Shape::Text(text) => {
                ui.label("Text");
                egui::Grid::new("object_text")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Content");
                        ui.text_edit_singleline(&mut text.content);
                        ui.end_row();
                        ui.label("Font");
                        let mut font = text.font.clone();
                        if font_picker(ui, "object_text_font", &mut font) {
                            text.font.clone_from(&font);
                            self.preferences.general.text_font = font;
                        }
                        ui.end_row();
                        ui.label("Size");
                        ui.add(
                            egui::DragValue::new(&mut text.size)
                                .speed(0.1)
                                .range(0.1..=10000.0),
                        );
                        ui.end_row();
                        ui.label("Letter spacing");
                        ui.add(
                            egui::DragValue::new(&mut text.letter_spacing)
                                .speed(0.05)
                                .range(-10.0..=50.0)
                                .suffix(" mm"),
                        );
                        ui.end_row();
                        ui.label("Line spacing");
                        ui.add(
                            egui::Slider::new(&mut text.line_spacing, 0.5..=5.0).fixed_decimals(2),
                        );
                        ui.end_row();
                        ui.label("Align");
                        let mut align = text.align;
                        egui::ComboBox::from_id_salt("text_align")
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
                        text.align = align;
                        ui.end_row();
                    });
                ui.weak("Use Shift+Enter in the content field for a second line.");
            }
            Shape::Image(image) => {
                use crate::objects::ImageMode;
                ui.label("Image");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut image.path);
                    if ui.button("Relink...").clicked() {
                        relink_request = true;
                    }
                });
                ui.weak(format!(
                    "{:.1} x {:.1} mm, source {} x {} px at {:.0} dpi",
                    image.width_mm(),
                    image.height_mm(),
                    image.width_px,
                    image.height_px,
                    image.dpi
                ));
                let (work_width, work_height) = image.effective_size_px();
                ui.weak(format!(
                    "Working: {work_width} x {work_height} px, {:.0} dpi effective",
                    image.effective_dpi()
                ));
                if image.effective_dpi() < 100.0 {
                    ui.colored_label(
                        Color32::from_rgb(180, 120, 0),
                        "Low working resolution: details may be lost.",
                    );
                }
                ui.separator();
                ui.label("Transform (non-destructive)");
                ui.horizontal(|ui| {
                    ui.label("Rotation");
                    let mut rotation = image.settings.rotation_degrees;
                    egui::ComboBox::from_id_salt("image_rotation")
                        .selected_text(format!("{rotation:.0} deg"))
                        .show_ui(ui, |ui| {
                            for candidate in [0.0, 90.0, 180.0, 270.0] {
                                ui.selectable_value(
                                    &mut rotation,
                                    candidate,
                                    format!("{candidate:.0} deg"),
                                );
                            }
                        });
                    image.settings.rotation_degrees = rotation;
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut image.settings.flip_horizontal, "Flip horizontal");
                    ui.checkbox(&mut image.settings.flip_vertical, "Flip vertical");
                });
                ui.horizontal(|ui| {
                    ui.label("Working size");
                    ui.add(
                        egui::DragValue::new(&mut image.settings.target_width_px)
                            .speed(1.0)
                            .range(0..=20000)
                            .suffix(" px X"),
                    );
                    ui.add(
                        egui::DragValue::new(&mut image.settings.target_height_px)
                            .speed(1.0)
                            .range(0..=20000)
                            .suffix(" px Y"),
                    );
                });
                ui.weak("0 x 0 keeps the source resolution.");
                let mut crop_x = image.crop.map_or(0, |crop| crop.x);
                let mut crop_y = image.crop.map_or(0, |crop| crop.y);
                let mut crop_w = image.crop.map_or(image.width_px, |crop| crop.width);
                let mut crop_h = image.crop.map_or(image.height_px, |crop| crop.height);
                ui.horizontal(|ui| {
                    ui.label("Crop");
                    ui.add(egui::DragValue::new(&mut crop_x).speed(1.0).prefix("X "));
                    ui.add(egui::DragValue::new(&mut crop_y).speed(1.0).prefix("Y "));
                    ui.add(egui::DragValue::new(&mut crop_w).speed(1.0).prefix("W "));
                    ui.add(egui::DragValue::new(&mut crop_h).speed(1.0).prefix("H "));
                });
                ui.horizontal(|ui| {
                    if ui.button("Apply crop").clicked() {
                        image.crop = Some(crate::objects::ImageCrop {
                            x: crop_x,
                            y: crop_y,
                            width: crop_w,
                            height: crop_h,
                        });
                    }
                    if ui.button("Clear crop").clicked() {
                        image.crop = None;
                    }
                });
                if let Some(crop) = image.crop {
                    ui.weak(format!(
                        "Cropped to {} x {} px at ({}, {})",
                        crop.width, crop.height, crop.x, crop.y
                    ));
                }
                ui.separator();
                ui.label("Preparation (used by G-code export)");
                let mut mode = image.settings.mode;
                egui::ComboBox::from_label("Mode")
                    .selected_text(mode.label())
                    .show_ui(ui, |ui| {
                        for candidate in ImageMode::all() {
                            ui.selectable_value(&mut mode, candidate, candidate.label());
                        }
                    });
                image.settings.mode = mode;
                ui.add(egui::Slider::new(&mut image.settings.threshold, 0..=255).text("Threshold"));
                ui.add(
                    egui::Slider::new(&mut image.settings.brightness, -100..=100)
                        .text("Brightness"),
                );
                ui.add(
                    egui::Slider::new(&mut image.settings.contrast, 0.25..=3.0).text("Contrast"),
                );
                ui.add(egui::Slider::new(&mut image.settings.gamma, 0.2..=3.0).text("Gamma"));
                ui.checkbox(&mut image.settings.invert, "Invert");
                ui.add(
                    egui::DragValue::new(&mut image.settings.line_interval_mm)
                        .speed(0.01)
                        .range(0.01..=2.0)
                        .suffix(" mm line interval"),
                );
                if matches!(image.settings.mode, ImageMode::Sketch) {
                    ui.weak("In sketch mode the threshold sets the edge strength.");
                }
                if let Err(error) = image.settings.validate() {
                    ui.colored_label(Color32::from_rgb(200, 60, 60), error);
                }
                if ui.button("Reset preparation").clicked() {
                    image.settings = crate::objects::ImageSettings::default();
                }
            }
        }
        ui.separator();
        ui.label("Style");
        egui::Grid::new("object_style")
            .num_columns(2)
            .spacing([8.0, 6.0])
            .show(ui, |ui| {
                ui.label("Stroke width");
                ui.add(
                    egui::DragValue::new(&mut object.style.stroke_width_mm)
                        .speed(0.01)
                        .range(0.0..=100.0),
                );
                ui.end_row();
                ui.label("Stroke");
                edit_color(ui, &mut object.style.stroke);
                ui.end_row();
                ui.label("Fill");
                edit_color(ui, &mut object.style.fill);
                ui.end_row();
            });
        ui.horizontal(|ui| {
            if let Some(color) = object.style.stroke.clone() {
                if ui.button("Select same stroke").clicked() {
                    select_same_color = Some(color);
                }
            }
            if let Some(color) = object.style.fill.clone() {
                if ui.button("Select same fill").clicked() {
                    select_same_color = Some(color);
                }
            }
        });
        if let Some(color) = select_same_color {
            self.editor.select_by_color(&color);
        }
        if relink_request {
            self.relink_target = Some(id.to_string());
            self.open_dialog(FileMode::RelinkImage);
        }
    }

    pub(super) fn node_properties(&mut self, ui: &mut Ui) {
        ui.heading("Path nodes");
        let count = self.editor.node_points().len();
        ui.label(format!(
            "{count} node(s), {} selected",
            self.editor.node_selection.len()
        ));
        if self.editor.node_selection.len() == 1 {
            let index = self.editor.node_selection[0];
            if let Some(point) = self.editor.node_points().get(index).copied() {
                let mut x = point.x;
                let mut y = point.y;
                ui.horizontal(|ui| {
                    ui.label("X");
                    if ui
                        .add(egui::DragValue::new(&mut x).speed(0.1).suffix(" mm"))
                        .changed()
                    {
                        self.editor
                            .set_selected_node_position(Point2::new(x, point.y));
                    }
                    ui.label("Y");
                    if ui
                        .add(egui::DragValue::new(&mut y).speed(0.1).suffix(" mm"))
                        .changed()
                    {
                        self.editor
                            .set_selected_node_position(Point2::new(point.x, y));
                    }
                });
            }
        }
        let has_nodes = !self.editor.node_selection.is_empty();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(has_nodes, egui::Button::new("Corner"))
                .clicked()
            {
                self.editor.convert_selected_nodes(false);
            }
            if ui
                .add_enabled(has_nodes, egui::Button::new("Smooth"))
                .clicked()
            {
                self.editor.convert_selected_nodes(true);
            }
            if ui
                .add_enabled(has_nodes, egui::Button::new("Delete node"))
                .clicked()
            {
                let removed = self.editor.delete_selected_nodes();
                self.status = format!("Deleted {removed} node(s)");
            }
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(has_nodes, egui::Button::new("Align handles"))
                .clicked()
            {
                let aligned = self.editor.align_selected_node_handles();
                self.status = format!("Aligned {aligned} node handle(s)");
            }
            if ui
                .add_enabled(
                    self.editor.node_selection.len() == 1,
                    egui::Button::new("Open at node"),
                )
                .clicked()
            {
                self.editor.open_path_at_selected_node();
            }
            if ui
                .add_enabled(
                    self.editor.node_selection.len() == 1,
                    egui::Button::new("Split at node"),
                )
                .clicked()
                && self.editor.split_path_at_selected_node().is_some()
            {
                self.status = "Path split".to_string();
            }
        });
        ui.weak("Double-click a segment to insert a node.");
    }

    pub(super) fn multi_selection_properties(&mut self, ui: &mut Ui) {
        ui.label(format!("{} objects selected", self.editor.selection.len()));
        if let Some((min, max)) = self.editor.selected_bounds() {
            let mut left = min.x;
            let mut bottom = min.y;
            let mut width = max.x - min.x;
            let mut height = max.y - min.y;
            let mut changed = false;
            egui::Grid::new("multi_bounds")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Left");
                    changed |= ui
                        .add(egui::DragValue::new(&mut left).speed(0.1).suffix(" mm"))
                        .changed();
                    ui.end_row();
                    ui.label("Bottom");
                    changed |= ui
                        .add(egui::DragValue::new(&mut bottom).speed(0.1).suffix(" mm"))
                        .changed();
                    ui.end_row();
                    ui.label("Width");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut width)
                                .speed(0.1)
                                .range(0.01..=100_000.0)
                                .suffix(" mm"),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Height");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut height)
                                .speed(0.1)
                                .range(0.01..=100_000.0)
                                .suffix(" mm"),
                        )
                        .changed();
                    ui.end_row();
                });
            if changed {
                if !self.multi_props_recorded {
                    self.editor.record_action("Multi-selection transform");
                    self.multi_props_recorded = true;
                }
                let dx = left - min.x;
                let dy = bottom - min.y;
                if dx.abs() > 1e-9 || dy.abs() > 1e-9 {
                    self.editor.move_selection(dx, dy);
                }
                if (width - (max.x - min.x)).abs() > 1e-9 || (height - (max.y - min.y)).abs() > 1e-9
                {
                    self.editor.set_selection_size(width, height, false);
                }
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Duplicate").clicked() {
                self.editor.duplicate_selection();
            }
            if ui.button("Delete").clicked() {
                self.editor.delete_selection();
            }
        });
    }
}
