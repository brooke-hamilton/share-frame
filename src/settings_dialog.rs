//! Modal settings dialog.
//!
//! There is exactly one setting (launch at Windows startup), so the
//! native Task Dialog with its built-in verification checkbox is a
//! perfect fit — it gives us a properly themed modal dialog with
//! keyboard handling (Tab/Enter/Esc) and DPI scaling for free.

use std::mem;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowRect, IDOK, SWP_NOSIZE, SWP_NOZORDER, SetWindowPos,
};

use crate::settings;

/// Shows the settings dialog modal to `parent`. Reads current state from
/// the registry, displays a checkbox, and on OK applies the change.
pub fn show(parent: HWND) {
    let initial = settings::is_startup_enabled();

    // All TASKDIALOGCONFIG strings must be null-terminated wide strings
    // that outlive the call. Keep the backing Vecs in scope.
    let title = wide("Share Frame Settings");
    let main = wide("Share Frame");
    let content = wide("Configure how Share Frame starts.");
    let verify = wide("Launch Share Frame when Windows starts");

    // SAFETY: All pointers come from local Vecs that outlive the
    // synchronous TaskDialogIndirect call. The struct is zero-initialized
    // because TASKDIALOGCONFIG contains a function-pointer field (callback)
    // that does not implement Default.
    unsafe {
        let mut config: TASKDIALOGCONFIG = mem::zeroed();
        config.cbSize = mem::size_of::<TASKDIALOGCONFIG>() as u32;
        config.hwndParent = parent;
        // Note: we deliberately do NOT set TDF_POSITION_RELATIVE_TO_WINDOW.
        // The tray's owner window is parked offscreen at (-32000, -32000),
        // so centering relative to it would push the dialog into a
        // clamped corner of the primary monitor. Instead we let the
        // dialog spawn at its default location and reposition it onto
        // the monitor containing the cursor (which, since the user just
        // clicked the tray icon, is the monitor with the system tray)
        // from the TDN_DIALOG_CONSTRUCTED callback.
        let mut flags = TASKDIALOG_FLAGS(0);
        if initial {
            flags |= TDF_VERIFICATION_FLAG_CHECKED;
        }
        config.dwFlags = flags;
        config.dwCommonButtons = TDCBF_OK_BUTTON | TDCBF_CANCEL_BUTTON;
        config.pszWindowTitle = PCWSTR(title.as_ptr());
        config.pszMainInstruction = PCWSTR(main.as_ptr());
        config.pszContent = PCWSTR(content.as_ptr());
        config.pszVerificationText = PCWSTR(verify.as_ptr());
        config.pfCallback = Some(task_dialog_callback);

        let mut button: i32 = 0;
        let mut checked = BOOL(0);
        if TaskDialogIndirect(&config, Some(&mut button), None, Some(&mut checked)).is_err() {
            return;
        }

        if button == IDOK.0 {
            let new_state = checked.as_bool();
            if new_state != initial {
                // Best-effort: silently ignore registry failures (e.g.
                // user lacks HKCU write access in some locked-down env).
                let _ = settings::set_startup_enabled(new_state);
            }
        }
    }
}

/// Encodes a string as null-terminated UTF-16, suitable for `PCWSTR`.
fn wide(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}

/// Task Dialog notification callback. We only handle
/// `TDN_DIALOG_CONSTRUCTED` to recenter the dialog onto the monitor that
/// contains the cursor — i.e. the monitor whose system tray the user
/// just clicked.
unsafe extern "system" fn task_dialog_callback(
    hwnd: HWND,
    msg: TASKDIALOG_NOTIFICATIONS,
    _wparam: WPARAM,
    _lparam: LPARAM,
    _ref_data: isize,
) -> HRESULT {
    if msg == TDN_DIALOG_CONSTRUCTED {
        center_on_cursor_monitor(hwnd);
    }
    HRESULT(0)
}

/// Moves `hwnd` so it is centered on the work area of the monitor that
/// currently contains the cursor. Silently no-ops on any API failure;
/// the dialog stays at its default location in that case.
unsafe fn center_on_cursor_monitor(hwnd: HWND) {
    let mut cursor = POINT::default();
    if GetCursorPos(&mut cursor).is_err() {
        return;
    }
    let monitor: HMONITOR = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
    let mut mi = MONITORINFO {
        cbSize: mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !GetMonitorInfoW(monitor, &mut mi).as_bool() {
        return;
    }
    let mut rect = RECT::default();
    if GetWindowRect(hwnd, &mut rect).is_err() {
        return;
    }
    let dlg_w = rect.right - rect.left;
    let dlg_h = rect.bottom - rect.top;
    let work_w = mi.rcWork.right - mi.rcWork.left;
    let work_h = mi.rcWork.bottom - mi.rcWork.top;
    let x = mi.rcWork.left + (work_w - dlg_w) / 2;
    let y = mi.rcWork.top + (work_h - dlg_h) / 2;
    let _ = SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
}
