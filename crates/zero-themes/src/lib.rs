#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use thiserror::Error;
use zero_core::ThemeMode;

#[derive(Debug, Error)]
pub enum ThemeError {
    #[error("Theme '{0}' not found")]
    NotFound(String),
    #[error("Cannot remove built-in theme '{0}'")]
    CannotRemoveBuiltin(String),
    #[error("Invalid theme JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeTokens {
    pub bg_server_rail: [u8; 3],
    pub bg_channel_rail: [u8; 3],
    pub bg_chat: [u8; 3],
    pub bg_composer: [u8; 3],
    pub bg_user_panel: [u8; 3],
    pub bg_hover: [u8; 3],
    pub bg_selected: [u8; 3],
    pub border: [u8; 3],
    pub text_primary: [u8; 3],
    pub text_secondary: [u8; 3],
    pub text_muted: [u8; 3],
    pub accent: [u8; 3],
    pub unread_badge: [u8; 3],
    pub font_size: f32,
}

impl ThemeTokens {
    pub fn dark_default() -> Self {
        Self {
            bg_server_rail: [30, 31, 34],
            bg_channel_rail: [43, 45, 49],
            bg_chat: [49, 51, 56],
            bg_composer: [56, 58, 64],
            bg_user_panel: [35, 36, 40],
            bg_hover: [53, 55, 60],
            bg_selected: [64, 66, 73],
            border: [38, 40, 44],
            text_primary: [242, 243, 245],
            text_secondary: [181, 186, 193],
            text_muted: [148, 155, 164],
            accent: [88, 101, 242],
            unread_badge: [242, 63, 66],
            font_size: 14.0,
        }
    }

