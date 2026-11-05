//! One table of application commands.
//!
//! The command palette and the MCP `list_commands`/`describe_command` index read
//! this catalog, so a label, a category or a shortcut is written once. Contextual
//! menu entries keep their own wording because they act on the current selection.

use serde_json::{Value, json};

use crate::config::shortcuts::Command as Shortcut;

/// A command the application offers outside the menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandId {
    New,
    Open,
    Save,
    SaveAs,
    SaveArchive,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Duplicate,
    Delete,
    SelectAll,
    SelectNone,
    SelectLayer,
    SelectContained,
    SelectOpenShapes,
    BringForward,
    SendBackward,
    SetPosition,
    ClosePaths,
    JoinPaths,
    ConvertToPath,
    CornerRadius,
    ClearCornerRadius,
    BooleanUnion,
    BooleanDifference,
    BooleanIntersection,
    BooleanExclusion,
    Weld,
    Offset,
    Repeat,
    Skew,
    Nest,
    ZoomIn,
    ZoomOut,
    FitPage,
    ToggleGrid,
    ToggleSnap,
    ToggleToolpaths,
    ToolSelect,
    ToolLine,
    ToolRectangle,
    ToolEllipse,
    ToolStar,
    ToolText,
    ToolPolygon,
    ToolPencil,
    Import,
    ExportSvg,
    ExportDxf,
    ExportEps,
    ExportPdf,
    ExportImage,
    ExportGcode,
    Cuts,
    Parameters,
    Constraints,
    Preview,
    MaterialTest,
    RegistrationMarks,
    DocumentProperties,
    Preferences,
    Workspaces,
    InterfacePresets,
    History,
    MachineProfiles,
    Materials,
    KeyboardShortcuts,
    Assistant,
}

/// Grouping shown before the label in the palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    File,
    Edit,
    View,
    Tools,
    Operations,
    Project,
    Settings,
    Help,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::File => "File",
            Category::Edit => "Edit",
            Category::View => "View",
            Category::Tools => "Tools",
            Category::Operations => "Operations",
            Category::Project => "Project",
            Category::Settings => "Settings",
            Category::Help => "Help",
        }
    }
}

/// One catalog entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteCommand {
    pub id: CommandId,
    /// Stable identifier used by agents and documentation.
    pub key: &'static str,
    pub label: &'static str,
    pub category: Category,
    /// Remappable keyboard command, when one is bound.
    pub shortcut: Option<Shortcut>,
    /// One line an agent can read to choose the command.
    pub summary: &'static str,
    /// Whether running it can change the document.
    pub mutates: bool,
}

impl PaletteCommand {
    /// The default key chord for the bound shortcut, if any.
    pub fn default_shortcut(&self) -> Option<&'static str> {
        self.shortcut.map(Shortcut::default_chord)
    }

    pub fn to_json(&self) -> Value {
        json!({
            "kind": "command",
            "id": self.key,
            "label": self.label,
            "category": self.category.label(),
            "shortcut": self.default_shortcut(),
            "shortcut_id": self.shortcut.map(Shortcut::id),
            "summary": self.summary,
            "mutates": self.mutates,
        })
    }
}

macro_rules! command {
    ($id:ident, $key:literal, $label:literal, $category:ident, $shortcut:expr, $mutates:literal, $summary:literal) => {
        PaletteCommand {
            id: CommandId::$id,
            key: $key,
            label: $label,
            category: Category::$category,
            shortcut: $shortcut,
            summary: $summary,
            mutates: $mutates,
        }
    };
}

