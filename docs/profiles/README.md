# Profile Parameters Documentation

Platform-specific configuration parameters for customizing RustFrame's preview window behavior.

## Platform Documentation

- **[Windows Parameters](windows.md)** - Windows-specific window styles and extended styles
- **[macOS Parameters](macos.md)** - macOS NSWindow and collection behavior settings (Coming Soon)
- **[Linux Parameters](linux.md)** - X11/Wayland window properties (Coming Soon)


## Overview

RustFrame uses platform-specific parameters to control how the preview window appears and behaves. Each platform has different windowing APIs and conventions.

| Platform | Windowing API | Key Parameters | Notes |
|----------|---------------|---------------|-------|
| Windows  | Win32         | WS_*, WS_EX_* | Taskbar, z-order, transparency |
| macOS    | Cocoa/AppKit  | NSWindow, NSWindowLevel, NSWindowCollectionBehavior | Mission Control, screen sharing |
| Linux    | X11/Wayland   | Type hints, stacking, compositor hints | Desktop integration, experimental |

- [See full Windows documentation](windows.md)
- [macOS documentation coming soon](macos.md)
- [Linux documentation coming soon](linux.md)

For implementation details, see [../technical/](../technical/).

---

## Common Configuration Patterns

### Default Configuration
**Use Case**: Standard screen sharing with Meet/Zoom/Teams  
**Characteristics**: Hidden from user, always in background

### Discord Configuration
**Use Case**: Discord screen sharing (requires taskbar visibility)  
**Characteristics**: Visible in taskbar for 15s, then auto-hides

### Debugging Configuration
**Use Case**: Development and troubleshooting  
**Characteristics**: Always on top, fully visible, interactive

---

## How Profiles Work

1. **Default Settings**: Base configuration in `resources/default_settings/<platform>.json`
2. **Profile Override**: Platform-specific profiles in `resources/profiles/<platform>/*.json`
3. **User Config**: User can save custom configurations in their config directory
4. **Runtime Application**: Settings applied when capture starts or profile changes

---

## Creating Custom Profiles

See the [User Guide - Profiles](../user-guide/features.md#profiles) for instructions on creating and using custom profiles.

---

## Related Documentation

- [User Guide - Settings](../user-guide/settings.md)
- [User Guide - Profiles](../user-guide/features.md#profiles)
- [Technical - Window Management](../technical/)
