# RustFrame Documentation

Complete documentation for RustFrame—a cross-platform screen region capture tool.

## 📖 Documentation Structure

### For Users

- **[User Guide](user-guide/)** - Installation, usage, and troubleshooting
  - [Installation](user-guide/installation.md) - Download and install
  - [Quick Start](user-guide/quick-start.md) - Get started in 30 seconds
  - [Features Guide](user-guide/features.md) - Complete feature reference
  - [Settings Reference](user-guide/settings.md) - All settings explained
  - [Troubleshooting](user-guide/troubleshooting.md) - Common issues and solutions
  - [FAQ](user-guide/faq.md) - Frequently asked questions

### For Developers

- **[Developer Guide](developer/)** - Architecture and contributing
  - [Architecture Overview](developer/README.md) - System design and code structure
  - [Building from Source](developer/building.md) - Compilation instructions
  - [Capture Engines](developer/capture-engines.md) - Screen capture implementations
  - [Platform-Specific Code](developer/platform-specific.md) - Cross-platform abstractions
  - [Rendering Pipeline](developer/rendering-pipeline.md) - GPU/CPU rendering
  - [Performance Optimization](developer/performance.md) - CPU/GPU tuning
  - [Known Issues](developer/known-issues.md) - Current limitations

### Technical Deep Dives

- **[Technical Documentation](technical/)** - Low-level technical details
  - [GPU Optimization](technical/gpu-optimization.md) - Platform-specific GPU strategies
  - [Zero-Copy Strategy](technical/zero-copy-strategy.md) - Memory optimization
  - [Multi-Monitor Support](technical/multi-monitor.md) - Display detection and DPI
  - [Color Format Handling](technical/color-formats.md) - BGRA vs RGBA across platforms
  - [Coordinate Systems](technical/coordinate-system-architecture.md) - Screen coordinate handling
  - [macOS Window Visibility](technical/macos-window-visibility.md) - Screen sharing compatibility

### Historical Records

- **[Experiments](experiments/)** - Development experiments and fixes
  - [Border Capture Fix](experiments/2026-01-07_border_capture_fix.md)
  - [Color Format Fix](experiments/2026-01-07_color_format_fix.md)
  - [Event-Driven Optimization](experiments/2026-01-07_event_driven_optimization.md)
  - [Coordinate System](experiments/2026-01-07_coordinate_system_architecture.md)
  - [Window Visibility Tests](experiments/2026-01-07_window_visibility_tests.md)

- **[Changelog](changelog/)** - Version history
  - [All Releases](changelog/README.md)
  - [v1.56.0](changelog/v1.56.0.md) - Bug fixes and logging improvements
  - [v1.1.0](changelog/v1.1.0.md) - Multi-monitor support and custom tray icon

## 🚀 Quick Links

### I want to...

