use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::WindowsAndMessaging::*;

const TIMER_ID: usize = 1;
const FRAME_INTERVAL_MS: u32 = 33;

/// Owns the off-screen GDI buffer used to capture the desktop region behind
/// the window, plus the per-frame timer driving recapture.
///
/// All resources are released by the [`Drop`] impl, so the only requirement
/// on the caller is to drop (or replace) the value before the owning `HWND`
/// is destroyed.
pub struct CaptureState {
    hwnd: HWND,
    timer_id: usize,
    memory_dc: HDC,
    bitmap: HBITMAP,
    /// The 1×1 default bitmap selected into `memory_dc` at creation time.
    /// Re-selected before `memory_dc` is deleted.
    default_bitmap: HGDIOBJ,
    width: i32,
    height: i32,
    capture_ok: bool,
    /// Set to `false` once `SetWindowDisplayAffinity` has failed once. When
    /// `false` we skip the affinity dance entirely (it has no effect anyway)
    /// and accept that the window may include itself in the capture on
    /// platforms that do not support `WDA_EXCLUDEFROMCAPTURE` (pre Win10 2004).
    affinity_supported: bool,
    /// When `true`, timer-driven captures are skipped (e.g. during
    /// interactive resize/move). The painter continues to display the last
    /// captured frame, stretched to fit the current client size.
    paused: bool,
}

