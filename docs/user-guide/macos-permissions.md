# macOS Permissions Guide

RustFrame requires specific system permissions to function properly on macOS. This guide explains what permissions are needed and why.

---

## ⚠️ IMPORTANT: First Launch

**On first launch, macOS will automatically ask for Screen Recording permission.** You MUST approve this for RustFrame to work.

**Signs that permissions are missing:**
- Start Capture button doesn't change to Stop
- Black/empty preview window
- Error messages or warnings about access
- macOS shows: "RustFrame is requesting to bypass the system private window picker"

**Solution:** Grant Screen Recording and Accessibility permissions (see below).

---

## Required Permissions

### 1. Screen Recording Permission ⚠️ **CRITICAL**

**Why it's needed:** RustFrame captures your screen region to display it in the preview window.

**How to grant:**
1. macOS will prompt you automatically on first launch - **Click "Allow"**
2. Or manually: **System Settings** → **Privacy & Security** → **Screen Recording**
3. Enable checkbox for **RustFrame** (or **Terminal/VS Code** if running via `cargo`)
4. **Restart RustFrame** after granting permission

**What happens without it:**
- Preview window shows black screen
- Start button doesn't change to Stop
- Application logs: "Screen capture permission denied"
- macOS shows "bypass system private window picker" warning

---

### 2. Accessibility Permission ⚠️ **RECOMMENDED**

**Why it's needed:** For mouse tracking and click highlighting features. Also helps with proper cursor capture.

**How to grant:**
1. **System Settings** → **Privacy & Security** → **Accessibility**
2. Click the **🔒 lock** icon and enter your password
3. Click **+** button and add **RustFrame**
4. Enable checkbox for **RustFrame**
5. **Restart RustFrame** after granting permission

**What happens without it:**
- Basic features work fine
- Advanced click effects may not work

---

### 3. Automation Permission (Single Instance)

**Why it's needed:** When you launch RustFrame while it's already running, the app uses AppleScript to bring the existing window to front.

**How to grant:**
1. macOS will prompt you when launching second instance
2. Or manually: **System Settings** → **Privacy & Security** → **Automation**
3. Enable **RustFrame** → **System Events** (or **VS Code/Terminal** → **System Events** when running via cargo)

**What happens without it:**
- First instance works normally
- Second instance shows "RustFrame is already running!" but window doesn't come to front
- You'll need to manually click the window in Mission Control/Expose

---

## First Launch Checklist

When you launch RustFrame for the first time:

1. ✅ **Screen Recording prompt** → Click "Allow" (required)
2. ✅ **Automation prompt** (when launching 2nd instance) → Click "OK" (recommended)
3. ⚠️ If you accidentally clicked "Don't Allow", follow manual grant steps above

---

## Running from Terminal vs App Bundle

### Running via `cargo run` or Terminal

Permissions will be requested for **Terminal.app** or **VS Code.app** (the parent process).

**Pros:**
- Easy for development
- Quick testing

**Cons:**
- Terminal must keep running
- Permissions tied to Terminal app
- Icon shows as generic Terminal icon in Dock

### Running as App Bundle (Production)

Permissions will be requested for **RustFrame.app** itself.

**Pros:**
- Clean app experience
- Proper app icon in Dock
- Independent from Terminal
- Recommended for end users

**Cons:**
- Requires building `.app` bundle: `cargo tauri build`

---

## Troubleshooting

### Preview window shows black screen

**Cause:** Screen Recording permission not granted

**Solution:**
1. Open **System Settings** → **Privacy & Security** → **Screen Recording**
2. Enable **RustFrame** (or Terminal if running via cargo)
3. **Restart RustFrame** (permission changes require app restart)

### Second instance doesn't bring window to front

**Cause:** Automation permission not granted

**Solution:**
1. Open **System Settings** → **Privacy & Security** → **Automation**
2. Expand **RustFrame** (or Terminal/VS Code)
3. Enable **System Events**
4. Try launching second instance again

### "Visual Studio Code wants access to control System Events"

**Cause:** Running via `cargo run` from VS Code terminal

**Solution:**
- This is normal! Click "OK" to allow
- Permission is for VS Code, not RustFrame itself
- When using app bundle, permission will be for RustFrame.app

---

## Security & Privacy

### Why does RustFrame need these permissions?

- **Screen Recording:** Core feature - capturing screen region
- **Automation:** User experience - bringing window to front when already running
- **Accessibility:** Optional - enhanced mouse tracking (not required)

### What data is collected?

**Nothing.** RustFrame:
- Does NOT send data to internet
- Does NOT record or save screenshots
- Only captures screen content **while preview window is open**
- All capture happens **locally on your Mac**

### Can I revoke permissions?

Yes, at any time in **System Settings** → **Privacy & Security**. The app will stop working but no data is retained.

---

## Version-Specific Notes

### macOS 15 Sequoia (2024)

**"Bypass private window picker" warning:**
- macOS 15 introduced stricter screen capture policies
- Apps must either use SCContentSharingPicker (system picker) OR show this warning
- RustFrame **cannot use the picker** because it captures custom regions, not single windows
- **This warning is expected and unavoidable**
- After granting Screen Recording + Accessibility permissions, the app works normally
- The warning may appear on first few launches, then macOS remembers your choice

### macOS 14 Sonoma (2023)

Screen Recording permission is **mandatory** and strictly enforced. The app cannot function without it.
No "bypass picker" warning - works smoothly.

### macOS 13 Ventura (2022)

Same as Sonoma. Permission prompts work correctly.

### macOS 12 Monterey (2021)

Screen Recording permission works the same, but uses legacy ScreenCaptureKit API.

### macOS 11 Big Sur and earlier (2020)

Uses CGWindowListCreateImage API. Permission handling is the same.

### macOS 10.15 Catalina (2019)

**Minimum supported version.** Screen Recording permission was introduced in Catalina.
All permission descriptions (NSScreenCaptureUsageDescription) work correctly.

---

## Developer Notes

### Testing Permissions in Development

When running via `cargo run`:
- Permissions apply to **Terminal.app** process
- To test app bundle permissions: `cargo tauri build` → run `.app` bundle
- Reset permissions: `tccutil reset All com.salihcantekin.rustframe` (requires SIP disabled)

### Bundle Identifier

Production app uses: `com.salihcantekin.rustframe` (from `tauri.conf.json`)

### Why Can't We Use SCContentSharingPicker?

RustFrame's core feature is **arbitrary region selection** - users draw a region anywhere on screen, not limited to window boundaries. SCContentSharingPicker only allows selecting entire windows or displays, which defeats the purpose of RustFrame. Therefore, the macOS 15 "bypass picker" warning is an unavoidable trade-off for this functionality.

This identifier is used for:
- Permission storage in TCC database
- Single instance lock file: `~/.config/RustFrame/.rustframe.lock`
- App identification in NSWorkspace

---

## Related Documentation

- [Installation Guide](installation.md) - How to install RustFrame
- [Quick Start](quick-start.md) - Getting started guide
- [Troubleshooting](troubleshooting.md) - Common issues and solutions
- [FAQ](faq.md) - Frequently asked questions

---

**Last Updated:** January 9, 2026  
**RustFrame Version:** 1.1.0+
