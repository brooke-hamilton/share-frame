use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::capture::CaptureState;
use crate::geometry;

// --- Render constants ---

/// Background fill for the captured-content region (under the grid overlay).
/// DWM glass is unaffected because the title bar uses [`TitleBarTheme`]
/// colors instead.
const CONTENT_BG_COLOR: COLORREF = COLORREF(0x00000000);
/// Grid-line color drawn over the captured frame.
const GRID_LINE_COLOR: COLORREF = COLORREF(0x00FFFFFF);
/// Fallback fill when capture fails (dark red).
const ERROR_FILL_COLOR: COLORREF = COLORREF(0x0000008B);
/// Glyph code point for the "Send to Back" button (Segoe MDL2 Assets).
const SEND_BACK_GLYPH: u16 = 0xE72D;
/// Window title rendered in the custom title bar. Must be ASCII so the
/// byte length equals the UTF-16 code-unit length used by the on-stack
/// buffer in [`title_text_buffer`].
const WINDOW_TITLE: &str = "Share Frame";
const WINDOW_TITLE_LEN: usize = WINDOW_TITLE.len();
const _: () = assert!(WINDOW_TITLE.is_ascii(), "WINDOW_TITLE must be ASCII");
/// Source-over blend op for `BLENDFUNCTION::BlendOp`.
const AC_SRC_OVER: u8 = 0x00;

/// Colors used to paint the custom title bar. Computed from the active
/// Windows theme and DWM accent settings each paint so it matches whatever
/// DWM draws in the caption-buttons strip on the right.
#[derive(Copy, Clone)]
pub struct TitleBarTheme {
    pub background: COLORREF,
    pub text: COLORREF,
    pub hover: COLORREF,
}

/// Cached per-DPI fonts used to draw the custom title bar.
struct CachedFonts {
    dpi: u32,
    title_font: HFONT,
    icon_font: HFONT,
}

impl CachedFonts {
    fn create(dpi: u32) -> Self {
        // SAFETY: Plain GDI font creation; handles are owned by `Self`.
        unsafe {
            let height = -scale_font_for_dpi(dpi);
            Self {
                dpi,
                title_font: create_font(height, w!("Segoe UI")),
                icon_font: create_font(height, w!("Segoe MDL2 Assets")),
            }
        }
    }
}

impl Drop for CachedFonts {
    fn drop(&mut self) {
        // SAFETY: Both handles were created in `create` and are not
        // currently selected into any DC at drop time.
        unsafe {
            let _ = DeleteObject(self.title_font);
            let _ = DeleteObject(self.icon_font);
        }
    }
}

/// Cached 32-bit DIB-section back-buffer reused across paints. Recreated
/// only when the client size changes — avoids per-frame `CreateDIBSection`
/// (which allocates `width * height * 4` bytes from the GDI heap) plus the
/// matching DC create/select/delete dance.
struct BackBuffer {
    dc: HDC,
    bitmap: HBITMAP,
    /// Bitmap originally selected into `dc`; re-selected before deletion.
    default_bitmap: HGDIOBJ,
    /// Pointer to the DIB pixel bits (used by `fix_title_bar_alpha`).
    bits: *mut std::ffi::c_void,
    width: i32,
    height: i32,
}

impl BackBuffer {
    /// Creates a top-down 32-bit BGRA DIB section sized `width x height`.
    /// `screen_hdc` is used as the compatibility source and is not retained.
    unsafe fn new(screen_hdc: HDC, width: i32, height: i32) -> Option<Self> {
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let dc = CreateCompatibleDC(screen_hdc);
        let bitmap =
            CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        let default_bitmap = SelectObject(dc, bitmap);
        Some(Self { dc, bitmap, default_bitmap, bits, width, height })
    }
}

impl Drop for BackBuffer {
    fn drop(&mut self) {
        // SAFETY: Re-select the original bitmap before deleting our DIB so
        // GDI doesn't leak the bitmap, then release the DC.
        unsafe {
            SelectObject(self.dc, self.default_bitmap);
            let _ = DeleteObject(self.bitmap);
            let _ = DeleteDC(self.dc);
        }
    }
}

