# RustFrame User Guide

Welcome to RustFrame! This guide will help you get started with capturing and sharing specific regions of your screen.

## 📚 Table of Contents

- [Installation](installation.md) - How to install RustFrame on your system
- [Quick Start](quick-start.md) - Get up and running in 30 seconds
- [Features](features.md) - Complete feature overview
- [Settings](settings.md) - Configuration options and customization guide
- [Troubleshooting](troubleshooting.md) - Common issues and solutions
- [FAQ](faq.md) - Frequently asked questions

## What is RustFrame?

RustFrame is a cross-platform screen region capture application that allows you to:

- **Select any screen region** - Use a resizable, draggable border to select what to capture
- **Share in video calls** - The preview window appears in Google Meet, Zoom, Teams, Discord
- **Multi-monitor support** - Capture from any connected display
- **High performance** - GPU-accelerated capture with minimal CPU usage
- **Customizable** - Configure cursor visibility, click highlights, borders, and more

## Why RustFrame?

### Problem
When screen sharing in video calls, you often need to share:
- A specific application window
- Part of your screen (excluding personal info, chat windows, etc.)
- Content from a specific monitor in a multi-display setup

Most screen sharing tools force you to share:
- Your entire screen (privacy concerns)
- A full window (can't exclude parts)
- Or struggle with clunky region selection

### Solution
RustFrame creates a **shareable preview window** that displays exactly the region you select. Simply:
1. Select your capture region
2. Start capture
3. Share the "RustFrame Preview" window in your video call
4. Only your selected region is visible to participants


## Supported Platforms & Feature Comparison

| Platform         | Status         | Capture Method                | Rendering         | Notes |
|------------------|---------------|-------------------------------|-------------------|-------|
| Windows 10/11    | ✅ Full Support| Windows Graphics Capture (WGC)| DirectX 11 (GPU)  | Best performance, full GPU pipeline |
| macOS 12.3+      | ✅ Full Support| ScreenCaptureKit              | Metal (GPU)       | Full GPU pipeline, window exclusion supported |
| macOS 10.15-12.2 | ✅ Supported   | CoreGraphics                  | CPU fallback      | Lower performance, no window exclusion |
| Linux            | 🚧 Experimental| X11/Wayland                   | wgpu              | Limited features, experimental |

See [Technical Docs](../technical/) for platform-specific implementation details and limitations.

## Key Features at a Glance

### 🎯 Capture
- **Hollow Border Window**: Visual indicator of capture region
- **Live Region Adjustment**: Drag and resize while capturing
- **Monitor Detection**: Automatically switches when dragged to different display
- **Multiple Capture Methods**: GPU-accelerated or CPU fallback

### 🖱️ Mouse & Clicks
- **Cursor Visibility Toggle**: Show/hide your cursor in capture
- **Click Highlights**: Visual feedback for mouse clicks
- **Customizable Colors**: Choose highlight colors
- **Dissolve Effect**: Smooth fade-out animation

### ⚙️ Settings
- **Region Configuration**: Precise pixel control
- **Performance Tuning**: FPS adjustment (15-144 FPS)
- **Border Customization**: Width, color, visibility
- **Capture Profiles**: Pre-configured settings for different apps

### 📊 Recording Indicator
- **Live Recording Badge**: "REC" indicator during capture
- **Configurable Position**: Follows border window
- **Toggle Visibility**: Show/hide as needed

## Getting Started

Ready to start? Head to the [Quick Start Guide](quick-start.md) →

## Need Help?

- **Common Issues**: Check [Troubleshooting](troubleshooting.md)
- **Questions**: See [FAQ](faq.md)
- **Bugs/Feature Requests**: [GitHub Issues](https://github.com/salihcantekin/RustFrame/issues)
- **Contributing**: See [CONTRIBUTING.md](../../CONTRIBUTING.md)

## Platform-Specific Notes

### Windows
- Requires Windows 10 1803+ for GPU capture
- Best performance with Windows Graphics Capture (default)
- GDI fallback available for compatibility

### macOS
- Requires Screen Recording permission (prompted on first run)
- ScreenCaptureKit (macOS 12.3+) recommended for best performance
- Legacy CoreGraphics support for older versions

### Linux
- Requires PipeWire (Wayland) or X11 server
- May need manual permission configuration depending on compositor

---

**Next**: [Installation Guide](installation.md) →