| Goal | Documentation |
|------|---------------|
| **Install RustFrame** | [Installation Guide](user-guide/installation.md) |
| **Learn basic usage** | [Quick Start](user-guide/quick-start.md) (30 sec) |
| **Fix a problem** | [Troubleshooting](user-guide/troubleshooting.md) |
| **Understand all features** | [Features Guide](user-guide/features.md) |
| **Configure settings** | [Settings Reference](user-guide/settings.md) |
| **Build from source** | [Building Guide](developer/building.md) |
| **Contribute code** | [Developer Guide](developer/README.md) + [CONTRIBUTING.md](../CONTRIBUTING.md) |
| **Optimize performance** | [Performance Guide](developer/performance.md) |
| **Report a bug** | [GitHub Issues](https://github.com/salihcantekin/RustFrame/issues) |

## 📂 Documentation by Topic

### Installation & Setup
- [Windows Installation](user-guide/installation.md#windows)
- [macOS Installation](user-guide/installation.md#macos)
- [Linux Installation](user-guide/installation.md#linux)
- [Permission Setup (macOS)](user-guide/installation.md#grant-screen-recording-permission)
- [Verification](user-guide/installation.md#verification)

### Basic Usage
- [30-Second Workflow](user-guide/quick-start.md#-30-second-workflow)
- [Configuring Capture Region](user-guide/quick-start.md#2-configure-your-capture-region)
- [Starting Capture](user-guide/quick-start.md#4-start-capturing)
- [Sharing in Video Calls](user-guide/quick-start.md#5-share-in-your-video-call)

### Features
- [Capture Region](user-guide/features.md#capture-region)
- [Mouse & Cursor](user-guide/features.md#mouse--cursor)
- [Click Highlights](user-guide/features.md#click-highlights)
- [Border Customization](user-guide/features.md#border-customization)
- [Performance Settings](user-guide/features.md#performance-settings)
- [Multi-Monitor Support](user-guide/features.md#multi-monito

### Settings & Configuration
- [All Settings Reference](user-guide/settings.md)
- [Mouse & Cursor Settings](user-guide/settings.md#mouse--cursor-settings)
- [Click Highlight Configuration](user-guide/settings.md#click-highlights)
- [Border Settings](user-guide/settings.md#border-settings)
- [Performance Tuning](user-guide/settings.md#performance-settings)
- [Logging Configuration](user-guide/settings.md#logging-settings)
- [Advanced Windows Settings](user-guide/settings.md#advanced-windows-settings)r-support)
- [Capture Profiles](user-guide/features.md#capture-profiles)

### Troubleshooting
- [Black Preview Window](user-guide/troubleshooting.md#black-preview-window)
- [Permission Problems](user-guide/troubleshooting.md#permission-problems)
- [Performance Issues](user-guide/troubleshooting.md#performance-issues)
- [Screen Sharing Issues](user-guide/troubleshooting.md#screen-sharing-issues)
- [Platform-Specific Issues](user-guide/troubleshooting.md#platform-specific)

### Development
- [Architecture Overview](developer/README.md#architecture-overview)
- [Building Instructions](developer/building.md)
- [Platform Abstractions](developer/platform-specific.md)
- [Capture Engine Design](developer/capture-engines.md)
- [Window Management](developer/platform-specific.md#window-management)
- [Testing Procedures](developer/building.md#testing)

### Technical Details
- [Windows Graphics Capture (WGC)](technical/gpu-optimization.md#windows-wgc--d3d11)
- [macOS ScreenCaptureKit](technical/gpu-optimization.md#macos-screencapturekit--metal)
- [GPU vs CPU Rendering](developer/rendering-pipeline.md)
- [Memory Optimization](technical/zero-copy-strategy.md)
- [DPI/Scaling Handling](technical/multi-monitor.md)

## 🔍 Documentation Standards

### For Contributors

When writing documentation:
1. **User Docs**: Focus on "how to" and practical examples
2. **Developer Docs**: Include architecture diagrams and code examples
3. **Technical Docs**: Explain the "why" behind design decisions
4. **Always Include**: Platform-specific notes where applicable

See [CONTRIBUTING.md](../CONTRIBUTING.md) for style guidelines.

## 📝 Missing Documentation?

Found outdated or missing documentation?

1. **Quick fix**: [Open an issue](https://github.com/salihcantekin/RustFrame/issues) describing what's missing
2. **Contribute**: Submit a PR with documentation improvements (see [CONTRIBUTING.md](../CONTRIBUTING.md))

## 📚 External Resources

### Official
- **GitHub Repository**: https://github.com/salihcantekin/RustFrame
- **Releases**: https://github.com/salihcantekin/RustFrame/releases
- **Issue Tracker**: https://github.com/salihcantekin/RustFrame/issues
- **Discussions**: https://github.com/salihcantekin/RustFrame/discussions

### Technology References
- **Tauri**: https://tauri.app/
- **Rust**: https://www.rust-lang.org/
- **React**: https://react.dev/
- **Windows Graphics Capture**: https://docs.microsoft.com/en-us/uwp/api/windows.graphics.capture
- **macOS ScreenCaptureKit**: https://developer.apple.com/documentation/screencapturekit

---

## 🗺️ Documentation Map

```
docs/
├── README.md (you are here)
│
├── user-guide/
│   ├── README.md ............... User guide overview
│   ├── installation.md ......... Platform-specific installation
│   ├── quick-start.md .......... 30-second start guide
│   ├── features.md ............. Complete feature reference
│   ├── troubleshooting.md ...... Common issues and fixes
│   └── faq.md .................. Frequently asked questions
│
├── developer/
│   ├── README.md ............... Developer guide overview
│   ├── building.md ............. Build instructions
│   ├── capture-engines.md ...... Capture implementations
│   ├── platform-specific.md .... Cross-platform code
│   ├── rendering-pipeline.md ... GPU/CPU rendering
│   ├── performance.md .......... Performance optimization
│   ├── known-issues.md ......... Current limitations
│   └── refactoring-plan.md ..... Future refactoring plans
│
├── technical/
│   ├── gpu-optimization.md ..... Platform GPU strategies
│   ├── zero-copy-strategy.md ... Memory optimization
│   ├── multi-monitor.md ........ Display detection
│   ├── color-formats.md ........ BGRA vs RGBA
│   ├── coordinate-system-architecture.md
│   └── macos-window-visibility.md
│
├── experiments/ ................ Development experiments
│   ├── 2026-01-07_border_capture_fix.md
│   ├── 2026-01-07_color_format_fix.md
│   ├── 2026-01-07_event_driven_optimization.md
│   ├── 2026-01-07_coordinate_system_architecture.md
│   └── 2026-01-07_window_visibility_tests.md
│
└── changelog/ .................. Version history
    ├── README.md
    ├── v1.56.0.md
    └── v1.1.0.md
```

---

**Start Here**: [User Guide](user-guide/) | [Developer Guide](developer/)  
**Get Help**: [Troubleshooting](user-guide/troubleshooting.md) | [FAQ](user-guide/faq.md)  
**Contribute**: [Developer Guide](developer/) | [CONTRIBUTING.md](../CONTRIBUTING.md)
