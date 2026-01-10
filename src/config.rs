//! Application Configuration Constants
//!
//! Centralized configuration for all magic numbers, colors, timings, and defaults.
//! This makes the codebase more maintainable and easier to tune.

/// Window and Border Configuration
pub mod window {
    /// Default initial region for hollow border (x, y, width, height)
    pub const DEFAULT_REGION: (i32, i32, i32, i32) = (0, 0, 800, 600);

    /// Default hollow border width in pixels
    pub const DEFAULT_BORDER_WIDTH: i32 = 4;

    /// Default border color (BGR format: 0xBBGGRR)
    /// Orange color: RGB(255, 128, 64) = BGR(0x4080FF)
    pub const DEFAULT_BORDER_COLOR: u32 = 0x4080FF;

    /// Preview mode background color (BGR format)
    /// Dark gray: RGB(32, 32, 32) = BGR(0x202020)
    pub const PREVIEW_BG_COLOR: u32 = 0x202020;

    /// Capture mode background color (BGR format) - used as transparency key
    /// Bright green: RGB(0, 255, 0) = BGR(0x00FF00)
    pub const CAPTURE_BG_COLOR: u32 = 0x00FF00;

    /// Corner thickness calculation minimum value
    pub const MIN_CORNER_THICKNESS: i32 = 4;

    /// Poll interval for thread message loops (milliseconds)
    pub const THREAD_POLL_INTERVAL_MS: u64 = 10;
}

/// Capture Engine Configuration
pub mod capture {
    /// Default target FPS for capture
    pub const DEFAULT_TARGET_FPS: u32 = 60;

    /// Destination window timer interval (~60 FPS)
    pub const DESTINATION_WINDOW_TIMER_MS: u32 = 16;

    /// Default click highlight color [R, G, B, A]
    /// Yellow with transparency
    pub const DEFAULT_CLICK_HIGHLIGHT_COLOR: [u8; 4] = [255, 255, 0, 180];

    /// Log retention period in days
    pub const LOG_RETENTION_DAYS: u64 = 30;
}

/// REC Indicator Configuration
pub mod rec_indicator {
    /// Size presets: (width, height) in pixels
    pub const SIZE_SMALL: (i32, i32) = (50, 18);
    pub const SIZE_MEDIUM: (i32, i32) = (70, 24);
    pub const SIZE_LARGE: (i32, i32) = (90, 30);

    /// Default size setting
    pub const DEFAULT_SIZE: &str = "medium";

    /// Background opacity (0-255)
    pub const BACKGROUND_ALPHA: u8 = 255;

    /// Poll interval for position updates (milliseconds)
    pub const UPDATE_POLL_INTERVAL_MS: u64 = 10;
}

/// Retry and Timing Configuration
pub mod timing {
    /// Sleep duration before border cleanup (milliseconds)
    pub const BORDER_CLEANUP_DELAY_MS: u64 = 200;

    /// Maximum retries for border window validation
    pub const BORDER_VALIDATION_MAX_RETRIES: u32 = 15;

    /// Delay between border validation retries (milliseconds)
    pub const BORDER_VALIDATION_RETRY_DELAY_MS: u64 = 30;

    /// Timeout for window creation (iterations)
    pub const WINDOW_CREATION_TIMEOUT_ITERATIONS: u32 = 50;

    /// Poll interval during window creation wait (milliseconds)
    pub const WINDOW_CREATION_POLL_INTERVAL_MS: u64 = 10;
}

/// Color Utilities
pub mod colors {
    /// Convert ARGB u32 to RGBA byte array
    pub fn argb_to_rgba(color: u32) -> [u8; 4] {
        [
            ((color >> 16) & 0xFF) as u8, // R
            ((color >> 8) & 0xFF) as u8,  // G
            (color & 0xFF) as u8,         // B
            ((color >> 24) & 0xFF) as u8, // A
        ]
    }

    /// Convert RGBA byte array to ARGB u32
    pub fn rgba_to_argb(rgba: [u8; 4]) -> u32 {
        ((rgba[3] as u32) << 24)
            | ((rgba[0] as u32) << 16)
            | ((rgba[1] as u32) << 8)
            | (rgba[2] as u32)
    }

    /// Normalize alpha value (0-255 range to 0.0-1.0)
    pub fn normalize_alpha(alpha: u8) -> f32 {
        alpha as f32 / 255.0
    }
}

#[cfg(test)]
mod tests {
    use super::colors::*;

    #[test]
    fn test_color_conversion() {
        let rgba = [255, 128, 64, 200];
        let argb = rgba_to_argb(rgba);
        let converted = argb_to_rgba(argb);
        assert_eq!(rgba, converted);
    }

    #[test]
    fn test_alpha_normalization() {
        assert_eq!(normalize_alpha(0), 0.0);
        assert_eq!(normalize_alpha(255), 1.0);
        assert!((normalize_alpha(128) - 0.502).abs() < 0.01);
    }
}
