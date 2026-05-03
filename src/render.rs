use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::capture::CaptureState;
use crate::geometry;

/// Paints the title bar (with custom "Send to Back" button and title text),
/// captured content, and grid overlay. Uses a 32-bit DIB section buffer so
/// we can set the alpha channel for DWM glass compositing.
pub fn paint(hwnd: HWND, state: &CaptureState, send_back_hovered: bool, title_bar_height: i32) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let screen_hdc = BeginPaint(hwnd, &mut ps);

        let mut client_rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut client_rect);
        let cw = client_rect.right - client_rect.left;
        let ch = client_rect.bottom - client_rect.top;

        // Create a 32-bit DIB section so we can manipulate the alpha channel.
        // DWM interprets alpha=0 as transparent in the extended frame region,
        // so standard GDI drawing (which leaves alpha at 0) would be invisible.
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: cw,
                biHeight: -ch, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let buf_dc = CreateCompatibleDC(screen_hdc);
        let buf_bmp = CreateDIBSection(buf_dc, &bmi, DIB_RGB_COLORS, &mut bits_ptr, None, 0)
            .unwrap_or_default();
        let old_bmp = SelectObject(buf_dc, buf_bmp);
        let hdc = buf_dc;

        let tb_height = title_bar_height;
        let content_height = ch - tb_height;

        // --- Title Bar ---
        // Fill with black (will become DWM glass after alpha fixup)
        let black_brush = CreateSolidBrush(COLORREF(0x00000000));
        let title_bar_rect = RECT { left: 0, top: 0, right: cw, bottom: tb_height };
        FillRect(hdc, &title_bar_rect, black_brush);
        let _ = DeleteObject(black_brush);

        // Determine "Send to Back" button position (left of DWM caption buttons)
        let caption_buttons_width = get_caption_buttons_width(hwnd);
        let button_left = cw - caption_buttons_width - geometry::SEND_BACK_BUTTON_WIDTH;
        let button_right = button_left + geometry::SEND_BACK_BUTTON_WIDTH;

        // Draw button hover background
        if send_back_hovered {
            let hover_brush = CreateSolidBrush(COLORREF(0x00333333));
            let btn_rect = RECT {
                left: button_left,
                top: 0,
                right: button_right,
                bottom: tb_height,
            };
            FillRect(hdc, &btn_rect, hover_brush);
            let _ = DeleteObject(hover_brush);
        }

        // Draw window icon (small icon from the window class)
        let icon_handle = GetClassLongPtrW(hwnd, GCLP_HICONSM) as isize;
        let icon_size = 16;
        let icon_x = 8;
        let icon_y = (tb_height - icon_size) / 2;
        if icon_handle != 0 {
            let _ = DrawIconEx(
                hdc,
                icon_x,
                icon_y,
                HICON(icon_handle as *mut _),
                icon_size,
                icon_size,
                0,
                None,
                DI_NORMAL,
            );
        }

        // Draw window title text (to the right of the icon)
        let text_left = icon_x + icon_size + 8;
        let title_font_name: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
        let title_font = CreateFontW(
            -12, 0, 0, 0,
            FW_NORMAL.0 as i32,
            0, 0, 0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            DEFAULT_PITCH.0 as u32 | (FF_DONTCARE.0 as u32),
            PCWSTR(title_font_name.as_ptr()),
        );
        let old_font = SelectObject(hdc, title_font);
        SetTextColor(hdc, COLORREF(0x00FFFFFF));
        SetBkMode(hdc, TRANSPARENT);
        let mut title_text: Vec<u16> = "Share Frame".encode_utf16().collect();
        let mut title_rect = RECT {
            left: text_left,
            top: 0,
            right: button_left - 8,
            bottom: tb_height,
        };
        DrawTextW(hdc, &mut title_text, &mut title_rect, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        SelectObject(hdc, old_font);
        let _ = DeleteObject(title_font);

        // Draw "Send to Back" glyph (U+E72D from Segoe MDL2 Assets)
        let icon_font_name: Vec<u16> = "Segoe MDL2 Assets\0".encode_utf16().collect();
        let icon_font = CreateFontW(
            -12, 0, 0, 0,
            FW_NORMAL.0 as i32,
            0, 0, 0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            DEFAULT_PITCH.0 as u32 | (FF_DONTCARE.0 as u32),
            PCWSTR(icon_font_name.as_ptr()),
        );
        let old_font2 = SelectObject(hdc, icon_font);
        SetTextColor(hdc, COLORREF(0x00FFFFFF));
        let glyph: Vec<u16> = vec![0xE72D];
        let mut glyph_rect = RECT {
            left: button_left,
            top: 0,
            right: button_right,
            bottom: tb_height,
        };
        DrawTextW(hdc, &mut glyph.clone(), &mut glyph_rect, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        SelectObject(hdc, old_font2);
        let _ = DeleteObject(icon_font);

        // Fix alpha channel in title bar region: set to 255 (opaque) so DWM
        // doesn't treat our drawn pixels as transparent glass.
        // EXCEPT: leave the caption button area (right side) at alpha=0 so
        // DWM can render its native close/maximize buttons there.
        if !bits_ptr.is_null() {
            let stride = cw as usize * 4; // 32bpp = 4 bytes per pixel
            let pixels = bits_ptr as *mut u8;
            // Caption buttons region starts at button_right (left edge of DWM buttons)
            // which equals cw - caption_buttons_width. Leave that area transparent.
            let opaque_right = (cw - caption_buttons_width).max(0) as usize;
            for row in 0..tb_height as usize {
                for col in 0..opaque_right {
                    let offset = row * stride + col * 4 + 3; // +3 = alpha byte
                    *pixels.add(offset) = 255;
                }
            }
        }

        // --- Content Area ---
        if content_height > 0 {
            if state.capture_ok {
                if state.width != cw || state.height != content_height {
                    SetStretchBltMode(hdc, HALFTONE);
                    let _ = SetBrushOrgEx(hdc, 0, 0, None);
                    let _ = StretchBlt(
                        hdc,
                        0,
                        tb_height,
                        cw,
                        content_height,
                        state.memory_dc,
                        0,
                        0,
                        state.width,
                        state.height,
                        SRCCOPY,
                    );
                } else {
                    let _ = BitBlt(hdc, 0, tb_height, cw, content_height, state.memory_dc, 0, 0, SRCCOPY);
                }
            } else {
                // Error fallback: dark red background
                let error_brush = CreateSolidBrush(COLORREF(139));
                let content_rect = RECT { left: 0, top: tb_height, right: cw, bottom: ch };
                FillRect(hdc, &content_rect, error_brush);
                let _ = DeleteObject(error_brush);
            }

            // --- Grid Overlay ---
            if cw > 0 && content_height > 0 {
                let grid_dc = CreateCompatibleDC(hdc);
                let grid_bmp = CreateCompatibleBitmap(hdc, cw, content_height);
                let old_grid_bmp = SelectObject(grid_dc, grid_bmp);

                let grid_black_brush = CreateSolidBrush(COLORREF(0x00000000));
                let grid_rect = RECT { left: 0, top: 0, right: cw, bottom: content_height };
                FillRect(grid_dc, &grid_rect, grid_black_brush);
                let _ = DeleteObject(grid_black_brush);

                let white_pen = CreatePen(PS_SOLID, 1, COLORREF(0x00FFFFFF));
                let old_pen = SelectObject(grid_dc, white_pen);

                let grid_spacing = 48;

                let mut x = grid_spacing;
                while x < cw {
                    let _ = MoveToEx(grid_dc, x, 0, None);
                    let _ = LineTo(grid_dc, x, content_height);
                    x += grid_spacing;
                }

                let mut y = grid_spacing;
                while y < content_height {
                    let _ = MoveToEx(grid_dc, 0, y, None);
                    let _ = LineTo(grid_dc, cw, y);
                    y += grid_spacing;
                }

                SelectObject(grid_dc, old_pen);
                let _ = DeleteObject(white_pen);

                let blend_fn = BLENDFUNCTION {
                    BlendOp: 0,
                    BlendFlags: 0,
                    SourceConstantAlpha: 25,
                    AlphaFormat: 0,
                };
                let _ = AlphaBlend(
                    hdc, 0, tb_height, cw, content_height,
                    grid_dc, 0, 0, cw, content_height,
                    blend_fn,
                );

                SelectObject(grid_dc, old_grid_bmp);
                let _ = DeleteObject(grid_bmp);
                let _ = DeleteDC(grid_dc);
            }
        }

        // Blit composed frame to screen
        let _ = BitBlt(screen_hdc, 0, 0, cw, ch, buf_dc, 0, 0, SRCCOPY);

        // Clean up
        SelectObject(buf_dc, old_bmp);
        let _ = DeleteObject(buf_bmp);
        let _ = DeleteDC(buf_dc);

        let _ = EndPaint(hwnd, &ps);
    }
}

/// Returns the width of DWM-drawn caption buttons.
unsafe fn get_caption_buttons_width(hwnd: HWND) -> i32 {
    let mut buttons_rect = RECT::default();
    let result = DwmGetWindowAttribute(
        hwnd,
        DWMWA_CAPTION_BUTTON_BOUNDS,
        &mut buttons_rect as *mut _ as *mut _,
        std::mem::size_of::<RECT>() as u32,
    );
    if result.is_ok() {
        buttons_rect.right - buttons_rect.left
    } else {
        138
    }
}
