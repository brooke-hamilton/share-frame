#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod geometry;
mod render;
mod window;

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

fn main() {
    unsafe {
        // Set per-monitor DPI awareness before any window creation
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        // `--minimized` (used by the auto-start registry entry) launches
        // with no visible window — only the tray icon appears.
        let start_hidden = std::env::args().any(|a| a == "--minimized");

        // Single-instance enforcement via named mutex.
        // The handle MUST live for the lifetime of `main` so the OS sees the
        // mutex as held while this instance is running. `HANDLE` has no `Drop`
        // impl, so the binding (named, not `_`) keeps it in scope until
        // `main` returns; the kernel then releases the named mutex on
        // process exit.
        let mutex_name = w!("ShareFrame_SingleInstance_Mutex");
        let _instance_lock = match CreateMutexW(None, FALSE, mutex_name) {
            Ok(handle) => {
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    // Another instance is running — close our duplicate
                    // handle, post the registered "show window" message to
                    // the existing instance (works whether the window is
                    // hidden or visible), and exit. `SetForegroundWindow`
                    // would silently fail when the target window is hidden;
                    // the existing instance restores + foregrounds itself
                    // in response to the message.
                    let _ = CloseHandle(handle);
                    if let Ok(existing) = FindWindowW(w!("ShareFrameClass"), w!("Share Frame")) {
                        let msg = window::show_window_message();
                        if msg != 0 {
                            let _ = AllowSetForegroundWindow(ASFW_ANY);
                            let _ = PostMessageW(existing, msg, WPARAM(0), LPARAM(0));
                        }
                    }
                    return;
                }
                Some(handle)
            }
            Err(_) => {
                // Mutex creation failed — proceed without single-instance enforcement.
                None
            }
        };

        // Create and run the main window
        if let Err(e) = window::create_and_run(start_hidden) {
            let mut wide: Vec<u16> = format!("Share Frame failed to start:\n{e}")
                .encode_utf16()
                .collect();
            wide.push(0);
            let _ = MessageBoxW(
                None,
                PCWSTR(wide.as_ptr()),
                w!("Share Frame Error"),
                MB_OK | MB_ICONERROR,
            );
        }

        // The named binding keeps `_instance_lock` alive until the end of
        // `main`. `HANDLE` is `Copy` and has no `Drop`, so an explicit drop
        // would be a no-op; relying on scope is sufficient.
    }
}
