# Research: Share Frame

**Feature**: Share Frame | **Date**: 2026-04-24

## R1: Screen Capture Approach — BitBlt vs Windows Graphics Capture API

**Decision**: BitBlt from the desktop device context (DC) for MVP.

**Rationale**: BitBlt is the simplest, most widely compatible screen capture method on Windows. It works on all Windows versions (no 1903+ requirement in practice), has minimal setup, and captures the raw pixel content of the desktop. For this application's needs — capturing a rectangular region of the screen at 15–30 FPS — BitBlt provides adequate performance with minimal code complexity. The Windows Graphics Capture API, while more modern and efficient, requires significantly more setup code (WinRT activation, frame pool management, Direct3D device creation) and adds complexity that violates the Simplicity First principle for marginal benefit at this frame rate.

**Alternatives considered**:
- **Windows Graphics Capture API**: More efficient for high-FPS capture, supports hardware acceleration, but requires WinRT interop, a Direct3D device, and significantly more boilerplate. Overkill for 15–30 FPS of a desktop region. Reserve as future optimization if BitBlt proves insufficient.
- **DXGI Desktop Duplication**: Captures the entire desktop at GPU level. Very fast but requires Direct3D setup and outputs the full screen — additional cropping logic needed. More complex than BitBlt for a windowed region capture.
- **PrintWindow**: Captures a specific window's content. Not applicable here — we need the desktop region behind our window, not a specific window.

## R2: Window Style for Borderless Resizable Window

**Decision**: Use `WS_POPUP | WS_VISIBLE` style with custom `WM_NCHITTEST` handling.

**Rationale**: A `WS_POPUP` window has no title bar or system borders, giving full control over the window's appearance. By handling `WM_NCHITTEST` in the window procedure, we can define custom hit-test regions that map mouse positions to resize edges (HTLEFT, HTRIGHT, HTTOP, HTBOTTOM, HTTOPLEFT, etc.) and a move region (HTCAPTION). This approach leverages the native Win32 resize/move machinery — Windows handles the actual drag/resize loop, providing native-feeling interaction without implementing custom drag logic.

**Alternatives considered**:
- **WS_OVERLAPPEDWINDOW with DWM customization**: Retains the standard window frame. Would require DwmExtendFrameIntoClientArea or similar tricks to hide the title bar while keeping resize. More complex and less control over appearance.
- **Custom drag/resize via WM_LBUTTONDOWN tracking**: Full manual implementation of drag and resize. Significantly more code, harder to get right (acceleration, snap, edge cases), and reimplements what WM_NCHITTEST provides for free.

## R3: Avoiding Capture Feedback Loop

**Decision**: Use `WS_EX_LAYERED` window with temporary alpha=0 during BitBlt capture.

**Rationale**: When capturing the desktop region that includes our own window, we'd get a feedback loop (the window captures itself). The approach is to make the window a layered window (`WS_EX_LAYERED`) and use `SetLayeredWindowAttributes` to set alpha=0 just before each BitBlt capture, then restore alpha=255 immediately after. With alpha=0, the DWM compositor renders our window as fully transparent, so BitBlt from the desktop DC at our window's position captures the content behind it — exactly what we want. Critically, DWM maintains the window's redirected surface regardless of the layered alpha value, so Teams' window capture (which reads the DWM surface) always sees our last-painted content. This avoids the risk of blank frames that `ShowWindow(SW_HIDE)` would introduce, since hiding a window can cause DWM to discard its cached composition bitmap.

**Alternatives considered**:
- **ShowWindow(SW_HIDE) / ShowWindow(SW_SHOW) per frame**: Risks DWM discarding the window's redirected surface during the hidden interval. If Teams' capture reads during the ~1-5ms hidden window, remote participants see blank/black frames. Also risks taskbar button flickering at 30 FPS with `WS_EX_APPWINDOW`.
- **SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)**: Would exclude our window from BitBlt capture, but also excludes it from Teams' window capture — defeating the entire purpose of the application.
- **Windows Graphics Capture API with window exclusion**: The Graphics Capture API can exclude specific windows, but would require the more complex capture approach from R1.

## R4: Rendering Captured Content onto Window Surface

**Decision**: GDI `BitBlt` from a memory DC onto the window DC in `WM_PAINT`.

