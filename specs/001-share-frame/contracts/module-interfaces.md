# Module Interface Contract

**Feature**: Share Frame | **Date**: 2026-04-24

This document defines the public interfaces between the five source modules.

## Module Dependency Graph

```text
main.rs
  ├── window.rs    (creates window, runs message loop)
  │     ├── capture.rs   (called from WndProc for WM_TIMER, WM_SIZE)
  │     │     └── geometry.rs  (logical_to_physical for DPI conversion in capture)
  │     ├── render.rs    (called from WndProc for WM_PAINT)
  │     └── geometry.rs  (called from WndProc for WM_NCHITTEST, WM_SIZING, WM_MOVING)
  └── geometry.rs  (called from main for default size/position calculation)
```

## main.rs

```rust
fn main()
```

Responsibilities:
- Call `SetProcessDpiAwarenessContext`
- Create named mutex for single-instance check
- If duplicate instance: find existing window, foreground it, exit
- Call `window::create_and_run()` to create the window and enter the message loop

## window.rs

```rust
/// Creates the window and runs the Win32 message loop. Returns when WM_QUIT is received.
pub fn create_and_run() -> windows::core::Result<()>

/// Window procedure. Dispatches messages to capture, render, and geometry modules.
unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT
```

Internal state (per-window, stored via `SetWindowLongPtrW` / `GWLP_USERDATA`):
- `CaptureState` — owned, passed to `capture` module functions
- Current DPI value
- Monitor work area cache

## capture.rs

```rust
/// Initializes capture state: creates memory DC, compatible bitmap, starts timer.
/// Called during WM_CREATE. Width/height are logical pixels; converted to physical using dpi.
pub fn init(hwnd: HWND, width: i32, height: i32, dpi: u32) -> CaptureState

/// Captures the desktop region behind the window. Called on WM_TIMER.
/// Sets alpha=0 on layered window, BitBlts desktop region, restores alpha=255, invalidates rect.
/// Returns true if capture succeeded, false if BitBlt or DC acquisition failed.
/// On failure, sets state.capture_ok = false so render::paint can show fallback.
pub fn capture_frame(hwnd: HWND, state: &mut CaptureState) -> bool

/// Recreates the bitmap to match new window dimensions. Called on WM_SIZE.
/// Width/height are logical pixels; converted to physical using dpi.
pub fn resize(state: &mut CaptureState, width: i32, height: i32, dpi: u32)

/// Cleans up capture resources (kills timer, deletes DC/bitmap). Called on WM_DESTROY.
pub fn cleanup(state: &mut CaptureState)
```

## render.rs

```rust
/// Paints captured content and border onto the window. Called during WM_PAINT.
/// - If state.capture_ok: BitBlts from CaptureState.memory_dc to the paint DC
/// - If !state.capture_ok: fills the window with a dark red background as error indicator
/// - Draws 3px border and 6px corner grips in neutral dark gray
pub fn paint(hwnd: HWND, state: &CaptureState)
```

## geometry.rs

```rust
/// Calculates the default window size for the given monitor.
/// Returns min(1920, monitor_width * 0.75) with 16:9 aspect ratio.
pub fn default_size(monitor_width: i32, monitor_height: i32) -> Size

/// Calculates centered position for a window of the given size on the given monitor.
pub fn centered_position(window_size: Size, work_area: Rect) -> Point

/// Performs hit testing for WM_NCHITTEST. Returns the appropriate HT* value
/// based on cursor position relative to window bounds and border margins.
pub fn hit_test(cursor: Point, window_rect: Rect, margin: i32, grip: i32) -> i32

/// Constrains a RECT to enforce minimum size during WM_SIZING.
pub fn constrain_size(rect: &mut Rect, min_width: i32, min_height: i32, edge: usize)

/// Constrains a RECT to stay within monitor work area during WM_MOVING.
pub fn constrain_position(rect: &mut Rect, work_area: Rect)

/// Converts logical pixels to physical pixels for the given DPI.
pub fn logical_to_physical(logical: i32, dpi: u32) -> i32

/// Converts physical pixels to logical pixels for the given DPI.
pub fn physical_to_logical(physical: i32, dpi: u32) -> i32

/// Gets the work area for the monitor containing the given window.
pub fn get_monitor_work_area(hwnd: HWND) -> Rect

// --- Types ---
pub struct Point { pub x: i32, pub y: i32 }
pub struct Size { pub width: i32, pub height: i32 }
pub struct Rect { pub left: i32, pub top: i32, pub right: i32, pub bottom: i32 }
```

**Unit-testable functions** (no Win32 dependency): `default_size`, `centered_position`, `hit_test`, `constrain_size`, `constrain_position`, `logical_to_physical`, `physical_to_logical`.

**Win32-dependent functions** (manual testing only): `get_monitor_work_area`.

## Error Handling Strategy

| Call Site | Failure Mode | Behavior |
|-----------|-------------|----------|
| `CreateWindowExW` returns null | Window creation failed | Log error via `GetLastError`, exit process with non-zero code |
| `SetTimer` returns 0 | Timer creation failed | Log error, exit process (capture is essential) |
| `GetDC(None)` returns null | Desktop DC acquisition failed | Set `capture_ok = false`, skip frame, retry next timer tick |
| `CreateCompatibleDC` / `CreateCompatibleBitmap` returns null | Memory DC/bitmap creation failed | Log error, exit process (cannot function without capture buffer) |
| `BitBlt` returns FALSE | Blit operation failed | Set `capture_ok = false`, skip frame, retry next timer tick |
| `SetLayeredWindowAttributes` fails | Alpha toggle failed | Log warning, attempt capture anyway (may include self in capture) |
| `CreateMutexW` fails | Mutex creation failed | Log warning, proceed without single-instance enforcement |
| `FindWindowW` fails | Cannot find existing instance | Exit silently (mutex indicates instance exists but window not found) |

**General rules**:
- Fatal failures (window creation, initial resource allocation) → exit with error message via `MessageBoxW`
- Transient failures (per-frame DC acquisition, BitBlt) → degrade gracefully, retry on next tick
- All unsafe Win32 FFI calls check return values; null/zero/FALSE results are never ignored

## P3 Migration Notes

**Context menu (FR-014)**: The current hit-test design returns `HTCAPTION` for the entire interior area (enabling drag-to-move). In `HTCAPTION` regions, right-click triggers the Win32 system window menu rather than `WM_CONTEXTMENU`. When P3 is implemented, the hit-test must be modified to either: (a) split the interior into `HTCLIENT` with manual move via `WM_LBUTTONDOWN` + `SC_MOVE`, or (b) intercept `WM_NCRBUTTONUP` in the HTCAPTION area to show a custom context menu.
