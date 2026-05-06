//! Modal settings dialog.
//!
//! There is exactly one setting (launch at Windows startup), so the
//! native Task Dialog with its built-in verification checkbox is a
//! perfect fit — it gives us a properly themed modal dialog with
//! keyboard handling (Tab/Enter/Esc) and DPI scaling for free.

use std::mem;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::WindowsAndMessaging::IDOK;

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
        // TDF_POSITION_RELATIVE_TO_WINDOW centers the dialog over the
        // owner instead of the screen.
        let mut flags = TDF_POSITION_RELATIVE_TO_WINDOW;
        if initial {
            flags |= TDF_VERIFICATION_FLAG_CHECKED;
        }
        config.dwFlags = flags;
        config.dwCommonButtons = TDCBF_OK_BUTTON | TDCBF_CANCEL_BUTTON;
        config.pszWindowTitle = PCWSTR(title.as_ptr());
        config.pszMainInstruction = PCWSTR(main.as_ptr());
        config.pszContent = PCWSTR(content.as_ptr());
        config.pszVerificationText = PCWSTR(verify.as_ptr());

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
