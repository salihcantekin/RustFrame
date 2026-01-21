//! Common traits for cross-platform window components
//!
//! These traits ensure consistent API across all platforms (Windows, macOS, Linux)
//! and help prevent missing implementations.


/// Hollow border window - shows capture region with resizable/draggable border
pub trait BorderWindow: Send + Sync {
    /// Create a new border window at specified position
    fn new(
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        border_width: i32,
        border_color: u32,
    ) -> Option<Self>
    where
        Self: Sized;

    /// Get current window rectangle (x, y, width, height)
    fn get_rect(&self) -> (i32, i32, i32, i32);

    /// Get inner rectangle (capture area, excluding border width)
    fn get_inner_rect(&self) -> (i32, i32, i32, i32);

    /// Update window position and size
    fn update_rect(&self, x: i32, y: i32, width: i32, height: i32);

    /// Update border color
    fn update_color(&self, color: u32);

    /// Update border width and color
    fn update_style(&self, width: i32, color: u32);

    /// Hide the border window
    fn hide(&self);

    /// Show the border window
    fn show(&self);

    /// Get platform-specific window handle
    fn hwnd_value(&self) -> isize;

    /// Set capture mode: interior is click-through, only edges are interactive
    fn set_capture_mode(&mut self);

    /// Set preview mode: interior is draggable (not click-through)
    fn set_preview_mode(&mut self);

    /// Stop the border window (cleanup before drop)
    fn stop(&mut self);
}

/// Recording indicator overlay - shows "● REC" indicator
pub trait RecordingIndicator: Send + Sync {
    /// Create a new recording indicator
    fn new() -> Option<Self>
    where
        Self: Sized;

    /// Show indicator at specified position (typically top-right of capture region)
    /// - x, y: top-left corner of capture region
    /// - region_width: capture region width (to calculate right edge)
    /// - border_width: border thickness to account for
    fn show(&self, x: i32, y: i32, region_width: i32, border_width: i32);

    /// Hide the indicator
    fn hide(&self);

    /// Update indicator position (called when capture region moves/resizes)
    /// - region_x, region_y: capture region position
    /// - region_width: capture region width
    /// - border_width: border thickness
    fn update_position(&self, region_x: i32, region_y: i32, region_width: i32, border_width: i32);

    /// Set indicator size ("small", "medium", "large")
    fn set_size(&self, size: &str);
}

/// Destination window - displays captured frames for screen sharing
pub trait PreviewWindow: Send + Sync {
    /// Configuration for creating destination window
    type Config;

    /// Create a new destination window with specified dimensions and config
    fn new(x: i32, y: i32, width: u32, height: u32, config: Self::Config) -> Option<Self>
    where
        Self: Sized;

    /// Get platform-specific window handle
    fn hwnd_value(&self) -> isize;

    /// Update frame with new pixel data
    /// - data: RGBA pixel data (owned Vec for platform flexibility)
    /// - width, height: frame dimensions
    fn update_frame(&self, data: Vec<u8>, width: u32, height: u32);

    /// Render pixel data to window (alternative interface with borrowed data)
    fn render(&mut self, pixels: &[u8], width: u32, height: u32);

    /// Resize window (called when capture region changes size)
    fn resize(&mut self, width: u32, height: u32);

    /// Move window to new position
    fn set_pos(&mut self, x: i32, y: i32);

    /// Send window to back (HWND_BOTTOM on Windows) for screen sharing compatibility
    /// Keeps window visible but at lowest z-order
    fn send_to_back(&self);

    /// Bring window to front (for debugging or special cases)
    fn bring_to_front(&self);

    /// Exclude window from screen capture to prevent infinite mirroring
    /// (Windows 10 2000H+ only, no-op on other platforms)
    fn exclude_from_capture(&self);
}