/// Every command, in the order the palette shows before a search.
pub const ALL: &[PaletteCommand] = &[
    command!(
        New,
        "file.new",
        "New project",
        File,
        Some(Shortcut::New),
        true,
        "Start an empty project; unsaved changes are confirmed first."
    ),
    command!(
        Open,
        "file.open",
        "Open project...",
        File,
        Some(Shortcut::Open),
        true,
        "Open a .bwp project or a supported drawing."
    ),
    command!(
        Save,
        "file.save",
        "Save",
        File,
        Some(Shortcut::Save),
        false,
        "Write the project to its current path."
    ),
    command!(
        SaveAs,
        "file.save-as",
        "Save as...",
        File,
        Some(Shortcut::SaveAs),
        false,
        "Write the project to a new path."
    ),
    command!(
        SaveArchive,
        "file.save-archive",
        "Save project with images...",
        File,
        None,
        false,
        "Write a .bwpz archive that embeds every image."
    ),
    command!(
        Undo,
        "edit.undo",
        "Undo",
        Edit,
        Some(Shortcut::Undo),
        true,
        "Undo the last change."
    ),
    command!(
        Redo,
        "edit.redo",
        "Redo",
        Edit,
        Some(Shortcut::Redo),
        true,
        "Repeat the last undone change."
    ),
    command!(
        Cut,
        "edit.cut",
        "Cut",
        Edit,
        Some(Shortcut::Cut),
        true,
        "Move the selection to the clipboard."
    ),
    command!(
        Copy,
        "edit.copy",
        "Copy",
        Edit,
        Some(Shortcut::Copy),
        true,
        "Copy the selection to the clipboard."
    ),
    command!(
        Paste,
        "edit.paste",
        "Paste",
        Edit,
        Some(Shortcut::Paste),
        true,
        "Paste the clipboard into the document."
    ),
    command!(
        Duplicate,
        "edit.duplicate",
        "Duplicate",
        Edit,
        Some(Shortcut::Duplicate),
        true,
        "Copy the selection in place."
    ),
    command!(
        Delete,
        "edit.delete",
        "Delete selection",
        Edit,
        Some(Shortcut::Delete),
        true,
        "Remove the selected objects."
    ),
    command!(
        SelectAll,
        "edit.select-all",
        "Select all",
        Edit,
        Some(Shortcut::SelectAll),
        false,
        "Select every visible object."
    ),
    command!(
        SelectNone,
        "edit.select-none",
        "Select none",
        Edit,
        None,
        false,
        "Clear the selection."
    ),
    command!(
        SelectLayer,
        "edit.select-layer",
        "Select all on active layer",
        Edit,
        None,
        false,
        "Select every visible, unlocked object on the active layer."
    ),
    command!(
        SelectContained,
        "edit.select-contained",
        "Select contained shapes",
        Edit,
        None,
        false,
        "Select the shapes that lie inside the single selected closed shape."
    ),
    command!(
        SelectOpenShapes,
        "edit.select-open-shapes",
        "Select open shapes",
        Edit,
        None,
        false,
        "Select every visible path that is not closed."
    ),
    command!(
        BringForward,
        "edit.bring-forward",
        "Bring forward",
        Edit,
        None,
        false,
        "Move the selection one step up the drawing order."
    ),
    command!(
        SendBackward,
        "edit.send-backward",
        "Send backward",
        Edit,
        None,
        false,
        "Move the selection one step down the drawing order."
    ),
    command!(
        SetPosition,
        "arrange.move-to-position",
        "Move to position...",
        Edit,
        None,
        false,
        "Place the bottom-left corner of the selection at an exact X and Y."
    ),
    command!(
        ClosePaths,
        "edit.close-paths",
        "Close paths",
        Edit,
        None,
        false,
        "Close every selected open path."
    ),
    command!(
        ConvertToPath,
        "edit.convert-to-path",
        "Convert to path",
        Edit,
        None,
        false,
        "Replace the selected rectangles and ellipses with editable paths."
    ),
    command!(
        JoinPaths,
        "edit.join-paths",
        "Join paths",
        Edit,
        None,
        false,
        "Join selected paths whose ends meet."
    ),
    command!(
        BooleanUnion,
        "tools.boolean-union",
        "Boolean union",
        Tools,
        None,
        false,
        "Combine the selected closed shapes into one outline."
    ),
    command!(
        BooleanDifference,
        "tools.boolean-difference",
        "Boolean difference",
        Tools,
        None,
        false,
        "Subtract the upper shapes from the bottom one."
    ),
    command!(
        BooleanIntersection,
        "tools.boolean-intersection",
        "Boolean intersection",
        Tools,
        None,
        false,
        "Keep only the area shared by the selected shapes."
    ),
    command!(
        BooleanExclusion,
        "tools.boolean-exclusion",
        "Boolean exclusion",
        Tools,
        None,
        false,
        "Keep the areas covered by an odd number of shapes."
    ),
    command!(
        CornerRadius,
        "tools.corner-radius",
        "Corner radius...",
        Tools,
        None,
        false,
        "Round the corners of the selected rectangles."
    ),
    command!(
        ClearCornerRadius,
        "tools.clear-corner-radius",
        "Clear corner radius",
        Tools,
        None,
        false,
        "Square the corners of the selected rectangles."
    ),
    command!(
        Weld,
        "tools.weld",
        "Weld shapes",
        Tools,
        None,
        false,
        "Union every selected closed shape, merging the overlapping parts."
    ),
    command!(
        Offset,
        "tools.offset",
        "Offset...",
        Tools,
        None,
        false,
        "Offset the selected closed shapes by a distance."
    ),
    command!(
        Repeat,
        "tools.repeat",
        "Repeat selection...",
        Tools,
        None,
        false,
        "Duplicate the selection in a line, circle or grid."
    ),
    command!(
        Skew,
        "tools.skew",
        "Skew...",
        Tools,
        None,
        false,
        "Shear the selection around its centre."
    ),
    command!(
        Nest,
        "tools.nest",
        "Nest selection...",
        Tools,
        None,
        false,
        "Propose a compact arrangement of the selected parts."
    ),
    command!(
        ZoomIn,
        "view.zoom-in",
        "Zoom in",
        View,
        Some(Shortcut::ZoomIn),
        false,
        "Magnify the canvas."
    ),
    command!(
        ZoomOut,
        "view.zoom-out",
        "Zoom out",
        View,
        Some(Shortcut::ZoomOut),
        false,
        "Shrink the canvas."
    ),
    command!(
        FitPage,
        "view.fit-page",
        "Fit page",
        View,
        Some(Shortcut::Fit),
        false,
        "Fit the whole page in the window."
    ),
    command!(
        ToggleGrid,
        "view.grid",
        "Toggle grid",
        View,
        Some(Shortcut::Grid),
        false,
        "Show or hide the grid."
    ),
    command!(
        ToggleSnap,
        "view.snap",
        "Toggle snapping",
        View,
        None,
        false,
        "Snap drawing and dragging to the grid and guides."
    ),
    command!(
        ToggleToolpaths,
        "view.toolpaths",
        "Toggle toolpaths",
        View,
        None,
        false,
        "Show or hide the toolpath preview."
    ),
    command!(
        ToolSelect,
        "tool.select",
        "Tool: Select",
        Tools,
        Some(Shortcut::ToolSelect),
        false,
        "Select and move objects."
    ),
    command!(
        ToolLine,
        "tool.line",
        "Tool: Line",
        Tools,
        Some(Shortcut::ToolLine),
        false,
        "Draw a line."
    ),
    command!(
        ToolRectangle,
        "tool.rectangle",
        "Tool: Rectangle",
        Tools,
        Some(Shortcut::ToolRectangle),
        false,
        "Draw a rectangle."
    ),
    command!(
        ToolEllipse,
        "tool.ellipse",
        "Tool: Ellipse",
        Tools,
        Some(Shortcut::ToolEllipse),
        false,
        "Draw an ellipse."
    ),
    command!(
        ToolStar,
        "tool.star",
        "Tool: Star",
        Tools,
        Some(Shortcut::ToolStar),
        false,
        "Draw a star."
    ),
    command!(
        ToolText,
        "tool.text",
        "Tool: Text",
        Tools,
        Some(Shortcut::ToolText),
        false,
        "Place a text object."
    ),
    command!(
        ToolPolygon,
        "tool.polygon",
        "Tool: Polygon",
        Tools,
        Some(Shortcut::PolygonTool),
        false,
        "Draw a regular polygon."
    ),
    command!(
        ToolPencil,
        "tool.pencil",
        "Tool: Freehand",
        Tools,
        Some(Shortcut::PencilTool),
        false,
        "Draw a freehand path."
    ),
    command!(
        Import,
        "file.import",
        "Import SVG, DXF or image...",
        File,
        None,
        true,
        "Add a drawing or image to the open project."
    ),
    command!(
        ExportSvg,
        "export.svg",
        "Export SVG...",
        File,
        None,
        false,
        "Write the project as SVG."
    ),
    command!(
        ExportDxf,
        "export.dxf",
        "Export DXF...",
        File,
        None,
        false,
        "Write the project as DXF."
    ),
    command!(
        ExportEps,
        "export.eps",
        "Export EPS...",
        File,
        None,
        false,
        "Write the project as EPS."
    ),
    command!(
        ExportPdf,
        "export.pdf",
        "Export PDF...",
        File,
        None,
        false,
        "Write the project as PDF."
    ),
    command!(
        ExportImage,
        "export.image",
        "Export image...",
        File,
        None,
        false,
        "Write a PNG, BMP, TIFF or WebP image."
    ),
    command!(
        ExportGcode,
        "export.gcode",
        "Export G-code...",
        File,
        None,
        false,
        "Write the job file for the sender of your choice."
    ),
    command!(
        Cuts,
        "operations.cuts",
        "Cuts and layers...",
        Operations,
        None,
        true,
        "Edit the operations, their speed, power and passes."
    ),
    command!(
        Parameters,
        "operations.parameters",
        "Parameters...",
        Operations,
        None,
        true,
        "Edit named parameters and the expressions that use them."
    ),
    command!(
        Constraints,
        "operations.constraints",
        "Constraints...",
        Operations,
        None,
        true,
        "Add, edit and solve geometric constraints between path nodes."
    ),
    command!(
        Preview,
        "operations.preview",
        "Preview and G-code...",
        Operations,
        Some(Shortcut::Preview),
        false,
        "Inspect the toolpaths and the generated G-code."
    ),
    command!(
        MaterialTest,
        "operations.material-test",
        "Material test...",
        Operations,
        None,
        true,
        "Create a grid that tests several speeds and powers."
    ),
    command!(
        RegistrationMarks,
        "operations.registration-marks",
        "Registration marks...",
        Operations,
        None,
        true,
        "Add marks around the selection for a second pass."
    ),
    command!(
        DocumentProperties,
        "project.properties",
        "Document properties...",
        Project,
        None,
        true,
        "Edit the page size, origin and material area."
    ),
    command!(
        Preferences,
        "settings.preferences",
        "Preferences...",
        Settings,
        None,
        false,
        "Change the interface, shortcuts and defaults."
    ),
    command!(
        Workspaces,
        "settings.workspaces",
        "Workspaces...",
        Settings,
        None,
        false,
        "Save and load panel layouts."
    ),
    command!(
        InterfacePresets,
        "settings.interface-presets",
        "Interface presets...",
        Settings,
        None,
        false,
        "Apply a ready-made interface arrangement."
    ),
    command!(
        History,
        "edit.history",
        "History...",
        Edit,
        None,
        false,
        "Jump to an earlier or later snapshot."
    ),
    command!(
        MachineProfiles,
        "settings.machine-profiles",
        "Machine profiles...",
        Settings,
        None,
        false,
        "Record the work area of the equipment you use."
    ),
    command!(
        Materials,
        "materials.open",
        "Materials and operations...",
        Project,
        None,
        false,
        "Browse the material and operation library."
    ),
    command!(
        KeyboardShortcuts,
        "help.shortcuts",
        "Keyboard shortcuts",
        Help,
        None,
        false,
        "Show and remap the keyboard shortcuts."
    ),
    command!(
        Assistant,
        "help.assistant",
        "Assistant for agents...",
        Help,
        None,
        false,
        "Bridge status, token, launch lines and a prompt with the document state."
    ),
];

