use windows::Win32::UI::WindowsAndMessaging::*;

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

// --- BorderStyle Constants ---

pub const BORDER_WIDTH: i32 = 2;
pub const GRIP_SIZE: i32 = 6;
pub const HIT_TEST_MARGIN: i32 = 8;
pub const MIN_WIDTH: i32 = 200;
pub const MIN_HEIGHT: i32 = 150;

// --- Title Bar Constants ---

pub const TITLE_BAR_HEIGHT: i32 = 24;
/// Close button width (square, same height as title bar)
pub const CLOSE_BUTTON_WIDTH: i32 = 36;
/// RGB(232, 17, 35) — red close button hover background
pub const CLOSE_BUTTON_HOVER_COLOR: u32 = 232 | (17 << 8) | (35 << 16);

// --- Theme Colors ---

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct ThemeColors {
    pub border: u32,
    /// Border color when window is focused (accent color if enabled, otherwise same as border)
    pub active_border: u32,
    pub title_bar_bg: u32,
    pub title_bar_text: u32,
}

/// Dark mode colors
pub const DARK_THEME: ThemeColors = ThemeColors {
    border: 100 | (100 << 8) | (100 << 16),          // RGB(100, 100, 100)
    active_border: 100 | (100 << 8) | (100 << 16),   // same until accent overrides
    title_bar_bg: 45 | (45 << 8) | (45 << 16),       // RGB(45, 45, 45)
    title_bar_text: 200 | (200 << 8) | (200 << 16),  // RGB(200, 200, 200)
};

/// Light mode colors
pub const LIGHT_THEME: ThemeColors = ThemeColors {
    border: 160 | (160 << 8) | (160 << 16),           // RGB(160, 160, 160)
    active_border: 160 | (160 << 8) | (160 << 16),    // same until accent overrides
    title_bar_bg: 243 | (243 << 8) | (243 << 16),     // RGB(243, 243, 243)
    title_bar_text: 30 | (30 << 8) | (30 << 16),      // RGB(30, 30, 30)
};

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

/// Performs hit testing for WM_NCHITTEST. Returns the appropriate HT* value.
pub fn hit_test(cursor: Point, window_rect: Rect, margin: i32, grip: i32) -> i32 {
    let x = cursor.x;
    let y = cursor.y;
    let left = window_rect.left;
    let top = window_rect.top;
    let right = window_rect.right;
    let bottom = window_rect.bottom;

    let on_left = x >= left && x < left + margin;
    let on_right = x >= right - margin && x < right;
    let on_top = y >= top && y < top + margin;
    let on_bottom = y >= bottom - margin && y < bottom;

    let in_top_grip = y >= top && y < top + grip;
    let in_bottom_grip = y >= bottom - grip && y < bottom;
    let in_left_grip = x >= left && x < left + grip;
    let in_right_grip = x >= right - grip && x < right;

    // Corners (check first — corners override edges)
    if on_top && in_left_grip || on_left && in_top_grip {
        return HTTOPLEFT as i32;
    }
    if on_top && in_right_grip || on_right && in_top_grip {
        return HTTOPRIGHT as i32;
    }
    if on_bottom && in_left_grip || on_left && in_bottom_grip {
        return HTBOTTOMLEFT as i32;
    }
    if on_bottom && in_right_grip || on_right && in_bottom_grip {
        return HTBOTTOMRIGHT as i32;
    }

    // Edges
    if on_left {
        return HTLEFT as i32;
    }
    if on_right {
        return HTRIGHT as i32;
    }
    if on_top {
        return HTTOP as i32;
    }
    if on_bottom {
        return HTBOTTOM as i32;
    }

    // Close button in the title bar — return HTCLIENT so we receive mouse msgs
    if y < top + TITLE_BAR_HEIGHT && x >= right - CLOSE_BUTTON_WIDTH {
        return HTCLIENT as i32;
    }

    // Title bar area — return HTCAPTION for drag-to-move
    if y < top + TITLE_BAR_HEIGHT {
        return HTCAPTION as i32;
    }

    // Interior — return HTCAPTION for drag-to-move
    HTCAPTION as i32
}

/// Returns true if the given client-relative point is inside the close button.
pub fn is_in_close_button(client_x: i32, client_y: i32, client_width: i32) -> bool {
    client_y < TITLE_BAR_HEIGHT && client_x >= client_width - CLOSE_BUTTON_WIDTH
}

/// Constrains a RECT to enforce minimum size during WM_SIZING.
/// `edge` corresponds to WMSZ_* values indicating which edge is being dragged.
pub fn constrain_size(rect: &mut Rect, min_width: i32, min_height: i32, edge: usize) {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;

    if width < min_width {
        match edge {
            // WMSZ_LEFT=1, WMSZ_TOPLEFT=4, WMSZ_BOTTOMLEFT=7
            1 | 4 | 7 => rect.left = rect.right - min_width,
            // WMSZ_RIGHT=2, WMSZ_TOPRIGHT=5, WMSZ_BOTTOMRIGHT=8
            _ => rect.right = rect.left + min_width,
        }
    }

    if height < min_height {
        match edge {
            // WMSZ_TOP=3, WMSZ_TOPLEFT=4, WMSZ_TOPRIGHT=5
            3 | 4 | 5 => rect.top = rect.bottom - min_height,
            // WMSZ_BOTTOM=6, WMSZ_BOTTOMLEFT=7, WMSZ_BOTTOMRIGHT=8
            _ => rect.bottom = rect.top + min_height,
        }
    }
}

