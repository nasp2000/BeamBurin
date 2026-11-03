#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::similar_names,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::unused_self
)]

use std::path::{Path, PathBuf};

use egui::{Color32, Key, Modifiers, Pos2, Rect, Sense, Ui};

use crate::color::{normalize_hex_color, parse_hex_color, to_hex};
use crate::commands::{CommandId as PaletteAction, PaletteCommand};
use crate::config::shortcuts::{Command, parse_chord};
use crate::document::{Document, Shape, Unit};
use crate::editor::alignment::AlignMode;
use crate::editor::renderer::{PreviewShape, Theme, format_mm, paint_canvas};
use crate::editor::{EditorState, Tool};
use crate::geometry::Point2;
use crate::report::FileReport;
use crate::ui::file_dialog::{DialogResult, FileDialog, FileMode};
use crate::ui::icons::{self, Icon};

mod chrome;
mod dialogs;
mod dialogs_assets;
mod dialogs_file;
mod dialogs_help;
mod dialogs_image;
mod dialogs_machines;
mod dialogs_operations;
mod dialogs_preferences;
mod dialogs_reports;
mod dialogs_transform;
mod files;
mod layers;
mod properties;

/// One row of the layers table.
type LayerRow = (
    String,
    String,
    String,
    bool,
    bool,
    Option<String>,
    bool,
    usize,
);

/// Result produced by a background task.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum TaskResult {
    Import {
        path: PathBuf,
        outcome: std::result::Result<crate::import::format::ImportOutcome, String>,
    },
    Export(std::result::Result<FileReport, String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingAction {
    New,
    Open,
    Quit,
}

/// Constraint kinds the window offers, in display order.
const CONSTRAINT_KINDS: [&str; 8] = [
    "coincident",
    "horizontal",
    "vertical",
    "distance",
    "angle",
    "parallel",
    "perpendicular",
    "tangent",
];

fn constraint_kind_label(key: &str) -> &'static str {
    match key {
        "coincident" => "Coincident",
        "vertical" => "Vertical",
        "distance" => "Distance",
        "angle" => "Angle",
        "parallel" => "Parallel",
        "perpendicular" => "Perpendicular",
        "tangent" => "Tangent",
        _ => "Horizontal",
    }
}

/// One item chosen from the command palette.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PaletteTarget {
    Command(PaletteAction),
    Object(String),
    Layer(String),
    Operation(String),
    Parameter,
}

/// Objects whose name or kind contains `query`, at most eight.
fn filter_objects<'a>(document: &'a Document, query: &str) -> Vec<&'a crate::document::Object> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    document
        .objects
        .iter()
        .filter(|object| {
            object.name.to_lowercase().contains(&query)
                || object.shape.kind_name().to_lowercase().contains(&query)
        })
        .take(8)
        .collect()
}

fn filter_named<'a, T>(
    items: impl Iterator<Item = &'a T>,
    query: &str,
    name: impl Fn(&T) -> &str,
) -> Vec<&'a T> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    items
        .filter(|item| name(item).to_lowercase().contains(&query))
        .take(8)
        .collect()
}

/// Indices of the catalog entries that match `query`, ignoring case.
fn filter_palette(entries: &[PaletteCommand], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    entries
        .iter()
        .enumerate()
        .filter(|(_, command)| {
            query.is_empty()
                || command.label.to_lowercase().contains(&query)
                || command.key.to_lowercase().contains(&query)
                || command.category.label().to_lowercase().contains(&query)
                || command.summary.to_lowercase().contains(&query)
        })
        .map(|(index, _)| index)
        .collect()
}

/// Entry offered by the canvas context menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextAction {
    Cut,
    Copy,
    Paste,
    Duplicate,
    Delete,
    ToFront,
    ToBack,
    Group,
    Ungroup,
    Lock,
    Unlock,
    EditNodes,
    TextToOutlines,
    AdjustImage,
    VectorizeImage,
    CenterlineImage,
    ExportSvg,
    ExportDxf,
    ExportGcode,
    SelectAll,
    ZoomIn,
    ZoomOut,
    FitPage,
    ZoomSelection,
    Guides,
    DocumentProperties,
    Preview,
    Preferences,
}

impl ContextAction {
    fn label(self) -> &'static str {
        match self {
            ContextAction::Cut => "Cut",
            ContextAction::Copy => "Copy",
            ContextAction::Paste => "Paste",
            ContextAction::Duplicate => "Duplicate",
            ContextAction::Delete => "Delete",
            ContextAction::ToFront => "Bring to front",
            ContextAction::ToBack => "Send to back",
            ContextAction::Group => "Group",
            ContextAction::Ungroup => "Ungroup",
            ContextAction::Lock => "Lock",
            ContextAction::Unlock => "Unlock",
            ContextAction::EditNodes => "Edit nodes",
            ContextAction::TextToOutlines => "Text to outlines",
            ContextAction::AdjustImage => "Adjust image...",
            ContextAction::VectorizeImage => "Vectorize image...",
            ContextAction::CenterlineImage => "Trace centerline...",
            ContextAction::ExportSvg => "Export selection as SVG...",
            ContextAction::ExportDxf => "Export selection as DXF...",
            ContextAction::ExportGcode => "Export selection as G-code...",
            ContextAction::SelectAll => "Select all",
            ContextAction::ZoomIn => "Zoom in",
            ContextAction::ZoomOut => "Zoom out",
            ContextAction::FitPage => "Fit page",
            ContextAction::ZoomSelection => "Zoom to selection",
            ContextAction::Guides => "Guides...",
            ContextAction::DocumentProperties => "Document properties...",
            ContextAction::Preview => "Preview and G-code...",
            ContextAction::Preferences => "Preferences...",
        }
    }

    fn icon(self) -> Icon {
        match self {
            ContextAction::Cut => Icon::Cut,
            ContextAction::Copy => Icon::Copy,
            ContextAction::Paste => Icon::Paste,
            ContextAction::Duplicate => Icon::Duplicate,
            ContextAction::Delete => Icon::Delete,
            ContextAction::ToFront => Icon::ToFront,
            ContextAction::ToBack => Icon::ToBack,
            ContextAction::Group => Icon::Group,
            ContextAction::Ungroup => Icon::Ungroup,
            ContextAction::Lock => Icon::Lock,
            ContextAction::Unlock => Icon::Unlock,
            ContextAction::EditNodes => Icon::Nodes,
            ContextAction::TextToOutlines => Icon::TextOutline,
            ContextAction::AdjustImage => Icon::Image,
            ContextAction::VectorizeImage => Icon::Bezier,
            ContextAction::CenterlineImage => Icon::Pencil,
            ContextAction::ExportSvg | ContextAction::ExportDxf | ContextAction::ExportGcode => {
                Icon::Export
            }
            ContextAction::SelectAll => Icon::SelectAll,
            ContextAction::ZoomIn => Icon::ZoomIn,
            ContextAction::ZoomOut => Icon::ZoomOut,
            ContextAction::FitPage => Icon::ZoomFit,
            ContextAction::ZoomSelection => Icon::ZoomSelection,
            ContextAction::Guides => Icon::Guides,
            ContextAction::DocumentProperties => Icon::Page,
            ContextAction::Preview => Icon::Play,
            ContextAction::Preferences => Icon::Preferences,
        }
    }
}