/// Per-window cache of resources reused across paints. Owned by the window
/// state; invalidated when DPI changes.
#[derive(Default)]
pub struct RenderCache {
    fonts: Option<CachedFonts>,
    back_buffer: Option<BackBuffer>,
}

impl RenderCache {
    /// Drops cached resources that depend on DPI; called from `WM_DPICHANGED`.
    pub fn invalidate_dpi_dependent(&mut self) {
        self.fonts = None;
    }

    fn fonts_for(&mut self, dpi: u32) -> &CachedFonts {
        if self.fonts.as_ref().map(|f| f.dpi) != Some(dpi) {
            self.fonts = Some(CachedFonts::create(dpi));
        }
        self.fonts.as_ref().expect("fonts populated above")
    }

    /// Returns a back-buffer matching `width x height`, recreating it only
    /// when the dimensions change. Returns `None` if DIB allocation fails.
    unsafe fn back_buffer_for(
        &mut self,
        screen_hdc: HDC,
        width: i32,
        height: i32,
    ) -> Option<&BackBuffer> {
        let needs_new = match &self.back_buffer {
            Some(b) => b.width != width || b.height != height,
            None => true,
        };
        if needs_new {
            // Drop the old buffer before allocating the new one so we don't
            // hold two full-window DIBs in memory simultaneously.
            self.back_buffer = None;
            self.back_buffer = BackBuffer::new(screen_hdc, width, height);
        }
        self.back_buffer.as_ref()
    }
}

