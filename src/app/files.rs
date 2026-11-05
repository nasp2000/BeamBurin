use super::{
    BeamBurinApp, Document, FileDialog, FileMode, FileReport, Path, TaskResult, Unit, run_export,
    sanitize_name,
};

impl BeamBurinApp {
    pub(super) fn new_document(&mut self) {
        let mut document = Document::default();
        if let Some(unit) = Unit::from_label(self.preferences.general.default_unit.trim()) {
            document.unit = unit;
        }
        self.editor.set_document(document.clone());
        self.saved_snapshot = Some(document);
        self.current_path = None;
        self.needs_fit = true;
        self.drag = None;
        let _ = std::fs::remove_file(&self.recovery_path);
        self.status = "New document".to_string();
    }

    pub(super) fn open_dialog(&mut self, mode: FileMode) {
        let stem = sanitize_name(&self.editor.document.name);
        let suggested = match mode {
            FileMode::ImportFile => "drawing.svg".to_string(),
            FileMode::ImportImage | FileMode::RelinkImage => "image.png".to_string(),
            FileMode::ExportDxf => format!("{stem}.dxf"),
            FileMode::ExportHpgl => format!("{stem}.plt"),
            FileMode::ExportEps => format!("{stem}.eps"),
            FileMode::ExportPdf => format!("{stem}.pdf"),
            FileMode::ExportImage => format!("{stem}.png"),
            FileMode::ExportSvg => format!("{stem}.svg"),
            FileMode::ExportGcode => format!("{stem}.nc"),
            FileMode::PreviewGcode => "job.nc".to_string(),
            FileMode::ImportProfiles | FileMode::ExportProfiles => "profiles.json".to_string(),
            FileMode::ImportMaterialsCsv | FileMode::ExportMaterialsCsv => {
                "materials.csv".to_string()
            }
            FileMode::ImportMachines | FileMode::ExportMachines => "machines.json".to_string(),
            FileMode::OpenProject | FileMode::SaveProject => format!("{stem}.bwp"),
            FileMode::SaveArchive => format!("{stem}.bwpz"),
        };
        let default_directory = self.preferences.general.default_directory.trim();
        let start = self.current_path.clone().or_else(|| {
            if default_directory.is_empty() {
                None
            } else {
                Some(std::path::PathBuf::from(default_directory))
            }
        });
        // The in-app browser is used everywhere: it needs no desktop portal,
        // works on the portable Windows build and cannot block the interface
        // waiting for a dialog no window manager shows.
        self.dialog = Some(FileDialog::new(mode, start.as_deref(), &suggested));
    }

    pub(super) fn handle_file(&mut self, mode: FileMode, path: &Path) {
        if self.recover_next && matches!(mode, FileMode::OpenProject) {
            self.recover_next = false;
            self.recover_project(path);
            return;
        }
        let selection_only = self.export_subset && !self.editor.selection.is_empty();
        let offset_origin = self.export_offset_origin;
        let origin = self.gcode_origin;
        self.export_subset = false;
        self.export_offset_origin = false;
        let document = if selection_only {
            self.editor.document.subset(self.editor.selection.ids())
        } else {
            self.editor.document.clone()
        };
        match mode {
            FileMode::OpenProject | FileMode::ImportFile | FileMode::ImportImage => {
                self.start_import(path);
            }
            FileMode::PreviewGcode => self.preview_external_gcode(path),
            FileMode::ImportMaterialsCsv => self.import_materials_csv(path),
            FileMode::ExportMaterialsCsv => self.export_materials_csv(path),
            FileMode::ImportProfiles => {
                match crate::profiles::library::import_into(&mut self.profiles, path) {
                    Ok(count) => {
                        match crate::profiles::library::save(&self.profiles, &self.profiles_path) {
                            Ok(()) => {
                                self.status = format!("Imported {count} profile(s)");
                            }
                            Err(error) => {
                                self.status = format!("Cannot save the profile library: {error}");
                            }
                        }
                    }
                    Err(error) => self.status = format!("Profile import failed: {error}"),
                }
            }
            FileMode::ExportProfiles => {
                match crate::profiles::library::export(&self.profiles, path) {
                    Ok(()) => self.status = format!("Exported profiles to {}", path.display()),
                    Err(error) => self.status = format!("Profile export failed: {error}"),
                }
            }
            FileMode::ImportMachines => match self.machines.import_into(path) {
                Ok(count) => match self.machines.save(&self.machines_path) {
                    Ok(()) => {
                        self.status = format!("Imported {count} machine profile(s)");
                    }
                    Err(error) => {
                        self.status = format!("Cannot save the machine library: {error}");
                    }
                },
                Err(error) => self.status = format!("Machine import failed: {error}"),
            },
            FileMode::ExportMachines => match self.machines.export_to(path) {
                Ok(()) => {
                    self.status = format!("Exported machine profiles to {}", path.display());
                }
                Err(error) => self.status = format!("Machine export failed: {error}"),
            },
            FileMode::SaveProject => self.save_to(path),
            FileMode::SaveArchive => self.save_archive(path),
            FileMode::ExportImage
            | FileMode::ExportDxf
            | FileMode::ExportHpgl
            | FileMode::ExportEps
            | FileMode::ExportPdf
            | FileMode::ExportGcode
            | FileMode::ExportSvg => self.start_export(mode, path, document, offset_origin),
            FileMode::RelinkImage => self.relink_image(path),
        }
    }