/// Find a catalog entry by its stable key.
pub fn find(key: &str) -> Option<&'static PaletteCommand> {
    ALL.iter().find(|command| command.key == key)
}

/// Find a catalog entry by identity.
pub fn by_id(id: CommandId) -> Option<&'static PaletteCommand> {
    ALL.iter().find(|command| command.id == id)
}

#[cfg(test)]
mod tests {
    use super::{ALL, Category, CommandId, by_id, find};

    #[test]
    fn keys_and_labels_are_unique() {
        let mut keys = std::collections::HashSet::new();
        let mut labels = std::collections::HashSet::new();
        for command in ALL {
            assert!(keys.insert(command.key), "duplicate key {}", command.key);
            assert!(
                labels.insert(command.label),
                "duplicate label {}",
                command.label
            );
            assert_ne!(command.label, "");
            assert_ne!(command.summary, "");
            assert!(by_id(command.id).is_some());
        }
    }

    #[test]
    fn the_catalog_covers_every_command() {
        let mut ids = std::collections::HashSet::new();
        for command in ALL {
            assert!(ids.insert(command.id), "duplicate id {:?}", command.id);
        }
        assert_eq!(ids.len(), ALL.len());
        assert!(ALL.len() >= 47);
    }

    #[test]
    fn entries_can_be_found_by_key() {
        let save = find("file.save").unwrap();
        assert_eq!(save.id, CommandId::Save);
        assert_eq!(save.category, Category::File);
        assert_eq!(save.default_shortcut(), Some("Ctrl+S"));
        assert!(!save.mutates);
        let cuts = find("operations.cuts").unwrap();
        assert!(cuts.mutates);
        assert!(find("missing").is_none());
    }

    #[test]
    fn json_reports_the_shortcut_and_the_scope() {
        let undo = find("edit.undo").unwrap().to_json();
        assert_eq!(undo["kind"], "command");
        assert_eq!(undo["shortcut"], "Ctrl+Z");
        assert_eq!(undo["category"], "Edit");
        assert_eq!(undo["mutates"], true);
        let zoom = find("view.zoom-out").unwrap().to_json();
        assert_eq!(zoom["shortcut_id"], "zoom_out");
    }
}
