use std::mem;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::capture;
use crate::geometry;
use crate::render;

struct WindowState {
    capture: capture::CaptureState,
    work_area: geometry::Rect,
    close_hovered: bool,
    tracking_mouse: bool,
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
            // Use a NULL stock brush rather than a default (null) HBRUSH so
            // DefWindowProc never tries to FillRect with an invalid handle if
            // a future code path forgets to handle WM_ERASEBKGND.
            hbrBackground: HBRUSH(GetStockObject(NULL_BRUSH).0),
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
            WS_POPUP | WS_SYSMENU | WS_THICKFRAME | WS_VISIBLE,
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
            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);
            // GetWindowRect on a per-monitor DPI-aware window already returns
            // physical pixels — pass them straight through to capture::init.
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;

            let work_area = geometry::get_monitor_work_area(hwnd);
            // Capture buffer covers only the content area below the title bar
            let content_height = (height - geometry::TITLE_BAR_HEIGHT).max(1);
            let cap_state = capture::init(hwnd, width, content_height);

            let state = Box::new(WindowState {
                capture: cap_state,
                work_area,
                close_hovered: false,
                tracking_mouse: false,
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
                render::paint(hwnd, &state.capture, state.close_hovered);
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
                // lParam holds the new client size in physical pixels for a
                // per-monitor DPI-aware window. Resize the back buffer to
                // match exactly so paint() can use BitBlt (1:1) instead of
                // StretchBlt.
                let width = (lparam.0 & 0xFFFF) as i32;
                let height = ((lparam.0 >> 16) & 0xFFFF) as i32;
                let content_height = height - geometry::TITLE_BAR_HEIGHT;
                if width > 0 && content_height > 0 {
                    capture::resize(&mut state.capture, width, content_height);
                    // Skip the immediate recapture while the user is dragging
                    // — paused captures will resume on WM_EXITSIZEMOVE. The
                    // stale frame is StretchBlt'd to the new size in the
                    // meantime, which is what the user expects to see.
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
                // Refresh after the user finishes dragging so the displayed
                // frame matches the new geometry.
                capture::capture_frame(hwnd, &mut state.capture);
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
            let _new_dpi = (wparam.0 & 0xFFFF) as u32;
            let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !state_ptr.is_null() {
                let state = &mut *state_ptr;
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

        WM_NCCALCSIZE => {
            // Returning 0 with wParam TRUE tells the OS the entire window is
            // client area (no non-client frame). This removes the visible
            // thick frame that WS_THICKFRAME would normally draw, while
            // keeping resize functionality active.
            if wparam.0 != 0 {
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_ERASEBKGND => {
            // Return 1 to prevent background erase flicker
            LRESULT(1)
        }

        WM_MOUSEMOVE => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !ptr.is_null() {
                let state = &mut *ptr;

                // Request WM_MOUSELEAVE tracking if not already active
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

                let hovered = geometry::is_in_close_button(x, y, cw);
                if hovered != state.close_hovered {
                    state.close_hovered = hovered;
                    // Invalidate just the title bar to repaint the close button
                    let tb_rect = RECT { left: cw - geometry::CLOSE_BUTTON_WIDTH, top: 0, right: cw, bottom: geometry::TITLE_BAR_HEIGHT };
                    let _ = InvalidateRect(hwnd, Some(&tb_rect), FALSE);
                }
            }
            LRESULT(0)
        }

        WM_MOUSELEAVE => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
            if !ptr.is_null() {
                let state = &mut *ptr;
                state.tracking_mouse = false;
                if state.close_hovered {
                    state.close_hovered = false;
                    let mut client_rect = RECT::default();
                    let _ = GetClientRect(hwnd, &mut client_rect);
                    let cw = client_rect.right - client_rect.left;
                    let tb_rect = RECT { left: cw - geometry::CLOSE_BUTTON_WIDTH, top: 0, right: cw, bottom: geometry::TITLE_BAR_HEIGHT };
                    let _ = InvalidateRect(hwnd, Some(&tb_rect), FALSE);
                }
            }
            LRESULT(0)
        }

        WM_LBUTTONUP => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;

            let mut client_rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut client_rect);
            let cw = client_rect.right - client_rect.left;

            if geometry::is_in_close_button(x, y, cw) {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
