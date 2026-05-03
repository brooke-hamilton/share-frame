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