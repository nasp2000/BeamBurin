//! `BeamBurin` entry point.
//!
//! The portable Windows release is built as a GUI-subsystem executable so no
//! console window appears behind the editor. CLI output needs a console build:
//! the `console` feature selects the console subsystem, and every non-Windows
//! or debug build already has one.
#![cfg_attr(
    all(windows, not(debug_assertions), not(feature = "console")),
    windows_subsystem = "windows"
)]
#![allow(dead_code)]

fn load_icon() -> egui::IconData {
    let bytes = include_bytes!("../assets/icons/beamburin.png");
    if let Ok(img) = image::load_from_memory(bytes) {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        return egui::IconData {
            rgba: rgba.into_raw(),
            width: w,
            height: h,
        };
    }
    egui::IconData {
        rgba: vec![],
        width: 0,
        height: 0,
    }
}

use beamburin::app::BeamBurinApp;
use clap::Parser;

/// Graphics options that also work on the limited adapters found on small
/// boards and remote sessions.
///
/// The default descriptor asks for the full desktop limits, including eight
/// colour attachments. A software or virtual adapter that exposes four
/// rejects the device, so the window never opens. The interface only needs one
/// colour attachment, so the adapter's own limits are requested instead.
fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("BeamBurin")
            .with_inner_size([1280.0, 800.0])
            .with_icon(load_icon()),
        wgpu_options: beamburin::app::wgpu_configuration(),
        ..Default::default()
    }
}

fn main() -> eframe::Result<()> {
    tracing::info!("starting BeamBurin");
    let cli = beamburin::cli::Cli::parse();
    if cli.licenses {
        beamburin::cli::print_licenses();
        return Ok(());
    }
    if cli.command.is_some() {
        if let Err(e) = beamburin::cli::run(cli) {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }
    let options = native_options();
    eframe::run_native(
        "BeamBurin",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::light());
            let mut app = BeamBurinApp::new(cli.file);
            if cli.smoke_test {
                app.set_smoke_frames(20);
            }
            if cli.frame_log {
                app.enable_frame_log();
            }
            Ok(Box::new(app))
        }),
    )
}
