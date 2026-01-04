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
  },
  
  // Links
  links: {
    github: "https://github.com/salihcantekin/RustFrameApp",
    documentation: "https://github.com/salihcantekin/RustFrameApp",
  },
  
  // UI Settings
  ui: {
    defaultWindowWidth: 900,
    defaultWindowHeight: 820,
  },
} as const;

export type AppConfigType = typeof AppConfig;
