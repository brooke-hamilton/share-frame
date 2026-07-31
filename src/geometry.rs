// --- Types ---

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Size {
    pub width: i32,
    pub height: i32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

// --- Constants ---

pub const MIN_WIDTH: i32 = 200;
pub const MIN_HEIGHT: i32 = 150;

/// Width of the custom "Send to Back" button in the title bar.
pub const SEND_BACK_BUTTON_WIDTH: i32 = 46;
/// Resize border width for hit testing.
pub const RESIZE_MARGIN: i32 = 8;
/// Logical width (at 96 DPI) of each self-drawn caption button
/// (minimize / maximize / close). Share Frame paints these itself rather
/// than relying on DWM to composite native caption buttons through the
/// extended frame, which does not render on all machines.
pub const CAPTION_BUTTON_WIDTH: i32 = 46;

/// One of the three self-drawn caption buttons, ordered left-to-right as
/// they appear from the right edge of the title bar.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CaptionButton {
    Minimize,
    Maximize,
    Close,
}
/// Spacing in pixels between grid lines drawn over the captured frame.
pub const GRID_SPACING: i32 = 48;
/// Constant alpha used for the grid overlay (~10%).
pub const GRID_ALPHA: u8 = 25;
/// Title-bar font height in logical pixels (negative = character height).
pub const TITLE_FONT_HEIGHT: i32 = -12;
/// Pixel size of the small window icon shown in the custom title bar.
pub const TITLE_ICON_SIZE: i32 = 16;
/// Horizontal inset for the title-bar icon.
pub const TITLE_ICON_INSET: i32 = 8;

/// Returns the inclusive-exclusive horizontal range of the "Send to Back"
/// button, expressed as `(left, right)` in client coordinates.
pub fn send_back_button_range(client_width: i32, caption_buttons_width: i32) -> (i32, i32) {
    let right = client_width - caption_buttons_width;
    let left = right - SEND_BACK_BUTTON_WIDTH;
    (left, right)
}

/// Returns true when `(x, y)` (client coordinates) hits the "Send to Back"
/// button drawn left of the DWM caption buttons.
pub fn point_in_send_back_button(
    x: i32,
    y: i32,
    client_width: i32,
    title_bar_height: i32,
    caption_buttons_width: i32,
) -> bool {
    if y < 0 || y >= title_bar_height {
        return false;
    }
    let (left, right) = send_back_button_range(client_width, caption_buttons_width);
    x >= left && x < right
}

// --- Pure Functions ---

/// Calculates the default window size for the given monitor dimensions (logical pixels).
/// Returns min(1920, monitor_width * 75/100) with a 16:9 aspect ratio. If the
/// 16:9 height would exceed 75% of monitor height (tall/narrow monitors), the
/// width is recomputed from the height-clamped value so the result stays 16:9.
pub fn default_size(monitor_width: i32, monitor_height: i32) -> Size {
    let mut width = std::cmp::min(1920, monitor_width * 75 / 100);
    let mut height = width * 9 / 16;
    let max_height = monitor_height * 75 / 100;
    if height > max_height {
        height = max_height;
        width = height * 16 / 9;
    }
    Size { width, height }
}

/// Calculates centered position for a window within the work area.
pub fn centered_position(window_size: Size, work_area: Rect) -> Point {
    let area_width = work_area.right - work_area.left;
    let area_height = work_area.bottom - work_area.top;
    Point {
        x: work_area.left + (area_width - window_size.width) / 2,
        y: work_area.top + (area_height - window_size.height) / 2,
    }
}

/// Converts logical pixels to physical pixels for the given DPI (rounded).
pub fn logical_to_physical(logical: i32, dpi: u32) -> i32 {
    let dpi = dpi as i64;
    let logical = logical as i64;
    // Round to nearest, away from zero, instead of truncating toward zero.
    let scaled = logical * dpi;
    let rounded = if scaled >= 0 {
        (scaled + 48) / 96
    } else {
        (scaled - 48) / 96
    };
    rounded as i32
}

/// Converts physical pixels to logical pixels for the given DPI.
#[cfg(test)]
pub fn physical_to_logical(physical: i32, dpi: u32) -> i32 {
    ((physical as i64 * 96) / dpi as i64) as i32
}

// --- Win32-Dependent Functions ---

/// Returns the total width of the three self-drawn caption buttons at the
/// given window's DPI. Used to reserve the right-hand strip of the title
/// bar and to position the "Send to Back" button left of it.
pub unsafe fn caption_buttons_width(hwnd: windows::Win32::Foundation::HWND) -> i32 {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

    let dpi = GetDpiForWindow(hwnd);
    let dpi = if dpi == 0 { 96 } else { dpi };
    let mut rect = RECT::default();
    let _ = GetClientRect(hwnd, &mut rect);
    let client_width = rect.right - rect.left;
    3 * caption_button_width(client_width, dpi)
}

