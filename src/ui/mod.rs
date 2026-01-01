// ui/mod.rs - egui-based User Interface Components
//
// This module contains all UI components built with egui.
// Platform-specific components (like capture_border) use native APIs.

mod overlay;
mod destination;
mod settings;
mod theme;
mod tray_simple;
mod border_frame;
mod capture_border;

pub use overlay::OverlayUi;
pub use destination::DestinationUi;
pub use settings::SettingsDialog;
pub use theme::RustFrameTheme;
pub use tray_simple::SystemTray;
pub use border_frame::BorderFrameUi;

// Platform-specific capture border
pub use capture_border::CaptureBorderWindow;
pub use capture_border::{BorderStyle, BorderColors, CaptureBorder};
