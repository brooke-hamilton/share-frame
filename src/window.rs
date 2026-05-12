use std::mem;
use std::sync::atomic::{AtomicU32, Ordering};

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

/// Cached registered window message id. `RegisterWindowMessageW` returns the
/// same value for the same string across the whole session, so any caller
/// (including a second instance of this process) gets the same id.
static SHOW_WINDOW_MSG: AtomicU32 = AtomicU32::new(0);
/// Registered exit message id; the tray's Exit menu posts this so the
/// main window can run its normal destroy/cleanup path.
static EXIT_APP_MSG: AtomicU32 = AtomicU32::new(0);

/// Returns the registered window message that asks the existing instance to
/// restore + foreground its window. Lazily registered on first call.
/// Returns `0` if registration failed (caller should treat as no-op).
pub fn show_window_message() -> u32 {
    let cached = SHOW_WINDOW_MSG.load(Ordering::Relaxed);
    if cached != 0 {
        return cached;
    }
    // SAFETY: `RegisterWindowMessageW` is thread-safe and returns the same
    // id for the same null-terminated wide string. Racing callers may both
    // register; the OS deduplicates by string, so storing either result is
    // correct.
    let id = unsafe { RegisterWindowMessageW(w!("ShareFrame_ShowWindow_v1")) };
    SHOW_WINDOW_MSG.store(id, Ordering::Relaxed);
    id
}

/// Returns the registered window message that asks the main window to
/// fully exit (destroy itself, which triggers WM_DESTROY cleanup).
pub fn exit_app_message() -> u32 {
    let cached = EXIT_APP_MSG.load(Ordering::Relaxed);
    if cached != 0 {
        return cached;
    }
    let id = unsafe { RegisterWindowMessageW(w!("ShareFrame_ExitApp_v1")) };
    EXIT_APP_MSG.store(id, Ordering::Relaxed);
    id
}

use crate::capture::CaptureState;
use crate::geometry;
use crate::render::{self, RenderCache, TitleBarTheme};
use crate::tray;

/// Per-window state. Owned via `Box` and stored in `GWLP_USERDATA`.
struct WindowState {
    capture: CaptureState,
    render_cache: RenderCache,
    send_back_hovered: bool,
    tracking_mouse: bool,
    title_bar_height: i32,
    is_active: bool,
    /// Hidden message-only window owning the tray icon. `HWND::default()`
    /// until `tray::install` succeeds. Cleaned up in WM_DESTROY before
    /// `PostQuitMessage`.
    tray_hwnd: HWND,
}

const CLASS_NAME: PCWSTR = w!("ShareFrameClass");
const WINDOW_TITLE: PCWSTR = w!("Share Frame");

/// Reads a `REG_DWORD` from `HKEY_CURRENT_USER\<subkey>\<value>`.
/// Returns `None` if the key/value is missing or unreadable.
fn read_hkcu_dword(subkey: PCWSTR, value: PCWSTR) -> Option<u32> {
    // SAFETY: Registry handle is closed before return.
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey, 0, KEY_READ, &mut hkey).is_err() {
            return None;
        }
        let mut data: u32 = 0;
        let mut data_size = mem::size_of::<u32>() as u32;
        let result = RegQueryValueExW(
            hkey,
            value,
            None,
            None,
            Some(&mut data as *mut u32 as *mut u8),
            Some(&mut data_size),
        );
        let _ = RegCloseKey(hkey);
        if result.is_err() { None } else { Some(data) }
    }
}

/// Detects whether Windows is in dark mode by reading the registry.
/// Defaults to `false` (light mode, matching the Windows default) when the
/// value cannot be read.
fn is_dark_mode() -> bool {
    read_hkcu_dword(
        w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
        w!("AppsUseLightTheme"),
    ) == Some(0)
}

