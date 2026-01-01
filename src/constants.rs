// constants.rs - Application-wide Constants
//
// Centralized constants for colors, dimensions, and other magic numbers.
// This makes the code more maintainable and consistent.

/// Overlay window colors (ARGB format)
pub mod colors {
    /// Bright blue border color
    pub const BORDER: u32 = 0xFF00A8FF;
    /// Orange border when capture is active
    pub const BORDER_ACTIVE: u32 = 0xFFFF9A3C;
    /// Almost fully transparent fill
    pub const FILL: u32 = 0x10000000;
    /// Lighter blue for corner markers
    pub const CORNER: u32 = 0xFF00D4FF;
    /// Orange corner markers when capture is active
    pub const CORNER_ACTIVE: u32 = 0xFFFFB766;
    /// Dark gray background for text box
    pub const TEXT_BG: u32 = 0xF0181818;
    /// Blue border for text box
    pub const TEXT_BORDER: u32 = 0xFF00A8FF;
    /// Orange border for text box when capture is active
    pub const TEXT_BORDER_ACTIVE: u32 = 0xFFFF9A3C;
    /// White text
    pub const TEXT_WHITE: u32 = 0xFFFFFFFF;
    /// Blue text (for title)
    pub const TEXT_BLUE: u32 = 0xFF00D4FF;
    /// Gray text (for secondary info)
    pub const TEXT_GRAY: u32 = 0xFFB0B0B0;
    /// Green text (for enabled settings)
    pub const TEXT_GREEN: u32 = 0xFF00DD00;
    /// Red text (for disabled settings)
    pub const TEXT_RED: u32 = 0xFFFF4444;
    /// Yellow text (for dev mode indicator)
    pub const TEXT_YELLOW: u32 = 0xFFFFCC00;
    /// Bright green for play button
    pub const PLAY: u32 = 0xFF00FF7A;
}

/// Overlay window dimensions
pub mod overlay {
    /// Default window width
    pub const DEFAULT_WIDTH: u32 = 800;
    /// Default window height
    pub const DEFAULT_HEIGHT: u32 = 600;
    /// Minimum window width
    pub const MIN_WIDTH: u32 = 400;
    /// Minimum window height
    pub const MIN_HEIGHT: u32 = 300;
    /// Border width in pixels
    pub const BORDER_WIDTH: i32 = 4;
    /// Corner marker size in pixels
    pub const CORNER_SIZE: i32 = 20;
    /// Resize handle margin in pixels
    #[allow(dead_code)]
    pub const RESIZE_MARGIN: i32 = 8;
}

/// Text box dimensions for help text
pub mod text_box {
    /// Fixed width of the help text box
    pub const WIDTH: i32 = 280;
    /// Fixed height of the help text box  
    pub const HEIGHT: i32 = 260;
    /// Border width of text box
    pub const BORDER_WIDTH: i32 = 2;
}

/// Settings dialog dimensions
pub mod dialog {
    /// Dialog width in pixels
    pub const WIDTH: i32 = 420;
    /// Dialog height in dev mode (with production mode option)
    pub const HEIGHT_DEV: i32 = 320;
    /// Dialog height in production mode
    pub const HEIGHT_PROD: i32 = 280;
}

/// Default capture settings
pub mod capture {
    /// Default border width for hollow frame
    pub const DEFAULT_BORDER_WIDTH: u32 = 3;
    /// Minimum allowed border width
    pub const MIN_BORDER_WIDTH: u32 = 1;
    /// Maximum allowed border width
    pub const MAX_BORDER_WIDTH: u32 = 50;
}
