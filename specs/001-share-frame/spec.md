# Feature Specification: Share Frame

**Feature Branch**: `001-share-frame`  
**Created**: April 24, 2026  
**Status**: Draft  
**Input**: User description: "Share Frame - A lightweight Windows desktop application for ultrawide monitor users to share a specific screen region during Microsoft Teams calls via a transparent, resizable, moveable frame window."

## User Scenarios & Testing

### User Story 1 - Share a Screen Region in Teams (Priority: P1)

A user with an ultrawide monitor launches Share Frame before or during a Teams call. A transparent frame with a visible border appears centered on their primary monitor. The user drags the frame over the content they want to share, resizes it to capture exactly the desired region, then shares the "Share Frame" window in Teams. Remote participants see only the content within the frame boundary at an appropriate size for standard monitors. The user can reposition or resize the frame during the call with changes reflected in real time.

**Why this priority**: This is the core value proposition — without this, the application has no purpose. It solves the fundamental problem of ultrawide monitor users overwhelming standard-monitor viewers.

**Independent Test**: Can be fully tested by launching the application, positioning the frame over content, sharing the window in a Teams call, and verifying remote participants see only the framed region at a usable size.

**Acceptance Scenarios**:

1. **Given** the application is not running, **When** the user launches Share Frame, **Then** a transparent frame window with a visible border appears centered on the primary monitor at a default size no larger than 1920×1080.
2. **Given** the frame is visible on screen, **When** the user clicks and drags the frame border area, **Then** the frame moves smoothly to follow the cursor and can be repositioned anywhere on the current monitor.
3. **Given** the frame is visible on screen, **When** the user drags a border edge or corner handle, **Then** the frame resizes smoothly in the direction of the drag.
4. **Given** the frame is positioned over screen content, **When** the user opens Teams' "Share content → Window" picker, **Then** "Share Frame" appears as a selectable window in the list.
5. **Given** the user is sharing "Share Frame" in Teams, **When** remote participants view the shared content, **Then** they see the screen content within the frame boundary (not a transparent or black rectangle).
6. **Given** the frame is being shared in Teams, **When** the user moves or resizes the frame, **Then** the shared content updates in real time to reflect the new position and size.
7. **Given** the frame is being shared in Teams, **When** the user closes Share Frame, **Then** the Teams share session ends gracefully.

---

### User Story 2 - Quick Toggle Frame Visibility (Priority: P2)

A user who has Share Frame already running wants to quickly show or hide the frame without closing the application. They use the system tray icon or a keyboard shortcut to toggle the frame's visibility, allowing them to use it on demand during calls without it cluttering the screen when not needed.

**Why this priority**: Enhances usability for repeated use during a workday but is not required for the core sharing functionality. The user can still close and relaunch the application as a workaround.

**Independent Test**: Can be tested by launching the application, hiding the frame via the system tray icon or hotkey, confirming the frame disappears, then showing it again and confirming it reappears at its previous position and size.

**Acceptance Scenarios**:

1. **Given** the application is running and the frame is visible, **When** the user clicks the system tray icon and selects "Hide", **Then** the frame disappears from the screen but the application remains running in the system tray.
2. **Given** the application is running and the frame is hidden, **When** the user clicks the system tray icon and selects "Show", **Then** the frame reappears at its previous position and size.
3. **Given** the application is running, **When** the user presses the global hotkey (Ctrl+Shift+F), **Then** the frame toggles between visible and hidden states.
4. **Given** the application is running, **When** the user right-clicks the system tray icon and selects "Exit", **Then** the application closes completely.

---

### User Story 3 - Snap Frame to Preset Resolution (Priority: P3)

A user wants to quickly set the frame to a standard resolution (e.g., 1920×1080, 2560×1440, or 1280×720) instead of manually resizing. They right-click the frame border to access a context menu with preset resolution options, and selecting one instantly resizes the frame to that resolution while keeping it centered at its current position.

