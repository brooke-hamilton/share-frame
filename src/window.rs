use std::mem;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::capture;
use crate::geometry;
use crate::render;

struct WindowState {
    capture: capture::CaptureState,
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

        // Message loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        Ok(())
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let mut client_rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut client_rect);
            let width = (client_rect.right - client_rect.left).max(1);
            let height = (client_rect.bottom - client_rect.top).max(1);

            let cap_state = capture::init(hwnd, width, height);

            let state = Box::new(WindowState {
                capture: cap_state,
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
                render::paint(hwnd, &state.capture);
            } else {
                // No state yet, do default paint to avoid infinite WM_PAINT loop
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
                if width > 0 && height > 0 {
                    capture::resize(&mut state.capture, width, height);
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
            LRESULT(0)
        }

        WM_ERASEBKGND => {
            LRESULT(1)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