/// Returns `true` when the user has enabled "Show accent color on title bars
/// and window borders" in Windows Personalization settings. DWM then paints
/// the caption-buttons strip with the accent color, so the rest of our
/// custom title bar must match.
fn color_prevalence() -> bool {
    read_hkcu_dword(w!("Software\\Microsoft\\Windows\\DWM"), w!("ColorPrevalence"))
        == Some(1)
}

/// Computes the current title-bar color scheme from the system theme,
/// accent-color setting, DWM colorization color, and the window's focus
/// state so our custom title bar matches whatever DWM paints in the
/// caption-buttons strip (which switches to a neutral inactive color when
/// the window loses focus).
fn current_title_bar_theme(active: bool) -> TitleBarTheme {
    let dark = is_dark_mode();

    if !active {
        // DWM paints the caption-buttons strip in a neutral inactive color
        // regardless of the accent / color-prevalence setting. Match it.
        return inactive_theme(dark);
    }

    if color_prevalence() {
        // SAFETY: Out parameters are local stack values.
        let mut color: u32 = 0;
        let mut opaque = BOOL(0);
        let ok = unsafe { DwmGetColorizationColor(&mut color, &mut opaque) }.is_ok();
        if ok {
            // `color` is 0xAARRGGBB; COLORREF is 0x00BBGGRR.
            let r = ((color >> 16) & 0xFF) as u8;
            let g = ((color >> 8) & 0xFF) as u8;
            let b = (color & 0xFF) as u8;
            let bg = COLORREF(
                (r as u32) | ((g as u32) << 8) | ((b as u32) << 16),
            );
            // Use perceived luminance to pick a legible foreground.
            let lum = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
            let (text, hover) = if lum > 140.0 {
                (COLORREF(0x00000000), tint_color(r, g, b, -32))
            } else {
                (COLORREF(0x00FFFFFF), tint_color(r, g, b, 32))
            };
            return TitleBarTheme { background: bg, text, hover };
        }
    }

    if dark {
        TitleBarTheme {
            background: COLORREF(0x00000000),
            text: COLORREF(0x00FFFFFF),
            hover: COLORREF(0x00333333),
        }
    } else {
        TitleBarTheme {
            background: COLORREF(0x00FFFFFF),
            text: COLORREF(0x00000000),
            hover: COLORREF(0x00CCCCCC),
        }
    }
}

/// Title-bar colors used when the window is not the foreground window.
/// Approximates the neutral inactive caption color DWM paints behind the
/// caption buttons in Windows 10/11.
fn inactive_theme(dark: bool) -> TitleBarTheme {
    if dark {
        TitleBarTheme {
            background: COLORREF(0x002B2B2B),
            text: COLORREF(0x006B6B6B),
            hover: COLORREF(0x003F3F3F),
        }
    } else {
        TitleBarTheme {
            background: COLORREF(0x00F3F3F3),
            text: COLORREF(0x009B9B9B),
            hover: COLORREF(0x00DADADA),
        }
    }
}

/// Lightens (positive `delta`) or darkens (negative `delta`) the given
/// RGB color and returns it as a `COLORREF` (0x00BBGGRR).
fn tint_color(r: u8, g: u8, b: u8, delta: i32) -> COLORREF {
    let adjust = |c: u8| -> u8 {
        (c as i32 + delta).clamp(0, 255) as u8
    };
    let r = adjust(r) as u32;
    let g = adjust(g) as u32;
    let b = adjust(b) as u32;
    COLORREF(r | (g << 8) | (b << 16))
}

