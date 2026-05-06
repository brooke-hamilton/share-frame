//! Persistent user settings for Share Frame.
//!
//! The only current setting is "launch at Windows startup". The Windows
//! Run key (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`) is both
//! the storage and the mechanism, so no separate config file is needed:
//! presence of our value enables auto-start; absence disables it.

use std::mem;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::*;

/// HKCU subkey listing programs to launch at user logon.
const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");

/// Value name under `RUN_KEY` we own. Choose a stable identifier so we
/// can remove our own entry without touching anything else.
const APP_VALUE_NAME: PCWSTR = w!("ShareFrame");

/// Returns `true` when our auto-start entry is present in HKCU\...\Run.
/// Returns `false` if the key/value is missing or unreadable.
pub fn is_startup_enabled() -> bool {
    // SAFETY: Standard registry read; handle closed before return.
    unsafe {
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, 0, KEY_READ, &mut hkey).is_err() {
            return false;
        }
        let mut data_size: u32 = 0;
        // Pass null buffer to query the value's existence and size.
        let result = RegQueryValueExW(
            hkey,
            APP_VALUE_NAME,
            None,
            None,
            None,
            Some(&mut data_size),
        );
        let _ = RegCloseKey(hkey);
        result.is_ok()
    }
}

/// Adds (or removes) our auto-start entry. The command line written
/// includes `--minimized` so the boot launch starts in tray-only mode.
pub fn set_startup_enabled(enabled: bool) -> windows::core::Result<()> {
    // SAFETY: Single-key open + one set/delete call + close.
    unsafe {
        let mut hkey = HKEY::default();
        // KEY_WRITE is needed to set or delete a value. KEY_READ included
        // for symmetry; safe to combine.
        let open = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            0,
            KEY_READ | KEY_WRITE,
            &mut hkey,
        );
        if open.is_err() {
            return Err(Error::from_win32());
        }

        let result = if enabled {
            write_run_value(hkey)
        } else {
            // ERROR_FILE_NOT_FOUND on a missing value is fine; treat
            // "already disabled" as success.
            let r = RegDeleteValueW(hkey, APP_VALUE_NAME);
            if r.is_ok() || r == ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                Err(Error::from_win32())
            }
        };
        let _ = RegCloseKey(hkey);
        result
    }
}

/// Builds the command line to register and writes it as a `REG_SZ` value
/// under the already-opened Run key.
unsafe fn write_run_value(hkey: HKEY) -> windows::core::Result<()> {
    let exe = current_exe_path()?;

    // Quote the path so spaces in the install location are handled
    // correctly. Format: "C:\path\share-frame.exe" --minimized
    let mut command: Vec<u16> = Vec::with_capacity(exe.len() + 16);
    command.push(b'"' as u16);
    command.extend_from_slice(&exe);
    command.push(b'"' as u16);
    command.push(b' ' as u16);
    for c in "--minimized".encode_utf16() {
        command.push(c);
    }
    command.push(0); // null terminator

    // Byte length includes the terminating null per RegSetValueExW spec.
    let cb = (command.len() * mem::size_of::<u16>()) as u32;
    let r = RegSetValueExW(
        hkey,
        APP_VALUE_NAME,
        0,
        REG_SZ,
        Some(std::slice::from_raw_parts(
            command.as_ptr() as *const u8,
            cb as usize,
        )),
    );
    if r.is_err() {
        return Err(Error::from_win32());
    }
    Ok(())
}

/// Returns the current executable path as a UTF-16 vector WITHOUT a
/// trailing null. Grows the buffer on truncation.
unsafe fn current_exe_path() -> windows::core::Result<Vec<u16>> {
    let mut buf = vec![0u16; 260];
    loop {
        let len = GetModuleFileNameW(None, &mut buf);
        if len == 0 {
            return Err(Error::from_win32());
        }
        // Truncation: returned length equals buffer size and last error is
        // ERROR_INSUFFICIENT_BUFFER. Grow and retry.
        if len as usize == buf.len() && GetLastError() == ERROR_INSUFFICIENT_BUFFER {
            buf.resize(buf.len() * 2, 0);
            continue;
        }
        buf.truncate(len as usize);
        return Ok(buf);
    }
}
