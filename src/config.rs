use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimationSpeed {
    Instant,
    Fast,
    Smooth,
    Relaxed,
}

impl AnimationSpeed {
    pub fn duration_ms(&self) -> u64 {
        match self {
            Self::Instant => 0,
            Self::Fast => 100,
            Self::Smooth => 200,
            Self::Relaxed => 350,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Instant => "Instant",
            Self::Fast => "Fast",
            Self::Smooth => "Smooth",
            Self::Relaxed => "Relaxed",
        }
    }

    pub const ALL: [AnimationSpeed; 4] = [
        Self::Instant,
        Self::Fast,
        Self::Smooth,
        Self::Relaxed,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnSize {
    Compact,
    Medium,
    Large,
}

impl SpawnSize {
    pub fn base_width(&self, aspect_ratio: f32) -> f32 {
        let base: f32 = match self {
            Self::Compact => 320.0,
            Self::Medium => 400.0,
            Self::Large => 560.0,
        };
        if aspect_ratio < 0.85 {
            (base * 0.72).round()
        } else if aspect_ratio > 2.2 {
            (base * 1.2).round()
        } else {
            base
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Compact => "Compact",
            Self::Medium => "Medium",
            Self::Large => "Large",
        }
    }

    pub const ALL: [SpawnSize; 3] = [Self::Compact, Self::Medium, Self::Large];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub target_fps: u32,
    pub show_fps_badge: bool,
    pub hover_opacity: f32,
    pub animation_speed: AnimationSpeed,
    pub always_on_top: bool,
    pub default_spawn_size: SpawnSize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            target_fps: 60,
            show_fps_badge: false,
            hover_opacity: 0.35,
            animation_speed: AnimationSpeed::Smooth,
            always_on_top: true,
            default_spawn_size: SpawnSize::Medium,
        }
    }
}

impl AppConfig {
    pub fn sanitized(mut self) -> Self {
        self.hover_opacity = if self.hover_opacity.is_finite() {
            self.hover_opacity.clamp(0.05, 1.0)
        } else {
            0.35
        };
        self.target_fps = match self.target_fps {
            1..=45 => 30,
            _ => 60,
        };
        self
    }

    pub fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join("aire").join("config.json"))
    }

    pub fn from_json_str(raw: &str) -> Self {
        serde_json::from_str::<Self>(raw)
            .unwrap_or_default()
            .sanitized()
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        match fs::read_to_string(&path) {
            Ok(raw) => Self::from_json_str(&raw),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<()> {
        let Some(path) = Self::config_path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("Failed to create config directory")?;
        }
        let clean = self.clone().sanitized();
        let json = serde_json::to_string_pretty(&clean)?;
        fs::write(&path, json).context("Failed to write config.json")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.target_fps, 60);
        assert!(!cfg.show_fps_badge);
        assert!((cfg.hover_opacity - 0.35).abs() < 1e-5);
        assert_eq!(cfg.animation_speed, AnimationSpeed::Smooth);
        assert!(cfg.always_on_top);
        assert_eq!(cfg.default_spawn_size, SpawnSize::Medium);
    }

    #[test]
    fn corrupted_or_out_of_range_json_is_sanitized() {
        let broken = AppConfig::from_json_str("{not valid json");
        assert_eq!(broken, AppConfig::default());

        let out_of_range = AppConfig::from_json_str(
            r#"{"target_fps": 999, "hover_opacity": -5.0, "show_fps_badge": true}"#,
        );
        assert_eq!(out_of_range.target_fps, 60);
        assert!((out_of_range.hover_opacity - 0.05).abs() < 1e-5);
        assert!(out_of_range.show_fps_badge);
    }
}
