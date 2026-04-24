use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::geometry;

const TIMER_ID: usize = 1;
const FRAME_INTERVAL_MS: u32 = 33;

pub struct CaptureState {
    pub timer_id: usize,
    pub memory_dc: HDC,
    pub bitmap: HBITMAP,
    pub width: i32,
    pub height: i32,
    pub capture_ok: bool,
}

/// Initializes capture state: creates memory DC, compatible bitmap, starts timer.
/// Width/height are logical pixels; converted to physical using dpi.
pub fn init(hwnd: HWND, width: i32, height: i32, dpi: u32) -> CaptureState {
    let phys_w = geometry::logical_to_physical(width, dpi);
    let phys_h = geometry::logical_to_physical(height, dpi);

    unsafe {
        let screen_dc = GetDC(None);
        let memory_dc = CreateCompatibleDC(screen_dc);
        let bitmap = CreateCompatibleBitmap(screen_dc, phys_w, phys_h);
        SelectObject(memory_dc, bitmap);
        ReleaseDC(None, screen_dc);

        let timer_id = SetTimer(hwnd, TIMER_ID, FRAME_INTERVAL_MS, None);

        CaptureState {
            timer_id,
            memory_dc,
            bitmap,
            width: phys_w,
            height: phys_h,
            capture_ok: true,
        }
    }
}

/// Captures the desktop region behind the window.
/// Temporarily excludes the window from screen capture via display affinity
/// so BitBlt does not pick up the window itself (avoids recursive self-capture).
/// The window stays fully visible on the physical monitor — no flicker.
pub fn capture_frame(hwnd: HWND, state: &mut CaptureState) -> bool {
    unsafe {
        // Hide from screen-capture APIs (Win10 2004+). The window stays visible
        // on the physical display, so there is no user-visible flicker.
        let _ = SetWindowDisplayAffinity(hwnd, WINDOW_DISPLAY_AFFINITY(0x11)); // WDA_EXCLUDEFROMCAPTURE

        // Wait for DWM to composite a frame with the updated affinity so the
        // subsequent BitBlt does not include this window.
        let _ = DwmFlush();

        let desktop_dc = GetDC(None);
        if desktop_dc.is_invalid() {
            state.capture_ok = false;
            let _ = SetWindowDisplayAffinity(hwnd, WINDOW_DISPLAY_AFFINITY(0)); // WDA_NONE
            let _ = InvalidateRect(hwnd, None, false);
            return false;
        }

        // Get window rect (physical pixels on DPI-aware window)
        let mut win_rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut win_rect);

        let ok = BitBlt(
            state.memory_dc,
            0,
            0,
            state.width,
            state.height,
            desktop_dc,
            win_rect.left,
            win_rect.top,
            SRCCOPY,
        );

        ReleaseDC(None, desktop_dc);

        // Restore normal affinity so Teams can capture the window content
        let _ = SetWindowDisplayAffinity(hwnd, WINDOW_DISPLAY_AFFINITY(0)); // WDA_NONE

        state.capture_ok = ok.is_ok();

        let _ = InvalidateRect(hwnd, None, false);

        state.capture_ok
    }
}

/// Recreates the bitmap to match new window dimensions.
/// Width/height are logical pixels; converted to physical using dpi.
pub fn resize(state: &mut CaptureState, width: i32, height: i32, dpi: u32) {
    let phys_w = geometry::logical_to_physical(width, dpi);
    let phys_h = geometry::logical_to_physical(height, dpi);

    if phys_w <= 0 || phys_h <= 0 {
        return;
    }

    unsafe {
        let screen_dc = GetDC(None);
        let new_bitmap = CreateCompatibleBitmap(screen_dc, phys_w, phys_h);
        ReleaseDC(None, screen_dc);

        // Select new bitmap into memory DC (deselects old one)
        SelectObject(state.memory_dc, new_bitmap);

        // Delete old bitmap
        let _ = DeleteObject(state.bitmap);

        state.bitmap = new_bitmap;
        state.width = phys_w;
        state.height = phys_h;
    }
}

/// Cleans up capture resources.
pub fn cleanup(state: &mut CaptureState, hwnd: HWND) {
    unsafe {
        if state.timer_id != 0 {
            let _ = KillTimer(hwnd, state.timer_id);
            state.timer_id = 0;
        }
        if !state.memory_dc.is_invalid() {
            let _ = DeleteDC(state.memory_dc);
        }
        if !state.bitmap.is_invalid() {
            let _ = DeleteObject(state.bitmap);
        }
    }
}
