import { useState, useEffect } from "react";
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

interface WindowExclusionTabProps {
  settings: Settings;
  onSettingsChange: (settings: Settings) => void;
}

export function WindowExclusionTab({ settings, onSettingsChange }: WindowExclusionTabProps) {
  const [availableApps, setAvailableApps] = useState<AvailableApp[]>([]);
  const [loading, setLoading] = useState(true);
  const [expandedApps, setExpandedApps] = useState<Set<string>>(new Set());

  // Load available windows on mount
  useEffect(() => {
    const loadWindows = async () => {
      try {
        setLoading(true);
        const apps = await invoke<AvailableApp[]>("get_available_windows");
        setAvailableApps(apps);
      } catch (error) {
        console.error("Failed to load available windows:", error);
        setAvailableApps([]);
      } finally {
        setLoading(false);
      }
    };

    loadWindows();
  }, []);

  const toggleAppExpanded = (bundleId: string) => {
    setExpandedApps(prev => {
      const next = new Set(prev);
      if (next.has(bundleId)) {
        next.delete(bundleId);
      } else {
        next.add(bundleId);
      }
      return next;
    });
  };

  const isWindowExcluded = (bundleId: string, windowId: number): boolean => {
    return settings.window_filter.excluded_windows.some(
      w => w.app_id === bundleId && w.window_name.includes(windowId.toString())
    );
  };

  const toggleWindowExclusion = (bundleId: string, windowId: number, windowTitle: string) => {
    const excluded_windows = [...settings.window_filter.excluded_windows];
    const index = excluded_windows.findIndex(
      w => w.app_id === bundleId && w.window_name.includes(windowId.toString())
    );

    if (index >= 0) {
      // Remove exclusion
      excluded_windows.splice(index, 1);
    } else {
      // Add exclusion
      excluded_windows.push({
        app_id: bundleId,
        window_name: windowTitle,
      });
    }

    const newSettings = {
      ...settings,
      window_filter: {
        ...settings.window_filter,
        excluded_windows,
      },
    };
    onSettingsChange(newSettings);
  };

  return (
    <div className="space-y-4 animate-fadeIn">
      {/* Header */}
      <div className="bg-blue-900/10 border border-blue-500/20 rounded-xl p-4">
        <h3 className="text-lg font-bold text-blue-300 mb-2">Window Exclusion</h3>
        <p className="text-gray-300 text-sm">
          Exclude windows from being captured. This prevents the "Infinity Mirror" effect when a preview window overlaps with the capture region.
        </p>
      </div>

      {/* Auto-exclude preview option */}
      <div className="bg-gray-800/50 rounded-xl p-5 border border-gray-700 shadow-sm">
        <h3 className="text-lg font-bold text-gray-200 mb-4">Auto-Exclusion</h3>
        <div className="flex items-center justify-between">
          <label className="flex items-center gap-3 cursor-pointer flex-1">
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
              className="w-4 h-4 accent-blue-500"
            />
            <span className="text-gray-300">Automatically exclude preview window during capture</span>
          </label>
        </div>
      </div>

      {/* Available windows list */}
      <div className="bg-gray-800/50 rounded-xl p-5 border border-gray-700 shadow-sm">
        <h3 className="text-lg font-bold text-gray-200 mb-4">Manual Exclusions</h3>
        
        {loading ? (
          <div className="flex items-center justify-center py-8">
            <div className="text-gray-400">Loading available windows...</div>
          </div>
        ) : availableApps.length === 0 ? (
          <div className="text-gray-400 text-center py-8">
            No running applications found. Please run some apps to see windows here.
          </div>
        ) : (
          <div className="space-y-2 max-h-96 overflow-y-auto">
            {availableApps.map((app) => (
              <div key={app.bundle_id} className="border border-gray-700 rounded-lg overflow-hidden bg-gray-900/50">
                {/* App header */}
                <button
                  onClick={() => toggleAppExpanded(app.bundle_id)}
                  className="w-full px-4 py-3 flex items-center justify-between hover:bg-gray-800/50 transition-colors text-left"
                >
                  <div className="flex items-center gap-3 flex-1">
                    <div className="text-lg">📱</div>
                    <div>
                      <div className="font-semibold text-gray-200">{app.app_name}</div>
                      <div className="text-xs text-gray-400">{app.bundle_id}</div>
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    <span className="text-xs bg-gray-700 px-2 py-1 rounded text-gray-300">
                      {app.windows.length} window{app.windows.length !== 1 ? "s" : ""}
                    </span>
                    <svg
                      className={`w-4 h-4 text-gray-400 transition-transform ${
                        expandedApps.has(app.bundle_id) ? "rotate-180" : ""
                      }`}
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 14l-7 7m0 0l-7-7m7 7V3" />
                    </svg>
                  </div>
                </button>

                {/* Windows list */}
                {expandedApps.has(app.bundle_id) && app.windows.length > 0 && (
                  <div className="border-t border-gray-700 bg-gray-900/30 py-2">
                    {app.windows.map((window) => (
                      <label
                        key={`${app.bundle_id}-${window.id}`}
                        className="flex items-center gap-3 px-4 py-2 hover:bg-gray-800/30 cursor-pointer transition-colors"
                      >
                        <input
                          type="checkbox"
                          checked={isWindowExcluded(app.bundle_id, window.id)}
                          onChange={() =>
                            toggleWindowExclusion(app.bundle_id, window.id, window.title)
                          }
                          className="w-4 h-4 accent-blue-500"
                        />
                        <div className="flex-1 min-w-0">
                          <div className="text-gray-300 text-sm truncate">{window.title}</div>
                          <div className="text-xs text-gray-500">ID: {window.id}</div>
                        </div>
                      </label>
                    ))}
                  </div>
                )}

                {/* Empty windows state */}
                {expandedApps.has(app.bundle_id) && app.windows.length === 0 && (
                  <div className="border-t border-gray-700 bg-gray-900/30 px-4 py-3 text-sm text-gray-400">
                    No windows found for this application
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Currently excluded summary */}
      {settings.window_filter.excluded_windows.length > 0 && (
        <div className="bg-amber-900/20 border border-amber-700/50 rounded-xl p-4">
          <h4 className="text-sm font-semibold text-amber-300 mb-2">
            Currently Excluding {settings.window_filter.excluded_windows.length} window{settings.window_filter.excluded_windows.length !== 1 ? "s" : ""}
          </h4>
          <div className="space-y-1 text-xs text-amber-200/80">
            {settings.window_filter.excluded_windows.slice(0, 5).map((w, i) => (
              <div key={i}>• {w.window_name}</div>
            ))}
            {settings.window_filter.excluded_windows.length > 5 && (
              <div>• +{settings.window_filter.excluded_windows.length - 5} more...</div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
