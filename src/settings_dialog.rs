// settings_dialog.rs - Native Windows Settings Dialog
//
// A simple Win32 dialog for adjusting capture settings.
// Uses raw Win32 API to avoid dependency conflicts with wgpu.

use crate::capture::CaptureSettings;
use log::info;
use std::cell::RefCell;

#[cfg(windows)]
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM, LRESULT},
    UI::WindowsAndMessaging::*,
    UI::Controls::*,
    Graphics::Gdi::{GetSysColorBrush, COLOR_3DFACE},
    System::LibraryLoader::GetModuleHandleW,
};

#[cfg(windows)]
use std::ffi::c_void;

// Control IDs
const ID_CHECK_CURSOR: i32 = 101;
const ID_CHECK_BORDER: i32 = 102;
const ID_CHECK_PROD_MODE: i32 = 103;
const ID_EDIT_BORDER_WIDTH: i32 = 105;
const ID_BTN_OK: i32 = 106;
const ID_BTN_CANCEL: i32 = 107;
const ID_BTN_APPLY: i32 = 108;

// Dialog dimensions
const DIALOG_WIDTH: i32 = 360;
const DIALOG_HEIGHT: i32 = 250;

// Thread-local state for dialog (safer than static mut)
thread_local! {
    static DIALOG_SETTINGS: RefCell<Option<CaptureSettings>> = RefCell::new(None);
    static SETTINGS_CHANGED: RefCell<bool> = RefCell::new(false);
    static DIALOG_HWND: RefCell<isize> = RefCell::new(0);
}

