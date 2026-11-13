#![allow(clippy::wildcard_imports)]

use super::*;

impl BeamBurinApp {
    /// Close the editor after `frames` frames, for headless CI.
    pub fn set_smoke_frames(&mut self, frames: u32) {
        self.smoke_frames = Some(frames);
    }

    /// Print one line per frame on stderr, for measuring a slow session.
    pub fn enable_frame_log(&mut self) {
        self.frame_trace.enable();
    }

    pub fn new(file: Option<PathBuf>) -> Self {
        Self::new_inner(file, true)
    }

    /// A deterministic application for widget tests: no live bridge and no
    /// machine or recovery state from the user's data directory.
    #[cfg(test)]
    pub(crate) fn new_for_ui_test() -> Self {
        Self::new_inner(None, false)
    }

    fn new_inner(file: Option<PathBuf>, start_bridge: bool) -> Self {
        let document = Document::default();
        let preferences_path = crate::config::preferences::default_path();
        let preferences =
            crate::config::preferences::AppPreferences::load_or_default(&preferences_path)
                .unwrap_or_default();
        let mut editor = EditorState::new(document.clone());
        editor.set_history_limit(preferences.drawing.undo_steps);
        editor.canvas.grid_spacing_mm = preferences.drawing.grid_spacing_mm;
        editor.snap_step_mm = preferences.drawing.snap_step_mm;
        editor.snap_enabled = preferences.drawing.snap_enabled;
        editor.snap_nodes = preferences.drawing.snap_nodes;
        editor.snap_endpoints = preferences.drawing.snap_endpoints;
        editor.snap_centers = preferences.drawing.snap_centers;
        editor.snap_midpoints = preferences.drawing.snap_midpoints;
        editor.snap_intersections = preferences.drawing.snap_intersections;
        editor.snap_limits = preferences.drawing.snap_limits;
        editor.show_rulers = preferences.drawing.show_rulers;
        editor.show_selection = preferences.drawing.show_selection;
        editor
            .selection_color
            .clone_from(&preferences.drawing.selection_color);
        editor.handle_style =
            crate::editor::state::HandleStyle::from_label(&preferences.drawing.handle_style);
        editor.zoom_step = preferences.drawing.zoom_step;
        editor.select_whole_group = preferences.drawing.select_whole_group;
        editor.scroll_zoom_step = preferences.drawing.scroll_zoom_step;
        editor.guides_visible = preferences.drawing.guides_visible;
        editor.snap_guides = preferences.drawing.snap_guides;
        let dark_mode = preferences.general.theme == "dark";
        let bridge_address = crate::bridge::configured_address();
        let bridge_network_token = crate::bridge::configured_token();
        let bridge_network = !bridge_address.is_loopback() && bridge_network_token.is_some();
        let bridge_bind = if bridge_network {
            bridge_address
        } else {
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        };
        let machines_path = crate::machines::default_path();
        let machines =
            crate::machines::MachineLibrary::load_or_default(&machines_path).unwrap_or_default();
        let machine_wizard_open = !machines.first_run_done;
        let recovery_path = crate::config::paths::recovery_path();
        let recovery_timestamp = std::fs::metadata(&recovery_path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or_else(String::new, |duration| {
                crate::profiles::model::format_utc(duration.as_secs())
            });
        let recovery_prompt_open = recovery_path.is_file();
        let mut app = Self {
            editor,
            dark_mode,
            current_path: None,
            saved_snapshot: Some(document),
            status: "Ready".to_string(),
            dialog: None,
            confirm: None,
            about_open: false,
            licenses_open: false,
            drag: None,
            cursor_doc: None,
            needs_fit: true,
            quit_requested: false,
            smoke_frames: None,
            frame_trace: frame_trace::FrameTrace::default(),
            job: None,
            active_operation: None,
            profiles: crate::profiles::load_or_default(&crate::profiles::default_path())
                .unwrap_or_else(|_| crate::profiles::ProfileLibrary::with_builtins()),
            profiles_path: crate::profiles::default_path(),
            profiles_open: false,
            profile_search: String::new(),
            selected_material: None,
            selected_operation: None,
            profile_message: String::new(),
            profile_favorites_only: false,
            profile_category_filter: String::new(),
            profile_sort_by_date: false,
            profile_tag_buffer: String::new(),
            profile_tag_owner: String::new(),
            profile_io_path: crate::profiles::default_path()
                .to_string_lossy()
                .to_string(),
            selected_image_profile: None,
            selected_project_profile: None,
            file_report: None,
            report_open: false,
            preview_toolpaths: false,
            image_previews: std::collections::HashMap::new(),
            burn_preview: None,
            invert_preview: None,
            image_original_previews: std::collections::HashMap::new(),
            image_adjust: None,
            text_editor_open: false,
            text_editor_object: None,
            text_editor_draft: crate::objects::TextObject::default(),
            image_adjust_snapshot: None,
            image_adjust_edit: None,
            image_compare: false,
            image_burn_map: false,
            image_invert_display: false,
            image_preview_zoom: 0.0,
            image_estimate_speed: 3000.0,
            image_target_dpi: 254.0,
            image_target_width_mm: 0.0,
            image_histogram: None,
            machines,
            machines_path,
            machines_open: false,
            machine_wizard_open,
            machine_editor: crate::machines::MachineProfile::default(),
            machine_edit_id: None,
            machine_error: None,
            doc_properties_snapshot: None,
            cuts_open: false,
            cuts_snapshot: None,
            parameters_open: false,
            parameters_snapshot: None,
            constraints_open: false,
            constraints_snapshot: None,
            constraint_kind_key: "horizontal".to_string(),
            constraint_value: 10.0,
            constraint_message: String::new(),
            recipes_open: false,
            recipe_name: String::new(),
            recipe_material: String::new(),
            recipe_machine: String::new(),
            recipe_head: String::new(),
            recipe_search: String::new(),
            recipe_filter_material: String::new(),
            recipe_filter_machine: String::new(),
            recipe_message: String::new(),
            help_shortcuts_open: false,
            assistant_open: false,
            multi_props_recorded: false,
            smart_guides: Vec::new(),
            row_settings: None,
            row_settings_snapshot: None,
            row_settings_done: false,
            material_test_open: false,
            gcode_dialect: crate::gcode::Dialect::default(),
            gcode_origin: crate::gcode::JobOrigin::default(),
            gcode_smooth_mm: 0.0,
            gcode_decimals: 3,
            gcode_comments: true,
            gcode_travel: true,
            gcode_crlf: false,
            gcode_arcs: false,
            export_presets_open: false,
            export_preset_name: String::new(),
            image_export_preset_name: String::new(),
            vector_export_preset_name: String::new(),
            vector_export_preset_format: "svg".to_string(),
            history_open: false,
            tour_open: !preferences.general.tour_seen,
            tour_step: 0,
            material_test_options: crate::material_test::Options::default(),
            image_test_open: false,
            image_test_options: crate::image_test::Options::default(),
            image_test_target: None,
            bridge: if start_bridge {
                crate::bridge::LiveBridge::start(
                    bridge_bind,
                    crate::bridge::configured_port(),
                    Some(crate::bridge::effective_token()),
                )
            } else {
                None
            },
            live_server: crate::mcp::McpServer::new(),
            toasts: Vec::new(),
            preferences: preferences.clone(),
            preferences_path,
            preferences_open: false,
            workspaces_open: false,
            workspace_name: String::new(),
            palette_open: false,
            palette_query: String::new(),
            palette_focus: false,
            regmarks_open: false,
            regmarks_options: crate::regmarks::Options::default(),
            interface_presets_open: false,
            interface_preset_name: String::new(),
            shortcut_capture: None,
            external_gcode_open: false,
            external_gcode: None,
            preferences_section: crate::config::preferences::Section::General,
            preferences_search: String::new(),
            polygon_sides: 6,
            grid_array_open: false,
            offset_open: false,
            offset_distance: 1.0,
            star_points: 5,
            star_inner: 0.5,
            line_angle_snap: false,
            text_size_mm: 10.0,
            repeat_open: false,
            repeat_options: crate::editor::RepeatOptions::default(),
            clipboard: Vec::new(),
            context_object: None,
            context_point: None,
            operation_params: None,
            link_proportions: false,
            skew_open: false,
            position_open: false,
            position_x: 0.0,
            position_y: 0.0,
            corner_radius_open: false,
            corner_radius_value: 0.0,
            pending_corner_radius: None,
            barcode_open: false,
            barcode_text: "ABC".to_string(),
            barcode_module_mm: 0.5,
            barcode_height_mm: 10.0,
            barcode_label: true,
            skew_x: 0.0,
            skew_y: 0.0,
            simplify_open: false,
            simplify_tolerance: 0.2,
            nest_open: false,
            nest_spacing: 1.0,
            nest_margin: 0.0,
            nest_rotation: true,
            nest_proposal: None,
            nest_message: String::new(),
            grid_array: crate::editor::GridOptions {
                columns: 3,
                rows: 2,
                dx: 20.0,
                dy: 20.0,
                ..crate::editor::GridOptions::default()
            },
            search_query: String::new(),
            preview_open: false,
            playback: None,
            playback_progress: 0.0,
            playback_playing: false,
            playback_speed: 1.0,
            playback_error: None,
            gcode_preview: String::new(),
            gcode_line_count: 0,
            preview_snapshot: None,
            preview_line_numbers: false,
            preview_order: false,
            show_layers_panel: true,
            show_properties_panel: true,
            show_tool_panel: true,
            side_panel_tab: SidePanelTab::Properties,
            doc_properties_open: false,
            operations_compare_open: false,
            profile_pending: None,
            relink_target: None,
            pending_file: None,
            recovery_path,
            last_autosave: std::time::Instant::now(),
            recovery_prompt_open,
            recovery_timestamp,
            task: None,
            guides_open: false,
            guide_position: 50.0,
            exact_size: false,
            exact_width: 20.0,
            exact_height: 10.0,
            profile_confirm_open: false,
            zoom_to_selection_request: false,
            export_subset: false,
            export_offset_origin: false,
            export_image_open: false,
            export_image_dpi: 96.0,
            export_image_dpi_custom: false,
            export_image_transparent: false,
            export_image_selection: false,
            export_image_drawing: false,
            export_image_custom_size: false,
            export_image_width_px: 1000,
            export_image_height_px: 1000,
            export_image_keep_aspect: true,
            export_image_margin_mm: 0.0,
            export_image_offset_x_mm: 0.0,
            export_image_offset_y_mm: 0.0,
            export_svg_open: false,
            export_svg_selection: false,
            export_svg_text_paths: false,
            export_svg_embed_images: false,
            export_svg_layer_ids: Vec::new(),
            export_dxf_open: false,
            export_dxf_selection: false,
            export_dxf_text_paths: false,
            export_dxf_arcs: false,
            export_dxf_unit: crate::export::DxfUnit::Millimetres,
            export_dxf_layer_ids: Vec::new(),
            export_vector_open: false,
            export_vector_mode: FileMode::ExportPdf,
            export_vector_selection: false,
            export_vector_text_paths: false,
            export_vector_margin_mm: 0.0,
            export_vector_drawing: false,
            export_vector_layer_ids: Vec::new(),
            recover_next: false,
            pen_nodes: Vec::new(),
            pen_drag_start: None,
        };
        if let Some(path) = file {
            app.open_path(&path);
        }
        if let Some(bridge) = &app.bridge {
            if bridge.is_network() {
                app.status = format!(
                    "Ready - live bridge on {}:{} (token required)",
                    bridge.address(),
                    bridge.port()
                );
            } else {
                app.status = format!(
                    "Ready - live bridge on 127.0.0.1:{} (BeamBurin mcp --live)",
                    bridge.port()
                );
            }
            if !bridge.is_network() && !bridge_address.is_loopback() {
                app.status = "Live bridge kept on localhost: set BEAMBURIN_BRIDGE_TOKEN to expose it on the network".to_string();
            }
        }
        app
    }

    pub(super) fn is_dirty(&self) -> bool {
        self.saved_snapshot.as_ref() != Some(&self.editor.document)
    }

    pub(super) fn title(&self) -> String {
        let name = self
            .current_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map_or_else(|| self.editor.document.name.clone(), str::to_string);
        if self.is_dirty() {
            format!("{name} *")
        } else {
            name
        }
    }

    pub(super) fn request(&mut self, action: PendingAction) {
        if self.is_dirty() {
            self.confirm = Some(action);
        } else {
            self.perform(action);
        }
    }

    pub(super) fn perform(&mut self, action: PendingAction) {
        match action {
            PendingAction::New => self.new_document(),
            PendingAction::Open => self.open_dialog(FileMode::OpenProject),
            PendingAction::Quit => self.quit_requested = true,
        }
    }

    pub(super) fn open_document_properties(&mut self) {
        self.doc_properties_snapshot = Some(self.editor.document.clone());
        self.doc_properties_open = true;
    }

    pub(super) fn undo(&mut self) {
        self.status = match self.editor.undo() {
            Some(action) => format!("Undo {action}"),
            None => "Nothing to undo".to_string(),
        };
    }

    pub(super) fn redo(&mut self) {
        self.status = match self.editor.redo() {
            Some(action) => format!("Redo {action}"),
            None => "Nothing to redo".to_string(),
        };
    }

    pub(super) fn undo_label(&self) -> String {
        self.editor
            .history
            .undo_label()
            .map_or_else(|| "Undo".to_string(), |action| format!("Undo {action}"))
    }

    pub(super) fn redo_label(&self) -> String {
        self.editor
            .history
            .redo_label()
            .map_or_else(|| "Redo".to_string(), |action| format!("Redo {action}"))
    }
}
