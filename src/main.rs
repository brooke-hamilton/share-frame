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

        // Single-instance enforcement via named mutex
        let mutex_name = w!("ShareFrame_SingleInstance_Mutex");
        let mutex = CreateMutexW(None, FALSE, mutex_name);

        match mutex {
            Ok(_handle) => {
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    // Another instance is running — foreground it and exit
                    if let Ok(existing) = FindWindowW(w!("ShareFrameClass"), w!("Share Frame")) {
                        let _ = SetForegroundWindow(existing);
                    }
                    return;
                }
            }
            Err(_) => {
                // Mutex creation failed — proceed without single-instance enforcement
            }
        }

        // Create and run the main window
        if let Err(e) = window::create_and_run() {
            let msg = format!("Share Frame failed to start:\n{}\0", e);
            let wide: Vec<u16> = msg.encode_utf16().collect();
            let _ = MessageBoxW(
                None,
                PCWSTR(wide.as_ptr()),
                w!("Share Frame Error"),
                MB_OK | MB_ICONERROR,
            );
        }
    }
}
