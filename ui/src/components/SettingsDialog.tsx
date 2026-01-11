import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save, ask } from "@tauri-apps/plugin-dialog";
import { Settings, MonitorInfo } from "../App";
import { PlatformInfo } from "../config";

interface CaptureRegion {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface SettingsDialogProps {
  initialTab?: TabType;
  settings: Settings;
  platformInfo: PlatformInfo;
  captureRegion: CaptureRegion;
  monitors: MonitorInfo[];
  selectedMonitor: number;
  onSave: (settings: Settings) => void;
  onRegionChange: (region: CaptureRegion) => void;
  onMonitorChange: (index: number) => void;
  onClose: () => void;
}

type TabType = "capture" | "mouse" | "visual" | "region" | "performance" | "profiles" | "advanced" | "about";

const SectionCard = ({ title, children, className = "" }: { title: string; children: React.ReactNode; className?: string }) => (
  <div className={`bg-gray-800/50 rounded-xl p-5 border border-gray-700 shadow-sm ${className}`}>
    <h3 className="text-lg font-bold text-gray-200 mb-4">{title}</h3>
    {children}
  </div>
);

function SettingsDialog({ 
  initialTab = "capture",
  settings,
  platformInfo,
  captureRegion, 
  monitors, 
  selectedMonitor, 
  onSave, 
  onRegionChange, 
  onMonitorChange, 
  onClose 
}: SettingsDialogProps) {
  const [activeTab, setActiveTab] = useState<TabType>(initialTab);
  const [localSettings, setLocalSettings] = useState<Settings>(settings);
  const [localRegion, setLocalRegion] = useState<CaptureRegion>(captureRegion);
  const [localMonitor, setLocalMonitor] = useState<number>(selectedMonitor);
  const [previewEnabled, setPreviewEnabled] = useState(false);
  const [positionPreset, setPositionPreset] = useState<string>("center");
  const [isSyncingFromBackend, setIsSyncingFromBackend] = useState(false);
  const [devMode, setDevMode] = useState(false);
  const [appVersion, setAppVersion] = useState("Unknown");
  const [toastMessage, setToastMessage] = useState<string | null>(null);
  const [clickHighlightTest, setClickHighlightTest] = useState<{x: number, y: number, timestamp: number} | null>(null);

  const pointerDownOnBackdropRef = useRef(false);
  
  // Profile management states
  const [profilesLoading, setProfilesLoading] = useState(false);
  const [profileVersionData, setProfileVersionData] = useState<any>(null);
  const [selectedProfileForDetails, setSelectedProfileForDetails] = useState<string>("");
  const [profileDetails, setProfileDetails] = useState<any>(null);

  useEffect(() => {
    invoke<string>("get_app_version").then(setAppVersion).catch(e => console.error(e));
  }, []);

  // Helper for color conversion
  const rgbaToHex = (rgba: [number, number, number, number]): string => {
    // Input: [r, g, b, a]
    // Output: #RRGGBB
    return `#${rgba[0].toString(16).padStart(2, '0')}${rgba[1].toString(16).padStart(2, '0')}${rgba[2].toString(16).padStart(2, '0')}`;
  };

  const hexToRgba = (hex: string): [number, number, number, number] => {
    const r = parseInt(hex.slice(1, 3), 16);
    const g = parseInt(hex.slice(3, 5), 16);
    const b = parseInt(hex.slice(5, 7), 16);
    return [r, g, b, 255];
  };

  // Auto-hide toast
  useEffect(() => {
    if (toastMessage) {
      const timer = setTimeout(() => setToastMessage(null), 3000);
      return () => clearTimeout(timer);
    }
  }, [toastMessage]);






  
  // Auto-detect monitor when region changes
  useEffect(() => {
    if (monitors.length === 0) return;
    
    // Find which monitor contains the center of the region
    const centerX = localRegion.x + localRegion.width / 2;
    const centerY = localRegion.y + localRegion.height / 2;
    
    const newMonitorIndex = monitors.findIndex((mon) => {
      return (
        centerX >= mon.x &&
        centerX < mon.x + mon.width &&
        centerY >= mon.y &&
        centerY < mon.y + mon.height
      );
    });
    
    if (newMonitorIndex !== -1 && newMonitorIndex !== localMonitor) {
      setLocalMonitor(newMonitorIndex);
    }
  }, [localRegion, monitors]);
  
  // Handle ESC key to close dialog
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };
    
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose]);

  // Load dev mode
  useEffect(() => {
    invoke<boolean>("is_dev_mode").then(setDevMode).catch(() => setDevMode(false));
  }, []);

  // Performance calculation
  const roundToStandardRefreshRate = (rate: number): number => {
    const standardRates = [24, 25, 30, 50, 60, 75, 90, 120, 144, 165, 240, 360];
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
  const maxFps = monitorRefreshRate * 2;

  // Prevent background scroll
  useEffect(() => {
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = "unset";
    };
  }, []);

  // Preview border management - only create/destroy on toggle
  useEffect(() => {
    if (previewEnabled) {
      // Backend (Windows COLORREF) expects 0x00BBGGRR from [R, G, B, A]
      const borderColor = (localSettings.border_color[0])
        | (localSettings.border_color[1] << 8)
        | (localSettings.border_color[2] << 16);
      
      // Create border once when preview is enabled
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

  const lastSentRegion = useRef({ x: 0, y: 0, width: 0, height: 0 });
  
  useEffect(() => {
    if (previewEnabled && !isSyncingFromBackend) {
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

  useEffect(() => {
    if (previewEnabled) {
      // Backend (Windows COLORREF) expects 0x00BBGGRR from [R, G, B, A]
      const borderColor = (localSettings.border_color[0])
        | (localSettings.border_color[1] << 8)
        | (localSettings.border_color[2] << 16);
      
      invoke("update_preview_border_style", {
        borderWidth: localSettings.border_width,
        borderColor: borderColor
      }).catch(console.error);
    }
  }, [localSettings.border_color, localSettings.border_width, previewEnabled]);

  // Sync back from border movement
  useEffect(() => {
    if (!previewEnabled) return;

    let lastKnownRect = { x: localRegion.x, y: localRegion.y, width: localRegion.width, height: localRegion.height };

    const syncInterval = setInterval(async () => {
      try {
        const rect = await invoke<[number, number, number, number] | null>("get_preview_border_rect");
        if (rect) {
          const [x, y, width, height] = rect;
          
          if (x !== lastKnownRect.x || y !== lastKnownRect.y || 
              width !== lastKnownRect.width || height !== lastKnownRect.height) {
            
            lastKnownRect = { x, y, width, height };
            lastSentRegion.current = { x, y, width, height };
            
            setIsSyncingFromBackend(true);
            setLocalRegion({ x, y, width, height });
            setPositionPreset("custom");
            
            setTimeout(() => setIsSyncingFromBackend(false), 100);
            
            for (let i = 0; i < monitors.length; i++) {
              const mon = monitors[i];
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
      } catch (e) {}
    }, 300);

    return () => clearInterval(syncInterval);
  }, [previewEnabled, monitors]);

  const handleSave = () => {
    onSave(localSettings);
    onRegionChange(localRegion);
    onMonitorChange(localMonitor);
    onClose();
  };

  const handleExportSettings = async () => {
    try {
      const filePath = await save({
        defaultPath: "rustframe-settings.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (filePath) {
        await invoke("export_settings", { path: filePath });
        setToastMessage("Settings exported successfully");
      }
    } catch (error) {
      console.error(error);
      setToastMessage("Failed to export settings");
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
        onSave(imported);
        setToastMessage("Settings imported successfully");
      }
    } catch (error) {
      console.error(error);
      setToastMessage("Failed to import settings");
    }
  };

  const tabs: { id: TabType; label: string; icon: string }[] = [
    { id: "capture", label: "Capture", icon: "🎯" },
    { id: "mouse", label: "Mouse", icon: "🖱️" },
    { id: "visual", label: "Visual", icon: "🎨" },
    { id: "region", label: "Region", icon: "📐" },
    { id: "performance", label: "Perf", icon: "🚀" },
    { id: "profiles", label: "Profiles", icon: "📦" },
    { id: "advanced", label: "Advanced", icon: "🔧" },
    { id: "about", label: "About", icon: "ℹ️" },
  ];

  return (
    <div
      className="fixed inset-0 bg-black/80 flex items-center justify-center z-50"
      style={{ WebkitAppRegion: 'no-drag' } as any} onMouseDown={(e) => e.stopPropagation()}
      onPointerDownCapture={(e) => {
        pointerDownOnBackdropRef.current = e.target === e.currentTarget;
      }}
      onClick={(e) => {
      // Close only if the *press* started on the backdrop.
      // This prevents a slider drag that ends outside the input from closing the modal.
      if (e.target === e.currentTarget && pointerDownOnBackdropRef.current) onClose();
    }}>
      {/* Toast */}
      {toastMessage && (
        <div className="fixed top-6 right-6 z-[70] bg-gray-900 border border-gray-700 rounded-lg px-4 py-3 shadow-2xl animate-slide-in flex items-center gap-2">
          <div className="w-2 h-2 rounded-full bg-green-500"></div>
          <p className="text-white text-sm font-medium">{toastMessage}</p>
        </div>
      )}
      
      <div
        className="bg-gray-900 rounded-2xl shadow-2xl w-full max-w-4xl max-h-[85vh] flex flex-col border border-gray-700"
        style={{ WebkitAppRegion: 'no-drag' } as any}
        onMouseDown={(e) => e.stopPropagation()}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="px-6 py-5 border-b border-gray-800 bg-gradient-to-r from-gray-900 to-gray-800 flex items-center justify-between">
          <div>
            <h2 className="text-2xl font-bold bg-clip-text text-transparent bg-gradient-to-r from-blue-400 to-purple-400">Settings</h2>
            <p className="text-gray-400 text-sm mt-1">Configure capture behavior and appearance</p>
          </div>
          <button
            onClick={onClose}
            className="text-gray-400 hover:text-white transition-colors p-2 hover:bg-gray-800 rounded-lg"
          >
            <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Tabs Bar */}
        <div className="px-2 pt-2 border-b border-gray-800 bg-gray-900">
          <div className="flex flex-wrap gap-1 p-2">
            {tabs.map((tab) => (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`flex items-center gap-2 px-4 py-3 rounded-xl transition-all duration-200 font-medium text-sm flex-shrink-0 ${
                  activeTab === tab.id
                    ? "bg-gray-800 text-white shadow-lg border border-gray-700 transform scale-[1.02]"
                    : "text-gray-400 hover:text-white hover:bg-gray-800/50 border border-transparent"
                }`}
              >
                <span className="text-lg">{tab.icon}</span>
                {tab.label}
              </button>
            ))}
          </div>
        </div>

        {/* Content Area */}
        <div className="flex-1 p-6 bg-gray-900/50" style={{ overflow: 'auto' }}>
          
          {/* TAB: CAPTURE */}
          {activeTab === "capture" && (
            <div className="space-y-6 animate-fadeIn">
              {/* Platform Info */}
              <div className="bg-blue-900/10 border border-blue-500/20 rounded-xl p-4 flex items-center gap-4">
                <div className="p-3 bg-blue-500/20 rounded-lg">
                  <svg className="w-6 h-6 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
                  </svg>
                </div>
                <div>
                  <div className="font-bold text-gray-200">{platformInfo.os_name} {platformInfo.os_version}</div>
                  <div className="text-sm text-gray-400">
                    {platformInfo.capabilities.supports_hardware_acceleration ? "✓ Hardware Acceleration Active" : "Hardware Acceleration Unavailable"}
                  </div>
                </div>
              </div>

              <SectionCard title="Capture Method">
                <div className={`grid grid-cols-1 ${platformInfo.available_capture_methods.length > 1 ? 'md:grid-cols-2' : ''} gap-4`}>
                  {platformInfo.available_capture_methods.map((method) => (
                    <label 
                      key={method.id}
                      className={`relative flex flex-col p-4 rounded-xl border-2 transition-all cursor-pointer hover:bg-gray-700/50 ${
                        localSettings.capture_method === method.id 
                          ? 'bg-blue-600/10 border-blue-500' 
                          : 'bg-gray-900 border-gray-700'
                      }`}
                    >
                      <input
                        type="radio"
                        name="capture_method"
                        checked={localSettings.capture_method === method.id}
                        onChange={() => setLocalSettings({ ...localSettings, capture_method: method.id as any })}
                        className="absolute top-4 right-4 w-5 h-5 text-blue-600 bg-gray-700 border-gray-600 focus:ring-blue-500 focus:ring-offset-gray-900"
                      />
                      <span className="font-bold text-white mb-1">{method.name}</span>
                      <p className="text-xs text-gray-400 mb-3">{method.description}</p>
                      
                      <div className="flex flex-wrap gap-2 mt-auto">
                        {method.recommended && (
                          <span className="px-2 py-1 bg-green-500/10 text-green-400 text-xs rounded-lg font-medium border border-green-500/20">
                            Recommended
                          </span>
                        )}
                        {method.hardware_accelerated && (
                          <span className="px-2 py-1 bg-purple-500/10 text-purple-400 text-xs rounded-lg font-medium border border-purple-500/20">
                            GPU Accelerated
                          </span>
                        )}
                      </div>
                    </label>
                  ))}
                </div>
              </SectionCard>

              <SectionCard title="Window Preview Mode">
                <div className="space-y-4">
                  <p className="text-sm text-gray-400 mb-2">Controls how the shareable output window is rendered.</p>
                  
                  {platformInfo.os_type === "windows" && (
                     <label className={`flex items-center p-4 rounded-xl border transition-all cursor-pointer ${
                       localSettings.preview_mode === "WinApiGdi" ? 'bg-blue-600/10 border-blue-500' : 'bg-gray-900 border-gray-700 hover:bg-gray-700/50'
                     }`}>
                       <input
                         type="radio"
                         name="preview_mode"
                         checked={localSettings.preview_mode === "WinApiGdi"}
                         onChange={() => setLocalSettings({ ...localSettings, preview_mode: "WinApiGdi" })}
                         className="w-5 h-5 text-blue-600 mr-4"
                       />
                       <div>
                         <div className="font-bold text-gray-200">WinAPI GDI</div>
                         <div className="text-xs text-gray-500">Native Windows API. Best performance for Meet, Teams, and Zoom.</div>
                       </div>
                     </label>
                  )}

                  <label className={`flex items-center p-4 rounded-xl border transition-all cursor-pointer ${
                    localSettings.preview_mode === "TauriCanvas" ? 'bg-blue-600/10 border-blue-500' : 'bg-gray-900 border-gray-700 hover:bg-gray-700/50'
                  }`}>
                    <input
                      type="radio"
                      name="preview_mode"
                      checked={localSettings.preview_mode === "TauriCanvas"}
                      onChange={() => setLocalSettings({ ...localSettings, preview_mode: "TauriCanvas" })}
                      className="w-5 h-5 text-blue-600 mr-4"
                    />
                    <div>
                      <div className="font-bold text-gray-200">Tauri Canvas</div>
                      <div className="text-xs text-gray-500">Cross-platform webview rendering.</div>
                    </div>
                  </label>
                </div>
              </SectionCard>
            </div>
          )}

          {/* TAB: MOUSE */}
          {activeTab === "mouse" && (
            <div className="space-y-6 animate-fadeIn">
              <SectionCard title="Cursor Visibility">
                <div className="flex flex-col gap-4">
                  <label className="flex items-center justify-between p-4 bg-gray-900 rounded-lg border border-gray-700 cursor-pointer hover:bg-gray-800 transition-colors">
                    <div className="flex items-center gap-3">
                      <div className="p-2 bg-blue-500/20 rounded text-blue-400">
                        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 15l-2 5L9 9l11 4-5 2zm0 0l5 5M7.188 2.239l.777 2.897M5.136 7.965l-2.898-.777M13.95 4.05l-2.122 2.122m-5.657 5.656l-2.12 2.122" />
                        </svg>
                      </div>
                      <div>
                        <span className="font-medium text-gray-200 block">Show Cursor</span>
                        <span className="text-xs text-gray-500">Include mouse cursor in capture</span>
                      </div>
                    </div>
                    <div className={`w-12 h-6 rounded-full p-1 transition-colors ${localSettings.show_cursor ? 'bg-blue-600' : 'bg-gray-600'}`}>
                      <input 
                        type="checkbox" 
                        className="hidden" 
                        checked={localSettings.show_cursor}
                        onChange={(e) => setLocalSettings({ ...localSettings, show_cursor: e.target.checked })}
                      />
                      <div className={`w-4 h-4 rounded-full bg-white transition-transform ${localSettings.show_cursor ? 'translate-x-6' : ''}`}></div>
                    </div>
                  </label>

                  <label className="flex items-center justify-between p-4 bg-gray-900 rounded-lg border border-gray-700 cursor-pointer hover:bg-gray-800 transition-colors">
                    <div className="flex items-center gap-3">
                      <div className="p-2 bg-purple-500/20 rounded text-purple-400">
                        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                        </svg>
                      </div>
                      <div>
                        <span className="font-medium text-gray-200 block">Highlight Clicks</span>
                        <span className="text-xs text-gray-500">Show visual ripples when clicking</span>
                      </div>
                    </div>
                    <div className={`w-12 h-6 rounded-full p-1 transition-colors ${localSettings.capture_clicks ? 'bg-purple-600' : 'bg-gray-600'}`}>
                      <input 
                        type="checkbox" 
                        className="hidden" 
                        checked={localSettings.capture_clicks}
                        onChange={(e) => setLocalSettings({ ...localSettings, capture_clicks: e.target.checked })}
                      />
                      <div className={`w-4 h-4 rounded-full bg-white transition-transform ${localSettings.capture_clicks ? 'translate-x-6' : ''}`}></div>
                    </div>
                  </label>
                </div>
              </SectionCard>

              {/* Click Customization - Only if enabled */}
              <div className={`transition-all duration-300 ${localSettings.capture_clicks ? 'opacity-100 max-h-[600px]' : 'opacity-40 max-h-0 overflow-hidden pointer-events-none select-none grayscale'}`}
                   onMouseDown={(e) => {
                     // Ensure mouse down in this container also stops propagation if enabled.
                     if (localSettings.capture_clicks) e.stopPropagation();
                   }}
              >
                 <SectionCard title="Click Highlight Style">
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                      <div>
                        <label className="block text-sm text-gray-400 mb-2">Color</label>
                        <div className="flex items-center gap-3">
                          <input
                            type="color"
                            value={rgbaToHex(localSettings.click_highlight_color)}
                            onInput={(e) => setLocalSettings({ ...localSettings, click_highlight_color: hexToRgba((e.target as HTMLInputElement).value) })}
                            onChange={(e) => setLocalSettings({ ...localSettings, click_highlight_color: hexToRgba(e.target.value) })}
                            className="w-12 h-8 rounded cursor-pointer"
                            style={{ WebkitAppRegion: 'no-drag' } as any} onMouseDown={(e) => e.stopPropagation()}
                          />
                          <div className="text-xs text-gray-500 font-mono">
                            {rgbaToHex(localSettings.click_highlight_color)}
                          </div>
                        </div>
                      </div>
                      
                      <div className="space-y-4">
                         <div>
                            <div className="flex justify-between text-sm mb-1">
                              <span className="text-gray-400">Radius</span>
                              <span className="text-gray-200">{localSettings.click_highlight_radius}px</span>
                            </div>
                            <input
                              type="range"
                              min="10"
                              max="100"
                              value={localSettings.click_highlight_radius}
                              onChange={(e) => setLocalSettings({ ...localSettings, click_highlight_radius: parseInt(e.target.value) })}
                              className="w-full h-2 bg-gray-700 rounded-lg cursor-pointer"
                              style={{ WebkitAppRegion: 'no-drag' } as any} 
                              onMouseDown={(e) => e.stopPropagation()}
                            />
                         </div>
                         <div>
                            <div className="flex justify-between text-sm mb-1">
                              <span className="text-gray-400">Fade Duration</span>
                              <span className="text-gray-200">{localSettings.click_dissolve_ms}ms</span>
                            </div>
                            <input
                              type="range"
                              min="100"
                              max="2000"
                              step="100"
                              value={localSettings.click_dissolve_ms}
                              onChange={(e) => setLocalSettings({ ...localSettings, click_dissolve_ms: parseInt(e.target.value) })}
                              className="w-full h-2 bg-gray-700 rounded-lg cursor-pointer"
                              style={{ WebkitAppRegion: 'no-drag' } as any} 
                              onMouseDown={(e) => e.stopPropagation()}
                            />
                         </div>
                      </div>
                    </div>
                    
                    {/* Test Button with Preview */}
                    <div className="mt-6 border-t border-gray-700 pt-4">
                      <div className="flex justify-between items-center mb-3">
                        <label className="text-sm text-gray-400">Preview</label>
                        <button
                          onClick={() => {
                            // Calculate dimensions dynamically
                            const previewWidth = Math.max(150, localSettings.click_highlight_radius * 3);
                            const previewHeight = Math.max(60, localSettings.click_highlight_radius * 2.5);
                            setClickHighlightTest({ 
                              x: previewWidth / 2, 
                              y: previewHeight / 2, 
                              timestamp: Date.now() 
                            });
                            setTimeout(() => setClickHighlightTest(null), localSettings.click_dissolve_ms);
                          }}
                          className="px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg transition-colors text-sm font-medium flex items-center gap-2"
                        >
                          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                          </svg>
                          Test Highlight
                        </button>
                      </div>
                      
                      {/* Preview Area (non-interactive, display only) - sized based on radius */}
                      <div 
                        className="relative bg-gray-900/50 rounded-lg border border-gray-700 overflow-hidden mx-auto"
                        style={{ 
                          pointerEvents: 'none',
                          width: `${Math.max(150, localSettings.click_highlight_radius * 3)}px`,
                          height: `${Math.max(60, localSettings.click_highlight_radius * 2.5)}px`
                        }}
                      >
                        <div className="absolute inset-0 flex items-center justify-center text-gray-600 text-xs">
                          Click test button to preview
                        </div>
                        
                        {/* Click Highlight Animation */}
                        {clickHighlightTest && (
                          <div
                            style={{
                              position: 'absolute',
                              left: clickHighlightTest.x,
                              top: clickHighlightTest.y,
                              width: localSettings.click_highlight_radius * 2,
                              height: localSettings.click_highlight_radius * 2,
                              marginLeft: -localSettings.click_highlight_radius,
                              marginTop: -localSettings.click_highlight_radius,
                              borderRadius: '50%',
                              backgroundColor: rgbaToHex(localSettings.click_highlight_color),
                              opacity: localSettings.click_highlight_color[3] / 255,
                              animation: `clickHighlightFade ${localSettings.click_dissolve_ms}ms ease-out forwards`,
                              pointerEvents: 'none',
                            }}
                          />
                        )}
                      </div>
                    </div>
                 </SectionCard>
              </div>
            </div>
          )}

          {/* TAB: VISUAL */}
          {activeTab === "visual" && (
            <div className="space-y-6 animate-fadeIn">
              <SectionCard title="Capture Border">
                <div className="space-y-6">
                  <label className="flex items-center justify-between">
                    <div>
                      <span className="font-medium text-gray-200 block">Show Border</span>
                      <span className="text-xs text-gray-500">Visible outline around capture region</span>
                    </div>
                    <div className={`w-12 h-6 rounded-full p-1 transition-colors ${localSettings.show_border ? 'bg-blue-600' : 'bg-gray-600'}`}>
                      <input 
                        type="checkbox" 
                        className="hidden" 
                        checked={localSettings.show_border}
                        onChange={(e) => setLocalSettings({ ...localSettings, show_border: e.target.checked })}
                      />
                      <div className={`w-4 h-4 rounded-full bg-white transition-transform ${localSettings.show_border ? 'translate-x-6' : ''}`}></div>
                    </div>
                  </label>

                  <div className={`grid grid-cols-2 gap-6 transition-opacity ${localSettings.show_border ? 'opacity-100' : 'opacity-40 pointer-events-none'}`}>
                    <div>
                      <label className="block text-sm text-gray-400 mb-2">Border Color</label>
                      <input
                        type="color"
                        value={rgbaToHex(localSettings.border_color)}
                        onInput={(e) => setLocalSettings({ ...localSettings, border_color: hexToRgba((e.target as HTMLInputElement).value) })}
                        onChange={(e) => setLocalSettings({ ...localSettings, border_color: hexToRgba(e.target.value) })}
                        className="w-full h-10 rounded-lg cursor-pointer bg-gray-700"
                        style={{ WebkitAppRegion: 'no-drag' } as any} onMouseDown={(e) => e.stopPropagation()}


                      />
                    </div>
                    <div>
                      <label className="block text-sm text-gray-400 mb-2">Border Width: {localSettings.border_width}px</label>
                       <input
                        type="range"
                        min="1"
                        max="20"
                        value={localSettings.border_width}
                        onInput={(e) => setLocalSettings({ ...localSettings, border_width: parseInt((e.target as HTMLInputElement).value) })}
                        onChange={(e) => setLocalSettings({ ...localSettings, border_width: parseInt(e.target.value) })}
                        className="w-full h-2 bg-gray-700 rounded-lg cursor-pointer mt-2"
                        style={{ WebkitAppRegion: 'no-drag' } as any} onMouseDown={(e) => e.stopPropagation()}


                      />
                    </div>
                  </div>
                </div>
              </SectionCard>

              <SectionCard title="REC Indicator">
                <div className="space-y-6">
                   <label className="flex items-center justify-between">
                    <div>
                      <span className="font-medium text-gray-200 block">Show Indicator</span>
                      <span className="text-xs text-gray-500">Red "● REC" badge inside capture area</span>
                    </div>
                    <div className={`w-12 h-6 rounded-full p-1 transition-colors ${localSettings.show_rec_indicator ? 'bg-red-600' : 'bg-gray-600'}`}>
                      <input 
                        type="checkbox" 
                        className="hidden" 
                        checked={localSettings.show_rec_indicator}
                        onChange={(e) => setLocalSettings({ ...localSettings, show_rec_indicator: e.target.checked })}
                      />
                      <div className={`w-4 h-4 rounded-full bg-white transition-transform ${localSettings.show_rec_indicator ? 'translate-x-6' : ''}`}></div>
                    </div>
                  </label>
                   
                   <div className={`transition-opacity ${localSettings.show_rec_indicator ? 'opacity-100' : 'opacity-40 pointer-events-none'}`}>
                      <label className="block text-sm text-gray-400 mb-2">Size</label>
                      <div className="flex gap-2">
                        {['small', 'medium', 'large'].map((size) => (
                          <button
                            key={size}
                            onClick={() => setLocalSettings({ ...localSettings, rec_indicator_size: size as any })}
                            className={`flex-1 py-2 rounded-lg border text-sm capitalize transition-colors ${
                              localSettings.rec_indicator_size === size
                                ? 'bg-red-500/20 border-red-500 text-red-300'
                                : 'bg-gray-900 border-gray-700 text-gray-400 hover:bg-gray-800'
                            }`}
                          >
                            {size}
                          </button>
                        ))}
                      </div>
                   </div>
                </div>
              </SectionCard>
            </div>
          )}

          {/* TAB: REGION */}
          {activeTab === "region" && (
            <div className="space-y-6 animate-fadeIn">
              <div className="bg-yellow-500/10 border border-yellow-500/20 rounded-xl p-4 flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <svg className="w-6 h-6 text-yellow-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                  </svg>
                  <div>
                    <div className="font-bold text-gray-200">Live Preview</div>
                    <div className="text-xs text-gray-400">Show borders while adjusting</div>
                  </div>
                </div>
                <label className="relative inline-flex items-center cursor-pointer">
                  <input type="checkbox" className="sr-only peer" checked={previewEnabled} onChange={(e) => setPreviewEnabled(e.target.checked)} />
                  <div className="w-11 h-6 bg-gray-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-yellow-500"></div>
                </label>
              </div>

              <SectionCard title="Active Monitor">
                <select
                  value={localMonitor}
                  onChange={(e) => {
                    const idx = parseInt(e.target.value);
                    setLocalMonitor(idx);
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
                  className="w-full px-4 py-3 bg-gray-900 border border-gray-700 rounded-lg focus:ring-2 focus:ring-blue-500 focus:outline-none text-white text-lg"
                >
                  {monitors.map((mon, idx) => (
                    <option key={mon.id} value={idx}>
                      {mon.name} ({mon.width}x{mon.height}{mon.scale_factor ? ` @ ${(mon.scale_factor * 100).toFixed(0)}%` : ""}) {mon.is_primary ? "⭐ Primary" : ""}
                    </option>
                  ))}
                </select>
              </SectionCard>

              <SectionCard title="Dimensions">
                <div className="grid grid-cols-2 gap-4 mb-4">
                  <div>
                    <label className="text-xs text-gray-500 uppercase font-bold tracking-wider mb-1 block">Width</label>
                    <input
                      type="number"
                      value={localRegion.width}
                      onChange={(e) => setLocalRegion({ ...localRegion, width: parseInt(e.target.value) || 800 })}
                      className="w-full bg-gray-900 border border-gray-700 rounded-lg p-3 text-white focus:border-blue-500 focus:outline-none"
                    />
                  </div>
                   <div>
                    <label className="text-xs text-gray-500 uppercase font-bold tracking-wider mb-1 block">Height</label>
                    <input
                      type="number"
                      value={localRegion.height}
                      onChange={(e) => setLocalRegion({ ...localRegion, height: parseInt(e.target.value) || 600 })}
                      className="w-full bg-gray-900 border border-gray-700 rounded-lg p-3 text-white focus:border-blue-500 focus:outline-none"
                    />
                  </div>
                </div>
                
                <p className="text-xs text-gray-500 mb-2 font-bold">PRESETS</p>
                <div className="grid grid-cols-3 gap-2">
                  {[
                    { label: "720p", w: 1280, h: 720 },
                    { label: "1080p", w: 1920, h: 1080 },
                    { label: "1440p", w: 2560, h: 1440 },
                    { label: "4K", w: 3840, h: 2160 },
                    { label: "Squares", w: 1080, h: 1080 },
                    { label: "Small", w: 800, h: 600 },
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
                      className="px-3 py-2 bg-gray-700 hover:bg-gray-600 rounded text-sm text-gray-300 transition-colors"
                    >
                      {preset.label}
                    </button>
                  ))}
                </div>
              </SectionCard>

              <SectionCard title="Position">
                <div className="grid grid-cols-2 gap-4 mb-4">
                  <div>
                    <label className="text-xs text-gray-500 uppercase font-bold tracking-wider mb-1 block">X Position</label>
                    <input
                      type="number"
                      value={localRegion.x}
                      onChange={(e) => {
                        setLocalRegion({ ...localRegion, x: parseInt(e.target.value) || 0 });
                        setPositionPreset("custom");
                      }}
                      className="w-full bg-gray-900 border border-gray-700 rounded-lg p-3 text-white focus:border-blue-500 focus:outline-none"
                    />
                  </div>
                  <div>
                    <label className="text-xs text-gray-500 uppercase font-bold tracking-wider mb-1 block">Y Position</label>
                    <input
                      type="number"
                      value={localRegion.y}
                      onChange={(e) => {
                        setLocalRegion({ ...localRegion, y: parseInt(e.target.value) || 0 });
                        setPositionPreset("custom");
                      }}
                      className="w-full bg-gray-900 border border-gray-700 rounded-lg p-3 text-white focus:border-blue-500 focus:outline-none"
                    />
                  </div>
                </div>
                
                <p className="text-xs text-gray-500 mb-2 font-bold">POSITION PRESETS</p>
                <div className="grid grid-cols-3 gap-2 mb-4">
                  {[
                    { label: "Center", position: "center" },
                    { label: "Top-Left", position: "top-left" },
                    { label: "Top-Right", position: "top-right" },
                    { label: "Bottom-Left", position: "bottom-left" },
                    { label: "Bottom-Right", position: "bottom-right" },
                    { label: "Top-Center", position: "top-center" },
                  ].map((preset) => (
                    <button
                      key={preset.position}
                      onClick={() => {
                        const mon = monitors[localMonitor];
                        if (mon) {
                          let x = mon.x;
                          let y = mon.y;
                          
                          // Calculate position based on preset
                          switch (preset.position) {
                            case "center":
                              x = mon.x + Math.floor((mon.width - localRegion.width) / 2);
                              y = mon.y + Math.floor((mon.height - localRegion.height) / 2);
                              break;
                            case "top-left":
                              x = mon.x + 20;
                              y = mon.y + 20;
                              break;
                            case "top-right":
                              x = mon.x + mon.width - localRegion.width - 20;
                              y = mon.y + 20;
                              break;
                            case "bottom-left":
                              x = mon.x + 20;
                              y = mon.y + mon.height - localRegion.height - 20;
                              break;
                            case "bottom-right":
                              x = mon.x + mon.width - localRegion.width - 20;
                              y = mon.y + mon.height - localRegion.height - 20;
                              break;
                            case "top-center":
                              x = mon.x + Math.floor((mon.width - localRegion.width) / 2);
                              y = mon.y + 20;
                              break;
                          }
                          
                          setLocalRegion({ ...localRegion, x, y });
                          setPositionPreset(preset.position);
                        }
                      }}
                      className={`px-3 py-2 rounded text-sm transition-colors ${
                        positionPreset === preset.position 
                          ? 'bg-blue-600 text-white' 
                          : 'bg-gray-700 hover:bg-gray-600 text-gray-300'
                      }`}
                    >
                      {preset.label}
                    </button>
                  ))}
                </div>

                <p className="text-xs text-gray-500 mb-2 font-bold">SNAP PRESETS (SIZE + POSITION)</p>
                <div className="grid grid-cols-4 gap-2">
                  {[
                    { label: "Left Half", snap: "left-half", svg: <svg viewBox="0 0 40 24" className="w-10 h-6"><rect x="2" y="2" width="18" height="20" fill="currentColor" opacity="0.8"/><rect x="20" y="2" width="18" height="20" fill="currentColor" opacity="0.2"/></svg> },
                    { label: "Right Half", snap: "right-half", svg: <svg viewBox="0 0 40 24" className="w-10 h-6"><rect x="2" y="2" width="18" height="20" fill="currentColor" opacity="0.2"/><rect x="20" y="2" width="18" height="20" fill="currentColor" opacity="0.8"/></svg> },
                    { label: "Top Half", snap: "top-half", svg: <svg viewBox="0 0 40 24" className="w-10 h-6"><rect x="2" y="2" width="36" height="9" fill="currentColor" opacity="0.8"/><rect x="2" y="13" width="36" height="9" fill="currentColor" opacity="0.2"/></svg> },
                    { label: "Bottom Half", snap: "bottom-half", svg: <svg viewBox="0 0 40 24" className="w-10 h-6"><rect x="2" y="2" width="36" height="9" fill="currentColor" opacity="0.2"/><rect x="2" y="13" width="36" height="9" fill="currentColor" opacity="0.8"/></svg> },
                    { label: "Top-Left ¼", snap: "top-left-quarter", svg: <svg viewBox="0 0 40 24" className="w-10 h-6"><rect x="2" y="2" width="18" height="9" fill="currentColor" opacity="0.8"/><rect x="20" y="2" width="18" height="9" fill="currentColor" opacity="0.2"/><rect x="2" y="13" width="18" height="9" fill="currentColor" opacity="0.2"/><rect x="20" y="13" width="18" height="9" fill="currentColor" opacity="0.2"/></svg> },
                    { label: "Top-Right ¼", snap: "top-right-quarter", svg: <svg viewBox="0 0 40 24" className="w-10 h-6"><rect x="2" y="2" width="18" height="9" fill="currentColor" opacity="0.2"/><rect x="20" y="2" width="18" height="9" fill="currentColor" opacity="0.8"/><rect x="2" y="13" width="18" height="9" fill="currentColor" opacity="0.2"/><rect x="20" y="13" width="18" height="9" fill="currentColor" opacity="0.2"/></svg> },
                    { label: "Bottom-Left ¼", snap: "bottom-left-quarter", svg: <svg viewBox="0 0 40 24" className="w-10 h-6"><rect x="2" y="2" width="18" height="9" fill="currentColor" opacity="0.2"/><rect x="20" y="2" width="18" height="9" fill="currentColor" opacity="0.2"/><rect x="2" y="13" width="18" height="9" fill="currentColor" opacity="0.8"/><rect x="20" y="13" width="18" height="9" fill="currentColor" opacity="0.2"/></svg> },
                    { label: "Bottom-Right ¼", snap: "bottom-right-quarter", svg: <svg viewBox="0 0 40 24" className="w-10 h-6"><rect x="2" y="2" width="18" height="9" fill="currentColor" opacity="0.2"/><rect x="20" y="2" width="18" height="9" fill="currentColor" opacity="0.2"/><rect x="2" y="13" width="18" height="9" fill="currentColor" opacity="0.2"/><rect x="20" y="13" width="18" height="9" fill="currentColor" opacity="0.8"/></svg> },
                    { label: "Left 1/3", snap: "left-third", svg: <svg viewBox="0 0 40 24" className="w-10 h-6"><rect x="2" y="2" width="11" height="20" fill="currentColor" opacity="0.8"/><rect x="15" y="2" width="11" height="20" fill="currentColor" opacity="0.2"/><rect x="28" y="2" width="10" height="20" fill="currentColor" opacity="0.2"/></svg> },
                    { label: "Right 1/3", snap: "right-third", svg: <svg viewBox="0 0 40 24" className="w-10 h-6"><rect x="2" y="2" width="11" height="20" fill="currentColor" opacity="0.2"/><rect x="15" y="2" width="11" height="20" fill="currentColor" opacity="0.2"/><rect x="28" y="2" width="10" height="20" fill="currentColor" opacity="0.8"/></svg> },
                    { label: "Left 2/3", snap: "left-two-thirds", svg: <svg viewBox="0 0 40 24" className="w-10 h-6"><rect x="2" y="2" width="24" height="20" fill="currentColor" opacity="0.8"/><rect x="28" y="2" width="10" height="20" fill="currentColor" opacity="0.2"/></svg> },
                    { label: "Right 2/3", snap: "right-two-thirds", svg: <svg viewBox="0 0 40 24" className="w-10 h-6"><rect x="2" y="2" width="11" height="20" fill="currentColor" opacity="0.2"/><rect x="15" y="2" width="23" height="20" fill="currentColor" opacity="0.8"/></svg> },
                  ].map((preset) => (
                    <button
                      key={preset.snap}
                      onClick={() => {
                        const mon = monitors[localMonitor];
                        if (mon) {
                          let x = mon.x;
                          let y = mon.y;
                          let w = mon.width;
                          let h = mon.height;
                          
                          // Calculate size and position based on snap preset
                          switch (preset.snap) {
                            case "left-half":
                              w = Math.floor(mon.width / 2);
                              break;
                            case "right-half":
                              x = mon.x + Math.floor(mon.width / 2);
                              w = Math.floor(mon.width / 2);
                              break;
                            case "top-half":
                              h = Math.floor(mon.height / 2);
                              break;
                            case "bottom-half":
                              y = mon.y + Math.floor(mon.height / 2);
                              h = Math.floor(mon.height / 2);
                              break;
                            case "top-left-quarter":
                              w = Math.floor(mon.width / 2);
                              h = Math.floor(mon.height / 2);
                              break;
                            case "top-right-quarter":
                              x = mon.x + Math.floor(mon.width / 2);
                              w = Math.floor(mon.width / 2);
                              h = Math.floor(mon.height / 2);
                              break;
                            case "bottom-left-quarter":
                              y = mon.y + Math.floor(mon.height / 2);
                              w = Math.floor(mon.width / 2);
                              h = Math.floor(mon.height / 2);
                              break;
                            case "bottom-right-quarter":
                              x = mon.x + Math.floor(mon.width / 2);
                              y = mon.y + Math.floor(mon.height / 2);
                              w = Math.floor(mon.width / 2);
                              h = Math.floor(mon.height / 2);
                              break;
                            case "left-third":
                              w = Math.floor(mon.width / 3);
                              break;
                            case "right-third":
                              x = mon.x + Math.floor(mon.width * 2 / 3);
                              w = Math.floor(mon.width / 3);
                              break;
                            case "left-two-thirds":
                              w = Math.floor(mon.width * 2 / 3);
                              break;
                            case "right-two-thirds":
                              x = mon.x + Math.floor(mon.width / 3);
                              w = Math.floor(mon.width * 2 / 3);
                              break;
                          }
                          
                          setLocalRegion({ x, y, width: w, height: h });
                          setPositionPreset(preset.snap);
                        }
                      }}
                      className="flex flex-col items-center gap-1 px-2 py-2 bg-gray-700 hover:bg-gray-600 rounded-lg transition-colors group"
                    >
                      <div className="text-blue-400 group-hover:text-blue-300 transition-colors">
                        {preset.svg}
                      </div>
                      <span className="text-[10px] text-gray-300">{preset.label}</span>
                    </button>
                  ))}
                </div>
              </SectionCard>
            </div>
          )}

           {/* TAB: PERFORMANCE */}
          {activeTab === "performance" && (
            <div className="space-y-6 animate-fadeIn">
               <SectionCard title="Target Framerate" className="border-l-4 border-l-green-500">
                  <div className="mb-6">
                    <div className="flex justify-between items-end mb-4">
                      <div className="text-4xl font-bold text-white">{localSettings.target_fps} <span className="text-lg text-gray-500 font-normal">FPS</span></div>
                       <div className="text-right">
                         <div className="text-sm font-medium text-gray-400">Monitor Refresh</div>
                         <div className="text-white font-mono">{monitorRefreshRate} Hz</div>
                       </div>
                    </div>
                    
                    <input
                      type="range"
                      min="15"
                      max={maxFps}
                      step="5"
                      value={Math.min(localSettings.target_fps, maxFps)}
                      onInput={(e) => setLocalSettings({ ...localSettings, target_fps: parseInt((e.target as HTMLInputElement).value) })}
                      onChange={(e) => setLocalSettings({ ...localSettings, target_fps: parseInt(e.target.value) })}
                      className="w-full h-3 bg-gray-700 rounded-lg cursor-pointer accent-green-500 hover:accent-green-400"
                      style={{ WebkitAppRegion: 'no-drag' } as any} onMouseDown={(e) => e.stopPropagation()}


                    />
                    <div className="flex justify-between text-xs text-gray-500 mt-2 font-mono">
                      <span>15 FPS</span>
                      <span>MAX {maxFps} FPS</span>
                    </div>
                  </div>
                  
                  <div className="bg-gray-700/30 rounded p-3 text-sm text-gray-400">
                    <span className="text-green-400 font-bold">Tip:</span> Higher FPS requires more CPU/GPU. matching your monitor's refresh rate (e.g. 60Hz) is usually optimal.
                  </div>
               </SectionCard>
            </div>
          )}

          {/* TAB: ADVANCED */}
          {activeTab === "advanced" && (
            <div className="space-y-6 animate-fadeIn">
              <SectionCard title="Startup Behavior">
                 <label className="flex items-center justify-between cursor-pointer">
                    <div>
                      <span className="font-medium text-gray-200 block">Remember Last Region</span>
                      <span className="text-xs text-gray-500">Restore size and position on launch</span>
                    </div>
                    <div className={`w-12 h-6 rounded-full p-1 transition-colors ${localSettings.remember_last_region ? 'bg-blue-600' : 'bg-gray-600'}`}>
                      <input 
                        type="checkbox" 
                        className="hidden" 
                        checked={localSettings.remember_last_region}
                        onChange={(e) => setLocalSettings({ ...localSettings, remember_last_region: e.target.checked })}
                      />
                      <div className={`w-4 h-4 rounded-full bg-white transition-transform ${localSettings.remember_last_region ? 'translate-x-6' : ''}`}></div>
                    </div>
                  </label>
              </SectionCard>

              <SectionCard title="Logging & Troubleshooting">
                <div className="grid grid-cols-2 gap-4 mb-4">
                   <div>
                     <label className="block text-sm text-gray-400 mb-2">Log Level</label>
                     <select
                        value={localSettings.log_level}
                        onChange={(e) => setLocalSettings({ ...localSettings, log_level: e.target.value })}
                        className="w-full h-10 bg-gray-900 border border-gray-700 rounded-lg px-3 text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent"
                      >
                        <option value="Off">Off</option>
                        <option value="Error">Error (Recommended)</option>
                        <option value="Warn">Warn</option>
                        <option value="Info">Info</option>
                        <option value="Debug">Debug</option>
                      </select>
                   </div>
                   
                   <div>
                      <div className="flex items-center justify-between mb-2">
                        <label className="text-sm text-gray-400">Retention</label>
                        <label className="flex items-center gap-2 cursor-pointer text-xs text-blue-400 hover:text-blue-300">
                           <input
                            type="checkbox"
                            checked={localSettings.log_to_file}
                            onChange={(e) => setLocalSettings({ ...localSettings, log_to_file: e.target.checked })}
                            className="rounded bg-gray-700 border-gray-600 text-blue-600 focus:ring-offset-gray-900"
                           />
                           Save to File
                        </label>
                      </div>
                      <div className="relative">
                        <input
                          type="number"
                          min="1"
                          disabled={!localSettings.log_to_file}
                          value={localSettings.log_retention_days}
                          onChange={(e) => setLocalSettings({ ...localSettings, log_retention_days: parseInt(e.target.value) })}
                          className="w-full bg-gray-900 border border-gray-700 rounded-lg p-2 text-white disabled:opacity-50 disabled:cursor-not-allowed focus:ring-2 focus:ring-blue-500 focus:border-transparent"
                        />
                         <span className="absolute right-3 top-2 text-gray-500 text-sm pointer-events-none">Days</span>
                      </div>
                   </div>
                </div>
                
                <div className="flex gap-3 mt-4">
                    <button onClick={() => invoke("open_logs_folder")} className="px-4 py-2 bg-gray-700 hover:bg-gray-600 rounded-lg text-sm text-white transition-colors">
                      Open Log Folder
                    </button>
                    <button onClick={async () => {
                         const confirmed = await ask(`Delete logs older than ${localSettings.log_retention_days} days?`, { title: 'Clear Old Logs', kind: 'warning' });
                         if (confirmed) {
                           const count = await invoke("clear_old_logs", { keepDays: localSettings.log_retention_days });
                           setToastMessage(`Deleted ${count} logs`);
                         }
                    }} className="px-4 py-2 bg-red-900/50 hover:bg-red-900/80 text-red-100 rounded-lg text-sm transition-colors border border-red-800">
                      Clear Old Logs
                    </button>
                </div>


              </SectionCard>
            </div>
          )}

          {/* TAB: PROFILES */}
          {activeTab === "profiles" && (
            <div className="space-y-6 animate-fadeIn">
              <div className="bg-purple-500/10 border border-purple-500/20 rounded-xl p-4">
                <div className="flex items-center gap-3 mb-2">
                  <svg className="w-6 h-6 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20 7l-8-4-8 4m16 0l-8 4m8-4v10l-8 4m0-10L4 7m8 4v10M4 7v10l8 4" />
                  </svg>
                  <div className="flex-1">
                    <h3 className="font-bold text-white">Capture Profiles</h3>
                    <p className="text-sm text-gray-400">Optimize window settings for different apps</p>
                  </div>
                  <a 
                    href="https://github.com/salihcantekin/RustFrame/tree/master/resources/profiles" 
                    target="_blank" 
                    rel="noopener noreferrer"
                    className="text-blue-400 hover:text-blue-300 text-sm flex items-center gap-1"
                  >
                    <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
                      <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/>
                    </svg>
                    View on GitHub
                  </a>
                </div>
              </div>

              <SectionCard title="Profile Selection">
                <div className="space-y-4">
                  <div>
                    <label className="text-sm text-gray-400 block mb-2">Select Profile</label>
                    <select
                      value={selectedProfileForDetails}
                      onChange={async (e) => {
                        const profileId = e.target.value;
                        setSelectedProfileForDetails(profileId);
                        if (profileId) {
                          try {
                            const details = await invoke("get_profile_details", { profileId });
                            setProfileDetails(details);
                          } catch (error) {
                            console.error("Failed to load profile details:", error);
                            setProfileDetails(null);
                          }
                        } else {
                          setProfileDetails(null);
                        }
                      }}
                      className="w-full px-4 py-3 bg-gray-900 border border-gray-700 rounded-lg focus:ring-2 focus:ring-blue-500 focus:outline-none text-white"
                    >
                      <option value="">-- Select a profile --</option>
                      <option value="discord">Discord</option>
                      <option value="googlemeet">Google Meet</option>
                      <option value="teams">Microsoft Teams</option>
                      <option value="zoom">Zoom</option>
                    </select>
                  </div>

                  {selectedProfileForDetails && (
                    <div className="flex gap-2">
                      <button
                        onClick={async () => {
                          try {
                            await invoke("download_profile", { profileId: selectedProfileForDetails });
                            setToastMessage(`Profile '${selectedProfileForDetails}' downloaded!`);
                            setTimeout(() => setToastMessage(null), 3000);
                          } catch (error) {
                            console.error("Download failed:", error);
                            setToastMessage(`Error: ${error}`);
                            setTimeout(() => setToastMessage(null), 5000);
                          }
                        }}
                        className="flex-1 px-4 py-2 bg-blue-600 hover:bg-blue-700 text-white rounded-lg transition-colors flex items-center justify-center gap-2"
                      >
                        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                        </svg>
                        Download
                      </button>
                      <button
                        onClick={async () => {
                          setProfilesLoading(true);
                          try {
                            const data = await invoke("check_profile_updates");
                            setProfileVersionData(data);
                            setToastMessage("Profile version checked!");
                            setTimeout(() => setToastMessage(null), 3000);
                          } catch (error) {
                            console.error("Failed to check updates:", error);
                            setToastMessage(`Error: ${error}`);
                            setTimeout(() => setToastMessage(null), 5000);
                          } finally {
                            setProfilesLoading(false);
                          }
                        }}
                        disabled={profilesLoading}
                        className="flex-1 px-4 py-2 bg-green-600 hover:bg-green-700 disabled:bg-gray-700 text-white rounded-lg transition-colors flex items-center justify-center gap-2"
                      >
                        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                        </svg>
                        Check Update
                      </button>
                      <button
                        onClick={async () => {
                          if (await ask(`Delete profile '${selectedProfileForDetails}'?`, { kind: "warning" })) {
                            try {
                              await invoke("delete_profile", { profileId: selectedProfileForDetails });
                              setToastMessage(`Profile '${selectedProfileForDetails}' deleted!`);
                              setSelectedProfileForDetails("");
                              setProfileDetails(null);
                              setTimeout(() => setToastMessage(null), 3000);
                            } catch (error) {
                              console.error("Delete failed:", error);
                              setToastMessage(`Error: ${error}`);
                              setTimeout(() => setToastMessage(null), 5000);
                            }
                          }
                        }}
                        className="px-4 py-2 bg-red-600 hover:bg-red-700 text-white rounded-lg transition-colors"
                      >
                        Delete
                      </button>
                    </div>
                  )}

                  {profileVersionData && (
                    <div className="p-3 bg-green-500/10 border border-green-500/20 rounded-lg text-sm text-green-400">
                      ✓ Latest version: {profileVersionData.version} ({profileVersionData.last_updated})
                    </div>
                  )}
                </div>
              </SectionCard>

              {profileDetails && (
                <>
                  <SectionCard title={profileDetails.name}>
                    <div className="space-y-4">
                      <p className="text-gray-300">{profileDetails.description}</p>
                      
                      {profileDetails.settings.explanation && (
                        <div className="p-4 bg-blue-500/10 border border-blue-500/20 rounded-lg">
                          <div className="flex items-start gap-2">
                            <svg className="w-5 h-5 text-blue-400 flex-shrink-0 mt-0.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                            </svg>
                            <div>
                              <h4 className="font-bold text-blue-300 mb-1">Why these settings?</h4>
                              <p className="text-sm text-gray-300 leading-relaxed">{profileDetails.settings.explanation}</p>
                            </div>
                          </div>
                        </div>
                      )}

                      <div className="space-y-2">
                        <h4 className="font-bold text-white text-sm">Profile Settings:</h4>
                        <div className="bg-gray-900/50 rounded-lg border border-gray-700 divide-y divide-gray-800">
                          {Object.entries(profileDetails.settings).map(([key, value]: [string, any]) => {
                            if (key === "name" || key === "description" || key === "explanation") return null;
                            
                            const tooltips: {[k: string]: string} = {
                              "winapi_destination_overlapped": "WS_OVERLAPPEDWINDOW: Makes window appear as a standard application window with title bar and borders",
                              "winapi_destination_appwindow": "WS_EX_APPWINDOW: Forces window to appear in taskbar and Alt-Tab switcher",
                              "winapi_destination_toolwindow": "WS_EX_TOOLWINDOW: Hides window from taskbar, appears as a tool window",
                              "winapi_destination_layered": "WS_EX_LAYERED: Enables transparency and alpha blending support",
                              "winapi_destination_alpha": "Window opacity level (0-255). 255 = fully opaque",
                              "winapi_destination_topmost": "WS_EX_TOPMOST: Keeps window above all non-topmost windows",
                              "winapi_destination_click_through": "WS_EX_TRANSPARENT: Makes window click-through (mouse events pass to windows below)",
                              "winapi_destination_noactivate": "WS_EX_NOACTIVATE: Prevents window from stealing focus when clicked",
                              "winapi_destination_hide_taskbar_after_ms": "Milliseconds to wait before hiding window from taskbar (null = never hide)"
                            };

                            return (
                              <div key={key} className="flex items-center justify-between p-3 group hover:bg-gray-800/50 transition-colors">
                                <div className="flex items-center gap-2 flex-1">
                                  <span className="text-gray-400 text-sm font-mono">{key}</span>
                                  {tooltips[key] && (
                                    <div className="relative group/tooltip">
                                      <svg className="w-4 h-4 text-gray-600 hover:text-blue-400 cursor-help" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                                      </svg>
                                      <div className="absolute left-0 top-6 w-64 p-2 bg-gray-950 border border-gray-700 rounded-lg shadow-xl z-50 opacity-0 group-hover/tooltip:opacity-100 pointer-events-none transition-opacity text-xs text-gray-300">
                                        {tooltips[key]}
                                      </div>
                                    </div>
                                  )}
                                </div>
                                <span className="text-white font-mono text-sm font-semibold">{JSON.stringify(value)}</span>
                              </div>
                            );
                          })}
                        </div>
                      </div>
                    </div>
                  </SectionCard>
                </>
              )}
            </div>
          )}

           {/* TAB: ABOUT */}
          {activeTab === "about" && (
             <div className="space-y-6 animate-fadeIn">
                <div className="text-center py-8">
                   <div className="w-20 h-20 bg-gradient-to-br from-blue-500 to-purple-600 rounded-2xl mx-auto flex items-center justify-center shadow-lg mb-4">
                      <img src="/icon.png" className="w-12 h-12 drop-shadow-md" alt="Logo" />
                   </div>
                   <h2 className="text-3xl font-bold text-white mb-2">RustFrame</h2>
                   <p className="text-gray-400">Modern High-Performance Screen Capture</p>
                   <p className="text-gray-500 text-sm mt-2">v{appVersion} • {platformInfo.os_type} {platformInfo.os_version !== "Unknown" ? platformInfo.os_version : ""}</p>
                </div>

                <div className="grid grid-cols-2 gap-4">
                   <button onClick={handleExportSettings} className="p-4 bg-gray-800 hover:bg-gray-700 rounded-xl border border-gray-600 transition-colors flex flex-col items-center gap-2">
                      <span className="text-2xl">📤</span>
                      <span className="font-medium text-white">Export Settings</span>
                   </button>
                   <button onClick={handleImportSettings} className="p-4 bg-gray-800 hover:bg-gray-700 rounded-xl border border-gray-600 transition-colors flex flex-col items-center gap-2">
                       <span className="text-2xl">📥</span>
                      <span className="font-medium text-white">Import Settings</span>
                   </button>
                </div>

                <div className="bg-gray-800/50 rounded-xl p-4 text-center">
                   <button onClick={() => invoke("open_settings_folder")} className="text-blue-400 hover:text-blue-300 hover:underline text-sm">
                      Open Configuration Folder
                   </button>
                </div>
             </div>
          )}

        </div>

        {/* Footer Actions */}
        <div className="px-6 py-5 border-t border-gray-800 bg-gray-900 flex justify-end gap-3 z-10">
          <button
            onClick={onClose}
            className="px-6 py-2.5 rounded-xl text-gray-400 hover:text-white hover:bg-gray-800 transition-colors font-medium border border-transparent hover:border-gray-700"
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            className="px-8 py-2.5 rounded-xl bg-gradient-to-r from-blue-600 to-purple-600 hover:from-blue-500 hover:to-purple-500 text-white font-bold shadow-lg shadow-blue-500/20 transform transition-all hover:scale-[1.02] active:scale-[0.98]"
          >
            Apply Changes
          </button>
        </div>
      </div>
    </div>
  );
}

export default SettingsDialog;