/// Creates the window and runs the Win32 message loop.
///
/// `start_hidden = true` creates the window without `WS_VISIBLE`, leaving
/// only the tray icon onscreen. The window can later be shown by sending
/// `show_window_message()` to it.
pub fn create_and_run(start_hidden: bool) -> windows::core::Result<()> {
    // SAFETY: Standard Win32 window registration / message loop. All handles
    // are owned by the OS for the lifetime of the process or by the window
    // state via `Drop`.
    unsafe {
        // Eagerly register the cross-process messages so their cached ids
        // are non-zero before any external sender (a second-instance
        // launch) can `PostMessageW` them to us. `wnd_proc` short-circuits
        // when the cached id is 0, so without this the first instance
        // would silently ignore the show/exit messages until something in
        // *this* process happened to register them first.
        let _ = show_window_message();
        let _ = exit_app_message();

        let hinstance: HINSTANCE = GetModuleHandleW(None)?.into();

        // Load the application icon from embedded resources (ID 1) at both
        // the large and small system sizes so the title-bar icon is crisp.
        // `PCWSTR(1 as *const u16)` is the Win32 `MAKEINTRESOURCE(1)` idiom
        // — the low word is interpreted as an integer resource id, not a
        // string pointer.
        #[allow(clippy::manual_dangling_ptr)]
        let icon_lg = LoadImageW(
            hinstance,
            PCWSTR(1 as *const u16),
            IMAGE_ICON,
            GetSystemMetrics(SM_CXICON),
            GetSystemMetrics(SM_CYICON),
            LR_SHARED,
        )?;
        #[allow(clippy::manual_dangling_ptr)]
        let icon_sm = LoadImageW(
            hinstance,
            PCWSTR(1 as *const u16),
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            LR_SHARED,
        )?;

        let wc = WNDCLASSEXW {
            cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance,
            lpszClassName: CLASS_NAME,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hIcon: HICON(icon_lg.0),
            hIconSm: HICON(icon_sm.0),
            hbrBackground: HBRUSH(GetStockObject(NULL_BRUSH).0),
            ..Default::default()
        };

        let atom = RegisterClassExW(&wc);
        if atom == 0 {
            return Err(Error::from_win32());
        }

        // Place the window on the primary monitor's work area.
        let work_area = geometry::get_monitor_work_area(GetDesktopWindow());
        let monitor_width = work_area.right - work_area.left;
        let monitor_height = work_area.bottom - work_area.top;
        let size = geometry::default_size(monitor_width, monitor_height);
        let pos = geometry::centered_position(size, work_area);

        // Build the window style. Omit `WS_VISIBLE` when starting hidden
        // so `CreateWindowExW` does not show the window even briefly
        // before we get a chance to call `SW_HIDE` — critical for the
        // `--minimized` auto-start path, where any flash on logon is very
        // visible. The window is later shown with `SW_SHOW`, which works
        // fine on a window created without `WS_VISIBLE`.
        let mut style = WS_OVERLAPPED
            | WS_CAPTION
            | WS_SYSMENU
            | WS_THICKFRAME
            | WS_MINIMIZEBOX
            | WS_MAXIMIZEBOX;
        if !start_hidden {
            style |= WS_VISIBLE;
        }

        let hwnd = CreateWindowExW(
            WS_EX_APPWINDOW,
            CLASS_NAME,
            WINDOW_TITLE,
            style,
            pos.x,
            pos.y,
            size.width,
            size.height,
            None,
            None,
            hinstance,
            None,
        )?;

        // Install the tray icon. Use the small icon for the notification
        // area. Failure here is non-fatal: the user can still use the
        // window directly via the taskbar (when visible). We do not show
        // an error dialog because the tray is a convenience feature.
        let tray_hwnd = tray::install(hwnd, HICON(icon_sm.0)).unwrap_or_default();
        // Stash the tray HWND on the main window so WM_DESTROY can clean
        // it up. WM_CREATE has already run by this point and constructed
        // the WindowState; mutate that state in place.
        with_state(hwnd, |state| state.tray_hwnd = tray_hwnd);

        apply_dark_mode(hwnd);
        apply_extended_frame(hwnd);

        // Force the window to re-evaluate its frame so subsequent
        // GetClientRect calls reflect the WM_NCCALCSIZE changes.
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );

        // Message loop. `GetMessageW` returns -1 on error; bail out rather
        // than spinning.
        let mut msg = MSG::default();
        loop {
            let r = GetMessageW(&mut msg, None, 0, 0).0;
            if r == 0 {
                break; // WM_QUIT
            }
            if r == -1 {
                return Err(Error::from_win32());
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        Ok(())
    }
}

/// Computes the title bar height from system metrics for the given window's
/// current DPI.
unsafe fn compute_title_bar_height(hwnd: HWND) -> i32 {
    let dpi = GetDpiForWindow(hwnd);
    let frame_y = GetSystemMetricsForDpi(SM_CYFRAME, dpi);
    let caption = GetSystemMetricsForDpi(SM_CYCAPTION, dpi);
    let padding = GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
    frame_y + caption + padding
}

/// Tells DWM to use a dark or light immersive title bar based on the current
/// system theme.
unsafe fn apply_dark_mode(hwnd: HWND) {
    let dark: BOOL = if is_dark_mode() { TRUE } else { FALSE };
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
        &dark as *const _ as *const _,
        mem::size_of_val(&dark) as u32,
    );
}