/// Entries of the context menu over the selected objects.
fn object_context_actions(
    count: usize,
    text_selected: bool,
    image_selected: bool,
) -> Vec<ContextAction> {
    let mut actions = vec![
        ContextAction::Cut,
        ContextAction::Copy,
        ContextAction::Duplicate,
        ContextAction::Delete,
        ContextAction::ToFront,
        ContextAction::ToBack,
    ];
    if count >= 2 {
        actions.push(ContextAction::Group);
    }
    actions.push(ContextAction::Ungroup);
    actions.push(ContextAction::Lock);
    actions.push(ContextAction::Unlock);
    if count == 1 {
        actions.push(ContextAction::EditNodes);
    }
    if text_selected {
        actions.push(ContextAction::TextToOutlines);
    }
    if image_selected {
        actions.push(ContextAction::AdjustImage);
        actions.push(ContextAction::VectorizeImage);
        actions.push(ContextAction::CenterlineImage);
    }
    actions.extend([
        ContextAction::ExportSvg,
        ContextAction::ExportDxf,
        ContextAction::ExportGcode,
    ]);
    actions
}

/// Entries of the context menu over empty work area.
fn canvas_context_actions() -> Vec<ContextAction> {
    let mut actions = vec![ContextAction::Paste, ContextAction::SelectAll];
    actions.extend([
        ContextAction::ZoomIn,
        ContextAction::ZoomOut,
        ContextAction::FitPage,
        ContextAction::ZoomSelection,
        ContextAction::Guides,
        ContextAction::DocumentProperties,
        ContextAction::Preview,
        ContextAction::Preferences,
    ]);
    actions
}

/// Which part of the selection frame is being dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandleKind {
    Move,
    /// Index into the eight frame handles, clockwise from the top-left.
    Scale(usize),
    Rotate,
}

#[derive(Debug, Clone)]
enum DragState {
    Move {
        last: Point2,
        recorded: bool,
    },
    NodeMove {
        last: Point2,
        recorded: bool,
    },
    Handle {
        node: usize,
        outgoing: bool,
        recorded: bool,
    },
    Draw {
        tool: Tool,
        start: Point2,
        end: Point2,
        /// Shift: square/circle for rectangles and ellipses, 45° steps for lines.
        constrain: bool,
    },
    Marquee {
        start: Point2,
        end: Point2,
        additive: bool,
    },
    Sketch {
        points: Vec<Point2>,
    },
    Transform {
        kind: HandleKind,
        last: Point2,
        centre: Point2,
        recorded: bool,
        constrain: bool,
    },
    Pan {
        last: (f32, f32),
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum SidePanelTab {
    #[default]
    Properties,
    Tool,
}

pub struct BeamBurinApp {
    editor: EditorState,
    dark_mode: bool,
    current_path: Option<PathBuf>,
    saved_snapshot: Option<Document>,
    status: String,
    dialog: Option<FileDialog>,
    confirm: Option<PendingAction>,
    about_open: bool,
    licenses_open: bool,
    drag: Option<DragState>,
    cursor_doc: Option<Point2>,
    needs_fit: bool,
    quit_requested: bool,
    /// Frames left before a smoke-test run closes itself.
    smoke_frames: Option<u32>,
    job: Option<crate::gcode::JobReport>,
    active_operation: Option<String>,
    profiles: crate::profiles::ProfileLibrary,
    profiles_path: PathBuf,
    profiles_open: bool,
    profile_search: String,
    selected_material: Option<String>,
    selected_operation: Option<String>,
    profile_message: String,
    profile_io_path: String,
    profile_favorites_only: bool,
    profile_category_filter: String,
    profile_sort_by_date: bool,
    profile_tag_buffer: String,
    profile_tag_owner: String,
    selected_image_profile: Option<String>,
    selected_project_profile: Option<String>,
    file_report: Option<FileReport>,
    report_open: bool,
    preview_toolpaths: bool,
    image_previews: std::collections::HashMap<String, ImagePreview>,
    burn_preview: Option<BurnPreview>,
    image_original_previews: std::collections::HashMap<String, egui::TextureHandle>,
    image_adjust: Option<String>,
    image_adjust_snapshot: Option<Document>,
    image_burn_map: bool,
    image_estimate_speed: f64,
    image_target_dpi: f64,
    image_target_width_mm: f64,
    machines: crate::machines::MachineLibrary,
    machines_path: PathBuf,
    machines_open: bool,
    machine_wizard_open: bool,
    machine_editor: crate::machines::MachineProfile,
    machine_edit_id: Option<String>,
    machine_error: Option<String>,
    doc_properties_snapshot: Option<Document>,
    cuts_open: bool,
    cuts_snapshot: Option<Document>,
    parameters_open: bool,
    parameters_snapshot: Option<Document>,
    constraints_open: bool,
    constraints_snapshot: Option<Document>,
    constraint_kind_key: String,
    constraint_value: f64,
    constraint_message: String,
    recipes_open: bool,
    recipe_name: String,
    recipe_material: String,
    recipe_machine: String,
    recipe_head: String,
    recipe_search: String,
    recipe_filter_material: String,
    recipe_filter_machine: String,
    recipe_message: String,
    help_shortcuts_open: bool,
    assistant_open: bool,
    multi_props_recorded: bool,
    smart_guides: Vec<(bool, f64)>,
    cuts_drag: Option<usize>,
    cuts_drag_target: Option<usize>,
    material_test_open: bool,
    gcode_dialect: crate::gcode::Dialect,
    gcode_smooth_mm: f64,
    gcode_decimals: u8,
    gcode_comments: bool,
    gcode_travel: bool,
    gcode_crlf: bool,
    gcode_arcs: bool,
    export_presets_open: bool,
    export_preset_name: String,
    history_open: bool,
    tour_open: bool,
    tour_step: usize,
    material_test_options: crate::material_test::Options,
    image_test_open: bool,
    image_test_options: crate::image_test::Options,
    image_test_target: Option<String>,
    bridge: Option<crate::bridge::LiveBridge>,
    live_server: crate::mcp::McpServer,
    toasts: Vec<Toast>,
    preferences: crate::config::preferences::AppPreferences,
    preferences_path: PathBuf,
    preferences_open: bool,
    workspaces_open: bool,
    workspace_name: String,
    palette_open: bool,
    palette_query: String,
    palette_focus: bool,
    regmarks_open: bool,
    regmarks_options: crate::regmarks::Options,
    interface_presets_open: bool,
    interface_preset_name: String,
    shortcut_capture: Option<crate::config::shortcuts::Command>,
    external_gcode_open: bool,
    external_gcode: Option<(PathBuf, crate::gcode::external::Program)>,
    preferences_section: crate::config::preferences::Section,
    preferences_search: String,
    polygon_sides: usize,
    grid_array_open: bool,
    offset_open: bool,
    offset_distance: f64,
    star_points: usize,
    star_inner: f64,
    line_angle_snap: bool,
    text_size_mm: f64,
    repeat_open: bool,
    repeat_options: crate::editor::RepeatOptions,
    clipboard: Vec<crate::document::Object>,
    context_object: Option<String>,
    context_point: Option<Point2>,
    operation_params: Option<crate::operations::OperationParams>,
    link_proportions: bool,
    skew_open: bool,
    skew_x: f64,
    skew_y: f64,
    simplify_open: bool,
    simplify_tolerance: f64,
    nest_open: bool,
    nest_spacing: f64,
    nest_margin: f64,
    nest_rotation: bool,
    nest_proposal: Option<crate::editor::NestProposal>,
    nest_message: String,
    grid_array: crate::editor::GridOptions,
    search_query: String,
    preview_open: bool,
    playback: Option<crate::gcode::Playback>,
    playback_progress: f64,
    playback_playing: bool,
    playback_speed: f32,
    playback_error: Option<String>,
    gcode_text: String,
    preview_snapshot: Option<Document>,
    preview_line_numbers: bool,
    preview_order: bool,
    show_layers_panel: bool,
    show_properties_panel: bool,
    show_tool_panel: bool,
    side_panel_tab: SidePanelTab,
    doc_properties_open: bool,
    operations_compare_open: bool,
    profile_pending: Option<(String, crate::profiles::OperationProfile)>,
    relink_target: Option<String>,
    pending_file: Option<(FileMode, PathBuf)>,
    recovery_path: PathBuf,
    last_autosave: std::time::Instant,
    recovery_prompt_open: bool,
    recovery_timestamp: String,
    task: Option<crate::task::BackgroundTask<TaskResult>>,
    guides_open: bool,
    guide_position: f64,
    exact_size: bool,
    exact_width: f64,
    exact_height: f64,
    profile_confirm_open: bool,
    zoom_to_selection_request: bool,
    export_subset: bool,
    export_offset_origin: bool,
    recover_next: bool,
    pen_nodes: Vec<crate::geometry::path::Node>,
    pen_drag_start: Option<Point2>,
}

/// Build one export and write it, reporting what it contains.
///
/// Runs on a worker thread; `cancel` is checked before writing so a cancelled
/// export never creates or replaces a file.
fn run_export(
    mode: FileMode,
    path: &Path,
    document: &crate::document::Document,
    cancel: &std::sync::atomic::AtomicBool,
    offset_origin: bool,
    gcode: crate::gcode::GCodeOptions,
    smooth_mm: f64,
) -> std::result::Result<FileReport, String> {
    let cancelled = || {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            Err("Cancelled".to_string())
        } else {
            Ok(())
        }
    };
    match mode {
        FileMode::ExportImage => {
            cancelled()?;
            let (data, width, height, warnings) =
                crate::export::render_rgba(document, &crate::export::PngOptions::default())
                    .map_err(|error| error.to_string())?;
            let picture = image::RgbaImage::from_raw(width, height, data)
                .ok_or_else(|| "cannot build the image buffer".to_string())?;
            picture.save(path).map_err(|error| error.to_string())?;
            let format = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("png")
                .to_ascii_uppercase();
            Ok(FileReport::new("Export image", path.display().to_string())
                .with_summary("Width", format!("{width} px"))
                .with_summary("Height", format!("{height} px"))
                .with_summary("Format", format)
                .with_warnings(warnings))
        }
        FileMode::ExportDxf => {
            let dxf = crate::export::export_dxf(document).map_err(|error| error.to_string())?;
            cancelled()?;
            std::fs::write(path, dxf).map_err(|error| error.to_string())?;
            Ok(FileReport::new("Export DXF", path.display().to_string())
                .with_summary("Objects", document.objects.len().to_string())
                .with_summary("Format", "LWPOLYLINE subset"))
        }
        FileMode::ExportEps => {
            let eps = crate::export::export_eps(document).map_err(|error| error.to_string())?;
            cancelled()?;
            std::fs::write(path, eps).map_err(|error| error.to_string())?;
            Ok(FileReport::new("Export EPS", path.display().to_string())
                .with_summary("Objects", document.objects.len().to_string())
                .with_summary("Format", "Encapsulated PostScript"))
        }
        FileMode::ExportPdf => {
            let pdf = crate::export::export_pdf(document).map_err(|error| error.to_string())?;
            cancelled()?;
            std::fs::write(path, pdf.as_bytes()).map_err(|error| error.to_string())?;
            Ok(FileReport::new("Export PDF", path.display().to_string())
                .with_summary("Objects", document.objects.len().to_string())
                .with_summary("Format", "single-page vector"))
        }
        FileMode::ExportHpgl => {
            let hpgl = crate::export::export_hpgl(document).map_err(|error| error.to_string())?;
            cancelled()?;
            std::fs::write(path, hpgl).map_err(|error| error.to_string())?;
            Ok(FileReport::new("Export HPGL", path.display().to_string())
                .with_summary("Objects", document.objects.len().to_string())
                .with_summary("Units", "40 per mm"))
        }
        FileMode::ExportGcode => {
            cancelled()?;
            let cancelled_flag = || cancel.load(std::sync::atomic::Ordering::Relaxed);
            let report = crate::gcode::export_job_cancellable(
                document,
                path,
                &crate::gcode::JobOptions {
                    offset_to_origin: offset_origin,
                    smooth_mm,
                    gcode,
                    ..crate::gcode::JobOptions::default()
                },
                &cancelled_flag,
            )
            .map_err(|error| error.to_string())?;
            Ok(FileReport::new("Export G-code", path.display().to_string())
                .with_summary("Paths", report.paths.to_string())
                .with_summary("Lines", report.lines.to_string())
                .with_summary("Bytes", report.bytes.to_string())
                .with_summary("Estimated", format!("{:.1} s", report.seconds))
                .with_warnings(report.warnings))
        }
        FileMode::ExportSvg => {
            let svg = crate::export::export_svg(document).map_err(|error| error.to_string())?;
            cancelled()?;
            std::fs::write(path, svg).map_err(|error| error.to_string())?;
            Ok(FileReport::new("Export SVG", path.display().to_string())
                .with_summary("Objects", document.objects.len().to_string()))
        }
        _ => Err("unsupported export mode".to_string()),
    }
}

/// Short description shown when hovering a toolbar tool.
fn tool_help(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "Select and move objects (V)",
        Tool::Line => "Draw a line; Shift snaps to 45° (L)",
        Tool::Rect => "Draw a rectangle; Shift for a square (R)",
        Tool::Ellipse => "Draw an ellipse; Shift for a circle (E)",
        Tool::Polygon => "Draw a regular polygon (P)",
        Tool::Star => "Draw a star with adjustable tips (S)",
        Tool::Text => "Insert text (T)",
        Tool::Measure => "Measure a distance and angle",
        Tool::Bezier => "Draw a Bézier path node by node",
        Tool::Pencil => "Draw a freehand path (B)",
        Tool::Pan => "Pan the canvas",
    }
}

