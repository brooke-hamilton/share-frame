# Implementation Plan: Share Frame

**Branch**: `001-share-frame` | **Date**: 2026-04-24 | **Spec**: `specs/001-share-frame/spec.md`
**Input**: Feature specification from `specs/001-share-frame/spec.md`

## Summary

Share Frame is a lightweight Windows desktop application that creates a borderless window capturing and relaying the screen content behind it, enabling ultrawide monitor users to share a specific screen region during Microsoft Teams calls. Built in Rust using `windows-rs` for direct Win32 API access, the application uses BitBlt screen capture at ~30 FPS, renders captured content onto the window surface, and registers as a standard window titled "Share Frame" for Teams discoverability. The MVP covers P1 functionality: window creation, drag/resize, screen capture, rendering, DPI awareness, and single-instance enforcement.

## Technical Context

**Language/Version**: Rust (latest stable)
**Primary Dependencies**: `windows` crate (Microsoft `windows-rs`) for Win32 bindings
**Storage**: N/A for MVP (P3 persistence deferred)
**Testing**: `cargo test` for unit-testable logic (geometry, DPI math); manual verification for Win32 UI behavior
**Target Platform**: Windows 10 version 1903+, Windows 11
**Project Type**: desktop-app (single binary .exe)
**Performance Goals**: <5% CPU steady-state, 15–30 FPS screen capture, sub-16ms input response for drag/resize
**Constraints**: <10 MB binary, no network access, no UAC/admin, no external runtime dependencies
**Scale/Scope**: Single user, single window, single monitor

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Evidence |
|-----------|--------|----------|
| **I. Simplicity First** | ✅ PASS | Single binary, one dependency (`windows-rs`), no frameworks, no abstractions, no speculative features. MVP is P1 only. |
| **II. Native Performance** | ✅ PASS | Direct Win32 APIs via `windows-rs`, BitBlt capture (minimal overhead), WM_TIMER at ~30ms, sub-16ms input via native message loop. |
| **III. Platform Native** | ✅ PASS | Pure Win32 window (CreateWindowExW, WndProc, message loop), standard window title for Teams capture, GDI rendering. |
| **IV. Security by Default** | ✅ PASS | No network sockets, no file I/O in MVP, no UAC, unsafe blocks only for Win32 FFI via `windows-rs`. |
| **V. Pragmatic Quality** | ✅ PASS | `geometry.rs` is fully unit-testable (coordinate math, DPI scaling, monitor bounds). Win32 UI tested manually. Clear module boundaries (5 modules). |

**Gate result**: PASS — no violations, no justifications needed.

## Project Structure

### Documentation (this feature)

```text
specs/001-share-frame/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/           # Phase 1 output
└── tasks.md             # Phase 2 output (/speckit.tasks — NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
Cargo.toml               # Package manifest, windows-rs dependency, build profile
src/
├── main.rs              # Entry point: single-instance mutex, window creation, message loop
├── window.rs            # CreateWindowExW, WndProc, WM_NCHITTEST hit testing, resize/move
├── capture.rs           # BitBlt screen capture, frame rate timer, DC management
├── render.rs            # WM_PAINT handler, captured bitmap blitting, border/grip rendering
└── geometry.rs          # Coordinate math, DPI scaling, monitor bounds, region calculations
```

**Structure Decision**: Single-project flat module layout. Five modules with clear responsibilities: `main.rs` orchestrates startup and the message loop; `window.rs` owns window creation and the window procedure; `capture.rs` handles screen capture via BitBlt; `render.rs` paints captured content and the border overlay; `geometry.rs` encapsulates all coordinate math and DPI logic for unit testability. No `tests/` directory needed — unit tests for `geometry.rs` live inline via `#[cfg(test)]` modules per Rust convention.

## Complexity Tracking

No violations to justify. All design choices align with constitution principles.
