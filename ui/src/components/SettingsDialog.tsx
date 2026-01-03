import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Settings, MonitorInfo } from "../App";

interface CaptureRegion {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface SettingsDialogProps {
  settings: Settings;
  captureRegion: CaptureRegion;
  monitors: MonitorInfo[];
  selectedMonitor: number;
  onSave: (settings: Settings) => void;
  onRegionChange: (region: CaptureRegion) => void;
  onMonitorChange: (index: number) => void;
  onClose: () => void;
}

type TabType = "general" | "region" | "capture" | "advanced";

function SettingsDialog({ 
  settings, 
  captureRegion, 
  monitors, 
  selectedMonitor, 
  onSave, 
  onRegionChange, 
  onMonitorChange, 
  onClose 
}: SettingsDialogProps) {
  const [activeTab, setActiveTab] = useState<TabType>("general");
  const [localSettings, setLocalSettings] = useState<Settings>(settings);
  const [localRegion, setLocalRegion] = useState<CaptureRegion>(captureRegion);
  const [localMonitor, setLocalMonitor] = useState<number>(selectedMonitor);
  const [previewEnabled, setPreviewEnabled] = useState(false);
  const [positionPreset, setPositionPreset] = useState<string>("center");
  const [isSyncingFromBackend, setIsSyncingFromBackend] = useState(false); // Flag to prevent update loops

  // Get max FPS based on selected monitor
  // Round to nearest standard refresh rate (handles 119.9Hz -> 120Hz)
  const roundToStandardRefreshRate = (rate: number): number => {
    const standardRates = [24, 25, 30, 50, 60, 75, 90, 120, 144, 165, 240, 360];
    // Allow 10% tolerance for matching
    for (const standard of standardRates) {
      if (Math.abs(rate - standard) <= standard * 0.1) {
        return standard;
      }
    }
    return Math.round(rate);
  };
  
  const monitorRefreshRate = monitors[localMonitor] 
    ? roundToStandardRefreshRate(monitors[localMonitor].refresh_rate)
    : 60;
  const maxFps = monitorRefreshRate * 2; // Allow up to 2x refresh rate for high-fps capture

  // Prevent background scroll when modal is open
  useEffect(() => {
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = "unset";
    };
  }, []);

  // Preview border management
  useEffect(() => {
    if (previewEnabled) {
      // COLORREF format is 0x00BBGGRR
      const borderColor = (localSettings.border_color[0]) 
        | (localSettings.border_color[1] << 8) 
        | (localSettings.border_color[2] << 16);
      
      invoke("show_preview_border", {
        x: localRegion.x,
        y: localRegion.y,
        width: localRegion.width,
        height: localRegion.height,
        borderWidth: localSettings.border_width,
        borderColor: borderColor
      }).catch(console.error);
    } else {
      invoke("hide_preview_border").catch(console.error);
    }
    
    return () => {
      invoke("hide_preview_border").catch(console.error);
    };
  }, [previewEnabled]);

  // Update preview border when region changes (only from UI, not from sync)
  // Use a ref to track last sent values to avoid redundant updates
  const lastSentRegion = useRef({ x: 0, y: 0, width: 0, height: 0 });
  
  useEffect(() => {
    if (previewEnabled && !isSyncingFromBackend) {
      // Only send update if values actually changed from what we last sent
      if (localRegion.x !== lastSentRegion.current.x ||
          localRegion.y !== lastSentRegion.current.y ||
          localRegion.width !== lastSentRegion.current.width ||
          localRegion.height !== lastSentRegion.current.height) {
        
        lastSentRegion.current = { ...localRegion };
        invoke("update_preview_border", {
          x: localRegion.x,
          y: localRegion.y,
          width: localRegion.width,
          height: localRegion.height
        }).catch(console.error);
      }
    }
  }, [localRegion, previewEnabled, isSyncingFromBackend]);

