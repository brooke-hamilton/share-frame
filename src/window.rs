use std::mem;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::Controls::{MARGINS, WM_MOUSELEAVE};
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::capture;
use crate::geometry;
use crate::render;

struct WindowState {
    capture: capture::CaptureState,
    send_back_hovered: bool,
    tracking_mouse: bool,
    title_bar_height: i32,
}

const CLASS_NAME: PCWSTR = w!("ShareFrameClass");
const WINDOW_TITLE: PCWSTR = w!("Share Frame");

/// Detects whether Windows is in dark mode by reading the registry.
fn is_dark_mode() -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        let subkey = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
        let result = RegOpenKeyExW(HKEY_CURRENT_USER, subkey, 0, KEY_READ, &mut hkey);
        if result.is_err() {
            return true; // default to dark
        }

        let value_name = w!("AppsUseLightTheme");
        let mut data: u32 = 0;
        let mut data_size = mem::size_of::<u32>() as u32;
        let result = RegQueryValueExW(
            hkey,
            value_name,
            None,
            None,
            Some(&mut data as *mut u32 as *mut u8),
            Some(&mut data_size),
        );
        let _ = RegCloseKey(hkey);

        if result.is_err() {
            return true;
        }

        data == 0
    }
}

/// Creates the window and runs the Win32 message loop.
pub fn create_and_run() -> windows::core::Result<()> {
    unsafe {
        let hinstance: HINSTANCE = GetModuleHandleW(None)?.into();

        // Load the application icon from embedded resources (ID 1)
        let icon = LoadImageW(
            hinstance,
            PCWSTR(1 as *const u16),
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE | LR_SHARED,
        )?;

        let wc = WNDCLASSEXW {
            cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance,
            lpszClassName: CLASS_NAME,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hIcon: HICON(icon.0),
            hIconSm: HICON(icon.0),
            hbrBackground: HBRUSH(GetStockObject(NULL_BRUSH).0),
            ..Default::default()
        };

        let atom = RegisterClassExW(&wc);
        if atom == 0 {
            return Err(Error::from_win32());
        }

        // Get primary monitor work area for initial placement
        let desktop_hwnd = GetDesktopWindow();
        let work_area = geometry::get_monitor_work_area(desktop_hwnd);
        let monitor_width = work_area.right - work_area.left;
        let monitor_height = work_area.bottom - work_area.top;

        let size = geometry::default_size(monitor_width, monitor_height);
        let pos = geometry::centered_position(size, work_area);

        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            CLASS_NAME,
            WINDOW_TITLE,
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_MAXIMIZEBOX | WS_VISIBLE,
            pos.x,
            pos.y,
            size.width,
            size.height,
            None,
            None,
            hinstance,
            None,
        )?;

        if hwnd == HWND::default() {
            return Err(Error::from_win32());
        }

        // Tell DWM to use dark mode title bar if the system is in dark mode
        let dark: BOOL = if is_dark_mode() { TRUE } else { FALSE };
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const _ as *const _,
            mem::size_of_val(&dark) as u32,
        );

        // Extend frame into client area so we can draw in the title bar region
        // while DWM still renders the native caption buttons.
        let title_bar_height = compute_title_bar_height(hwnd);
        let margins = MARGINS {
            cxLeftWidth: 0,
            cxRightWidth: 0,
            cyTopHeight: title_bar_height,
            cyBottomHeight: 0,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        // Force the window to re-evaluate its frame (sends WM_NCCALCSIZE again
        // now that DWM knows about the extended frame).
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );

        // Message loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        Ok(())
    }
}

