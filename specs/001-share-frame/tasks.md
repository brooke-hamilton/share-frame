# Tasks: Share Frame

**Input**: Design documents from `/specs/001-share-frame/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/module-interfaces.md, contracts/win32-messages.md, quickstart.md

**Tests**: Unit tests included for `geometry.rs` pure functions per plan.md ("cargo test for unit-testable logic"). No integration test framework — Win32 UI behavior verified manually per quickstart.md.

**Organization**: MVP scope is P1 only (User Story 1). P2 (system tray, hotkey) and P3 (presets, persistence) are deferred.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1)
- Include exact file paths in descriptions

## Path Conventions

Single binary Rust project at repository root:
- `Cargo.toml` — package manifest
- `src/main.rs` — entry point + module declarations
- `src/window.rs` — window creation, WndProc
- `src/capture.rs` — BitBlt screen capture
- `src/render.rs` — WM_PAINT, border drawing
- `src/geometry.rs` — coordinate math, DPI, unit tests

---

## Phase 1: Setup

**Purpose**: Create Rust project structure and configure dependencies

- [X] T001 Create Cargo.toml with package name `share-frame`, edition 2021, `windows` crate dependency (v0.58+) with feature flags `Win32_Foundation`, `Win32_UI_WindowsAndMessaging`, `Win32_Graphics_Gdi`, `Win32_System_LibraryLoader`, `Win32_System_Threading`, `Win32_UI_HiDpi`, and `[profile.release]` with `strip = true` and `lto = true` in Cargo.toml
- [X] T002 Create module structure: src/main.rs with `mod window; mod capture; mod render; mod geometry;` declarations and empty `fn main() {}`, plus empty module files src/window.rs, src/capture.rs, src/render.rs, src/geometry.rs

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Implement geometry types, constants, and pure computation functions that all other modules depend on

**⚠️ CRITICAL**: No User Story 1 work can begin until this phase is complete — window, capture, and render modules all depend on geometry types and functions

- [X] T003 Implement geometry module in src/geometry.rs: define `Point { x: i32, y: i32 }`, `Size { width: i32, height: i32 }`, `Rect { left, top, right, bottom: i32 }` structs (public, Copy, Clone, Debug, PartialEq); define `BorderStyle` constants (`BORDER_WIDTH: i32 = 3`, `GRIP_SIZE: i32 = 6`, `BORDER_COLOR: u32 = RGB(100,100,100)`, `HIT_TEST_MARGIN: i32 = 8`, `MIN_WIDTH: i32 = 200`, `MIN_HEIGHT: i32 = 150`); implement pure functions `default_size(monitor_width, monitor_height) -> Size` (returns min(1920, monitor_width*75/100) with 16:9 ratio), `centered_position(window_size, work_area) -> Point`, `hit_test(cursor, window_rect, margin, grip) -> i32` (returns HTCAPTION/HTLEFT/HTRIGHT/HTTOP/HTBOTTOM/HTTOPLEFT/HTTOPRIGHT/HTBOTTOMLEFT/HTBOTTOMRIGHT per contracts/win32-messages.md hit test regions), `constrain_size(rect, min_width, min_height, edge)`, `constrain_position(rect, work_area)`, `logical_to_physical(logical, dpi) -> i32`, `physical_to_logical(physical, dpi) -> i32`; implement Win32-dependent function `get_monitor_work_area(hwnd) -> Rect` using `MonitorFromWindow` + `GetMonitorInfoW`
- [X] T004 Implement `#[cfg(test)] mod tests` in src/geometry.rs with unit tests for: `default_size` (monitor wider than 2560 returns 1920×1080; small monitor returns 75% width with 16:9 ratio), `centered_position` (window centered in work area), `hit_test` (returns correct HT* values for all 9 regions: 4 corners, 4 edges, interior), `constrain_size` (enforces 200×150 minimum), `constrain_position` (clamps to work area bounds), `logical_to_physical` and `physical_to_logical` (round-trip at 96, 120, 144 DPI)

**Checkpoint**: `cargo test` passes — geometry foundation ready, user story implementation can begin

---

## Phase 3: User Story 1 — Share a Screen Region in Teams (Priority: P1) 🎯 MVP

