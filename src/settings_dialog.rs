// settings_dialog.rs - Native Windows Settings Dialog
//
// A Win32 dialog for adjusting capture settings.
// Uses modern Windows controls with proper DPI scaling and Segoe UI font.

use crate::capture::CaptureSettings;
use log::info;
use std::cell::RefCell;

#[cfg(windows)]
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM, LRESULT},
    UI::WindowsAndMessaging::*,
    UI::Controls::*,
    Graphics::Gdi::{GetSysColorBrush, COLOR_3DFACE, CreateFontW, SelectObject, HFONT, GetDC, ReleaseDC, DeleteObject, HGDIOBJ,
        FW_NORMAL, CLEARTYPE_QUALITY, DEFAULT_CHARSET, OUT_TT_PRECIS, CLIP_DEFAULT_PRECIS, FF_SWISS},
    Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE, DWM_SYSTEMBACKDROP_TYPE, DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW},
    System::LibraryLoader::GetModuleHandleW,
};

#[cfg(windows)]
use std::ffi::c_void;

// Control IDs
const ID_CHECK_CURSOR: i32 = 101;
const ID_CHECK_BORDER: i32 = 102;
const ID_CHECK_PROD_MODE: i32 = 103;
const ID_EDIT_BORDER_WIDTH: i32 = 105;
const ID_BTN_SAVE: i32 = 106;
const ID_BTN_CANCEL: i32 = 107;

// Dialog dimensions
const DIALOG_WIDTH: i32 = 420;
const DIALOG_HEIGHT_DEV: i32 = 290;
const DIALOG_HEIGHT_PROD: i32 = 250;

// Thread-local state for dialog
thread_local! {
    static DIALOG_SETTINGS: RefCell<Option<CaptureSettings>> = RefCell::new(None);
    static SETTINGS_CHANGED: RefCell<bool> = RefCell::new(false);
    static DIALOG_HWND: RefCell<isize> = RefCell::new(0);
    static DIALOG_FONT: RefCell<isize> = RefCell::new(0);
    static DIALOG_DEV_MODE: RefCell<bool> = RefCell::new(false);
}

/// Show the settings dialog
/// Returns Some(CaptureSettings) if user clicked Save, None if cancelled
/// dev_mode: if true, shows production mode option
#[cfg(windows)]
pub fn show_settings_dialog(current_settings: &CaptureSettings, dev_mode: bool) -> Option<CaptureSettings> {
    use windows::core::PCWSTR;
    
    unsafe {
        // Store dev_mode for create_controls
        DIALOG_DEV_MODE.with(|d| *d.borrow_mut() = dev_mode);
        
        // Initialize common controls for modern visual style
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES | ICC_WIN95_CLASSES,
        };
        let _ = InitCommonControlsEx(&icc);
        
        // Store settings in thread-local state
        DIALOG_SETTINGS.with(|s| *s.borrow_mut() = Some(current_settings.clone()));
        SETTINGS_CHANGED.with(|c| *c.borrow_mut() = false);
        
        // Create modern font (Segoe UI, 10pt)
        let font_name = wide_string("Segoe UI");
        let hdc = GetDC(None);
        let font_height = -((12.0 * GetDeviceCaps(hdc, windows::Win32::Graphics::Gdi::LOGPIXELSY) as f32) / 72.0) as i32;
        let _ = ReleaseDC(None, hdc);
        
        let hfont = CreateFontW(
            font_height,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_TT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            (FF_SWISS.0 | 0) as u32,
            PCWSTR(font_name.as_ptr()),
        );
        DIALOG_FONT.with(|f| *f.borrow_mut() = hfont.0 as isize);
        
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
        let dialog_height = if dev_mode { DIALOG_HEIGHT_DEV } else { DIALOG_HEIGHT_PROD };
        let x = (screen_width - DIALOG_WIDTH) / 2;
        let y = (screen_height - dialog_height) / 2;
        
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
            dialog_height,
            None,
            None,
            GetModuleHandleW(None).unwrap(),
            None,
        ).unwrap();
        
        // Enable Windows 11 Mica backdrop effect
        let use_mica = DWMSBT_MAINWINDOW;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &use_mica as *const _ as *const c_void,
            std::mem::size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
        );
        
        // Enable dark mode
        let use_dark: i32 = 1;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &use_dark as *const _ as *const c_void,
            std::mem::size_of::<i32>() as u32,
        );
        
        // Store hwnd for reference
        DIALOG_HWND.with(|h| *h.borrow_mut() = hwnd.0 as isize);
        
        // Create controls (pass dev_mode for conditional UI)
        create_controls(hwnd, current_settings, hfont, dev_mode);
        
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
        let _ = DeleteObject(HGDIOBJ(hfont.0 as *mut c_void));
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
use windows::Win32::Graphics::Gdi::GetDeviceCaps;

