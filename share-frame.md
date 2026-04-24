# Share Frame

## Vision

Share Frame is a lightweight Windows desktop application for people with ultrawide monitors who need to share a specific region of their screen during Microsoft Teams calls. It creates a transparent, resizable, and moveable frame window that appears as a shareable application in Teams — anything visible within the frame is what remote participants see, eliminating the problem of sharing an oversized ultrawide display to viewers on standard monitors.

## Target Users

- **Ultrawide monitor user**: A knowledge worker with an ultrawide (21:9 or wider) monitor who regularly participates in Teams meetings and needs to share portions of their screen. Moderate technical level — comfortable installing and running desktop apps, but expects intuitive UX without configuration.

## Features

### Must-Have (P1)
1. **Transparent frame window**: A borderless, transparent window with a visible resize border that allows the user to see applications underneath. The frame captures and relays the underlying screen content so that when shared via Teams, remote viewers see everything within the frame region.
2. **Resizable border**: The frame border should have obvious, modern-looking resize handles/edges so the user can freeform drag to any size. The resize affordance should be intuitive without being garish — think subtle but clear visual cues (e.g., a thin border with slightly thicker corner grips).
3. **Moveable frame**: The user can click and drag the frame to reposition it anywhere on the current monitor.
4. **Teams-shareable window**: The window must register as a standard Win32 window with the title "Share Frame" so it appears in Teams' "Share content → Window" picker. When shared, Teams captures the frame's content (the underlying screen region), not just a transparent rectangle.
5. **Default launch size**: On launch, the frame appears centered on the primary monitor at a sensible default size (e.g., 1920×1080 or 75% of monitor width, whichever is smaller).

### Should-Have (P2)
1. **System tray icon**: A system tray icon for quick access to show/hide the frame and exit the application.
2. **Keyboard shortcut**: A global hotkey to toggle frame visibility (e.g., `Ctrl+Shift+F`).

### Nice-to-Have (P3)
1. **Preset resolution snapping**: Right-click context menu to snap the frame to common resolutions (1920×1080, 2560×1440, 1280×720).
2. **Remember last position/size**: Persist the frame's last position and size across app restarts.

## User Journeys

### Primary: Share a screen region in Teams
1. User launches Share Frame (e.g., from Start menu or taskbar).
2. A transparent frame with a visible border appears centered on the primary monitor at a default size.
3. User drags the frame to position it over the content they want to share (e.g., a specific app window, a portion of a dashboard).
4. User resizes the frame by dragging the border edges or corners to capture exactly the desired region.
5. User switches to Teams, clicks "Share content" → "Window" and selects "Share Frame" from the window list.
6. Remote participants see the screen content within the frame boundary, sized appropriately for their standard monitor.
7. User can continue to reposition or resize the frame during the call; changes are reflected in the Teams share in real time.
8. When done, user stops sharing in Teams and closes Share Frame.

### Secondary: Quick toggle during call
1. User has Share Frame already running but hidden/minimized.
2. User presses a hotkey or clicks the system tray icon to show the frame.
3. User positions the frame and shares it in Teams.
4. After the share, user hides the frame again via hotkey or tray icon.

## Core Entities

| Entity | Key Attributes | Relationships |
|--------|---------------|---------------|
| Frame Window | position (x, y), size (width, height), visibility state, border style | Owns the capture region |
| Capture Region | screen coordinates matching frame bounds, pixel content | Defined by Frame Window bounds; content is relayed to Teams via the window surface |

## Technical Preferences

- **Platform**: Windows desktop (Win32)
- **Tech Stack**: Rust, using the Windows API (windows-rs crate). Minimal dependencies.
- **Database**: None required
- **Auth**: None
- **Integrations**: Microsoft Teams (via standard Win32 window sharing — no Teams API integration needed, just appearing as a normal shareable window)

## Technical Notes

The key technical challenge is making the window both transparent (so the user sees through it) and capturable by Teams (so remote viewers see the underlying screen content). Approaches to consider:

- The window must perform screen capture of the region behind it and paint that captured content onto its own surface. A fully transparent window would show as blank/black when shared in Teams because Teams captures the window's own rendered content, not what's visually behind it.
- Use the Windows Graphics Capture API (via `windows-rs`) or BitBlt/PrintWindow to continuously capture the screen region corresponding to the frame's position and render it onto the window surface.
- The frame border should be rendered on top of the captured content so the user can see the boundary, but ideally the border should be excluded or minimal in the Teams share (or at least unobtrusive).
- The capture loop should run at a reasonable frame rate (15-30 FPS) to be smooth in Teams without excessive CPU usage.
- The window should use `WS_EX_TOOLWINDOW` or similar extended styles to avoid appearing in the taskbar (the system tray icon handles access instead), or appear as a normal window — either approach is acceptable for MVP.

## Constraints & Scope

### In Scope
- Windows 10/11 support
- Single-monitor frame positioning (frame should be constrained to the bounds of the monitor it is on)
- Freeform resize only
- Standard Win32 window that Teams can capture

### Out of Scope
- Cross-monitor frame spanning
- Linux or macOS support
- Teams API integration or Teams app/extension
- Audio capture
- Recording or saving captured content
- Streaming to anything other than Teams window sharing
- Preset resolution snapping (P3, deferred)
- System tray and hotkeys (P2, deferred from MVP)

### Non-Functional Requirements
- **Performance**: The capture and render loop should consume minimal CPU (target < 5% on a modern machine). Frame rate of 15-30 FPS is sufficient.
- **Responsiveness**: Frame dragging and resizing should feel native and smooth with no perceptible lag.
- **Binary size**: Keep the binary small by minimizing dependencies. Ideally a single `.exe` with no installer required.

## Quality Expectations

- **Testing**: Post-implementation manual testing is fine. Automated unit tests are not a priority for a UI-heavy Win32 app, but any non-trivial logic (coordinate math, capture region calculation) should be testable.
- **Deployment**: Single `.exe` binary, no installer. User downloads and runs.
- **UX Style**: Minimal and modern. Thin border (e.g., 3-4px) in a neutral color (dark gray or semi-transparent white). Slightly more prominent corner grips for resize affordance. No title bar — the entire frame border area acts as the drag handle. Clean and unobtrusive.
