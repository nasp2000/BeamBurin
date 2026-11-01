//! Background export jobs started over MCP.
//!
//! A job clones the document, renders in a thread and reports its state until it
//! finishes. Cancelling sets a flag the renderer checks between phases; a job
//! that already finished is left alone. The live document is never touched, so
//! the editor stays responsive while a long job runs.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::{Value, json};

use crate::document::Document;
use crate::errors::{BeamError, Result};

/// Jobs kept in memory; the oldest finished one is dropped first.
pub const MAX_JOBS: usize = 16;

#[derive(Debug, Clone)]
enum JobState {
    Running,
    Done(Value),
    Failed(String),
    Cancelled,
}

struct Job {
    id: String,
    tool: String,
    cancel: Arc<AtomicBool>,
    state: Arc<Mutex<JobState>>,
    started: Instant,
}

/// The jobs a session has started.
#[derive(Default)]
pub struct Jobs {
    next_id: u64,
    entries: Vec<Job>,
}

impl std::fmt::Debug for Jobs {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Jobs")
            .field("next_id", &self.next_id)
            .field("jobs", &self.entries.len())
            .finish()
    }
}

impl Jobs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Start an export in the background and return its job id.
    pub fn start_export(&mut self, tool: &str, document: &Document, path: &Path) -> Result<String> {
        let document = document.clone();
        let path = path.to_path_buf();
        match tool {
            "export_gcode" => Ok(self.start_worker(tool, move |cancel| {
                let report = crate::gcode::export_job_cancellable(
                    &document,
                    &path,
                    &crate::gcode::JobOptions::default(),
                    cancel,
                )?;
                Ok(json!({
                    "path": path.display().to_string(),
                    "lines": report.lines,
                    "bytes": report.bytes,
                    "paths": report.paths,
                    "seconds": report.seconds,
                    "warnings": report.warnings,
                }))
            })),
            "export_svg" => Ok(self.start_worker(tool, move |cancel| {
                let text = crate::export::export_svg(&document)?;
                write_text(&path, &text, cancel)?;
                Ok(json!({"path": path.display().to_string(), "bytes": text.len()}))
            })),
            "export_dxf" => Ok(self.start_worker(tool, move |cancel| {
                let text = crate::export::export_dxf(&document)?;
                write_text(&path, &text, cancel)?;
                Ok(json!({"path": path.display().to_string(), "bytes": text.len()}))
            })),
            other => Err(BeamError::InvalidInput(format!(
                "'{other}' cannot run in the background"
            ))),
        }
    }

    /// State of one job, or `None` when the id is unknown.
    pub fn status(&self, id: &str) -> Option<Value> {
        let job = self.entries.iter().find(|job| job.id == id)?;
        let state = job.state.lock().map(|state| state.clone()).ok()?;
        let mut value = json!({
            "id": job.id,
            "tool": job.tool,
            "elapsed_ms": u64::try_from(job.started.elapsed().as_millis()).unwrap_or(u64::MAX),
        });
        match state {
            JobState::Running => value["state"] = json!("running"),
            JobState::Done(result) => {
                value["state"] = json!("done");
                value["result"] = result;
            }
            JobState::Failed(error) => {
                value["state"] = json!("failed");
                value["error"] = json!(error);
            }
            JobState::Cancelled => value["state"] = json!("cancelled"),
        }
        Some(value)
    }

    /// Ask a running job to stop.
    pub fn cancel(&self, id: &str) -> Option<Value> {
        let job = self.entries.iter().find(|job| job.id == id)?;
        let running = job
            .state
            .lock()
            .is_ok_and(|state| matches!(*state, JobState::Running));
        if running {
            job.cancel.store(true, Ordering::Relaxed);
        }
        let mut value = self.status(id)?;
        value["cancel_requested"] = json!(running);
        Some(value)
    }

    fn start_worker<F>(&mut self, tool: &str, worker: F) -> String
    where
        F: FnOnce(&dyn Fn() -> bool) -> Result<Value> + Send + 'static,
    {
        self.next_id += 1;
        let id = format!("job-{}", self.next_id);
        let cancel = Arc::new(AtomicBool::new(false));
        let state = Arc::new(Mutex::new(JobState::Running));
        let thread_cancel = Arc::clone(&cancel);
        let thread_state = Arc::clone(&state);
        std::thread::spawn(move || {
            let is_cancelled = move || thread_cancel.load(Ordering::Relaxed);
            let outcome = worker(&is_cancelled);
            let next = match outcome {
                Ok(value) => JobState::Done(value),
                Err(error) => {
                    if is_cancelled() {
                        JobState::Cancelled
                    } else {
                        JobState::Failed(error.to_string())
                    }
                }
            };
            if let Ok(mut guard) = thread_state.lock() {
                *guard = next;
            }
        });
        self.entries.push(Job {
            id: id.clone(),
            tool: tool.to_string(),
            cancel,
            state,
            started: Instant::now(),
        });
        self.prune();
        id
    }

    fn prune(&mut self) {
        while self.entries.len() > MAX_JOBS {
            let finished = self.entries.iter().position(|job| {
                job.state
                    .lock()
                    .is_ok_and(|state| !matches!(*state, JobState::Running))
            });
            match finished {
                Some(index) => {
                    self.entries.remove(index);
                }
                None => break,
            }
        }
    }
}

