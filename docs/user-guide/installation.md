# Installation Guide

## System Requirements

### Windows
- **OS**: Windows 10 (1803) or later, Windows 11
- **RAM**: 2 GB minimum, 4 GB recommended
- **GPU**: DirectX 11 compatible graphics card
- **Display**: Any resolution, multi-monitor supported

### macOS
- **OS**: macOS 10.15 (Catalina) or later
  - macOS 12.3+ recommended for ScreenCaptureKit
- **RAM**: 2 GB minimum, 4 GB recommended
- **Permissions**: Screen Recording permission required

### Linux
- **Compositor**: X11 or Wayland with PipeWire
- **RAM**: 2 GB minimum
- **GPU**: OpenGL 3.3+ or Vulkan 1.0+

## Download

### Pre-built Binaries (Recommended)

1. Visit the [Releases page](https://github.com/salihcantekin/RustFrame/releases/latest)
2. Download the appropriate package for your platform:
   - **Windows**: `RustFrame-v{version}-windows-x64.zip`
   - **macOS**: `RustFrame-v{version}-macos-universal.dmg` (Intel + Apple Silicon)
   - **Linux**: `RustFrame-v{version}-linux-x64.AppImage`

## Installation Steps

### Windows

1. **Extract the ZIP file**
   ```
   Right-click → Extract All... → Choose destination
   ```

2. **Run the executable**
   ```
   Double-click RustFrame.exe
   ```

3. **Windows Defender SmartScreen** (first time only)
   - If SmartScreen warning appears:
   - Click "More info"
   - Click "Run anyway"
   - This is normal for new applications without extensive download history

4. **No installation required!** 
   - Portable application
   - Settings saved to `%APPDATA%\RustFrame\`

### macOS

1. **Mount the DMG**
   ```
   Double-click RustFrame-v{version}-macos-universal.dmg
   ```

2. **Drag to Applications**
   ```
   Drag RustFrame.app to /Applications folder
   ```

3. **First Launch**
   - Right-click RustFrame.app → Open (or double-click)
   - macOS may show "App is damaged" warning (Gatekeeper):
     ```bash
     # Remove quarantine attribute:
     xattr -cr /Applications/RustFrame.app
     ```

4. **Grant Screen Recording Permission**
   - On first capture, macOS will prompt for permission
   - System Settings → Privacy & Security → Screen Recording
   - Enable RustFrame
   - Restart RustFrame

5. **Settings Location**
   ```
   ~/Library/Application Support/RustFrame/
   ```

### Linux

1. **Make AppImage executable**
   ```bash
   chmod +x RustFrame-v{version}-linux-x64.AppImage
   ```

2. **Run the application**
   ```bash
   ./RustFrame-v{version}-linux-x64.AppImage
   ```

3. **Optional: Install system-wide**
   ```bash
   # Using AppImageLauncher (recommended)
   sudo apt install appimagelauncher  # Ubuntu/Debian
   # Then double-click the AppImage

   # Or manually:
   sudo mv RustFrame*.AppImage /opt/rustframe
   sudo ln -s /opt/rustframe/RustFrame*.AppImage /usr/local/bin/rustframe
   ```

4. **Wayland Permission**
   - PipeWire screen sharing may require portal permission
   - Some compositors auto-prompt, others need manual configuration

5. **Settings Location**
   ```
   ~/.config/RustFrame/
   ```

## Building from Source

If you want to build RustFrame yourself, see the [Developer Guide](../developer/building.md).

## Verification

### Test Installation

1. Launch RustFrame
2. UI window should appear with "Start Capture" button
3. Open Settings → Capture Region
4. A hollow border window should appear on your screen
5. If everything works, installation is successful!

### Check Version

- **Windows/Linux**: Help menu or About dialog
- **macOS**: RustFrame menu → About RustFrame
- **All platforms**: Check logs in settings folder

## Troubleshooting

### Windows

**"VCRUNTIME140.dll is missing"**
```
Install Microsoft Visual C++ Redistributable:
https://aka.ms/vs/17/release/vc_redist.x64.exe
```

**"Application failed to start (0xc000007b)"**
```
- Ensure you downloaded the x64 version (not x86)
- Install/repair .NET Desktop Runtime
```

**Black preview window**
```
- Update graphics drivers
- Try GDI capture method in Settings
```

### macOS

**"App is damaged and can't be opened"**
```bash
# Remove Gatekeeper quarantine:
xattr -cr /Applications/RustFrame.app
```

**Screen Recording permission not working**
```
1. System Settings → Privacy & Security → Screen Recording
2. Remove RustFrame if listed
3. Restart Mac
4. Launch RustFrame again
5. Grant permission when prompted
```

**Preview window shows black screen**
```
- Ensure Screen Recording permission granted
- Try restarting RustFrame
- Check Console.app for errors (search "RustFrame")
```

### Linux

**AppImage won't run**
```bash
# Install FUSE (required for AppImage)
sudo apt install libfuse2  # Ubuntu/Debian
sudo pacman -S fuse2       # Arch
```

**No capture permission**
```bash
# Ensure PipeWire is running (Wayland)
systemctl --user status pipewire

# Or check X11 permissions
xhost +local:
```

**Missing libraries**
```bash
# Install common dependencies
sudo apt install libgtk-3-0 libwebkit2gtk-4.0-37 libayatana-appindicator3-1
```

## Uninstallation

### Windows
1. Delete the RustFrame folder
2. Optionally delete settings: `%APPDATA%\RustFrame\`

### macOS
1. Drag RustFrame.app to Trash
2. Optionally delete settings:
   ```bash
   rm -rf ~/Library/Application\ Support/RustFrame
   ```

### Linux
1. Delete the AppImage file
2. Optionally delete settings:
   ```bash
   rm -rf ~/.config/RustFrame
   ```

## Next Steps

- **New User?** → [Quick Start Guide](quick-start.md)
- **Explore Features** → [Features Overview](features.md)
- **Problems?** → [Troubleshooting](troubleshooting.md)

---

**Previous**: [User Guide Home](README.md) | **Next**: [Quick Start](quick-start.md) →
