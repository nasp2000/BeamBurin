#![allow(clippy::wildcard_imports)]

use super::*;

impl BeamBurinApp {
    pub(super) fn canvas(&mut self, ui: &mut Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
                if rect.width() < 8.0 || rect.height() < 8.0 {
                    return;
                }
                if self.needs_fit {
                    self.editor
                        .fit_view(f64::from(rect.width()), f64::from(rect.height()));
                    self.needs_fit = false;
                }
                if self.zoom_to_selection_request {
                    self.editor
                        .zoom_to_selection(f64::from(rect.width()), f64::from(rect.height()));
                    self.zoom_to_selection_request = false;
                }
                let ctx = ui.ctx().clone();
                let modal = self.dialog.is_some()
                    || self.confirm.is_some()
                    || self.about_open
                    || self.licenses_open
                    || self.task.is_some()
                    || self.machines_open
                    || self.machine_wizard_open
                    || self.cuts_open
                    || self.palette_open;
                if modal {
                    // A window opened mid-drag must not leave the drag alive:
                    // its release would be ignored and the canvas would keep
                    // asking for repaints.
                    self.drag = None;
                }
                let pointer = if modal {
                    None
                } else {
                    ctx.input(|input| input.pointer.interact_pos())
                };
                // Menus, windows and tooltips live in their own layers above the
                // canvas. A click on one of them must not reach the canvas even
                // when it happens to fall inside the canvas rectangle.
                let pointer_over_window = pointer.is_some_and(|pos| {
                    ctx.layer_id_at(pos)
                        .is_some_and(|layer| layer.order != egui::Order::Background)
                });
                let primary_pressed = !modal
                    && !pointer_over_window
                    && ctx.input(|input| input.pointer.primary_pressed());
                let primary_released =
                    !modal && ctx.input(|input| input.pointer.primary_released());
                let shift = ctx.input(|input| input.modifiers.shift);

                if let Some(pos) = pointer {
                    let (x, y) = self.editor.canvas.screen_to_doc(
                        f64::from(pos.x - rect.min.x),
                        f64::from(pos.y - rect.min.y),
                    );
                    self.cursor_doc = Some(Point2::new(x, y));
                }

                let pointer_in_canvas =
                    pointer.is_some_and(|pos| rect.contains(pos) && !pointer_over_window);
                if pointer_in_canvas {
                    let scroll = ctx.input(|input| input.smooth_scroll_delta.y);
                    if scroll.abs() > 0.0 {
                        if let Some(pos) = pointer {
                            let step = if self.editor.scroll_zoom_step > 1.0 {
                                self.editor.scroll_zoom_step
                            } else {
                                1.15
                            };
                            let factor = if scroll > 0.0 { step } else { 1.0 / step };
                            self.editor.canvas.zoom_at(
                                f64::from(pos.x - rect.min.x),
                                f64::from(pos.y - rect.min.y),
                                factor,
                            );
                        }
                    }
                }

                if pointer_in_canvas
                    && ctx.input(|input| input.pointer.button_down(egui::PointerButton::Middle))
                {
                    let delta = ctx.input(|input| input.pointer.delta());
                    self.editor
                        .canvas
                        .pan(f64::from(delta.x), f64::from(delta.y));
                }

                if self.editor.tool != Tool::Bezier && !self.pen_nodes.is_empty() {
                    self.pen_nodes.clear();
                    self.pen_drag_start = None;
                }
                if self.editor.tool == Tool::Bezier && !modal {
                    if primary_pressed {
                        if let Some(pos) =
                            pointer.filter(|pos| rect.contains(*pos) && !pointer_over_window)
                        {
                            let point = self.doc_point(rect, pos);
                            let tolerance = 8.0 / self.editor.canvas.zoom.max(0.01);
                            if self.pen_nodes.len() >= 2
                                && self.pen_nodes[0].point.distance(point) <= tolerance
                            {
                                self.finish_pen(true);
                            } else {
                                self.pen_drag_start = Some(point);
                            }
                        }
                    } else if primary_released {
                        if let Some(start) = self.pen_drag_start.take() {
                            if let Some(pos) =
                                pointer.filter(|pos| rect.contains(*pos) && !pointer_over_window)
                            {
                                let point = self.doc_point(rect, pos);
                                let (handle_in, handle_out) = if start.distance(point) > 0.5 {
                                    (
                                        Some(Point2::new(
                                            2.0 * start.x - point.x,
                                            2.0 * start.y - point.y,
                                        )),
                                        Some(point),
                                    )
                                } else {
                                    (None, None)
                                };
                                self.pen_nodes.push(crate::geometry::path::Node {
                                    point: start,
                                    handle_in,
                                    handle_out,
                                });
                                self.status =
                                    "Pen: click to add nodes, Enter to finish, Esc to cancel"
                                        .to_string();
                            }
                        }
                    }
                }

                if primary_pressed && pointer_in_canvas {
                    response.request_focus();
                    if let Some(pos) = pointer {
                        let point = self.doc_point(rect, pos);
                        if self.editor.is_editing_nodes() {
                            self.begin_node_interaction(point, shift);
                            if primary_released {
                                self.drag = None;
                            }
                        } else if self.editor.tool == Tool::Select
                            && let Some(kind) = self.selection_handle_at(rect, pos)
                        {
                            let centre = self.selection_doc_centre().unwrap_or(point);
                            self.drag = Some(DragState::Transform {
                                kind,
                                last: point,
                                centre,
                                recorded: false,
                                constrain: shift,
                            });
                        } else {
                            match self.editor.tool {
                                Tool::Select => self.begin_select(point, shift),
                                Tool::Bezier => {}
                                Tool::Rect
                                | Tool::Ellipse
                                | Tool::Line
                                | Tool::Polygon
                                | Tool::Star
                                | Tool::Text
                                | Tool::Measure
                                | Tool::Pencil => {
                                    let constrain = shift
                                        || (self.editor.tool == Tool::Line && self.line_angle_snap);
                                    if self.editor.layer_locked(&self.editor.active_layer_id)
                                        || !self.editor.layer_visible(&self.editor.active_layer_id)
                                    {
                                        self.status =
                                            "Cannot draw on a hidden or locked layer".to_string();
                                    } else if self.editor.tool == Tool::Pencil {
                                        self.drag = Some(DragState::Sketch {
                                            points: vec![point],
                                        });
                                    } else {
                                        self.drag = Some(DragState::Draw {
                                            tool: self.editor.tool,
                                            start: point,
                                            end: point,
                                            constrain,
                                        });
                                    }
                                }
                                Tool::Pan => {
                                    self.drag = Some(DragState::Pan {
                                        last: (pos.x, pos.y),
                                    });
                                }
                            }
                        }
                    }
                }

                if self.editor.is_editing_nodes() && pointer_in_canvas && response.double_clicked()
                {
                    if let Some(pos) = pointer {
                        let point = self.doc_point(rect, pos);
                        if let Some((segment, t)) = self.editor.nearest_segment_on_path(point) {
                            if self.editor.insert_node_at(segment, t).is_some() {
                                self.status = "Node inserted".to_string();
                                self.drag = None;
                            }
                        }
                    }
                }

                if let Some(pos) = pointer {
                    if self.drag.is_some() {
                        self.update_drag(rect, pos);
                    }
                }

                if primary_released {
                    if let Some(drag) = self.drag.take() {
                        self.finish_drag(drag);
                    }
                }

                let preview = match &self.drag {
                    Some(DragState::Draw {
                        tool, start, end, ..
                    }) => Some(match tool {
                        Tool::Rect => PreviewShape::Rect {
                            start: *start,
                            end: *end,
                        },
                        Tool::Ellipse => PreviewShape::Ellipse {
                            start: *start,
                            end: *end,
                        },
                        _ => PreviewShape::Line {
                            start: *start,
                            end: *end,
                        },
                    }),
                    Some(DragState::Marquee { start, end, .. }) => Some(PreviewShape::Rect {
                        start: *start,
                        end: *end,
                    }),
                    Some(DragState::Sketch { points }) => {
                        Some(PreviewShape::Polyline(points.clone()))
                    }
                    _ => None,
                };
                let preview = if self.editor.tool == Tool::Bezier && self.pen_nodes.len() >= 2 {
                    Some(PreviewShape::Polyline(
                        self.pen_nodes.iter().map(|node| node.point).collect(),
                    ))
                } else {
                    preview
                };
                let theme = Theme {
                    dark: self.dark_mode,
                };
                let painter = ui.painter_at(rect);
                let toolpaths = if self.preview_toolpaths
                    || (self.preview_open && self.preview_order)
                {
                    let (paths, _) = crate::gcode::build_toolpaths(&self.editor.document);
                    Some(
                        paths
                            .into_iter()
                            .filter(|path| {
                                path.operation_id.as_deref().is_none_or(|id| {
                                    self.editor
                                        .document
                                        .operations
                                        .iter()
                                        .find(|operation| operation.id == id)
                                        .is_none_or(|operation| operation.params.show_in_preview)
                                })
                            })
                            .collect::<Vec<_>>(),
                    )
                } else {
                    None
                };
                self.refresh_image_previews(&ctx);
                let textures: crate::editor::renderer::ImageTextures = self
                    .image_previews
                    .iter()
                    .map(|(id, preview)| (id.clone(), preview.texture.id()))
                    .collect();
                let playback_overlay = if self.preview_open {
                    self.playback
                        .as_ref()
                        .map(|playback| (playback, self.playback_progress))
                } else {
                    None
                };
                let nest_overlay = if self.nest_open {
                    self.nest_proposal
                        .as_ref()
                        .map(|proposal| proposal.placements.as_slice())
                } else {
                    None
                };
                let overlays = crate::editor::renderer::CanvasOverlays {
                    toolpaths: toolpaths.as_deref(),
                    images: Some(&textures),
                    playback: playback_overlay,
                    nest: nest_overlay,
                    show_order: self.preview_open && self.preview_order,
                    guides: Some(self.smart_guides.as_slice()),
                };
                paint_canvas(&painter, rect, &self.editor, theme, preview, &overlays);

                if let Some(text) = drag_measurement(self.drag.as_ref()) {
                    if let Some(DragState::Draw { end, .. } | DragState::Marquee { end, .. }) =
                        self.drag.as_ref()
                    {
                        let anchor = crate::editor::renderer::screen_pos(rect, &self.editor, *end);
                        let anchor = egui::pos2(anchor.x + 14.0, anchor.y - 14.0);
                        let font = egui::FontId::monospace(12.0);
                        let galley = painter.layout_no_wrap(
                            text,
                            font,
                            egui::Color32::from_rgb(235, 235, 235),
                        );
                        let box_rect = egui::Rect::from_min_size(
                            egui::pos2(anchor.x - 5.0, anchor.y - galley.size().y - 3.0),
                            galley.size() + egui::vec2(10.0, 6.0),
                        );
                        painter.rect_filled(
                            box_rect,
                            4.0,
                            egui::Color32::from_rgba_unmultiplied(20, 22, 25, 220),
                        );
                        painter.galley(
                            egui::pos2(anchor.x, anchor.y - galley.size().y),
                            galley,
                            egui::Color32::from_rgb(235, 235, 235),
                        );
                    }
                }

                if pointer_in_canvas {
                    let cursor = match self.editor.tool {
                        Tool::Pan => egui::CursorIcon::Grab,
                        Tool::Select => {
                            match pointer.and_then(|pos| self.selection_handle_at(rect, pos)) {
                                Some(HandleKind::Move) => egui::CursorIcon::Grab,
                                Some(HandleKind::Rotate) => egui::CursorIcon::Crosshair,
                                Some(HandleKind::Scale(index)) => match index {
                                    0 | 4 => egui::CursorIcon::ResizeNwSe,
                                    2 | 6 => egui::CursorIcon::ResizeNeSw,
                                    1 | 5 => egui::CursorIcon::ResizeVertical,
                                    _ => egui::CursorIcon::ResizeHorizontal,
                                },
                                None => egui::CursorIcon::Default,
                            }
                        }
                        _ => egui::CursorIcon::Crosshair,
                    };
                    ctx.set_cursor_icon(cursor);
                }
                if !modal && response.secondary_clicked() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let point = self.doc_point(rect, pos);
                        let tolerance = 8.0 / self.editor.canvas.zoom.max(0.01);
                        let hit = self.editor.object_at(point, tolerance);
                        if let Some(id) = &hit {
                            if !self.editor.selection.contains(id) {
                                self.editor.select_at(point, false, tolerance);
                            }
                        }
                        self.context_object = hit;
                        self.context_point = Some(point);
                    }
                }
                if !modal {
                    response.context_menu(|ui| {
                        if self.context_object.is_some() {
                            self.object_context_menu(ui);
                        } else {
                            self.canvas_context_menu(ui);
                        }
                    });
                }
                // Canvas edits are applied after the side panels have already
                // been painted; request one more frame so they show the result.
                if primary_pressed || primary_released || self.drag.is_some() {
                    ctx.request_repaint();
                }
            });
    }

    pub(super) fn object_context_menu(&mut self, ui: &mut Ui) {
        let count = self.editor.selection.len();
        let first = self.editor.selection.ids().first().cloned();
        let text_selected = count == 1
            && first.as_ref().is_some_and(|id| {
                self.editor
                    .document
                    .objects
                    .iter()
                    .any(|object| &object.id == id && matches!(object.shape, Shape::Text(_)))
            });
        let image_selected = count == 1
            && first.as_ref().is_some_and(|id| {
                self.editor
                    .document
                    .objects
                    .iter()
                    .any(|object| &object.id == id && matches!(object.shape, Shape::Image(_)))
            });
        let has_selection = count > 0;
        for action in object_context_actions(count, text_selected, image_selected) {
            if matches!(
                action,
                ContextAction::ToFront
                    | ContextAction::Lock
                    | ContextAction::EditNodes
                    | ContextAction::ExportSvg
            ) {
                ui.separator();
            }
            let enabled = match action {
                ContextAction::Cut
                | ContextAction::Copy
                | ContextAction::Duplicate
                | ContextAction::Delete
                | ContextAction::ToFront
                | ContextAction::ToBack
                | ContextAction::Lock
                | ContextAction::Unlock
                | ContextAction::Ungroup
                | ContextAction::ExportSvg
                | ContextAction::ExportDxf
                | ContextAction::ExportGcode => has_selection,
                ContextAction::Group => count >= 2,
                ContextAction::GroupByColour => self.editor.document.objects.len() >= 2,
                ContextAction::EditNodes => count == 1,
                ContextAction::TextToOutlines => text_selected,
                ContextAction::AdjustImage
                | ContextAction::VectorizeImage
                | ContextAction::CenterlineImage => image_selected,
                _ => true,
            };
            if icons::button_enabled(ui, enabled, action.icon(), action.label()).clicked() {
                self.run_context_action(action);
                ui.close();
            }
        }
    }

    pub(super) fn canvas_context_menu(&mut self, ui: &mut Ui) {
        for action in canvas_context_actions() {
            match action {
                ContextAction::Paste => {
                    if icons::button_enabled(ui, !self.clipboard.is_empty(), Icon::Paste, "Paste")
                        .clicked()
                    {
                        self.run_context_action(action);
                        ui.close();
                    }
                }
                _ => {
                    if icons::button(ui, action.icon(), action.label()).clicked() {
                        self.run_context_action(action);
                        ui.close();
                    }
                }
            }
        }
        ui.separator();
        icons::checkbox(
            ui,
            &mut self.editor.canvas.grid_visible,
            Icon::Grid,
            "Show grid",
        );
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
        icons::checkbox(
            ui,
            &mut self.editor.show_rulers,
            Icon::Rulers,
            "Show rulers",
        );
        icons::checkbox(
            ui,
            &mut self.editor.guides_visible,
            Icon::Guides,
            "Show guides",
        );
        icons::checkbox(
            ui,
            &mut self.editor.snap_enabled,
            Icon::Snap,
            "Snap to grid",
        );
    }

    pub(super) fn run_context_action(&mut self, action: ContextAction) {
        match action {
            ContextAction::Cut => self.cut(),
            ContextAction::Copy => self.copy(),
            ContextAction::Paste => self.paste(),
            ContextAction::Duplicate => {
                self.editor.duplicate_selection();
            }
            ContextAction::Delete => {
                self.editor.delete_selection();
            }
            ContextAction::ToFront => {
                let count = self.editor.bring_selection_to_front();
                self.status = format!("Brought {count} object(s) to the front");
            }
            ContextAction::ToBack => {
                let count = self.editor.send_selection_to_back();
                self.status = format!("Sent {count} object(s) to the back");
            }
            ContextAction::Group => {
                self.editor.group_selection();
            }
            ContextAction::GroupByColour => {
                let (groups, objects) = self.editor.group_by_colour();
                self.status = if groups == 0 {
                    "No colour has two or more objects".to_string()
                } else {
                    format!("Grouped {objects} object(s) into {groups} colour group(s)")
                };
            }
            ContextAction::Ungroup => {
                self.editor.ungroup_selection();
            }
            ContextAction::CutSettings => self.open_cut_settings_for_selection(),
            ContextAction::SelectSameColour => {
                if let Some((_, colour, _)) = self.selection_cut_context() {
                    let count = self.editor.select_by_color(&colour);
                    self.status = format!("Selected {count} object(s) of {colour}");
                }
            }
            ContextAction::Lock => {
                let count = self.editor.lock_selection();
                self.status = format!("Locked {count} object(s)");
            }
            ContextAction::Unlock => {
                let count = self.editor.unlock_selection();
                self.status = format!("Unlocked {count} object(s)");
            }
            ContextAction::EditNodes => {
                if let Some(id) = self.editor.selection.ids().first().cloned() {
                    match self.editor.begin_node_edit(&id) {
                        Ok(()) => {
                            self.status =
                                "Editing nodes: drag to move, double-click a segment to insert"
                                    .to_string();
                        }
                        Err(error) => self.status = format!("Cannot edit nodes: {error}"),
                    }
                }
            }
            ContextAction::TextToOutlines => self.apply_text_outlines(),
            ContextAction::AdjustImage => self.open_image_adjust(),
            ContextAction::VectorizeImage => self.vectorize_selected_image(),
            ContextAction::CenterlineImage => self.centerline_selected_image(),
            ContextAction::ExportSvg => {
                self.open_export_svg(true);
            }
            ContextAction::ExportDxf => {
                self.open_export_dxf(true);
            }
            ContextAction::ExportGcode => {
                self.export_subset = true;
                self.open_dialog(FileMode::ExportGcode);
            }
            ContextAction::SelectAll => {
                self.editor.select_all();
            }
            ContextAction::ZoomIn => self.editor.zoom_in(),
            ContextAction::ZoomOut => self.editor.zoom_out(),
            ContextAction::FitPage => self.needs_fit = true,
            ContextAction::ZoomSelection => self.zoom_to_selection_request = true,
            ContextAction::Guides => self.guides_open = true,
            ContextAction::DocumentProperties => self.open_document_properties(),
            ContextAction::Preview => self.open_preview(),
            ContextAction::Preferences => self.preferences_open = true,
        }
    }
}
