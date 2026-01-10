//! Platform-agnostic utility functions
//!
//! Common helper functions that can be used across all platforms.
//! These utilities help reduce code duplication and provide consistent behavior.

/// Convert BGR color (Windows format) to RGBA
///
/// # Arguments
/// * `bgr` - Color in BGR format (0xBBGGRR)
///
/// # Returns
/// RGBA color as byte array [R, G, B, A]
pub fn bgr_to_rgba(bgr: u32) -> [u8; 4] {
    [
        (bgr & 0xFF) as u8,         // R (lowest byte)
        ((bgr >> 8) & 0xFF) as u8,  // G
        ((bgr >> 16) & 0xFF) as u8, // B (highest byte)
        255,                        // A (fully opaque)
    ]
}

/// Convert RGBA to BGR color (Windows format)
///
/// # Arguments
/// * `rgba` - Color as RGBA byte array [R, G, B, A]
///
/// # Returns
/// Color in BGR format (0xBBGGRR), alpha channel is ignored
pub fn rgba_to_bgr(rgba: [u8; 4]) -> u32 {
    ((rgba[2] as u32) << 16) | ((rgba[1] as u32) << 8) | (rgba[0] as u32)
}

/// Calculate inner rectangle (excluding border)
///
/// # Arguments
/// * `x` - Outer rectangle X position
/// * `y` - Outer rectangle Y position
/// * `width` - Outer rectangle width
/// * `height` - Outer rectangle height
/// * `border_width` - Border width to subtract from all sides
///
/// # Returns
/// Inner rectangle as (x, y, width, height)
pub fn calculate_inner_rect(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    border_width: i32,
) -> (i32, i32, i32, i32) {
    (
        x + border_width,
        y + border_width,
        (width - 2 * border_width).max(0),
        (height - 2 * border_width).max(0),
    )
}

/// Validate window dimensions
///
/// # Arguments
/// * `width` - Window width to validate
/// * `height` - Window height to validate
///
/// # Returns
/// Ok(()) if valid, Err with description if invalid
pub fn validate_window_size(width: i32, height: i32) -> Result<(), String> {
    if width < 50 || height < 50 {
        return Err(format!(
            "Window too small: {}x{} (minimum 50x50)",
            width, height
        ));
    }
    if width > 7680 || height > 4320 {
        return Err(format!(
            "Window too large: {}x{} (maximum 7680x4320)",
            width, height
        ));
    }
    Ok(())
}

/// Calculate corner hit test thickness
///
/// Corners should be easier to grab than edges, so we make them thicker.
/// This calculation ensures corners are at least MIN_CORNER_THICKNESS pixels
/// but scales with border width for consistency.
///
/// # Arguments
/// * `border_width` - Border width in pixels
///
/// # Returns
/// Corner thickness in pixels (always >= MIN_CORNER_THICKNESS)
pub fn calculate_corner_thickness(border_width: i32) -> i32 {
    use crate::config::window::MIN_CORNER_THICKNESS;
    (border_width * 2).max(MIN_CORNER_THICKNESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_conversion() {
        // Test round-trip BGR <-> RGBA conversion
        // Input: RGB(255, 128, 64) with alpha=200
        let rgba = [255, 128, 64, 200];
        let bgr = rgba_to_bgr(rgba); // Alpha is ignored in BGR

        // BGR format: 0xBBGGRR
        // Expected: 0x00408064 = (64 << 16) | (128 << 8) | 255
        assert_eq!(bgr, 0x4080FF);

        let converted = bgr_to_rgba(bgr);
        assert_eq!(rgba[0], converted[0]); // R = 255
        assert_eq!(rgba[1], converted[1]); // G = 128
        assert_eq!(rgba[2], converted[2]); // B = 64
        assert_eq!(255, converted[3]); // A (always 255 in BGR->RGBA)
    }

    #[test]
    fn test_inner_rect() {
        let (ix, iy, iw, ih) = calculate_inner_rect(100, 100, 200, 150, 4);
        assert_eq!(ix, 104); // 100 + 4
        assert_eq!(iy, 104); // 100 + 4
        assert_eq!(iw, 192); // 200 - 8
        assert_eq!(ih, 142); // 150 - 8
    }

    #[test]
    fn test_inner_rect_minimum() {
        // Border larger than window should return 0 size, not negative
        let (_, _, iw, ih) = calculate_inner_rect(0, 0, 10, 10, 20);
        assert_eq!(iw, 0);
        assert_eq!(ih, 0);
    }

    #[test]
    fn test_window_size_validation() {
        assert!(validate_window_size(100, 100).is_ok());
        assert!(validate_window_size(7680, 4320).is_ok());
        assert!(validate_window_size(49, 100).is_err()); // Too small
        assert!(validate_window_size(100, 49).is_err()); // Too small
        assert!(validate_window_size(7681, 100).is_err()); // Too large
        assert!(validate_window_size(100, 4321).is_err()); // Too large
    }

    #[test]
    fn test_corner_thickness() {
        use crate::config::window::MIN_CORNER_THICKNESS;
        assert_eq!(calculate_corner_thickness(2), MIN_CORNER_THICKNESS);
        assert_eq!(calculate_corner_thickness(4), 8);
        assert_eq!(calculate_corner_thickness(10), 20);
    }
}