/// Show the settings dialog
/// Returns Some(CaptureSettings) if user clicked OK/Apply, None if cancelled
#[cfg(windows)]
pub fn show_settings_dialog(current_settings: &CaptureSettings) -> Option<CaptureSettings> {
    use windows::core::PCWSTR;
    
    unsafe {
        // Initialize common controls for modern visual style
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES | ICC_WIN95_CLASSES,
        };
        let _ = InitCommonControlsEx(&icc);
        
        // Store settings in thread-local state
        DIALOG_SETTINGS.with(|s| *s.borrow_mut() = Some(current_settings.clone()));
        SETTINGS_CHANGED.with(|c| *c.borrow_mut() = false);
        
        // Generate unique class name
        let class_name = wide_string(&format!("RustFrameSettings_{}", std::process::id()));
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(settings_dialog_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: GetModuleHandleW(None).unwrap().into(),
            hIcon: HICON::default(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hbrBackground: GetSysColorBrush(COLOR_3DFACE),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            hIconSm: HICON::default(),
        };
        
        RegisterClassExW(&wc);
        
        // Get screen dimensions for centering
        let screen_width = GetSystemMetrics(SM_CXSCREEN);
        let screen_height = GetSystemMetrics(SM_CYSCREEN);
        let x = (screen_width - DIALOG_WIDTH) / 2;
        let y = (screen_height - DIALOG_HEIGHT) / 2;
        
        // Create the dialog window with modern style
        let window_name = wide_string("RustFrame Settings");
        let hwnd = CreateWindowExW(
            WS_EX_DLGMODALFRAME | WS_EX_TOPMOST,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(window_name.as_ptr()),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
            x,
            y,
            DIALOG_WIDTH,
            DIALOG_HEIGHT,
            None,
            None,
            GetModuleHandleW(None).unwrap(),
            None,
        ).unwrap();
        
        // Store hwnd for reference
        DIALOG_HWND.with(|h| *h.borrow_mut() = hwnd.0 as isize);
        
        // Create controls
        create_controls(hwnd, current_settings);
        
        // Message loop - run until window is closed
        let mut msg = MSG::default();
        loop {
            let result = GetMessageW(&mut msg, None, 0, 0);
            if !result.as_bool() || result.0 == -1 {
                break;
            }
            
            // Check if dialog still exists
            if !IsWindow(hwnd).as_bool() {
                break;
            }
            
            if !IsDialogMessageW(hwnd, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        
        // Cleanup
        let _ = UnregisterClassW(PCWSTR(class_name.as_ptr()), GetModuleHandleW(None).unwrap());
        
        // Return settings if changed
        let changed = SETTINGS_CHANGED.with(|c| *c.borrow());
        if changed {
            DIALOG_SETTINGS.with(|s| s.borrow().clone())
        } else {
            None
        }
    }
}

#[cfg(windows)]
unsafe fn create_controls(hwnd: HWND, settings: &CaptureSettings) {
    use windows::core::PCWSTR;
    
    let hinstance = GetModuleHandleW(None).unwrap();
    let button_class = wide_string("BUTTON");
    let static_class = wide_string("STATIC");
    let edit_class = wide_string("EDIT");
    
    let mut y_pos = 25;
    let left_margin = 25;
    let control_width = 290;
    let control_height = 24;
    let spacing = 35;
    
    // Checkbox: Show Cursor
    let text = wide_string("Show cursor in capture");
    let check_cursor = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PCWSTR(button_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        left_margin, y_pos, control_width, control_height,
        hwnd,
        HMENU(ID_CHECK_CURSOR as *mut c_void),
        hinstance,
        None,
    ).unwrap();
    if settings.show_cursor {
        let _ = SendMessageW(check_cursor, BM_SETCHECK, WPARAM(BST_CHECKED.0 as usize), LPARAM(0));
    }
    y_pos += spacing;
    
    // Checkbox: Show Border
    let text = wide_string("Show border frame");
    let check_border = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PCWSTR(button_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        left_margin, y_pos, control_width, control_height,
        hwnd,
        HMENU(ID_CHECK_BORDER as *mut c_void),
        hinstance,
        None,
    ).unwrap();
    if settings.show_border {
        let _ = SendMessageW(check_border, BM_SETCHECK, WPARAM(BST_CHECKED.0 as usize), LPARAM(0));
    }
    y_pos += spacing;
    
    // Border width label and edit
    let text = wide_string("Border width (pixels):");
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PCWSTR(static_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE,
        left_margin, y_pos + 3, 145, control_height,
        hwnd,
        None,
        hinstance,
        None,
    );
    
    let text = wide_string(&settings.border_width.to_string());
    let _ = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        PCWSTR(edit_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(ES_NUMBER as u32 | ES_CENTER as u32),
        left_margin + 155, y_pos, 55, control_height,
        hwnd,
        HMENU(ID_EDIT_BORDER_WIDTH as *mut c_void),
        hinstance,
        None,
    );
    y_pos += spacing;
    
    // Checkbox: Production Mode
    let text = wide_string("Production mode (hide destination window)");
    let check_prod = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PCWSTR(button_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        left_margin, y_pos, control_width, control_height,
        hwnd,
        HMENU(ID_CHECK_PROD_MODE as *mut c_void),
        hinstance,
        None,
    ).unwrap();
    if settings.exclude_from_capture {
        let _ = SendMessageW(check_prod, BM_SETCHECK, WPARAM(BST_CHECKED.0 as usize), LPARAM(0));
    }
    y_pos += spacing + 15;
    
    // Buttons - centered
    let btn_width = 90;
    let btn_height = 30;
    let btn_spacing = 12;
    let total_btn_width = btn_width * 3 + btn_spacing * 2;
    let btn_start_x = (DIALOG_WIDTH - total_btn_width) / 2;
    
    let text = wide_string("OK");
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PCWSTR(button_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
        btn_start_x, y_pos, btn_width, btn_height,
        hwnd,
        HMENU(ID_BTN_OK as *mut c_void),
        hinstance,
        None,
    );
    
    let text = wide_string("Cancel");
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PCWSTR(button_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        btn_start_x + btn_width + btn_spacing, y_pos, btn_width, btn_height,
        hwnd,
        HMENU(ID_BTN_CANCEL as *mut c_void),
        hinstance,
        None,
    );
    
    let text = wide_string("Apply");
    let _ = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PCWSTR(button_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        btn_start_x + (btn_width + btn_spacing) * 2, y_pos, btn_width, btn_height,
        hwnd,
        HMENU(ID_BTN_APPLY as *mut c_void),
        hinstance,
        None,
    );
    
    // Note: SetFocus removed - Tab key navigation still works with WS_TABSTOP
}

#[cfg(windows)]
unsafe extern "system" fn settings_dialog_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let control_id = (wparam.0 & 0xFFFF) as i32;
            
            match control_id {
                ID_BTN_OK => {
                    save_settings_from_controls(hwnd);
                    SETTINGS_CHANGED.with(|c| *c.borrow_mut() = true);
                    let _ = DestroyWindow(hwnd);
                }
                ID_BTN_CANCEL => {
                    SETTINGS_CHANGED.with(|c| *c.borrow_mut() = false);
                    let _ = DestroyWindow(hwnd);
                }
                ID_BTN_APPLY => {
                    // Apply also closes the dialog and applies settings
                    save_settings_from_controls(hwnd);
                    SETTINGS_CHANGED.with(|c| *c.borrow_mut() = true);
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            SETTINGS_CHANGED.with(|c| *c.borrow_mut() = false);
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(windows)]
unsafe fn save_settings_from_controls(hwnd: HWND) {
    DIALOG_SETTINGS.with(|settings_cell| {
        let mut settings_opt = settings_cell.borrow_mut();
        if let Some(ref mut settings) = *settings_opt {
            // Read checkbox states
            if let Ok(check_cursor) = GetDlgItem(hwnd, ID_CHECK_CURSOR) {
                settings.show_cursor = SendMessageW(check_cursor, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == BST_CHECKED.0 as isize;
            }
            
            if let Ok(check_border) = GetDlgItem(hwnd, ID_CHECK_BORDER) {
                settings.show_border = SendMessageW(check_border, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == BST_CHECKED.0 as isize;
            }
            
            if let Ok(check_prod) = GetDlgItem(hwnd, ID_CHECK_PROD_MODE) {
                settings.exclude_from_capture = SendMessageW(check_prod, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == BST_CHECKED.0 as isize;
            }
            
            // Read border width
            if let Ok(edit_width) = GetDlgItem(hwnd, ID_EDIT_BORDER_WIDTH) {
                let mut buffer = [0u16; 16];
                let len = GetWindowTextW(edit_width, &mut buffer);
                if len > 0 {
                    let text: String = String::from_utf16_lossy(&buffer[..len as usize]);
                    if let Ok(width) = text.parse::<u32>() {
                        settings.border_width = width.max(1).min(50); // Clamp between 1-50
                    }
                }
            }
            
            info!("Settings saved: cursor={}, border={}, width={}, prod_mode={}", 
                  settings.show_cursor, settings.show_border, 
                  settings.border_width, settings.exclude_from_capture);
        }
    });
}

/// Convert a Rust string to a null-terminated wide string (UTF-16)
#[cfg(windows)]
fn wide_string(s: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(not(windows))]
pub fn show_settings_dialog(_current_settings: &CaptureSettings) -> Option<CaptureSettings> {
    // Settings dialog not supported on non-Windows platforms
    None
}