#[cfg(windows)]
unsafe fn create_controls(hwnd: HWND, settings: &CaptureSettings, hfont: HFONT, dev_mode: bool) {
    use windows::core::PCWSTR;
    
    let hinstance = GetModuleHandleW(None).unwrap();
    let button_class = wide_string("BUTTON");
    let static_class = wide_string("STATIC");
    let edit_class = wide_string("EDIT");
    
    let mut y_pos = 20;
    let left_margin = 30;
    let control_width = 340;
    let control_height = 24;
    let spacing = 32;
    
    // Title label
    let text = wide_string("Capture Settings");
    let title_hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PCWSTR(static_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE,
        left_margin, y_pos - 10, control_width, 28,
        hwnd,
        None,
        hinstance,
        None,
    ).unwrap();
    let _ = SendMessageW(title_hwnd, WM_SETFONT, WPARAM(hfont.0 as usize), LPARAM(1));
    y_pos += spacing;
    
    // Checkbox: Show Cursor
    let text = wide_string("  Show cursor in capture");
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
    let _ = SendMessageW(check_cursor, WM_SETFONT, WPARAM(hfont.0 as usize), LPARAM(1));
    if settings.show_cursor {
        let _ = SendMessageW(check_cursor, BM_SETCHECK, WPARAM(BST_CHECKED.0 as usize), LPARAM(0));
    }
    y_pos += spacing;
    
    // Checkbox: Show Border
    let text = wide_string("  Show border frame");
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
    let _ = SendMessageW(check_border, WM_SETFONT, WPARAM(hfont.0 as usize), LPARAM(1));
    if settings.show_border {
        let _ = SendMessageW(check_border, BM_SETCHECK, WPARAM(BST_CHECKED.0 as usize), LPARAM(0));
    }
    y_pos += spacing;
    
    // Border width label and edit (on same line)
    let text = wide_string("       Border width:");
    let label_hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PCWSTR(static_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE,
        left_margin, y_pos + 2, 120, control_height,
        hwnd,
        None,
        hinstance,
        None,
    ).unwrap();
    let _ = SendMessageW(label_hwnd, WM_SETFONT, WPARAM(hfont.0 as usize), LPARAM(1));
    
    let text = wide_string(&settings.border_width.to_string());
    let edit_hwnd = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        PCWSTR(edit_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(ES_NUMBER as u32 | ES_CENTER as u32),
        left_margin + 125, y_pos, 50, control_height,
        hwnd,
        HMENU(ID_EDIT_BORDER_WIDTH as *mut c_void),
        hinstance,
        None,
    ).unwrap();
    let _ = SendMessageW(edit_hwnd, WM_SETFONT, WPARAM(hfont.0 as usize), LPARAM(1));
    
    let text = wide_string("px");
    let px_hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PCWSTR(static_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE,
        left_margin + 180, y_pos + 2, 25, control_height,
        hwnd,
        None,
        hinstance,
        None,
    ).unwrap();
    let _ = SendMessageW(px_hwnd, WM_SETFONT, WPARAM(hfont.0 as usize), LPARAM(1));
    y_pos += spacing;
    
    // Checkbox: Production Mode (only in dev mode)
    if dev_mode {
        let text = wide_string("  Production mode (hide destination window)");
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
        let _ = SendMessageW(check_prod, WM_SETFONT, WPARAM(hfont.0 as usize), LPARAM(1));
        if settings.exclude_from_capture {
            let _ = SendMessageW(check_prod, BM_SETCHECK, WPARAM(BST_CHECKED.0 as usize), LPARAM(0));
        }
        y_pos += spacing;
    }
    y_pos += 20;
    
    // Buttons - Save and Cancel
    let btn_width = 100;
    let btn_height = 32;
    let btn_spacing = 20;
    let total_btn_width = btn_width * 2 + btn_spacing;
    let btn_start_x = (DIALOG_WIDTH - total_btn_width) / 2;
    
    let text = wide_string("Save");
    let save_btn = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PCWSTR(button_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
        btn_start_x, y_pos, btn_width, btn_height,
        hwnd,
        HMENU(ID_BTN_SAVE as *mut c_void),
        hinstance,
        None,
    ).unwrap();
    let _ = SendMessageW(save_btn, WM_SETFONT, WPARAM(hfont.0 as usize), LPARAM(1));
    
    let text = wide_string("Cancel");
    let cancel_btn = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PCWSTR(button_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        btn_start_x + btn_width + btn_spacing, y_pos, btn_width, btn_height,
        hwnd,
        HMENU(ID_BTN_CANCEL as *mut c_void),
        hinstance,
        None,
    ).unwrap();
    let _ = SendMessageW(cancel_btn, WM_SETFONT, WPARAM(hfont.0 as usize), LPARAM(1));
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
                ID_BTN_SAVE => {
                    save_settings_from_controls(hwnd);
                    SETTINGS_CHANGED.with(|c| *c.borrow_mut() = true);
                    let _ = DestroyWindow(hwnd);
                }
                ID_BTN_CANCEL => {
                    SETTINGS_CHANGED.with(|c| *c.borrow_mut() = false);
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
    let dev_mode = DIALOG_DEV_MODE.with(|d| *d.borrow());
    
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
            
            // Production mode checkbox only exists in dev mode
            if dev_mode {
                if let Ok(check_prod) = GetDlgItem(hwnd, ID_CHECK_PROD_MODE) {
                    settings.exclude_from_capture = SendMessageW(check_prod, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == BST_CHECKED.0 as isize;
                }
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
pub fn show_settings_dialog(_current_settings: &CaptureSettings, _dev_mode: bool) -> Option<CaptureSettings> {
    // Settings dialog not supported on non-Windows platforms
    None
}