  // Update preview border when border settings change (color/width only - no recreate)
  useEffect(() => {
    if (previewEnabled) {
      // COLORREF format is 0x00BBGGRR
      const borderColor = (localSettings.border_color[0]) 
        | (localSettings.border_color[1] << 8) 
        | (localSettings.border_color[2] << 16);
      
      invoke("update_preview_border_style", {
        borderWidth: localSettings.border_width,
        borderColor: borderColor
      }).catch(console.error);
    }
  }, [localSettings.border_color, localSettings.border_width, previewEnabled]);

  // Sync border changes back to settings when user drags/resizes the preview border
  useEffect(() => {
    if (!previewEnabled) return;

    // Use a ref to track the last known rect to avoid dependency issues
    let lastKnownRect = { x: localRegion.x, y: localRegion.y, width: localRegion.width, height: localRegion.height };

    const syncInterval = setInterval(async () => {
      try {
        const rect = await invoke<[number, number, number, number] | null>("get_preview_border_rect");
        if (rect) {
          const [x, y, width, height] = rect;
          
          // Check if the rect actually changed from what we know
          if (x !== lastKnownRect.x || y !== lastKnownRect.y || 
              width !== lastKnownRect.width || height !== lastKnownRect.height) {
            
            // Update our tracking
            lastKnownRect = { x, y, width, height };
            
            // Also update lastSentRegion to prevent echo back
            lastSentRegion.current = { x, y, width, height };
            
            // Set flag to prevent update_preview_border from being called
            setIsSyncingFromBackend(true);
            setLocalRegion({ x, y, width, height });
            setPositionPreset("custom"); // Switch to custom since user manually moved it
            
            // Reset flag after state update is processed
            setTimeout(() => setIsSyncingFromBackend(false), 100);
            
            // Check if border moved to a different monitor
            for (let i = 0; i < monitors.length; i++) {
              const mon = monitors[i];
              // Check if center of border is within this monitor
              const centerX = x + width / 2;
              const centerY = y + height / 2;
              
              if (centerX >= mon.x && centerX < mon.x + mon.width &&
                  centerY >= mon.y && centerY < mon.y + mon.height) {
                setLocalMonitor(i);
                break;
              }
            }
          }
        }
      } catch (e) {
        // Ignore errors during polling
      }
    }, 300); // Poll every 300ms (slightly slower to reduce contention)

    return () => clearInterval(syncInterval);
  }, [previewEnabled, monitors]); // Remove localRegion from dependencies

  const handleSave = () => {
    onSave(localSettings);
    onRegionChange(localRegion);
    onMonitorChange(localMonitor);
  };