    pub fn light_default() -> Self {
        Self {
            bg_server_rail: [227, 229, 232],
            bg_channel_rail: [242, 243, 245],
            bg_chat: [255, 255, 255],
            bg_composer: [235, 237, 239],
            bg_user_panel: [235, 237, 239],
            bg_hover: [229, 231, 235],
            bg_selected: [212, 215, 220],
            border: [218, 220, 224],
            text_primary: [46, 51, 56],
            text_secondary: [79, 86, 96],
            text_muted: [116, 127, 141],
            accent: [88, 101, 242],
            unread_badge: [218, 55, 60],
            font_size: 14.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeDefinition {
    pub id: String,
    pub name: String,
    pub author: String,
    pub base_mode: ThemeMode,
    pub tokens: ThemeTokens,
    pub builtin: bool,
}

#[derive(Debug, Clone)]
pub struct ThemeParseReport {
    pub theme: ThemeDefinition,
    pub mapped_variables: usize,
    pub warnings: Vec<String>,
}

pub struct ThemeManager {
    themes: Vec<ThemeDefinition>,
    active_theme_id: String,
}

impl Default for ThemeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ThemeManager {
    pub fn new() -> Self {
        let themes = vec![
            ThemeDefinition {
                id: "zero-dark".into(),
                name: "Discord Zero Dark".into(),
                author: "Discord Zero".into(),
                base_mode: ThemeMode::Dark,
                tokens: ThemeTokens::dark_default(),
                builtin: true,
            },
            ThemeDefinition {
                id: "zero-light".into(),
                name: "Discord Zero Light".into(),
                author: "Discord Zero".into(),
                base_mode: ThemeMode::Light,
                tokens: ThemeTokens::light_default(),
                builtin: true,
            },
            ThemeDefinition {
                id: "midnight-oled".into(),
                name: "Midnight OLED".into(),
                author: "Discord Zero".into(),
                base_mode: ThemeMode::Dark,
                tokens: ThemeTokens {
                    bg_server_rail: [0, 0, 0],
                    bg_channel_rail: [10, 10, 14],
                    bg_chat: [14, 15, 20],
                    bg_composer: [24, 26, 32],
                    bg_user_panel: [8, 8, 12],
                    bg_hover: [26, 28, 36],
                    bg_selected: [36, 39, 52],
                    border: [28, 30, 38],
                    accent: [114, 137, 218],
                    ..ThemeTokens::dark_default()
                },
                builtin: true,
            },
        ];

        Self {
            themes,
            active_theme_id: "zero-dark".into(),
        }
    }

    pub fn themes(&self) -> &[ThemeDefinition] {
        &self.themes
    }

    pub fn active_theme_id(&self) -> &str {
        &self.active_theme_id
    }

    pub fn active_theme(&self) -> &ThemeDefinition {
        self.themes
            .iter()
            .find(|t| t.id == self.active_theme_id)
            .unwrap_or(&self.themes[0])
    }

    pub fn set_active(&mut self, id: &str) -> Result<(), ThemeError> {
        if self.themes.iter().any(|t| t.id == id) {
            self.active_theme_id = id.to_string();
            Ok(())
        } else {
            Err(ThemeError::NotFound(id.to_string()))
        }
    }

    pub fn install_theme(&mut self, mut theme: ThemeDefinition) {
        theme.builtin = false;
        if let Some(existing) = self.themes.iter_mut().find(|t| t.id == theme.id) {
            *existing = theme;
        } else {
            self.themes.push(theme);
        }
    }

    pub fn remove_theme(&mut self, id: &str) -> Result<(), ThemeError> {
        let Some(pos) = self.themes.iter().position(|t| t.id == id) else {
            return Err(ThemeError::NotFound(id.to_string()));
        };
        if self.themes[pos].builtin {
            return Err(ThemeError::CannotRemoveBuiltin(id.to_string()));
        }
        self.themes.remove(pos);
        if self.active_theme_id == id {
            self.active_theme_id = "zero-dark".into();
        }
        Ok(())
    }

    pub fn export_css(&self, id: &str) -> Result<String, ThemeError> {
        let t = self
            .themes
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| ThemeError::NotFound(id.to_string()))?;
        let k = &t.tokens;
        Ok(format!(
            "/* {} by {} */\n:root {{\n  --background-tertiary: #{:02x}{:02x}{:02x};\n  --background-secondary: #{:02x}{:02x}{:02x};\n  --background-primary: #{:02x}{:02x}{:02x};\n  --channeltextarea-background: #{:02x}{:02x}{:02x};\n  --text-normal: #{:02x}{:02x}{:02x};\n  --text-muted: #{:02x}{:02x}{:02x};\n  --brand-experiment: #{:02x}{:02x}{:02x};\n  --font-size: {}px;\n}}\n",
            t.name,
            t.author,
            k.bg_server_rail[0], k.bg_server_rail[1], k.bg_server_rail[2],
            k.bg_channel_rail[0], k.bg_channel_rail[1], k.bg_channel_rail[2],
            k.bg_chat[0], k.bg_chat[1], k.bg_chat[2],
            k.bg_composer[0], k.bg_composer[1], k.bg_composer[2],
            k.text_primary[0], k.text_primary[1], k.text_primary[2],
            k.text_muted[0], k.text_muted[1], k.text_muted[2],
            k.accent[0], k.accent[1], k.accent[2],
            k.font_size,
        ))
    }

    pub fn export_json(&self, id: &str) -> Result<String, ThemeError> {
        let t = self
            .themes
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| ThemeError::NotFound(id.to_string()))?;
        Ok(serde_json::to_string_pretty(t)?)
    }
}

/// Translates a subset of Discord/Vencord CSS variables into native `ThemeTokens`,
/// explicitly reporting any unsupported CSS constructs.
pub fn parse_css_theme(id: &str, name: &str, author: &str, css: &str) -> ThemeParseReport {
    let mut tokens = ThemeTokens::dark_default();
    let mut mapped_variables = 0usize;
    let mut warnings = Vec::new();

    let normalized = css
        .replace('{', "{\n")
        .replace('}', "\n}\n")
        .replace(';', ";\n");
    for raw_line in normalized.lines() {
        let line = raw_line.trim();
        if line.is_empty()
            || line.starts_with("/*")
            || line.starts_with('*')
            || line == "{"
            || line == "}"
        {
            continue;
        }
        if line.starts_with("@import")
            || line.starts_with("@keyframes")
            || line.starts_with("@media")
        {
            warnings.push(format!("Unsupported CSS at-rule ignored: {line}"));
            continue;
        }
        if !line.starts_with("--") {
            if line.contains('{') && !line.starts_with(":root") && !line.starts_with("body") {
                warnings.push(format!("Unsupported CSS selector ignored: {line}"));
            } else if line.contains("backdrop-filter")
                || line.contains("transform:")
                || line.contains("animation:")
            {
                warnings.push(format!("Unsupported CSS property ignored: {line}"));
            }
            continue;
        }

        let Some((var_name, raw_val)) = line.split_once(':') else {
            continue;
        };
        let var_name = var_name.trim();
        let val = raw_val.trim().trim_end_matches(';').trim();

        if var_name == "--font-size" {
            if let Some(px) = val
                .strip_suffix("px")
                .and_then(|n| n.trim().parse::<f32>().ok())
            {
                tokens.font_size = px.clamp(10.0, 24.0);
                mapped_variables += 1;
            }
            continue;
        }

        if let Some(rgb) = parse_css_color(val) {
            let matched = match var_name {
                "--background-tertiary" | "--bg-server-rail" => {
                    tokens.bg_server_rail = rgb;
                    true
                }
                "--background-secondary" | "--bg-channel-rail" => {
                    tokens.bg_channel_rail = rgb;
                    true
                }
                "--background-primary" | "--bg-chat" => {
                    tokens.bg_chat = rgb;
                    true
                }
                "--channeltextarea-background" | "--bg-composer" => {
                    tokens.bg_composer = rgb;
                    true
                }
                "--background-secondary-alt" | "--bg-user-panel" => {
                    tokens.bg_user_panel = rgb;
                    true
                }
                "--text-normal" | "--header-primary" => {
                    tokens.text_primary = rgb;
                    true
                }
                "--text-muted" | "--channels-default" => {
                    tokens.text_muted = rgb;
                    true
                }
                "--brand-experiment" | "--accent" => {
                    tokens.accent = rgb;
                    true
                }
                _ => false,
            };
            if matched {
                mapped_variables += 1;
            } else {
                warnings.push(format!("Unmapped CSS variable: {var_name}"));
            }
        } else {
            warnings.push(format!("Unsupported color format for {var_name}: {val}"));
        }
    }

    ThemeParseReport {
        theme: ThemeDefinition {
            id: id.to_string(),
            name: name.to_string(),
            author: author.to_string(),
            base_mode: ThemeMode::Dark,
            tokens,
            builtin: false,
        },
        mapped_variables,
        warnings,
    }
}

fn parse_css_color(raw: &str) -> Option<[u8; 3]> {
    let s = raw.trim();
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some([r, g, b]);
        } else if hex.len() == 3 {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            return Some([r, g, b]);
        }
    }
    if let Some(inner) = s.strip_prefix("rgb(").and_then(|v| v.strip_suffix(')')) {
        let parts: Vec<u8> = inner
            .split(',')
            .filter_map(|p| p.trim().parse::<u8>().ok())
            .collect();
        if parts.len() == 3 {
            return Some([parts[0], parts[1], parts[2]]);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_theme_parser_maps_discord_variables_and_reports_unsupported() {
        let css = r#"
            @import url("https://example.com/font.css");
            .chat-2ZfjoI {
                backdrop-filter: blur(8px);
            }
            :root {
                --background-primary: #111214;
                --background-secondary: #1e1f22;
                --brand-experiment: rgb(88, 101, 242);
                --unknown-var: #ff00ff;
                --font-size: 15px;
            }
        "#;

        let report = parse_css_theme("custom-1", "Custom Dark", "Tester", css);
        assert_eq!(report.mapped_variables, 4);
        assert_eq!(report.theme.tokens.bg_chat, [17, 18, 20]);
        assert_eq!(report.theme.tokens.bg_channel_rail, [30, 31, 34]);
        assert_eq!(report.theme.tokens.accent, [88, 101, 242]);
        assert_eq!(report.theme.tokens.font_size, 15.0);
        assert!(report.warnings.iter().any(|w| w.contains("@import")));
        assert!(report.warnings.iter().any(|w| w.contains(".chat-2ZfjoI")));
        assert!(report.warnings.iter().any(|w| w.contains("--unknown-var")));
    }

    #[test]
    fn theme_manager_installs_exports_and_removes_custom_themes() {
        let mut mgr = ThemeManager::new();
        assert!(mgr.remove_theme("zero-dark").is_err());

        let report = parse_css_theme("neon", "Neon", "User", ":root { --accent: #00ff88; }");
        mgr.install_theme(report.theme);
        mgr.set_active("neon").unwrap();
        assert_eq!(mgr.active_theme().tokens.accent, [0, 255, 136]);

        let css_out = mgr.export_css("neon").unwrap();
        assert!(css_out.contains("#00ff88"));

        mgr.remove_theme("neon").unwrap();
        assert_eq!(mgr.active_theme_id(), "zero-dark");
    }
}
