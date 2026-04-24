use std::mem;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::capture;
use crate::geometry;
use crate::render;

struct WindowState {
    capture: capture::CaptureState,
    dpi: u32,
    work_area: geometry::Rect,
}

const CLASS_NAME: PCWSTR = w!("ShareFrameClass");
const WINDOW_TITLE: PCWSTR = w!("Share Frame");

/// Creates the window and runs the Win32 message loop.
pub fn create_and_run() -> windows::core::Result<()> {
    unsafe {
        let hinstance: HINSTANCE = GetModuleHandleW(None)?.into();

        let wc = WNDCLASSEXW {
            cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance,
            lpszClassName: CLASS_NAME,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH::default(), // No background brush
            ..Default::default()
        };

        let atom = RegisterClassExW(&wc);
        if atom == 0 {
            return Err(Error::from_win32());
        }

        // Get primary monitor work area for initial placement
        // Use a temporary calculation with the desktop window
        let desktop_hwnd = GetDesktopWindow();
        let work_area = geometry::get_monitor_work_area(desktop_hwnd);
        let monitor_width = work_area.right - work_area.left;
        let monitor_height = work_area.bottom - work_area.top;

        let size = geometry::default_size(monitor_width, monitor_height);
        let pos = geometry::centered_position(size, work_area);

        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW | WS_EX_LAYERED,
            CLASS_NAME,
            WINDOW_TITLE,
            WS_POPUP | WS_VISIBLE,
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
            let dpi = GetDpiForWindow(hwnd);
            let dpi = if dpi == 0 { 96 } else { dpi };

            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;

            let work_area = geometry::get_monitor_work_area(hwnd);
            let cap_state = capture::init(hwnd, width, height, dpi);

            let state = Box::new(WindowState {
                capture: cap_state,
                dpi,
                work_area,
            });

            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);

            // Set initial layered window alpha to fully opaque
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);

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
                    capture::resize(&mut state.capture, width, height, state.dpi);
                }
            }
            LRESULT(0)
        }

        WM_SIZING => {
            let rect_ptr = lparam.0 as *mut geometry::Rect;
            if !rect_ptr.is_null() {
                let rect = &mut *rect_ptr;
                geometry::constrain_size(
                    rect,
                    geometry::MIN_WIDTH,
                    geometry::MIN_HEIGHT,
                    wparam.0,
                );
            }
            LRESULT(1) // Return TRUE to indicate we handled it
        }

        WM_MOVING => {
            let rect_ptr = lparam.0 as *mut geometry::Rect;
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !rect_ptr.is_null() && !state_ptr.is_null() {
                let rect = &mut *rect_ptr;
                let state = &*state_ptr;
                geometry::constrain_position(rect, state.work_area);
            }
            LRESULT(1)
        }

        WM_NCHITTEST => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);

            let cursor = geometry::Point { x, y };
            let window_rect = geometry::Rect {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
            };

            let result = geometry::hit_test(
                cursor,
                window_rect,
                geometry::HIT_TEST_MARGIN,
                geometry::GRIP_SIZE,
            );

            LRESULT(result as isize)
        }

        WM_DPICHANGED => {
            let new_dpi = (wparam.0 & 0xFFFF) as u32;
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !state_ptr.is_null() {
                let state = &mut *state_ptr;
                state.dpi = new_dpi;
                state.work_area = geometry::get_monitor_work_area(hwnd);
            }

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

        WM_DISPLAYCHANGE => {
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !state_ptr.is_null() {
                let state = &mut *state_ptr;
                state.work_area = geometry::get_monitor_work_area(hwnd);

                // Check if window is out of bounds and reposition
                let mut rect = RECT::default();
                let _ = GetWindowRect(hwnd, &mut rect);
                let mut geo_rect = geometry::Rect {
                    left: rect.left,
                    top: rect.top,
                    right: rect.right,
                    bottom: rect.bottom,
                };
                geometry::constrain_position(&mut geo_rect, state.work_area);
                if geo_rect.left != rect.left || geo_rect.top != rect.top {
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        geo_rect.left,
                        geo_rect.top,
                        geo_rect.right - geo_rect.left,
                        geo_rect.bottom - geo_rect.top,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            }
            LRESULT(0)
        }

        WM_ERASEBKGND => {
            // Return 1 to prevent background erase flicker
            LRESULT(1)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
