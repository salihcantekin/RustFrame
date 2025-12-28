// settings_dialog.rs - Native Windows Settings Dialog
//
// A Win32 dialog for adjusting capture settings.
// Uses modern Windows controls with proper DPI scaling and Segoe UI font.

use crate::capture::CaptureSettings;
use crate::constants::{capture as capture_const, dialog};
use crate::utils::wide_string;
use log::info;
use std::cell::RefCell;

#[cfg(windows)]
use windows::Win32::{
    Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Dwm::{
        DwmSetWindowAttribute, DWMSBT_MAINWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
        DWMWA_USE_IMMERSIVE_DARK_MODE, DWM_SYSTEMBACKDROP_TYPE,
    },
    Graphics::Gdi::{
        CreateFontW, DeleteObject, GetDC, GetSysColorBrush, ReleaseDC, CLEARTYPE_QUALITY,
        CLIP_DEFAULT_PRECIS, COLOR_3DFACE, DEFAULT_CHARSET, FF_SWISS, FW_NORMAL, HFONT, HGDIOBJ,
        OUT_TT_PRECIS,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::Controls::*,
    UI::WindowsAndMessaging::*,
};

#[cfg(windows)]
use std::ffi::c_void;

const ID_CHECK_CURSOR: i32 = 101;
const ID_CHECK_BORDER: i32 = 102;
const ID_CHECK_PROD_MODE: i32 = 103;
const ID_EDIT_BORDER_WIDTH: i32 = 105;
const ID_BTN_SAVE: i32 = 106;
const ID_BTN_CANCEL: i32 = 107;

// Static text style for center alignment
const SS_CENTER: u32 = 0x01;

// Thread-local state for dialog
thread_local! {
    static DIALOG_SETTINGS: RefCell<Option<CaptureSettings>> = const { RefCell::new(None) };
    static SETTINGS_CHANGED: RefCell<bool> = const { RefCell::new(false) };
    static DIALOG_HWND: RefCell<Option<HWND>> = const { RefCell::new(None) };
    static DIALOG_FONT: RefCell<Option<HFONT>> = const { RefCell::new(None) };
    static DIALOG_DEV_MODE: RefCell<bool> = const { RefCell::new(false) };

    static DLG_CHECK_CURSOR: RefCell<Option<HWND>> = const { RefCell::new(None) };
    static DLG_CHECK_BORDER: RefCell<Option<HWND>> = const { RefCell::new(None) };
    static DLG_CHECK_PROD: RefCell<Option<HWND>> = const { RefCell::new(None) };
    static DLG_EDIT_BORDER_WIDTH: RefCell<Option<HWND>> = const { RefCell::new(None) };
}

/// Show the settings dialog
/// Returns Some(CaptureSettings) if user clicked Save, None if cancelled
/// dev_mode: if true, shows production mode option
#[cfg(windows)]
pub fn show_settings_dialog(
    current_settings: &CaptureSettings,
    dev_mode: bool,
) -> Option<CaptureSettings> {
    use windows::core::PCWSTR;

    unsafe {
        // Store dev_mode for create_controls
        DIALOG_DEV_MODE.with(|d| *d.borrow_mut() = dev_mode);

        // Initialize common controls for modern visual style
        let icc = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES | ICC_WIN95_CLASSES,
        };
        let _ = InitCommonControlsEx(&icc);

        // Store settings in thread-local state
        DIALOG_SETTINGS.with(|s| *s.borrow_mut() = Some(current_settings.clone()));
        SETTINGS_CHANGED.with(|c| *c.borrow_mut() = false);

        // Create modern font (Segoe UI, 10pt)
        let font_name = wide_string("Segoe UI");
        let hdc = GetDC(None);
        let font_height = -((12.0
            * GetDeviceCaps(Some(hdc), windows::Win32::Graphics::Gdi::LOGPIXELSY) as f32)
            / 72.0) as i32;
        let _ = ReleaseDC(None, hdc);

        // Create font using strongly-typed constants
        let hfont = CreateFontW(
            font_height,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_TT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            FF_SWISS.0 as u32,
            PCWSTR(font_name.as_ptr()),
        );
        DIALOG_FONT.with(|f| *f.borrow_mut() = Some(hfont));

        let module = GetModuleHandleW(None).unwrap();
        let hinstance: HINSTANCE = module.into();

        // Generate unique class name
        let class_name = wide_string(&format!("RustFrameSettings_{}", std::process::id()));
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(settings_dialog_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
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
        let dialog_height = if dev_mode {
            dialog::HEIGHT_DEV
        } else {
            dialog::HEIGHT_PROD
        };
        let x = (screen_width - dialog::WIDTH) / 2;
        let y = (screen_height - dialog_height) / 2;

        // Create the dialog window with modern style
        let window_name = wide_string("RustFrame Settings");
        let style_bits = WS_OVERLAPPED.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_VISIBLE.0;
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_DLGMODALFRAME.0 | WS_EX_TOPMOST.0),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(window_name.as_ptr()),
            WINDOW_STYLE(style_bits),
            x,
            y,
            dialog::WIDTH,
            dialog_height,
            None,
            None,
            Some(hinstance),
            None,
        )
        .unwrap();

        // Enable Windows 11 Mica backdrop effect
        let use_mica = DWMSBT_MAINWINDOW;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &use_mica as *const _ as *const c_void,
            size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
        );

        // Enable dark mode
        let use_dark: i32 = 1;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &use_dark as *const _ as *const c_void,
            size_of::<i32>() as u32,
        );

        // Store hwnd for reference
        DIALOG_HWND.with(|h| *h.borrow_mut() = Some(hwnd));

        // Create controls
        create_controls(hwnd, current_settings, hfont, dev_mode);

        // Message loop - run until window is closed
        let mut msg = MSG::default();
        loop {
            let result = GetMessageW(&mut msg, None, 0, 0);
            if !result.as_bool() || result.0 == -1 {
                break;
            }

            // Check if dialog still exists
            if !IsWindow(Some(hwnd)).as_bool() {
                break;
            }

            if !IsDialogMessageW(hwnd, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        // Cleanup
        if let Some(font) = DIALOG_FONT.with(|f| *f.borrow()) {
            let _ = DeleteObject(HGDIOBJ(font.0));
        }
        let _ = UnregisterClassW(PCWSTR(class_name.as_ptr()), Some(hinstance));

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

    let module = GetModuleHandleW(None).unwrap();
    let hinstance: HINSTANCE = module.into();
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
        WINDOW_EX_STYLE(0),
        PCWSTR(static_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE,
        left_margin,
        y_pos - 10,
        control_width,
        28,
        Some(hwnd),
        None,
        Some(hinstance),
        None,
    )
    .unwrap();
    let _ = SendMessageW(
        title_hwnd,
        WM_SETFONT,
        Some(WPARAM(hfont.0 as usize)),
        Some(LPARAM(1)),
    );
    y_pos += spacing;

    // Checkbox: Show Cursor
    let text = wide_string("  Show cursor in capture");
    let check_cursor = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        PCWSTR(button_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        left_margin,
        y_pos,
        control_width,
        control_height,
        Some(hwnd),
        Some(HMENU(ID_CHECK_CURSOR as isize as *mut c_void)),
        Some(hinstance),
        None,
    )
    .unwrap();
    DLG_CHECK_CURSOR.with(|c| *c.borrow_mut() = Some(check_cursor));
    let _ = SendMessageW(
        check_cursor,
        WM_SETFONT,
        Some(WPARAM(hfont.0 as usize)),
        Some(LPARAM(1)),
    );
    if settings.show_cursor {
        let _ = SendMessageW(
            check_cursor,
            BM_SETCHECK,
            Some(WPARAM(BST_CHECKED.0 as usize)),
            Some(LPARAM(0)),
        );
    }
    y_pos += spacing;

    // Checkbox: Show Border
    let text = wide_string("  Show border frame");
    let check_border = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        PCWSTR(button_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        left_margin,
        y_pos,
        control_width,
        control_height,
        Some(hwnd),
        Some(HMENU(ID_CHECK_BORDER as isize as *mut c_void)),
        Some(hinstance),
        None,
    )
    .unwrap();
    DLG_CHECK_BORDER.with(|c| *c.borrow_mut() = Some(check_border));
    let _ = SendMessageW(
        check_border,
        WM_SETFONT,
        Some(WPARAM(hfont.0 as usize)),
        Some(LPARAM(1)),
    );
    if settings.show_border {
        let _ = SendMessageW(
            check_border,
            BM_SETCHECK,
            Some(WPARAM(BST_CHECKED.0 as usize)),
            Some(LPARAM(0)),
        );
    }
    y_pos += spacing;

    // Border width label and edit (on same line)
    let text = wide_string("       Border width:");
    let label_hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        PCWSTR(static_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE,
        left_margin,
        y_pos + 2,
        120,
        control_height,
        Some(hwnd),
        None,
        Some(hinstance),
        None,
    )
    .unwrap();
    let _ = SendMessageW(
        label_hwnd,
        WM_SETFONT,
        Some(WPARAM(hfont.0 as usize)),
        Some(LPARAM(1)),
    );

    let text = wide_string(&settings.border_width.to_string());
    let edit_hwnd = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        PCWSTR(edit_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD
            | WS_VISIBLE
            | WS_TABSTOP
            | WINDOW_STYLE(ES_NUMBER as u32)
            | WINDOW_STYLE(ES_CENTER as u32),
        left_margin + 125,
        y_pos,
        50,
        control_height,
        Some(hwnd),
        Some(HMENU(ID_EDIT_BORDER_WIDTH as isize as *mut c_void)),
        Some(hinstance),
        None,
    )
    .unwrap();
    DLG_EDIT_BORDER_WIDTH.with(|c| *c.borrow_mut() = Some(edit_hwnd));
    let _ = SendMessageW(
        edit_hwnd,
        WM_SETFONT,
        Some(WPARAM(hfont.0 as usize)),
        Some(LPARAM(1)),
    );

    let text = wide_string("px");
    let px_hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        PCWSTR(static_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE,
        left_margin + 180,
        y_pos + 2,
        25,
        control_height,
        Some(hwnd),
        None,
        Some(hinstance),
        None,
    )
    .unwrap();
    let _ = SendMessageW(
        px_hwnd,
        WM_SETFONT,
        Some(WPARAM(hfont.0 as usize)),
        Some(LPARAM(1)),
    );
    y_pos += spacing;

    // Checkbox: Production Mode (only in dev mode)
    if dev_mode {
        let text = wide_string("  Production mode (hide destination window)");
        let check_prod = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(button_class.as_ptr()),
            PCWSTR(text.as_ptr()),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            left_margin,
            y_pos,
            control_width,
            control_height,
            Some(hwnd),
            Some(HMENU(ID_CHECK_PROD_MODE as isize as *mut c_void)),
            Some(hinstance),
            None,
        )
        .unwrap();
        DLG_CHECK_PROD.with(|c| *c.borrow_mut() = Some(check_prod));
        let _ = SendMessageW(
            check_prod,
            WM_SETFONT,
            Some(WPARAM(hfont.0 as usize)),
            Some(LPARAM(1)),
        );
        if settings.exclude_from_capture {
            let _ = SendMessageW(
                check_prod,
                BM_SETCHECK,
                Some(WPARAM(BST_CHECKED.0 as usize)),
                Some(LPARAM(0)),
            );
        }
        y_pos += spacing;
    }
    y_pos += 20;

    // Buttons - Save and Cancel
    let btn_width = 100;
    let btn_height = 32;
    let btn_spacing = 20;
    let total_btn_width = btn_width * 2 + btn_spacing;
    let btn_start_x = (dialog::WIDTH - total_btn_width) / 2;

    let text = wide_string("Save");
    let save_btn = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        PCWSTR(button_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
        btn_start_x,
        y_pos,
        btn_width,
        btn_height,
        Some(hwnd),
        Some(HMENU(ID_BTN_SAVE as isize as *mut c_void)),
        Some(hinstance),
        None,
    )
    .unwrap();
    let _ = SendMessageW(
        save_btn,
        WM_SETFONT,
        Some(WPARAM(hfont.0 as usize)),
        Some(LPARAM(1)),
    );

    let text = wide_string("Cancel");
    let cancel_btn = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        PCWSTR(button_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        btn_start_x + btn_width + btn_spacing,
        y_pos,
        btn_width,
        btn_height,
        Some(hwnd),
        Some(HMENU(ID_BTN_CANCEL as isize as *mut c_void)),
        Some(hinstance),
        None,
    )
    .unwrap();
    let _ = SendMessageW(
        cancel_btn,
        WM_SETFONT,
        Some(WPARAM(hfont.0 as usize)),
        Some(LPARAM(1)),
    );

    // Credit label at bottom
    let dialog_height = if dev_mode {
        dialog::HEIGHT_DEV
    } else {
        dialog::HEIGHT_PROD
    };
    let text = wide_string("by Salih Cantekin");
    let credit_hwnd = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        PCWSTR(static_class.as_ptr()),
        PCWSTR(text.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(SS_CENTER),
        0,
        dialog_height - 55,
        dialog::WIDTH,
        18,
        Some(hwnd),
        None,
        Some(hinstance),
        None,
    )
    .unwrap();
    // Use smaller font for credit
    let small_font = CreateFontW(
        14,
        0,
        0,
        0,
        FW_NORMAL.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_TT_PRECIS,
        CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        FF_SWISS.0 as u32,
        PCWSTR(wide_string("Segoe UI").as_ptr()),
    );
    let _ = SendMessageW(
        credit_hwnd,
        WM_SETFONT,
        Some(WPARAM(small_font.0 as usize)),
        Some(LPARAM(1)),
    );
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
                    save_settings_from_controls();
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
unsafe fn save_settings_from_controls() {
    let dev_mode = DIALOG_DEV_MODE.with(|d| *d.borrow());

    DIALOG_SETTINGS.with(|settings_cell| {
        let mut settings_opt = settings_cell.borrow_mut();
        if let Some(ref mut settings) = *settings_opt {
            // Read checkbox states
            DLG_CHECK_CURSOR.with(|c| {
                if let Some(h) = *c.borrow() {
                    let state = SendMessageW(h, BM_GETCHECK, Some(WPARAM(0)), Some(LPARAM(0))).0;
                    settings.show_cursor = state == BST_CHECKED.0 as isize;
                }
            });

            DLG_CHECK_BORDER.with(|c| {
                if let Some(h) = *c.borrow() {
                    let state = SendMessageW(h, BM_GETCHECK, Some(WPARAM(0)), Some(LPARAM(0))).0;
                    settings.show_border = state == BST_CHECKED.0 as isize;
                }
            });

            // Production mode checkbox only exists in dev mode
            if dev_mode {
                DLG_CHECK_PROD.with(|c| {
                    if let Some(h) = *c.borrow() {
                        let state =
                            SendMessageW(h, BM_GETCHECK, Some(WPARAM(0)), Some(LPARAM(0))).0;
                        settings.exclude_from_capture = state == BST_CHECKED.0 as isize;
                    }
                });
            }

            // Read border width
            DLG_EDIT_BORDER_WIDTH.with(|c| {
                if let Some(h) = *c.borrow() {
                    let mut buffer = [0u16; 16];
                    let len = GetWindowTextW(h, &mut buffer);
                    if len > 0 {
                        let text: String = String::from_utf16_lossy(&buffer[..len as usize]);
                        if let Ok(width) = text.parse::<u32>() {
                            settings.border_width = width.clamp(
                                capture_const::MIN_BORDER_WIDTH,
                                capture_const::MAX_BORDER_WIDTH,
                            );
                        }
                    }
                }
            });

            info!(
                "Settings saved: cursor={}, border={}, width={}, prod_mode={}",
                settings.show_cursor,
                settings.show_border,
                settings.border_width,
                settings.exclude_from_capture
            );
        }
    });
}

#[cfg(not(windows))]
pub fn show_settings_dialog(
    _current_settings: &CaptureSettings,
    _dev_mode: bool,
) -> Option<CaptureSettings> {
    // Settings dialog not supported on non-Windows platforms
    None
}