**Goal**: User launches Share Frame, a borderless window appears centered on the primary monitor. User drags/resizes the frame over content, shares "Share Frame" window in Teams, and remote participants see the framed screen region updating in real time.

**Independent Test**: Launch application → position frame over content → share "Share Frame" window in Teams → verify remote participants see only the framed region at usable size → move/resize frame during sharing → verify updates reflect in real time.

**Covers**: FR-001 (borderless capture window), FR-002 (border + grips), FR-003 (drag to move), FR-004 (resize), FR-005 (Teams discoverability), FR-006 (captured content visible in Teams), FR-007 (15–30 FPS), FR-008 (default centered size), FR-009 (min size), FR-010 (monitor bounds), FR-010a (DPI awareness), FR-010b (single instance), FR-011 (single .exe)

### Implementation for User Story 1

- [X] T005 [US1] Implement capture module in src/capture.rs: define `CaptureState` struct (timer_id: usize, memory_dc: CreatedHDC, bitmap: HBITMAP, width: i32, height: i32 — stored in physical pixels, frame_interval_ms: u32 = 33, capture_ok: bool); implement `pub fn init(hwnd: HWND, width: i32, height: i32, dpi: u32) -> CaptureState` — convert logical width/height to physical pixels via `geometry::logical_to_physical` then CreateCompatibleDC, CreateCompatibleBitmap at physical size, SetTimer with ~33ms interval; implement `pub fn capture_frame(hwnd: HWND, state: &mut CaptureState) -> bool` — SetLayeredWindowAttributes alpha=0, GetDC(None), get window rect via GetWindowRect (returns physical pixels on DPI-aware window), BitBlt from desktop DC to memory_dc using physical pixel coordinates and state's physical width/height, ReleaseDC, SetLayeredWindowAttributes alpha=255, InvalidateRect — per capture-render pipeline in contracts/win32-messages.md, on failure set state.capture_ok = false; implement `pub fn resize(state: &mut CaptureState, width: i32, height: i32, dpi: u32)` — convert logical to physical via `geometry::logical_to_physical`, delete old bitmap, CreateCompatibleBitmap with new physical size, select into memory_dc; implement `pub fn cleanup(state: &mut CaptureState)` (KillTimer, DeleteDC, DeleteObject). Note: capture.rs depends on geometry.rs for `logical_to_physical`
- [X] T006 [US1] Implement render module in src/render.rs: implement `pub fn paint(hwnd: HWND, state: &crate::capture::CaptureState)` — call BeginPaint, if state.capture_ok BitBlt from state.memory_dc to paint DC (using StretchBlt if physical and logical sizes differ for DPI correctness) else FillRect with dark red (RGB(139,0,0)) error background, draw 3px border rectangle in BORDER_COLOR using FrameRect or Rectangle, draw 6px corner grip squares at all four corners using FillRect, call EndPaint. Note: WM_ERASEBKGND is handled in window.rs WndProc (returns 1 directly) — not in render.rs
- [X] T007 [US1] Implement window module in src/window.rs: define `WindowState` struct holding `CaptureState`, current `dpi: u32`, and cached `work_area: geometry::Rect` (stored via SetWindowLongPtrW/GWLP_USERDATA); implement RegisterClassExW with class name `"ShareFrameClass"` and no background brush; implement CreateWindowExW with style `WS_POPUP | WS_VISIBLE`, extended style `WS_EX_APPWINDOW | WS_EX_LAYERED`, title `"Share Frame"`, using geometry::default_size and geometry::centered_position for initial placement; implement `unsafe extern "system" fn wnd_proc` dispatching: WM_CREATE → allocate WindowState + capture::init + Box into GWLP_USERDATA, WM_DESTROY → capture::cleanup + PostQuitMessage(0) + drop WindowState, WM_TIMER → capture::capture_frame, WM_PAINT → render::paint, WM_SIZE → capture::resize, WM_SIZING → geometry::constrain_size, WM_MOVING → geometry::constrain_position, WM_NCHITTEST → geometry::hit_test, WM_DPICHANGED → update DPI + reposition per suggested rect, WM_DISPLAYCHANGE → re-query monitor bounds via geometry::get_monitor_work_area + reposition if out of bounds (SetWindowPos), WM_ERASEBKGND → return LRESULT(1) directly (no delegation to render); implement `pub fn create_and_run() -> windows::core::Result<()>` (register class, calculate size/position, create window, enter GetMessage/TranslateMessage/DispatchMessage loop)
- [X] T008 [US1] Implement entry point in src/main.rs: call `SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)` before any window creation; create named mutex via `CreateMutexW` with name `"ShareFrame_SingleInstance_Mutex"` — if `GetLastError() == ERROR_ALREADY_EXISTS` then `FindWindowW("ShareFrameClass", "Share Frame")` + `SetForegroundWindow` + exit silently; otherwise call `window::create_and_run()` and handle errors via `MessageBoxW`

