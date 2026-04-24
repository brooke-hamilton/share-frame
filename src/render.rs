use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

use crate::capture::CaptureState;
use crate::geometry;

/// Paints captured content and border onto the window. Called during WM_PAINT.
pub fn paint(hwnd: HWND, state: &CaptureState) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);

        let mut client_rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut client_rect);
        let cw = client_rect.right - client_rect.left;
        let ch = client_rect.bottom - client_rect.top;

        if state.capture_ok {
            // If physical capture size differs from logical client size, use StretchBlt
            if state.width != cw || state.height != ch {
                SetStretchBltMode(hdc, HALFTONE);
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
            // Error fallback: dark red background RGB(139, 0, 0) as COLORREF = 0x0000008B
            let error_brush = CreateSolidBrush(COLORREF(139));
            FillRect(hdc, &client_rect, error_brush);
            let _ = DeleteObject(error_brush);
        }

        // Draw 3px border
        let border_brush = CreateSolidBrush(COLORREF(geometry::BORDER_COLOR));
        let border_pen = CreatePen(PS_SOLID, geometry::BORDER_WIDTH, COLORREF(geometry::BORDER_COLOR));
        let old_pen = SelectObject(hdc, border_pen);
        let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH));
        let _ = Rectangle(hdc, 0, 0, cw, ch);
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        let _ = DeleteObject(border_pen);

        // Draw 6px corner grip squares
        let grip = geometry::GRIP_SIZE;
        let grip_rects = [
            // Top-left
            RECT { left: 0, top: 0, right: grip, bottom: grip },
            // Top-right
            RECT { left: cw - grip, top: 0, right: cw, bottom: grip },
            // Bottom-left
            RECT { left: 0, top: ch - grip, right: grip, bottom: ch },
            // Bottom-right
            RECT { left: cw - grip, top: ch - grip, right: cw, bottom: ch },
        ];

        for r in &grip_rects {
            FillRect(hdc, r, border_brush);
        }

        let _ = DeleteObject(border_brush);

        let _ = EndPaint(hwnd, &ps);
    }
}