**Rationale**: The captured bitmap is already in a GDI-compatible device context (from the capture step). Painting it onto the window surface is a single `BitBlt` call in the `WM_PAINT` handler. The border and corner grips are drawn on top using GDI primitives (`Rectangle`, `FillRect` with solid brushes). This is the simplest rendering approach with zero additional dependencies. Direct2D would add unnecessary complexity for drawing a static bitmap and some rectangles.

**Alternatives considered**:
- **Direct2D**: Hardware-accelerated 2D rendering. Significant setup code (factory, render target, bitmap conversion). Overkill for blitting a pre-captured bitmap and drawing rectangles.
- **DirectComposition**: Visual tree-based composition. Very powerful but extreme overkill for this use case.
- **GDI+**: Slightly higher-level than GDI, but adds a dependency and initialization overhead for no benefit here.

## R5: DPI Awareness Strategy

**Decision**: Per-monitor DPI awareness v2 via `SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)`.

**Rationale**: Per-monitor v2 is the most modern DPI awareness mode and handles scaling correctly on multi-DPI setups. It must be set before any window creation. All user-facing dimensions (default size, minimum size) are specified in logical pixels and scaled to physical pixels using the monitor's DPI. Capture coordinates are converted to physical pixels for BitBlt (since BitBlt operates in physical pixel space on the desktop DC).

**Alternatives considered**:
- **System DPI aware**: Simpler but incorrect on multi-DPI setups. The frame would render at the wrong size on non-primary monitors.
- **DPI unaware**: Windows would bitmap-scale the application, causing blurry rendering. Unacceptable for a screen capture tool.
- **Application manifest DPI declaration**: Works but is less flexible than the runtime API call. The API call is one line of code and equally effective.

## R6: Single-Instance Enforcement

**Decision**: Named mutex via `CreateMutexW`. If the mutex already exists (`GetLastError() == ERROR_ALREADY_EXISTS`), find the existing window via `FindWindowW` and bring it to the foreground, then exit.

**Rationale**: Named mutex is the standard Win32 pattern for single-instance enforcement. It's simple, reliable, and well-documented. The mutex name should be unique (e.g., `"ShareFrame_SingleInstance_Mutex"`). Combined with `FindWindowW("ShareFrameClass", "Share Frame")` to locate and foreground the existing instance.

**Alternatives considered**:
- **File lock**: More complex, requires cleanup on crash, and doesn't provide a way to communicate with the existing instance.
- **Shared memory**: Overkill for a boolean "am I running?" check.

## R7: Timer-Based Capture Loop

**Decision**: Use `SetTimer` with a ~33ms interval (approximately 30 FPS) for the capture loop.

**Rationale**: `WM_TIMER` messages are low-priority and don't block the message loop. The timer fires at approximately the desired interval, triggering a capture-and-invalidate cycle. `InvalidateRect` after each capture causes a `WM_PAINT` to redraw with the new content. This approach is simple and integrates naturally with the Win32 message loop. No separate threads needed.

**Alternatives considered**:
- **Dedicated capture thread**: More precise timing but adds threading complexity (synchronization, thread-safe bitmap handoff). Unnecessary at 30 FPS.
- **`WM_PAINT` self-invalidation loop**: Would consume 100% of one CPU core. Not acceptable.
- **Multimedia timer (timeSetEvent)**: More precise than WM_TIMER but adds multimedia API dependency. 30 FPS doesn't need millisecond precision.

## R8: windows-rs Crate Configuration

**Decision**: Use the `windows` crate with feature flags for the specific Win32 APIs needed.

**Rationale**: The `windows` crate from Microsoft generates bindings on demand based on Cargo feature flags. Only the needed APIs are compiled in, keeping binary size small. Required feature flags:

```toml
[dependencies.windows]
version = "0.58"  # or latest stable
features = [
    "Win32_Foundation",
    "Win32_UI_WindowsAndMessaging",
    "Win32_Graphics_Gdi",
    "Win32_System_LibraryLoader",
    "Win32_System_Threading",
    "Win32_UI_HiDpi",
]
```

**Alternatives considered**:
- **`winapi` crate**: Older, manually maintained bindings. Less ergonomic, no longer recommended now that `windows-rs` is the official Microsoft crate.
- **Raw FFI declarations**: Maximum control but enormous maintenance burden. The `windows` crate handles this correctly.