**Checkpoint**: `cargo build --release` succeeds. Launch share-frame.exe → frame appears centered → drag to move → resize from edges/corners → content behind frame captured at ~30 FPS → "Share Frame" visible in Teams window picker → sharing shows captured content to remote participants → second launch foregrounds existing instance → close via Alt+F4

---

## Phase 4: Polish & Cross-Cutting Concerns

**Purpose**: Verify NFRs, validate build output, and confirm end-to-end behavior

- [X] T009 Run `cargo build --release` and verify: binary at target/release/share-frame.exe is under 10 MB (NFR-006), no external DLL dependencies, `cargo test` passes all geometry unit tests
- [ ] T010 Validate quickstart.md workflow: launch share-frame.exe on Windows, confirm frame appears in under 2 seconds (NFR-005), position over content, share in Teams, verify remote participants see framed content, verify sub-16ms input response for drag/resize (NFR-004), verify CPU usage < 5% steady-state (NFR-003)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately
- **Foundational (Phase 2)**: Depends on Phase 1 — BLOCKS all user story work
- **User Story 1 (Phase 3)**: Depends on Phase 2 completion (geometry types/functions required by all modules)
- **Polish (Phase 4)**: Depends on Phase 3 completion

### Within Phase 3 (User Story 1)

```text
T005 (capture.rs)
  └─▸ T006 (render.rs)         [render uses CaptureState from capture]
        └─▸ T007 (window.rs)   [WndProc dispatches to capture + render]
              └─▸ T008 (main.rs) [main calls window::create_and_run]
```

All Phase 3 tasks are sequential — each module depends on the prior module's types/functions.

### Parallel Opportunities

- **Phase 2**: T004 (geometry tests) can be written immediately after T003 (geometry implementation), but both are in the same file so effectively sequential
- **Phase 3**: No parallelism — strict dependency chain through module interfaces
- **Phase 4**: T009 (build verification) and T010 (manual validation) can run independently

---

## Parallel Example: Phase 4

```bash
# These can run independently:
Task T009: "Run cargo build --release and verify binary size and tests"
Task T010: "Validate quickstart.md workflow on Windows"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (Cargo.toml + module stubs)
2. Complete Phase 2: Foundational (geometry module + tests)
3. Complete Phase 3: User Story 1 (capture → render → window → main)
4. **STOP and VALIDATE**: `cargo test` + manual Teams sharing test
5. Release share-frame.exe as MVP

### Deferred Work (Not in This Task List)

- **P2 — User Story 2** (Quick Toggle Visibility): System tray icon (FR-012), global hotkey Ctrl+Shift+F (FR-013)
- **P3 — User Story 3** (Snap to Preset Resolution): Right-click context menu with preset resolutions (FR-014)
- **P3 — User Story 4** (Remember Position): Persist/restore frame position and size (FR-015)

### Incremental Delivery

1. Setup + Foundational → geometry tested, project compiles ✓
2. Add capture.rs → BitBlt logic in place ✓
3. Add render.rs → paint pipeline complete ✓
4. Add window.rs → full window with all message handling ✓
5. Add main.rs → single-instance, DPI-aware entry point ✓
6. **MVP complete** — single .exe, shareable in Teams

---

## Notes

- All pixel dimensions in tasks refer to logical pixels unless explicitly noted as physical
- `unsafe` blocks are required for Win32 FFI calls via `windows-rs` — each must check return values per error handling strategy in contracts/module-interfaces.md
- The WS_EX_LAYERED + alpha toggle approach (research.md R3) is critical for avoiding capture feedback loops while keeping the window visible to Teams
- Commit after each task or logical group
- Stop at any checkpoint to validate independently
