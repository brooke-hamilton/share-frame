//! Windows notification-area (system tray) icon for Share Frame.
//!
//! Owns a hidden top-level window that receives `Shell_NotifyIconW`
//! callbacks. We deliberately use an invisible top-level window
//! (`WS_POPUP` + `WS_EX_TOOLWINDOW`, parked offscreen) rather than a
//! `HWND_MESSAGE` window for two reasons:
//!
//! 1. The shell broadcasts the `TaskbarCreated` message ONLY to top-level
//!    windows, so a message-only window would silently miss explorer
//!    restarts and the icon would vanish forever.
//! 2. `SetForegroundWindow` (required by the documented tray-menu
//!    dismissal pattern) does not work on message-only windows.
//!
//! Left-click and the "Open" menu item ask the main window to restore +
//! foreground itself by posting the registered show-window message. The
//! "Exit" menu item posts the registered exit message, which the main
//! window handles by destroying itself; that triggers the normal
//! WM_DESTROY cleanup path (which also calls `shutdown` on this tray).
//! The "Settings" menu item is handled entirely on the tray side — the
//! dialog parents off the invisible tray window so the main window can
//! stay hidden.

use std::mem;
use std::sync::atomic::{AtomicU32, Ordering};

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::settings_dialog;
use crate::window;

const TRAY_CLASS: PCWSTR = w!("ShareFrameTrayClass");
const TRAY_TIP_RAW: &str = "Share Frame";
/// Callback message Shell_NotifyIcon will send to our hidden window.
const WM_APP_TRAY: u32 = WM_APP + 1;
/// Stable id for our single tray icon (per-window scope).
const TRAY_ICON_UID: u32 = 1;

const IDM_OPEN: u32 = 100;
const IDM_SETTINGS: u32 = 101;
const IDM_EXIT: u32 = 102;

/// Cached id of the OS-broadcast `TaskbarCreated` message. Sent to all
/// top-level windows when explorer.exe restarts; we re-add the icon.
static TASKBAR_CREATED_MSG: AtomicU32 = AtomicU32::new(0);

/// Per-tray-window state stored in `GWLP_USERDATA`.
struct TrayState {
    /// HWND of the main Share Frame window. Tray commands post messages to
    /// it.
    main_hwnd: HWND,
    /// Icon to (re)register with the shell. Held as a raw `HICON`; the
    /// caller of `install` retains ownership (typically an `LR_SHARED`
    /// resource icon whose lifetime matches the process).
    hicon: HICON,
}

/// Creates the tray icon and its hidden owner window. The returned HWND
/// identifies the tray's invisible top-level window; pass it to
/// `shutdown` when tearing the app down to remove the icon and destroy
/// the window.
///
/// `main_hwnd` must outlive the tray (the tray holds it as a raw handle).
/// `hicon` is the icon shown in the notification area; it is not owned by
/// the tray (typically a shared resource icon loaded with `LR_SHARED`).
pub fn install(main_hwnd: HWND, hicon: HICON) -> windows::core::Result<HWND> {
    // SAFETY: Standard Win32 window-class registration / window creation.
    unsafe {
        let hinstance: HINSTANCE = GetModuleHandleW(None)?.into();

        // Register the class once per process. `RegisterClassExW` returns
        // 0 on failure; the only non-fatal failure is
        // `ERROR_CLASS_ALREADY_EXISTS` (this function being re-entered).
        let wc = WNDCLASSEXW {
            cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(tray_wnd_proc),
            hInstance: hinstance,
            lpszClassName: TRAY_CLASS,
            ..Default::default()
        };
        let atom = RegisterClassExW(&wc);
        if atom == 0 {
            let err = GetLastError();
            if err != ERROR_CLASS_ALREADY_EXISTS {
                return Err(Error::from_win32());
            }
        }

        // Cache the broadcast TaskbarCreated message id so the wnd_proc
        // can compare against it cheaply.
        let tc = RegisterWindowMessageW(w!("TaskbarCreated"));
        TASKBAR_CREATED_MSG.store(tc, Ordering::Relaxed);

        // Invisible top-level window: WS_POPUP avoids a frame, no
        // WS_VISIBLE means it never paints, WS_EX_TOOLWINDOW keeps it out
        // of Alt+Tab and the taskbar. Parked at the documented offscreen
        // sentinel coordinates. Top-level (not HWND_MESSAGE) so the
        // shell's `TaskbarCreated` broadcast actually reaches us and so
        // `SetForegroundWindow` works for the popup-menu dismissal dance.
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            TRAY_CLASS,
            w!("ShareFrameTray"),
            WS_POPUP,
            -32000,
            -32000,
            0,
            0,
            None,
            None,
            hinstance,
            None,
        )?;

        // Box the state and stash on the tray window.
        let state = Box::new(TrayState { main_hwnd, hicon });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);

        if let Err(e) = add_icon(hwnd, hicon) {
            // Best-effort: destroy the helper window so we don't leak it.
            let _ = DestroyWindow(hwnd);
            return Err(e);
        }

        Ok(hwnd)
    }
}