/// Physical width of a single caption button, clamped so all three always
/// fit within `client_width`. Without the clamp, high-DPI + very narrow
/// windows (down to `MIN_WIDTH` physical pixels) would yield negative
/// `left` coordinates and make hit-testing treat the whole title bar as
/// caption buttons, breaking window dragging.
fn caption_button_width(client_width: i32, dpi: u32) -> i32 {
    let ideal = logical_to_physical(CAPTION_BUTTON_WIDTH, dpi);
    let max_fit = (client_width / 3).max(0);
    ideal.min(max_fit)
}

/// Returns the inclusive-exclusive client-x range `(left, right)` of one
/// caption button. From the right edge: Close, then Maximize, then Minimize.
pub fn caption_button_range(button: CaptionButton, client_width: i32, dpi: u32) -> (i32, i32) {
    let bw = caption_button_width(client_width, dpi);
    match button {
        CaptionButton::Close => (client_width - bw, client_width),
        CaptionButton::Maximize => (client_width - 2 * bw, client_width - bw),
        CaptionButton::Minimize => (client_width - 3 * bw, client_width - 2 * bw),
    }
}

/// Returns which caption button contains `(x, y)` (client coordinates), or
/// `None` if the point is outside the title bar or the caption-button strip.
pub fn caption_button_at(
    x: i32,
    y: i32,
    client_width: i32,
    title_bar_height: i32,
    dpi: u32,
) -> Option<CaptionButton> {
    if y < 0 || y >= title_bar_height {
        return None;
    }
    for button in [CaptionButton::Minimize, CaptionButton::Maximize, CaptionButton::Close] {
        let (left, right) = caption_button_range(button, client_width, dpi);
        if x >= left && x < right {
            return Some(button);
        }
    }
    None
}

