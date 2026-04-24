<!--
  Sync Impact Report
  ===================
  Version change: N/A → 1.0.0 (initial ratification)
  Modified principles: None (initial creation)
  Added sections:
    - Core Principles (5): Simplicity First, Native Performance,
      Platform Native, Security by Default, Pragmatic Quality
    - Additional Constraints
    - Development Workflow
    - Governance
  Removed sections: None
  Templates requiring updates:
    - .specify/templates/plan-template.md ✅ aligned (Constitution Check
      section is generic; principles apply at plan time)
    - .specify/templates/spec-template.md ✅ aligned (no constitution-
      specific sections requiring change)
    - .specify/templates/tasks-template.md ✅ aligned (task categorization
      is generic; no principle-driven task types to add)
  Follow-up TODOs: None
-->

# Share Frame Constitution

## Core Principles

### I. Simplicity First

- Every feature MUST justify its existence against the current scope.
  YAGNI — do not build speculative functionality.
- The application MUST ship as a single `.exe` binary with no
  installer, no runtime dependencies, and no external frameworks.
- Minimal dependencies: only `windows-rs` and essential Rust crates.
  Each new dependency MUST be justified.
- Avoid unnecessary abstractions. Prefer direct, readable code over
  layers of indirection.
- When trade-offs arise between simplicity and other concerns,
  simplicity wins unless native performance is directly at stake.

### II. Native Performance

- CPU usage MUST remain below 5% during steady-state operation
  (frame displayed, no active resize/drag).
- Screen capture MUST sustain 15–30 FPS for the framed region.
- Frame dragging and resizing MUST feel native and smooth with no
  perceptible lag — target sub-16ms response for user input events.
- Use Win32 APIs directly via `windows-rs`. Do not introduce
  intermediate abstraction layers that add overhead.
- Binary size MUST be kept small; prefer static linking and strip
  unused symbols in release builds.

### III. Platform Native

- Share Frame is a pure Win32 application built in Rust using the
  `windows-rs` crate. No cross-platform abstraction layers.
- Target Windows 10 and Windows 11 only.
- The application window MUST register as a standard Win32 window
  so it appears in the Microsoft Teams "Share content → Window"
  picker and is capturable by Teams.
- Use native Win32 message loop, window procedures, and GDI/Direct
  Composition APIs. Do not wrap them in platform-agnostic facades.

### IV. Security by Default

- Minimal attack surface: the application MUST NOT open network
  sockets, listen on ports, or make outbound network requests.
- File I/O is limited to optional settings persistence (window
  position/size). No other file system writes are permitted.
- The application MUST NOT require elevated privileges (no admin
  or UAC prompt).
- Use safe Rust by default. Unsafe blocks are permitted only for
  direct Win32 FFI calls via `windows-rs` and MUST be clearly
  documented and minimally scoped.

### V. Pragmatic Quality

- Manual testing is acceptable for UI-heavy Win32 interaction code
  (window creation, drag/resize, rendering).
- Non-trivial logic — coordinate math, capture region calculation,
  monitor geometry, DPI scaling — MUST be structured for unit
  testing and covered by `cargo test`.
- Code MUST maintain clear module boundaries: separate window
  management, capture logic, and input handling into distinct
  modules.
- Code MUST be clean and readable. Favor explicit over clever.

## Additional Constraints

- **Tech stack**: Rust + `windows-rs` crate. No other UI frameworks.
- **No database**: No persistent storage beyond optional flat-file
  settings (e.g., JSON or TOML for window position/size).
- **No auth**: No authentication or authorization mechanisms.
- **No external services**: No network integrations of any kind.
- **Single-monitor support only**: Multi-monitor is explicitly out
  of scope.
- **Binary output**: `cargo build --release` MUST produce a single
  self-contained `.exe`.

## Development Workflow

- **Branching**: Feature branches for all development work. Merge
  to main when complete.
- **Build**: `cargo build` for debug, `cargo build --release` for
  production.
- **Test**: `cargo test` for all unit-testable logic. Manual
  verification for Win32 UI behavior.
- **Output**: Single binary via `cargo build --release`.
- **Code review**: All changes MUST be reviewed against this
  constitution before merge.

## Governance

This constitution is the authoritative guide for all Share Frame
development decisions. When principles conflict, priority order is:

1. Simplicity First
2. Native Performance
3. Platform Native
4. Security by Default
5. Pragmatic Quality

Amendments to this constitution require:

- A documented rationale for the change.
- Version increment following semantic versioning:
  - MAJOR: Principle removal or incompatible redefinition.
  - MINOR: New principle or materially expanded guidance.
  - PATCH: Clarifications, wording, or non-semantic refinements.
- Update of the `Last Amended` date.

All pull requests and code reviews MUST verify compliance with
these principles. Deviations MUST be justified in the PR
description.

**Version**: 1.0.0 | **Ratified**: 2026-04-24 | **Last Amended**: 2026-04-24
