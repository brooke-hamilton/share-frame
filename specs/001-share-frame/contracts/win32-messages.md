# Window Procedure Message Contract

**Feature**: Share Frame | **Date**: 2026-04-24

This document defines the Win32 message handling contract — the messages the Share Frame window processes, their expected behavior, and the module responsible for each.

## Message Handling Table

| Message | Handler Module | Behavior |
|---------|---------------|----------|
| `WM_CREATE` | `window.rs` | Initialize capture state, set timer, get monitor info |
| `WM_DESTROY` | `window.rs` | Kill timer, release GDI resources, call `PostQuitMessage(0)` |
| `WM_PAINT` | `render.rs` | BitBlt captured bitmap onto window DC, draw border and corner grips |
| `WM_TIMER` | `capture.rs` | Set alpha=0 → BitBlt desktop region → set alpha=255 → `InvalidateRect` |
| `WM_NCHITTEST` | `window.rs` | Return `HTCAPTION` for move region, `HTLEFT`/`HTRIGHT`/`HTTOP`/`HTBOTTOM`/`HTTOPLEFT`/`HTTOPRIGHT`/`HTBOTTOMLEFT`/`HTBOTTOMRIGHT` for resize edges/corners |
| `WM_SIZE` | `capture.rs` | Recreate the compatible bitmap to match new window size |
| `WM_SIZING` | `geometry.rs` | Enforce minimum size (200×150) and monitor bounds constraints |
| `WM_MOVING` | `geometry.rs` | Constrain window position within monitor work area |
| `WM_DPICHANGED` | `window.rs` | Update DPI value, resize/reposition per suggested rect |
| `WM_DISPLAYCHANGE` | `window.rs` | Re-query monitor bounds via `geometry::get_monitor_work_area`, reposition via `SetWindowPos` if window is out of bounds |
| `WM_ERASEBKGND` | `window.rs` | Return 1 (non-zero) directly in WndProc to prevent background erase flicker |

## Hit Test Regions

The `WM_NCHITTEST` handler divides the window client area into regions based on cursor position relative to window edges:

```text
┌──HTTOPLEFT──────────HTTOP──────────HTTOPRIGHT──┐
│  (grip_size)                      (grip_size)   │
│                                                  │
HTLEFT            HTCAPTION              HTRIGHT
│                                                  │
│  (grip_size)                      (grip_size)   │
└──HTBOTTOMLEFT───HTBOTTOM───────HTBOTTOMRIGHT───┘

Edge detection margin: hit_test_margin (8 logical pixels)
Corner detection zone: grip_size (6 logical pixels) from each corner
Move region: entire interior (returns HTCAPTION)
```

## Capture-Render Pipeline

```text
WM_TIMER fires
  │
  ├─ 1. SetLayeredWindowAttributes(hwnd, 0, 0, LWA_ALPHA)   [alpha=0: transparent to desktop capture]
  ├─ 2. GetDC(None)                                          [acquire desktop DC]
  ├─ 3. BitBlt(memory_dc ← desktop_dc)                      [capture region at physical coords]
  ├─ 4. ReleaseDC(None, desktop_dc)                          [release desktop DC]
  ├─ 5. SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA)  [alpha=255: restore full opacity]
  └─ 6. InvalidateRect(hwnd)                                 [trigger WM_PAINT]
  └─ 6. InvalidateRect(hwnd)              [trigger WM_PAINT]

WM_PAINT fires
  │
  ├─ 1. BeginPaint(hwnd)                  [get paint DC]
  ├─ 2. BitBlt(paint_dc ← memory_dc)     [blit captured content]
  ├─ 3. Draw border rectangles            [3px border, 6px corner grips]
  └─ 4. EndPaint(hwnd)                    [release paint DC]
```

## Startup Sequence

```text
1. SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
2. CreateMutexW("ShareFrame_SingleInstance_Mutex")
   ├─ If ERROR_ALREADY_EXISTS → FindWindowW + SetForegroundWindow → exit
   └─ If success → continue
3. RegisterClassExW (class name: "ShareFrameClass")
4. Calculate default size: min(1920, monitor_width * 0.75) × corresponding 16:9 height
5. Calculate centered position on primary monitor
6. CreateWindowExW(WS_POPUP | WS_VISIBLE, "ShareFrameClass", "Share Frame", ...)
7. Enter message loop: GetMessage → TranslateMessage → DispatchMessage
8. Cleanup and exit on WM_QUIT
```

## Window Registration

| Property | Value |
|----------|-------|
| Class name | `"ShareFrameClass"` |
| Window title | `"Share Frame"` |
| Style | `WS_POPUP \| WS_VISIBLE` |
| Extended style | `WS_EX_APPWINDOW \| WS_EX_LAYERED` (ensures taskbar presence, Teams discoverability, and alpha-based capture exclusion) |
| Background brush | `None` (handled in WM_PAINT) |