/// Extends the DWM frame into the client area so we can draw in the
/// title-bar region while DWM still renders the native caption buttons.
unsafe fn apply_extended_frame(hwnd: HWND) {
    let title_bar_height = compute_title_bar_height(hwnd);
    let margins = MARGINS {
        cxLeftWidth: 0,
        cxRightWidth: 0,
        cyTopHeight: title_bar_height,
        cyBottomHeight: 0,
    };
    let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
}

/// Borrows the per-window state stored in `GWLP_USERDATA` for the duration
/// of `f`. Returns `None` (without invoking `f`) when no state has been
/// attached yet.
///
/// Scoping the borrow inside a closure prevents two concurrent `&mut`
/// references to the same state from being constructed.
///
/// # Safety
///
/// Must only be called from the UI thread that owns `hwnd`.
unsafe fn with_state<R>(hwnd: HWND, f: impl FnOnce(&mut WindowState) -> R) -> Option<R> {
    let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
    if p.is_null() {
        None
    } else {
        Some(f(&mut *p))
    }
}

/// Decodes an `LPARAM` carrying a packed pair of signed 16-bit coordinates
/// (mouse messages) to `(x, y)` as `i32`.
fn lparam_to_xy(lparam: LPARAM) -> (i32, i32) {
    let x = (lparam.0 & 0xFFFF) as i16 as i32;
    let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
    (x, y)
}

