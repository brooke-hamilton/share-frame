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

        // Single-instance enforcement via named mutex.
        // The handle MUST live for the lifetime of `main` so the OS sees the
        // mutex as held while this instance is running. `HANDLE` has no `Drop`
        // impl, so binding it (without `CloseHandle`) is sufficient — the
        // kernel keeps the named mutex alive until the process exits.
        let mutex_name = w!("ShareFrame_SingleInstance_Mutex");
        let _mutex_guard = match CreateMutexW(None, FALSE, mutex_name) {
            Ok(handle) => {
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    // Another instance is running — close our duplicate
                    // handle, foreground the existing window, and exit.
                    let _ = CloseHandle(handle);
                    if let Ok(existing) = FindWindowW(w!("ShareFrameClass"), w!("Share Frame")) {
                        let _ = SetForegroundWindow(existing);
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
        if let Err(e) = window::create_and_run() {
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

        // Reference the guard so the binding survives the message loop.
        let _ = &_mutex_guard;
    }
}
