# Cross-Platform System Tray Daemon, Multi-Window Mirrors & Settings Control Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transform `aire` into a cross-platform pure system tray daemon with startup notifications, multi-stream floating window mirrors (with per-window aspect-ratio locking and live source switching), and a dedicated reactive Settings control panel.

**Architecture:** `main()` loads persistent configuration via `dirs` + `serde_json`, triggers a native startup notification via `notify-rust`, spawns a cross-platform system tray service (`ksni` on Linux, `tray-icon` + `muda` on macOS/Windows), and enters the GPUI event loop with zero initial windows. Tray commands are received over an `async_channel` in GPUI to spawn floating `MirrorView` windows (after running the window picker on a background thread) or open/focus the `SettingsView` window, sharing a reactive `Entity<AppState>` across all windows.

**Tech Stack:** Rust, GPUI, `serde`, `serde_json`, `dirs`, `notify-rust`, `ksni` (Linux StatusNotifierItem), `tray-icon` + `muda` (macOS/Windows tray), PipeWire / `ashpd` (Linux capture).

**Spec:** `docs/superpowers/specs/2026-10-16-tray-daemon-and-settings-design.md`

## Global Constraints

- Cross-platform module structure for tray (`src/tray/`), notifications (`src/notify.rs`), config (`src/config.rs`), window management (`src/window/`), and capture (`src/capture/`).
- Pure tray daemon startup: zero windows opened on startup; native notification shown on launch.
- Settings: Target FPS (`30` / `60`), Debug FPS badge toggle (`false` by default, in separate Debug Options section), Hover transparency continuous slider (`0.05..=1.00`, default `0.35`), Animation speed (`Instant`, `Fast`, `Smooth`, `Relaxed`), Window behavior (`always_on_top` default `true`, `default_spawn_size` default `Medium`).
- Every retired `RenderImage` must be explicitly evicted from `WgpuAtlas` via `cx.drop_image(...)`.

## Review Focus

1. **Corrupted or missing `config.json` on startup**: Falls back gracefully to `AppConfig::default()` without crashing, and clamps out-of-range `hover_opacity` (`0.05..=1.0`) or invalid `target_fps` (`30` or `60`).
2. **Multiple mirrors with different aspect ratios + Settings window open simultaneously**: Per-window resize constraints (`set_window_resize_constraint_for(window_id, ...)`) ensure each mirror locks to its own source aspect ratio and the Settings window is fixed-size (`460x560`), never inheriting a mirror's aspect ratio.
3. **Cancelling the portal picker when spawning a mirror or switching source (`⇄`)**: If the user cancels the picker dialog, no empty/broken window is opened (on spawn) and the existing stream continues uninterrupted (on switch source).
4. **Closing all floating mirrors or closing the Settings window**: Only closes that specific window (`window.remove_window()`) and frees its PipeWire stream + GPU texture; `aire` remains alive in the system tray until **Quit aire** is clicked.
5. **Toggling `always_on_top` at runtime**: Immediately updates platform window rules (`kwinrulesrc` + live KWin script on KDE, `hyprctl` on Hyprland, `swaymsg` on Sway, and macOS/Windows APIs) so existing floating mirrors reflect the new state immediately.

---

### Task 1: Cross-Platform Configuration, Persistence & Startup Notification (`src/config.rs`, `src/notify.rs`)

**Files:**
- Modify: `Cargo.toml`
- Create: `src/config.rs`
- Create: `src/notify.rs`

**Interfaces:**
- Produces:
  - `AppConfig { target_fps: u32, show_fps_badge: bool, hover_opacity: f32, animation_speed: AnimationSpeed, always_on_top: bool, default_spawn_size: SpawnSize }`
  - `AnimationSpeed::{Instant, Fast, Smooth, Relaxed}` with `duration_ms(&self) -> u64`
  - `SpawnSize::{Compact, Medium, Large}` with `base_width(&self) -> f32`
  - `AppConfig::load() -> Self`, `AppConfig::save(&self) -> anyhow::Result<()>`, `AppConfig::sanitized(self) -> Self`
  - `notify::send_startup_notification()`

- [ ] **Step 1: Add cross-platform dependencies (`serde`, `serde_json`, `dirs`, `notify-rust`, `ksni`, `tray-icon`, `muda`) to `Cargo.toml`**
- [ ] **Step 2: Create `src/config.rs` with unit tests for serialization, clamping, and corrupted JSON fallback**
- [ ] **Step 3: Create `src/notify.rs` with cross-platform `notify-rust` startup notification**
- [ ] **Step 4: Run `cargo test` to verify config tests pass**

---

### Task 2: Per-Window Resize Constraints & Dynamic Always-on-Top (`patches/gpui_linux`, `src/window/`, `src/capture/`)

