import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-shell";
import SettingsDialog from "./components/SettingsDialog";
import { AppConfig, PlatformInfo } from "./config";

export interface Settings {
  // Mouse & Cursor
  show_cursor: boolean;
  capture_clicks: boolean;
  click_highlight_color: [number, number, number, number];
  click_dissolve_ms: number;
  click_highlight_radius: number;
  
  // Border
  show_border: boolean;
  border_color: [number, number, number, number];
  border_width: number;
  
  // Performance
  target_fps: number;

  // Capture Method (platform-specific)
  capture_method: "Wgc" | "GdiCopy" | "CoreGraphics";
  
  // Preview Mode (platform-specific)
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
  
  // Window Exclusion
  window_filter: {
    mode: "None" | "Include" | "Exclude";
    excluded_windows: Array<{
      app_id: string;
      window_name: string;
    }>;
    auto_exclude_preview: boolean;
    dev_mode: boolean;
  };
  
  // Logging
  log_level: string;
  log_to_file: boolean;
  log_retention_days: number;
}

export interface MonitorInfo {
  id: number;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  scale_factor: number;
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
  const [platformInfo, setPlatformInfo] = useState<PlatformInfo | null>(null);
  const [isCapturing, setIsCapturing] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [initialSettingsTab, setInitialSettingsTab] = useState<"capture" | "mouse" | "visual" | "region" | "performance" | "exclusion" | "advanced" | "about">("capture");
  const [showDonate, setShowDonate] = useState(false);
  const [showDonateReminder, setShowDonateReminder] = useState(false);
  const [captureRegion, setCaptureRegion] = useState({ x: 0, y: 0, width: 800, height: 600 });
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [selectedMonitor, setSelectedMonitor] = useState<number>(0);
  const [devMode, setDevMode] = useState(false);

  const [profiles, setProfiles] = useState<CaptureProfileInfo[]>([]);
  const [activeProfile, setActiveProfile] = useState<string | null>(null);
  const [activeProfileHints, setActiveProfileHints] = useState<CaptureProfileHints | null>(null);
  const [showProfileInfo, setShowProfileInfo] = useState(false); // Tooltip state

  useEffect(() => {
    initializeApp();
    loadDevMode();
    applyDpiAwareWindowSize();
  }, []);

  // Apply DPI-aware window sizing on startup
  const applyDpiAwareWindowSize = async () => {
    try {
      const [width, height] = await invoke<[number, number]>("get_recommended_window_size");
      const window = getCurrentWindow();
      
      // Only resize if we got valid dimensions
      if (width > 0 && height > 0) {
        await window.setSize({ width, height });
        console.log(`Window resized to ${width}x${height} (DPI-aware)`);
      }
    } catch (error) {
      console.error("Failed to apply DPI-aware window size:", error);
      // Non-critical error, continue without resizing
    }
  };

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
      // First load platform info
      const platform = await invoke<PlatformInfo>("get_platform_info");
      setPlatformInfo(platform);
      console.log("Platform detected:", platform.os_name, platform.os_version);

      // Then load settings
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
    console.log("[DEBUG] handleStartCapture called");
    console.log("[DEBUG] captureRegion:", captureRegion);
    console.log("[DEBUG] captureRegion values:", {
      x: captureRegion.x,
      y: captureRegion.y,
      width: captureRegion.width,
      height: captureRegion.height
    });
    
