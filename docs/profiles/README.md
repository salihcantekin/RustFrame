# Profile Parameters Documentation

Platform-specific configuration parameters for customizing RustFrame's preview window behavior.

## Platform Documentation

- **[Windows Parameters](windows.md)** - Windows-specific window styles and extended styles
- **[macOS Parameters](macos.md)** - macOS NSWindow and collection behavior settings (Coming Soon)
- **[Linux Parameters](linux.md)** - X11/Wayland window properties (Coming Soon)

## Overview

RustFrame uses platform-specific parameters to control how the preview window appears and behaves. Each platform has different windowing APIs and conventions:

### Windows
Uses Win32 API window styles (`WS_*`) and extended styles (`WS_EX_*`) to control:
- Window decoration (title bar, borders)
- Taskbar visibility
- Transparency and layering
- Z-order and focus behavior

[→ See full Windows documentation](windows.md)

### macOS
Uses Cocoa/AppKit APIs (`NSWindow`, `NSWindowLevel`, `NSWindowCollectionBehavior`) to control:
- Window level (floating, normal, etc.)
- Mission Control and Exposé behavior
- Screen sharing visibility
- Transparency and compositing

[→ macOS documentation coming soon](macos.md)

### Linux
Uses X11 properties and Wayland protocols to control:
- Window type hints
- Stacking order
- Desktop environment integration
- Compositor behavior

[→ Linux documentation coming soon](linux.md)

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