  const handleExportSettings = async () => {
    try {
      const filePath = await save({
        defaultPath: "rustframe-settings.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (filePath) {
        await invoke("export_settings", { path: filePath });
      }
    } catch (error) {
      console.error("Failed to export settings:", error);
    }
  };

  const handleImportSettings = async () => {
    try {
      const filePath = await open({
        filters: [{ name: "JSON", extensions: ["json"] }],
        multiple: false,
      });
      if (filePath) {
        const imported = await invoke<Settings>("import_settings", { path: filePath });
        setLocalSettings(imported);
        onSave(imported); // Also update parent
      }
    } catch (error) {
      console.error("Failed to import settings:", error);
    }
  };

  const handleOpenSettingsFolder = async () => {
    try {
      await invoke("open_settings_folder");
    } catch (error) {
      console.error("Failed to open settings folder:", error);
    }
  };

  const rgbaToHex = (rgba: [number, number, number, number]): string => {
    return `#${rgba[0].toString(16).padStart(2, '0')}${rgba[1].toString(16).padStart(2, '0')}${rgba[2].toString(16).padStart(2, '0')}`;
  };

  const hexToRgba = (hex: string): [number, number, number, number] => {
    const r = parseInt(hex.slice(1, 3), 16);
    const g = parseInt(hex.slice(3, 5), 16);
    const b = parseInt(hex.slice(5, 7), 16);
    return [r, g, b, 255];
  };

  const tabs: { id: TabType; label: string }[] = [
    { id: "general", label: "General" },
    { id: "region", label: "Capture Region" },
    { id: "capture", label: "Capture" },
    { id: "advanced", label: "Advanced" },
  ];

  return (
    <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
      <div className="bg-gray-800 rounded-lg shadow-xl w-full max-w-3xl max-h-[80vh] flex flex-col">
        {/* Header */}
        <div className="px-6 py-4 border-b border-gray-700 flex items-center justify-between">
          <h2 className="text-2xl font-bold">Settings</h2>
          <button
            onClick={onClose}
            className="text-gray-400 hover:text-white transition-colors"
          >
            <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Tabs */}
        <div className="px-6 pt-4 border-b border-gray-700">
          <div className="flex space-x-1">
            {tabs.map((tab) => (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`px-4 py-2 rounded-t-lg transition-colors ${
                  activeTab === tab.id
                    ? "bg-gray-700 text-white"
                    : "text-gray-400 hover:text-white hover:bg-gray-750"
                }`}
              >
                {tab.label}
              </button>
            ))}
          </div>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-6">
          {activeTab === "general" && (
            <div className="space-y-6">
              <div>
                <h3 className="text-lg font-semibold mb-4">Display Options</h3>
                <div className="space-y-4">
                  <label className="flex items-center space-x-3 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={localSettings.show_cursor}
                      onChange={(e) =>
                        setLocalSettings({ ...localSettings, show_cursor: e.target.checked })
                      }
                      className="w-5 h-5 rounded bg-gray-700 border-gray-600 text-blue-600 focus:ring-blue-500"
                    />
                    <span>Show Cursor</span>
                  </label>
                  <label className="flex items-center space-x-3 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={localSettings.capture_clicks}
                      onChange={(e) =>
                        setLocalSettings({ ...localSettings, capture_clicks: e.target.checked })
                      }
                      className="w-5 h-5 rounded bg-gray-700 border-gray-600 text-blue-600 focus:ring-blue-500"
                    />
                    <span>Capture Clicks</span>
                  </label>
                </div>
              </div>

              <div>
                <h3 className="text-lg font-semibold mb-4">Click Highlight</h3>
                <div className="space-y-4">
                  <div className="flex items-center space-x-4">
                    <span className="text-sm text-gray-400">Color:</span>
                    <input
                      type="color"
                      value={rgbaToHex(localSettings.click_highlight_color)}
                      onChange={(e) =>
                        setLocalSettings({
                          ...localSettings,
                          click_highlight_color: hexToRgba(e.target.value),
                        })
                      }
                      className="w-12 h-8 rounded cursor-pointer"
                      disabled={!localSettings.capture_clicks}
                    />
                    <span className="text-gray-400 text-sm">
                      RGB({localSettings.click_highlight_color[0]}, {localSettings.click_highlight_color[1]}, {localSettings.click_highlight_color[2]})
                    </span>
                  </div>
                  <div>
                    <label className="block text-sm text-gray-400 mb-2">
                      Dissolve Duration: {localSettings.click_dissolve_ms}ms
                    </label>
                    <input
                      type="range"
                      min="100"
                      max="1000"
                      step="50"
                      value={localSettings.click_dissolve_ms}
                      onChange={(e) =>
                        setLocalSettings({
                          ...localSettings,
                          click_dissolve_ms: parseInt(e.target.value),
                        })
                      }
                      className="w-full h-2 bg-gray-700 rounded-lg appearance-none cursor-pointer"
                      disabled={!localSettings.capture_clicks}
                    />
                  </div>
                </div>
              </div>

              <div>
                <h3 className="text-lg font-semibold mb-4">Border Settings</h3>
                <div className="space-y-4">
                  <label className="flex items-center space-x-3 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={localSettings.show_border}
                      onChange={(e) =>
                        setLocalSettings({ ...localSettings, show_border: e.target.checked })
                      }
                      className="w-5 h-5 rounded bg-gray-700 border-gray-600 text-blue-600 focus:ring-blue-500"
                    />
                    <div>
                      <span>Show Capture Border</span>
                      <p className="text-sm text-gray-400">Display the hollow border around capture region</p>
                    </div>
                  </label>
                  <div className="flex items-center space-x-4 pl-8">
                    <span className="text-sm text-gray-400">Color:</span>
                    <input
                      type="color"
                      value={rgbaToHex(localSettings.border_color)}
                      onChange={(e) =>
                        setLocalSettings({
                          ...localSettings,
                          border_color: hexToRgba(e.target.value),
                        })
                      }
                      className="w-12 h-8 rounded cursor-pointer"
                      disabled={!localSettings.show_border}
                    />
                    <span className="text-gray-400 text-sm">
                      RGB({localSettings.border_color[0]}, {localSettings.border_color[1]}, {localSettings.border_color[2]})
                    </span>
                  </div>
                  <div className="flex items-center space-x-4 pl-8">
                    <span className="text-sm text-gray-400 w-16">Width:</span>
                    <input
                      type="range"
                      min="1"
                      max="20"
                      value={localSettings.border_width}
                      onChange={(e) =>
                        setLocalSettings({
                          ...localSettings,
                          border_width: parseInt(e.target.value),
                        })
                      }
                      className="flex-1 h-2 bg-gray-600 rounded-lg appearance-none cursor-pointer"
                      disabled={!localSettings.show_border}
                    />
                    <span className="text-gray-400 text-sm w-12 text-right">
                      {localSettings.border_width}px
                    </span>
                  </div>
                </div>
              </div>

              {/* REC Indicator */}
              <div>
                <h3 className="text-lg font-semibold mb-4">REC Indicator</h3>
                <div className="space-y-4">
                  <label className="flex items-center space-x-3 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={localSettings.show_rec_indicator}
                      onChange={(e) =>
                        setLocalSettings({ ...localSettings, show_rec_indicator: e.target.checked })
                      }
                      className="w-5 h-5 rounded bg-gray-700 border-gray-600 text-blue-600 focus:ring-blue-500"
                    />
                    <div>
                      <span>Show REC Indicator</span>
                      <p className="text-sm text-gray-400">Display a "● REC" indicator in capture region during recording</p>
                    </div>
                  </label>
                  <div className="flex items-center space-x-4 pl-8">
                    <span className="text-sm text-gray-400">Size:</span>
                    <select
                      value={localSettings.rec_indicator_size}
                      onChange={(e) =>
                        setLocalSettings({
                          ...localSettings,
                          rec_indicator_size: e.target.value as "small" | "medium" | "large",
                        })
                      }
                      className="bg-gray-700 border border-gray-600 rounded px-3 py-1 focus:ring-blue-500 focus:border-blue-500"
                      disabled={!localSettings.show_rec_indicator}
                    >
                      <option value="small">Small</option>
                      <option value="medium">Medium</option>
                      <option value="large">Large</option>
                    </select>
                  </div>
                </div>
              </div>

              {/* Remember Last Region */}
              <div>
                <h3 className="text-lg font-semibold mb-4">Region Memory</h3>
                <label className="flex items-center space-x-3 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={localSettings.remember_last_region}
                    onChange={(e) =>
                      setLocalSettings({ ...localSettings, remember_last_region: e.target.checked })
                    }
                    className="w-5 h-5 rounded bg-gray-700 border-gray-600 text-blue-600 focus:ring-blue-500"
                  />
                  <div>
                    <span>Remember Last Capture Region</span>
                    <p className="text-sm text-gray-400">Restore position and size on next app start</p>
                  </div>
                </label>
              </div>
            </div>
          )}

