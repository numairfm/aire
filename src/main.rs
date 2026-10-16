mod capture;
mod config;
mod notify;
mod state;
mod tray;
mod ui;
mod window;

use anyhow::Result;
use gpui::{App, AppContext};
use gpui_platform::application;

use crate::{
    config::AppConfig,
    notify::send_startup_notification,
    state::{AppState, APP_ID},
    tray::{start_system_tray, TrayCommand},
    window::configure_platform_window,
};

fn main() -> Result<()> {
    // 1. Load persistent configuration
    let config = AppConfig::load();

    // 2. Configure platform window rules (Always-on-top, noborder, skiptaskbar)
    if let Err(err) = configure_platform_window(APP_ID, config.always_on_top) {
        eprintln!("[WINDOW] Platform window setup warning: {:?}", err);
    }

    // 3. Start cross-platform system tray icon & menu
    let (tray_handle, tray_rx) = match start_system_tray() {
        Ok((handle, rx)) => (Some(handle), Some(rx)),
        Err(err) => {
            eprintln!("[TRAY] System tray warning: {:?}", err);
            (None, None)
        }
    };

    // 4. Send startup notification informing the user that aire is running in the tray
    send_startup_notification();
    eprintln!("[AIRE] Running in system tray. Use tray icon to mirror windows or open Settings.");

    // 5. Run GPUI application loop as a pure tray daemon (zero initial windows open)
    application().run(move |cx: &mut App| {
        let app_state = cx.new(|_| AppState::new(config, tray_handle));

        if let Some(rx) = tray_rx {
            let state_for_loop = app_state.clone();
            cx.spawn(async move |cx| {
                while let Ok(cmd) = rx.recv().await {
                    let state_entity = state_for_loop.clone();
                    let _ = cx.update(|cx| match cmd {
                        TrayCommand::SpawnMirror => {
                            AppState::spawn_mirror(state_entity, cx);
                        }
                        TrayCommand::TogglePauseAll => {
                            state_entity.update(cx, |state, cx| {
                                state.toggle_pause_all(cx);
                            });
                        }
                        TrayCommand::CloseAllMirrors => {
                            state_entity.update(cx, |state, cx| {
                                state.close_all_mirrors(cx);
                            });
                        }
                        TrayCommand::OpenSettings => {
                            AppState::open_settings(state_entity, cx);
                        }
                        TrayCommand::Quit => {
                            state_entity.update(cx, |state, cx| {
                                state.close_all_mirrors(cx);
                            });
                            cx.quit();
                        }
                    });
                }
            })
            .detach();
        }
    });

    Ok(())
}
