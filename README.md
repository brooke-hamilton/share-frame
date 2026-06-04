# <img src="assets/icons/logo.svg" width="32" alt="Share Frame logo"/> Share Frame

**Share exactly what you mean.**
A lightweight screen-region sharing tool for ultrawide monitor users on Microsoft Teams.

[Installation](#installation) •
[How It Works](#how-it-works) •
[Features](#features) •
[Build](#building-from-source) •
[License](#license)

---

## The Problem

Ultrawide (21:9+) monitors are great for productivity — but terrible for screen sharing. When you share your full display in Teams, remote participants on standard 16:9 monitors see a tiny, letterboxed view that's impossible to read.

**Share Frame** fixes this. It creates a resizable window that captures only the region you frame, then presents it as a normal shareable window in Teams. Remote viewers see exactly what you want them to see, at full resolution.

## Installation

### Download a release

Grab the latest prebuilt binary from the [Releases](https://github.com/brooke-hamilton/share-frame/releases) page. Builds are provided for all Windows architectures:

| Architecture | Asset |
|---|---|
| x64 (most PCs) | `share-frame-<version>-x64.zip` |
| ARM64 (Surface / Snapdragon) | `share-frame-<version>-arm64.zip` |
| x86 (32-bit) | `share-frame-<version>-x86.zip` |

Each archive contains `share-frame.exe` and the license — copy the executable anywhere and run. No installer or runtime dependencies needed. A matching `.sha256` file is published for each archive so you can verify the download.

### Build from source

Requires [Rust](https://rustup.rs/):

```sh
git clone https://github.com/brooke-hamilton/share-frame.git
cd share-frame
cargo build --release
```

Output: `target/release/share-frame.exe` — copy it anywhere and run. No installer or runtime dependencies needed.

## How It Works

1. **Launch** `share-frame.exe` — a frame window appears centered on your primary monitor
2. **Position** the frame over the area you want to share (drag to move, edges/corners to resize)
3. **Place your application windows on top** of Share Frame — the apps you want to demo go in front so you can interact with them normally
4. **Share** in Teams → *Share content* → *Window* → select **"Share Frame"**
5. **Done** — remote participants see only what's inside the frame, updated in real time

Share Frame captures the screen region behind it at ~30 FPS and paints that content onto its own window surface. To Teams, it looks like any other application window. Your other windows sit on top of Share Frame so you can click and type in them as usual.

## Features

| Feature | Details |
|---------|---------|
| **Transparent capture** | See through to your desktop — the frame relays the content beneath it |
| **Resizable** | Drag edges or 6px corner grips to any size (min 200×150) |
| **Drag to move** | Click anywhere inside the frame to reposition |
| **Teams-ready** | Shows up as "Share Frame" in the window picker |
| **Adaptive theme** | Follows Windows dark/light mode and accent color |
| **DPI-aware** | Per-monitor DPI v2 — correct at any scaling level |
| **Single instance** | Second launch foregrounds the existing window |
| **Zero install** | Single `.exe`, no runtime dependencies, <5% CPU |

## Usage

| Action | How |
|--------|-----|
| Move | Drag anywhere inside the frame |
| Resize | Drag any edge or corner |
| Close | Click the × button or Alt+F4 |

**Default size:** 1920×1080 or 75% of monitor width (whichever is smaller), 16:9 aspect ratio.

## System Requirements

- Windows 10 version 2004+ or Windows 11
- Microsoft Teams desktop app (for window sharing)

## Building from Source

### Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- Windows MSVC target (default on Windows)

### Build

```sh
cargo build --release
```

Output: `target/release/share-frame.exe`

### Regenerate Icon

Requires [resvg](https://github.com/nicodemus26/resvg) (`cargo install resvg`):

```sh
./scripts/generate-icon.ps1
```

Converts `assets/icons/logo.svg` → `assets/icons/share-frame.ico` at 16/32/48/256px.

## Architecture

```
src/
├── main.rs       Entry point — DPI awareness, single-instance mutex
├── window.rs     Win32 window creation, WndProc message loop
├── capture.rs    BitBlt screen capture at ~30 FPS with display affinity
├── render.rs     Double-buffered painting: content, title bar, border
└── geometry.rs   Coordinate math, hit testing, size/position constraints
```

### Key Design Decisions

| Decision | Approach | Rationale |
|----------|----------|-----------|
| Screen capture | `BitBlt` from desktop DC | Lowest overhead for 30 FPS refresh |
| Self-capture avoidance | `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` | Window stays visible on display but excluded from capture APIs |
| Window style | `WS_POPUP` + custom `WM_NCHITTEST` | Borderless with native OS resize/move behavior |
| Rendering | GDI double-buffer + `BitBlt` | Zero-dependency blit, no Direct2D/D3D setup |
| DPI | Per-monitor DPI aware v2 | Correct scaling across mixed-DPI setups |
| Single instance | Named mutex + `FindWindowW` | Standard Win32 pattern |

## Contributing

Contributions are welcome! Please open an issue to discuss changes before submitting a PR.

## License

[MIT](LICENSE) © Brooke Hamilton