          {activeTab === "region" && (
            <div className="space-y-6">
              {/* Enable Preview */}
              <div className="bg-gray-700 p-4 rounded-lg">
                <label className="flex items-center space-x-3 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={previewEnabled}
                    onChange={(e) => setPreviewEnabled(e.target.checked)}
                    className="w-5 h-5 rounded bg-gray-600 border-gray-500 text-blue-600 focus:ring-blue-500"
                  />
                  <div>
                    <span className="font-medium">Enable Preview</span>
                    <p className="text-sm text-gray-400">Show capture border on screen while adjusting</p>
                  </div>
                </label>
              </div>

              {/* Monitor Selection */}
              <div>
                <h3 className="text-lg font-semibold mb-4">Monitor</h3>
                <select
                  value={localMonitor}
                  onChange={(e) => {
                    const idx = parseInt(e.target.value);
                    setLocalMonitor(idx);
                    // Center region on new monitor
                    if (monitors[idx]) {
                      const mon = monitors[idx];
                      setLocalRegion({
                        x: mon.x + Math.floor((mon.width - localRegion.width) / 2),
                        y: mon.y + Math.floor((mon.height - localRegion.height) / 2),
                        width: localRegion.width,
                        height: localRegion.height
                      });
                    }
                  }}
                  className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:border-blue-500"
                >
                  {monitors.map((mon, idx) => (
                    <option key={mon.id} value={idx}>
                      {mon.name} ({mon.width}x{mon.height} @ {mon.refresh_rate}Hz){mon.is_primary ? " - Primary" : ""}
                    </option>
                  ))}
                </select>
              </div>

              {/* Region Size */}
              <div>
                <h3 className="text-lg font-semibold mb-4">Region Size</h3>
                <div className="grid grid-cols-2 gap-4 mb-4">
                  <div>
                    <label className="block text-sm text-gray-400 mb-2">Width</label>
                    <input
                      type="number"
                      value={localRegion.width}
                      onChange={(e) =>
                        setLocalRegion({
                          ...localRegion,
                          width: parseInt(e.target.value) || 800,
                        })
                      }
                      className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:border-blue-500"
                    />
                  </div>
                  <div>
                    <label className="block text-sm text-gray-400 mb-2">Height</label>
                    <input
                      type="number"
                      value={localRegion.height}
                      onChange={(e) =>
                        setLocalRegion({
                          ...localRegion,
                          height: parseInt(e.target.value) || 600,
                        })
                      }
                      className="w-full px-3 py-2 bg-gray-700 border border-gray-600 rounded-lg focus:outline-none focus:border-blue-500"
                    />
                  </div>
                </div>
                {/* Size Presets */}
                <div className="grid grid-cols-3 gap-2">
                  {[
                    { label: "720p", w: 1280, h: 720 },
                    { label: "1080p", w: 1920, h: 1080 },
                    { label: "1440p", w: 2560, h: 1440 },
                    { label: "4K", w: 3840, h: 2160 },
                    { label: "Square", w: 1080, h: 1080 },
                    { label: "800x600", w: 800, h: 600 },
                  ].map((preset) => (
                    <button
                      key={preset.label}
                      onClick={() => {
                        const mon = monitors[localMonitor];
                        if (mon) {
                          setLocalRegion({
                            x: mon.x + Math.floor((mon.width - preset.w) / 2),
                            y: mon.y + Math.floor((mon.height - preset.h) / 2),
                            width: preset.w,
                            height: preset.h,
                          });
                        } else {
                          setLocalRegion({ ...localRegion, width: preset.w, height: preset.h });
                        }
                      }}
                      className={`px-3 py-2 rounded-lg transition-colors ${
                        localRegion.width === preset.w && localRegion.height === preset.h
                          ? "bg-blue-600 text-white"
                          : "bg-gray-700 hover:bg-gray-600"
                      }`}
                    >
                      {preset.label}
                    </button>
                  ))}
                </div>
              </div>

              {/* Region Position */}
              <div>
                <h3 className="text-lg font-semibold mb-4">Region Position</h3>
                {/* Position Presets */}
                <div className="grid grid-cols-3 gap-2 mb-4">
                  {[
                    { id: "center", label: "Center" },
                    { id: "top-left", label: "Top-Left" },
                    { id: "top-right", label: "Top-Right" },
                    { id: "bottom-left", label: "Bottom-Left" },
                    { id: "bottom-right", label: "Bottom-Right" },
                    { id: "custom", label: "Custom" },
                  ].map((preset) => (
                    <button
                      key={preset.id}
                      onClick={() => {
                        setPositionPreset(preset.id);
                        if (preset.id !== "custom") {
                          const mon = monitors[localMonitor];
                          if (mon) {
                            let newPos = { x: localRegion.x, y: localRegion.y };
                            switch (preset.id) {
                              case "center":
                                newPos = { 
                                  x: mon.x + Math.floor((mon.width - localRegion.width) / 2), 
                                  y: mon.y + Math.floor((mon.height - localRegion.height) / 2) 
                                };
                                break;
                              case "top-left":
                                newPos = { x: mon.x, y: mon.y };
                                break;
                              case "top-right":
                                newPos = { x: mon.x + mon.width - localRegion.width, y: mon.y };
                                break;
                              case "bottom-left":
                                newPos = { x: mon.x, y: mon.y + mon.height - localRegion.height };
                                break;
                              case "bottom-right":
                                newPos = { x: mon.x + mon.width - localRegion.width, y: mon.y + mon.height - localRegion.height };
                                break;
                            }
                            setLocalRegion({ ...localRegion, ...newPos });
                          }
                        }
                      }}
                      className={`px-3 py-2 rounded-lg transition-colors ${
                        positionPreset === preset.id
                          ? "bg-blue-600 text-white"
                          : "bg-gray-700 hover:bg-gray-600"
                      }`}
                    >
                      {preset.label}
                    </button>
                  ))}
                </div>
                
                {/* Custom X/Y inputs - only visible when Custom is selected */}
                {positionPreset === "custom" && (
                  <div className="grid grid-cols-2 gap-4 p-4 bg-gray-700 rounded-lg">
                    <div>
                      <label className="block text-sm text-gray-400 mb-2">X</label>
                      <input
                        type="number"
                        value={localRegion.x}
                        onChange={(e) =>
                          setLocalRegion({
                            ...localRegion,
                            x: parseInt(e.target.value) || 0,
                          })
                        }
                        className="w-full px-3 py-2 bg-gray-600 border border-gray-500 rounded-lg focus:outline-none focus:border-blue-500"
                      />
                    </div>
                    <div>
                      <label className="block text-sm text-gray-400 mb-2">Y</label>
                      <input
                        type="number"
                        value={localRegion.y}
                        onChange={(e) =>
                          setLocalRegion({
                            ...localRegion,
                            y: parseInt(e.target.value) || 0,
                          })
                        }
                        className="w-full px-3 py-2 bg-gray-600 border border-gray-500 rounded-lg focus:outline-none focus:border-blue-500"
                      />
                    </div>
                  </div>
                )}
                
                {/* Current position display */}
                {positionPreset !== "custom" && (
                  <div className="text-sm text-gray-400 p-3 bg-gray-700 rounded-lg">
                    Position: {localRegion.x}, {localRegion.y}
                  </div>
                )}
              </div>
            </div>
          )}