/// Icon shown by each tool in the toolbar and Draw menu.
fn alignment_snap(
    document: &Document,
    selection: &[String],
    bounds: Option<(Point2, Point2)>,
    zoom: f64,
    dx: f64,
    dy: f64,
) -> (f64, f64, Vec<(bool, f64)>) {
    let Some((min, max)) = bounds else {
        return (dx, dy, Vec::new());
    };
    let tolerance = 6.0 / zoom.max(0.01);
    let selected: std::collections::HashSet<&str> = selection.iter().map(String::as_str).collect();
    let mut verticals = Vec::new();
    let mut horizontals = Vec::new();
    for object in &document.objects {
        if selected.contains(object.id.as_str()) || !object.visible || object.locked {
            continue;
        }
        let Some((other_min, other_max)) = object.bounds() else {
            continue;
        };
        verticals.extend([
            other_min.x,
            f64::midpoint(other_min.x, other_max.x),
            other_max.x,
        ]);
        horizontals.extend([
            other_min.y,
            f64::midpoint(other_min.y, other_max.y),
            other_max.y,
        ]);
    }
    let current_x = [min.x + dx, f64::midpoint(min.x, max.x) + dx, max.x + dx];
    let current_y = [min.y + dy, f64::midpoint(min.y, max.y) + dy, max.y + dy];
    let mut guides = Vec::new();
    let snap = |lines: [f64; 3], candidates: &[f64], guides: &mut Vec<f64>| -> f64 {
        let mut best: Option<(f64, f64)> = None;
        for line in lines {
            for candidate in candidates {
                let difference = candidate - line;
                if difference.abs() <= tolerance
                    && best.is_none_or(|(best, _)| difference.abs() < best.abs())
                {
                    best = Some((difference, *candidate));
                }
            }
        }
        match best {
            Some((difference, coordinate)) => {
                guides.push(coordinate);
                difference
            }
            None => 0.0,
        }
    };
    let adjust_x = snap(current_x, &verticals, &mut guides);
    let adjust_y = snap(current_y, &horizontals, &mut guides);
    guides.sort_by(f64::total_cmp);
    guides.dedup_by(|left, right| (*left - *right).abs() < 1e-9);
    let mut result = Vec::new();
    for guide in guides {
        if verticals.iter().any(|value| (value - guide).abs() < 1e-9) {
            result.push((true, guide));
        }
        if horizontals.iter().any(|value| (value - guide).abs() < 1e-9) {
            result.push((false, guide));
        }
    }
    (dx + adjust_x, dy + adjust_y, result)
}

