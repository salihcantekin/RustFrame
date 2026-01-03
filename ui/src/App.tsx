import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import SettingsDialog from "./components/SettingsDialog";

export interface Settings {
  // Mouse & Cursor
  show_cursor: boolean;
  capture_clicks: boolean;
  click_highlight_color: [number, number, number, number];
  click_dissolve_ms: number;
  
  // Border
  show_border: boolean;
  border_color: [number, number, number, number];
  border_width: number;
  
  // Performance
  target_fps: number;

  // Capture Method
  capture_method: "Wgc" | "GdiCopy";
  
  // Preview Mode
  preview_mode: "TauriCanvas" | "WinApiGdi";

  // Advanced (hidden) WinAPI Destination Window overrides
  // These can be added manually to settings.json when troubleshooting.
  winapi_destination_alpha?: number | null; // 0..255
  winapi_destination_topmost?: boolean | null;
  winapi_destination_click_through?: boolean | null;
  winapi_destination_toolwindow?: boolean | null;
  winapi_destination_layered?: boolean | null;
  winapi_destination_appwindow?: boolean | null;
  winapi_destination_noactivate?: boolean | null;
  winapi_destination_overlapped?: boolean | null;
  winapi_destination_hide_taskbar_after_ms?: number | null;
  
  // Region Memory
  remember_last_region: boolean;
  last_region: [number, number, number, number] | null; // [x, y, width, height]
  
  // REC Indicator
  show_rec_indicator: boolean;
  rec_indicator_size: "small" | "medium" | "large";
}

export interface MonitorInfo {
  id: number;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  is_primary: boolean;
  refresh_rate: number;
}

export interface CaptureProfileInfo {
  id: string;
  file_name: string;
}

export interface CaptureProfileHints {
  hide_taskbar_after_ms?: number | null;
}