/// Removes the tray icon and destroys the hidden owner window. Safe to
/// call with a default/zero HWND (no-op).
pub fn shutdown(tray_hwnd: HWND) {
    if tray_hwnd == HWND::default() {
        return;
    }
    // SAFETY: NIM_DELETE + DestroyWindow on a window we own.
    unsafe {
        let mut nid = base_nid(tray_hwnd);
        let _ = Shell_NotifyIconW(NIM_DELETE, &mut nid);
        let _ = DestroyWindow(tray_hwnd);
    }
}

/// Builds the minimal NOTIFYICONDATAW with `hWnd` + `uID` set so it
/// identifies our single tray entry across NIM_ADD/MODIFY/DELETE calls.
unsafe fn base_nid(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW {
        cbSize: mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ICON_UID,
        ..Default::default()
    };
    // Anonymous union; default-init leaves it zeroed which is correct
    // here (we only set uVersion when we explicitly want to).
    nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    nid
}

/// Sends `NIM_ADD` for the tray icon; on success, `NIM_SETVERSION` so the
/// callback uses the modern packed lparam format we expect.
unsafe fn add_icon(hwnd: HWND, hicon: HICON) -> windows::core::Result<()> {
    let mut nid = base_nid(hwnd);
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
    nid.uCallbackMessage = WM_APP_TRAY;
    nid.hIcon = hicon;

    // szTip is a fixed-size [u16; 128] — write our tooltip then ensure the
    // last cell is the null terminator.
    let tip: Vec<u16> = TRAY_TIP_RAW.encode_utf16().collect();
    let copy_len = tip.len().min(nid.szTip.len() - 1);
    nid.szTip[..copy_len].copy_from_slice(&tip[..copy_len]);
    nid.szTip[copy_len] = 0;

    if !Shell_NotifyIconW(NIM_ADD, &mut nid).as_bool() {
        return Err(Error::from_win32());
    }
    // NIM_SETVERSION upgrades the callback format; failure is non-fatal
    // (we still get callbacks, just in the legacy format).
    let _ = Shell_NotifyIconW(NIM_SETVERSION, &mut nid);
    Ok(())
}

/// `WM_APP_TRAY` packs the original mouse message in the LOW word of
/// lparam under NOTIFYICON_VERSION_4 (and as the entire lparam under the
/// legacy versions). Extracting just the low word handles both.
fn tray_event(lparam: LPARAM) -> u32 {
    (lparam.0 & 0xFFFF) as u32
}