fn boolean_icon(op: crate::geometry::BooleanOp) -> Icon {
    match op {
        crate::geometry::BooleanOp::Union => Icon::BooleanUnion,
        crate::geometry::BooleanOp::Difference => Icon::BooleanDifference,
        crate::geometry::BooleanOp::Intersection => Icon::BooleanIntersection,
        crate::geometry::BooleanOp::Xor => Icon::BooleanExclusion,
    }
}

fn tool_icon(tool: Tool) -> Icon {
    match tool {
        Tool::Select => Icon::Select,
        Tool::Line => Icon::Line,
        Tool::Rect => Icon::Rectangle,
        Tool::Ellipse => Icon::Ellipse,
        Tool::Polygon => Icon::Polygon,
        Tool::Star => Icon::Star,
        Tool::Text => Icon::Text,
        Tool::Measure => Icon::Measure,
        Tool::Bezier => Icon::Bezier,
        Tool::Pencil => Icon::Freehand,
        Tool::Pan => Icon::Pan,
    }
}

/// Scale around a point, for direct property edits.
fn egui_transform_scale(
    factor_x: f64,
    factor_y: f64,
    center: Point2,
) -> crate::geometry::Transform2 {
    crate::geometry::Transform2::scale_around(factor_x, factor_y, center.x, center.y)
}

/// One row in a profile list.
struct ProfileRow {
    id: String,
    name: String,
    builtin: bool,
    favorite: bool,
    category: String,
    modified_at: String,
    tags: Vec<String>,
}

/// Search, filter and sort settings shared by the four profile lists.
#[derive(Clone)]
struct ProfileFilter {
    query: String,
    favorites_only: bool,
    category: String,
    sort_by_date: bool,
}

fn profile_rows<T>(
    profiles: &[T],
    parts: impl Fn(&T) -> (&str, &str, bool, &crate::profiles::ProfileMeta),
    extra: impl Fn(&T) -> String,
    filter: &ProfileFilter,
) -> Vec<ProfileRow> {
    let mut rows: Vec<ProfileRow> = profiles
        .iter()
        .filter_map(|profile| {
            let (id, name, builtin, meta) = parts(profile);
            let row = ProfileRow {
                id: id.to_string(),
                name: name.to_string(),
                builtin,
                favorite: meta.favorite,
                category: meta.category.clone(),
                modified_at: meta.modified_at.clone(),
                tags: meta.tags.clone(),
            };
            let text = format!("{} {}", row.name, extra(profile)).to_lowercase();
            let query_matches = filter.query.is_empty()
                || text.contains(&filter.query)
                || row.category.to_lowercase().contains(&filter.query)
                || row
                    .tags
                    .iter()
                    .any(|tag| tag.to_lowercase().contains(&filter.query));
            (query_matches
                && (!filter.favorites_only || row.favorite)
                && (filter.category.is_empty() || row.category == filter.category))
                .then_some(row)
        })
        .collect();
    if filter.sort_by_date {
        rows.sort_by(|left, right| right.modified_at.cmp(&left.modified_at));
    } else {
        rows.sort_by_key(|row| row.name.to_lowercase());
    }
    rows
}

