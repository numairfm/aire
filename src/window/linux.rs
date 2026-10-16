use anyhow::Result;

pub struct LinuxWindowManager;

impl LinuxWindowManager {
    pub fn set_always_on_top(app_id: &str, enabled: bool) -> Result<()> {
        let app_id_owned = app_id.to_string();
        std::thread::spawn(move || {
            let _ = Self::apply_always_on_top_sync(&app_id_owned, enabled);
        });
        Ok(())
    }

    fn apply_always_on_top_sync(app_id: &str, enabled: bool) -> Result<()> {
        let is_kde = std::env::var("XDG_CURRENT_DESKTOP")
            .map(|d| d.to_lowercase().contains("kde"))
            .unwrap_or(false);

        let above_str = if enabled { "true" } else { "false" };

        if is_kde {
            let rule_id = format!("{}_mirror_rule", app_id);
            for (key, val) in [
                ("Description", "aire floating window mirror"),
                ("wmclass", app_id),
                ("wmclassmatch", "1"),
                ("above", above_str),
                ("aboverule", "2"),
                ("noborder", "true"),
                ("noborderrule", "2"),
                ("skiptaskbar", "true"),
                ("skiptaskbarrule", "2"),
                ("skipswitcher", "true"),
                ("skipswitcherrule", "2"),
            ] {
                let _ = std::process::Command::new("kwriteconfig6")
                    .args(["--file", "kwinrulesrc", "--group", &rule_id, "--key", key, val])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }

            let _ = std::process::Command::new("qdbus-qt6")
                .args(["org.kde.KWin", "/KWin", "reconfigure"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();

            let script_id = format!("{}_keepabove_daemon", app_id);
            let js_code = format!(
                r#"
                function enforce(client) {{
                    if (client && (client.resourceClass == "{app_id}" || client.resourceName == "{app_id}")) {{
                        client.keepAbove = {above_str};
                        client.noBorder = true;
                        client.skipTaskbar = true;
                        client.skipSwitcher = true;
                    }}
                }}
                workspace.windowAdded.connect(enforce);
                var list = workspace.windowList();
                for (var i = 0; i < list.length; i++) {{
                    enforce(list[i]);
                }}
                "#
            );

            let temp_path = std::env::temp_dir().join(format!("{}.js", script_id));
            if std::fs::write(&temp_path, js_code).is_ok() {
                let path_str = temp_path.to_string_lossy().to_string();
                let _ = std::process::Command::new("qdbus-qt6")
                    .args([
                        "org.kde.KWin",
                        "/Scripting",
                        "org.kde.kwin.Scripting.unloadScript",
                        &script_id,
                    ])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                let _ = std::process::Command::new("qdbus-qt6")
                    .args([
                        "org.kde.KWin",
                        "/Scripting",
                        "org.kde.kwin.Scripting.loadScript",
                        &path_str,
                        &script_id,
                    ])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                let _ = std::process::Command::new("qdbus-qt6")
                    .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.start"])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        } else {
            if enabled {
                let _ = std::process::Command::new("hyprctl")
                    .args(["keyword", "windowrulev2", &format!("pin,class:^({})$", app_id)])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                let _ = std::process::Command::new("swaymsg")
                    .args([&format!("[app_id=\"{}\"] floating enable, sticky enable", app_id)])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            } else {
                let _ = std::process::Command::new("swaymsg")
                    .args([&format!("[app_id=\"{}\"] sticky disable", app_id)])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
        }

        Ok(())
    }

    pub fn set_resize_constraint_for(
        window_id: Option<u64>,
        aspect_ratio: Option<f32>,
        min_size: Option<(f32, f32)>,
        max_size: Option<(f32, f32)>,
    ) {
        gpui_linux::set_wayland_resize_constraint_for(window_id, aspect_ratio, min_size, max_size);
    }

    pub fn remove_resize_constraint(window_id: u64) {
        gpui_linux::remove_wayland_resize_constraint(window_id);
    }
}