function App() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [isCapturing, setIsCapturing] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [captureRegion, setCaptureRegion] = useState({ x: 0, y: 0, width: 800, height: 600 });
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [selectedMonitor, setSelectedMonitor] = useState<number>(0);
  const [devMode, setDevMode] = useState(false);

  const [profiles, setProfiles] = useState<CaptureProfileInfo[]>([]);
  const [activeProfile, setActiveProfile] = useState<string | null>(null);
  const [activeProfileHints, setActiveProfileHints] = useState<CaptureProfileHints | null>(null);

  useEffect(() => {
    initializeApp();
    loadDevMode();
  }, []);

  useEffect(() => {
    const loadHints = async () => {
      if (!activeProfile) {
        setActiveProfileHints(null);
        return;
      }
      try {
        const hints = await invoke<CaptureProfileHints>("get_capture_profile_hints", {
          profile: activeProfile,
        });
        setActiveProfileHints(hints);
      } catch (error) {
        console.error("Failed to load profile hints:", error);
        setActiveProfileHints(null);
      }
    };

    loadHints();
  }, [activeProfile]);
  
  // Combined initialization to ensure proper order
  const initializeApp = async () => {
    try {
      // First load settings
      const loadedSettings = await invoke<Settings>("get_settings");
      setSettings(loadedSettings);

      // Load capture profiles (profile_*.json) and current selection
      const loadedProfiles = await invoke<CaptureProfileInfo[]>("get_capture_profiles");
      setProfiles(loadedProfiles);
      const selectedProfile = await invoke<string | null>("get_active_capture_profile");
      setActiveProfile(selectedProfile);
      
      // Then load monitors
      const monitorList = await invoke<MonitorInfo[]>("get_monitors");
      setMonitors(monitorList);
      
      // Find primary monitor
      const primaryIndex = monitorList.findIndex(m => m.is_primary) || 0;
      setSelectedMonitor(primaryIndex);
      
      // Decide which region to use
      if (loadedSettings.remember_last_region && loadedSettings.last_region) {
        // Use saved region from settings
        const [x, y, width, height] = loadedSettings.last_region;
        setCaptureRegion({ x, y, width, height });
      } else if (monitorList[primaryIndex]) {
        // Use default region based on primary monitor
        const mon = monitorList[primaryIndex];
        setCaptureRegion({
          x: mon.x + Math.floor(mon.width / 4),
          y: mon.y + Math.floor(mon.height / 4),
          width: Math.floor(mon.width / 2),
          height: Math.floor(mon.height / 2),
        });
      }
      
    } catch (error) {
      console.error("Failed to initialize app:", error);
    }
  };

  const handleProfileChange = async (profileId: string) => {
    const next = profileId === "" ? null : profileId;
    try {
      await invoke("set_active_capture_profile", { profile: next });
      setActiveProfile(next);
    } catch (error) {
      console.error("Failed to set active profile:", error);
    }
  };

  // Disable browser features in release mode (context menu, keyboard shortcuts)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // In release mode, block browser shortcuts
      if (!devMode) {
        // Block F5 (refresh), F12 (dev tools), Ctrl+R (refresh), Ctrl+Shift+I (inspect)
        if (e.key === 'F5' || e.key === 'F12') {
          e.preventDefault();
          return;
        }
        if (e.ctrlKey && (e.key === 'r' || e.key === 'R')) {
          e.preventDefault();
          return;
        }
        if (e.ctrlKey && e.shiftKey && (e.key === 'i' || e.key === 'I' || e.key === 'j' || e.key === 'J' || e.key === 'c' || e.key === 'C')) {
          e.preventDefault();
          return;
        }
        // Block Ctrl+U (view source)
        if (e.ctrlKey && (e.key === 'u' || e.key === 'U')) {
          e.preventDefault();
          return;
        }
      }
    };

    const handleContextMenu = (e: MouseEvent) => {
      // Disable right-click context menu in release mode
      if (!devMode) {
        e.preventDefault();
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    document.addEventListener('contextmenu', handleContextMenu);

    return () => {
      document.removeEventListener('keydown', handleKeyDown);
      document.removeEventListener('contextmenu', handleContextMenu);
    };
  }, [devMode]);

  const loadDevMode = async () => {
    try {
      const isDevMode = await invoke<boolean>("is_dev_mode");
      setDevMode(isDevMode);
    } catch (error) {
      console.error("Failed to check dev mode:", error);
    }
  };

  const handleStartCapture = async () => {
    try {
      console.log("Starting capture with region:", captureRegion);
      await invoke("start_capture", {
        x: captureRegion.x,
        y: captureRegion.y,
        width: captureRegion.width,
        height: captureRegion.height,
      });
      setIsCapturing(true);
      console.log("Capture started successfully!");
      //alert("Capture started successfully! Check for hollow border and preview window.");
    } catch (error) {
      console.error("Failed to start capture:", error);
      //alert(`Failed to start capture: ${error}`);
    }
  };

  const handleStopCapture = async () => {
    try {
      await invoke("stop_capture");
      setIsCapturing(false);
      
      // Save last region if remember_last_region is enabled
      if (settings?.remember_last_region) {
        const updatedSettings = {
          ...settings,
          last_region: [captureRegion.x, captureRegion.y, captureRegion.width, captureRegion.height] as [number, number, number, number]
        };
        await invoke("save_settings", { settings: updatedSettings });
        setSettings(updatedSettings);
      }
    } catch (error) {
      console.error("Failed to stop capture:", error);
    }
  };

  const handleSaveSettings = async (newSettings: Settings) => {
    try {
      await invoke("save_settings", { settings: newSettings });
      setSettings(newSettings);
      setShowSettings(false);
    } catch (error) {
      console.error("Failed to save settings:", error);
    }
  };

  if (!settings) {
    return (
      <div className="flex items-center justify-center h-screen bg-gray-900 text-white">
        <div className="text-xl">Loading...</div>
      </div>
    );
  }

  return (
    <div className="h-screen bg-gray-900 text-white flex flex-col">
      {/* Header */}
      <div className="bg-gray-800 border-b border-gray-700 px-6 py-4">
        <div className="flex items-center justify-between">
          <h1 className="text-2xl font-bold">RustFrame</h1>
          <button
            onClick={() => setShowSettings(true)}
            className="px-4 py-2 bg-gray-700 hover:bg-gray-600 rounded-lg transition-colors"
          >
            Settings
          </button>
        </div>
      </div>

      {/* Main Content */}
      <div className="flex-1 p-6">
        <div className="max-w-4xl mx-auto space-y-6">
          {/* Quick Capture Region Info */}
          <div className="bg-gray-800 rounded-lg p-6 border border-gray-700">
            <h2 className="text-xl font-semibold mb-4">Capture Region</h2>
            <p className="text-sm text-gray-400 mb-4">
              💡 Configure capture region in <button onClick={() => setShowSettings(true)} className="text-blue-400 hover:text-blue-300 underline">Settings → Capture Region</button>. The hollow border will appear at the configured position.
            </p>
            <div className="grid grid-cols-2 gap-4 text-sm">
              <div className="flex justify-between p-3 bg-gray-700 rounded-lg">
                <span className="text-gray-400">Position:</span>
                <span>{captureRegion.x}, {captureRegion.y}</span>
              </div>
              <div className="flex justify-between p-3 bg-gray-700 rounded-lg">
                <span className="text-gray-400">Size:</span>
                <span>{captureRegion.width} × {captureRegion.height}</span>
              </div>
            </div>
            {monitors[selectedMonitor] && (
              <div className="mt-4 text-sm text-gray-400">
                Monitor: {monitors[selectedMonitor].name} ({monitors[selectedMonitor].width}x{monitors[selectedMonitor].height})
              </div>
            )}
          </div>

          {/* Capture Control */}
          <div className="bg-gray-800 rounded-lg p-6 border border-gray-700">
            <div className="flex items-center justify-between">
              <div>
                <h2 className="text-xl font-semibold">Capture Status</h2>
                <p className="text-gray-400 mt-1">
                  {isCapturing ? "Capturing..." : "Ready to capture"}
                </p>
                <div className="mt-3 flex items-center gap-3">
                  <label className="text-sm text-gray-400">Capture Profile:</label>
                  <select
                    value={activeProfile ?? ""}
                    onChange={(e) => handleProfileChange(e.target.value)}
                    className="bg-gray-700 border border-gray-600 rounded px-2 py-1 text-sm"
                    disabled={isCapturing}
                  >
                    <option value="">Default</option>
                    {profiles.map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.id}
                      </option>
                    ))}
                  </select>
                  <span className="text-xs text-gray-500">
                    {isCapturing ? "(takes effect next start)" : ""}
                  </span>
                </div>
                {activeProfileHints?.hide_taskbar_after_ms != null && (
                  <div className="mt-2 text-xs text-gray-400 max-w-md">
                    Note: Some profiles (e.g. Discord) may require the preview window to appear in the taskbar briefly so it can be selected.
                    This profile is configured to auto-hide it after <span className="font-medium">{activeProfileHints.hide_taskbar_after_ms} ms</span>.
                  </div>
                )}
              </div>
              <button
                onClick={isCapturing ? handleStopCapture : handleStartCapture}
                className={`px-6 py-3 rounded-lg font-semibold transition-colors ${
                  isCapturing
                    ? "bg-red-600 hover:bg-red-700"
                    : "bg-green-600 hover:bg-green-700"
                }`}
              >
                {isCapturing ? "Stop Capture" : "Start Capture"}
              </button>
            </div>
          </div>

          {/* Current Settings Preview */}
          <div className="bg-gray-800 rounded-lg p-6 border border-gray-700">
            <h2 className="text-xl font-semibold mb-4">Current Settings</h2>
            <div className="grid grid-cols-2 gap-3 text-sm">
              <div className="flex justify-between">
                <span className="text-gray-400">Show Cursor:</span>
                <span>{settings.show_cursor ? "Yes" : "No"}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-400">Capture Clicks:</span>
                <span>{settings.capture_clicks ? "Yes" : "No"}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-400">Target FPS:</span>
                <span>{settings.target_fps}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-400">Border Width:</span>
                <span>{settings.border_width}px</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Settings Dialog */}
      {showSettings && (
        <SettingsDialog
          settings={settings}
          captureRegion={captureRegion}
          monitors={monitors}
          selectedMonitor={selectedMonitor}
          onSave={handleSaveSettings}
          onRegionChange={setCaptureRegion}
          onMonitorChange={setSelectedMonitor}
          onClose={() => setShowSettings(false)}
        />
      )}
    </div>
  );
}

export default App;
