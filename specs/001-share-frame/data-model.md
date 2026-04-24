# Data Model: Share Frame

**Feature**: Share Frame | **Date**: 2026-04-24

## Entities

### FrameWindow

The main application window. Owns the capture region and manages all user interaction.

| Field | Type | Description | Constraints |
|-------|------|-------------|-------------|
| `hwnd` | `HWND` | Win32 window handle | Assigned by `CreateWindowExW`; non-null while window exists |
| `position` | `Point { x: i32, y: i32 }` | Top-left corner in logical pixels | Must be within monitor work area bounds |
| `size` | `Size { width: i32, height: i32 }` | Width and height in logical pixels | Min: 200×150, Max: monitor work area dimensions |
| `visible` | `bool` | Whether the window is currently shown | `true` on launch |
| `dpi` | `u32` | Current monitor DPI value | Retrieved via `GetDpiForWindow`; default 96 |

**Validation rules**:
- `size.width >= 200` and `size.height >= 150` (minimum size enforcement)
- `position.x >= monitor.left` and `position.x + size.width <= monitor.right`
- `position.y >= monitor.top` and `position.y + size.height <= monitor.bottom`

### CaptureState

Manages the screen capture lifecycle and frame buffer.

| Field | Type | Description | Constraints |
|-------|------|-------------|-------------|
| `timer_id` | `usize` | ID returned by `SetTimer` | Non-zero while capture is active |
| _(desktop_dc)_ | _(HDC)_ | _(transient — acquired/released per frame in capture_frame, not a struct field)_ | _(GetDC(None) / ReleaseDC per WM_TIMER)_ |
| `memory_dc` | `CreatedHDC` | Compatible memory DC for the captured bitmap | Created once, reused across frames |
| `bitmap` | `HBITMAP` | Compatible bitmap for captured content | Recreated on window resize |
| `width` | `i32` | Bitmap width in physical pixels | Set during init/resize, used for BitBlt |
| `height` | `i32` | Bitmap height in physical pixels | Set during init/resize, used for BitBlt |
| `frame_interval_ms` | `u32` | Timer interval in milliseconds | Default: 33 (~30 FPS); range 16–100 |
| `capture_ok` | `bool` | Whether the last capture succeeded | Default: `true`; set to `false` if BitBlt or DC acquisition fails |

**Lifecycle**:
1. Created when window is first shown
2. `bitmap` recreated whenever window size changes
3. `desktop_dc` acquired and released each frame
4. Destroyed when window is closed

### BorderStyle

Visual configuration for the frame border (compile-time constants).

| Field | Type | Description | Value |
|-------|------|-------------|-------|
| `border_width` | `i32` | Border line thickness in logical pixels | 3 |
| `grip_size` | `i32` | Corner grip area size in logical pixels | 6 |
| `border_color` | `COLORREF` | Border line color | `RGB(100, 100, 100)` (neutral dark gray) |
| `hit_test_margin` | `i32` | Margin around window edges for resize hit detection | 8 |

### MonitorInfo

Cached information about the monitor the window is on.

| Field | Type | Description | Constraints |
|-------|------|-------------|-------------|
| `work_area` | `Rect { left, top, right, bottom }` | Usable monitor area (excludes taskbar) | Retrieved via `MonitorFromWindow` + `GetMonitorInfoW` |
| `dpi_scale` | `f64` | DPI scale factor (e.g., 1.0, 1.25, 1.5) | Computed as `dpi / 96.0` |

## Relationships

```text
FrameWindow 1 ──── 1 CaptureState    (window owns its capture state)
FrameWindow 1 ──── 1 MonitorInfo      (window tracks its current monitor)
FrameWindow * ──── 1 BorderStyle      (all windows share the same border constants)
```

## State Transitions

### Window Lifecycle

```text
[Not Running] ──launch──▸ [Initializing]
[Initializing] ──CreateWindowExW──▸ [Visible]
[Visible] ──WM_CLOSE──▸ [Destroying]
[Destroying] ──DestroyWindow──▸ [Not Running]
```

### Capture Lifecycle

```text
[Idle] ──SetTimer──▸ [Capturing]
[Capturing] ──WM_TIMER──▸ [Frame Captured] ──InvalidateRect──▸ [Capturing]
[Capturing] ──WM_TIMER (BitBlt fails)──▸ [Error] ──InvalidateRect──▸ [Capturing]
[Capturing] ──WM_SIZE──▸ [Resizing] ──recreate bitmap──▸ [Capturing]
[Capturing] ──KillTimer──▸ [Idle]
```

## Coordinate System Notes

- **Logical pixels**: Used for all user-facing dimensions (window position/size, minimum size, border width). These are the units returned by `GetWindowRect` on a DPI-aware window.
- **Physical pixels**: Used for BitBlt capture coordinates. The desktop DC operates in physical pixel space. Conversion: `physical = logical * dpi_scale`.
- **DPI scaling**: Applied via `GetDpiForWindow` after window creation. All logical→physical conversions go through the `geometry` module.
