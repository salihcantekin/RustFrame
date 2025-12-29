# Contributing to RustFrame

First off, thank you for considering contributing to RustFrame! 🎉

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [How to Contribute](#how-to-contribute)
- [Pull Request Process](#pull-request-process)
- [Coding Standards](#coding-standards)
- [Commit Message Guidelines](#commit-message-guidelines)
- [Reporting Bugs](#reporting-bugs)
- [Suggesting Features](#suggesting-features)

## Code of Conduct

This project follows a simple code of conduct:
- Be respectful and inclusive
- Focus on constructive feedback
- Help others learn and grow

## Getting Started

### Prerequisites

- **Windows 10/11** (required for Windows.Graphics.Capture API)
- **Rust 1.87.0+** with MSVC toolchain
- **Visual Studio Build Tools** (for Windows SDK)

### Development Setup

1. **Clone the repository**
   ```bash
   git clone https://github.com/salihcantekin/RustFrame.git
   cd RustFrame
   ```

2. **Install Rust (if not already installed)**
   ```bash
   winget install Rustlang.Rustup
   rustup default stable-msvc
   ```

3. **Build the project**
   ```bash
   cargo build
   ```

4. **Run in development mode**
   ```bash
   cargo run
   ```

See [BUILD_INSTRUCTIONS.md](BUILD_INSTRUCTIONS.md) for detailed setup information.

## How to Contribute

### Types of Contributions

We welcome various types of contributions:

- 🐛 **Bug fixes**: Found a bug? Submit a fix!
- ✨ **New features**: Have an idea? Open an issue first to discuss
- 📝 **Documentation**: Improve docs, add examples
- 🧪 **Tests**: Add or improve test coverage
- 🔧 **Refactoring**: Clean up code, improve performance

### Branch Strategy

- `master` - Stable release branch
- `dev` - Development branch (target for PRs)
- `feature/*` - Feature branches
- `fix/*` - Bug fix branches

## Pull Request Process

1. **Fork the repository** and create your branch from `dev`
   ```bash
   git checkout dev
   git pull origin dev
   git checkout -b feature/your-feature-name
   ```

2. **Make your changes** following our coding standards

3. **Test your changes**
   ```bash
   cargo build
   cargo test
   cargo clippy
   ```

4. **Commit with a descriptive message** (see guidelines below)

5. **Push to your fork**
   ```bash
   git push origin feature/your-feature-name
   ```

6. **Open a Pull Request** to the `dev` branch

### PR Checklist

- [ ] Code compiles without warnings (`cargo build`)
- [ ] All tests pass (`cargo test`)
- [ ] No clippy warnings (`cargo clippy`)
- [ ] Code is formatted (`cargo fmt`)
- [ ] Documentation updated if needed
- [ ] Changelog updated in `docs/changelog/`

## Coding Standards

### Rust Style

- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `cargo fmt` before committing
- Address all `cargo clippy` warnings
- Add doc comments for public functions

### Code Organization

```
src/
├── main.rs           # Application entry, event handling
├── capture.rs        # Windows.Graphics.Capture implementation
├── renderer.rs       # wgpu rendering pipeline
├── window_manager.rs # Window creation and management
├── bitmap_font.rs    # Text rendering
├── constants.rs      # Configuration constants
├── settings_dialog.rs# Win32 settings UI
├── shader.wgsl       # GPU shaders
└── utils.rs          # Utility functions
```

### Safety Guidelines

Since this project uses Windows COM APIs:

1. **Document unsafe blocks** with safety justifications
2. **Handle COM errors** properly with `Result` types
3. **Clean up resources** in `Drop` implementations
4. **Test on multiple Windows versions** when possible

## Commit Message Guidelines

We follow [Conventional Commits](https://www.conventionalcommits.org/):

### Format

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

### Types

| Type | Description |
|------|-------------|
| `feat` | New feature |
| `fix` | Bug fix |
| `docs` | Documentation only |
| `style` | Formatting, no code change |
| `refactor` | Code change without feature/fix |
| `perf` | Performance improvement |
| `test` | Adding tests |
| `chore` | Build, CI, dependencies |

### Examples

```
feat(capture): add multi-monitor support

- Use MonitorFromPoint for accurate detection
- Pass overlay position to CaptureEngine
- Fixes #3

fix(tray): use application icon instead of default

docs(readme): update feature list with new capabilities
```

## Reporting Bugs

### Before Submitting

1. Check [existing issues](https://github.com/salihcantekin/RustFrame/issues)
2. Try the latest `dev` branch
3. Collect system information

### Bug Report Template

```markdown
**Description**
A clear description of the bug.

**Steps to Reproduce**
1. Step one
2. Step two
3. ...

**Expected Behavior**
What should happen.

**Actual Behavior**
What actually happens.

**System Information**
- Windows Version: [e.g., Windows 11 23H2]
- RustFrame Version: [e.g., v0.2.0]
- GPU: [e.g., NVIDIA RTX 3080]
- Number of Monitors: [e.g., 2]

**Screenshots/Logs**
If applicable, add screenshots or log output.
```

## Suggesting Features

### Feature Request Template

```markdown
**Problem**
What problem does this solve?

**Proposed Solution**
How would you implement it?

**Alternatives Considered**
Other approaches you've thought about.

**Additional Context**
Any other information, mockups, etc.
```

## Questions?

- Open a [Discussion](https://github.com/salihcantekin/RustFrame/discussions)
- Check [existing issues](https://github.com/salihcantekin/RustFrame/issues)

---

Thank you for contributing! 🦀
