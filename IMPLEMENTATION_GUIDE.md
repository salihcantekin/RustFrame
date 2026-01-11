# Windows Window Filtering - Implementation Guide

## Code Changes Required

### 1. Frontend Changes (ui/src/App.tsx)

Add platform support state and check:

```tsx
// After line 93 (const [settings, setSettings] = useState...)
const [supportsWindowFiltering, setSupportsWindowFiltering] = useState(false);

// In useEffect after loadSettings():
useEffect(() => {
  loadSettings();
  // Check platform support for window filtering
  invoke<boolean>('supports_window_filtering')
    .then(setSupportsWindowFiltering)
    .catch(() => setSupportsWindowFiltering(false));
}, []);

// Modify windowFilterSummary function (around line 442):
const windowFilterSummary = (() => {
  const wf = settings.window_filter;
  // Only show summary on platforms that support filtering
  if (!supportsWindowFiltering) {
    return null;
  }
  if (wf.mode === "include_only") {
    const count = wf.included_windows.length;
    return count > 0
      ? `Include only ${count} window${count === 1 ? "" : "s"}`
      : "Include only selected windows";
  }
  if (wf.mode === "exclude_list") {
    const count = wf.excluded_windows.length;
    return count > 0
      ? `Excluding ${count} window${count === 1 ? "" : "s"}`
      : "Capture all except preview";
  }
  return "Capture all windows";
})();

// When rendering windowFilterSummary (find the div):
{windowFilterSummary && (
  <div className="text-xs text-gray-400 mt-1">
    {windowFilterSummary}
  </div>
)}

// Pass supportsWindowFiltering to SettingsDialog:
<SettingsDialog
  initialTab={initialSettingsTab}
  settings={settings}
  platformInfo={platformInfo}
  captureRegion={captureRegion}
  monitors={monitors}
  selectedMonitor={selectedMonitor}
  supportsWindowFiltering={supportsWindowFiltering}  // ADD THIS
  onSave={handleSaveSettings}
  onRegionChange={setCaptureRegion}
  onMonitorChange={setSelectedMonitor}
  onClose={() => setShowSettings(false)}
/>
```

### 2. Frontend Changes (ui/src/components/SettingsDialog.tsx)

Add prop and conditional tab rendering:

```tsx
// In SettingsDialogProps interface (around line 14):
interface SettingsDialogProps {
  initialTab?: TabType;
  settings: Settings;
  platformInfo: PlatformInfo;
  captureRegion: CaptureRegion;
  monitors: MonitorInfo[];
  selectedMonitor: number;
  supportsWindowFiltering: boolean;  // ADD THIS
  onSave: (settings: Settings) => void;
  onRegionChange: (region: CaptureRegion) => void;
  onMonitorChange: (index: number) => void;
  onClose: () => void;
}

// In function parameter destructuring (around line 37):
function SettingsDialog({ 
  initialTab = "capture",
  settings,
  platformInfo,
  captureRegion, 
  monitors, 
  selectedMonitor,
  supportsWindowFiltering,  // ADD THIS
  onSave, 
  onRegionChange, 
  onMonitorChange, 
  onClose 
}: SettingsDialogProps) {

// Modify tabs array (around line 308):
const tabs: { id: TabType; label: string; icon: string }[] = [
  { id: "capture", label: "Capture", icon: "🎯" },
  { id: "mouse", label: "Mouse", icon: "🖱️" },
  { id: "visual", label: "Visual", icon: "🎨" },
  { id: "region", label: "Region", icon: "📐" },
  { id: "performance", label: "Perf", icon: "🚀" },
  ...(supportsWindowFiltering ? [{ id: "share_content" as const, label: "Share Content", icon: "📺" }] : []),
  { id: "profiles", label: "Profiles", icon: "📦" },
  { id: "advanced", label: "Advanced", icon: "🔧" },
  { id: "about", label: "About", icon: "ℹ️" },
];

// Find WindowExclusionTab rendering and wrap it:
{supportsWindowFiltering && activeTab === "share_content" && (
  <WindowExclusionTab settings={localSettings} onUpdate={handleUpdate} />
)}
```

### 3. Build and Test

```bash
# Frontend
cd ui
npm run build

# Backend
cargo build

# Test
cargo tauri dev
```

### 4. Create Documentation Files

Create `docs/technical/WINDOWS_LIMITATIONS.md` with the content from IMPLEMENTATION_GUIDE.md section below.

Update README.md to mention platform-specific features.

---

## Complete Documentation Content

[See attached WINDOWS_LIMITATIONS.md content in next section]
