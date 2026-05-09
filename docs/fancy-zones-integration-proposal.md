# Share Frame ↔ Microsoft Teams ↔ FancyZones Integration Plan

A design for sharing a single FancyZones zone in a Microsoft Teams call, using the existing
[`share-frame`](https://github.com/brooke-hamilton/share-frame) tool as the capture surface and
keeping changes to PowerToys and Teams to a minimum.

This plan is the consolidated output of an iterative design discussion that began from
[microsoft/PowerToys#279](https://github.com/microsoft/PowerToys/issues/279).

---

## 1. Problem and constraints

**User need.** Ultrawide (21:9+) and multi-monitor users want to share *just one region*
of their desktop in Microsoft Teams — typically the rectangle of a FancyZones zone — so
that remote participants on 16:9 monitors see a readable, full-resolution view.

**Hard platform constraint.** Teams does **not** expose an extension point for third-party
"custom share sources." The Teams Apps SDK supports tabs, bots, and meeting side-panels,
but those cannot inject new entries into Teams' Share-Content picker. The picker only
enumerates what Windows reports: monitors (`HMONITOR`) and top-level windows (`HWND`),
plus a few Microsoft 365 first-party sources.

**Consequence.** Any solution must make the zone look to Teams like something it already
knows how to share — a window or a monitor. We pick **window** (much cheaper than a
virtual monitor / Indirect Display Driver).

---

## 2. Architecture choice: external tool, not in-process feature

The capture/proxy-window pattern is already implemented by
[`share-frame`](https://github.com/brooke-hamilton/share-frame), a small Rust app:

- Borderless `WS_POPUP` with custom `WM_NCHITTEST` for native resize/move
- `BitBlt` from desktop DC at ~30 fps
- `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` to avoid hall-of-mirrors
- Already shows up as "Share Frame" in Teams' window picker
- ~5% CPU, single .exe, no install

Reasons to prefer external over building zone-sharing into FancyZones:

| Concern | In FancyZones | External (`share-frame`) |
|---|---|---|
| New code in PowerToys | Capture lib + WinRT/D3D + Settings + Editor changes | None (this plan) |
| Build/test impact | Significant; touches schema, editor, runner | None |
| Reuse outside FancyZones | None | Anyone (Zoom, OBS, screen recorders, no-zone users) |
| Crash blast radius | Inside FancyZones process | Separate process |
| Release cadence | Tied to PowerToys monthly | Independent |
| Already exists | No | Yes |

> One real friction worth naming: `share-frame` is Rust, in a primarily C++/C# repo. As long as
> `share-frame` stays a standalone download that PowerToys merely *cooperates with*, this is fine.
> If at some point it ships inside the PowerToys installer, that is a separate build-pipeline
> conversation.

> An alternative architecture — an Indirect Display Driver presenting each zone as a virtual
> monitor — was considered and explicitly **deferred**. It requires WHQL/EV signing, MSI driver
> payload, install elevation, and a much heavier maintenance burden. Not worth it before this
> simpler approach has proven demand.

---

## 3. End-to-end shape

```
┌──────────────────────┐    custom URI scheme (one-way)     ┌─────────────────────┐
│  share-frame.exe     │ ◄───────────────────────────────── │  Teams meeting app  │
│  (external, Rust)    │                                    │  (web, in WebView2) │
│  - generic capture   │                                    └─────────────────────┘
│  - URI handler       │
│  - FancyZones-aware  │ ─── reads JSON ──┐
│  - tray menu         │                  │
└──────────────────────┘                  ▼
                                ┌────────────────────────────────────────────┐
                                │ %LOCALAPPDATA%\Microsoft\PowerToys\        │
                                │   FancyZones\                              │
                                │     applied-layouts.json                   │
                                │     custom-layouts.json                    │
                                │     default-layouts.json                   │
                                └────────────────────────────────────────────┘
```

- **`share-frame`** is the one process that touches captured pixels.
- **Teams plugin** is web content; it can only fire URIs at `share-frame`.
- **PowerToys / FancyZones** is unmodified in v1; `share-frame` reads its on-disk JSON.

---

## 4. `share-frame` becomes FancyZones-aware (no PowerToys changes)

`share-frame` reads FancyZones' on-disk state directly. No IPC, no PowerToys process
interaction. If FancyZones isn't installed, the files just aren't there → fall back to
manual frame placement.

### 4a. Where the data lives

`%LOCALAPPDATA%\Microsoft\PowerToys\FancyZones\` (resolves via Rust's `dirs::data_local_dir()`):

| File | Contains |
|---|---|
| `custom-layouts.json` | User's named layouts: canvas zones with absolute `{x,y,width,height}` rects, or grid layouts with row/column percentages |
| `applied-layouts.json` | Which layout is currently assigned to which monitor (work area) |
| `default-layouts.json` | Fallback layouts when a monitor has no explicit assignment |

Source confirmation: `src/modules/fancyzones/FancyZonesLib/FancyZonesData/CustomLayouts.h`
and `AppliedLayouts.h` in the PowerToys repo.

### 4b. v1 scope: canvas layouts only

Grid → pixel-rect math (with spacing, edge cells, DPI) is exactly the math FancyZones has unit
tests for, in `FancyZonesEditor.UnitTests` and `FancyZonesTests`. Reimplementing that in Rust is
an off-by-one bug magnet. **v1 supports only `type: "canvas"`** (absolute pixel rects, scaled to
the current work area). Grid support is a follow-up that should validate against FancyZones'
test vectors.

### 4c. Defensive parsing

`custom-layouts.json` is FancyZones' internal schema, not a contract. PowerToys may change it
between releases. Parse defensively:

- Tolerate missing optional fields with sensible defaults.
- On any parse error, fall back to manual mode silently. Never crash.
- Watch the files for changes (`notify` crate) so the menu stays in sync when the user edits a
  layout.

### 4d. Optional v2 (PowerToys side, separate, additive)

If FancyZones-awareness proves valuable, contribute a tiny PR to PowerToys: write an additional
`applied-zones.json` containing the **resolved pixel rects per monitor**, updated whenever a
layout is applied. That gives `share-frame` (and any other tool) a stable contract to read,
instead of an internal schema. v2 deprecates v1's reader. Strictly additive, no schema migration.

---

## 5. Configuration and zone identity

### 5a. What `share-frame` stores

Persist per "configured zone":

```json
{
  "layout_uuid": "{...}",
  "zone_centroid": { "x": 0.50, "y": 0.50 },
  "monitor_friendly_name": "DELL U4919DW",
  "last_known_rect": { "x": 1920, "y": 0, "width": 1920, "height": 1080 }
}
```

### 5b. Why centroid, not index

Storing a zone *index* breaks when the user edits a layout (zone reordering shifts indices).
Composite key `(layout_uuid, centroid_in_layout_basis, monitor_friendly_name)` survives:

- On lookup: find the zone in the current layout whose centroid is closest to the saved one.
- If the closest distance exceeds a threshold (e.g., 10% of layout diagonal), treat as **stale**.

### 5c. `last_known_rect` as fallback

When the saved zone is stale (layout deleted, monitor disconnected, threshold exceeded),
`last_known_rect` is strictly better than guessing. Use it silently and surface a tray balloon
suggesting the user reconfigure.

---

## 6. URI contract

### 6a. Why URI for outbound, nothing for inbound

A custom URI scheme is a **launch mechanism**, not a transport. It can carry parameters
to `share-frame` but cannot return data. That's fine — the feature does not need a return
channel. (See §9 for what was rejected.)

### 6b. Verbs

`share-frame` registers `share-frame://` and accepts:

| URI | Behavior |
|---|---|
| `share-frame://activate` | Open the frame at the configured zone. See §6c for decision tree. |
| `share-frame://activate?zone=<name>` | Open at a named zone, overriding the saved configuration for this session. |
| `share-frame://deactivate` | Close the frame. |

No heartbeat verb. (See §9.)

### 6c. Decision tree on `activate`

```
on URI activate received:
  if share-frame already running with frame open:
       if rect matches configured zone, no-op
       else move to configured zone silently
  if first run (no config saved):
       activate at default rect, open setup dialog (non-modal)
  if config saved + zone resolves to a valid rect today:
       activate at that rect silently
  if config saved + zone is stale:
       activate at last_known_rect silently,
       surface a tray balloon "Zone changed; click to reconfigure"
```

The "stale zone" path matters: a modal popping up the moment a Teams call starts is worse
than silently using the last-known rect with a non-blocking notification.

---

## 7. Teams meeting app

Built with Teams Toolkit, ships as a meeting side-panel app (web content in Teams' WebView2).

### 7a. Surface

Two buttons:
- **Start sharing this zone** — fires `share-frame://activate`.
- **Stop sharing** — fires `share-frame://deactivate`.

Optionally: a read-only label showing the configured zone name (read by the user from
`share-frame`'s tray, set in `share-frame`'s config — the plugin does not need to know).

### 7b. Lifecycle hooks

Subscribe to Teams JS SDK events:
- `meeting.leave` (or equivalent in current SDK version) → fire `share-frame://deactivate`.
- That is the only automatic deactivation signal.

### 7c. What the plugin does NOT do

- Does not heartbeat (see §9).
- Does not detect microphone state (see §10).
- Does not try to receive data from `share-frame` (see §9).
- Does not start `share-frame` silently — the user clicks "Start" intentionally.

---

## 8. Failure mode and the "Started by Teams" label

The only real failure mode in this design:

> If Teams is force-killed (or the plugin panel is closed before the call ends and `meeting.leave`
> never fires), no `deactivate` is sent. The frame stays open until the user closes it.

Why this is acceptable:

- The frame is a visible window. The user will see it.
- One click to close. No data risk — there's no call to leak into.
- Resource cost while idle is bounded (~5% CPU per `share-frame`'s README).
- Every "smarter" detection scheme considered (process watching, idle timeouts, mic detection)
  introduced a worse risk: cutting off a live screen share mid-meeting.

**Trade accepted explicitly: a rare, visible, trivially-recoverable annoyance is preferable to
any chance of cutting off the user's share mid-meeting.**

**UX mitigation.** When `share-frame` is launched via `share-frame://activate` from Teams (vs.
manually from the tray), display a subtle "Started by Teams" label in its title bar. If the user
sees that label still present after their meeting ends, the diagnosis is self-explanatory.
Cheap, no detection logic.

---

## 9. Rejected: URI heartbeats and inbound channels

### 9a. Heartbeat-via-URI was considered and rejected

Original idea: Teams plugin pings `share-frame://heartbeat` every second; `share-frame`
self-deactivates after missed pings.

Rejected for three independent reasons:

1. **URIs aren't a transport.** Each invocation does protocol-handler lookup → `CreateProcess`
   or `WM_COPYDATA` forward. ~3,600 shell handoffs per hour to communicate "still alive."
2. **Browser/WebView prompt and throttling risk.** Programmatic 1 Hz protocol invocations from
   web content are exactly the pattern Edge's anti-abuse heuristics watch for. Real risk of
   throttling, deduplication, or permission warnings.
3. **The signal is one-way and ambiguous.** Even if pings get through, missing pings doesn't
   distinguish "plugin closed" from "WebView suspended" from "JS timer paused while tab
   backgrounded."

### 9b. Inbound data channel was considered and rejected

Could the Teams plugin receive state from `share-frame` (e.g., live status, "user closed the
frame" events)?

Yes, but only via a real transport — a localhost WebSocket on `127.0.0.1:<port>` with origin
allow-list, per-install random port, per-install bearer token, loopback-only bind. That's a
nontrivial security surface and the use cases for it (live status badge inside the Teams panel,
accurate disabled-button state) are nice-to-have polish, not core feature.

**Decision: ship URI-out only; revisit inbound channel only if telemetry shows a concrete UX
need.**

---

## 10. Rejected: microphone-capture detection

Earlier rounds proposed using `IAudioSessionEnumerator` to detect "is the user in a call" as
either (a) an automatic activation trigger or (b) a deactivation safety net.

**Rejected** because mic state is not a reliable proxy for "is the user still sharing":

- Users mute themselves while presenting (extremely common).
- Conference rooms route audio through room hardware; the user's mic may be inactive during a
  live call.
- Teams Rooms, Surface Hub, and similar setups break the assumption entirely.
- Headsets that keep mic streams open in other apps create false positives.

The cost of a wrong shutdown (cutting off a live share) is much higher than the cost of an
orphan window. **No automatic detection.** Only `meeting.leave` and explicit user action close
the frame.

---

## 11. Capture sanity check

`share-frame`'s README states it uses `WDA_EXCLUDEFROMCAPTURE` to avoid hall-of-mirrors when
itself is shared.

> **Action item before standardizing this design**: confirm that on a current Teams build (which
> uses Windows.Graphics.Capture), `WDA_EXCLUDEFROMCAPTURE` does *not* cause the share-frame
> window to be excluded from Teams' capture. The flag affects whether *the OS hides this window
> from capture APIs* — applied to share-frame itself, this would be wrong. Verify what's
> actually intended in `src/capture.rs` and that it works with current Teams.

---

## 12. Implementation plan

### 12a. Inside `share-frame` (separate repo)

1. **URI scheme registration**: register `share-frame://` handler at install/first-run. Verbs:
   `activate`, `activate?zone=<name>`, `deactivate`. Single-instance dispatch.
2. **Config persistence**: `(layout_uuid, zone_centroid, monitor_friendly_name, last_known_rect)`
   in a small JSON or TOML file under `%LOCALAPPDATA%\share-frame\`.
3. **FancyZones-aware mode (v1)**:
   - Read `applied-layouts.json` + `custom-layouts.json` from `%LOCALAPPDATA%\Microsoft\PowerToys\FancyZones\`.
   - Canvas layouts only.
   - Defensive parsing; fall back to manual mode on any failure.
   - Watch files via `notify` crate.
4. **Tray submenu** "Snap to FancyZone → ..." listing zones for the current monitor.
5. **"Configure FancyZone"** picker dialog (non-modal).
6. **"Started by Teams"** title-bar label when launched via URI.
7. **Activation decision tree** as in §6c.

### 12b. Teams meeting app (separate, new project)

1. Scaffold with Teams Toolkit (TypeScript).
2. Two buttons (Start / Stop) wired to `app.openLink("share-frame://...")`.
3. Subscribe to `meeting.leave`; fire `share-frame://deactivate`.
4. Optional read-only zone-name display.
5. Distribute via sideload first; AppSource if/when adoption justifies.

### 12c. PowerToys / FancyZones

**v1: nothing.** No PR needed. `share-frame` reads existing files.

**v2 (optional, only if v1 succeeds):** small additive PR that writes a new
`applied-zones.json` containing resolved pixel rects per monitor. No schema migration of
existing files. Strictly additive.

---

## 13. Open questions to resolve before coding

1. Confirm the `WDA_EXCLUDEFROMCAPTURE` behavior on a current Teams build (§11).
2. Confirm Teams JS SDK exposes a reliable "meeting ended" event in current SDK version
   (`meeting.leave` or successor). Test across desktop Teams and `teams.live.com`.
3. Confirm `app.openLink()` of a `share-frame://` URI from a Teams meeting app works without
   per-invocation prompts after the user's first acceptance.
4. Decide whether `share-frame` should be packaged for `winget` to ease the "FancyZones is
   installed but `share-frame` isn't" first-run experience.

---

## 14. What this plan deliberately does *not* do

- No FancyZones source modifications in v1.
- No Indirect Display Driver / virtual monitor.
- No microphone-capture detection of any kind.
- No URI heartbeats.
- No localhost socket / inbound data channel.
- No automatic activation. The user clicks a button.
- No automatic deactivation other than `meeting.leave`. The user can always close the window.

Every one of these omissions is the result of a specific failure mode considered and judged
worse than the failure mode of *not* having the feature.