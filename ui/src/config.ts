// Core application configuration
// Centralized settings for URLs, images, and other app-wide constants

export const AppConfig = {
  // Application Info
  appName: "RustFrame",
  version: "1.1.0",
  
  // Donation
  donate: {
    enabled: true,
    paypalUrl: "https://www.paypal.com/donate/?hosted_button_id=C9HWTHFJJQTJ6",
    qrCodePath: "/donate-qr.png",
    reminder: {
      showInterval: 3, // Show reminder every N capture sessions
      delayMs: 800, // Delay before showing reminder after stop capture
      storageKey: "rustframe_capture_count", // LocalStorage key for tracking
    },
  },
  
  // Links
  links: {
    github: "https://github.com/salihcantekin/RustFrame",
    documentation: "https://github.com/salihcantekin/RustFrame/blob/master/README.md",
    profilesBase: "https://raw.githubusercontent.com/salihcantekin/RustFrame/master/resources/profiles",
  },
  
  // UI Settings
  ui: {
    defaultWindowWidth: 900,
    defaultWindowHeight: 820,
  },
} as const;

export type AppConfigType = typeof AppConfig;

// ============================================================================
// Platform Types
// ============================================================================

export interface PlatformInfo {
  os_name: string;
  os_type: "windows" | "macos" | "linux";
  os_version: string;
  available_capture_methods: CaptureMethodInfo[];
  all_platforms_capture_methods: PlatformCaptureMethodInfo[];
  capabilities: PlatformCapabilities;
}

export interface CaptureMethodInfo {
  id: string;
  name: string;
  description: string;
  recommended: boolean;
  hardware_accelerated: boolean;
}

export interface PlatformCaptureMethodInfo {
  platform_name: string;
  platform_type: string;
  id: string;
  name: string;
  description: string;
  recommended: boolean;
  hardware_accelerated: boolean;
}

export interface PlatformCapabilities {
  supports_custom_rendering: boolean;
  supports_transparency: boolean;
  supports_hardware_acceleration: boolean;
  has_winapi_options: boolean;
}

