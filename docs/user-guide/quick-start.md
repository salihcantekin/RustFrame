# Quick Start Guide

Get RustFrame running in under 2 minutes!

## 🚀 30-Second Workflow

1. **Launch RustFrame** → UI window opens
2. **Configure region** → Settings → Capture Region tab
3. **Adjust border** → Drag and resize the hollow border
4. **Start capture** → Click "Start Capture" button
5. **Share** → In your video call, select "RustFrame Preview" window

## Step-by-Step Guide

### 1. Launch the Application

**Windows**: Double-click `RustFrame.exe`  
**macOS**: Open RustFrame from Applications  
**Linux**: Run the AppImage

You'll see the main control UI:

```
┌─────────────────────────────────┐
│      RustFrame Control UI       │
├─────────────────────────────────┤
│  Capture Status: ⚫ Not Active   │
│                                 │
│  [     Start Capture      ]     │
│  [        Settings        ]     │
└─────────────────────────────────┘
```

### 2. Configure Your Capture Region

Click the **Settings** button to open settings dialog.

Navigate to **Capture Region** tab:

- **Monitor**: Select which display to capture from
- **Position**: X, Y coordinates (or use preview border)
- **Size**: Width and height in pixels
- **Preview Border**: Toggle to see the region visually

**Tip**: Enable "Preview Border" to see and adjust the region interactively!

### 3. Adjust the Border (Optional)

┌────────────────────────────────┐
│                                │  ← Border (draggable/resizable)
│   ┌────────────────────────┐   │
│   │                        │   │
│   │   YOUR CONTENT HERE    │   │  ← This area will be captured
│   │                        │   │
│   └────────────────────────┘   │
│                                │
└────────────────────────────────┘
```

**Controls**:
- **Drag**: Click inside and drag to move
- **Resize**: Drag corners or edges
- **Monitor Switch**: Drag to another monitor (auto-detects!)

### 4. Start Capturing

Click **"Start Capture"** button in the UI.

RustFrame will:
1. Create the hollow border (shows capture region)
2. Create the preview window (shareable output)
3. Start capturing the region in real-time

**Status**: Button changes to "Stop Capture" ✅

### 5. Share in Your Video Call

#### Google Meet
1. Click "Present now" → "A window"
2. Find **"RustFrame Preview"** or **"Destination Window"**
3. Select it
4. Click "Share"

#### Zoom
1. Click "Share Screen"
2. Select "Advanced" tab → "Portion of Screen"  
   OR select **"RustFrame Preview"** from window list
3. Click "Share"

#### Microsoft Teams
1. Click "Share" icon
2. Select "Window" tab
3. Find **"RustFrame Preview"**
4. Click "Share"

#### Discord
1. Click "Share Your Screen" or camera icon
2. Select **"RustFrame Preview"** window
3. Ensure "Application Window" is selected (not Screen)

**Important**: Share the **Preview Window**, not the hollow border!

### 6. Adjust During Capture (Optional)

While capturing, you can:
- **Move region**: Drag the border to a new position
- **Resize region**: Drag border edges/corners
- **Switch monitors**: Drag to another display
- **Toggle cursor**: Settings → Show Cursor
- **Adjust FPS**: Settings → Performance → Target FPS

Changes apply immediately—no need to restart capture!

### 7. Stop Capturing

When done:
1. Click **"Stop Capture"** in the UI
2. Or close the UI window
3. Border and preview window will close automatically

## Common Workflows

Goal: Share only VS Code, exclude everything else

3. Enable Preview Border
4. Position border to cover ONLY VS Code window
```

```
Setup: Laptop screen (primary) + External monitor

Steps:
1. Settings → Capture Region → Monitor dropdown
2. Select "Monitor 2" (external)
3. Enable Preview Border (it will appear on Monitor 2)
4. Adjust border position on external monitor
5. Start Capture
6. Share preview window in video call
```

### Scenario 3: Hide Personal Info

```
Goal: Share browser but hide bookmarks bar and tabs

Steps:
1. Open browser window
6. Start Capture and share
```

### Scenario 4: Live Demo with Click Highlights

```
Goal: Tutorial video with visible mouse clicks

Steps:
1. Settings → Mouse & Clicks
3. Choose highlight color (e.g., bright red)
4. Set radius (20-30px recommended)
5. Set dissolve time (300-500ms)
6. Start Capture
7. Your clicks will show colored circles in the preview!
```

## Keyboard Shortcuts

| Action | Windows | macOS | Linux |
|--------|---------|-------|-------|
| Open Settings | `Ctrl+,` | `Cmd+,` | `Ctrl+,` |
| Start/Stop | `Ctrl+S` | `Cmd+S` | `Ctrl+S` |
| Quit | `Ctrl+Q` | `Cmd+Q` | `Ctrl+Q` |

## Tips & Tricks

### 🎯 Perfect Region Selection
- Use grid lines (enable in Settings → Display)
- Snap to pixel boundaries for crisp edges
- Test with screenshot to verify exact area

### 🚀 Performance Optimization
- Lower FPS (30-60) for presentations
- Higher FPS (60-144) for smooth demos
- Disable click highlights if not needed

### 🖱️ Cursor Visibility
- **Hide cursor** (default): Best for presentations
- **Show cursor**: Helpful for tutorials where you point at things

### 📱 Portrait Content
- RustFrame handles any aspect ratio
- Perfect for sharing mobile app demos

### 💡 Remember Last Region
- Settings → Advanced → "Remember Last Region"
- Your position/size is saved between sessions

## Quick Troubleshooting

### Black preview window?
→ Check [Troubleshooting Guide](troubleshooting.md#black-preview-window)

### Can't find preview window in video call?
→ Look for "RustFrame Preview" or "Destination Window" in window list

### Border corners too thick?
→ Capture automatically excludes border (don't worry, it won't show!)

### High CPU usage?
→ Lower target FPS or disable click highlights

## What's Next?

- **Learn All Features** → [Features Guide](features.md)
- **Advanced Configuration** → [Advanced Settings](features.md#advanced-settings)
- **Problems?** → [Troubleshooting](troubleshooting.md)
- **Customize** → [Capture Profiles](features.md#capture-profiles)

## Example Use Cases

✅ **Teaching/Tutorials**: Show specific code editor pane  
✅ **Client Demos**: Share app window without desktop clutter  
✅ **Gaming**: Capture game area without overlays/chat  
✅ **Privacy**: Exclude personal info (notifications, browser tabs)  
✅ **Multi-Window**: Capture parts of multiple windows  
✅ **Wide-screen Sharing**: Share portion of ultrawide monitor  

---

**Previous**: [Installation](installation.md) | **Next**: [Features Guide](features.md) →
