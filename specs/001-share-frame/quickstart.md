# Quickstart: Share Frame

**Feature**: Share Frame | **Date**: 2026-04-24

## Prerequisites

- **Rust toolchain**: Install via [rustup](https://rustup.rs/) (latest stable, MSVC target)
  ```
  rustup default stable-x86_64-pc-windows-msvc
  ```
- **Windows 10 version 1903+** or Windows 11
- **Microsoft Teams** desktop application (for testing window sharing)

## Build

```bash
# Debug build
cargo build

# Release build (optimized, stripped)
cargo build --release
```

The output binary is at:
- Debug: `target/debug/share-frame.exe`
- Release: `target/release/share-frame.exe`

## Run

```bash
# Run directly
cargo run

# Or run the built binary
./target/release/share-frame.exe
```

The application:
1. Opens a borderless frame window centered on the primary monitor
2. Continuously captures the screen content behind the frame at ~30 FPS
3. Renders the captured content onto the window surface with a thin border

## Test

```bash
# Run unit tests (geometry module)
cargo test
```

Unit tests cover: default size calculation, centered position, hit testing, size/position constraints, DPI conversion.

## Use with Teams

1. Launch `share-frame.exe`
2. Position and resize the frame over the content you want to share
3. In Teams: **Share content** → **Window** → select **"Share Frame"**
4. Remote participants see the screen content within the frame
5. Close the window or press Alt+F4 to exit

## Project Structure

```
Cargo.toml          # Package manifest
src/
├── main.rs         # Entry point, single-instance check, message loop
├── window.rs       # Window creation, WndProc, hit testing
├── capture.rs      # BitBlt screen capture, timer management
├── render.rs       # WM_PAINT handler, border drawing
└── geometry.rs     # Coordinate math, DPI scaling (unit-tested)
```
