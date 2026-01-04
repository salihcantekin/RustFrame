import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-shell";
import SettingsDialog from "./components/SettingsDialog";
import { AppConfig } from "./config";

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
  const [showDonate, setShowDonate] = useState(false);
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
    <div className="h-screen bg-gray-900 text-white flex flex-col rounded-lg overflow-hidden border border-gray-700">
      {/* Custom Titlebar */}
      <div 
        className="bg-gray-800 border-b border-gray-700 px-4 py-2 flex items-center justify-between select-none cursor-default"
        onMouseDown={(e) => {
          // Only handle left click and not on interactive elements
          if (e.button !== 0 || (e.target as HTMLElement).closest('button')) {
            return;
          }
          
          // e.detail: 1 = single click, 2 = double click
          if (e.detail === 2) {
            // Double click - toggle maximize
            getCurrentWindow().toggleMaximize();
          } else {
            // Single click - start dragging
            getCurrentWindow().startDragging();
          }
        }}
      >
        <div className="flex items-center gap-3 pointer-events-none">
          <img src="/icon.png" alt="RustFrame" className="w-6 h-6" />
          <h1 className="text-lg font-semibold">RustFrame</h1>
        </div>
        <div className="flex items-center gap-2">
          {/* Donate Button */}
          <button
            onClick={() => setShowDonate(true)}
            className="px-2 py-1 text-sm flex items-center gap-1 text-red-500 hover:text-red-400 hover:bg-red-500/10 rounded transition-colors"
            title="Support Development"
          >
            <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
              <path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/>
            </svg>
            <span className="hidden sm:inline">Donate</span>
          </button>
          <div className="w-px h-4 bg-gray-600"></div>
          <button
            onClick={() => setShowSettings(true)}
            className="px-3 py-1 text-sm bg-gray-700 hover:bg-gray-600 rounded transition-colors flex items-center gap-1.5"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            </svg>
            Settings
          </button>
          <button
            onClick={() => getCurrentWindow().minimize()}
            className="w-8 h-8 flex items-center justify-center hover:bg-gray-700 rounded transition-colors"
            title="Minimize"
          >
            <svg className="w-4 h-4 pointer-events-none" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 12H4" />
            </svg>
          </button>
          <button
            onClick={() => getCurrentWindow().toggleMaximize()}
            className="w-8 h-8 flex items-center justify-center hover:bg-gray-700 rounded transition-colors"
            title="Maximize"
          >
            <svg className="w-4 h-4 pointer-events-none" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 8V4h16v4M4 16v4h16v-4" />
            </svg>
          </button>
          <button
            onClick={() => getCurrentWindow().close()}
            className="w-8 h-8 flex items-center justify-center hover:bg-red-600 rounded transition-colors"
            title="Close"
          >
            <svg className="w-4 h-4 pointer-events-none" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>

      {/* Main Content */}
      <div className="flex-1 p-6 overflow-y-auto">
        <div className="max-w-4xl mx-auto space-y-6">
          {/* Current Settings Preview - More Prominent */}
          <div className="bg-gradient-to-br from-gray-800 to-gray-850 rounded-lg p-6 border border-gray-600 shadow-lg">
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-xl font-semibold flex items-center gap-2">
                <svg className="w-5 h-5 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                </svg>
                Active Configuration
              </h2>
              <button
                onClick={() => setShowSettings(true)}
                className="text-sm text-blue-400 hover:text-blue-300 flex items-center gap-1"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                </svg>
                Edit
              </button>
            </div>
            
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              {/* Cursor Settings */}
              <div className="bg-gray-700/50 rounded-lg p-4 text-center">
                <div className={`w-10 h-10 mx-auto mb-2 rounded-full flex items-center justify-center ${settings.show_cursor ? 'bg-green-500/20 text-green-400' : 'bg-gray-600 text-gray-400'}`}>
                  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 15l-2 5L9 9l11 4-5 2zm0 0l5 5M7.188 2.239l.777 2.897M5.136 7.965l-2.898-.777M13.95 4.05l-2.122 2.122m-5.657 5.656l-2.12 2.122" />
                  </svg>
                </div>
                <div className="text-xs text-gray-400">Show Cursor</div>
                <div className={`text-sm font-medium ${settings.show_cursor ? 'text-green-400' : 'text-gray-500'}`}>
                  {settings.show_cursor ? "ON" : "OFF"}
                </div>
              </div>

              {/* Click Capture */}
              <div className="bg-gray-700/50 rounded-lg p-4 text-center">
                <div className={`w-10 h-10 mx-auto mb-2 rounded-full flex items-center justify-center ${settings.capture_clicks ? 'bg-green-500/20 text-green-400' : 'bg-gray-600 text-gray-400'}`}>
                  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                  </svg>
                </div>
                <div className="text-xs text-gray-400">Click Highlight</div>
                <div className={`text-sm font-medium ${settings.capture_clicks ? 'text-green-400' : 'text-gray-500'}`}>
                  {settings.capture_clicks ? "ON" : "OFF"}
                </div>
              </div>

              {/* FPS */}
              <div className="bg-gray-700/50 rounded-lg p-4 text-center">
                <div className="w-10 h-10 mx-auto mb-2 rounded-full flex items-center justify-center bg-blue-500/20 text-blue-400">
                  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
                  </svg>
                </div>
                <div className="text-xs text-gray-400">Target FPS</div>
                <div className="text-sm font-medium text-blue-400">{settings.target_fps}</div>
              </div>

              {/* Border */}
              <div className="bg-gray-700/50 rounded-lg p-4 text-center">
                <div className={`w-10 h-10 mx-auto mb-2 rounded-full flex items-center justify-center ${settings.show_border ? 'bg-purple-500/20 text-purple-400' : 'bg-gray-600 text-gray-400'}`}>
                  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 5a1 1 0 011-1h14a1 1 0 011 1v2a1 1 0 01-1 1H5a1 1 0 01-1-1V5zM4 13a1 1 0 011-1h6a1 1 0 011 1v6a1 1 0 01-1 1H5a1 1 0 01-1-1v-6zM16 13a1 1 0 011-1h2a1 1 0 011 1v6a1 1 0 01-1 1h-2a1 1 0 01-1-1v-6z" />
                  </svg>
                </div>
                <div className="text-xs text-gray-400">Border</div>
                <div className={`text-sm font-medium ${settings.show_border ? 'text-purple-400' : 'text-gray-500'}`}>
                  {settings.show_border ? `${settings.border_width}px` : "OFF"}
                </div>
              </div>
            </div>

            {/* Additional Settings Row */}
            <div className="mt-4 pt-4 border-t border-gray-700 grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
              <div className="flex items-center gap-2">
                <span className="text-gray-400">Capture:</span>
                <span className="text-white font-medium">{settings.capture_method === "Wgc" ? "Windows Graphics" : "GDI Copy"}</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-gray-400">Preview:</span>
                <span className="text-white font-medium">{settings.preview_mode === "TauriCanvas" ? "Tauri" : "WinAPI"}</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-gray-400">REC Indicator:</span>
                <span className={settings.show_rec_indicator ? "text-red-400 font-medium" : "text-gray-500"}>{settings.show_rec_indicator ? settings.rec_indicator_size : "OFF"}</span>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-gray-400">Remember Region:</span>
                <span className={settings.remember_last_region ? "text-green-400 font-medium" : "text-gray-500"}>{settings.remember_last_region ? "Yes" : "No"}</span>
              </div>
            </div>
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

          {/* Capture Region Info - Moved to Bottom */}
          <div className="bg-gray-800 rounded-lg p-6 border border-gray-700">
            <h2 className="text-xl font-semibold mb-4 flex items-center gap-2">
              <svg className="w-5 h-5 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 5a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1H5a1 1 0 01-1-1V5zM14 5a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1V5zM4 15a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1H5a1 1 0 01-1-1v-4zM14 15a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1v-4z" />
              </svg>
              Capture Region
            </h2>
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

      {/* Donate Modal */}
      {showDonate && (
        <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50" onClick={() => setShowDonate(false)}>
          <div 
            className="bg-gray-800 rounded-xl overflow-hidden max-w-md w-full mx-4 border border-gray-700 shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            {/* Header */}
            <div className="flex items-center justify-between p-4 border-b border-gray-700">
              <h2 className="text-lg font-semibold flex items-center gap-2">
                <svg className="w-5 h-5 text-pink-400" fill="currentColor" viewBox="0 0 24 24">
                  <path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/>
                </svg>
                Support RustFrame
              </h2>
              <button
                onClick={() => setShowDonate(false)}
                className="text-gray-400 hover:text-white transition-colors p-1"
              >
                <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
            </div>
            
            {/* Content */}
            <div className="p-6">
              <p className="text-gray-300 text-sm mb-6 text-center">
                RustFrame is free and open source. If you find it useful, consider supporting its development with a donation. Every contribution helps!
              </p>

              {/* QR Code */}
              <div className="flex justify-center mb-6">
                <div className="bg-white rounded-lg p-4">
                  <img src={AppConfig.donate.qrCodePath} alt="PayPal Donate QR Code" className="w-40 h-40 object-contain" />
                </div>
              </div>

              <p className="text-gray-400 text-xs text-center mb-4">
                Scan with your phone or click the button below
              </p>

              {/* Open in Browser Button */}
              <button
                onClick={() => open(AppConfig.donate.paypalUrl)}
                className="w-full py-3 bg-blue-600 hover:bg-blue-700 text-white font-semibold rounded-lg transition-colors flex items-center justify-center gap-2"
              >
                <svg className="w-5 h-5" viewBox="0 0 24 24" fill="currentColor">
                  <path d="M7.076 21.337H2.47a.641.641 0 0 1-.633-.74L4.944 3.72a.771.771 0 0 1 .76-.654h6.281c2.09 0 3.63.554 4.58 1.648.951 1.093 1.227 2.536.82 4.287-.542 2.344-1.615 4.048-3.193 5.07-1.578 1.022-3.584 1.54-5.963 1.54H6.394l-1.318 5.726z"/>
                  <path d="M23.595 8.328c-.548 2.38-1.625 4.116-3.2 5.16-1.576 1.044-3.592 1.573-5.99 1.573h-.83a.77.77 0 0 0-.76.652l-.86 5.44a.641.641 0 0 1-.634.74h-3.43l-.21.916a.641.641 0 0 0 .633.74h3.457a.77.77 0 0 0 .76-.654l.71-4.497h1.173c2.38 0 4.39-.528 5.97-1.573 1.578-1.044 2.65-2.779 3.198-5.159.32-1.387.32-2.548 0-3.483-.002-.008-.005-.015-.007-.022-.072-.23-.163-.45-.274-.66z"/>
                </svg>
                Open PayPal in Browser
              </button>

              <p className="text-gray-500 text-xs text-center mt-4">
                Thank you for your support! 💜
              </p>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
