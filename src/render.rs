use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

use crate::capture::CaptureState;
use crate::geometry;

/// Paints captured content, title bar, and border onto the window. Called during WM_PAINT.
pub fn paint(hwnd: HWND, state: &CaptureState, close_button_hovered: bool) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);

        let mut client_rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut client_rect);
        let cw = client_rect.right - client_rect.left;
        let ch = client_rect.bottom - client_rect.top;

        let tb_height = geometry::TITLE_BAR_HEIGHT;

        if state.capture_ok {
            // Paint captured content below the title bar
            if state.width != cw || state.height != (ch - tb_height) {
                SetStretchBltMode(hdc, HALFTONE);
                let _ = SetBrushOrgEx(hdc, 0, 0, None);
                let _ = StretchBlt(
                    hdc,
                    0,
                    tb_height,
                    cw,
                    ch - tb_height,
                    state.memory_dc,
                    0,
                    0,
                    state.width,
                    state.height,
                    SRCCOPY,
                );
            } else {
                let _ = BitBlt(hdc, 0, tb_height, cw, ch - tb_height, state.memory_dc, 0, 0, SRCCOPY);
            }
        } else {
            // Error fallback: dark red background below title bar
            let error_brush = CreateSolidBrush(COLORREF(139));
            let content_rect = RECT { left: 0, top: tb_height, right: cw, bottom: ch };
            FillRect(hdc, &content_rect, error_brush);
            let _ = DeleteObject(error_brush);
        }

        // --- Title Bar ---
        let title_bar_brush = CreateSolidBrush(COLORREF(geometry::TITLE_BAR_COLOR));
        let title_bar_rect = RECT { left: 0, top: 0, right: cw, bottom: tb_height };
        FillRect(hdc, &title_bar_rect, title_bar_brush);
        let _ = DeleteObject(title_bar_brush);

        // Draw title text
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, COLORREF(geometry::TITLE_BAR_TEXT_COLOR));
        let mut text_rect = RECT { left: 0, top: 0, right: cw, bottom: tb_height };
        let title: Vec<u16> = "Share Frame".encode_utf16().collect();
        DrawTextW(hdc, &mut title.clone(), &mut text_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);

        // --- Close Button ---
        let close_left = cw - geometry::CLOSE_BUTTON_WIDTH;
        let close_rect = RECT { left: close_left, top: 0, right: cw, bottom: tb_height };

        if close_button_hovered {
            let hover_brush = CreateSolidBrush(COLORREF(geometry::CLOSE_BUTTON_HOVER_COLOR));
            FillRect(hdc, &close_rect, hover_brush);
            let _ = DeleteObject(hover_brush);
        }

        // Draw X glyph using Segoe Fluent Icons (U+E8BB = ChromeClose)
        let font_name: Vec<u16> = "Segoe Fluent Icons\0".encode_utf16().collect();
        let icon_font = CreateFontW(
            14, 0, 0, 0,
            FW_NORMAL.0 as i32,
            0, 0, 0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            DEFAULT_PITCH.0 as u32 | (FF_DONTCARE.0 as u32),
            PCWSTR(font_name.as_ptr()),
        );
        let old_font = SelectObject(hdc, icon_font);
        let text_color = if close_button_hovered {
            COLORREF(0x00FFFFFF) // white on red
        } else {
            COLORREF(geometry::TITLE_BAR_TEXT_COLOR)
        };
        SetTextColor(hdc, text_color);
        SetBkMode(hdc, TRANSPARENT);
        // U+E8BB = ChromeClose glyph
        let glyph: Vec<u16> = vec![0xE8BB];
        let mut glyph_rect = RECT { left: close_left, top: 0, right: cw, bottom: tb_height };
        DrawTextW(hdc, &mut glyph.clone(), &mut glyph_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        SelectObject(hdc, old_font);
        let _ = DeleteObject(icon_font);

        // --- Border ---
        let border_brush = CreateSolidBrush(COLORREF(geometry::BORDER_COLOR));
        let bw = geometry::BORDER_WIDTH;
        let edges = [
            // Top (below title bar not needed — title bar covers it)
            // Bottom
            RECT { left: 0, top: ch - bw, right: cw, bottom: ch },
            // Left
            RECT { left: 0, top: tb_height, right: bw, bottom: ch },
            // Right
            RECT { left: cw - bw, top: tb_height, right: cw, bottom: ch },
        ];
        for r in &edges {
            FillRect(hdc, r, border_brush);
        }

        // Draw 6px corner grip squares at the bottom corners only
        let grip = geometry::GRIP_SIZE;
        let grip_rects = [
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
