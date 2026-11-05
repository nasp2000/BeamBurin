use std::path::{Path, PathBuf};

use crate::ui::icons::{self, Icon};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMode {
    OpenProject,
    SaveProject,
    SaveArchive,
    ImportFile,
    ImportImage,
    ExportSvg,
    ExportDxf,
    ExportHpgl,
    ExportEps,
    ExportPdf,
    ExportImage,
    ExportGcode,
    RelinkImage,
    ImportProfiles,
    ExportProfiles,
    ImportMachines,
    ExportMachines,
    ImportMaterialsCsv,
    ExportMaterialsCsv,
    PreviewGcode,
}

impl FileMode {
    fn title(self) -> &'static str {
        match self {
            FileMode::OpenProject => "Open project or SVG",
            FileMode::SaveProject => "Save project",
            FileMode::SaveArchive => "Save project with images",
            FileMode::ImportFile => "Import SVG, DXF or image",
            FileMode::ImportImage => "Import image",
            FileMode::ExportSvg => "Export SVG",
            FileMode::ExportDxf => "Export DXF",
            FileMode::ExportHpgl => "Export HPGL",
            FileMode::ExportEps => "Export EPS",
            FileMode::ExportPdf => "Export PDF",
            FileMode::ExportImage => "Export image",
            FileMode::ExportGcode => "Export G-code",
            FileMode::RelinkImage => "Relink image",
            FileMode::ImportProfiles => "Import profiles",
            FileMode::ExportProfiles => "Export profiles",
            FileMode::ImportMachines => "Import machine profiles",
            FileMode::ExportMachines => "Export machine profiles",
            FileMode::ImportMaterialsCsv => "Import materials CSV",
            FileMode::ExportMaterialsCsv => "Export materials CSV",
            FileMode::PreviewGcode => "Preview external G-code",
        }
    }

    fn action_label(self) -> &'static str {
        match self {
            FileMode::OpenProject
            | FileMode::ImportFile
            | FileMode::ImportImage
            | FileMode::PreviewGcode => "Open",
            FileMode::SaveProject | FileMode::SaveArchive => "Save",
            FileMode::ExportSvg
            | FileMode::ExportDxf
            | FileMode::ExportHpgl
            | FileMode::ExportEps
            | FileMode::ExportPdf
            | FileMode::ExportImage
            | FileMode::ExportGcode
            | FileMode::ExportProfiles
            | FileMode::ExportMachines
            | FileMode::ExportMaterialsCsv => "Export",
            FileMode::RelinkImage => "Relink",
            FileMode::ImportProfiles | FileMode::ImportMachines | FileMode::ImportMaterialsCsv => {
                "Import"
            }
        }
    }

    fn default_extension(self) -> &'static str {
        match self {
            FileMode::SaveProject => "bwp",
            FileMode::SaveArchive => "bwpz",
            FileMode::ExportGcode | FileMode::PreviewGcode => "nc",
            FileMode::ExportDxf => "dxf",
            FileMode::ExportHpgl => "plt",
            FileMode::ExportEps => "eps",
            FileMode::ExportPdf => "pdf",
            FileMode::ExportImage | FileMode::RelinkImage => "png",
            FileMode::OpenProject | FileMode::ImportFile | FileMode::ExportSvg => "svg",
            FileMode::ImportImage => "png",
            FileMode::ImportProfiles
            | FileMode::ExportProfiles
            | FileMode::ImportMachines
            | FileMode::ExportMachines => "json",
            FileMode::ImportMaterialsCsv | FileMode::ExportMaterialsCsv => "csv",
        }
    }

    /// Human-readable list of what the browser shows for this mode.
    fn filter_hint(self) -> &'static str {
        match self {
            FileMode::OpenProject => "Shows .bwp and .svg files",
            FileMode::ImportProfiles | FileMode::ExportProfiles => "Shows .json profile libraries",
            FileMode::ImportMachines | FileMode::ExportMachines => "Shows .json machine libraries",
            FileMode::ImportMaterialsCsv | FileMode::ExportMaterialsCsv => {
                "Shows .csv material lists"
            }
            FileMode::SaveProject => "Saves .bwp files",
            FileMode::SaveArchive => "Saves .bwpz files with the images inside",
            FileMode::ImportFile => {
                "Shows .svg, .dxf, .plt, .hpgl, .pdf, .png, .jpg, .jpeg, .bmp, .tif, .tiff, .webp and .gif files"
            }
            FileMode::ExportSvg => "Saves .svg files",
            FileMode::ExportDxf => "Saves .dxf files",
            FileMode::ExportHpgl => "Saves .plt and .hpgl files",
            FileMode::ExportEps => "Saves .eps files",
            FileMode::ExportPdf => "Saves .pdf files",
            FileMode::ExportImage => "Saves .png, .bmp, .tif, .tiff and .webp files",
            FileMode::ExportGcode => {
                "Saves .nc, .gcode, .gco, .gc, .ngc, .tap, .cnc, .mpf and .txt files"
            }
            FileMode::RelinkImage => {
                "Shows .png, .jpg, .jpeg, .bmp, .tif, .tiff, .webp and .gif files"
            }
            FileMode::PreviewGcode => {
                "Shows .nc, .gcode, .gco, .gc, .ngc, .tap, .cnc, .mpf and .txt files"
            }
        }
    }

    fn accepts(self, path: &Path) -> bool {
        let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
            return false;
        };
        let extension = extension.to_ascii_lowercase();
        match self {
            FileMode::OpenProject => {
                extension == "bwp" || extension == "bwpz" || extension == "svg"
            }
            FileMode::SaveProject => extension == "bwp",
            FileMode::SaveArchive => extension == "bwpz",
            FileMode::ImportFile => {
                matches!(
                    extension.as_str(),
                    "svg"
                        | "dxf"
                        | "plt"
                        | "hpgl"
                        | "pdf"
                        | "png"
                        | "jpg"
                        | "jpeg"
                        | "bmp"
                        | "tif"
                        | "tiff"
                        | "webp"
                        | "gif"
                )
            }
            FileMode::ImportImage => {
                matches!(
                    extension.as_str(),
                    "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff" | "webp" | "gif"
                )
            }
            FileMode::ExportSvg => extension == "svg",
            FileMode::ExportDxf => extension == "dxf",
            FileMode::ExportHpgl => extension == "plt" || extension == "hpgl",
            FileMode::ExportEps => extension == "eps",
            FileMode::ExportPdf => extension == "pdf",
            FileMode::ExportImage => {
                matches!(extension.as_str(), "png" | "bmp" | "tif" | "tiff" | "webp")
            }
            FileMode::ExportGcode => matches!(
                extension.as_str(),
                "nc" | "gcode" | "gco" | "gc" | "ngc" | "tap" | "cnc" | "mpf" | "txt"
            ),
            FileMode::RelinkImage => {
                matches!(
                    extension.as_str(),
                    "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff" | "webp" | "gif"
                )
            }
            FileMode::PreviewGcode => matches!(
                extension.as_str(),
                "nc" | "gcode" | "gco" | "gc" | "ngc" | "tap" | "cnc" | "mpf" | "txt"
            ),
            FileMode::ImportProfiles
            | FileMode::ExportProfiles
            | FileMode::ImportMachines
            | FileMode::ExportMachines => extension == "json",
            FileMode::ImportMaterialsCsv | FileMode::ExportMaterialsCsv => extension == "csv",
        }
    }
}