/// Write the temporary file first, so a cancelled job leaves no half file.
fn write_text(path: &Path, text: &str, cancel: &dyn Fn() -> bool) -> Result<()> {
    if cancel() {
        return Err(BeamError::InvalidInput("cancelled".to_string()));
    }
    let temporary = temporary_path(path);
    std::fs::write(&temporary, text)?;
    if cancel() {
        let _ = std::fs::remove_file(&temporary);
        return Err(BeamError::InvalidInput("cancelled".to_string()));
    }
    std::fs::rename(&temporary, path)?;
    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map_or_else(
        || std::ffi::OsString::from("export"),
        std::ffi::OsString::from,
    );
    name.push(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::{Jobs, MAX_JOBS};
    use crate::document::model::Object;
    use crate::document::{Document, Shape};

    fn rectangle_document() -> Document {
        let mut document = Document::default();
        document.operations.push(crate::operations::Operation {
            id: "cut".to_string(),
            name: "Cut".to_string(),
            op_type: crate::operations::OperationType::OutlineCutting,
            params: crate::operations::OperationParams::default(),
            enabled: true,
            depends_on: Vec::new(),
        });
        let layer_id = document.layers[0].id.clone();
        let mut object = Object::new(
            layer_id,
            Shape::Rect {
                width: 10.0,
                height: 5.0,
                corner_radius: 0.0,
            },
        );
        object.operation_id = Some("cut".to_string());
        document.add_object(object);
        document
    }

    fn wait_for(jobs: &Jobs, id: &str) -> String {
        for _ in 0..200 {
            let status = jobs.status(id).unwrap();
            let state = status["state"].as_str().unwrap().to_string();
            if state != "running" {
                return state;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("job {id} never finished");
    }

    #[test]
    fn an_export_runs_in_the_background_and_writes_the_file() {
        let path =
            std::env::temp_dir().join(format!("beamwright_job_export_{}.nc", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut jobs = Jobs::new();
        let id = jobs
            .start_export("export_gcode", &rectangle_document(), &path)
            .unwrap();
        assert_eq!(wait_for(&jobs, &id), "done");
        let status = jobs.status(&id).unwrap();
        assert!(status["result"]["lines"].as_u64().unwrap() > 0);
        assert!(path.is_file());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("G1"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn svg_and_dxf_exports_are_supported() {
        for (tool, extension) in [("export_svg", "svg"), ("export_dxf", "dxf")] {
            let path = std::env::temp_dir().join(format!(
                "beamwright_job_{tool}_{}.{extension}",
                std::process::id()
            ));
            let _ = std::fs::remove_file(&path);
            let mut jobs = Jobs::new();
            let id = jobs
                .start_export(tool, &rectangle_document(), &path)
                .unwrap();
            assert_eq!(wait_for(&jobs, &id), "done");
            assert!(path.is_file(), "{tool} did not write the file");
            let _ = std::fs::remove_file(&path);
        }
    }

    #[test]
    fn unsupported_tools_are_rejected() {
        let mut jobs = Jobs::new();
        let path = std::env::temp_dir().join("beamwright_job_none.bin");
        assert!(
            jobs.start_export("export_pdf", &rectangle_document(), &path)
                .is_err()
        );
        assert!(jobs.status("job-1").is_none());
        assert!(jobs.cancel("job-1").is_none());
    }

    #[test]
    fn a_running_job_can_be_cancelled() {
        let mut jobs = Jobs::new();
        let id = jobs.start_worker("test", |cancel| {
            while !cancel() {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            Err(crate::errors::BeamError::InvalidInput(
                "cancelled".to_string(),
            ))
        });
        std::thread::sleep(std::time::Duration::from_millis(10));
        let status = jobs.cancel(&id).unwrap();
        assert_eq!(status["cancel_requested"], true);
        assert_eq!(wait_for(&jobs, &id), "cancelled");
    }

    #[test]
    fn cancelling_a_finished_job_is_a_no_op() {
        let mut jobs = Jobs::new();
        let id = jobs.start_worker("test", |_| Ok(serde_json::json!({"kept": true})));
        assert_eq!(wait_for(&jobs, &id), "done");
        let status = jobs.cancel(&id).unwrap();
        assert_eq!(status["cancel_requested"], false);
        assert_eq!(status["state"], "done");
        assert_eq!(status["result"]["kept"], true);
    }

    #[test]
    fn a_failing_job_reports_the_error() {
        let mut jobs = Jobs::new();
        let id = jobs.start_worker("test", |_| {
            Err(crate::errors::BeamError::InvalidInput(
                "no canvas".to_string(),
            ))
        });
        assert_eq!(wait_for(&jobs, &id), "failed");
        assert_eq!(
            jobs.status(&id).unwrap()["error"],
            "invalid input: no canvas"
        );
    }

    #[test]
    fn finished_jobs_are_pruned_at_the_limit() {
        let mut jobs = Jobs::new();
        for _ in 0..MAX_JOBS + 3 {
            let id = jobs.start_worker("test", |_| Ok(serde_json::json!({})));
            assert_eq!(wait_for(&jobs, &id), "done");
        }
        assert_eq!(jobs.len(), MAX_JOBS);
    }
}
