use anyhow::Result;

pub struct LinuxWindowManager;

impl LinuxWindowManager {
    pub fn configure(app_id: &str) -> Result<()> {
        let is_kde = std::env::var("XDG_CURRENT_DESKTOP")
            .map(|d| d.to_lowercase().contains("kde"))
            .unwrap_or(false);

        if is_kde {
            // Apply KWin Window Rule for Keep-Above, No-Border, Skip-Taskbar, Skip-Switcher
            let rule_id = format!("{}_mirror_rule", app_id);
            let _ = std::process::Command::new("kwriteconfig6")
                .args(&[
                    "--file", "kwinrulesrc",
                    "--group", &rule_id,
                    "--key", "Description", "aire floating window mirror",
                ])
                .status();

            let _ = std::process::Command::new("kwriteconfig6")
                .args(&[
                    "--file", "kwinrulesrc",
                    "--group", &rule_id,
                    "--key", "wmclass", app_id,
                ])
                .status();

            let _ = std::process::Command::new("kwriteconfig6")
                .args(&[
                    "--file", "kwinrulesrc",
                    "--group", &rule_id,
                    "--key", "wmclassmatch", "1", // Exact match
                ])
                .status();

            let _ = std::process::Command::new("kwriteconfig6")
                .args(&[
                    "--file", "kwinrulesrc",
                    "--group", &rule_id,
                    "--key", "above", "true",
                ])
                .status();

            let _ = std::process::Command::new("kwriteconfig6")
                .args(&[
                    "--file", "kwinrulesrc",
                    "--group", &rule_id,
                    "--key", "aboverule", "3", // Force
                ])
                .status();

            let _ = std::process::Command::new("kwriteconfig6")
                .args(&[
                    "--file", "kwinrulesrc",
                    "--group", &rule_id,
                    "--key", "noborder", "true",
                ])
                .status();

            let _ = std::process::Command::new("kwriteconfig6")
                .args(&[
                    "--file", "kwinrulesrc",
                    "--group", &rule_id,
                    "--key", "noborderrule", "3",
                ])
                .status();

            let _ = std::process::Command::new("kwriteconfig6")
                .args(&[
                    "--file", "kwinrulesrc",
                    "--group", &rule_id,
                    "--key", "skiptaskbar", "true",
                ])
                .status();

            let _ = std::process::Command::new("kwriteconfig6")
                .args(&[
                    "--file", "kwinrulesrc",
                    "--group", &rule_id,
                    "--key", "skiptaskbarrule", "3",
                ])
                .status();

            let _ = std::process::Command::new("kwriteconfig6")
                .args(&[
                    "--file", "kwinrulesrc",
                    "--group", &rule_id,
                    "--key", "skipswitcher", "true",
                ])
                .status();

            let _ = std::process::Command::new("kwriteconfig6")
                .args(&[
                    "--file", "kwinrulesrc",
                    "--group", &rule_id,
                    "--key", "skipswitcherrule", "3",
                ])
                .status();

            // Reconfigure KWin
            let _ = std::process::Command::new("qdbus-qt6")
                .args(&["org.kde.KWin", "/KWin", "reconfigure"])
                .status();
        } else {
            eprintln!("[WINDOW] Note: Running outside KDE Plasma. Toggle 'Always on Top' via your compositor's window menu if needed.");
        }

        Ok(())
    }
}