/// Edit the category, tags and favourite mark of a profile.
///
/// `editable` is false for built-in profiles, whose other fields are read-only;
/// marking a built-in as a favourite is still allowed.
fn profile_meta_editor(
    ui: &mut egui::Ui,
    id: &str,
    meta: &mut crate::profiles::ProfileMeta,
    editable: bool,
    tag_buffer: &mut String,
    tag_owner: &mut String,
) -> bool {
    if tag_owner != id {
        *tag_owner = id.to_string();
        *tag_buffer = meta.tag_text();
    }
    let mut changed = ui.checkbox(&mut meta.favorite, "Favourite").changed();
    ui.add_enabled_ui(editable, |ui| {
        ui.horizontal(|ui| {
            ui.label("Category");
            changed |= ui.text_edit_singleline(&mut meta.category).changed();
            egui::ComboBox::from_id_salt(format!("category_suggest_{id}"))
                .selected_text("Suggest")
                .width(90.0)
                .show_ui(ui, |ui| {
                    for category in crate::profiles::DEFAULT_CATEGORIES {
                        if ui
                            .selectable_label(meta.category == category, category)
                            .clicked()
                        {
                            meta.category = category.to_string();
                            changed = true;
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Tags");
            if ui.text_edit_singleline(tag_buffer).changed() {
                meta.set_tag_text(tag_buffer);
                changed = true;
            }
            ui.weak("comma separated");
        });
    });
    let created = if meta.created_at.is_empty() {
        "unknown".to_string()
    } else {
        meta.created_at.clone()
    };
    let modified = if meta.modified_at.is_empty() {
        "unknown".to_string()
    } else {
        meta.modified_at.clone()
    };
    ui.weak(format!("Created {created} - modified {modified}"));
    if changed && editable {
        meta.touch();
    }
    changed
}

/// A short message shown over the canvas and then forgotten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToastKind {
    Info,
    Error,
}

struct Toast {
    text: String,
    kind: ToastKind,
    created: std::time::Instant,
}

struct ImagePreview {
    /// Black burns on white paper, used by the canvas and the normal preview.
    texture: egui::TextureHandle,
    path: String,
    settings: crate::objects::ImageSettings,
    crop: Option<crate::objects::ImageCrop>,
    stats: crate::raster::RasterStats,
}

/// The burn-map texture of the image currently open in Adjust image.
///
/// Only one exists at a time: keeping one per image would double the texture
/// memory of a project with many images.
struct BurnPreview {
    object_id: String,
    settings: crate::objects::ImageSettings,
    crop: Option<crate::objects::ImageCrop>,
    texture: egui::TextureHandle,
}

impl Default for BeamBurinApp {
    fn default() -> Self {
        Self::new(None)
    }
}

impl BeamBurinApp {
    /// Close the editor after `frames` frames, for headless CI.
    pub fn set_smoke_frames(&mut self, frames: u32) {
        self.smoke_frames = Some(frames);
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
            image_original_previews: std::collections::HashMap::new(),
            image_adjust: None,
            image_adjust_snapshot: None,
            image_burn_map: false,
            image_estimate_speed: 3000.0,
            image_target_dpi: 254.0,
            image_target_width_mm: 0.0,
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
            cuts_drag: None,
            cuts_drag_target: None,
            material_test_open: false,
            gcode_dialect: crate::gcode::Dialect::default(),
            gcode_smooth_mm: 0.0,
            gcode_decimals: 3,
            gcode_comments: true,
            gcode_travel: true,
            gcode_crlf: false,
            gcode_arcs: false,
            export_presets_open: false,
            export_preset_name: String::new(),
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
            gcode_text: String::new(),
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

    fn is_dirty(&self) -> bool {
        self.saved_snapshot.as_ref() != Some(&self.editor.document)
    }

    fn title(&self) -> String {
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

    fn request(&mut self, action: PendingAction) {
        if self.is_dirty() {
            self.confirm = Some(action);
        } else {
            self.perform(action);
        }
    }

    fn perform(&mut self, action: PendingAction) {
        match action {
            PendingAction::New => self.new_document(),
            PendingAction::Open => self.open_dialog(FileMode::OpenProject),
            PendingAction::Quit => self.quit_requested = true,
        }
    }

    fn open_document_properties(&mut self) {
        self.doc_properties_snapshot = Some(self.editor.document.clone());
        self.doc_properties_open = true;
    }

    fn undo(&mut self) {
        self.status = match self.editor.undo() {
            Some(action) => format!("Undo {action}"),
            None => "Nothing to undo".to_string(),
        };
    }

    fn redo(&mut self) {
        self.status = match self.editor.redo() {
            Some(action) => format!("Redo {action}"),
            None => "Nothing to redo".to_string(),
        };
    }

    fn undo_label(&self) -> String {
        self.editor
            .history
            .undo_label()
            .map_or_else(|| "Undo".to_string(), |action| format!("Undo {action}"))
    }

    fn redo_label(&self) -> String {
        self.editor
            .history
            .redo_label()
            .map_or_else(|| "Redo".to_string(), |action| format!("Redo {action}"))
    }

    fn canvas(&mut self, ui: &mut Ui) {
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
                    || self.cuts_open;
                let pointer = if modal {
                    None
                } else {
                    ctx.input(|input| input.pointer.interact_pos())
                };
                let primary_pressed = !modal && ctx.input(|input| input.pointer.primary_pressed());
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

                let pointer_in_canvas = pointer.is_some_and(|pos| rect.contains(pos));
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
                        if let Some(pos) = pointer.filter(|pos| rect.contains(*pos)) {
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
                            if let Some(pos) = pointer.filter(|pos| rect.contains(*pos)) {
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

    fn object_context_menu(&mut self, ui: &mut Ui) {
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

    fn canvas_context_menu(&mut self, ui: &mut Ui) {
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

    fn run_context_action(&mut self, action: ContextAction) {
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
            ContextAction::Ungroup => {
                self.editor.ungroup_selection();
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
                self.export_subset = true;
                self.open_dialog(FileMode::ExportSvg);
            }
            ContextAction::ExportDxf => {
                self.export_subset = true;
                self.open_dialog(FileMode::ExportDxf);
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

    /// Keep the saved theme in step with the Dark mode toggle and presets.
    fn persist_theme(&mut self) {
        let theme = if self.dark_mode { "dark" } else { "light" };
        if self.preferences.general.theme != theme {
            self.preferences.general.theme = theme.to_string();
            let _ = self.preferences.save(&self.preferences_path);
        }
    }

    fn refresh_image_previews(&mut self, ctx: &egui::Context) {
        use crate::document::Shape;

        let mut needed: Vec<(String, crate::objects::ImageObject)> = Vec::new();
        for object in &self.editor.document.objects {
            if let Shape::Image(image) = &object.shape {
                needed.push((object.id.clone(), image.clone()));
            }
        }
        self.image_previews.retain(|id, preview| {
            needed.iter().any(|(needed_id, image)| {
                needed_id == id
                    && preview.path == image.path
                    && preview.settings == image.settings
                    && preview.crop == image.crop
            })
        });
        for (id, image) in needed {
            if self.image_previews.contains_key(&id) {
                continue;
            }
            let Ok(bitmap) =
                crate::raster::process_object(std::path::Path::new(&image.path), &image)
            else {
                continue;
            };
            let path = image.path.clone();
            let settings = image.settings;
            let crop = image.crop;
            let stats = crate::raster::RasterStats::of(&bitmap);
            let texture = ctx.load_texture(
                format!("image-preview-{id}"),
                preview_color_image(&bitmap, false),
                egui::TextureOptions::NEAREST,
            );
            self.image_previews.insert(
                id,
                ImagePreview {
                    texture,
                    path,
                    settings,
                    crop,
                    stats,
                },
            );
        }
    }

    fn selection_doc_centre(&self) -> Option<Point2> {
        self.editor
            .selected_bounds()
            .map(|(min, max)| Point2::new(f64::midpoint(min.x, max.x), f64::midpoint(min.y, max.y)))
    }

    fn selection_handle_at(&self, rect: Rect, pos: Pos2) -> Option<HandleKind> {
        if !self.editor.show_selection || self.editor.selection.is_empty() {
            return None;
        }
        let (min, max) = self.editor.selected_bounds()?;
        let p0 = crate::editor::renderer::screen_pos(rect, &self.editor, Point2::new(min.x, max.y));
        let p1 = crate::editor::renderer::screen_pos(rect, &self.editor, Point2::new(max.x, min.y));
        let bounds = Rect::from_two_pos(p0, p1).expand(2.0);
        let centre = bounds.center();
        if bounds.width() > 48.0 && bounds.height() > 48.0 {
            for corner in [
                bounds.left_top(),
                bounds.right_top(),
                bounds.right_bottom(),
                bounds.left_bottom(),
            ] {
                let direction = corner - centre;
                let length = direction.length();
                if length > 1.0 && (corner + direction / length * 18.0 - pos).length() <= 9.0 {
                    return Some(HandleKind::Rotate);
                }
            }
        }
        let handles = [
            bounds.left_top(),
            bounds.center_top(),
            bounds.right_top(),
            bounds.right_center(),
            bounds.right_bottom(),
            bounds.center_bottom(),
            bounds.left_bottom(),
            bounds.left_center(),
        ];
        for (index, handle) in handles.iter().enumerate() {
            if (*handle - pos).length() <= 8.0 {
                return Some(HandleKind::Scale(index));
            }
        }
        if (centre - pos).length() <= 9.0 {
            return Some(HandleKind::Move);
        }
        None
    }

    fn doc_point(&self, rect: Rect, pos: Pos2) -> Point2 {
        let (x, y) = self
            .editor
            .canvas
            .screen_to_doc(f64::from(pos.x - rect.min.x), f64::from(pos.y - rect.min.y));
        self.editor.snap(Point2::new(x, y))
    }

    fn nearest_node_handle(&self, point: Point2, tolerance: f64) -> Option<(usize, bool)> {
        let mut best: Option<(usize, bool, f64)> = None;
        for (index, (handle_in, handle_out)) in self.editor.node_handles().iter().enumerate() {
            for (outgoing, handle) in [(false, handle_in), (true, handle_out)] {
                let Some(handle) = handle else {
                    continue;
                };
                let distance = handle.distance(point);
                if distance <= tolerance && best.is_none_or(|(_, _, current)| distance < current) {
                    best = Some((index, outgoing, distance));
                }
            }
        }
        best.map(|(index, outgoing, _)| (index, outgoing))
    }

    fn begin_node_interaction(&mut self, point: Point2, shift: bool) {
        let tolerance = 6.0 / self.editor.canvas.zoom.max(0.01);
        if let Some((node, outgoing)) = self.nearest_node_handle(point, tolerance) {
            self.drag = Some(DragState::Handle {
                node,
                outgoing,
                recorded: false,
            });
            return;
        }
        if self
            .editor
            .select_node_at(point, tolerance, shift)
            .is_some()
        {
            self.drag = Some(DragState::NodeMove {
                last: point,
                recorded: false,
            });
        }
    }

    fn begin_select(&mut self, point: Point2, shift: bool) {
        let tolerance = 5.0 / self.editor.canvas.zoom.max(0.01);
        let hit = self.editor.object_at(point, tolerance);
        if let Some(id) = hit {
            if shift {
                self.editor.selection.toggle(id);
            } else {
                if !self.editor.selection.contains(&id) {
                    self.editor.selection.clear();
                    self.editor.selection.add(id);
                }
                self.drag = Some(DragState::Move {
                    last: point,
                    recorded: false,
                });
            }
        } else {
            if !shift {
                self.editor.selection.clear();
            }
            self.drag = Some(DragState::Marquee {
                start: point,
                end: point,
                additive: shift,
            });
        }
    }

    fn update_drag(&mut self, rect: Rect, pos: Pos2) {
        let point = self.doc_point(rect, pos);
        self.smart_guides.clear();
        let Some(drag) = &mut self.drag else {
            return;
        };
        match drag {
            DragState::Move { last, recorded } => {
                let dx = point.x - last.x;
                let dy = point.y - last.y;
                let (dx, dy, guides) = alignment_snap(
                    &self.editor.document,
                    self.editor.selection.ids(),
                    self.editor.selected_bounds(),
                    self.editor.canvas.zoom,
                    dx,
                    dy,
                );
                self.smart_guides = guides;
                if dx.abs() > 1e-9 || dy.abs() > 1e-9 {
                    if !*recorded {
                        self.editor.record_action("Move");
                        *recorded = true;
                    }
                    self.editor.move_selection(dx, dy);
                    *last = point;
                }
            }
            DragState::NodeMove { last, recorded } => {
                let dx = point.x - last.x;
                let dy = point.y - last.y;
                if dx.abs() > 1e-9 || dy.abs() > 1e-9 {
                    if !*recorded {
                        self.editor.record_action("Move node");
                        *recorded = true;
                    }
                    self.editor.move_selected_nodes(dx, dy);
                    *last = point;
                }
            }
            DragState::Handle {
                node,
                outgoing,
                recorded,
            } => {
                if !*recorded {
                    self.editor.record_action("Edit handle");
                    *recorded = true;
                }
                self.editor.set_node_handle(*node, *outgoing, point);
            }
            DragState::Draw {
                end,
                start,
                tool,
                constrain,
            } => {
                *end = match (*constrain, tool) {
                    (true, Tool::Rect | Tool::Ellipse) => {
                        crate::geometry::point::square_corner(*start, point)
                    }
                    (true, Tool::Line) => {
                        crate::geometry::point::constrain_angle(*start, point, 45.0)
                    }
                    _ => point,
                };
            }
            DragState::Marquee { end, .. } => {
                *end = point;
            }
            DragState::Sketch { points } => {
                if points.last().is_none_or(|last| last.distance(point) >= 0.5) {
                    points.push(point);
                }
            }
            DragState::Transform {
                kind,
                last,
                centre,
                recorded,
                constrain,
            } => {
                let (dx, dy) = (point.x - last.x, point.y - last.y);
                let action = match kind {
                    HandleKind::Move => "Move",
                    HandleKind::Scale(_) => "Scale",
                    HandleKind::Rotate => "Rotate",
                };
                if !*recorded && (dx.abs() > 1e-9 || dy.abs() > 1e-9) {
                    self.editor.record_action(action);
                    *recorded = true;
                }
                match *kind {
                    HandleKind::Move => {
                        let (dx, dy, guides) = alignment_snap(
                            &self.editor.document,
                            self.editor.selection.ids(),
                            self.editor.selected_bounds(),
                            self.editor.canvas.zoom,
                            dx,
                            dy,
                        );
                        self.smart_guides = guides;
                        self.editor.move_selection(dx, dy);
                    }
                    HandleKind::Rotate => {
                        let before = (last.y - centre.y).atan2(last.x - centre.x);
                        let now = (point.y - centre.y).atan2(point.x - centre.x);
                        let mut delta = now - before;
                        while delta > std::f64::consts::PI {
                            delta -= std::f64::consts::TAU;
                        }
                        while delta < -std::f64::consts::PI {
                            delta += std::f64::consts::TAU;
                        }
                        if delta.abs() > 1e-9 {
                            self.editor.rotate_selection(delta);
                        }
                    }
                    HandleKind::Scale(index) => {
                        let uses_x = !matches!(index, 1 | 5);
                        let uses_y = !matches!(index, 3 | 7);
                        let last_x = last.x - centre.x;
                        let last_y = last.y - centre.y;
                        let mut factor_x = if uses_x && last_x.abs() > 1e-9 {
                            (point.x - centre.x) / last_x
                        } else {
                            1.0
                        };
                        let mut factor_y = if uses_y && last_y.abs() > 1e-9 {
                            (point.y - centre.y) / last_y
                        } else {
                            1.0
                        };
                        if *constrain {
                            let factor = if (factor_x - 1.0).abs() > (factor_y - 1.0).abs() {
                                factor_x
                            } else {
                                factor_y
                            };
                            factor_x = factor;
                            factor_y = factor;
                        }
                        if factor_x.is_finite()
                            && factor_y.is_finite()
                            && factor_x > 0.01
                            && factor_y > 0.01
                        {
                            self.editor.scale_selection(factor_x, factor_y);
                        }
                    }
                }
                *last = point;
            }
            DragState::Pan { last } => {
                let dx = f64::from(pos.x - last.0);
                let dy = f64::from(pos.y - last.1);
                self.editor.canvas.pan(dx, dy);
                *last = (pos.x, pos.y);
            }
        }
    }

    fn finish_drag(&mut self, drag: DragState) {
        match drag {
            DragState::Draw {
                tool,
                start,
                mut end,
                ..
            } => {
                if self.exact_size && matches!(tool, Tool::Rect | Tool::Ellipse) {
                    end = crate::geometry::point::sized_corner(
                        start,
                        end,
                        self.exact_width,
                        self.exact_height,
                    );
                }
                match tool {
                    Tool::Rect => {
                        self.editor.add_rectangle(start, end);
                    }
                    Tool::Ellipse => {
                        self.editor.add_ellipse(start, end);
                    }
                    Tool::Line => {
                        self.editor.add_line(start, end);
                    }
                    Tool::Polygon => {
                        self.editor.add_polygon(
                            Point2::new(
                                f64::midpoint(start.x, end.x),
                                f64::midpoint(start.y, end.y),
                            ),
                            end,
                            self.polygon_sides,
                        );
                    }
                    Tool::Star => {
                        self.editor.add_star(
                            Point2::new(
                                f64::midpoint(start.x, end.x),
                                f64::midpoint(start.y, end.y),
                            ),
                            end,
                            self.star_points,
                            self.star_inner,
                        );
                    }
                    Tool::Text => {
                        if let Some(id) = self.editor.add_text(start) {
                            let font = self.preferences.general.text_font.clone();
                            if let Some(object) = self
                                .editor
                                .document
                                .objects
                                .iter_mut()
                                .find(|object| object.id == id)
                            {
                                if let Shape::Text(text) = &mut object.shape {
                                    text.font = font;
                                    text.size = self.text_size_mm.clamp(1.0, 500.0);
                                }
                            }
                        }
                    }
                    Tool::Measure => {
                        let distance = start.distance(end);
                        let angle = (end.y - start.y).atan2(end.x - start.x).to_degrees();
                        self.status = format!("Distance {distance:.2} mm at {angle:.1}°");
                    }
                    _ => {}
                }
            }
            DragState::Sketch { points } => {
                if self.editor.add_polyline(&points, false).is_some() {
                    self.status = "Freehand path created".to_string();
                }
            }
            DragState::Marquee {
                start,
                end,
                additive,
            } => {
                let min = Point2::new(start.x.min(end.x), start.y.min(end.y));
                let max = Point2::new(start.x.max(end.x), start.y.max(end.y));
                if (max.x - min.x) > 1e-6 || (max.y - min.y) > 1e-6 {
                    self.editor.select_in_rect((min, max), additive);
                }
            }
            DragState::Move { .. }
            | DragState::Pan { .. }
            | DragState::NodeMove { .. }
            | DragState::Handle { .. }
            | DragState::Transform { .. } => {}
        }
    }

    fn cut(&mut self) {
        let objects = self.editor.cut_selection();
        self.status = format!("Cut {} object(s)", objects.len());
        self.clipboard = objects;
    }

    fn copy(&mut self) {
        self.clipboard = self.editor.copy_selection();
        self.status = format!("Copied {} object(s)", self.clipboard.len());
    }

    fn paste_from_clipboard(&mut self) {
        if self.paste_external() {
            return;
        }
        self.paste();
    }

    fn paste_external(&mut self) -> bool {
        let Ok(mut clipboard) = arboard::Clipboard::new() else {
            return false;
        };
        if let Ok(image) = clipboard.get_image() {
            let path =
                std::env::temp_dir().join(format!("bw-clipboard-{}.png", uuid::Uuid::new_v4()));
            let Some(buffer) = image::RgbaImage::from_raw(
                image.width as u32,
                image.height as u32,
                image.bytes.into_owned(),
            ) else {
                return false;
            };
            if buffer.save(&path).is_err() {
                return false;
            }
            if let Ok(document) = crate::import::raster::import_raster(&path) {
                self.merge_imported(document, "Paste image");
                self.status = "Pasted image from the clipboard".to_string();
                return true;
            }
            return false;
        }
        if let Ok(text) = clipboard.get_text() {
            let trimmed = text.trim_start();
            if trimmed.starts_with("<svg") || trimmed.starts_with("<?xml") {
                if let Ok(document) = crate::import::svg::import_svg(&text) {
                    self.merge_imported(document, "Paste SVG");
                    self.status = "Pasted SVG from the clipboard".to_string();
                    return true;
                }
            }
            if self.clipboard.is_empty() && !text.trim().is_empty() {
                let at = self.cursor_doc.unwrap_or_else(Point2::origin);
                if let Some(id) = self.editor.add_text(at) {
                    if let Some(object) = self
                        .editor
                        .document
                        .objects
                        .iter_mut()
                        .find(|object| object.id == id)
                    {
                        if let Shape::Text(text_object) = &mut object.shape {
                            text_object.content = text;
                        }
                    }
                    self.status = "Pasted text from the clipboard".to_string();
                    return true;
                }
            }
        }
        false
    }

    fn merge_imported(&mut self, document: Document, action: &str) {
        self.editor.record_action(action);
        let layer = self.editor.active_layer_id.clone();
        for mut object in document.objects {
            object.layer_id.clone_from(&layer);
            object.operation_id = None;
            self.editor.document.add_object(object);
        }
    }

    fn paste(&mut self) {
        let count = self.editor.paste_objects(&self.clipboard);
        self.status = format!("Pasted {count} object(s)");
    }

    fn select_profile(&mut self, id: &str) {
        self.profiles_open = true;
        if self.profiles.material(id).is_some() {
            self.selected_material = Some(id.to_string());
        } else if self.profiles.operation(id).is_some() {
            self.selected_operation = Some(id.to_string());
        } else if self.profiles.image(id).is_some() {
            self.selected_image_profile = Some(id.to_string());
        } else if self.profiles.project(id).is_some() {
            self.selected_project_profile = Some(id.to_string());
        }
    }

    fn save_profiles(&mut self) {
        match crate::profiles::save(&self.profiles, &self.profiles_path) {
            Ok(()) => self.profile_message = "Profiles saved".to_string(),
            Err(error) => self.profile_message = format!("Cannot save profiles: {error}"),
        }
    }

    fn toast(&mut self, text: impl Into<String>) {
        self.push_toast(text.into(), ToastKind::Info);
    }

    fn toast_error(&mut self, text: impl Into<String>) {
        self.push_toast(text.into(), ToastKind::Error);
    }

    fn push_toast(&mut self, text: String, kind: ToastKind) {
        self.toasts.push(Toast {
            text,
            kind,
            created: std::time::Instant::now(),
        });
        while self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    fn paint_toasts(&mut self, ctx: &egui::Context) {
        const LIFE: f32 = 4.0;
        self.toasts
            .retain(|toast| toast.created.elapsed().as_secs_f32() < LIFE);
        if self.toasts.is_empty() {
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(120));
        egui::Area::new(egui::Id::new("toasts"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 48.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                for toast in &self.toasts {
                    let color = match toast.kind {
                        ToastKind::Info => crate::theme::ACCENT,
                        ToastKind::Error => egui::Color32::from_rgb(210, 80, 70),
                    };
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(28, 30, 34))
                        .stroke(egui::Stroke::new(1.0, color))
                        .corner_radius(6)
                        .inner_margin(8)
                        .show(ui, |ui| {
                            ui.colored_label(color, &toast.text);
                        });
                    ui.add_space(4.0);
                }
            });
    }

    fn poll_task(&mut self, ctx: &egui::Context) {
        let Some(task) = &self.task else {
            return;
        };
        match task.poll() {
            crate::task::TaskPoll::Pending => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
            crate::task::TaskPoll::Cancelled => {
                self.task = None;
                self.status = "Cancelled".to_string();
            }
            crate::task::TaskPoll::Finished(TaskResult::Import { path, outcome }) => {
                self.task = None;
                match outcome {
                    Ok(outcome) => self.apply_import(&path, outcome),
                    Err(error) => self.status = format!("Import failed: {error}"),
                }
            }
            crate::task::TaskPoll::Finished(TaskResult::Export(result)) => {
                self.task = None;
                match result {
                    Ok(report) => {
                        self.status = report.headline();
                        self.toast(self.status.clone());
                        self.show_report(report);
                    }
                    Err(error) if error == "Cancelled" => {
                        self.status = "Export cancelled".to_string();
                    }
                    Err(error) => {
                        self.status = format!("Export failed: {error}");
                        self.toast_error(self.status.clone());
                    }
                }
            }
        }
    }

    fn autosave(&mut self, ctx: &egui::Context) {
        if !self.is_dirty() {
            return;
        }
        let interval = std::time::Duration::from_secs(30);
        if self.last_autosave.elapsed() < interval {
            ctx.request_repaint_after(interval);
            return;
        }
        if crate::document::serialization::save_recovery(&self.editor.document, &self.recovery_path)
            .is_ok()
        {
            self.last_autosave = std::time::Instant::now();
        }
        ctx.request_repaint_after(interval);
    }

    fn advance_playback(&mut self, ctx: &egui::Context) {
        if self.preview_open
            && self.preview_snapshot.as_ref() != Some(&self.editor.document)
            && !ctx.input(|input| input.pointer.any_down())
        {
            self.refresh_preview();
        }
        if !self.playback_playing {
            return;
        }
        let Some(playback) = &self.playback else {
            self.playback_playing = false;
            return;
        };
        let delta = f64::from(ctx.input(|input| input.stable_dt).min(0.1));
        let rate = if playback.duration_seconds > 0.0 {
            playback.total_length / playback.duration_seconds
        } else {
            0.0
        };
        let advance = delta * rate * f64::from(self.playback_speed);
        self.playback_progress = (self.playback_progress + advance).min(playback.total_length);
        if self.playback_progress >= playback.total_length {
            self.playback_playing = false;
        }
        ctx.request_repaint();
    }

    fn open_preview(&mut self) {
        self.preview_open = true;
        self.refresh_preview();
    }

    /// Rebuild the simulated job and the G-code text from the document.
    fn refresh_preview(&mut self) {
        let options = crate::gcode::JobOptions::default();
        match crate::gcode::render_job(&self.editor.document, &options) {
            Ok((text, _report)) => {
                let (toolpaths, _) =
                    crate::gcode::planned_toolpaths(&self.editor.document, &options);
                let playback =
                    crate::gcode::Playback::from_toolpaths(&toolpaths, options.travel_speed_mm_min);
                self.playback_progress = self.playback_progress.min(playback.total_length);
                self.playback = Some(playback);
                self.playback_error = None;
                self.gcode_text = text;
            }
            Err(error) => {
                self.playback = None;
                self.playback_error = Some(error.to_string());
                self.gcode_text.clear();
            }
        }
        self.preview_snapshot = Some(self.editor.document.clone());
    }
}

impl BeamBurinApp {
    pub fn show(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if let Some(remaining) = self.smoke_frames {
            if remaining == 0 {
                self.quit_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                self.smoke_frames = Some(remaining - 1);
                ctx.request_repaint();
            }
        }
        if ctx.input(|input| input.viewport().close_requested())
            && self.is_dirty()
            && !self.quit_requested
            && self.preferences.general.confirm_destructive
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm = Some(PendingAction::Quit);
        }
        let document_fonts: Vec<String> = self
            .editor
            .document
            .objects
            .iter()
            .filter_map(|object| match &object.shape {
                Shape::Text(text) => Some(text.font.clone()),
                _ => None,
            })
            .collect();
        crate::textfont::ensure_loaded(&ctx, &document_fonts);
        crate::theme::apply(ui.ctx(), self.dark_mode);
        ui.style_mut().spacing.item_spacing = egui::vec2(8.0, 6.0);
        ui.style_mut().spacing.button_padding = egui::vec2(8.0, 5.0);
        self.shortcuts(ui);
        self.menu_bar(ui);
        self.toolbar(ui);
        self.status_bar(ui);
        self.tool_strip(ui);
        if self.show_layers_panel {
            self.layers_panel(ui);
        }
        if self.show_properties_panel {
            self.side_panel(ui);
        }
        self.canvas(ui);
        let ctx = ui.ctx().clone();
        self.dialogs(&ctx);
        self.paint_toasts(&ctx);
        if self.quit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    pub fn advance(&mut self, ctx: &egui::Context) {
        self.persist_theme();
        let requests = self
            .bridge
            .as_ref()
            .map(crate::bridge::LiveBridge::drain)
            .unwrap_or_default();
        if !requests.is_empty() {
            for pending in requests {
                let response = self.handle_live_request(&pending.request);
                let _ = pending.reply.send(response);
            }
            ctx.request_repaint();
        }
        if !ctx.input(|input| input.pointer.any_down()) {
            self.multi_props_recorded = false;
        }
        if self.drag.is_none() {
            self.smart_guides.clear();
        }
        self.advance_playback(ctx);
        self.autosave(ctx);
        self.poll_task(ctx);
        let title = format!("BeamBurin - {}", self.title());
        let current = ctx.input(|input| input.viewport().title.clone());
        if current.as_deref() != Some(title.as_str()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        }
    }
}

impl eframe::App for BeamBurinApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.advance(ctx);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

/// Font picker shared by the text properties and the preferences window.
/// The short first-run tour.
fn tour_steps() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "Draw or import",
            "Use the tools on the left, or File → Import SVG or DXF to bring in a drawing. \
             Objects keep their layer and can be moved, scaled and rotated on the canvas.",
        ),
        (
            "Assign operations",
            "Create an operation in Operations → New operation and assign it to the selection. \
             Speed, power, passes, kerf, tabs and leads live in Cuts / Layers.",
        ),
        (
            "Prepare images",
            "Right-click an image and choose Adjust image for dithering, tone curves and \
             presets. Vectorize image traces its outline; Trace centerline follows its strokes.",
        ),
        (
            "Check before writing",
            "View → Preview and G-code shows the planned path, the estimated time and the text \
             that will be written. Nothing is sent to a machine from this application.",
        ),
        (
            "Write the file",
            "File → Export G-code writes a standard file for a sender of your choice. \
             You can save the project as .bwp or, with images, as .bwpz.",
        ),
    ]
}

/// Live values shown next to the cursor while drawing or measuring.
fn drag_measurement(drag: Option<&DragState>) -> Option<String> {
    match drag? {
        DragState::Draw {
            tool, start, end, ..
        } => match tool {
            Tool::Measure | Tool::Line => {
                let distance = start.distance(*end);
                let angle = (end.y - start.y).atan2(end.x - start.x).to_degrees();
                Some(format!("{distance:.2} mm   {angle:.1}°"))
            }
            Tool::Rect | Tool::Ellipse => Some(format!(
                "{:.2} × {:.2} mm",
                (end.x - start.x).abs(),
                (end.y - start.y).abs()
            )),
            Tool::Polygon | Tool::Star => Some(format!("r {:.2} mm", start.distance(*end))),
            _ => None,
        },
        DragState::Marquee { start, end, .. } => Some(format!(
            "{:.2} × {:.2} mm",
            (end.x - start.x).abs(),
            (end.y - start.y).abs()
        )),
        _ => None,
    }
}

/// Format a parameter value without trailing zeros, with its unit label.
fn format_parameter_value(value: f64, unit: &str) -> String {
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.').to_string();
    if unit.trim().is_empty() {
        text
    } else {
        format!("{text} {}", unit.trim())
    }
}

fn format_seconds(seconds: f64) -> String {
    let total = seconds.round().max(0.0) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

fn section_icon(section: crate::config::preferences::Section) -> Icon {
    use crate::config::preferences::Section;
    match section {
        Section::General => Icon::Gear,
        Section::Interface => Icon::DarkMode,
        Section::Drawing => Icon::Snap,
        Section::Images => Icon::Image,
        Section::Files => Icon::Save,
        Section::Machines => Icon::Profiles,
        Section::Shortcuts => Icon::Text,
        Section::Advanced => Icon::Preferences,
    }
}

fn font_picker(ui: &mut Ui, id: &str, value: &mut String) -> bool {
    let mut changed = false;
    let choices = crate::textfont::names();
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.clone())
        .width(240.0)
        .show_ui(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for name in &choices {
                        if ui
                            .selectable_value(value, name.clone(), name.as_str())
                            .changed()
                        {
                            changed = true;
                        }
                    }
                });
        });
    changed
}

fn edit_color(ui: &mut Ui, color: &mut Option<String>) {
    ui.horizontal(|ui| {
        let mut enabled = color.is_some();
        if ui.checkbox(&mut enabled, "").changed() {
            *color = if enabled {
                Some("#000000".to_string())
            } else {
                None
            };
        }
        let Some(value) = color.as_mut() else {
            return;
        };
        let rgb = parse_hex_color(value).unwrap_or((0, 0, 0));
        let mut srgb = [rgb.0, rgb.1, rgb.2];
        if ui.color_edit_button_srgb(&mut srgb).changed() {
            *value = to_hex(srgb[0], srgb[1], srgb[2]);
        }
        let response = ui.add(
            egui::TextEdit::singleline(value)
                .desired_width(76.0)
                .hint_text("#rrggbb"),
        );
        if response.lost_focus() {
            if let Some(normalized) = normalize_hex_color(value) {
                *value = normalized;
            }
        }
        if normalize_hex_color(value).is_none() {
            ui.colored_label(Color32::from_rgb(200, 60, 60), "?");
        }
    });
}

fn sanitize_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '-' || character == '_' {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches('-').to_string();
    if cleaned.is_empty() {
        "untitled".to_string()
    } else {
        cleaned
    }
}

/// Burned pixels in the burn map: the canvas selection orange, so the map reads
/// as a preview of the work rather than as a grey picture.
const BURN_MAP_BURN: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x7A, 0x1A);
/// Paper in the burn map: near black, so the orange stays the only bright tone.
const BURN_MAP_PAPER: egui::Color32 = egui::Color32::from_rgb(16, 17, 19);

/// Downscale a burn bitmap to a preview texture.
///
/// Without the burn map the result is white paper with black burns, as the work
/// would be engraved. With the burn map, burns use [`BURN_MAP_BURN`] over a very
/// dark paper, which makes light engraving visible on a dark interface.
fn preview_color_image(bitmap: &crate::raster::RasterBitmap, burn_map: bool) -> egui::ColorImage {
    const MAX_SIDE: usize = 320;
    let step = ((bitmap.width.max(bitmap.height) as f32) / MAX_SIDE as f32)
        .ceil()
        .max(1.0) as usize;
    let width = (bitmap.width / step).max(1);
    let height = (bitmap.height / step).max(1);
    let mut pixels = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let level = bitmap.power_at(
                (x * step).min(bitmap.width - 1),
                (y * step).min(bitmap.height - 1),
            );
            let color = match (burn_map, level > 0) {
                (true, true) => BURN_MAP_BURN,
                (true, false) => BURN_MAP_PAPER,
                // A grayscale job burns partial power as a grey tone.
                (false, true) => egui::Color32::from_gray(255 - level),
                (false, false) => egui::Color32::WHITE,
            };
            pixels.extend_from_slice(&color.to_array());
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([width, height], &pixels)
}

#[cfg(test)]
mod tests;
