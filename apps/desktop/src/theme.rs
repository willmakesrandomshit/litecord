use iced::{Color, Theme};
use zero_core::ThemeMode;
use zero_themes::ThemeTokens;

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg_server_rail: Color,
    pub bg_channel_rail: Color,
    pub bg_chat: Color,
    pub bg_composer: Color,
    pub bg_user_panel: Color,
    pub bg_hover: Color,
    pub bg_selected: Color,
    pub border: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,
    pub accent: Color,
    pub unread_badge: Color,
    pub online: Color,
    pub idle: Color,
    pub dnd: Color,
    pub offline: Color,
}

fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgb8(c[0], c[1], c[2])
}

impl Palette {
    pub fn from_tokens(t: &ThemeTokens, mode: ThemeMode) -> Self {
        let (online, idle, dnd, offline) = match mode {
            ThemeMode::Dark => (
                Color::from_rgb8(35, 165, 90),
                Color::from_rgb8(240, 178, 50),
                Color::from_rgb8(242, 63, 67),
                Color::from_rgb8(128, 132, 142),
            ),
            ThemeMode::Light => (
                Color::from_rgb8(35, 165, 90),
                Color::from_rgb8(217, 142, 15),
                Color::from_rgb8(218, 55, 60),
                Color::from_rgb8(116, 127, 141),
            ),
        };
        Self {
            bg_server_rail: rgb(t.bg_server_rail),
            bg_channel_rail: rgb(t.bg_channel_rail),
            bg_chat: rgb(t.bg_chat),
            bg_composer: rgb(t.bg_composer),
            bg_user_panel: rgb(t.bg_user_panel),
            bg_hover: rgb(t.bg_hover),
            bg_selected: rgb(t.bg_selected),
            border: rgb(t.border),
            text_primary: rgb(t.text_primary),
            text_secondary: rgb(t.text_secondary),
            text_muted: rgb(t.text_muted),
            accent: rgb(t.accent),
            unread_badge: rgb(t.unread_badge),
            online,
            idle,
            dnd,
            offline,
        }
    }

    pub fn iced_theme(mode: ThemeMode) -> Theme {
        match mode {
            ThemeMode::Dark => Theme::Dark,
            ThemeMode::Light => Theme::Light,
        }
    }
}