impl CaptureState {
    /// Creates the capture buffer (memory DC + compatible bitmap) and starts
    /// the per-frame timer. `width`/`height` are physical pixels for the
    /// content region (not the full window).
    pub fn new(hwnd: HWND, width: i32, height: i32) -> Self {
        let phys_w = width.max(1);
        let phys_h = height.max(1);

        // SAFETY: All handles are stored on `Self` and released by `Drop`.
        // `hwnd` is owned by the caller and must outlive this value.
        unsafe {
            let screen_dc = GetDC(None);
            let memory_dc = CreateCompatibleDC(screen_dc);
            let bitmap = CreateCompatibleBitmap(screen_dc, phys_w, phys_h);
            let default_bitmap = SelectObject(memory_dc, bitmap);
            let _ = ReleaseDC(None, screen_dc);

            let timer_id = SetTimer(hwnd, TIMER_ID, FRAME_INTERVAL_MS, None);

            Self {
                hwnd,
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

    /// Width of the off-screen capture bitmap, in physical pixels.
    pub fn width(&self) -> i32 {
        self.width
    }

    /// Height of the off-screen capture bitmap, in physical pixels.
    pub fn height(&self) -> i32 {
        self.height
    }

    /// `true` if the most recent capture call succeeded.
    pub fn capture_ok(&self) -> bool {
        self.capture_ok
    }

    /// Memory DC backing the captured frame; the renderer blits from this
    /// into its own back-buffer.
    pub fn memory_dc(&self) -> HDC {
        self.memory_dc
    }

    /// `true` when timer-driven capture is currently suspended.
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Suspends timer-driven capture (call on `WM_ENTERSIZEMOVE`).
    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Resumes timer-driven capture (call on `WM_EXITSIZEMOVE`).
    pub fn resume(&mut self) {
        self.paused = false;
    }

    /// Captures the desktop region behind the window into the off-screen
    /// bitmap. Temporarily excludes the window from screen capture via
    /// `WDA_EXCLUDEFROMCAPTURE` so `BitBlt` does not pick up the window
    /// itself. The window stays fully visible on the physical monitor —
    /// no flicker.
    ///
    /// Returns the value of [`Self::capture_ok`] after the attempt.
    pub fn capture_frame(&mut self) -> bool {
        if self.paused {
            return self.capture_ok;
        }

        let hwnd = self.hwnd;

        // SAFETY: Win32 handles used here are owned by `self` or fetched
        // and released within this scope.
        unsafe {
            // Skip the entire capture pipeline when the window is not
            // observable — minimized or hidden. The affinity toggle,
            // `DwmFlush` (which blocks waiting for the next compose), the
            // fullscreen `BitBlt`, and the forced repaint together account
            // for the bulk of this app's idle CPU usage; doing none of it
            // when nothing is on screen is functionally equivalent.
            if IsIconic(hwnd).as_bool() || !IsWindowVisible(hwnd).as_bool() {
                return self.capture_ok;
            }

            if self.affinity_supported {
                if SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE).is_err() {
                    self.affinity_supported = false;
                } else {
                    // Wait for DWM to composite a frame with the updated
                    // affinity so the subsequent BitBlt does not include
                    // this window.
                    let _ = DwmFlush();
                }
            }

            let desktop_dc = GetDC(None);
            if desktop_dc.is_invalid() {
                self.capture_ok = false;
                if self.affinity_supported {
                    let _ = SetWindowDisplayAffinity(hwnd, WDA_NONE);
                }
                return false;
            }

            // Get client area origin in screen coordinates, offset by the
            // synthesized title bar so the captured region matches what the
            // renderer will draw beneath the title bar.
            let mut client_origin = POINT { x: 0, y: 0 };
            let _ = ClientToScreen(hwnd, &mut client_origin);
            let dpi = GetDpiForWindow(hwnd);
            let frame_y = GetSystemMetricsForDpi(SM_CYFRAME, dpi);
            let caption = GetSystemMetricsForDpi(SM_CYCAPTION, dpi);
            let padding = GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi);
            let title_bar_height = frame_y + caption + padding;
            client_origin.y += title_bar_height;

            let ok = BitBlt(
                self.memory_dc,
                0,
                0,
                self.width,
                self.height,
                desktop_dc,
                client_origin.x,
                client_origin.y,
                SRCCOPY,
            );

            let _ = ReleaseDC(None, desktop_dc);

            if self.affinity_supported {
                let _ = SetWindowDisplayAffinity(hwnd, WDA_NONE);
            }

            let was_ok = self.capture_ok;
            self.capture_ok = ok.is_ok();
            // Only request a repaint when there's something new to draw:
            // a fresh frame succeeded, or the success state just flipped
            // (e.g. transitioning into the error-fill state). Skipping the
            // repaint on a steady-state failure avoids needlessly re-running
            // the renderer 30×/sec while capture remains broken.
            if self.capture_ok || was_ok != self.capture_ok {
                let _ = InvalidateRect(hwnd, None, false);
            }
            self.capture_ok
        }
    }

    /// Recreates the off-screen bitmap to match new content dimensions.
    /// `width`/`height` are physical pixels.
    pub fn resize(&mut self, width: i32, height: i32) {
        let phys_w = width.max(1);
        let phys_h = height.max(1);

        if phys_w == self.width && phys_h == self.height {
            return;
        }

        // SAFETY: `memory_dc` and `bitmap` are owned by `self`.
        unsafe {
            let screen_dc = GetDC(None);
            let new_bitmap = CreateCompatibleBitmap(screen_dc, phys_w, phys_h);
            let _ = ReleaseDC(None, screen_dc);

            SelectObject(self.memory_dc, new_bitmap);
            let _ = DeleteObject(self.bitmap);

            self.bitmap = new_bitmap;
            self.width = phys_w;
            self.height = phys_h;
        }
    }
}

impl Drop for CaptureState {
    fn drop(&mut self) {
        // SAFETY: Releases handles created in `new` / `resize`. The owning
        // `HWND` must still be valid; the window proc drops the capture
        // state during `WM_DESTROY`, before the HWND is invalidated.
        unsafe {
            if self.timer_id != 0 {
                let _ = KillTimer(self.hwnd, self.timer_id);
            }
            if !self.memory_dc.is_invalid() {
                if !self.default_bitmap.is_invalid() {
                    SelectObject(self.memory_dc, self.default_bitmap);
                }
                let _ = DeleteDC(self.memory_dc);
            }
            if !self.bitmap.is_invalid() {
                let _ = DeleteObject(self.bitmap);
            }
        }
    }
}
