use anyhow::Result;

pub struct LinuxWindowManager;

impl LinuxWindowManager {
    pub fn configure(app_id: &str) -> Result<()> {
        let is_kde = std::env::var("XDG_CURRENT_DESKTOP")
            .map(|d| d.to_lowercase().contains("kde"))
            .unwrap_or(false);

        if is_kde {
            // 1. Static rule via kwinrulesrc
            let rule_id = format!("{}_mirror_rule", app_id);
            for (key, val) in [
                ("Description", "aire floating window mirror"),
                ("wmclass", app_id),
                ("wmclassmatch", "1"),
                ("above", "true"),
                ("aboverule", "3"),
                ("noborder", "true"),
                ("noborderrule", "3"),
                ("skiptaskbar", "true"),
                ("skiptaskbarrule", "3"),
                ("skipswitcher", "true"),
                ("skipswitcherrule", "3"),
            ] {
                let _ = std::process::Command::new("kwriteconfig6")
                    .args(&["--file", "kwinrulesrc", "--group", &rule_id, "--key", key, val])
                    .status();
            }

            let _ = std::process::Command::new("qdbus-qt6")
                .args(&["org.kde.KWin", "/KWin", "reconfigure"])
                .status();

            // 2. Dynamic live rule via KWin Scripting Engine (fires immediately on windowAdded and active windows)
            let script_id = format!("{}_keepabove_daemon", app_id);
            let js_code = format!(
                r#"
                function enforce(client) {{
                    if (client && (client.resourceClass == "{app_id}" || client.resourceName == "{app_id}")) {{
                        client.keepAbove = true;
                        client.noBorder = true;
                        client.skipTaskbar = true;
                        client.skipSwitcher = true;
                    }}
                }}
                workspace.windowAdded.connect(enforce);
                for (var i = 0; i < workspace.windowList().length; i++) {{
                    enforce(workspace.windowList()[i]);
                }}
                "#
            );

            let temp_path = std::env::temp_dir().join(format!("{}.js", script_id));
            if std::fs::write(&temp_path, js_code).is_ok() {
                let path_str = temp_path.to_string_lossy().to_string();
                let _ = std::process::Command::new("qdbus-qt6")
                    .args(&["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.unloadScript", &script_id])
                    .status();
                let _ = std::process::Command::new("qdbus-qt6")
                    .args(&["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.loadScript", &path_str, &script_id])
                    .status();
                let _ = std::process::Command::new("qdbus-qt6")
                    .args(&["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.start"])
                    .status();
            }
        } else {
            // Hyprland window rule
            let _ = std::process::Command::new("hyprctl")
                .args(&["keyword", "windowrulev2", &format!("stayfocused,class:^({})$", app_id)])
                .status();
            let _ = std::process::Command::new("hyprctl")
                .args(&["keyword", "windowrulev2", &format!("pin,class:^({})$", app_id)])
                .status();

            // Sway / i3 window rule
            let _ = std::process::Command::new("swaymsg")
                .args(&[&format!("[app_id=\"{}\"] floating enable, sticky enable", app_id)])
                .status();
        }

        Ok(())
    }

    pub fn set_resize_constraint(
        aspect_ratio: Option<f32>,
        min_size: Option<(f32, f32)>,
        max_size: Option<(f32, f32)>,
    ) {
        gpui_linux::set_wayland_resize_constraint(aspect_ratio, min_size, max_size);
    }
}
