# Cross-Platform System Tray Daemon, Multi-Window Mirrors & Settings Control Panel — Design Spec

## 1. Overview & Goals
Transform `aire` from a single-shot startup portal launcher into an ultra-minimal, cross-platform system tray daemon (`Linux`, `macOS`, `Windows`) with:
1. **Pure Tray Daemon Startup**: Starts silently in the OS system tray with zero windows open and sends a native startup desktop notification.
2. **Multi-Stream Floating Mirrors**: Spawn as many independent floating window mirrors as desired from the tray menu (or switch an existing mirror's source via a HUD button), each locked to its own source window's aspect ratio.
3. **Dedicated GPUI Settings Window**: Opened on demand from the tray icon, persisting settings cross-platform via `dirs::config_dir()` and applying changes live across all active mirrors.

---

## 2. Cross-Platform Architecture & Crate Selection

| Subsystem | Cross-Platform Strategy & Crates |
|---|---|
| **System Tray & Menu** (`src/tray/`) | Unified `TrayEvent` channel feeding the GPUI event loop. Uses `ksni` (pure D-Bus `StatusNotifierItem`, avoiding GTK/Wayland main-loop conflicts) on Linux, and `tray-icon` + `muda` (`NSStatusItem` / `Shell_NotifyIconW`) on macOS & Windows. |
| **Desktop Notifications** (`src/notify.rs`) | `notify-rust` crate (supports Freedesktop D-Bus on Linux, `NSUserNotification`/UserNotifications on macOS, and WinRT Toast notifications on Windows). |
| **Config Persistence** (`src/config.rs`) | `dirs` + `serde` + `serde_json`. Saves to `<config_dir>/aire/config.json` (`~/.config/aire/config.json` on Linux, `~/Library/Application Support/aire/config.json` on macOS, `%APPDATA%\aire\config.json` on Windows). |
| **Window Management** (`src/window/`) | Platform modules (`linux.rs`, `macos.rs`, `windows.rs`) implementing `configure_platform_window`, `set_always_on_top(app_id, enabled)`, and per-window aspect-ratio constraints (`set_window_resize_constraint_for(window_id, ...)`). |
| **Window Capture** (`src/capture/`) | Cross-platform `WindowCapture` trait (`pause`, `resume`, `stop`, `set_target_fps`) with platform implementations (`linux.rs` via PipeWire/XDG Portal, `macos.rs` stub/ScreenCaptureKit, `windows.rs` stub/Windows.Graphics.Capture). |

---

## 3. Component Design

### 3.1 Startup & Tray Lifecycle (`src/main.rs` & `src/tray/`)
1. **Launch**:
   - Loads `AppConfig` from `<config_dir>/aire/config.json` (or initializes defaults).
   - Configures platform window rules (`always_on_top` from config).
   - Fires a native startup notification via `notify-rust`:
     - Summary: `"aire is running"`
     - Body: `"Use the system tray icon to mirror a window or open Settings."`
   - Starts the system tray service and enters `gpui_platform::application().run(...)` with **zero initial windows open**.
2. **Tray Menu Structure**:
   - **+ New Window Mirror** → sends `TrayCommand::SpawnMirror`
   - **Pause / Resume All Mirrors** → sends `TrayCommand::TogglePauseAll`
   - **Close All Mirrors** → sends `TrayCommand::CloseAllMirrors`
   - *Separator*
   - **Settings…** → sends `TrayCommand::OpenSettings`
   - *Separator*
   - **Quit aire** → sends `TrayCommand::Quit`

### 3.2 Multi-Window Mirror Management & Per-Window Aspect Ratio (`src/ui/mirror.rs`, `patches/gpui_linux`)
- **Spawning Mirrors**:
  - When `TrayCommand::SpawnMirror` is received, `aire` runs `start_window_capture(target_fps)` on a background thread so the UI thread and existing mirrors never freeze while the OS window picker is open.
  - If the user cancels the picker, nothing happens.
  - If the user selects a window, GPUI opens a new floating `MirrorView` window sized according to `config.default_spawn_size` (`Compact = 320px`, `Medium = 400px`, `Large = 560px`) and locked to that source window's aspect ratio.
- **Per-Window Aspect Ratio Locking**:
  - `gpui_linux` stores a map of `window_id -> WaylandResizeConstraint` (plus a default constraint for newly created windows) so multiple floating mirrors with different aspect ratios (e.g., `16:9` and `9:16`) resize independently without interfering with each other or with the fixed-size `SettingsView` window.
- **Switch Source Button (`⇄`) in Mirror HUD**:
  - Located in the top-right hover overlay next to Minimize (`—`) and Close (`✕`).
  - Clicking `⇄` opens the window picker on a background thread; once selected, the old `WindowCapture` is stopped (`is_running = false`), its GPU texture is evicted via `cx.drop_image(...)`, and the mirror adapts in place to the new stream and aspect ratio.
- **Clean Window Close**:
  - Clicking `✕` on a mirror window stops its `WindowCapture`, drops its GPU atlas texture, and calls `window.remove_window()` (closing only that window while keeping `aire` running in the tray).

### 3.3 Reactive Settings & Control Panel (`src/config.rs` & `src/ui/settings.rs`)
`AppConfig` is stored in a shared GPUI `Entity<AppState>` observed by all open `MirrorView` windows and the `SettingsView` window. Any change in Settings immediately notifies all mirrors and writes to `<config_dir>/aire/config.json`.

#### Settings Schema (`AppConfig`)
1. **Appearance & Animation**:
   - `hover_opacity: f32` — Continuous slider (`0.05` to `1.00`, default `0.35`). Controls stream opacity when hovered.
   - `animation_speed: AnimationSpeed` — Segmented selector:
     - `Instant` (`0ms`)
     - `Fast` (`100ms`)
     - `Smooth` (`200ms`, default)
     - `Relaxed` (`350ms`)
     Controls smooth opacity interpolation for the video stream and HUD controls on hover enter/leave.
2. **Window Behavior**:
   - `always_on_top: bool` — Toggle (`true` by default). Dynamically updates KWin / macOS / Windows floating window rules.
   - `default_spawn_size: SpawnSize` — Segmented selector: `Compact (320px)`, `Medium (400px, default)`, `Large (560px)`.
3. **Stream Performance**:
   - `target_fps: u32` — Segmented selector: `30 FPS`, `60 FPS` (default `60`). Updates an `AtomicU32` read directly by the capture loop's frame pacer.
4. **Debug Options** *(separate section at bottom)*:
   - `show_fps_badge: bool` — Toggle (`false` by default). Shows or hides the bottom-left `1920×1080 • 60 fps` overlay badge on mirrors.
5. **Quick Actions Header in Settings**:
   - Includes a **`+ New Window Mirror`** button and an active mirror count with **`Close All`** for convenience when the Settings window is open.
