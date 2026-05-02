use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const TIMER_ID: usize = 1;
const FRAME_INTERVAL_MS: u32 = 33;

pub struct CaptureState {
    pub timer_id: usize,
    pub memory_dc: HDC,
    pub bitmap: HBITMAP,
    /// The 1x1 default bitmap that was selected into memory_dc when it was
    /// created. We keep it so we can re-select it before deleting `memory_dc`.
    default_bitmap: HGDIOBJ,
    pub width: i32,
    pub height: i32,
    pub capture_ok: bool,
    /// Set to false after the first SetWindowDisplayAffinity call fails. When
    /// false we skip the affinity dance entirely (it has no effect anyway) and
    /// accept that the window may include itself in the capture on platforms
    /// that do not support WDA_EXCLUDEFROMCAPTURE (pre Win10 2004).
    pub affinity_supported: bool,
    /// When true, timer-driven captures are skipped (e.g. during interactive
    /// resize/move). The painter will continue to display the last captured
    /// frame, stretched to fit the current client size.
    pub paused: bool,
}

/// Initializes capture state: creates memory DC, compatible bitmap, starts timer.
/// `width`/`height` are physical pixels (the window is per-monitor DPI aware
/// so its client / window rects are already in physical coordinates).
pub fn init(hwnd: HWND, width: i32, height: i32) -> CaptureState {
    let phys_w = width.max(1);
    let phys_h = height.max(1);

    unsafe {
        let screen_dc = GetDC(None);
        let memory_dc = CreateCompatibleDC(screen_dc);
        let bitmap = CreateCompatibleBitmap(screen_dc, phys_w, phys_h);
        let default_bitmap = SelectObject(memory_dc, bitmap);
        ReleaseDC(None, screen_dc);

        let timer_id = SetTimer(hwnd, TIMER_ID, FRAME_INTERVAL_MS, None);

        CaptureState {
            timer_id,
            memory_dc,
            bitmap,
            default_bitmap,
            width: phys_w,
            height: phys_h,
            capture_ok: true,
            affinity_supported: true,
            paused: false,
        }
    }
}

/// Captures the desktop region behind the window.
/// Temporarily excludes the window from screen capture via display affinity
/// so BitBlt does not pick up the window itself (avoids recursive self-capture).
/// The window stays fully visible on the physical monitor — no flicker.
pub fn capture_frame(hwnd: HWND, state: &mut CaptureState) -> bool {
    if state.paused {
        return state.capture_ok;
    }

    unsafe {
        // Hide from screen-capture APIs (Win10 2004+). The window stays visible
        // on the physical display, so there is no user-visible flicker.
        if state.affinity_supported {
            if SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE).is_err() {
                // The platform does not support per-window capture exclusion.
                // Stop trying to toggle it; subsequent captures will simply
                // include this window in the BitBlt source. Better than
                // failing every frame.
                state.affinity_supported = false;
            } else {
                // Wait for DWM to composite a frame with the updated affinity
                // so the subsequent BitBlt does not include this window.
                let _ = DwmFlush();
            }
        }

        let desktop_dc = GetDC(None);
        if desktop_dc.is_invalid() {
            state.capture_ok = false;
            if state.affinity_supported {
                let _ = SetWindowDisplayAffinity(hwnd, WDA_NONE);
            }
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

        // Restore normal affinity so downstream capture (e.g. Teams) sees this
        // window's content.
        if state.affinity_supported {
            let _ = SetWindowDisplayAffinity(hwnd, WDA_NONE);
        }

        state.capture_ok = ok.is_ok();

        let _ = InvalidateRect(hwnd, None, false);

        state.capture_ok
    }
}

/// Recreates the bitmap to match new window dimensions.
/// `width`/`height` are physical pixels.
pub fn resize(state: &mut CaptureState, width: i32, height: i32) {
    let phys_w = width.max(1);
    let phys_h = height.max(1);

    if phys_w == state.width && phys_h == state.height {
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
            // Reselect the original default bitmap before deleting our own.
            if !state.default_bitmap.is_invalid() {
                SelectObject(state.memory_dc, state.default_bitmap);
            }
            let _ = DeleteDC(state.memory_dc);
        }
        if !state.bitmap.is_invalid() {
            let _ = DeleteObject(state.bitmap);
        }
    }
}
