use anyhow::Result;
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};

use super::{generate_tray_icon_rgba32, TrayCommand};

pub struct DesktopTrayHandle;

impl DesktopTrayHandle {
    pub fn update_mirror_state(&self, _active_count: usize, _all_paused: bool) {}
}

pub fn start_desktop_tray(cmd_tx: async_channel::Sender<TrayCommand>) -> Result<DesktopTrayHandle> {
    let menu = Menu::new();
    let item_new = MenuItem::new("+ New Window Mirror…", true, None);
    let item_pause = MenuItem::new("Pause / Resume All Mirrors", true, None);
    let item_close_all = MenuItem::new("Close All Mirrors", true, None);
    let item_settings = MenuItem::new("Settings & Control Panel…", true, None);
    let item_quit = MenuItem::new("Quit aire", true, None);

    let _ = menu.append(&item_new);
    let _ = menu.append(&item_pause);
    let _ = menu.append(&item_close_all);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&item_settings);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&item_quit);

    let id_new = item_new.id().clone();
    let id_pause = item_pause.id().clone();
    let id_close_all = item_close_all.id().clone();
    let id_settings = item_settings.id().clone();
    let id_quit = item_quit.id().clone();

    let rgba = generate_tray_icon_rgba32(32);
    let icon = Icon::from_rgba(rgba, 32, 32)?;

    let _tray_icon = Box::leak(Box::new(
        TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("aire")
            .with_icon(icon)
            .build()?,
    ));

    std::thread::spawn(move || {
        let receiver = MenuEvent::receiver();
        while let Ok(event) = receiver.recv() {
            if event.id == id_new {
                let _ = cmd_tx.try_send(TrayCommand::SpawnMirror);
            } else if event.id == id_pause {
                let _ = cmd_tx.try_send(TrayCommand::TogglePauseAll);
            } else if event.id == id_close_all {
                let _ = cmd_tx.try_send(TrayCommand::CloseAllMirrors);
            } else if event.id == id_settings {
                let _ = cmd_tx.try_send(TrayCommand::OpenSettings);
            } else if event.id == id_quit {
                let _ = cmd_tx.try_send(TrayCommand::Quit);
            }
        }
    });

    Ok(DesktopTrayHandle)
}