**Files:**
- Modify: `patches/gpui_linux/src/linux/wayland/window.rs`
- Modify: `src/window/mod.rs`
- Modify: `src/window/linux.rs`
- Modify: `src/window/macos.rs`
- Modify: `src/window/windows.rs`
- Modify: `src/capture/mod.rs`
- Modify: `src/capture/linux.rs`
- Create: `src/capture/macos.rs`
- Create: `src/capture/windows.rs`

**Interfaces:**
- Produces:
  - `window::set_always_on_top(app_id: &str, enabled: bool) -> Result<()>`
  - `window::set_window_resize_constraint_for(window_id: Option<u64>, aspect_ratio: Option<f32>, min_size: Option<(f32, f32)>, max_size: Option<(f32, f32)>)`
  - `window::remove_window_resize_constraint(window_id: u64)`
  - `WindowCapture::stop(&self)` and `WindowCapture::set_target_fps(&self, fps: u32)`

- [ ] **Step 1: Update `patches/gpui_linux/src/linux/wayland/window.rs` to key `WaylandResizeConstraint` per `AnyWindowHandle::id()` (`u64`) with a fallback default for window creation**
- [ ] **Step 2: Update `src/window/{mod,linux,macos,windows}.rs` to support dynamic `set_always_on_top(app_id, enabled)` and per-window resize constraints**
- [ ] **Step 3: Update `src/capture/{mod,linux,macos,windows}.rs` so `WindowCapture` supports `stop(&self)`, `Drop` cleanup, and dynamic atomic `set_target_fps(&self, fps: u32)`**

---

### Task 3: Cross-Platform System Tray Service (`src/tray/`)

**Files:**
- Create: `src/tray/mod.rs`
- Create: `src/tray/linux.rs`
- Create: `src/tray/desktop.rs`

**Interfaces:**
- Produces:
  - `TrayCommand::{SpawnMirror, TogglePauseAll, CloseAllMirrors, OpenSettings, Quit}`
  - `TrayHandle::update_mirror_state(&self, active_count: usize, all_paused: bool)`
  - `tray::start_system_tray() -> Result<(TrayHandle, async_channel::Receiver<TrayCommand>)>`

- [ ] **Step 1: Implement `src/tray/mod.rs` with custom vector/ARGB32 tray icon generator and `TrayCommand` channel**
- [ ] **Step 2: Implement `src/tray/linux.rs` using `ksni` (`StatusNotifierItem` D-Bus service) and `src/tray/desktop.rs` using `tray-icon` + `muda` for macOS/Windows**

---

### Task 4: Multi-Window `MirrorView` with Switch Source (`⇄`), Smooth Hover Animation & Live Config (`src/state.rs`, `src/ui/mirror.rs`)

**Files:**
- Create: `src/state.rs`
- Modify: `src/ui/mirror.rs`

**Interfaces:**
- Consumes: `AppConfig`, `WindowCapture`, `set_window_resize_constraint_for`
- Produces:
  - `AppState` shared entity holding `AppConfig`, active mirror handles, and helper callbacks (`spawn_mirror_flow`, `open_settings_window`, `close_all_mirrors`, `toggle_pause_all`)
  - Updated `MirrorView` observing `AppState` for `hover_opacity`, `animation_speed`, `show_fps_badge`, `target_fps`, plus HUD Switch Source (`⇄`) button and per-window resize constraint binding

- [ ] **Step 1: Create `src/state.rs` managing `AppConfig`, active mirror registry, and tray state synchronization**
- [ ] **Step 2: Update `src/ui/mirror.rs` to observe `AppState`, animate hover opacity according to `animation_speed`, toggle the debug FPS badge via `show_fps_badge`, support `⇄` Switch Source, and clean up only its own window on close**

---

### Task 5: Dedicated Settings Control Panel (`src/ui/settings.rs`) & Pure Tray Daemon Entrypoint (`src/main.rs`)

**Files:**
- Create: `src/ui/settings.rs`
- Modify: `src/ui/mod.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Consumes: `AppState`, `MirrorView`, `start_system_tray`, `send_startup_notification`
- Produces:
  - `SettingsView` GPUI view with interactive Hover Transparency slider, Animation Speed selector, Target FPS selector, Window Behavior controls (`Always on Top`, `Default Spawn Size`), Quick Actions (`+ New Mirror`, `Close All`), and separate `Debug Options` section (`Show FPS & Resolution Badge`)
  - Pure tray daemon `src/main.rs`

- [ ] **Step 1: Create `src/ui/settings.rs` implementing the Settings Control Panel UI**
- [ ] **Step 2: Update `src/main.rs` to run as a pure tray daemon with startup notification and tray event listener**
- [ ] **Step 3: Run `cargo test` and `cargo build --release --bin aire` and verify zero errors/warnings**