    pub(super) fn relink_image(&mut self, path: &Path) {
        let Some(object_id) = self.relink_target.take() else {
            self.status = "No image selected for relinking".to_string();
            return;
        };
        match crate::raster::image_dimensions(path) {
            Ok((width, height)) => {
                match self.editor.relink_image(&object_id, path, width, height) {
                    Ok(()) => {
                        self.status = format!("Relinked image to {}", path.display());
                    }
                    Err(error) => self.status = format!("Relink failed: {error}"),
                }
            }
            Err(error) => self.status = format!("Relink failed: {error}"),
        }
    }

    pub(super) fn collect_images(&mut self) {
        let Some(project) = self.current_path.clone() else {
            self.status = "Save the project before collecting its images".to_string();
            return;
        };
        let stem = project
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("project");
        let directory = project
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!("{stem}_assets"));
        let has_images = self
            .editor
            .document
            .objects
            .iter()
            .any(|object| matches!(object.shape, crate::document::Shape::Image(_)));
        if !has_images {
            self.status = "The project has no images".to_string();
            return;
        }
        self.editor.record_action("Collect images");
        match self.editor.document.collect_images(&directory) {
            Ok(report) => {
                let mut file_report =
                    FileReport::new("Collect images", directory.display().to_string())
                        .with_summary("Copied", report.collected.len().to_string())
                        .with_warnings(report.missing.clone());
                if report.missing.is_empty() {
                    file_report.summary.push((
                        "Next".to_string(),
                        "save the project to keep the new paths".to_string(),
                    ));
                }
                self.status = file_report.headline();
                self.show_report(file_report);
            }
            Err(error) => {
                self.status = format!("Collect failed: {error}");
            }
        }
    }

    pub(super) fn recover_project(&mut self, path: &Path) {
        let backup = crate::document::serialization::backup_path(path);
        match crate::document::serialization::load_from_file(&backup) {
            Ok(document) => {
                self.editor.set_document(document);
                self.current_path = Some(path.to_path_buf());
                self.saved_snapshot = None;
                self.needs_fit = true;
                let report = FileReport::new("Recover project", path.display().to_string())
                    .with_summary("Backup", backup.display().to_string())
                    .with_summary("Objects", self.editor.document.objects.len().to_string())
                    .with_warnings(vec![
                        "Recovered the previous version; the current project file was not read."
                            .to_string(),
                    ]);
                self.status = format!("Recovered {}", backup.display());
                self.show_report(report);
            }
            Err(error) => {
                self.status = format!("Recovery failed: {error}");
            }
        }
    }

    pub(super) fn open_path(&mut self, path: &Path) {
        match crate::import::format::import_with_report_limited(
            path,
            self.preferences.drawing.max_image_pixels,
        ) {
            Ok(outcome) => self.apply_import(path, outcome),
            Err(error) => self.status = format!("Open failed: {error}"),
        }
    }

    pub(super) fn start_import(&mut self, path: &Path) {
        if self.task.is_some() {
            self.status = "Another import is still running".to_string();
            return;
        }
        let path = path.to_path_buf();
        let limit = self.preferences.drawing.max_image_pixels;
        let label = format!("Importing {}", path.display());
        let worker_path = path.clone();
        self.task = Some(crate::task::BackgroundTask::spawn(label, move |cancel| {
            let cancelled = || cancel.load(std::sync::atomic::Ordering::Relaxed);
            let outcome = crate::import::format::import_with_report_cancellable(
                &worker_path,
                limit,
                &cancelled,
            )
            .map_err(|error| error.to_string());
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return None;
            }
            Some(TaskResult::Import {
                path: worker_path,
                outcome,
            })
        }));
        self.status = "Importing...".to_string();
    }

    pub(super) fn start_export(
        &mut self,
        mode: FileMode,
        path: &Path,
        document: crate::document::Document,
        offset_origin: bool,
    ) {
        if self.task.is_some() {
            self.status = "Another operation is still running".to_string();
            return;
        }
        let path = path.to_path_buf();
        let gcode = crate::gcode::GCodeOptions {
            dialect: self.gcode_dialect,
            decimals: self.gcode_decimals,
            include_comments: self.gcode_comments,
            include_travel: self.gcode_travel,
            line_ending_crlf: self.gcode_crlf,
            arcs: self.gcode_arcs,
            ..crate::gcode::GCodeOptions::default()
        };
        let smooth_mm = self.gcode_smooth_mm;
        let label = format!("Exporting {}", path.display());
        let worker_path = path.clone();
        self.task = Some(crate::task::BackgroundTask::spawn(label, move |cancel| {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return None;
            }
            Some(TaskResult::Export(run_export(
                mode,
                &worker_path,
                &document,
                cancel,
                offset_origin,
                origin,
                gcode,
                smooth_mm,
            )))
        }));
        self.status = "Exporting...".to_string();
    }

    pub(super) fn apply_import(
        &mut self,
        path: &Path,
        outcome: crate::import::format::ImportOutcome,
    ) {
        let mut document = outcome.document;
        let object_count = document.objects.len();
        let layer_count = document.layers.len();
        let is_project = path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| {
                value.eq_ignore_ascii_case("bwp") || value.eq_ignore_ascii_case("bwpz")
            });
        if !is_project {
            if let Some(stem) = path.file_stem().and_then(|value| value.to_str()) {
                document.name = stem.to_string();
            }
        }
        self.editor.set_document(document.clone());
        self.current_path = if is_project {
            Some(path.to_path_buf())
        } else {
            None
        };
        self.saved_snapshot = if is_project { Some(document) } else { None };
        self.needs_fit = true;
        self.drag = None;
        let _ = std::fs::remove_file(&self.recovery_path);
        let report = FileReport::new(
            if is_project { "Open project" } else { "Import" },
            path.display().to_string(),
        )
        .with_summary(
            "Format",
            crate::import::format::detect_format(path).to_uppercase(),
        )
        .with_summary("Objects", object_count.to_string())
        .with_summary("Layers", layer_count.to_string())
        .with_warnings(outcome.warnings);
        self.status = report.headline();
        self.show_report(report);
    }

    pub(super) fn save(&mut self) {
        if let Some(path) = self.current_path.clone() {
            self.save_to(&path);
        } else {
            self.open_dialog(FileMode::SaveProject);
        }
    }

    pub(super) fn save_to(&mut self, path: &Path) {
        if self.editor.document.name.trim().is_empty()
            || self.editor.document.name.eq_ignore_ascii_case("untitled")
        {
            if let Some(stem) = path.file_stem().and_then(|value| value.to_str()) {
                self.editor.document.name = stem.to_string();
            }
        }
        match self
            .editor
            .save_with_backup(path, self.preferences.general.backup_enabled)
        {
            Ok(()) => {
                self.current_path = Some(path.to_path_buf());
                self.saved_snapshot = Some(self.editor.document.clone());
                self.remember_recent(path);
                let _ = std::fs::remove_file(&self.recovery_path);
                self.last_autosave = std::time::Instant::now();
                self.status = format!("Saved {}", path.display());
                self.toast(self.status.clone());
            }
            Err(error) => self.status = format!("Save failed: {error}"),
        }
    }

    pub(super) fn save_archive(&mut self, path: &Path) {
        match crate::project_archive::write_archive(&self.editor.document, path) {
            Ok(report) => {
                self.status = format!(
                    "Saved archive {} with {} image(s)",
                    path.display(),
                    report.images
                );
                self.toast(self.status.clone());
                if !report.missing.is_empty() {
                    let message = format!(
                        "{} image(s) could not be embedded: {}",
                        report.missing.len(),
                        report.missing.join(", ")
                    );
                    self.status = format!("{}; {message}", self.status);
                    self.toast_error(message);
                }
            }
            Err(error) => {
                self.status = format!("Archive failed: {error}");
                self.toast_error(self.status.clone());
            }
        }
    }
    pub(super) fn remember_recent(&mut self, path: &Path) {
        let entry = path.display().to_string();
        let mut recent = std::mem::take(&mut self.preferences.general.recent_projects);
        recent.retain(|existing| existing != &entry);
        recent.insert(0, entry);
        recent.truncate(10);
        self.preferences.general.recent_projects = recent;
        let _ = self.preferences.save(&self.preferences_path);
    }
}