/// Constrains a RECT to stay within monitor work area during WM_MOVING.
pub fn constrain_position(rect: &mut Rect, work_area: Rect) {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;

    if rect.left < work_area.left {
        rect.left = work_area.left;
        rect.right = rect.left + width;
    }
    if rect.right > work_area.right {
        rect.right = work_area.right;
        rect.left = rect.right - width;
    }
    if rect.top < work_area.top {
        rect.top = work_area.top;
        rect.bottom = rect.top + height;
    }
    if rect.bottom > work_area.bottom {
        rect.bottom = work_area.bottom;
        rect.top = rect.bottom - height;
    }
}

/// Converts logical pixels to physical pixels for the given DPI (rounded).
#[cfg(test)]
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

    // --- hit_test tests ---

    fn make_window_rect() -> Rect {
        Rect {
            left: 100,
            top: 100,
            right: 500,
            bottom: 400,
        }
    }

    #[test]
    fn hit_test_interior_returns_caption() {
        let r = make_window_rect();
        let cursor = Point { x: 250, y: 250 };
        assert_eq!(hit_test(cursor, r, 8, 6), HTCAPTION as i32);
    }

    #[test]
    fn hit_test_left_edge() {
        let r = make_window_rect();
        let cursor = Point { x: 103, y: 250 };
        assert_eq!(hit_test(cursor, r, 8, 6), HTLEFT as i32);
    }

    #[test]
    fn hit_test_right_edge() {
        let r = make_window_rect();
        let cursor = Point { x: 497, y: 250 };
        assert_eq!(hit_test(cursor, r, 8, 6), HTRIGHT as i32);
    }

    #[test]
    fn hit_test_top_edge() {
        let r = make_window_rect();
        let cursor = Point { x: 250, y: 103 };
        assert_eq!(hit_test(cursor, r, 8, 6), HTTOP as i32);
    }

    #[test]
    fn hit_test_bottom_edge() {
        let r = make_window_rect();
        let cursor = Point { x: 250, y: 397 };
        assert_eq!(hit_test(cursor, r, 8, 6), HTBOTTOM as i32);
    }

    #[test]
    fn hit_test_top_left_corner() {
        let r = make_window_rect();
        let cursor = Point { x: 102, y: 102 };
        assert_eq!(hit_test(cursor, r, 8, 6), HTTOPLEFT as i32);
    }

    #[test]
    fn hit_test_top_right_corner() {
        let r = make_window_rect();
        let cursor = Point { x: 498, y: 102 };
        assert_eq!(hit_test(cursor, r, 8, 6), HTTOPRIGHT as i32);
    }

    #[test]
    fn hit_test_bottom_left_corner() {
        let r = make_window_rect();
        let cursor = Point { x: 102, y: 398 };
        assert_eq!(hit_test(cursor, r, 8, 6), HTBOTTOMLEFT as i32);
    }

    #[test]
    fn hit_test_bottom_right_corner() {
        let r = make_window_rect();
        let cursor = Point { x: 498, y: 398 };
        assert_eq!(hit_test(cursor, r, 8, 6), HTBOTTOMRIGHT as i32);
    }

    // --- constrain_size tests ---

    #[test]
    fn constrain_size_enforces_min_width_from_right() {
        let mut r = Rect {
            left: 100,
            top: 100,
            right: 200,
            bottom: 400,
        }; // width=100
        constrain_size(&mut r, 200, 150, 2); // WMSZ_RIGHT
        assert_eq!(r.right, 300); // 100 + 200
        assert_eq!(r.left, 100);
    }

    #[test]
    fn constrain_size_enforces_min_width_from_left() {
        let mut r = Rect {
            left: 200,
            top: 100,
            right: 300,
            bottom: 400,
        }; // width=100
        constrain_size(&mut r, 200, 150, 1); // WMSZ_LEFT
        assert_eq!(r.left, 100); // 300 - 200
        assert_eq!(r.right, 300);
    }

    #[test]
    fn constrain_size_enforces_min_height_from_bottom() {
        let mut r = Rect {
            left: 100,
            top: 100,
            right: 500,
            bottom: 200,
        }; // height=100
        constrain_size(&mut r, 200, 150, 6); // WMSZ_BOTTOM
        assert_eq!(r.bottom, 250); // 100 + 150
        assert_eq!(r.top, 100);
    }

    #[test]
    fn constrain_size_enforces_min_height_from_top() {
        let mut r = Rect {
            left: 100,
            top: 200,
            right: 500,
            bottom: 300,
        }; // height=100
        constrain_size(&mut r, 200, 150, 3); // WMSZ_TOP
        assert_eq!(r.top, 150); // 300 - 150
        assert_eq!(r.bottom, 300);
    }

    // --- constrain_position tests ---

    #[test]
    fn constrain_position_clamps_left() {
        let work_area = Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let mut r = Rect {
            left: -50,
            top: 100,
            right: 350,
            bottom: 400,
        };
        constrain_position(&mut r, work_area);
        assert_eq!(r.left, 0);
        assert_eq!(r.right, 400);
    }

    #[test]
    fn constrain_position_clamps_right() {
        let work_area = Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let mut r = Rect {
            left: 1800,
            top: 100,
            right: 2200,
            bottom: 400,
        };
        constrain_position(&mut r, work_area);
        assert_eq!(r.right, 1920);
        assert_eq!(r.left, 1520);
    }

    #[test]
    fn constrain_position_clamps_top() {
        let work_area = Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let mut r = Rect {
            left: 100,
            top: -30,
            right: 400,
            bottom: 270,
        };
        constrain_position(&mut r, work_area);
        assert_eq!(r.top, 0);
        assert_eq!(r.bottom, 300);
    }

    #[test]
    fn constrain_position_clamps_bottom() {
        let work_area = Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let mut r = Rect {
            left: 100,
            top: 900,
            right: 400,
            bottom: 1200,
        };
        constrain_position(&mut r, work_area);
        assert_eq!(r.bottom, 1080);
        assert_eq!(r.top, 780);
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
}