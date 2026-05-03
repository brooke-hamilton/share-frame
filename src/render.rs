use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

use crate::capture::CaptureState;

/// Paints captured content and grid overlay onto the client area. Called during WM_PAINT.
/// Uses double-buffering to eliminate flicker.
pub fn paint(hwnd: HWND, state: &CaptureState) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let screen_hdc = BeginPaint(hwnd, &mut ps);

        let mut client_rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut client_rect);
        let cw = client_rect.right - client_rect.left;
        let ch = client_rect.bottom - client_rect.top;

        // Create offscreen buffer for flicker-free compositing
        let buf_dc = CreateCompatibleDC(screen_hdc);
        let buf_bmp = CreateCompatibleBitmap(screen_hdc, cw, ch);
        let old_bmp = SelectObject(buf_dc, buf_bmp);
        let hdc = buf_dc;

        if state.capture_ok {
            // Paint captured content filling the entire client area
            if state.width != cw || state.height != ch {
                SetStretchBltMode(hdc, HALFTONE);
                let _ = SetBrushOrgEx(hdc, 0, 0, None);
                let _ = StretchBlt(
                    hdc,
                    0,
                    0,
                    cw,
                    ch,
                    state.memory_dc,
                    0,
                    0,
                    state.width,
                    state.height,
                    SRCCOPY,
                );
            } else {
                let _ = BitBlt(hdc, 0, 0, cw, ch, state.memory_dc, 0, 0, SRCCOPY);
            }
        } else {
            // Error fallback: dark red background
            let error_brush = CreateSolidBrush(COLORREF(139));
            let content_rect = RECT { left: 0, top: 0, right: cw, bottom: ch };
            FillRect(hdc, &content_rect, error_brush);
            let _ = DeleteObject(error_brush);
        }

        // --- Grid Overlay (subtle visual cue that overlay is present) ---
        if ch > 0 && cw > 0 {
            let grid_dc = CreateCompatibleDC(hdc);
            let grid_bmp = CreateCompatibleBitmap(hdc, cw, ch);
            let old_grid_bmp = SelectObject(grid_dc, grid_bmp);

            // Fill with black background
            let black_brush = CreateSolidBrush(COLORREF(0x00000000));
            let grid_rect = RECT { left: 0, top: 0, right: cw, bottom: ch };
            FillRect(grid_dc, &grid_rect, black_brush);
            let _ = DeleteObject(black_brush);

            // Draw white grid lines every 48px
            let white_pen = CreatePen(PS_SOLID, 1, COLORREF(0x00FFFFFF));
            let old_pen = SelectObject(grid_dc, white_pen);

            let grid_spacing = 48;

            // Vertical lines
            let mut x = grid_spacing;
            while x < cw {
                let _ = MoveToEx(grid_dc, x, 0, None);
                let _ = LineTo(grid_dc, x, ch);
                x += grid_spacing;
            }

            // Horizontal lines
            let mut y = grid_spacing;
            while y < ch {
                let _ = MoveToEx(grid_dc, 0, y, None);
                let _ = LineTo(grid_dc, cw, y);
                y += grid_spacing;
            }

            SelectObject(grid_dc, old_pen);
            let _ = DeleteObject(white_pen);

            // AlphaBlend the grid over content area at ~10% opacity
            let blend_fn = BLENDFUNCTION {
                BlendOp: 0,              // AC_SRC_OVER
                BlendFlags: 0,
                SourceConstantAlpha: 25, // ~10% visibility
                AlphaFormat: 0,
            };
            let _ = AlphaBlend(
                hdc, 0, 0, cw, ch,
                grid_dc, 0, 0, cw, ch,
                blend_fn,
            );

            // Clean up grid resources
            SelectObject(grid_dc, old_grid_bmp);
            let _ = DeleteObject(grid_bmp);
            let _ = DeleteDC(grid_dc);
        }

        // Blit the composed frame to the screen in one operation
        let _ = BitBlt(screen_hdc, 0, 0, cw, ch, buf_dc, 0, 0, SRCCOPY);

        // Clean up offscreen buffer
        SelectObject(buf_dc, old_bmp);
        let _ = DeleteObject(buf_bmp);
        let _ = DeleteDC(buf_dc);

        let _ = EndPaint(hwnd, &ps);
    }
}