/// Gets the work area for the monitor containing the given window.
pub fn get_monitor_work_area(hwnd: windows::Win32::Foundation::HWND) -> Rect {
    use windows::Win32::Graphics::Gdi::*;

    unsafe {
        let hmonitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmonitor, &mut info).as_bool() {
            Rect {
                left: info.rcWork.left,
                top: info.rcWork.top,
                right: info.rcWork.right,
                bottom: info.rcWork.bottom,
            }
        } else {
            // Fallback: assume 1920x1080
            Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- default_size tests ---

    #[test]
    fn default_size_large_monitor_caps_at_1920() {
        let size = default_size(3440, 1440);
        assert_eq!(size.width, 1920);
        assert_eq!(size.height, 1920 * 9 / 16); // 1080
    }

    #[test]
    fn default_size_small_monitor_uses_75_percent() {
        let size = default_size(1280, 720);
        assert_eq!(size.width, 1280 * 75 / 100); // 960
        assert_eq!(size.height, 960 * 9 / 16); // 540
    }

    // --- centered_position tests ---

    #[test]
    fn centered_position_in_work_area() {
        let size = Size {
            width: 800,
            height: 600,
        };
        let area = Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let pos = centered_position(size, area);
        assert_eq!(pos.x, (1920 - 800) / 2);
        assert_eq!(pos.y, (1080 - 600) / 2);
    }

    #[test]
    fn centered_position_with_offset_work_area() {
        let size = Size {
            width: 400,
            height: 300,
        };
        let area = Rect {
            left: 100,
            top: 50,
            right: 1100,
            bottom: 850,
        };
        let pos = centered_position(size, area);
        assert_eq!(pos.x, 100 + (1000 - 400) / 2);
        assert_eq!(pos.y, 50 + (800 - 300) / 2);
    }

    // --- DPI conversion tests ---

    #[test]
    fn logical_to_physical_96dpi() {
        assert_eq!(logical_to_physical(100, 96), 100);
    }

    #[test]
    fn logical_to_physical_120dpi() {
        assert_eq!(logical_to_physical(100, 120), 125);
    }

    #[test]
    fn logical_to_physical_144dpi() {
        assert_eq!(logical_to_physical(100, 144), 150);
    }

    #[test]
    fn physical_to_logical_96dpi() {
        assert_eq!(physical_to_logical(100, 96), 100);
    }

    #[test]
    fn physical_to_logical_120dpi() {
        assert_eq!(physical_to_logical(125, 120), 100);
    }

    #[test]
    fn physical_to_logical_144dpi() {
        assert_eq!(physical_to_logical(150, 144), 100);
    }

    #[test]
    fn dpi_round_trip_96() {
        let v = 123;
        assert_eq!(physical_to_logical(logical_to_physical(v, 96), 96), v);
    }

    #[test]
    fn dpi_round_trip_120() {
        let v = 100;
        assert_eq!(physical_to_logical(logical_to_physical(v, 120), 120), v);
    }

    #[test]
    fn dpi_round_trip_144() {
        let v = 100;
        assert_eq!(physical_to_logical(logical_to_physical(v, 144), 144), v);
    }

    // --- send-to-back button geometry ---

    #[test]
    fn send_back_button_range_is_left_of_caption_buttons() {
        let (left, right) = send_back_button_range(800, 138);
        assert_eq!(right, 800 - 138);
        assert_eq!(left, right - SEND_BACK_BUTTON_WIDTH);
    }

    #[test]
    fn point_in_send_back_button_inside() {
        let cw = 800;
        let cap = 138;
        let (left, _) = send_back_button_range(cw, cap);
        assert!(point_in_send_back_button(left + 1, 5, cw, 30, cap));
    }

    #[test]
    fn point_in_send_back_button_negative_y_is_outside() {
        assert!(!point_in_send_back_button(700, -1, 800, 30, 138));
    }

    #[test]
    fn point_in_send_back_button_below_title_bar_is_outside() {
        assert!(!point_in_send_back_button(700, 30, 800, 30, 138));
    }

    #[test]
    fn point_in_send_back_button_inside_caption_buttons_is_outside() {
        let cw = 800;
        let cap = 138;
        assert!(!point_in_send_back_button(cw - 10, 5, cw, 30, cap));
    }

    // --- caption button geometry ---

    #[test]
    fn caption_buttons_ordered_close_rightmost() {
        let cw = 800;
        let (min_l, min_r) = caption_button_range(CaptionButton::Minimize, cw, 96);
        let (max_l, max_r) = caption_button_range(CaptionButton::Maximize, cw, 96);
        let (close_l, close_r) = caption_button_range(CaptionButton::Close, cw, 96);
        assert_eq!(close_r, cw);
        assert_eq!(close_l, max_r);
        assert_eq!(max_l, min_r);
        assert_eq!(min_l, cw - 3 * CAPTION_BUTTON_WIDTH);
    }

    #[test]
    fn caption_button_at_maps_each_button() {
        let cw = 800;
        let (close_l, _) = caption_button_range(CaptionButton::Close, cw, 96);
        let (max_l, _) = caption_button_range(CaptionButton::Maximize, cw, 96);
        let (min_l, _) = caption_button_range(CaptionButton::Minimize, cw, 96);
        assert_eq!(caption_button_at(close_l + 1, 5, cw, 30, 96), Some(CaptionButton::Close));
        assert_eq!(caption_button_at(max_l + 1, 5, cw, 30, 96), Some(CaptionButton::Maximize));
        assert_eq!(caption_button_at(min_l + 1, 5, cw, 30, 96), Some(CaptionButton::Minimize));
    }

    #[test]
    fn caption_button_at_outside_strip_is_none() {
        let cw = 800;
        assert_eq!(caption_button_at(10, 5, cw, 30, 96), None);
        assert_eq!(caption_button_at(cw - 1, 30, cw, 30, 96), None);
        assert_eq!(caption_button_at(cw - 1, -1, cw, 30, 96), None);
    }

    #[test]
    fn caption_button_width_scales_with_dpi() {
        let at96 = caption_button_range(CaptionButton::Close, 800, 96);
        let at144 = caption_button_range(CaptionButton::Close, 800, 144);
        assert_eq!(at96.1 - at96.0, CAPTION_BUTTON_WIDTH);
        assert_eq!(at144.1 - at144.0, logical_to_physical(CAPTION_BUTTON_WIDTH, 144));
    }

    #[test]
    fn caption_buttons_stay_in_bounds_when_narrow() {
        // High DPI + a window at MIN_WIDTH: buttons would overflow without
        // the clamp. All ranges must stay within [0, client_width] and not
        // overlap.
        let cw = MIN_WIDTH;
        let dpi = 192; // 200%
        let (min_l, min_r) = caption_button_range(CaptionButton::Minimize, cw, dpi);
        let (max_l, max_r) = caption_button_range(CaptionButton::Maximize, cw, dpi);
        let (close_l, close_r) = caption_button_range(CaptionButton::Close, cw, dpi);
        assert!(min_l >= 0, "minimize left {min_l} should be non-negative");
        assert_eq!(min_r, max_l);
        assert_eq!(max_r, close_l);
        assert_eq!(close_r, cw);
        // caption_button_at must never match outside the client area.
        assert_eq!(caption_button_at(-1, 5, cw, 30, dpi), None);
    }
}