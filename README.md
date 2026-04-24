# Share Frame

A lightweight Windows desktop application for ultrawide monitor users who need to share a specific region of their screen during Microsoft Teams calls.

Share Frame creates a transparent, resizable, moveable frame window that appears as a shareable application in Teams. Anything visible within the frame is what remote participants see — eliminating the problem of sharing an oversized ultrawide display to viewers on standard monitors.

## The Problem

If you have an ultrawide (21:9 or wider) monitor and share your full screen in Teams, remote participants on standard 16:9 monitors see a tiny, squished view of your desktop. Share Frame solves this by letting you draw a frame around exactly the region you want to share.

## How It Works

1. Launch `share-frame.exe`
2. A borderless frame appears centered on your primary monitor (default 1920×1080)
3. Drag the frame over the content you want to share
4. Resize by dragging edges or corners
5. In Teams, click **Share content → Window** and select **"Share Frame"**
6. Remote participants see only the content within your frame

The frame captures the screen region behind it at ~30 FPS using BitBlt and paints that content onto its own window surface, so Teams can capture it as a normal window.

## Features

- **Transparent frame window** — see through to your desktop while the frame captures and relays the underlying content
- **Resizable border** — 3px border with 6px corner grips for intuitive freeform resizing
- **Drag to move** — click anywhere inside the frame to reposition it
- **Teams-shareable** — registers as a standard Win32 window titled "Share Frame" in the Teams window picker
- **DPI-aware** — per-monitor DPI v2 support for correct behavior at any scaling level
- **Single instance** — launching a second copy foregrounds the existing window
- **Minimal footprint** — single `.exe`, no installer, no runtime dependencies, < 5% CPU

## Requirements

- Windows 10 (version 1903+) or Windows 11
- Microsoft Teams desktop application (for window sharing)

## Building

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- Windows MSVC target: `rustup target add x86_64-pc-windows-msvc`

### Build

```bash
cargo build --release
```

The binary is at `target/release/share-frame.exe`. Copy it anywhere and run — no installation needed.

### Run Tests

```bash
cargo test
```

Tests cover the geometry module: coordinate math, DPI scaling, hit testing, size/position constraints.

## Architecture

```
src/
├── main.rs       # Entry point: DPI awareness, single-instance mutex, error handling
├── window.rs     # Win32 window creation, WndProc message dispatch, message loop
├── capture.rs    # BitBlt screen capture with WS_EX_LAYERED alpha toggle at ~30 FPS
├── render.rs     # WM_PAINT: blit captured content, draw border and corner grips
└── geometry.rs   # Coordinate math, DPI scaling, hit testing, constraints (unit tested)
```

### Key Technical Decisions

| Decision | Approach | Why |
|----------|----------|-----|
| Screen capture | BitBlt from desktop DC | Simplest, lowest overhead for 30 FPS |
| Feedback loop avoidance | WS_EX_LAYERED + alpha toggle (0→capture→255) | DWM keeps the redirected surface so Teams always sees content |
| Window style | WS_POPUP + custom WM_NCHITTEST | Borderless with native OS resize/move behavior |
| Rendering | GDI BitBlt/StretchBlt | Zero-overhead blit, no Direct2D setup needed |
| DPI | Per-monitor DPI aware v2 | Correct scaling on all display configurations |
| Single instance | Named mutex + FindWindowW | Standard Win32 pattern |

## Usage Tips

- **Resize**: Drag any edge or corner of the frame
- **Move**: Click and drag anywhere inside the frame
- **Close**: Alt+F4 or end the process
- **Default size**: 1920×1080 or 75% of monitor width (whichever is smaller), 16:9 aspect ratio
- **Minimum size**: 200×150 pixels

## Project Structure

```
├── Cargo.toml              # Rust package manifest
├── src/                    # Application source code
├── specs/001-share-frame/  # Design documents (spec, plan, tasks, research)
├── share-frame.md          # Original feature description
└── .specify/               # Spec Kit configuration
```

## License

[MIT](LICENSE)