/// Decodes an `LPARAM` carrying a packed pair of **unsigned** 16-bit
/// dimensions (used by `WM_SIZE`) to `(width, height)` as `i32`.
fn lparam_to_size(lparam: LPARAM) -> (i32, i32) {
    let w = (lparam.0 & 0xFFFF) as u16 as i32;
    let h = ((lparam.0 >> 16) & 0xFFFF) as u16 as i32;
    (w, h)
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Registered cross-process "show window" message from the tray icon
    // or a second-instance launch. Compare against the cached id (non-zero
    // once registered).
    let show_msg = SHOW_WINDOW_MSG.load(Ordering::Relaxed);
    if show_msg != 0 && msg == show_msg {
        restore_and_foreground(hwnd);
        return LRESULT(0);
    }
    // Registered "exit app" message from the tray's Exit menu item.
    let exit_msg = EXIT_APP_MSG.load(Ordering::Relaxed);
    if exit_msg != 0 && msg == exit_msg {
        // Triggers WM_DESTROY which tears down the tray and posts WM_QUIT.
        let _ = DestroyWindow(hwnd);
        return LRESULT(0);
    }

    // Answer shutdown / logoff queries immediately and unconditionally so
    // Windows never flags us as "preventing shutdown". Returning TRUE
    // here is the documented way to say "I'm ready to exit"; the OS will
    // follow up with WM_ENDSESSION. Doing this BEFORE DwmDefWindowProc
    // and the main match avoids any chance of a slower path delaying
    // the response past the shutdown manager's timeout.
    if msg == WM_QUERYENDSESSION {
        return LRESULT(1);
    }

    // Let DWM handle caption-button interactions for everything except the
    // messages we own (frame layout, client-area mouse handling, and the
    // close path — we want to intercept WM_CLOSE / SC_CLOSE before DWM or
    // DefWindowProc destroy the window, so the tray icon survives).
    if !matches!(
        msg,
        WM_NCHITTEST
            | WM_NCCALCSIZE
            | WM_MOUSEMOVE
            | WM_MOUSELEAVE
            | WM_LBUTTONUP
            | WM_CLOSE
            | WM_SYSCOMMAND
    ) {
        let mut dwm_result = LRESULT(0);
        if DwmDefWindowProc(hwnd, msg, wparam, lparam, &mut dwm_result).as_bool() {
            return dwm_result;
        }
    }

    match msg {
        WM_CREATE => on_create(hwnd),
        WM_SYSCOMMAND => {
            // The DWM caption × button generates WM_SYSCOMMAND with
            // wparam == SC_CLOSE (low 4 bits of wparam are reserved by
            // Windows, so mask them off per MSDN). Hide instead of close.
            // All other system commands (move, size, minimize, restore,
            // keyboard shortcuts) get default handling.
            let cmd = (wparam.0 as u32) & 0xFFF0;
            if cmd == SC_CLOSE {
                let _ = ShowWindow(hwnd, SW_HIDE);
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_CLOSE => {
            // Alt+F4 (and any code path that sends WM_CLOSE directly) lands
            // here. Hide the window instead of destroying it; the tray icon
            // keeps the process alive and the user reopens via the tray
            // menu. Returning 0 (without calling DefWindowProcW) suppresses
            // the default DestroyWindow behavior.
            let _ = ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }
        WM_ENDSESSION => {
            // Windows is logging off / shutting down (wparam != 0 means
            // the session really is ending; 0 means the prior
            // WM_QUERYENDSESSION was canceled). Per MSDN, the application
            // "can return prior to processing this message" and the
            // system "performs no further action if an application
            // returns immediately" — so return immediately. Calling
            // DestroyWindow here would cascade into WM_DESTROY →
            // tray::shutdown → DestroyWindow + Shell_NotifyIconW round-
            // trip to a shutting-down explorer.exe, which is exactly
            // what gets us flagged as "preventing shutdown".
            //
            // The tray icon is removed by the tray helper window's own
            // WM_ENDSESSION handler — it owns the icon and is also a
            // top-level window, so it receives WM_ENDSESSION too. We
            // intentionally do nothing here to avoid a second
            // NIM_DELETE round-trip on the shutdown critical path.
            LRESULT(0)
        }
        WM_DESTROY => on_destroy(hwnd),
        WM_TIMER => on_timer(hwnd),
        WM_PAINT => on_paint(hwnd),
        WM_SIZE => on_size(hwnd, lparam),
        WM_MOVE => {
            // `WM_MOVE` fires synchronously on every drag tick during a
            // user move (just like `WM_SIZE` during resize). Capturing here
            // — instead of waiting for the next 33 ms `WM_TIMER` — keeps
            // the content area aligned to the desktop in real time, so the
            // image no longer appears to slide behind the window while
            // dragging.
            //
            // Coalesce against pending paints: if `GetUpdateRect` reports
            // a non-empty update region, the prior captured frame hasn't
            // been displayed yet. Doing another `BitBlt` would just
            // overwrite it with no visible effect, so skip the capture and
            // let the queued `WM_PAINT` consume the existing frame. The
            // next `WM_MOVE` (or `WM_TIMER`) will recapture for the new
            // position. This caps capture work at one `BitBlt` per actual
            // displayed frame instead of one per OS-delivered move tick.
            let mut update = RECT::default();
            let has_pending_paint =
                GetUpdateRect(hwnd, Some(&mut update), FALSE).as_bool();
            if !has_pending_paint {
                with_state(hwnd, |state| {
                    state.capture.capture_frame();
                });
            }
            LRESULT(0)
        }
        WM_ENTERSIZEMOVE => {
            // Set `WDA_EXCLUDEFROMCAPTURE` and flush DWM once for the
            // duration of the modal move/size loop. Per-frame captures
            // during the loop then skip the affinity toggle and the
            // `DwmFlush` that follows it — saving ~16 ms per drag tick.
            with_state(hwnd, |state| state.capture.begin_interactive());
            LRESULT(0)
        }
        WM_EXITSIZEMOVE => {
            // Modal move/size loop ended; restore affinity so screen-share
            // apps can see us again, then force a final frame so the last
            // post-release position/size is correct.
            with_state(hwnd, |state| {
                state.capture.end_interactive();
                state.capture.capture_frame();
            });
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
            // Return 0 to make client area = full window rect, removing the
            // standard non-client frame so we can draw our own title bar.
            LRESULT(0)
        }
        WM_NCHITTEST => on_nc_hit_test(hwnd, msg, wparam, lparam),
        WM_DPICHANGED => on_dpi_changed(hwnd, lparam),
        WM_SETTINGCHANGE => {
            apply_dark_mode(hwnd);
            let _ = InvalidateRect(hwnd, None, FALSE);
            LRESULT(0)
        }
        WM_DWMCOLORIZATIONCOLORCHANGED => {
            // Accent color or its prevalence flag changed; repaint so our
            // custom title bar background tracks the DWM-painted strip.
            let _ = InvalidateRect(hwnd, None, FALSE);
            LRESULT(0)
        }
        WM_NCACTIVATE => {
            // Track focus so the custom title bar can match the inactive
            // color DWM paints in the caption-buttons strip. Forward to
            // `DefWindowProc` (with `wparam` preserved) so DWM still
            // updates the buttons themselves.
            let active = wparam.0 != 0;
            with_state(hwnd, |state| {
                if state.is_active != active {
                    state.is_active = active;
                    let _ = InvalidateRect(hwnd, None, FALSE);
                }
            });
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEMOVE => on_mouse_move(hwnd, lparam),
        WM_MOUSELEAVE => on_mouse_leave(hwnd),
        WM_LBUTTONUP => on_lbutton_up(hwnd, lparam),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn on_create(hwnd: HWND) -> LRESULT {
    let title_bar_height = compute_title_bar_height(hwnd);

    // Use client area for the capture buffer. WM_NCCALCSIZE has not yet
    // collapsed the standard frame at this point, but the bitmap will be
    // resized via WM_SIZE before the first paint.
    let mut client_rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut client_rect);
    let cw = (client_rect.right - client_rect.left).max(1);
    let ch = (client_rect.bottom - client_rect.top).max(1);
    let content_height = (ch - title_bar_height).max(1);

    let cap_state = CaptureState::new(hwnd, cw, content_height);

    let state = Box::new(WindowState {
        capture: cap_state,
        render_cache: RenderCache::default(),
        send_back_hovered: false,
        tracking_mouse: false,
        title_bar_height,
        // Newly created top-level windows are activated by the OS.
        is_active: true,
        tray_hwnd: HWND::default(),
    });

    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
    LRESULT(0)
}

unsafe fn on_destroy(hwnd: HWND) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
    if !ptr.is_null() {
        // Tear down the tray icon BEFORE dropping the box, so the tray's
        // wnd_proc can no longer post messages to a window whose state is
        // gone. `tray::shutdown` is a no-op for a default HWND.
        let tray_hwnd = (*ptr).tray_hwnd;
        tray::shutdown(tray_hwnd);

        // Drop the boxed state; CaptureState::drop releases GDI handles and
        // kills the timer while the HWND is still valid.
        drop(Box::from_raw(ptr));
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
    }
    PostQuitMessage(0);
    LRESULT(0)
}

unsafe fn on_timer(hwnd: HWND) -> LRESULT {
    with_state(hwnd, |state| {
        state.capture.capture_frame();
    });
    LRESULT(0)
}

unsafe fn on_paint(hwnd: HWND) -> LRESULT {
    let dpi = GetDpiForWindow(hwnd);
    let painted = with_state(hwnd, |state| {
        let theme = current_title_bar_theme(state.is_active);
        render::paint(
            hwnd,
            &state.capture,
            &mut state.render_cache,
            state.send_back_hovered,
            state.title_bar_height,
            dpi,
            theme,
        );
    });
    if painted.is_none() {
        let mut ps = PAINTSTRUCT::default();
        let _ = BeginPaint(hwnd, &mut ps);
        let _ = EndPaint(hwnd, &ps);
    }
    LRESULT(0)
}

unsafe fn on_size(hwnd: HWND, lparam: LPARAM) -> LRESULT {
    // `WM_SIZE` packs unsigned WORDs; sign-extension would silently drop
    // resizes wider than 32767 pixels.
    let (width, height) = lparam_to_size(lparam);
    with_state(hwnd, |state| {
        let content_height = height - state.title_bar_height;
        if width > 0 && content_height > 0 {
            state.capture.resize(width, content_height);
            if !state.capture.paused() {
                state.capture.capture_frame();
            }
        }
    });
    LRESULT(0)
}

unsafe fn on_nc_hit_test(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let (x, y) = lparam_to_xy(lparam);

    let mut rect = RECT::default();
    let _ = GetWindowRect(hwnd, &mut rect);

    let margin = geometry::RESIZE_MARGIN;
    let caption_buttons_width = caption_buttons_width(hwnd);
    let tb_height = with_state(hwnd, |s| s.title_bar_height)
        .unwrap_or_else(|| compute_title_bar_height(hwnd));

    // The DWM caption buttons live in the top-right strip (within the title
    // bar height). Suppress border resize hits there so the close/maximize
    // buttons remain clickable; everywhere else the right edge resizes.
    let in_caption_buttons_strip =
        x >= rect.right - caption_buttons_width && y < rect.top + tb_height;

    if y < rect.top + margin && !in_caption_buttons_strip {
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
    if x >= rect.right - margin && !in_caption_buttons_strip {
        return LRESULT(HTRIGHT as isize);
    }

    if y < rect.top + tb_height {
        // "Send to Back" button — left of the DWM caption buttons.
        let client_width = rect.right - rect.left;
        let client_x = x - rect.left;
        let (button_left, button_right) =
            geometry::send_back_button_range(client_width, caption_buttons_width);
        if client_x >= button_left && client_x < button_right {
            return LRESULT(HTCLIENT as isize);
        }

        // Let DWM check if cursor is over a caption button (close/max).
        let mut dwm_result = LRESULT(0);
        if DwmDefWindowProc(hwnd, msg, wparam, lparam, &mut dwm_result).as_bool() {
            return dwm_result;
        }

        return LRESULT(HTCAPTION as isize);
    }

    LRESULT(HTCLIENT as isize)
}

unsafe fn on_dpi_changed(hwnd: HWND, lparam: LPARAM) -> LRESULT {
    // Recompute title-bar metrics; refresh the extended frame and any
    // DPI-dependent render cache.
    with_state(hwnd, |state| {
        state.title_bar_height = compute_title_bar_height(hwnd);
        state.render_cache.invalidate_dpi_dependent();
    });
    apply_extended_frame(hwnd);

    // Use the suggested rect from lparam to reposition for the new DPI.
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
    let _ = InvalidateRect(hwnd, None, FALSE);
    LRESULT(0)
}

unsafe fn on_mouse_move(hwnd: HWND, lparam: LPARAM) -> LRESULT {
    let (x, y) = lparam_to_xy(lparam);
    let mut client_rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut client_rect);
    let cw = client_rect.right - client_rect.left;
    let cap_w = caption_buttons_width(hwnd);

    with_state(hwnd, |state| {
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

        let hovered = geometry::point_in_send_back_button(x, y, cw, state.title_bar_height, cap_w);
        if hovered != state.send_back_hovered {
            state.send_back_hovered = hovered;
            let (button_left, button_right) = geometry::send_back_button_range(cw, cap_w);
            let btn_rect = RECT {
                left: button_left,
                top: 0,
                right: button_right,
                bottom: state.title_bar_height,
            };
            let _ = InvalidateRect(hwnd, Some(&btn_rect), FALSE);
        }
    });
    LRESULT(0)
}

unsafe fn on_mouse_leave(hwnd: HWND) -> LRESULT {
    with_state(hwnd, |state| {
        state.tracking_mouse = false;
        if state.send_back_hovered {
            state.send_back_hovered = false;
            let _ = InvalidateRect(hwnd, None, FALSE);
        }
    });
    LRESULT(0)
}

unsafe fn on_lbutton_up(hwnd: HWND, lparam: LPARAM) -> LRESULT {
    let tb_height = with_state(hwnd, |s| s.title_bar_height)
        .unwrap_or_else(|| compute_title_bar_height(hwnd));

    let (x, y) = lparam_to_xy(lparam);
    let mut client_rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut client_rect);
    let cw = client_rect.right - client_rect.left;
    let cap_w = caption_buttons_width(hwnd);

    if geometry::point_in_send_back_button(x, y, cw, tb_height, cap_w) {
        let _ = SetWindowPos(
            hwnd,
            HWND_BOTTOM,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
        // Activate the topmost visible application window now that we moved
        // to the back of the Z-order.
        if let Some(top) = find_topmost_app_window(hwnd) {
            let _ = SetForegroundWindow(top);
        }
    }
    LRESULT(0)
}

/// Finds the topmost visible, non-minimized application window in Z-order,
/// skipping `exclude`. Returns `None` if no suitable window is found.
unsafe fn find_topmost_app_window(exclude: HWND) -> Option<HWND> {
    let Ok(mut current) = GetWindow(GetDesktopWindow(), GW_CHILD) else {
        return None;
    };
    loop {
        if current != exclude
            && IsWindowVisible(current).as_bool()
            && !IsIconic(current).as_bool()
        {
            let ex_style = GetWindowLongPtrW(current, GWL_EXSTYLE) as u32;
            if (ex_style & WS_EX_TOOLWINDOW.0) == 0 {
                return Some(current);
            }
        }
        current = match GetWindow(current, GW_HWNDNEXT) {
            Ok(next) => next,
            Err(_) => return None,
        };
    }
}

/// Returns the width of DWM-drawn caption buttons (close + maximize +
/// minimize), falling back to a constant if the API fails.
unsafe fn caption_buttons_width(hwnd: HWND) -> i32 {
    geometry::caption_buttons_width(hwnd)
}

/// Reliably brings `hwnd` to the foreground, showing it if hidden and
/// restoring it if minimized. Used by the tray Open command and by a
/// second-instance launch.
///
/// `SetForegroundWindow` alone is unreliable when the calling thread does
/// not own the foreground. The topmost-then-not-topmost dance is the
/// well-known Win32 workaround that does not require attaching to the
/// foreground thread's input queue.
unsafe fn restore_and_foreground(hwnd: HWND) {
    if !IsWindowVisible(hwnd).as_bool() {
        let _ = ShowWindow(hwnd, SW_SHOW);
    }
    if IsIconic(hwnd).as_bool() {
        let _ = ShowWindow(hwnd, SW_RESTORE);
    }
    let _ = SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
    );
    let _ = SetWindowPos(
        hwnd,
        HWND_NOTOPMOST,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
    );
    let _ = SetForegroundWindow(hwnd);
    let _ = BringWindowToTop(hwnd);
    // Final fallback: `SwitchToThisWindow` is undocumented but widely
    // used and reliably brings a window to the foreground when
    // `SetForegroundWindow` is denied (e.g. when our process does not
    // own the foreground at the moment of the call).
    SwitchToThisWindow(hwnd, TRUE);
}