    try {
      console.log("Starting capture with region:", captureRegion);
      console.log("[DEBUG] About to invoke start_capture...");
      await invoke("start_capture", {
        x: captureRegion.x,
        y: captureRegion.y,
        width: captureRegion.width,
        height: captureRegion.height,
      });
      console.log("[DEBUG] invoke completed successfully");
      setIsCapturing(true);
      console.log("Capture started successfully!");
    } catch (error) {
      console.error("[DEBUG] invoke failed with error:", error);
      console.error("Failed to start capture:", error);
      
      // CRITICAL: Clean up any partially created windows/borders
      try {
        console.log("[DEBUG] Calling cleanup_on_capture_failed...");
        await invoke("cleanup_on_capture_failed");
        console.log("[DEBUG] Cleanup completed");
      } catch (cleanupError) {
        console.error("Cleanup also failed:", cleanupError);
      }
      
      // Show user-friendly error message
      const errorMsg = typeof error === 'string' ? error : String(error);
      let userMessage = "Failed to start capture. ";
      
      // Platform-specific error hints
      if (platformInfo?.os === "macos") {
        if (errorMsg.toLowerCase().includes("permission") || errorMsg.toLowerCase().includes("denied")) {
          userMessage += "Please grant Screen Recording and Accessibility permissions in System Settings > Privacy & Security. Then restart RustFrame.";
        } else {
          userMessage += "Check System Settings > Privacy & Security for Screen Recording permission. See logs for details.";
        }
      } else if (platformInfo?.os === "windows") {
        if (errorMsg.toLowerCase().includes("permission") || errorMsg.toLowerCase().includes("access")) {
          userMessage += "Check Windows permissions or try running as Administrator. See logs for details."; } else {
          userMessage += "Try restarting the application. See logs for details.";
        }
      } else {
        userMessage += "Check system permissions. See logs for details.";
      }
      
      alert(userMessage);
    }
  };

  const handleStopCapture = async () => {
    try {
      // Backend stop_capture now saves last_region and returns updated settings
      const updatedSettings = await invoke<Settings>("stop_capture");
      setIsCapturing(false);
      
      // Update frontend state with backend's saved settings
      setSettings(updatedSettings);
      
      // If last_region was saved, update captureRegion for UI
      if (updatedSettings.last_region) {
        const [x, y, width, height] = updatedSettings.last_region;
        setCaptureRegion({ x, y, width, height });
        console.log("Last region saved:", { x, y, width, height });
      }
      
      // Show donation reminder occasionally (only in release mode)
      if (!devMode) {
        showDonationReminderIfNeeded();
      }
    } catch (error) {
      console.error("Failed to stop capture:", error);
    }
  };
  
  // Smart donation reminder: shows every N capture sessions (configured in AppConfig)
  const showDonationReminderIfNeeded = () => {
    try {
      const storageKey = AppConfig.donate.reminder.storageKey;
      const captureCount = parseInt(localStorage.getItem(storageKey) || '0', 10);
      const newCount = captureCount + 1;
      localStorage.setItem(storageKey, newCount.toString());
      
      // Show reminder at configured interval
      if (newCount % AppConfig.donate.reminder.showInterval === 0) {
        // Small delay so user sees capture stopped first
        setTimeout(() => setShowDonateReminder(true), AppConfig.donate.reminder.delayMs);
      }
    } catch (error) {
      // LocalStorage might not be available, silently fail
      console.log('Could not access localStorage for donation reminder');
    }
  };

  const openSettings = (tab: "capture" | "mouse" | "visual" | "region" | "performance" | "advanced" | "about" = "capture") => {
    setInitialSettingsTab(tab);
    setShowSettings(true);
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
          
           {/* Help Button */}
           <button
            onClick={() => open(AppConfig.links.documentation)}
            className="px-2 py-1 text-sm text-gray-400 hover:text-white hover:bg-gray-700/50 rounded transition-colors flex items-center gap-1"
            title="Help / Documentation"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
            </svg>
          </button>

          <div className="w-px h-4 bg-gray-600"></div>
          <button
            onClick={() => openSettings("capture")}
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
      <div className="flex-1 p-6 overflow-y-auto bg-gradient-to-b from-gray-900 to-gray-950">
        <div className="max-w-5xl mx-auto space-y-6">
          
          {/* HERO SECTION - Capture Control */}
          <div className={`relative overflow-hidden rounded-2xl p-8 border-2 transition-all duration-300 ${
            isCapturing 
              ? 'bg-gradient-to-br from-red-600/20 via-gray-800 to-gray-900 border-red-500/50 shadow-lg shadow-red-500/20' 
              : 'bg-gradient-to-br from-green-600/20 via-gray-800 to-gray-900 border-green-500/30 shadow-lg'
          }`}>
            {/* Background decoration */}
            <div className="absolute top-0 right-0 w-64 h-64 bg-gradient-to-br from-blue-500/10 to-purple-500/10 rounded-full blur-3xl -z-10"></div>
            
            <div className="flex flex-col md:flex-row items-center justify-between gap-6">
              {/* Left side - Status & Info */}
              <div className="flex-1 space-y-4">
                <div className="flex items-center gap-3">
                  <div className={`w-4 h-4 rounded-full animate-pulse ${isCapturing ? 'bg-red-500' : 'bg-green-500'}`}></div>
                  <h2 className="text-2xl font-bold">
                    {isCapturing ? (
                      <span className="bg-gradient-to-r from-red-400 to-red-600 bg-clip-text text-transparent">
                        Recording...
                      </span>
                    ) : (
                      <span className="text-gray-100">Ready to Capture</span>
                    )}
                  </h2>
                </div>
                
                {/* Profile Selection */}
                <div className="flex items-center gap-3 relative">
                  <label className="text-sm font-medium text-gray-400">Profile:</label>
                  <select
                    value={activeProfile ?? ""}
                    onChange={(e) => handleProfileChange(e.target.value)}
                    className="flex-1 max-w-xs h-10 bg-gray-700/50 border border-gray-600 rounded-lg px-4 text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent transition-all"
                    disabled={isCapturing}
                  >
                    <option value="">Default</option>
                    {profiles.map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.id}
                      </option>
                    ))}
                  </select>
                  
                  {/* Info Icon & Tooltip */}
                  <div className="relative">
                    <button 
                      onClick={() => setShowProfileInfo(!showProfileInfo)}
                      onBlur={() => setTimeout(() => setShowProfileInfo(false), 200)}
                      className="text-gray-400 hover:text-blue-400 focus:outline-none transition-colors"
                    >
                      <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                      </svg>
                    </button>
                    
                    {showProfileInfo && (
                      <div className="absolute left-full top-1/2 -translate-y-1/2 ml-3 w-64 p-4 bg-gray-800 border border-gray-700 rounded-xl shadow-2xl z-50 animate-fadeIn text-left">
                        <h4 className="font-bold text-white mb-1">Capture Profiles</h4>
                        <p className="text-sm text-gray-400 leading-relaxed">
                          Profiles optimize capture settings for specific apps (Discord, Zoom, etc). 
                          They handle window sizing and taskbar hiding automatically.
                        </p>
                        <div className="absolute left-0 top-1/2 -translate-x-[5px] -translate-y-1/2 w-2 h-2 bg-gray-800 border-l border-b border-gray-700 rotate-45"></div>
                      </div>
                    )}
                  </div>
                </div>

                {/* Region Summary */}
                <div className="flex items-center gap-2 text-sm text-gray-400">
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 5a1 1 0 011-1h14a1 1 0 011 1v2a1 1 0 01-1 1H5a1 1 0 01-1-1V5zM4 13a1 1 0 011-1h6a1 1 0 011 1v6a1 1 0 01-1 1H5a1 1 0 01-1-1v-6zM16 13a1 1 0 011-1h2a1 1 0 011 1v6a1 1 0 01-1 1h-2a1 1 0 01-1-1v-6z" />
                  </svg>
                  <span>Region: {captureRegion.width} × {captureRegion.height}</span>
                  <span className="text-gray-600">|</span>
                  <button 
                    onClick={() => openSettings("region")}
                    className="text-blue-400 hover:text-blue-300 underline"
                  >
                    Edit
                  </button>
                </div>

                {/* Profile Hint */}
                {activeProfileHints?.hide_taskbar_after_ms != null && !isCapturing && (
                  <div className="text-xs text-gray-500 bg-gray-800/50 rounded-lg p-3 border border-gray-700">
                    💡 This profile auto-hides preview window after {activeProfileHints.hide_taskbar_after_ms}ms
                  </div>
                )}
              </div>

              {/* Right side - Big Action Button */}
              <div className="flex flex-col items-center gap-3">
                <button
                  onClick={isCapturing ? handleStopCapture : handleStartCapture}
                  className={`group relative px-12 py-6 rounded-2xl font-bold text-lg transition-all duration-300 transform hover:scale-105 active:scale-95 shadow-xl ${
                    isCapturing
                      ? "bg-gradient-to-br from-red-700 to-red-800 hover:from-red-600 hover:to-red-700 text-white shadow-red-500/30"
                      : "bg-gradient-to-br from-green-700 to-green-800 hover:from-green-600 hover:to-green-700 text-white shadow-green-500/30"
                  }`}
                >
                  <div className="flex items-center gap-3 relative z-10">
                    {isCapturing ? (
                      <>
                        <svg className="w-6 h-6" fill="currentColor" viewBox="0 0 24 24">
                          <rect x="6" y="4" width="4" height="16" rx="1"/>
                          <rect x="14" y="4" width="4" height="16" rx="1"/>
                        </svg>
                        <span>STOP</span>
                      </>
                    ) : (
                      <>
                        <svg className="w-6 h-6" fill="currentColor" viewBox="0 0 24 24">
                          <circle cx="12" cy="12" r="10"/>
                        </svg>
                        <span>START CAPTURE</span>
                      </>
                    )}
                  </div>
                  {/* Glow effect */}
                  <div className={`absolute inset-0 rounded-2xl blur-xl transition-opacity duration-300 ${
                    isCapturing 
                      ? 'bg-red-500 opacity-5 group-hover:opacity-10' 
                      : 'bg-green-500 opacity-10 group-hover:opacity-20'
                  }`}></div>
                </button>
                
                {isCapturing && (
                  <span className="text-xs text-gray-400 animate-pulse">Press to stop recording</span>
                )}
              </div>
            </div>
          </div>

          {/* QUICK SETTINGS CARDS */}
          <div className="bg-gray-800/50 rounded-xl p-6 border border-gray-700">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-lg font-semibold text-gray-200">Quick Settings</h3>
              <button
                onClick={() => openSettings("capture")}
                className="text-sm text-blue-400 hover:text-blue-300 flex items-center gap-1 transition-colors"
              >
                <span>View All</span>
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                </svg>
              </button>
            </div>
            
            <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-4">
              {/* Cursor Card */}
              <div 
                onClick={() => openSettings("mouse")}
                className={`group relative bg-gradient-to-br rounded-xl p-4 border-2 transition-all duration-300 cursor-pointer hover:scale-105 ${
                settings.show_cursor 
                  ? 'from-green-500/20 to-gray-800 border-green-500/50 shadow-lg shadow-green-500/20' 
                  : 'from-gray-700/20 to-gray-800 border-gray-600 hover:border-gray-500'
              }`}>
                <div className="flex flex-col items-center text-center space-y-2">
                  <div className={`w-12 h-12 rounded-full flex items-center justify-center transition-all ${
                    settings.show_cursor ? 'bg-green-500/30 text-green-400' : 'bg-gray-600/30 text-gray-400'
                  }`}>
                    <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 15l-2 5L9 9l11 4-5 2zm0 0l5 5M7.188 2.239l.777 2.897M5.136 7.965l-2.898-.777M13.95 4.05l-2.122 2.122m-5.657 5.656l-2.12 2.122" />
                    </svg>
                  </div>
                  <div>
                    <div className="text-xs font-medium text-gray-300">Cursor</div>
                    <div className={`text-sm font-bold ${settings.show_cursor ? 'text-green-400' : 'text-gray-500'}`}>
                      {settings.show_cursor ? "ON" : "OFF"}
                    </div>
                  </div>
                </div>
              </div>

              {/* Click Highlight Card */}
              <div 
                onClick={() => openSettings("mouse")}
                className={`group relative bg-gradient-to-br rounded-xl p-4 border-2 transition-all duration-300 cursor-pointer hover:scale-105 ${
                settings.capture_clicks 
                  ? 'from-green-500/20 to-gray-800 border-green-500/50 shadow-lg shadow-green-500/20' 
                  : 'from-gray-700/20 to-gray-800 border-gray-600 hover:border-gray-500'
              }`}>
                <div className="flex flex-col items-center text-center space-y-2">
                  <div className={`w-12 h-12 rounded-full flex items-center justify-center transition-all ${
                    settings.capture_clicks ? 'bg-green-500/30 text-green-400' : 'bg-gray-600/30 text-gray-400'
                  }`}>
                    <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                    </svg>
                  </div>
                  <div>
                    <div className="text-xs font-medium text-gray-300">Clicks</div>
                    <div className={`text-sm font-bold ${settings.capture_clicks ? 'text-green-400' : 'text-gray-500'}`}>
                      {settings.capture_clicks ? "ON" : "OFF"}
                    </div>
                  </div>
                </div>
              </div>

              {/* FPS Card */}
              <div 
                onClick={() => openSettings("performance")}
                className="group relative bg-gradient-to-br from-blue-500/20 to-gray-800 rounded-xl p-4 border-2 border-blue-500/50 transition-all duration-300 cursor-pointer hover:scale-105 shadow-lg shadow-blue-500/20">
                <div className="flex flex-col items-center text-center space-y-2">
                  <div className="w-12 h-12 rounded-full flex items-center justify-center bg-blue-500/30 text-blue-400">
                    <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
                    </svg>
                  </div>
                  <div>
                    <div className="text-xs font-medium text-gray-300">FPS</div>
                    <div className="text-sm font-bold text-blue-400">{settings.target_fps}</div>
                  </div>
                </div>
              </div>

              {/* Border Card */}
              <div 
                onClick={() => openSettings("visual")}
                className={`group relative bg-gradient-to-br rounded-xl p-4 border-2 transition-all duration-300 cursor-pointer hover:scale-105 ${
                settings.show_border 
                  ? 'from-purple-500/20 to-gray-800 border-purple-500/50 shadow-lg shadow-purple-500/20' 
                  : 'from-gray-700/20 to-gray-800 border-gray-600 hover:border-gray-500'
              }`}>
                <div className="flex flex-col items-center text-center space-y-2">
                  <div className={`w-12 h-12 rounded-full flex items-center justify-center transition-all ${
                    settings.show_border ? 'bg-purple-500/30 text-purple-400' : 'bg-gray-600/30 text-gray-400'
                  }`}>
                    <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 5a1 1 0 011-1h14a1 1 0 011 1v2a1 1 0 01-1 1H5a1 1 0 01-1-1V5zM4 13a1 1 0 011-1h6a1 1 0 011 1v6a1 1 0 01-1 1H5a1 1 0 01-1-1v-6zM16 13a1 1 0 011-1h2a1 1 0 011 1v6a1 1 0 01-1 1h-2a1 1 0 01-1-1v-6z" />
                    </svg>
                  </div>
                  <div>
                    <div className="text-xs font-medium text-gray-300">Border</div>
                    <div className={`text-sm font-bold ${settings.show_border ? 'text-purple-400' : 'text-gray-500'}`}>
                      {settings.show_border ? `${settings.border_width}px` : "OFF"}
                    </div>
                  </div>
                </div>
              </div>

              {/* REC Indicator Card */}
              <div 
                onClick={() => openSettings("visual")}
                className={`group relative bg-gradient-to-br rounded-xl p-4 border-2 transition-all duration-300 cursor-pointer hover:scale-105 ${
                settings.show_rec_indicator 
                  ? 'from-red-500/20 to-gray-800 border-red-500/50 shadow-lg shadow-red-500/20' 
                  : 'from-gray-700/20 to-gray-800 border-gray-600 hover:border-gray-500'
              }`}>
                <div className="flex flex-col items-center text-center space-y-2">
                  <div className={`w-12 h-12 rounded-full flex items-center justify-center transition-all ${
                    settings.show_rec_indicator ? 'bg-red-500/30 text-red-400' : 'bg-gray-600/30 text-gray-400'
                  }`}>
                    <svg className="w-6 h-6" fill="currentColor" viewBox="0 0 24 24">
                      <circle cx="12" cy="12" r="8"/>
                    </svg>
                  </div>
                  <div>
                    <div className="text-xs font-medium text-gray-300">REC</div>
                    <div className={`text-sm font-bold ${settings.show_rec_indicator ? 'text-red-400' : 'text-gray-500'}`}>
                      {settings.show_rec_indicator ? settings.rec_indicator_size.toUpperCase() : "OFF"}
                    </div>
                  </div>
                </div>
              </div>

              {/* Region Memory Card */}
              <div 
                onClick={() => openSettings("advanced")}
                className={`group relative bg-gradient-to-br rounded-xl p-4 border-2 transition-all duration-300 cursor-pointer hover:scale-105 ${
                settings.remember_last_region 
                  ? 'from-yellow-500/20 to-gray-800 border-yellow-500/50 shadow-lg shadow-yellow-500/20' 
                  : 'from-gray-700/20 to-gray-800 border-gray-600 hover:border-gray-500'
              }`}>
                <div className="flex flex-col items-center text-center space-y-2">
                  <div className={`w-12 h-12 rounded-full flex items-center justify-center transition-all ${
                    settings.remember_last_region ? 'bg-yellow-500/30 text-yellow-400' : 'bg-gray-600/30 text-gray-400'
                  }`}>
                    <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 5a2 2 0 012-2h10a2 2 0 012 2v16l-7-3.5L5 21V5z" />
                    </svg>
                  </div>
                  <div>
                    <div className="text-xs font-medium text-gray-300">Auto-Restore</div>
                    <div className={`text-sm font-bold ${settings.remember_last_region ? 'text-yellow-400' : 'text-gray-500'}`}>
                      {settings.remember_last_region ? "ON" : "OFF"}
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>

          {/* OLD SECTIONS - Will be removed/redesigned in next steps */}
          {/* Current Settings Preview - More Prominent */}
          <div className="hidden">
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

          {/* MONITOR & REGION VISUALIZATION */}
          <div className="bg-gray-800/50 rounded-xl p-6 border border-gray-700">
            <div className="flex items-center justify-between mb-6">
              <h3 className="text-lg font-semibold text-gray-200 flex items-center gap-2">
                <svg className="w-5 h-5 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                </svg>
                Monitor & Region Setup
              </h3>
              <button
                onClick={() => setShowSettings(true)}
                className="text-sm text-blue-400 hover:text-blue-300 flex items-center gap-1 transition-colors"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                </svg>
                <span>Configure in Settings</span>
              </button>
            </div>

            {/* Monitor Selection */}
            {monitors.length > 1 && (
              <div className="mb-6">
                <label className="text-sm font-medium text-gray-400 mb-2 block">Select Monitor:</label>
                <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                  {monitors.map((mon, idx) => (
                    <button
                      key={mon.id}
                      onClick={() => setSelectedMonitor(idx)}
                      className={`p-4 rounded-lg border-2 transition-all duration-300 text-left ${
                        selectedMonitor === idx
                          ? 'bg-blue-500/20 border-blue-500 shadow-lg shadow-blue-500/20'
                          : 'bg-gray-700/30 border-gray-600 hover:border-gray-500 hover:bg-gray-700/50'
                      }`}
                    >
                      <div className="flex items-center justify-between mb-2">
                        <div className="flex items-center gap-2">
                          <svg className="w-5 h-5 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                          </svg>
                          <span className="font-medium text-white">{mon.name}</span>
                        </div>
                        {mon.is_primary && (
                          <span className="text-xs bg-green-500/20 text-green-400 px-2 py-1 rounded-full">Primary</span>
                        )}
                      </div>
                      <div className="text-sm text-gray-400">
                        {mon.width} × {mon.height} @ {mon.refresh_rate}Hz
                      </div>
                    </button>
                  ))}
                </div>
              </div>
            )}

            {/* Region Info */}
            <div className="bg-gray-700/30 rounded-lg p-4 border border-gray-600">
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                <div className="text-center">
                  <div className="text-xs text-gray-400 mb-1">Position</div>
                  <div className="text-sm font-medium text-white">{captureRegion.x}, {captureRegion.y}</div>
                </div>
                <div className="text-center">
                  <div className="text-xs text-gray-400 mb-1">Size</div>
                  <div className="text-sm font-medium text-white">{captureRegion.width} × {captureRegion.height}</div>
                </div>
                <div className="text-center">
                  <div className="text-xs text-gray-400 mb-1">Aspect Ratio</div>
                  <div className="text-sm font-medium text-white">
                    {(captureRegion.width / captureRegion.height).toFixed(2)}:1
                  </div>
                </div>
                <div className="text-center">
                  <div className="text-xs text-gray-400 mb-1">Monitor</div>
                  <div className="text-sm font-medium text-white truncate">
                    {monitors[selectedMonitor]?.name || "N/A"}
                  </div>
                </div>
              </div>

              {/* Quick Tip */}
              <div className="mt-4 p-3 bg-blue-500/10 border border-blue-500/30 rounded-lg">
                <div className="flex items-start gap-2">
                  <svg className="w-5 h-5 text-blue-400 flex-shrink-0 mt-0.5" fill="currentColor" viewBox="0 0 20 20">
                    <path fillRule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7-4a1 1 0 11-2 0 1 1 0 012 0zM9 9a1 1 0 000 2v3a1 1 0 001 1h1a1 1 0 100-2v-3a1 1 0 00-1-1H9z" clipRule="evenodd" />
                  </svg>
                  <div className="text-xs text-blue-300">
                    <strong>Tip:</strong> The hollow border will appear at this position when you start capturing. 
                    Adjust size and position in Settings → Capture Region for pixel-perfect positioning.
                  </div>
                </div>
              </div>
            </div>
          </div>

          {/* ACTIVE CONFIGURATION SUMMARY */}
          <div className="bg-gray-800/50 rounded-xl p-6 border border-gray-700">
            <h3 className="text-lg font-semibold text-gray-200 mb-4">Active Configuration</h3>
            <div className="grid grid-cols-2 md:grid-cols-3 gap-4 text-sm">
              <div className="flex items-center justify-between p-3 bg-gray-700/50 rounded-lg">
                <span className="text-gray-400">Capture Method:</span>
                <span className="text-white font-medium">{settings.capture_method}</span>
              </div>
              <div className="flex items-center justify-between p-3 bg-gray-700/50 rounded-lg">
                <span className="text-gray-400">Preview Mode:</span>
                <span className="text-white font-medium">{settings.preview_mode}</span>
              </div>
              <div className="flex items-center justify-between p-3 bg-gray-700/50 rounded-lg">
                <span className="text-gray-400">Profile:</span>
                <span className="text-white font-medium">{activeProfile || "Default"}</span>
              </div>
            </div>
          </div>
          </div>
        </div>
      </div>

      {/* Settings Dialog */}
      {showSettings && platformInfo && (
        <SettingsDialog
          initialTab={initialSettingsTab}
          settings={settings}
          platformInfo={platformInfo}
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

      {/* Donation Reminder Modal - Gentle version shown after captures */}
      {showDonateReminder && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 animate-fadeIn" onClick={() => setShowDonateReminder(false)}>
          <div 
            className="bg-gradient-to-br from-gray-800 to-gray-900 rounded-2xl overflow-hidden max-w-sm w-full mx-4 border border-gray-600 shadow-2xl animate-slideUp"
            onClick={(e) => e.stopPropagation()}
          >
            {/* Content */}
            <div className="p-6 text-center">
              {/* Icon */}
              <div className="flex justify-center mb-4">
                <div className="w-16 h-16 bg-gradient-to-br from-pink-500 to-purple-600 rounded-full flex items-center justify-center animate-pulse">
                  <svg className="w-8 h-8 text-white" fill="currentColor" viewBox="0 0 24 24">
                    <path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/>
                  </svg>
                </div>
              </div>

              <h3 className="text-xl font-bold text-white mb-2">
                Enjoying RustFrame? ✨
              </h3>
              
              <p className="text-gray-300 text-sm mb-6 leading-relaxed">
                We noticed you've been using RustFrame quite a bit! 🎉
                <br />
                <span className="text-gray-400 text-xs mt-2 block">
                  If it's been helpful, a small donation would mean the world and help keep this project alive!
                </span>
              </p>

              {/* Action Buttons */}
              <div className="space-y-2">
                <button
                  onClick={() => { setShowDonateReminder(false); setShowDonate(true); }}
                  className="w-full py-3 bg-gradient-to-r from-pink-500 to-purple-600 hover:from-pink-600 hover:to-purple-700 text-white font-semibold rounded-lg transition-all shadow-lg hover:shadow-xl flex items-center justify-center gap-2"
                >
                  <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 24 24">
                    <path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/>
                  </svg>
                  Yes, I'd love to help! 💝
                </button>
                
                <button
                  onClick={() => setShowDonateReminder(false)}
                  className="w-full py-2.5 text-gray-400 hover:text-white text-sm font-medium transition-colors"
                >
                  Maybe later
                </button>
              </div>

              <p className="text-gray-500 text-xs mt-4">
                This message appears occasionally. Thank you for understanding! 🙏
              </p>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export default App;