/// Paints the title bar (with custom "Send to Back" button and title text),
/// captured content, and grid overlay. Uses a 32-bit DIB section buffer so
/// the alpha channel can be set for DWM glass compositing.
pub fn paint(
    hwnd: HWND,
    state: &CaptureState,
    cache: &mut RenderCache,
    send_back_hovered: bool,
    title_bar_height: i32,
    dpi: u32,
    theme: TitleBarTheme,
) {
    // SAFETY: Standard GDI paint sequence; all created handles are released
    // before `EndPaint` returns.
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let screen_hdc = BeginPaint(hwnd, &mut ps);

        let mut client_rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut client_rect);
        let cw = client_rect.right - client_rect.left;
        let ch = client_rect.bottom - client_rect.top;

        if cw <= 0 || ch <= 0 {
            let _ = EndPaint(hwnd, &ps);
            return;
        }

        // 32-bit DIB section so we can manipulate the alpha channel; DWM
        // treats alpha=0 in the extended frame region as transparent glass.
        // Cached on `RenderCache` and reused as long as the client size is
        // unchanged, so a steady-state drag/resize doesn't churn the GDI
        // heap with a fresh `width * height * 4` allocation per frame.
        let Some(back) = cache.back_buffer_for(screen_hdc, cw, ch) else {
            let _ = EndPaint(hwnd, &ps);
            return;
        };
        let buf_dc = back.dc;
        let bits_ptr = back.bits;

        let caption_buttons_width = caption_buttons_width(hwnd);
        let content_height = ch - title_bar_height;

        // Skip per-region work when `ps.rcPaint` doesn't touch it. This
        // keeps capture-driven repaints (which invalidate only the content
        // rect) from re-running the title-bar drawing — icon, text, alpha
        // fix-up — on every frame, and conversely keeps title-bar-only
        // invalidations (e.g. button hover) from re-blitting the captured
        // content.
        let dirty = ps.rcPaint;
        let title_dirty = dirty.top < title_bar_height;
        let content_dirty = content_height > 0 && dirty.bottom > title_bar_height;

        if title_dirty {
            paint_title_bar(
                buf_dc,
                cache,
                cw,
                title_bar_height,
                caption_buttons_width,
                send_back_hovered,
                dpi,
                hwnd,
                theme,
            );

            fix_title_bar_alpha(bits_ptr, cw, title_bar_height, caption_buttons_width);
        }

        if content_dirty {
            paint_content(buf_dc, state, cw, content_height, title_bar_height);
            paint_grid_overlay(buf_dc, cw, content_height, title_bar_height);
        }

        // Blit only the dirty region. GDI would clip a full-window BitBlt
        // to `ps.rcPaint` anyway, but bounding the source rect keeps the
        // copy proportional to what actually changed.
        let dw = dirty.right - dirty.left;
        let dh = dirty.bottom - dirty.top;
        if dw > 0 && dh > 0 {
            let _ = BitBlt(
                screen_hdc,
                dirty.left,
                dirty.top,
                dw,
                dh,
                buf_dc,
                dirty.left,
                dirty.top,
                SRCCOPY,
            );
        }

        // Back-buffer DC, bitmap, and bits stay alive on `cache` for the
        // next paint; nothing to release here.
        let _ = EndPaint(hwnd, &ps);
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn paint_title_bar(
    hdc: HDC,
    cache: &mut RenderCache,
    cw: i32,
    tb_height: i32,
    caption_buttons_width: i32,
    send_back_hovered: bool,
    dpi: u32,
    hwnd: HWND,
    theme: TitleBarTheme,
) {
    fill_solid(
        hdc,
        RECT { left: 0, top: 0, right: cw, bottom: tb_height },
        theme.background,
    );

    let (button_left, button_right) =
        geometry::send_back_button_range(cw, caption_buttons_width);

    if send_back_hovered {
        fill_solid(
            hdc,
            RECT {
                left: button_left,
                top: 0,
                right: button_right,
                bottom: tb_height,
            },
            theme.hover,
        );
    }

    // Window icon (small icon from the window class).
    let icon_handle = GetClassLongPtrW(hwnd, GCLP_HICONSM) as *mut std::ffi::c_void;
    let icon_size = geometry::TITLE_ICON_SIZE;
    let icon_x = geometry::TITLE_ICON_INSET;
    let icon_y = (tb_height - icon_size) / 2;
    if !icon_handle.is_null() {
        let _ = DrawIconEx(
            hdc,
            icon_x,
            icon_y,
            HICON(icon_handle),
            icon_size,
            icon_size,
            0,
            None,
            DI_NORMAL,
        );
    }

    let fonts = cache.fonts_for(dpi);

    // Window title text.
    let text_left = icon_x + icon_size + 8;
    let old_font = SelectObject(hdc, fonts.title_font);
    SetTextColor(hdc, theme.text);
    SetBkMode(hdc, TRANSPARENT);
    let mut title_text = title_text_buffer();
    let mut title_rect = RECT {
        left: text_left,
        top: 0,
        right: button_left - 8,
        bottom: tb_height,
    };
    DrawTextW(
        hdc,
        &mut title_text,
        &mut title_rect,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    SelectObject(hdc, old_font);

    // "Send to Back" glyph.
    let old_font = SelectObject(hdc, fonts.icon_font);
    SetTextColor(hdc, theme.text);
    let mut glyph = [SEND_BACK_GLYPH];
    let mut glyph_rect = RECT {
        left: button_left,
        top: 0,
        right: button_right,
        bottom: tb_height,
    };
    DrawTextW(
        hdc,
        &mut glyph,
        &mut glyph_rect,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    SelectObject(hdc, old_font);
}

/// Sets the alpha channel to 255 across the title bar **except** the
/// rightmost strip where DWM will draw native caption buttons; that strip
/// must remain transparent (alpha=0) so DWM's glyphs show through.
unsafe fn fix_title_bar_alpha(
    bits_ptr: *mut std::ffi::c_void,
    cw: i32,
    tb_height: i32,
    caption_buttons_width: i32,
) {
    if bits_ptr.is_null() || cw <= 0 || tb_height <= 0 {
        return;
    }
    let stride = cw as usize * 4; // 32 bpp
    let pixels = bits_ptr as *mut u8;
    let opaque_right = (cw - caption_buttons_width).max(0) as usize;
    for row in 0..tb_height as usize {
        for col in 0..opaque_right {
            let offset = row * stride + col * 4 + 3; // alpha byte
            *pixels.add(offset) = 255;
        }
    }
}

unsafe fn paint_content(
    hdc: HDC,
    state: &CaptureState,
    cw: i32,
    content_height: i32,
    tb_height: i32,
) {
    if !state.capture_ok() {
        fill_solid(
            hdc,
            RECT { left: 0, top: tb_height, right: cw, bottom: tb_height + content_height },
            ERROR_FILL_COLOR,
        );
        return;
    }

    let (sw, sh) = (state.width(), state.height());
    if sw == cw && sh == content_height {
        let _ = BitBlt(hdc, 0, tb_height, cw, content_height, state.memory_dc(), 0, 0, SRCCOPY);
    } else {
        SetStretchBltMode(hdc, HALFTONE);
        let _ = SetBrushOrgEx(hdc, 0, 0, None);
        let _ = StretchBlt(
            hdc,
            0,
            tb_height,
            cw,
            content_height,
            state.memory_dc(),
            0,
            0,
            sw,
            sh,
            SRCCOPY,
        );
    }
}

unsafe fn paint_grid_overlay(hdc: HDC, cw: i32, content_height: i32, tb_height: i32) {
    if cw <= 0 || content_height <= 0 {
        return;
    }

    let grid_dc = CreateCompatibleDC(hdc);
    let grid_bmp = CreateCompatibleBitmap(hdc, cw, content_height);
    let old_grid_bmp = SelectObject(grid_dc, grid_bmp);

    fill_solid(
        grid_dc,
        RECT { left: 0, top: 0, right: cw, bottom: content_height },
        CONTENT_BG_COLOR,
    );

    let pen = CreatePen(PS_SOLID, 1, GRID_LINE_COLOR);
    let old_pen = SelectObject(grid_dc, pen);

    let mut x = geometry::GRID_SPACING;
    while x < cw {
        let _ = MoveToEx(grid_dc, x, 0, None);
        let _ = LineTo(grid_dc, x, content_height);
        x += geometry::GRID_SPACING;
    }
    let mut y = geometry::GRID_SPACING;
    while y < content_height {
        let _ = MoveToEx(grid_dc, 0, y, None);
        let _ = LineTo(grid_dc, cw, y);
        y += geometry::GRID_SPACING;
    }

    SelectObject(grid_dc, old_pen);
    let _ = DeleteObject(pen);

    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER,
        BlendFlags: 0,
        SourceConstantAlpha: geometry::GRID_ALPHA,
        AlphaFormat: 0,
    };
    let _ = AlphaBlend(
        hdc, 0, tb_height, cw, content_height,
        grid_dc, 0, 0, cw, content_height,
        blend,
    );

    SelectObject(grid_dc, old_grid_bmp);
    let _ = DeleteObject(grid_bmp);
    let _ = DeleteDC(grid_dc);
}

unsafe fn fill_solid(hdc: HDC, rect: RECT, color: COLORREF) {
    let brush = CreateSolidBrush(color);
    FillRect(hdc, &rect, brush);
    let _ = DeleteObject(brush);
}

unsafe fn create_font(height: i32, name: windows::core::PCWSTR) -> HFONT {
    CreateFontW(
        height,
        0,
        0,
        0,
        FW_NORMAL.0 as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        DEFAULT_PITCH.0 as u32 | (FF_DONTCARE.0 as u32),
        name,
    )
}

/// Converts the configured logical font height to physical pixels for the
/// given DPI.
fn scale_font_for_dpi(dpi: u32) -> i32 {
    // `TITLE_FONT_HEIGHT` is negative (character height); scale by magnitude.
    let logical = (-geometry::TITLE_FONT_HEIGHT) as i64;
    ((logical * dpi as i64) / 96) as i32
}

/// Returns a freshly populated mutable buffer containing the title text.
/// `DrawTextW` requires `&mut [u16]`, so a stack-allocated buffer is used to
/// avoid per-paint heap allocation. Length is derived from [`WINDOW_TITLE`]
/// (asserted ASCII at compile time, so byte length equals UTF-16 length).
fn title_text_buffer() -> [u16; WINDOW_TITLE_LEN] {
    let mut buf = [0u16; WINDOW_TITLE_LEN];
    for (i, b) in WINDOW_TITLE.as_bytes().iter().enumerate() {
        buf[i] = *b as u16;
    }
    buf
}

/// Returns the width of DWM-drawn caption buttons, falling back to a sensible
/// constant if the API is unavailable.
unsafe fn caption_buttons_width(hwnd: HWND) -> i32 {
    geometry::caption_buttons_width(hwnd)
}