/// Computes the title bar height from system metrics for the given DPI.
unsafe fn compute_title_bar_height(hwnd: HWND) -> i32 {
    let dpi = GetDpiForWindow(hwnd);
    let frame_y = GetSystemMetricsForDpi(SM_CYFRAME, dpi);
    let caption = GetSystemMetricsForDpi(SM_CYCAPTION, dpi);
    let padding = GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
    frame_y + caption + padding
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // For WM_NCHITTEST and WM_NCCALCSIZE we handle ourselves.
    // For mouse messages in client area, handle ourselves (don't let DWM eat them).
    // For everything else, let DWM handle caption button interactions.
    if msg != WM_NCHITTEST
        && msg != WM_NCCALCSIZE
        && msg != WM_MOUSEMOVE
        && msg != WM_MOUSELEAVE
        && msg != WM_LBUTTONDOWN
        && msg != WM_LBUTTONUP
    {
        let mut dwm_result = LRESULT(0);
        if DwmDefWindowProc(hwnd, msg, wparam, lparam, &mut dwm_result).as_bool() {
            return dwm_result;
        }
    }

    match msg {
        WM_CREATE => {
            let title_bar_height = compute_title_bar_height(hwnd);

            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);
            let width = (rect.right - rect.left).max(1);
            let height = (rect.bottom - rect.top).max(1);

            // Capture buffer covers only the content area below the title bar
            let content_height = (height - title_bar_height).max(1);
            let cap_state = capture::init(hwnd, width, content_height);

            let state = Box::new(WindowState {
                capture: cap_state,
                send_back_hovered: false,
                tracking_mouse: false,
                title_bar_height,
            });

            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);

            LRESULT(0)
        }

        WM_DESTROY => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !ptr.is_null() {
                let mut state = Box::from_raw(ptr);
                capture::cleanup(&mut state.capture, hwnd);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            PostQuitMessage(0);
            LRESULT(0)
        }

        WM_TIMER => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !ptr.is_null() {
                let state = &mut *ptr;
                capture::capture_frame(hwnd, &mut state.capture);
            }
            LRESULT(0)
        }

        WM_PAINT => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !ptr.is_null() {
                let state = &*ptr;
                render::paint(hwnd, &state.capture, state.send_back_hovered, state.title_bar_height);
            } else {
                let mut ps = PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut ps);
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }

        WM_SIZE => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !ptr.is_null() {
                let state = &mut *ptr;
                let width = (lparam.0 & 0xFFFF) as i32;
                let height = ((lparam.0 >> 16) & 0xFFFF) as i32;
                let content_height = height - state.title_bar_height;
                if width > 0 && content_height > 0 {
                    capture::resize(&mut state.capture, width, content_height);
                    if !state.capture.paused {
                        capture::capture_frame(hwnd, &mut state.capture);
                    }
                }
            }
            LRESULT(0)
        }

        WM_ENTERSIZEMOVE => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !ptr.is_null() {
                (*ptr).capture.paused = true;
            }
            LRESULT(0)
        }

        WM_EXITSIZEMOVE => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !ptr.is_null() {
                let state = &mut *ptr;
                state.capture.paused = false;
                capture::capture_frame(hwnd, &mut state.capture);
            }
            LRESULT(0)
        }

        WM_GETMINMAXINFO => {
            let info = lparam.0 as *mut MINMAXINFO;
            if !info.is_null() {
                (*info).ptMinTrackSize.x = geometry::MIN_WIDTH;
                (*info).ptMinTrackSize.y = geometry::MIN_HEIGHT;
            }
            LRESULT(0)
        }

        WM_NCCALCSIZE => {
            // Return 0 to make client area = full window rect, removing
            // the standard non-client frame so we can draw our own title bar.
            LRESULT(0)
        }

        WM_NCHITTEST => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);

            let margin = geometry::RESIZE_MARGIN;

            // Resize borders (check edges first)
            if y < rect.top + margin {
                if x < rect.left + margin {
                    return LRESULT(HTTOPLEFT as isize);
                }
                if x >= rect.right - margin {
                    return LRESULT(HTTOPRIGHT as isize);
                }
                return LRESULT(HTTOP as isize);
            }
            if y >= rect.bottom - margin {
                if x < rect.left + margin {
                    return LRESULT(HTBOTTOMLEFT as isize);
                }
                if x >= rect.right - margin {
                    return LRESULT(HTBOTTOMRIGHT as isize);
                }
                return LRESULT(HTBOTTOM as isize);
            }
            if x < rect.left + margin {
                return LRESULT(HTLEFT as isize);
            }
            if x >= rect.right - margin {
                return LRESULT(HTRIGHT as isize);
            }

            // Get title bar height from state (or compute)
            let tb_height = {
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowState;
                if !ptr.is_null() { (*ptr).title_bar_height } else { compute_title_bar_height(hwnd) }
            };

            // Title bar region
            if y < rect.top + tb_height {
                // "Send to Back" button — right side of title bar, before caption buttons
                let caption_buttons_width = get_caption_buttons_width(hwnd);
                let button_right = rect.right - caption_buttons_width;
                let button_left = button_right - geometry::SEND_BACK_BUTTON_WIDTH;
                if x >= button_left && x < button_right {
                    return LRESULT(HTCLIENT as isize);
                }

                // Let DWM check if cursor is over a caption button (close/max)
                let mut dwm_result = LRESULT(0);
                if DwmDefWindowProc(hwnd, msg, wparam, lparam, &mut dwm_result).as_bool() {
                    return dwm_result;
                }

                // Otherwise it's the draggable caption area
                return LRESULT(HTCAPTION as isize);
            }

            // Client area (content below title bar)
            LRESULT(HTCLIENT as isize)
        }

        WM_DPICHANGED => {
            // Use the suggested rect from lparam
            let suggested = lparam.0 as *const RECT;
            if !suggested.is_null() {
                let r = &*suggested;
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            LRESULT(0)
        }

        WM_SETTINGCHANGE => {
            // Update dark mode title bar on theme change
            let dark: BOOL = if is_dark_mode() { TRUE } else { FALSE };
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark as *const _ as *const _,
                mem::size_of_val(&dark) as u32,
            );
            let _ = InvalidateRect(hwnd, None, FALSE);
            LRESULT(0)
        }

        WM_ERASEBKGND => {
            LRESULT(1)
        }

        WM_MOUSEMOVE => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !ptr.is_null() {
                let state = &mut *ptr;

                if !state.tracking_mouse {
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = TrackMouseEvent(&mut tme);
                    state.tracking_mouse = true;
                }

                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

                let mut client_rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut client_rect);
                let cw = client_rect.right - client_rect.left;

                let hovered = is_in_send_back_button(hwnd, x, y, cw, state.title_bar_height);
                if hovered != state.send_back_hovered {
                    state.send_back_hovered = hovered;
                    let caption_buttons_width = get_caption_buttons_width(hwnd);
                    let button_left = cw - caption_buttons_width - geometry::SEND_BACK_BUTTON_WIDTH;

                    // Invalidate the button area
                    let btn_rect = RECT {
                        left: button_left,
                        top: 0,
                        right: button_left + geometry::SEND_BACK_BUTTON_WIDTH,
                        bottom: state.title_bar_height,
                    };
                    let _ = InvalidateRect(hwnd, Some(&btn_rect), FALSE);
                }
            }
            LRESULT(0)
        }

        WM_MOUSELEAVE => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !ptr.is_null() {
                let state = &mut *ptr;
                state.tracking_mouse = false;
                if state.send_back_hovered {
                    state.send_back_hovered = false;
                    let _ = InvalidateRect(hwnd, None, FALSE);
                }
            }
            LRESULT(0)
        }

        WM_LBUTTONUP => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowState;
            let tb_height = if !ptr.is_null() { (*ptr).title_bar_height } else { compute_title_bar_height(hwnd) };

            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

            let mut client_rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut client_rect);
            let cw = client_rect.right - client_rect.left;

            if is_in_send_back_button(hwnd, x, y, cw, tb_height) {
                // Send window to bottom of Z-order
                let _ = SetWindowPos(
                    hwnd,
                    HWND_BOTTOM,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Returns the width of DWM-drawn caption buttons (close + maximize + disabled minimize).
unsafe fn get_caption_buttons_width(hwnd: HWND) -> i32 {
    let mut buttons_rect = RECT::default();
    let result = DwmGetWindowAttribute(
        hwnd,
        DWMWA_CAPTION_BUTTON_BOUNDS,
        &mut buttons_rect as *mut _ as *mut _,
        mem::size_of::<RECT>() as u32,
    );
    if result.is_ok() {
        buttons_rect.right - buttons_rect.left
    } else {
        // Fallback: typical width for 3 caption buttons at 100% DPI
        138
    }
}

/// Returns true if the given client-relative point is inside the "Send to Back" button.
unsafe fn is_in_send_back_button(hwnd: HWND, x: i32, y: i32, client_width: i32, title_bar_height: i32) -> bool {
    if y >= title_bar_height {
        return false;
    }
    let caption_buttons_width = get_caption_buttons_width(hwnd);
    let button_left = client_width - caption_buttons_width - geometry::SEND_BACK_BUTTON_WIDTH;
    let button_right = button_left + geometry::SEND_BACK_BUTTON_WIDTH;
    x >= button_left && x < button_right
}
