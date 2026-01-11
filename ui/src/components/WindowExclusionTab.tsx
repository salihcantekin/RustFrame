import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Settings } from "../App";
import { PlatformInfo } from "../config";

interface AvailableWindow {
  id: number;
  title: string;
}

interface AvailableApp {
  bundle_id: string;
  app_name: string;
  windows: AvailableWindow[];
}

type FilterMode = "apps" | "windows";

type WindowFilterMode = "none" | "exclude_list" | "include_only";

interface WindowExclusionTabProps {
  settings: Settings;
  onSettingsChange: (settings: Settings) => void;
  platformInfo: PlatformInfo;
}

export function WindowExclusionTab({ settings, onSettingsChange, platformInfo }: WindowExclusionTabProps) {
  const [availableApps, setAvailableApps] = useState<AvailableApp[]>([]);
  const [loading, setLoading] = useState(false);
  const [filterMode, setFilterMode] = useState<FilterMode>("apps");
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedItems, setSelectedItems] = useState<Set<string>>(new Set());

  // Force "none" mode on Windows since window exclusion is not supported
  useEffect(() => {
    if (platformInfo.os_type === "windows" && settings.window_filter.mode !== "none") {
      console.log("Windows detected, forcing window_filter mode to 'none'");
      const newSettings = {
        ...settings,
        window_filter: {
          ...settings.window_filter,
          mode: "none" as WindowFilterMode,
          auto_exclude_preview: true,
        },
      };
      onSettingsChange(newSettings);
    }
  }, [platformInfo.os_type]); // Only run when platform changes, not on every settings change

  // Hide entire tab on Windows
  if (platformInfo.os_type === "windows") {
    return (
      <div className="space-y-6 animate-fadeIn">
        <div className="bg-yellow-900/20 border border-yellow-500/30 rounded-xl p-6 flex items-start gap-4">
          <div className="text-yellow-500 mt-1">
            <svg xmlns="http://www.w3.org/2000/svg" className="h-6 w-6" viewBox="0 0 20 20" fill="currentColor">
              <path fillRule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z" clipRule="evenodd" />
            </svg>
          </div>
          <div className="flex-1">
            <h3 className="text-lg font-semibold text-yellow-200 mb-2">Window Exclusion Not Available on Windows</h3>
            <p className="text-sm text-yellow-300/90 mb-3">
              Due to Windows OS limitations, selective window exclusion is not currently supported. 
              RustFrame will capture all content within the selected screen area.
            </p>
            <div className="bg-yellow-900/10 rounded-lg p-3 border border-yellow-500/20">
              <p className="text-xs text-yellow-300/80">
                <strong>Note:</strong> This feature is available on macOS and Linux. On Windows, 
                the preview window is automatically excluded from screen sharing applications.
              </p>
            </div>
          </div>
        </div>
      </div>
    );
  }

  const handleLoadWindows = async () => {
    try {
      setLoading(true);
      const apps = await invoke<AvailableApp[]>("get_available_windows");
      setAvailableApps(apps);
      setSelectedItems(new Set());
    } catch (error) {
      console.error("Failed to load available windows:", error);
      setAvailableApps([]);
    } finally {
      setLoading(false);
    }
  };

  const handleModeChange = (mode: WindowFilterMode) => {
    // On Windows, only allow "none" mode (Capture All)
    if (platformInfo.os_type === "windows" && mode !== "none") {
      return;
    }
    
    const newSettings = {
      ...settings,
      window_filter: {
        ...settings.window_filter,
        mode,
        // Always force preview exclusion on
        auto_exclude_preview: true,
      },
    };
    onSettingsChange(newSettings);
  };

  const toggleSelection = (id: string) => {
    setSelectedItems(prev => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const handleAddSelected = () => {
    const excluded_windows = [...settings.window_filter.excluded_windows];
    const included_windows = [...settings.window_filter.included_windows];
    
    selectedItems.forEach(id => {
      const pushIfMissing = (list: typeof excluded_windows, app_id: string, window_name: string) => {
        if (!list.some(w => w.app_id === app_id && w.window_name === window_name)) {
          list.push({ app_id, window_name });
        }
      };

      if (filterMode === "apps") {
        const app = availableApps.find(a => a.bundle_id === id);
        if (app) {
          app.windows.forEach(window => {
            if (settings.window_filter.mode === "include_only") {
              pushIfMissing(included_windows, app.bundle_id, window.title);
            } else {
              pushIfMissing(excluded_windows, app.bundle_id, window.title);
            }
          });
        }
      } else {
        const [bundleId, windowTitle] = id.split(":::");
        if (settings.window_filter.mode === "include_only") {
          pushIfMissing(included_windows, bundleId, windowTitle);
        } else {
          pushIfMissing(excluded_windows, bundleId, windowTitle);
        }
      }
    });

    const newSettings = {
      ...settings,
      window_filter: {
        ...settings.window_filter,
        excluded_windows,
        included_windows,
        auto_exclude_preview: true,
      },
    };
    onSettingsChange(newSettings);
    setSelectedItems(new Set());
  };

  const handleRemoveItem = (index: number) => {
    const wf = settings.window_filter;
    if (wf.mode === "include_only") {
      const included_windows = [...wf.included_windows];
      included_windows.splice(index, 1);
      onSettingsChange({
        ...settings,
        window_filter: { ...wf, included_windows, auto_exclude_preview: true },
      });
    } else {
      const excluded_windows = [...wf.excluded_windows];
      excluded_windows.splice(index, 1);
      onSettingsChange({
        ...settings,
        window_filter: { ...wf, excluded_windows, auto_exclude_preview: true },
      });
    }
  };

  const filteredApps = availableApps.filter(app =>
    app.app_name.toLowerCase().includes(searchQuery.toLowerCase()) ||
    app.bundle_id.toLowerCase().includes(searchQuery.toLowerCase())
  );

  const filteredWindows = availableApps.flatMap(app =>
    app.windows
      .filter(window =>
        window.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
        app.app_name.toLowerCase().includes(searchQuery.toLowerCase())
      )
      .map(window => ({ app, window }))
  );

  return (
    <div className="space-y-6 animate-fadeIn">
      {/* Header */}
      <div className="bg-blue-900/10 border border-blue-500/20 rounded-xl p-4">
        <h3 className="text-lg font-bold text-blue-300 mb-2">Share Content Settings</h3>
        <p className="text-gray-300 text-sm">
          Control which applications and windows can be captured during screen sharing.
        </p>
      </div>

      {/* Windows Warning */}
      {platformInfo.os === "windows" && (
        <div className="bg-yellow-900/20 border border-yellow-500/30 rounded-xl p-4 flex items-start gap-3">
          <div className="text-yellow-500 mt-1">
            <svg xmlns="http://www.w3.org/2000/svg" className="h-5 w-5" viewBox="0 0 20 20" fill="currentColor">
              <path fillRule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z" clipRule="evenodd" />
            </svg>
          </div>
          <div>
            <h4 className="text-sm font-semibold text-yellow-200">Window Exclusion Not Available on Windows</h4>
            <p className="text-xs text-yellow-300/80 mt-1">
              Due to Windows OS limitations, selective window exclusion is not currently supported. 
              RustFrame will capture all content on the selected screen area.
            </p>
          </div>
        </div>
      )}

      {/* Mode Selection */}
      <div className="bg-gray-800/50 rounded-xl p-4 border border-gray-700">
        <h4 className="text-md font-semibold text-gray-200 mb-3">Capture Mode</h4>
        <div className="flex gap-3">
          <button
            onClick={() => handleModeChange("none")}
            disabled={platformInfo.os === "windows" && settings.window_filter.mode !== "none"}
            className={`flex-1 px-4 py-3 rounded-lg border transition-all ${
              settings.window_filter.mode === "none"
                ? "bg-blue-600 border-blue-500 text-white shadow-lg"
                : "bg-gray-700 border-gray-600 text-gray-300 hover:bg-gray-600"
            } ${platformInfo.os === "windows" && settings.window_filter.mode !== "none" ? "opacity-50 cursor-not-allowed" : ""}`}
          >
            <div className="font-semibold">Capture All</div>
            <div className="text-xs opacity-80">No restrictions</div>
          </button>
          <button
            onClick={() => handleModeChange("exclude_list")}
            disabled={platformInfo.os === "windows"}
            className={`flex-1 px-4 py-3 rounded-lg border transition-all ${
              settings.window_filter.mode === "exclude_list"
                ? "bg-red-600 border-red-500 text-white shadow-lg"
                : "bg-gray-700 border-gray-600 text-gray-300 hover:bg-gray-600"
            } ${platformInfo.os === "windows" ? "opacity-50 cursor-not-allowed" : ""}`}
          >
            <div className="font-semibold">Exclude Windows</div>
            <div className="text-xs opacity-80">Hide specific windows</div>
            {platformInfo.os === "windows" && (
              <div className="text-xs opacity-60 mt-1">Not supported on Windows</div>
            )}
          </button>
          <button
            onClick={() => handleModeChange("include_only")}
            disabled={platformInfo.os === "windows"}
            className={`flex-1 px-4 py-3 rounded-lg border transition-all ${
              settings.window_filter.mode === "include_only"
                ? "bg-green-600 border-green-500 text-white shadow-lg"
                : "bg-gray-700 border-gray-600 text-gray-300 hover:bg-gray-600"
            } ${platformInfo.os === "windows" ? "opacity-50 cursor-not-allowed" : ""}`}
          >
            <div className="font-semibold">Include Only</div>
            <div className="text-xs opacity-80">Show only selected</div>
            {platformInfo.os === "windows" && (
              <div className="text-xs opacity-60 mt-1">Not supported on Windows</div>
            )}
          </button>
        </div>
      </div>

      {/* Current List (moved up for visibility) */}
      {((settings.window_filter.mode === "include_only" && settings.window_filter.included_windows.length > 0) ||
        (settings.window_filter.mode !== "include_only" && settings.window_filter.excluded_windows.length > 0)) && (
        <div className="bg-gray-800/60 rounded-xl p-4 border border-gray-700 space-y-3">
          <div className="flex items-center justify-between">
            <div>
              <h4 className="text-sm font-semibold text-gray-200">Current Selection</h4>
              <p className="text-xs text-gray-400">
                {settings.window_filter.mode === "include_only"
                  ? `${settings.window_filter.included_windows.length} item${settings.window_filter.included_windows.length !== 1 ? "s" : ""} selected`
                  : `${settings.window_filter.excluded_windows.length} item${settings.window_filter.excluded_windows.length !== 1 ? "s" : ""} selected`}
              </p>
            </div>
            <button
              onClick={() => {
                if (settings.window_filter.mode === "include_only") {
                  onSettingsChange({
                    ...settings,
                    window_filter: { ...settings.window_filter, included_windows: [], auto_exclude_preview: true },
                  });
                } else {
                  onSettingsChange({
                    ...settings,
                    window_filter: { ...settings.window_filter, excluded_windows: [], auto_exclude_preview: true },
                  });
                }
              }}
              className="text-xs text-red-400 hover:text-red-300 font-medium px-2 py-1 rounded hover:bg-red-900/20"
            >
              Clear All
            </button>
          </div>
          <div className="flex flex-wrap gap-2 max-h-32 overflow-y-auto">
            {(settings.window_filter.mode === "include_only"
              ? settings.window_filter.included_windows
              : settings.window_filter.excluded_windows
            ).map((item, index) => (
              <span
                key={index}
                className="flex items-center gap-2 px-3 py-1 rounded-full bg-gray-900/70 border border-gray-700 text-xs text-gray-200"
              >
                <span className="truncate max-w-[180px]" title={`${item.window_name} • ${item.app_id}`}>
                  {item.window_name}
                </span>
                <button
                  onClick={() => handleRemoveItem(index)}
                  className="text-red-400 hover:text-red-300"
                  aria-label="Remove"
                >
                  ✕
                </button>
              </span>
            ))}
          </div>
        </div>
      )}

      {/* Load & Filter Section */}
      {settings.window_filter.mode !== "none" && (
        <div className="bg-gray-800/50 rounded-xl p-4 border border-gray-700 space-y-3">
          <div className="flex items-center gap-3">
            <button
              onClick={handleLoadWindows}
              disabled={loading}
              className="px-6 py-3 bg-blue-600 hover:bg-blue-500 disabled:bg-gray-600 disabled:cursor-not-allowed rounded-lg font-semibold text-white transition-colors flex items-center gap-2"
            >
              {loading ? (
                <>
                  <svg className="animate-spin h-5 w-5" viewBox="0 0 24 24">
                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
                  </svg>
                  Loading...
                </>
              ) : (
                <>
                  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                  </svg>
                  Load Applications & Windows
                </>
              )}
            </button>

            {availableApps.length > 0 && (
              <div className="flex-1 flex items-center gap-3">
                <div className="flex bg-gray-700 rounded-lg p-1 gap-1">
                  <button
                    onClick={() => setFilterMode("apps")}
                    className={`px-4 py-2 rounded text-sm font-medium transition-colors ${
                      filterMode === "apps"
                        ? "bg-gray-600 text-white"
                        : "text-gray-400 hover:text-white"
                    }`}
                  >
                    📱 Applications ({availableApps.length})
                  </button>
                  <button
                    onClick={() => setFilterMode("windows")}
                    className={`px-4 py-2 rounded text-sm font-medium transition-colors ${
                      filterMode === "windows"
                        ? "bg-gray-600 text-white"
                        : "text-gray-400 hover:text-white"
                    }`}
                  >
                    🪟 Windows ({availableApps.reduce((sum, app) => sum + app.windows.length, 0)})
                  </button>
                </div>

                <input
                  type="text"
                  placeholder={`Search ${filterMode}...`}
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  className="flex-1 px-4 py-2 bg-gray-700 border border-gray-600 rounded-lg text-white placeholder-gray-400 focus:outline-none focus:border-blue-500"
                />
              </div>
            )}
          </div>

          {/* Selection Area */}
          {availableApps.length > 0 && (
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <div className="text-sm text-gray-400">
                  {selectedItems.size} selected
                </div>
                <button
                  onClick={handleAddSelected}
                  disabled={selectedItems.size === 0}
                  className="px-4 py-2 bg-green-600 hover:bg-green-500 disabled:bg-gray-600 disabled:cursor-not-allowed rounded-lg text-sm font-semibold text-white transition-colors"
                >
                  Add Selected ({selectedItems.size})
                </button>
              </div>

              <div className="max-h-72 overflow-y-auto bg-gray-900/50 rounded-lg border border-gray-700">
                {filterMode === "apps" ? (
                  /* Application List */
                  filteredApps.length === 0 ? (
                    <div className="text-center py-8 text-gray-400">No applications match your search</div>
                  ) : (
                    filteredApps.map((app) => (
                      <label
                        key={app.bundle_id}
                        className="flex items-start gap-3 p-3 hover:bg-gray-800/50 cursor-pointer transition-colors border-b border-gray-700 last:border-0"
                      >
                        <input
                          type="checkbox"
                          checked={selectedItems.has(app.bundle_id)}
                          onChange={() => toggleSelection(app.bundle_id)}
                          className="w-5 h-5 mt-1 accent-blue-500"
                        />
                        <div className="flex-1 min-w-0">
                          <div className="font-semibold text-gray-200 text-sm">{app.app_name}</div>
                          <div className="text-xs text-gray-400 truncate">{app.bundle_id}</div>
                          <div className="text-xs text-gray-500 mt-1">
                            {app.windows.length} window{app.windows.length !== 1 ? "s" : ""}
                          </div>
                        </div>
                      </label>
                    ))
                  )
                ) : (
                  /* Window List */
                  filteredWindows.length === 0 ? (
                    <div className="text-center py-8 text-gray-400">No windows match your search</div>
                  ) : (
                    filteredWindows.map(({ app, window }) => {
                      const id = `${app.bundle_id}:::${window.title}`;
                      return (
                        <label
                          key={id}
                          className="flex items-start gap-3 p-3 hover:bg-gray-800/50 cursor-pointer transition-colors border-b border-gray-700 last:border-0"
                        >
                          <input
                            type="checkbox"
                            checked={selectedItems.has(id)}
                            onChange={() => toggleSelection(id)}
                            className="w-5 h-5 mt-1 accent-blue-500"
                          />
                          <div className="flex-1 min-w-0">
                            <div className="font-semibold text-gray-200 truncate text-sm">{window.title}</div>
                            <div className="text-xs text-gray-400 truncate">{app.app_name}</div>
                            <div className="text-xs text-gray-500">ID: {window.id}</div>
                          </div>
                        </label>
                      );
                    })
                  )
                )}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
