//! Window and GPU options shared by the editor.

use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup};

/// wgpu configuration for the editor window.
///
/// The default descriptor asks for the full desktop limits, including eight
/// colour attachments. A software or virtual adapter that exposes four
/// rejects the device, so the window never opens. The interface only needs one
/// colour attachment, so the adapter's own limits are requested instead.
///
/// Frames are presented without waiting for vertical blank. On the small board
/// the virtual display's blank never arrives in time and every frame waits for
/// the driver's one-second timeout: the interface then updates about once a
/// second, which is what made clicks feel like they took seconds (measured with
/// `--frame-log`). Without the wait a frame takes about 20 ms.
pub fn wgpu_configuration() -> WgpuConfiguration {
    let mut wgpu = WgpuConfiguration::default();
    wgpu.surface.present_mode = eframe::wgpu::PresentMode::AutoNoVsync;
    if let WgpuSetup::CreateNew(setup) = &mut wgpu.wgpu_setup {
        setup.device_descriptor = std::sync::Arc::new(|adapter| eframe::wgpu::DeviceDescriptor {
            label: Some("BeamBurin device"),
            required_limits: adapter.limits(),
            ..Default::default()
        });
    }
    wgpu
}
