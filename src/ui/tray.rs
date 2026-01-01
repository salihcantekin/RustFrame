// ui/tray.rs - System Tray Icon Implementation
//
// Provides system tray functionality for RustFrame with menu options.

use std::sync::{Arc, Mutex};
use anyhow::Result;
use log::{error, info};
use tray_icon::{
    TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};
use winit::event_loop::EventLoopProxy;
use crate::UserEvent;

/// System tray icon manager
pub struct SystemTray {
    /// The tray icon
    tray_icon: Option<TrayIcon>,
    /// Menu items for state tracking
    menu_items: TrayMenuItems,
    /// Event loop proxy for sending events
    event_proxy: Arc<Mutex<Option<EventLoopProxy<UserEvent>>>>,
}

/// Menu items for the tray icon
#[derive(Default)]
struct TrayMenuItems {
    /// Start/Stop capture menu item
    capture_toggle: Option<MenuItem>,
    /// Show/Hide overlay menu item
    overlay_toggle: Option<MenuItem>,
    /// Settings menu item
    settings: Option<MenuItem>,
    /// About menu item
    about: Option<MenuItem>,
    /// Exit menu item
    exit: Option<MenuItem>,
}

impl Default for SystemTray {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemTray {
    /// Create a new system tray
    pub fn new() -> Self {
        Self {
            tray_icon: None,
            menu_items: TrayMenuItems::default(),
            event_proxy: Arc::new(Mutex::new(None)),
        }
    }
    
    /// Initialize the tray icon
    pub fn init(&mut self, event_proxy: EventLoopProxy<UserEvent>) -> Result<()> {
        info!("Initializing system tray icon");
        
        // Store event proxy
        *self.event_proxy.lock().unwrap() = Some(event_proxy);
        
        // Create menu items
        let capture_toggle = MenuItem::new("Start Capture", true, None);
        let overlay_toggle = MenuItem::new("Show Overlay", true, None);
        let settings = MenuItem::new("Settings...", true, None);
        let about = MenuItem::new("About RustFrame", true, None);
        let exit = MenuItem::new("Exit", true, None);
        
        // Create main menu
        let menu = Menu::new();
        menu.append_items(&[
            &capture_toggle.clone(),
            &overlay_toggle.clone(), 
            &PredefinedMenuItem::separator(),
            &settings.clone(),
            &about.clone(),
            &PredefinedMenuItem::separator(),
            &exit.clone(),
        ])?;
        
        // Create tray icon
        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("RustFrame - Screen Capture Tool")
            .with_icon(self.create_tray_icon()?)
            .build()?;
        
        // Store references
        self.menu_items.capture_toggle = Some(capture_toggle);
        self.menu_items.overlay_toggle = Some(overlay_toggle);
        self.menu_items.settings = Some(settings);
        self.menu_items.about = Some(about);
        self.menu_items.exit = Some(exit);
        self.tray_icon = Some(tray_icon);
        
        info!("System tray icon initialized successfully");
        Ok(())
    }
    
    /// Handle tray icon events
    pub fn handle_event(&mut self, event: TrayIconEvent) {
        match event {
            TrayIconEvent::Click { button, .. } => {
                match button {
                    tray_icon::ClickType::Left => {
                        self.send_event(UserEvent::ToggleOverlay);
                    },
                    tray_icon::ClickType::Right => {
                        // Right click shows context menu automatically
                    },
                    tray_icon::ClickType::Double => {
                        self.send_event(UserEvent::ToggleCapture);
                    },
                }
            },
            _ => {},
        }
    }
    
    /// Handle menu events
    pub fn handle_menu_event(&mut self, event: MenuEvent) {
        let menu_id = event.id();
        
        if let Some(capture_toggle) = &self.menu_items.capture_toggle {
            if menu_id == capture_toggle.id() {
                self.send_event(UserEvent::ToggleCapture);
                return;
            }
        }
        
        if let Some(overlay_toggle) = &self.menu_items.overlay_toggle {
            if menu_id == overlay_toggle.id() {
                self.send_event(UserEvent::ToggleOverlay);
                return;
            }
        }
        
        if let Some(settings) = &self.menu_items.settings {
            if menu_id == settings.id() {
                self.send_event(UserEvent::ShowSettings);
                return;
            }
        }
        
        if let Some(about) = &self.menu_items.about {
            if menu_id == about.id() {
                self.send_event(UserEvent::ShowAbout);
                return;
            }
        }
        
        if let Some(exit) = &self.menu_items.exit {
            if menu_id == exit.id() {
                self.send_event(UserEvent::Exit);
                return;
            }
        }
    }
    
    /// Send event to the main event loop
    fn send_event(&self, event: UserEvent) {
        if let Some(proxy) = self.event_proxy.lock().unwrap().as_ref() {
            if let Err(e) = proxy.send_event(event) {
                error!("Failed to send tray event: {}", e);
            }
        }
    }
    
    /// Create the tray icon
    fn create_tray_icon(&self) -> Result<tray_icon::Icon> {
        // Simple 16x16 icon with 'R' pattern
        let icon_width = 16;
        let icon_height = 16;
        let mut icon_data = vec![0u8; (icon_width * icon_height * 4) as usize];
        
        for y in 0..icon_height {
            for x in 0..icon_width {
                let offset = ((y * icon_width + x) * 4) as usize;
                
                if x == 0 || x == icon_width - 1 || y == 0 || y == icon_height - 1 {
                    // Border - white
                    icon_data[offset] = 255; icon_data[offset + 1] = 255; 
                    icon_data[offset + 2] = 255; icon_data[offset + 3] = 255;
                } else if (x >= 2 && x <= 4 && y >= 2 && y <= 13) ||
                         (x >= 2 && x <= 8 && y >= 2 && y <= 3) ||
                         (x >= 2 && x <= 6 && y >= 7 && y <= 8) ||
                         (x >= 6 && x <= 8 && y >= 4 && y <= 6) ||
                         (x >= 6 && x <= 8 && y >= 9 && y <= 13) {
                    // 'R' letter - blue
                    icon_data[offset] = 50; icon_data[offset + 1] = 100;
                    icon_data[offset + 2] = 255; icon_data[offset + 3] = 255;
                } else {
                    // Background - transparent
                    icon_data[offset] = 0; icon_data[offset + 1] = 0;
                    icon_data[offset + 2] = 0; icon_data[offset + 3] = 0;
                }
            }
        }
        
        Ok(tray_icon::Icon::from_rgba(icon_data, icon_width, icon_height)?)
    }
}