**Why this priority**: Convenience feature that saves time but is not essential — users can manually drag the frame to the approximate size they need.

**Independent Test**: Can be tested by right-clicking the frame, selecting a preset resolution from the context menu, and verifying the frame resizes to the exact specified dimensions.

**Acceptance Scenarios**:

1. **Given** the frame is visible on screen, **When** the user right-clicks the frame border, **Then** a context menu appears with preset resolution options (1920×1080, 2560×1440, 1280×720).
2. **Given** the context menu is visible, **When** the user selects a preset resolution, **Then** the frame resizes to the selected dimensions, centered at its current midpoint (or adjusted to stay within monitor bounds).

---

### User Story 4 - Remember Frame Position Across Restarts (Priority: P3)

A user who has positioned and sized the frame to their preference closes the application and relaunches it later. The frame reappears at the same position and size it was in when last closed, saving the user from repositioning it every session.

**Why this priority**: Quality-of-life improvement for repeated use but not essential — the default launch size and manual repositioning provide an acceptable baseline experience.

**Independent Test**: Can be tested by positioning the frame, closing the application, relaunching it, and verifying the frame appears at the saved position and size.

**Acceptance Scenarios**:

1. **Given** the frame is positioned and sized by the user, **When** the user closes the application, **Then** the frame's position and size are persisted to local storage.
2. **Given** saved position/size data exists, **When** the user relaunches Share Frame, **Then** the frame appears at the previously saved position and size.
3. **Given** saved position/size data references a location outside current monitor bounds, **When** the user relaunches Share Frame, **Then** the frame falls back to the default centered position at default size.

---

### Edge Cases

- What happens when the user drags the frame partially off-screen? The frame should be constrained to the bounds of the monitor it is on, preventing it from being moved to an inaccessible position.
- What happens when the monitor resolution changes while the frame is running (e.g., connecting/disconnecting a monitor)? The frame should detect the change and reposition itself within the new monitor bounds if it would otherwise be off-screen.
- What happens when the user resizes the frame to an extremely small size? A minimum frame size should be enforced (e.g., 200×150 pixels) to prevent the frame from becoming unusable.
- What happens when the user resizes the frame larger than the monitor? The frame should be constrained to not exceed the current monitor's dimensions.
- What happens if the screen capture mechanism fails or returns blank frames? The application should display a visual indicator (e.g., a colored background or error icon within the frame) rather than silently showing black/blank content.
- What happens when another application is running in full-screen mode on the same monitor? The frame should not interfere with full-screen applications and should maintain standard window z-order behavior.
- What happens when the display is running at a non-100% DPI scaling level (e.g., 125%, 150%, 175%)? The application must be DPI-aware and use logical (scaled) pixels for all positioning and sizing. All pixel dimensions in this spec (200×150 minimum, 1920×1080 default, preset resolutions) refer to logical pixels. The capture region coordinates must be correctly mapped to physical pixels for screen capture.
- What happens if the user launches a second instance of Share Frame while one is already running? The application should detect the existing instance, bring its window to the foreground, and exit the second instance silently.

## Requirements

### Functional Requirements