pub struct FileDialog {
    pub mode: FileMode,
    pub directory: PathBuf,
    pub file_name: String,
    pub error: Option<String>,
    /// List every file, not only the formats the mode accepts.
    show_all_files: bool,
    /// Editable copy of the current folder, so a path can be typed.
    path_text: String,
    /// Folder history for the Back/Forward buttons.
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
    /// Row highlighted in the listing, so a second click opens it.
    selected: Option<PathBuf>,
}

pub enum DialogResult {
    Pending,
    Cancelled,
    Accepted(PathBuf),
}

impl FileDialog {
    pub fn new(mode: FileMode, start: Option<&Path>, suggested_name: &str) -> Self {
        let (directory, file_name) = match start {
            Some(path) if path.is_dir() => (path.to_path_buf(), suggested_name.to_string()),
            Some(path) => (
                path.parent()
                    .map_or_else(|| PathBuf::from("."), Path::to_path_buf),
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(suggested_name)
                    .to_string(),
            ),
            None => (
                home_directory()
                    .or_else(|| std::env::current_dir().ok())
                    .unwrap_or_else(|| PathBuf::from(".")),
                suggested_name.to_string(),
            ),
        };
        let path_text = directory.display().to_string();
        Self {
            mode,
            directory,
            file_name,
            error: None,
            show_all_files: false,
            path_text,
            back: Vec::new(),
            forward: Vec::new(),
            selected: None,
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn show(&mut self, ctx: &egui::Context) -> DialogResult {
        let mut result = DialogResult::Pending;
        let mut open = true;
        egui::Window::new(self.mode.title())
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .collapsible(false)
            .resizable(true)
            .default_size([880.0, 660.0])
            .min_width(760.0)
            .min_height(560.0)
            .max_width(1000.0)
            .open(&mut open)
            .show(ctx, |ui| {
                let mut accept = false;
                let mut cancel = false;
                // Explorer-style address bar with history.
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(!self.back.is_empty(), egui::Button::new("Back"))
                        .on_hover_text("Previous folder")
                        .clicked()
                    {
                        self.go_back();
                    }
                    if ui
                        .add_enabled(!self.forward.is_empty(), egui::Button::new("Forward"))
                        .on_hover_text("Next folder")
                        .clicked()
                    {
                        self.go_forward();
                    }
                    if ui.button("Up").on_hover_text("Parent folder").clicked() {
                        if let Some(parent) = self.directory.parent() {
                            self.navigate(parent.to_path_buf());
                        }
                    }
                    if ui.button("Home").on_hover_text("Home folder").clicked() {
                        if let Some(home) = home_directory() {
                            self.navigate(home);
                        }
                    }
                    let width = (ui.available_width() - 170.0).max(120.0);
                    let response = ui
                        .add(egui::TextEdit::singleline(&mut self.path_text).desired_width(width));
                    let go = ui.button("Go").clicked()
                        || (response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter)));
                    if go {
                        self.navigate(PathBuf::from(self.path_text.trim()));
                    }
                    if ui
                        .button("New folder")
                        .on_hover_text("Create a folder in this location")
                        .clicked()
                    {
                        self.create_folder();
                    }
                });
                ui.separator();
                let mut target: Option<PathBuf> = None;
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(150.0, 330.0),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("dialog_places")
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    ui.label(egui::RichText::new("Quick access").strong());
                                    for (label, path) in places() {
                                        ui.horizontal(|ui| {
                                            ui.add(icons::image_at(ui.ctx(), Icon::Folder, 15.0));
                                            if ui
                                                .selectable_label(self.directory == path, label)
                                                .on_hover_text(path.display().to_string())
                                                .clicked()
                                            {
                                                target = Some(path);
                                            }
                                        });
                                    }
                                });
                        },
                    );
                    ui.separator();
                    ui.vertical(|ui| {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt("dialog_files")
                                .max_height(300.0)
                                .auto_shrink([false, false])
                                .show(ui, |ui| match self.listing(ui) {
                                    ListingAction::None => {}
                                    ListingAction::Navigate(path) => target = Some(path),
                                    ListingAction::Accept(path) => {
                                        if let Some(name) =
                                            path.file_name().and_then(|name| name.to_str())
                                        {
                                            self.file_name = name.to_string();
                                        }
                                        accept = true;
                                    }
                                });
                        });
                        ui.add_space(4.0);
                        ui.weak(format!("{} item(s)", self.item_count()));
                    });
                });
                if let Some(path) = target {
                    self.navigate(path);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("File name:");
                    let response = ui
                        .add(egui::TextEdit::singleline(&mut self.file_name).desired_width(520.0));
                    if response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        accept = true;
                    }
                });
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::from_rgb(200, 60, 60), error);
                }
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.show_all_files, "Show all files");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(self.mode.action_label()).clicked() {
                            accept = true;
                        }
                        if ui.button("Cancel").clicked() {
                            cancel = true;
                        }
                        ui.label(self.mode.filter_hint());
                    });
                });
                if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
                if accept {
                    match self.resolve() {
                        Ok(path) => result = DialogResult::Accepted(path),
                        Err(error) => self.error = Some(error),
                    }
                }
                if cancel {
                    result = DialogResult::Cancelled;
                }
            });
        if !open && matches!(result, DialogResult::Pending) {
            result = DialogResult::Cancelled;
        }
        result
    }

    fn navigate(&mut self, target: PathBuf) {
        if !target.is_dir() {
            self.error = Some(format!("Folder not found: {}", target.display()));
            return;
        }
        if self.directory != target {
            self.back.push(self.directory.clone());
            self.forward.clear();
            self.directory = target;
        }
        self.path_text = self.directory.display().to_string();
        self.selected = None;
        self.error = None;
    }

    fn go_back(&mut self) {
        if let Some(previous) = self.back.pop() {
            let current = std::mem::replace(&mut self.directory, previous);
            self.forward.push(current);
            self.path_text = self.directory.display().to_string();
            self.selected = None;
            self.error = None;
        }
    }

    fn go_forward(&mut self) {
        if let Some(next) = self.forward.pop() {
            let current = std::mem::replace(&mut self.directory, next);
            self.back.push(current);
            self.path_text = self.directory.display().to_string();
            self.selected = None;
            self.error = None;
        }
    }

    fn create_folder(&mut self) {
        let mut candidate = self.directory.join("New folder");
        let mut index = 2;
        while candidate.exists() {
            candidate = self.directory.join(format!("New folder {index}"));
            index += 1;
        }
        match std::fs::create_dir(&candidate) {
            Ok(()) => {
                self.selected = Some(candidate);
                self.error = None;
            }
            Err(error) => {
                self.error = Some(format!("Cannot create folder: {error}"));
            }
        }
    }

    fn item_count(&self) -> usize {
        let Ok(entries) = std::fs::read_dir(&self.directory) else {
            return 0;
        };
        entries
            .flatten()
            .filter(|entry| {
                let path = entry.path();
                path.is_dir() || self.show_all_files || self.mode.accepts(&path)
            })
            .count()
    }

    fn listing(&mut self, ui: &mut egui::Ui) -> ListingAction {
        let entries = match std::fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) => {
                ui.colored_label(
                    egui::Color32::from_rgb(200, 60, 60),
                    format!("Cannot read directory: {error}"),
                );
                return ListingAction::None;
            }
        };
        let mut directories = Vec::new();
        let mut files = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                directories.push(path);
            } else if self.show_all_files || self.mode.accepts(&path) {
                files.push(path);
            }
        }
        directories.sort();
        files.sort();
        if directories.is_empty() && files.is_empty() {
            ui.weak("This folder is empty or has no supported files.");
            return ListingAction::None;
        }
        let mut action = ListingAction::None;
        egui::Grid::new("dialog_listing")
            .num_columns(3)
            .striped(true)
            .spacing([8.0, 2.0])
            .show(ui, |ui| {
                ui.strong("Name");
                ui.strong("Type");
                ui.strong("Size");
                ui.end_row();
                for directory in directories {
                    let name = directory
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("?")
                        .to_string();
                    ui.horizontal(|ui| {
                        ui.add(icons::image_at(ui.ctx(), Icon::Folder, 16.0));
                        let selected = self.selected.as_deref() == Some(directory.as_path());
                        let response = ui
                            .selectable_label(selected, name.as_str())
                            .on_hover_text("Double-click to open this folder");
                        if response.clicked() {
                            self.selected = Some(directory.clone());
                        }
                        if response.double_clicked() {
                            self.selected = Some(directory.clone());
                            action = ListingAction::Navigate(directory.clone());
                        }
                    });
                    ui.label("Folder");
                    ui.weak("—");
                    ui.end_row();
                }
                for file in files {
                    let name = file
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("?");
                    ui.horizontal(|ui| {
                        ui.add(icons::image_at(ui.ctx(), file_icon(&file), 16.0));
                        let selected = self.selected.as_deref() == Some(file.as_path());
                        let response = ui
                            .selectable_label(selected, name)
                            .on_hover_text("Double-click to choose this file");
                        if response.clicked() {
                            self.selected = Some(file.clone());
                            self.file_name = name.to_string();
                            self.error = None;
                        }
                        if response.double_clicked() {
                            self.selected = Some(file.clone());
                            self.file_name = name.to_string();
                            self.error = None;
                            action = ListingAction::Accept(file.clone());
                        }
                    });
                    let kind = file
                        .extension()
                        .and_then(|value| value.to_str())
                        .map_or_else(|| "File".to_string(), str::to_ascii_uppercase);
                    ui.label(kind);
                    let size = std::fs::metadata(&file)
                        .map_or_else(|_| "—".to_string(), |metadata| format_size(metadata.len()));
                    ui.weak(size);
                    ui.end_row();
                }
            });
        action
    }

    fn resolve(&self) -> Result<PathBuf, String> {
        let name = self.file_name.trim();
        if name.is_empty() {
            return Err("Enter a file name".to_string());
        }
        let mut path = self.directory.join(name);
        if path.extension().is_none() {
            path.set_extension(self.mode.default_extension());
        }
        if !self.mode.accepts(&path) {
            if matches!(
                self.mode,
                FileMode::SaveProject
                    | FileMode::ExportSvg
                    | FileMode::ExportDxf
                    | FileMode::ExportImage
                    | FileMode::ExportGcode
                    | FileMode::ExportProfiles
                    | FileMode::ExportMachines
                    | FileMode::ExportMaterialsCsv
            ) {
                path.set_extension(self.mode.default_extension());
            } else {
                return Err(format!("Use a .{} file", self.mode.default_extension()));
            }
        }
        match self.mode {
            FileMode::OpenProject
            | FileMode::ImportFile
            | FileMode::ImportImage
            | FileMode::RelinkImage
            | FileMode::ImportProfiles
            | FileMode::ImportMachines
            | FileMode::ImportMaterialsCsv
            | FileMode::PreviewGcode => {
                if !path.is_file() {
                    return Err(format!("File not found: {}", path.display()));
                }
            }
            FileMode::SaveProject
            | FileMode::SaveArchive
            | FileMode::ExportSvg
            | FileMode::ExportDxf
            | FileMode::ExportHpgl
            | FileMode::ExportEps
            | FileMode::ExportPdf
            | FileMode::ExportImage
            | FileMode::ExportGcode
            | FileMode::ExportProfiles
            | FileMode::ExportMachines
            | FileMode::ExportMaterialsCsv => {
                if let Some(parent) = path.parent() {
                    if !parent.is_dir() {
                        return Err(format!("Directory not found: {}", parent.display()));
                    }
                }
            }
        }
        Ok(path)
    }
}