          {activeTab === "capture" && (
            <div className="space-y-6">
              <div>
                <h3 className="text-lg font-semibold mb-4">Capture Method</h3>
                <div className="space-y-3">
                  <label className="flex items-center space-x-3 cursor-pointer p-3 bg-gray-700 rounded-lg hover:bg-gray-650 transition-colors">
                    <input
                      type="radio"
                      name="capture_method"
                      checked={localSettings.capture_method === "Wgc"}
                      onChange={() =>
                        setLocalSettings({
                          ...localSettings,
                          capture_method: "Wgc",
                        })
                      }
                      className="w-4 h-4 text-blue-600"
                    />
                    <div className="flex-1">
                      <div className="font-medium">Windows.Graphics.Capture (WGC)</div>
                      <div className="text-sm text-gray-400">Modern API, GPU-backed, best quality/performance</div>
                    </div>
                  </label>
                  <label className="flex items-center space-x-3 cursor-pointer p-3 bg-gray-700 rounded-lg hover:bg-gray-650 transition-colors">
                    <input
                      type="radio"
                      name="capture_method"
                      checked={localSettings.capture_method === "GdiCopy"}
                      onChange={() =>
                        setLocalSettings({
                          ...localSettings,
                          capture_method: "GdiCopy",
                        })
                      }
                      className="w-4 h-4 text-blue-600"
                    />
                    <div className="flex-1">
                      <div className="font-medium">GDI Screen Copy (RegionToShare-style)</div>
                      <div className="text-sm text-gray-400">Compatibility option; can be slower and may miss some protected content</div>
                    </div>
                  </label>
                </div>
              </div>

              <div>
                <h3 className="text-lg font-semibold mb-4">Performance</h3>
                <div>
                  <label className="block text-sm text-gray-400 mb-2">
                    Target FPS: {localSettings.target_fps} (Monitor: {monitorRefreshRate}Hz, Max: {maxFps})
                  </label>
                  <input
                    type="range"
                    min="15"
                    max={maxFps}
                    step="1"
                    value={Math.min(localSettings.target_fps, maxFps)}
                    onChange={(e) =>
                      setLocalSettings({
                        ...localSettings,
                        target_fps: parseInt(e.target.value),
                      })
                    }
                    className="w-full h-2 bg-gray-700 rounded-lg appearance-none cursor-pointer"
                  />
                  <div className="flex justify-between text-xs text-gray-400 mt-1">
                    <span>15</span>
                    <span>{maxFps}</span>
                  </div>
                </div>
              </div>

              <div>
                <h3 className="text-lg font-semibold mb-4">Preview Mode</h3>
                <div className="space-y-3">
                  <label className="flex items-center space-x-3 cursor-pointer p-3 bg-gray-700 rounded-lg hover:bg-gray-650 transition-colors">
                    <input
                      type="radio"
                      name="preview_mode"
                      checked={localSettings.preview_mode === "WinApiGdi"}
                      onChange={() =>
                        setLocalSettings({
                          ...localSettings,
                          preview_mode: "WinApiGdi",
                        })
                      }
                      className="w-4 h-4 text-blue-600"
                    />
                    <div className="flex-1">
                      <div className="font-medium">WinAPI GDI (Recommended)</div>
                      <div className="text-sm text-gray-400">Lightweight, Windows-only, best performance</div>
                    </div>
                  </label>
                  <label className="flex items-center space-x-3 cursor-pointer p-3 bg-gray-700 rounded-lg hover:bg-gray-650 transition-colors">
                    <input
                      type="radio"
                      name="preview_mode"
                      checked={localSettings.preview_mode === "TauriCanvas"}
                      onChange={() =>
                        setLocalSettings({
                          ...localSettings,
                          preview_mode: "TauriCanvas",
                        })
                      }
                      className="w-4 h-4 text-blue-600"
                    />
                    <div className="flex-1">
                      <div className="font-medium">Tauri Canvas (Coming Soon)</div>
                      <div className="text-sm text-gray-400">Cross-platform, WebView2 based</div>
                    </div>
                  </label>
                </div>
              </div>
            </div>
          )}