unsafe extern "system" fn tray_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Re-create the icon when explorer restarts. The shell broadcasts
    // `TaskbarCreated` to all top-level windows after it rebuilds the
    // notification area; without re-adding our icon it would be lost
    // until the user restarts the app — and with hide-on-close they have
    // no way to reach the app at all.
    let tc = TASKBAR_CREATED_MSG.load(Ordering::Relaxed);
    if tc != 0 && msg == tc {
        let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayState;
        if !p.is_null() {
            let _ = add_icon(hwnd, (*p).hicon);
        }
        return LRESULT(0);
    }

    match msg {
        // Answer shutdown queries immediately so Windows doesn't flag
        // this top-level window (the invisible tray helper) as
        // "preventing shutdown". Returning TRUE tells the OS we're
        // ready; WM_ENDSESSION will follow.
        WM_QUERYENDSESSION => LRESULT(1),
        WM_ENDSESSION => {
            // Session is really ending (wparam != 0). Per MSDN we can
            // return immediately; the OS will terminate the process.
            // Pull the icon out of the notification area inline so it
            // doesn't ghost there until the next mouse hover. Avoid
            // calling DestroyWindow here — the cascading message
            // processing is what makes Windows show the "preventing
            // shutdown" UI.
            if wparam.0 != 0 {
                let nid = base_nid(hwnd);
                let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            }
            LRESULT(0)
        }
        WM_APP_TRAY => {
            let event = tray_event(lparam);
            match event {
                // Single left-click — same as Open.
                x if x == WM_LBUTTONUP => {
                    post_show(hwnd);
                    LRESULT(0)
                }
                // Right-click or keyboard context menu — show menu.
                x if x == WM_RBUTTONUP || x == WM_CONTEXTMENU => {
                    show_context_menu(hwnd);
                    LRESULT(0)
                }
                _ => LRESULT(0),
            }
        }
        WM_COMMAND => {
            // LOWORD(wparam) is the menu id.
            let cmd = (wparam.0 & 0xFFFF) as u32;
            match cmd {
                IDM_OPEN => post_show(hwnd),
                IDM_SETTINGS => {
                    // Parent the modal off the tray's invisible top-level
                    // window so the main window can stay hidden. The
                    // single-thread message loop continues to pump while
                    // the dialog is modal.
                    settings_dialog::show(hwnd);
                }
                IDM_EXIT => post_exit(hwnd),
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            // Drop our boxed state. The icon should already have been
            // removed via `shutdown`, but call NIM_DELETE again just in
            // case (idempotent on a missing icon: returns FALSE, no harm).
            let mut nid = base_nid(hwnd);
            let _ = Shell_NotifyIconW(NIM_DELETE, &mut nid);

            let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut TrayState;
            if !p.is_null() {
                drop(Box::from_raw(p));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Reads the main HWND out of tray state and posts the registered
/// show-window message. No-op if state is missing. The tray and main
/// window run on the same thread and in the same process, so no
/// `AllowSetForegroundWindow` dance is needed.
unsafe fn post_show(tray_hwnd: HWND) {
    if let Some(main) = main_hwnd(tray_hwnd) {
        let msg = window::show_window_message();
        if msg != 0 {
            let _ = PostMessageW(main, msg, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe fn post_exit(tray_hwnd: HWND) {
    if let Some(main) = main_hwnd(tray_hwnd) {
        let msg = window::exit_app_message();
        if msg != 0 {
            let _ = PostMessageW(main, msg, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe fn main_hwnd(tray_hwnd: HWND) -> Option<HWND> {
    let p = GetWindowLongPtrW(tray_hwnd, GWLP_USERDATA) as *mut TrayState;
    if p.is_null() { None } else { Some((*p).main_hwnd) }
}

/// Builds and tracks the right-click context menu. Microsoft's documented
/// pattern requires `SetForegroundWindow` before `TrackPopupMenu` and a
/// dummy `PostMessage` afterwards so the menu dismisses correctly when
/// the user clicks elsewhere. Our owner is an invisible top-level (not a
/// message-only window) so `SetForegroundWindow` actually works.
unsafe fn show_context_menu(tray_hwnd: HWND) {
    let menu = match CreatePopupMenu() {
        Ok(m) => m,
        Err(_) => return,
    };

    let _ = AppendMenuW(menu, MF_STRING, IDM_OPEN as usize, w!("Open"));
    let _ = AppendMenuW(menu, MF_STRING, IDM_SETTINGS as usize, w!("Settings"));
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    let _ = AppendMenuW(menu, MF_STRING, IDM_EXIT as usize, w!("Exit"));

    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);

    let _ = SetForegroundWindow(tray_hwnd);
    let _ = TrackPopupMenu(
        menu,
        TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
        pt.x,
        pt.y,
        0,
        tray_hwnd,
        None,
    );
    // Per MSDN, post a benign message so the menu dismisses cleanly when
    // the user clicks outside it.
    let _ = PostMessageW(tray_hwnd, WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(menu);
}