/// What the listing asks the dialog to do after a click.
enum ListingAction {
    None,
    Navigate(PathBuf),
    Accept(PathBuf),
}

fn file_icon(path: &Path) -> Icon {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "bmp" | "tif" | "tiff" | "webp" | "gif" => Icon::Image,
        "bwp" | "bwpz" | "svg" | "dxf" | "plt" | "hpgl" | "eps" | "pdf" | "nc" | "gcode"
        | "gco" | "gc" | "ngc" | "tap" | "cnc" | "mpf" | "json" | "csv" | "txt" => Icon::Page,
        _ => Icon::File,
    }
}

fn home_directory() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn places() -> Vec<(&'static str, PathBuf)> {
    let Some(home) = home_directory() else {
        return Vec::new();
    };
    let mut places = vec![("Home", home.clone())];
    for (label, name) in [
        ("Desktop", "Desktop"),
        ("Documents", "Documents"),
        ("Downloads", "Downloads"),
    ] {
        let path = home.join(name);
        if path.is_dir() {
            places.push((label, path));
        }
    }
    if cfg!(windows) {
        places.push(("This PC", PathBuf::from(r"C:\")));
    } else {
        places.push(("Root", PathBuf::from("/")));
    }
    places
}

#[allow(clippy::cast_precision_loss)]
fn format_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * 1024;
    if bytes >= MIB {
        format!("{:.1} MB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.0} KB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::{FileDialog, FileMode, file_icon, format_size};
    use crate::ui::icons::Icon;

    fn scratch(name: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let path = std::env::temp_dir().join(format!(
            "beamburin-dialog-{name}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("the scratch folder was not created");
        path
    }

    #[test]
    fn open_accepts_projects_and_svg() {
        assert!(FileMode::OpenProject.accepts(std::path::Path::new("a.bwp")));
        assert!(FileMode::OpenProject.accepts(std::path::Path::new("a.SVG")));
        assert!(!FileMode::OpenProject.accepts(std::path::Path::new("a.dxf")));
    }

    #[test]
    fn save_adds_default_extension() {
        let mut dialog = FileDialog::new(
            FileMode::SaveProject,
            Some(std::path::Path::new("/tmp/beamburin-dialog-test")),
            "untitled",
        );
        dialog.directory = std::env::temp_dir();
        dialog.file_name = "beamburin-dialog-test".to_string();
        let path = dialog.resolve().unwrap();
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("bwp")
        );
    }

    #[test]
    fn external_gcode_preview_accepts_the_common_extensions() {
        for name in [
            "job.nc",
            "job.gcode",
            "job.gco",
            "job.gc",
            "job.ngc",
            "job.tap",
            "job.cnc",
            "job.mpf",
            "job.txt",
        ] {
            let path = std::path::Path::new(name);
            assert!(FileMode::PreviewGcode.accepts(path), "{name} was rejected");
        }
        assert!(!FileMode::PreviewGcode.accepts(std::path::Path::new("job.bwp")));
        assert!(!FileMode::PreviewGcode.accepts(std::path::Path::new("job.svg")));
        assert_eq!(FileMode::PreviewGcode.action_label(), "Open");
    }

    #[test]
    fn gcode_export_uses_nc_extension() {
        let mut dialog = FileDialog::new(
            FileMode::ExportGcode,
            Some(std::path::Path::new("/tmp/beamburin-job")),
            "job",
        );
        dialog.directory = std::env::temp_dir();
        dialog.file_name = "beamburin-job".to_string();
        let path = dialog.resolve().unwrap();
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("nc")
        );
        assert!(FileMode::ExportGcode.accepts(std::path::Path::new("a.gcode")));
        assert!(!FileMode::ExportGcode.accepts(std::path::Path::new("a.svg")));
    }

    #[test]
    fn import_hint_lists_images_and_vectors() {
        let hint = FileMode::ImportFile.filter_hint();
        for extension in [
            ".svg", ".dxf", ".png", ".jpg", ".jpeg", ".bmp", ".tif", ".tiff",
        ] {
            assert!(hint.contains(extension), "{hint} misses {extension}");
        }
        assert!(FileMode::OpenProject.filter_hint().contains(".bwp"));
    }

    #[test]
    fn a_start_folder_keeps_the_suggested_name() {
        let folder = std::env::temp_dir();
        let dialog = FileDialog::new(FileMode::ExportGcode, Some(&folder), "job.nc");
        assert_eq!(dialog.directory, folder);
        assert_eq!(dialog.file_name, "job.nc");
    }

    #[test]
    fn a_new_dialog_starts_in_a_real_folder() {
        let dialog = FileDialog::new(FileMode::ImportFile, None, "drawing.svg");
        assert!(
            dialog.directory.is_dir(),
            "{} is not a folder",
            dialog.directory.display()
        );
    }

    #[test]
    fn material_csv_uses_the_csv_extension() {
        assert!(FileMode::ImportMaterialsCsv.accepts(std::path::Path::new("m.csv")));
        assert!(FileMode::ExportMaterialsCsv.accepts(std::path::Path::new("M.CSV")));
        assert!(!FileMode::ExportMaterialsCsv.accepts(std::path::Path::new("m.json")));
        let mut dialog = FileDialog::new(
            FileMode::ExportMaterialsCsv,
            Some(std::path::Path::new("/tmp/beamburin-csv")),
            "beamburin-csv",
        );
        dialog.directory = std::env::temp_dir();
        dialog.file_name = "beamburin-csv".to_string();
        let path = dialog.resolve().unwrap();
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("csv")
        );
    }

    #[test]
    fn profile_libraries_use_json() {
        let mut dialog = FileDialog::new(
            FileMode::ExportProfiles,
            Some(std::path::Path::new("/tmp/profiles")),
            "profiles",
        );
        dialog.directory = std::env::temp_dir();
        dialog.file_name = "beamburin-profiles".to_string();
        let path = dialog.resolve().unwrap();
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("json")
        );
        assert!(FileMode::ImportProfiles.accepts(std::path::Path::new("a.json")));
        assert!(FileMode::ExportProfiles.filter_hint().contains("json"));
    }

    #[test]
    fn the_listing_chooses_an_icon_per_file_type() {
        assert_eq!(file_icon(std::path::Path::new("photo.PNG")), Icon::Image);
        assert_eq!(file_icon(std::path::Path::new("drawing.svg")), Icon::Page);
        assert_eq!(file_icon(std::path::Path::new("job.gcode")), Icon::Page);
        assert_eq!(file_icon(std::path::Path::new("notes.xyz")), Icon::File);
    }

    #[test]
    fn sizes_read_like_a_file_manager() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2048), "2 KB");
        assert_eq!(format_size(3 * 1024 * 1024), "3.0 MB");
    }

    #[test]
    fn new_folders_get_a_free_name() {
        let root = scratch("folders");
        let mut dialog = FileDialog::new(FileMode::ExportSvg, Some(&root), "a.svg");
        dialog.create_folder();
        assert!(root.join("New folder").is_dir());
        dialog.create_folder();
        assert!(root.join("New folder 2").is_dir());
        assert_eq!(
            dialog.selected.as_deref(),
            Some(root.join("New folder 2").as_path())
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_dialog_shows_the_system_style_actions() {
        use egui_kittest::kittest::Queryable as _;

        let root = scratch("actions");
        let dialog = FileDialog::new(FileMode::ImportFile, Some(&root), "");
        let mut harness = egui_kittest::Harness::new_ui_state(
            |ui, dialog: &mut FileDialog| {
                let _ = dialog.show(ui.ctx());
            },
            dialog,
        );
        harness.run();
        assert!(harness.query_by_label("Cancel").is_some());
        assert!(harness.query_by_label("Open").is_some());
        assert!(harness.query_by_label("New folder").is_some());
        assert!(harness.query_by_label("Quick access").is_some());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_image_import_mode_only_accepts_images() {
        let dialog = FileDialog::new(FileMode::ImportImage, None, "");
        assert!(dialog.mode.accepts(Path::new("photo.PNG")));
        assert!(dialog.mode.accepts(Path::new("scan.bmp")));
        assert!(dialog.mode.accepts(Path::new("scan.webp")));
        assert!(!dialog.mode.accepts(Path::new("drawing.svg")));
        assert_eq!(dialog.mode.default_extension(), "png");
        assert_eq!(dialog.mode.action_label(), "Open");
    }

    #[test]
    fn import_rejects_missing_file() {
        let mut dialog = FileDialog::new(
            FileMode::ImportFile,
            Some(std::path::Path::new("/tmp")),
            "missing.svg",
        );
        dialog.file_name = "definitely-not-here-12345.svg".to_string();
        assert!(dialog.resolve().is_err());
    }
}
