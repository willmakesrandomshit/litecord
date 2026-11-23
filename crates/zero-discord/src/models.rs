use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zero_core::{
    Channel, ChannelId, ChannelKind, Guild, GuildId, Message, MessageId, User, UserId, UserStatus,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GatewayInfo {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
    pub global_name: Option<String>,
    #[serde(default)]
    pub bot: bool,
}

impl DiscordUser {
    pub fn parse_id(&self) -> u64 {
        self.id.parse::<u64>().unwrap_or(0)
    }

    pub fn to_zero_user(&self) -> User {
        let id = self.parse_id();
        let display = self
            .global_name
            .clone()
            .unwrap_or_else(|| self.username.clone());
        let color_seed = (id % 5) as usize;
        let palette = [
            [88, 101, 242],
            [235, 69, 158],
            [59, 165, 93],
            [250, 166, 26],
            [114, 137, 218],
        ];
        User {
            id: UserId(id),
            username: self.username.clone(),
            display_name: display,
            avatar_color: palette[color_seed],
            status: UserStatus::Online,
            custom_status: if self.bot {
                Some("Authorized Bot".into())
            } else {
                None
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordGuild {
    pub id: String,
    pub name: String,
}

impl DiscordGuild {
    pub fn to_zero_guild(&self, channels: Vec<Channel>) -> Guild {
        let id = self.id.parse::<u64>().unwrap_or(0);
        let short_name: String = self
            .name
            .split_whitespace()
            .filter_map(|w| w.chars().next())
            .take(2)
            .collect::<String>()
            .to_uppercase();
        Guild {
            id: GuildId(id),
            name: self.name.clone(),
            short_name: if short_name.is_empty() {
                "SV".into()
            } else {
                short_name
            },
            accent_color: [88, 101, 242],
            channels,
            member_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordChannel {
    pub id: String,
    pub guild_id: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub kind: u8,
    pub topic: Option<String>,
    pub position: Option<i32>,
}

impl DiscordChannel {
    pub fn to_zero_channel(&self) -> Option<Channel> {
        // Discord channel types: 0 = text, 2 = voice, 5 = announcement, 1 = DM
        let kind = match self.kind {
            0 => ChannelKind::Text,
            2 => ChannelKind::Voice,
            5 => ChannelKind::Announcement,
            1 => ChannelKind::DirectMessage,
            _ => return None,
        };
        let id = self.id.parse::<u64>().unwrap_or(0);
        let gid = self.guild_id.as_deref().and_then(|s| s.parse::<u64>().ok());
        Some(Channel {
            id: ChannelId(id),
            guild_id: gid.map(GuildId),
            category: Some("DISCORD".into()),
            name: self.name.clone().unwrap_or_else(|| format!("channel-{id}")),
            topic: self.topic.clone(),
            kind,
            unread_count: 0,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscordMessage {
    pub id: String,
    pub channel_id: String,
    pub author: DiscordUser,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub edited_timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    pub pinned: bool,
}

impl DiscordMessage {
    pub fn to_zero_message(&self) -> Message {
        Message {
            id: MessageId(self.id.parse::<u64>().unwrap_or(0)),
            channel_id: ChannelId(self.channel_id.parse::<u64>().unwrap_or(0)),
            author_id: UserId(self.author.parse_id()),
            content: self.content.clone(),
            timestamp: self.timestamp,
            edited: self.edited_timestamp.is_some(),
            pinned: self.pinned,
            reactions: Vec::new(),
            reply_to: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CreateMessagePayload<'a> {
    pub content: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discord_message_deserializes_and_maps_to_zero_core() {
        let json = r#"{
            "id": "123456789012345678",
            "channel_id": "987654321098765432",
            "author": {
                "id": "111222333444555666",
                "username": "zero_bot",
                "global_name": "Zero Bot",
                "bot": true
            },
            "content": "Hello from Discord API v10!",
            "timestamp": "2026-11-22T20:00:00.000000+00:00",
            "edited_timestamp": null,
            "pinned": false
        }"#;

        let dm: DiscordMessage = serde_json::from_str(json).unwrap();
        let zm = dm.to_zero_message();
        assert_eq!(zm.id, MessageId(123456789012345678));
        assert_eq!(zm.channel_id, ChannelId(987654321098765432));
        assert_eq!(zm.author_id, UserId(111222333444555666));
        assert_eq!(zm.content, "Hello from Discord API v10!");
    }
}