- **FR-001**: The application MUST create a borderless window that displays captured screen content from the region directly behind the frame's position on screen.
- **FR-002**: The application MUST render a visible border (3 pixels, neutral dark gray color) around the frame with more prominent corner grips (6 pixels) to provide resize affordance.
- **FR-003**: The user MUST be able to drag the frame border area to reposition the frame anywhere within the bounds of the current monitor.
- **FR-004**: The user MUST be able to resize the frame by dragging border edges or corner handles in any direction.
- **FR-005**: The frame window MUST register as a standard window with the title "Share Frame" so it appears in Teams' window sharing picker.
- **FR-006**: When shared via Teams, the window MUST display the captured screen content (not a transparent or blank rectangle) to remote participants.
- **FR-007**: The screen capture MUST update continuously at 15-30 frames per second to provide smooth content for Teams sharing.
- **FR-008**: On launch, the frame MUST appear centered on the primary monitor at a default size of 1920×1080 or 75% of monitor width (whichever is smaller), maintaining a 16:9 aspect ratio for the default size.
- **FR-009**: The frame MUST enforce a minimum size (no smaller than 200×150 pixels) to prevent the window from becoming unusable.
- **FR-010**: The frame MUST be constrained to the bounds of the monitor it is on, preventing repositioning or resizing beyond monitor edges.
- **FR-010a**: The application MUST be DPI-aware and correctly handle displays at non-100% scaling. All user-facing dimensions (default size, minimum size, preset resolutions) are in logical pixels.
- **FR-010b**: The application MUST detect an already-running instance and bring the existing window to the foreground instead of launching a duplicate.
- **FR-011**: The application MUST be a single executable file (.exe) that runs without installation.
- **FR-012** *(P2)*: The application MUST provide a system tray icon with options to show/hide the frame and exit the application.
- **FR-013** *(P2)*: The application MUST support a global hotkey (Ctrl+Shift+F) to toggle frame visibility.
- **FR-014** *(P3)*: The application MUST provide a right-click context menu on the frame with preset resolution options (1920×1080, 2560×1440, 1280×720).
- **FR-015** *(P3)*: The application MUST persist the frame's last position and size and restore them on next launch.

### Non-Functional Requirements

- **NFR-001**: The application MUST NOT require elevated privileges (no UAC prompt, no administrator rights).
- **NFR-002**: The application MUST NOT open network sockets, listen on ports, or make any outbound network requests.
- **NFR-003**: The capture and render loop MUST consume less than 5% CPU on a modern machine during steady-state operation.
- **NFR-004**: Frame dragging and resizing MUST feel native and smooth with sub-16ms response to user input events.
- **NFR-005**: The application MUST launch and display the frame in under 2 seconds.
- **NFR-006**: The application binary MUST be a single file under 10 MB with no external dependencies.

### Key Entities

- **Frame Window**: The main application window. Key attributes: position (x, y coordinates on screen), size (width, height in pixels), visibility state (visible/hidden), border style (thin neutral border with corner grips). The frame window owns and defines the capture region.
- **Capture Region**: The rectangular area of the screen that maps to the frame's position and size. Key attributes: screen coordinates matching frame bounds, continuously updated pixel content. The capture region content is rendered onto the frame window's surface so Teams can capture it.

## Success Criteria

### Measurable Outcomes

- **SC-001**: Users can go from launching the application to sharing a screen region in Teams in under 30 seconds on first use.
- **SC-002**: 12pt text in the shared region remains legible at the default frame size when viewed on a 1920×1080 remote display.
- **SC-003**: Frame repositioning and resizing feel native and smooth with no perceptible lag (indistinguishable from dragging a normal window).
- **SC-004**: The application consumes less than 5% CPU on a modern machine during continuous screen capture and sharing.
- **SC-005**: The shared content updates smoothly at 15-30 frames per second with no visible tearing or flickering for remote viewers.
- **SC-006**: The application binary is a single file under 10 MB with no external dependencies or installation required.
- **SC-007**: The application launches and displays the frame in under 2 seconds.

## Assumptions

- Users are running Windows 10 (version 1903 or later) or Windows 11, which support the Windows Graphics Capture API.
- Users have Microsoft Teams desktop application installed (not the browser version) for window sharing.
- The application only needs to support a single frame instance at a time (not multiple simultaneous frames).
- The frame operates on a single monitor — cross-monitor spanning is out of scope.
- No audio capture is needed; the application only handles visual screen content.
- No recording, saving, or streaming of captured content is needed beyond the live Teams window share.
- The system tray and hotkey features (P2) and preset snapping/persistence (P3) are deferred from the initial MVP release.
- The frame border is rendered on top of captured content and will be visible to remote participants, which is acceptable as long as it is thin and unobtrusive.
- Standard Win32 window sharing in Teams is sufficient — no Teams API integration or Teams app/extension is needed.
