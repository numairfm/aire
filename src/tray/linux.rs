use anyhow::Result;
use ksni::{blocking::TrayMethods, menu::StandardItem, Icon, MenuItem, Tray};

use super::{generate_tray_icon_argb32, TrayCommand};

struct AireTray {
    cmd_tx: async_channel::Sender<TrayCommand>,
    active_count: usize,
    all_paused: bool,
}

impl Tray for AireTray {
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "aire-mirror-daemon".into()
    }

    fn title(&self) -> String {
        if self.active_count > 0 {
            format!("aire ({} active)", self.active_count)
        } else {
            "aire".into()
        }
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![Icon {
            width: 32,
            height: 32,
            data: generate_tray_icon_argb32(32),
        }]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.cmd_tx.try_send(TrayCommand::OpenSettings);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items: Vec<MenuItem<Self>> = vec![StandardItem {
            label: "+ New Window Mirror…".into(),
            activate: Box::new(|this: &mut Self| {
                let _ = this.cmd_tx.try_send(TrayCommand::SpawnMirror);
            }),
            ..Default::default()
        }
        .into()];

        if self.active_count > 0 {
            let pause_label = if self.all_paused {
                format!("Resume All Mirrors ({})", self.active_count)
            } else {
                format!("Pause All Mirrors ({})", self.active_count)
            };
            items.push(
                StandardItem {
                    label: pause_label,
                    activate: Box::new(|this: &mut Self| {
                        let _ = this.cmd_tx.try_send(TrayCommand::TogglePauseAll);
                    }),
                    ..Default::default()
                }
                .into(),
            );
            items.push(
                StandardItem {
                    label: format!("Close All Mirrors ({})", self.active_count),
                    activate: Box::new(|this: &mut Self| {
                        let _ = this.cmd_tx.try_send(TrayCommand::CloseAllMirrors);
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }

        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: "Settings & Control Panel…".into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.cmd_tx.try_send(TrayCommand::OpenSettings);
                }),
                ..Default::default()
            }
            .into(),
        );
        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: "Quit aire".into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.cmd_tx.try_send(TrayCommand::Quit);
                }),
                ..Default::default()
            }
            .into(),
        );

        items
    }
}

pub struct LinuxTrayHandle {
    handle: ksni::blocking::Handle<AireTray>,
}

impl LinuxTrayHandle {
    pub fn update_mirror_state(&self, active_count: usize, all_paused: bool) {
        let handle = self.handle.clone();
        std::thread::spawn(move || {
            let _ = handle.update(|tray| {
                tray.active_count = active_count;
                tray.all_paused = all_paused;
            });
        });
    }
}

pub fn start_linux_tray(cmd_tx: async_channel::Sender<TrayCommand>) -> Result<LinuxTrayHandle> {
    let tray = AireTray {
        cmd_tx,
        active_count: 0,
        all_paused: false,
    };
    let handle = tray
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn StatusNotifierItem tray: {e}"))?;
    Ok(LinuxTrayHandle { handle })
}
