import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Settings } from "../App";

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

interface WindowExclusionTabProps {
  settings: Settings;
  onSettingsChange: (settings: Settings) => void;
}

export function WindowExclusionTab({ settings, onSettingsChange }: WindowExclusionTabProps) {
  const [availableApps, setAvailableApps] = useState<AvailableApp[]>([]);
  const [loading, setLoading] = useState(false);
  const [filterMode, setFilterMode] = useState<FilterMode>("apps");
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedItems, setSelectedItems] = useState<Set<string>>(new Set());

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

  const handleModeChange = (mode: "None" | "Include" | "Exclude") => {
    const newSettings = {
      ...settings,
      window_filter: {
        ...settings.window_filter,
        mode,
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
    
    selectedItems.forEach(id => {
      if (filterMode === "apps") {
        // Add all windows from this app
        const app = availableApps.find(a => a.bundle_id === id);
        if (app) {
          app.windows.forEach(window => {
            if (!excluded_windows.some(w => w.app_id === app.bundle_id && w.window_name === window.title)) {
              excluded_windows.push({
                app_id: app.bundle_id,
                window_name: window.title,
              });
            }
          });
        }
      } else {
        // Add specific window
        const [bundleId, windowTitle] = id.split(":::");
        if (!excluded_windows.some(w => w.app_id === bundleId && w.window_name === windowTitle)) {
          excluded_windows.push({
            app_id: bundleId,
            window_name: windowTitle,
          });
        }
      }
    });

    const newSettings = {
      ...settings,
      window_filter: {
        ...settings.window_filter,
        excluded_windows,
      },
    };
    onSettingsChange(newSettings);
    setSelectedItems(new Set());
  };

  const handleRemoveItem = (index: number) => {
    const excluded_windows = [...settings.window_filter.excluded_windows];
    excluded_windows.splice(index, 1);
    
    const newSettings = {
      ...settings,
      window_filter: {
        ...settings.window_filter,
        excluded_windows,
      },
    };
    onSettingsChange(newSettings);
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

      {/* Mode Selection */}
      <div className="bg-gray-800/50 rounded-xl p-4 border border-gray-700">
        <h4 className="text-md font-semibold text-gray-200 mb-3">Capture Mode</h4>
        <div className="flex gap-3">
          <button
            onClick={() => handleModeChange("None")}
            className={`flex-1 px-4 py-3 rounded-lg border transition-all ${
              settings.window_filter.mode === "None"
                ? "bg-blue-600 border-blue-500 text-white shadow-lg"
                : "bg-gray-700 border-gray-600 text-gray-300 hover:bg-gray-600"
            }`}
          >
            <div className="font-semibold">Capture All</div>
            <div className="text-xs opacity-80">No restrictions</div>
          </button>
          <button
            onClick={() => handleModeChange("Exclude")}
            className={`flex-1 px-4 py-3 rounded-lg border transition-all ${
              settings.window_filter.mode === "Exclude"
                ? "bg-red-600 border-red-500 text-white shadow-lg"
                : "bg-gray-700 border-gray-600 text-gray-300 hover:bg-gray-600"
            }`}
          >
            <div className="font-semibold">Exclude Windows</div>
            <div className="text-xs opacity-80">Hide specific windows</div>
          </button>
          <button
            onClick={() => handleModeChange("Include")}
            className={`flex-1 px-4 py-3 rounded-lg border transition-all ${
              settings.window_filter.mode === "Include"
                ? "bg-green-600 border-green-500 text-white shadow-lg"
                : "bg-gray-700 border-gray-600 text-gray-300 hover:bg-gray-600"
            }`}
          >
            <div className="font-semibold">Include Only</div>
            <div className="text-xs opacity-80">Show only selected</div>
          </button>
        </div>
      </div>

      {/* Auto-exclude preview */}
      <div className="bg-gray-800/50 rounded-xl p-4 border border-gray-700">
        <label className="flex items-center gap-3 cursor-pointer">
          <input
            type="checkbox"
            checked={settings.window_filter.auto_exclude_preview}
            onChange={(e) => {
              const newSettings = {
                ...settings,
                window_filter: {
                  ...settings.window_filter,
                  auto_exclude_preview: e.target.checked,
                },
              };
              onSettingsChange(newSettings);
            }}
            className="w-5 h-5 accent-blue-500"
          />
          <div>
            <div className="font-semibold text-gray-200">Auto-exclude Preview Window</div>
            <div className="text-sm text-gray-400">Prevents "Infinity Mirror" effect during capture</div>
          </div>
        </label>
      </div>

      {/* Current List (moved up for visibility) */}
      {settings.window_filter.excluded_windows.length > 0 && (
        <div className="bg-gray-800/60 rounded-xl p-4 border border-gray-700 space-y-3">
          <div className="flex items-center justify-between">
            <div>
              <h4 className="text-sm font-semibold text-gray-200">Current Selection</h4>
              <p className="text-xs text-gray-400">
                {settings.window_filter.excluded_windows.length} item{settings.window_filter.excluded_windows.length !== 1 ? "s" : ""} selected
              </p>
            </div>
            <button
              onClick={() => {
                const newSettings = {
                  ...settings,
                  window_filter: {
                    ...settings.window_filter,
                    excluded_windows: [],
                  },
                };
                onSettingsChange(newSettings);
              }}
              className="text-xs text-red-400 hover:text-red-300 font-medium px-2 py-1 rounded hover:bg-red-900/20"
            >
              Clear All
            </button>
          </div>
          <div className="flex flex-wrap gap-2 max-h-32 overflow-y-auto">
            {settings.window_filter.excluded_windows.map((item, index) => (
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
      {settings.window_filter.mode !== "None" && (
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