          {activeTab === "advanced" && (
            <div className="space-y-6">
              {/* Settings Management */}
              <div>
                <h3 className="text-lg font-semibold mb-4">Settings Management</h3>
                <div className="space-y-3">
                  <div className="flex flex-wrap gap-3">
                    <button
                      onClick={handleExportSettings}
                      className="px-4 py-2 bg-gray-700 hover:bg-gray-600 rounded-lg transition-colors flex items-center gap-2"
                    >
                      <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
                      </svg>
                      Export Settings
                    </button>
                    <button
                      onClick={handleImportSettings}
                      className="px-4 py-2 bg-gray-700 hover:bg-gray-600 rounded-lg transition-colors flex items-center gap-2"
                    >
                      <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                      </svg>
                      Import Settings
                    </button>
                    <button
                      onClick={handleOpenSettingsFolder}
                      className="px-4 py-2 bg-gray-700 hover:bg-gray-600 rounded-lg transition-colors flex items-center gap-2"
                    >
                      <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                      </svg>
                      Open Settings Folder
                    </button>
                  </div>
                  <p className="text-sm text-gray-400">
                    Export your settings to share or backup, import from a file, or open the settings folder.
                  </p>
                </div>
              </div>

              {/* Help / Troubleshooting */}
              <div>
                <h3 className="text-lg font-semibold mb-4">Help</h3>
                <div className="space-y-3">
                  <div className="text-sm text-gray-300 p-4 bg-gray-700 rounded-lg space-y-2">
                    <p>
                      <span className="font-medium">Preview Mode</span> controls how the shareable output window is rendered.
                      For Google Meet / Teams / Zoom on Windows, <span className="font-medium">WinAPI GDI</span> is the most compatible.
                    </p>
                    <p>
                      <span className="font-medium">Capture Method</span> controls how pixels are captured from the screen.
                      <span className="font-medium"> WGC</span> is the modern Windows API; <span className="font-medium">GDI Screen Copy</span> is a compatibility option similar to RegionToShare.
                    </p>
                    <p className="text-gray-400">
                      Advanced troubleshooting (hidden): you can add these keys to <span className="font-medium">settings.json</span> to override
                      the WinAPI destination window behavior.
                    </p>
                    <ul className="list-disc list-inside text-gray-400 space-y-1">
                      <li>
                        <span className="font-medium">winapi_destination_alpha</span>: number 0..255 (default: 0 in release builds)
                      </li>
                      <li>
                        <span className="font-medium">winapi_destination_topmost</span>: true/false (default: true)
                      </li>
                      <li>
                        <span className="font-medium">winapi_destination_toolwindow</span>: true/false (default: true)
                      </li>
                      <li>
                        <span className="font-medium">winapi_destination_click_through</span>: true/false (default: true)
                      </li>
                      <li>
                        <span className="font-medium">winapi_destination_layered</span>: true/false (default: true)
                      </li>
                      <li>
                        <span className="font-medium">winapi_destination_appwindow</span>: true/false (default: false; only used when toolwindow=false)
                      </li>
                      <li>
                        <span className="font-medium">winapi_destination_noactivate</span>: true/false (default: true)
                      </li>
                      <li>
                        <span className="font-medium">winapi_destination_overlapped</span>: true/false (default: false; uses a normal overlapped window style)
                      </li>
                      <li>
                        <span className="font-medium">winapi_destination_hide_taskbar_after_ms</span>: number (ms). If set, RustFrame will add TOOLWINDOW after this delay to hide from taskbar/Alt-Tab.
                      </li>
                    </ul>
                    <p className="text-gray-400">
                      Tip: If a meeting app shows black or stops updating, try setting <span className="font-medium">winapi_destination_alpha</span> to 1
                      or 255 for diagnostics.
                    </p>
                    <p className="text-gray-400">
                      Tip (Discord window list): Discord may hide “tool windows” and click-through/layered windows from its Applications picker.
                      Try setting <span className="font-medium">winapi_destination_toolwindow</span> to false and <span className="font-medium">winapi_destination_appwindow</span> to true.
                      If it still doesn’t appear, also try <span className="font-medium">winapi_destination_click_through</span> false.
                    </p>
                    <p className="text-gray-400">
                      If Discord still doesn’t list it, try making it more like a normal app window:
                      set <span className="font-medium">winapi_destination_overlapped</span> to true and <span className="font-medium">winapi_destination_noactivate</span> to false.
                    </p>
                  </div>
                </div>
              </div>


            </div>
          )}
        </div>

        {/* Footer */}
        <div className="px-6 py-4 border-t border-gray-700 flex justify-end space-x-3">
          <button
            onClick={onClose}
            className="px-4 py-2 bg-gray-700 hover:bg-gray-600 rounded-lg transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            className="px-4 py-2 bg-blue-600 hover:bg-blue-700 rounded-lg transition-colors"
          >
            Save Changes
          </button>
        </div>
      </div>
    </div>
  );
}

export default SettingsDialog;
