import { useState, useEffect, useRef } from "react";
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

type TabType = "capture" | "mouse" | "visual" | "region" | "performance" | "advanced" | "about";

function SettingsDialog({ 
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
  const [activeTab, setActiveTab] = useState<TabType>("capture");
  const [localSettings, setLocalSettings] = useState<Settings>(settings);
  const [localRegion, setLocalRegion] = useState<CaptureRegion>(captureRegion);
  const [localMonitor, setLocalMonitor] = useState<number>(selectedMonitor);
  const [previewEnabled, setPreviewEnabled] = useState(false);
  const [positionPreset, setPositionPreset] = useState<string>("center");
  const [isSyncingFromBackend, setIsSyncingFromBackend] = useState(false);
  const [devMode, setDevMode] = useState(false);
  const [toastMessage, setToastMessage] = useState<string | null>(null);

  // Helper for color conversion
  const rgbaToHex = (rgba: [number, number, number, number]): string => {
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

  // Preview border management (Same logic as original)
  useEffect(() => {
    if (previewEnabled) {
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
    { id: "advanced", label: "Advanced", icon: "🔧" },
    { id: "about", label: "About", icon: "ℹ️" },
  ];

  const SectionCard = ({ title, children, className = "" }: { title: string; children: React.ReactNode; className?: string }) => (
    <div className={`bg-gray-800/50 rounded-xl p-5 border border-gray-700 shadow-sm ${className}`}>
      <h3 className="text-lg font-bold text-gray-200 mb-4">{title}</h3>
      {children}
    </div>
  );

  return (
    <div className="fixed inset-0 bg-black/80 backdrop-blur-sm flex items-center justify-center z-50 animate-fadeIn">
      {/* Toast */}
      {toastMessage && (
        <div className="fixed top-6 right-6 z-[70] bg-gray-900 border border-gray-700 rounded-lg px-4 py-3 shadow-2xl animate-slide-in flex items-center gap-2">
          <div className="w-2 h-2 rounded-full bg-green-500"></div>
          <p className="text-white text-sm font-medium">{toastMessage}</p>
        </div>
      )}
      
      <div className="bg-gray-900 rounded-2xl shadow-2xl w-full max-w-4xl max-h-[85vh] flex flex-col border border-gray-700 overflow-hidden">
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
        <div className="px-2 pt-2 border-b border-gray-800 bg-gray-900 overflow-x-auto">
          <div className="flex space-x-1 min-w-max p-2">
            {tabs.map((tab) => (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`flex items-center gap-2 px-4 py-3 rounded-xl transition-all duration-200 font-medium text-sm ${
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
        <div className="flex-1 overflow-y-auto p-6 bg-gray-900/50">
          
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
                <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
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
              <div className={`transition-all duration-300 ${localSettings.capture_clicks ? 'opacity-100 max-h-[500px]' : 'opacity-40 max-h-0 overflow-hidden select-none grayscale'}`}>
                 <SectionCard title="Click Highlight Style">
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                      <div>
                        <label className="block text-sm text-gray-400 mb-2">Color</label>
                        <div className="flex items-center gap-3">
                          <input
                            type="color"
                            value={rgbaToHex(localSettings.click_highlight_color)}
                             onChange={(e) => setLocalSettings({ ...localSettings, click_highlight_color: hexToRgba(e.target.value) })}
                            className="w-12 h-12 rounded-lg bg-transparent cursor-pointer"
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
                              className="w-full h-2 bg-gray-700 rounded-lg appearance-none cursor-pointer"
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
                              className="w-full h-2 bg-gray-700 rounded-lg appearance-none cursor-pointer"
                            />
                         </div>
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
                        onChange={(e) => setLocalSettings({ ...localSettings, border_color: hexToRgba(e.target.value) })}
                        className="w-full h-10 rounded-lg cursor-pointer bg-gray-700"
                      />
                    </div>
                    <div>
                      <label className="block text-sm text-gray-400 mb-2">Border Width: {localSettings.border_width}px</label>
                       <input
                        type="range"
                        min="1"
                        max="20"
                        value={localSettings.border_width}
                        onChange={(e) => setLocalSettings({ ...localSettings, border_width: parseInt(e.target.value) })}
                        className="w-full h-2 bg-gray-700 rounded-lg appearance-none cursor-pointer mt-2"
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
                      {mon.name} ({mon.width}x{mon.height}) {mon.is_primary ? "⭐ Primary" : ""}
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
                      onChange={(e) => setLocalSettings({ ...localSettings, target_fps: parseInt(e.target.value) })}
                      className="w-full h-3 bg-gray-700 rounded-lg appearance-none cursor-pointer accent-green-500 hover:accent-green-400"
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
                        className="w-full bg-gray-900 border border-gray-700 rounded-lg p-2 text-white"
                      >
                        <option value="Off">Off</option>
                        <option value="Error">Error (Recommended)</option>
                        <option value="Warn">Warn</option>
                        <option value="Info">Info</option>
                        <option value="Debug">Debug</option>
                      </select>
                   </div>
                   
                   <div>
                      <label className="block text-sm text-gray-400 mb-2 cursor-pointer flex items-center gap-2">
                        <input
                          type="checkbox"
                          checked={localSettings.log_to_file}
                          onChange={(e) => setLocalSettings({ ...localSettings, log_to_file: e.target.checked })}
                          className="w-4 h-4 rounded bg-gray-700 border-gray-600 text-blue-600"
                        />
                        Save to File
                      </label>
                      <input
                        type="number"
                        placeholder="Days"
                        min="1"
                        disabled={!localSettings.log_to_file}
                        value={localSettings.log_retention_days}
                        onChange={(e) => setLocalSettings({ ...localSettings, log_retention_days: parseInt(e.target.value) })}
                        className="w-full bg-gray-900 border border-gray-700 rounded-lg p-2 text-white disabled:opacity-50"
                      />
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

           {/* TAB: ABOUT */}
          {activeTab === "about" && (
             <div className="space-y-6 animate-fadeIn">
                <div className="text-center py-8">
                   <div className="w-20 h-20 bg-gradient-to-br from-blue-500 to-purple-600 rounded-2xl mx-auto flex items-center justify-center shadow-lg mb-4">
                      <span className="text-4xl">🌫️</span>
                   </div>
                   <h2 className="text-3xl font-bold text-white mb-2">RustFrame</h2>
                   <p className="text-gray-400">Modern High-Performance Screen Capture</p>
                   <p className="text-gray-500 text-sm mt-2">v{platformInfo.os_version} • {platformInfo.os_type}</p>
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
