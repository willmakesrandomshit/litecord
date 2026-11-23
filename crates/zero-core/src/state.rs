use std::collections::{HashMap, VecDeque};

use chrono::{Duration, Utc};

use crate::model::{
    AppSettings, Channel, ChannelId, ChannelKind, Guild, GuildId, Message, MessageId, Reaction,
    User, UserId, UserStatus,
};

pub const MAX_CACHED_MESSAGES_PER_CHANNEL: usize = 100_000;

#[derive(Debug, Clone)]
pub struct ZeroState {
    pub current_user: User,
    pub users: HashMap<UserId, User>,
    pub guilds: Vec<Guild>,
    pub dm_channels: Vec<Channel>,
    pub selected_guild: Option<GuildId>,
    pub selected_channel: ChannelId,
    pub messages_by_channel: HashMap<ChannelId, VecDeque<Message>>,
    pub settings: AppSettings,
    pub composer_drafts: HashMap<ChannelId, String>,
    next_message_id: u64,
}

impl ZeroState {
    pub fn new_with_demo_data() -> Self {
        let now = Utc::now();

        let current_user = User {
            id: UserId(1),
            username: "litecord_pilot".into(),
            display_name: "Litecord Pilot".into(),
            avatar_color: [88, 101, 242],
            status: UserStatus::Online,
            custom_status: Some("Running Litecord in native Rust".into()),
        };

        let alice = User {
            id: UserId(2),
            username: "ferris_dev".into(),
            display_name: "Ferris Dev".into(),
            avatar_color: [235, 69, 158],
            status: UserStatus::Online,
            custom_status: Some("Optimizing allocations".into()),
        };

        let bob = User {
            id: UserId(3),
            username: "wgpu_wizard".into(),
            display_name: "WGPU Wizard".into(),
            avatar_color: [59, 165, 93],
            status: UserStatus::Idle,
            custom_status: None,
        };

        let carol = User {
            id: UserId(4),
            username: "sqlite_sage".into(),
            display_name: "SQLite Sage".into(),
            avatar_color: [250, 166, 26],
            status: UserStatus::DoNotDisturb,
            custom_status: Some("Benchmarking WAL mode".into()),
        };

        let dave = User {
            id: UserId(5),
            username: "async_tokio".into(),
            display_name: "Async Tokio".into(),
            avatar_color: [114, 137, 218],
            status: UserStatus::Offline,
            custom_status: None,
        };

        let mut users = HashMap::new();
        for u in [&current_user, &alice, &bob, &carol, &dave] {
            users.insert(u.id, u.clone());
        }

        let g1 = Guild {
            id: GuildId(100),
            name: "Litecord HQ".into(),
            short_name: "LC".into(),
            accent_color: [88, 101, 242],
            member_ids: vec![UserId(1), UserId(2), UserId(3), UserId(4), UserId(5)],
            channels: vec![
                Channel {
                    id: ChannelId(1001),
                    guild_id: Some(GuildId(100)),
                    category: Some("GENERAL".into()),
                    name: "general".into(),
                    topic: Some("Local demo workspace — zero Electron, 100% native Rust".into()),
                    kind: ChannelKind::Text,
                    unread_count: 0,
                },
                Channel {
                    id: ChannelId(1002),
                    guild_id: Some(GuildId(100)),
                    category: Some("GENERAL".into()),
                    name: "announcements".into(),
                    topic: Some("Release milestones and benchmark reports".into()),
                    kind: ChannelKind::Announcement,
                    unread_count: 2,
                },
                Channel {
                    id: ChannelId(1003),
                    guild_id: Some(GuildId(100)),
                    category: Some("ENGINEERING".into()),
                    name: "architecture".into(),
                    topic: Some(
                        "iced UI, zero-core state, SQLite storage, and plugin sandbox".into(),
                    ),
                    kind: ChannelKind::Text,
                    unread_count: 1,
                },
                Channel {
                    id: ChannelId(1004),
                    guild_id: Some(GuildId(100)),
                    category: Some("ENGINEERING".into()),
                    name: "benchmarks".into(),
                    topic: Some("Memory < 75 MB idle, cold startup < 1s, 120 FPS scrolling".into()),
                    kind: ChannelKind::Text,
                    unread_count: 0,
                },
                Channel {
                    id: ChannelId(1005),
                    guild_id: Some(GuildId(100)),
                    category: Some("VOICE".into()),
                    name: "Lounge".into(),
                    topic: None,
                    kind: ChannelKind::Voice,
                    unread_count: 0,
                },
            ],
        };

        let g2 = Guild {
            id: GuildId(200),
            name: "Rustacean Systems".into(),
            short_name: "RS".into(),
            accent_color: [230, 126, 34],
            member_ids: vec![UserId(1), UserId(2), UserId(4)],
            channels: vec![
                Channel {
                    id: ChannelId(2001),
                    guild_id: Some(GuildId(200)),
                    category: Some("LOUNGE".into()),
                    name: "rust-chat".into(),
                    topic: Some("Borrow checker appreciation society".into()),
                    kind: ChannelKind::Text,
                    unread_count: 3,
                },
                Channel {
                    id: ChannelId(2002),
                    guild_id: Some(GuildId(200)),
                    category: Some("LOUNGE".into()),
                    name: "wgpu-rendering".into(),
                    topic: Some("Shaders, glyphon text shaping, and frame pacing".into()),
                    kind: ChannelKind::Text,
                    unread_count: 0,
                },
            ],
        };

        let dm_channels = vec![
            Channel {
                id: ChannelId(9001),
                guild_id: None,
                category: None,
                name: "Ferris Dev".into(),
                topic: Some("Direct Message (Local Demo)".into()),
                kind: ChannelKind::DirectMessage,
                unread_count: 1,
            },
            Channel {
                id: ChannelId(9002),
                guild_id: None,
                category: None,
                name: "WGPU Wizard".into(),
                topic: Some("Direct Message (Local Demo)".into()),
                kind: ChannelKind::DirectMessage,
                unread_count: 0,
            },
        ];

        let mut messages_by_channel = HashMap::new();
        let mut msg_id = 1u64;
        let mut push_msg =
            |cid: ChannelId, uid: UserId, mins_ago: i64, text: &str, reactions: Vec<Reaction>| {
                let m = Message {
                    id: MessageId(msg_id),
                    channel_id: cid,
                    author_id: uid,
                    content: text.to_string(),
                    timestamp: now - Duration::minutes(mins_ago),
                    edited: false,
                    pinned: false,
                    reactions,
                    reply_to: None,
                };
                msg_id += 1;
                messages_by_channel
                    .entry(cid)
                    .or_insert_with(VecDeque::new)
                    .push_back(m);
            };

        push_msg(
            ChannelId(1001),
            UserId(2),
            42,
            "Welcome to Discord Zero! This is a native Rust desktop client rendered with iced (no Electron, no Chromium).",
            vec![Reaction {
                emoji: "🚀".into(),
                count: 4,
                me: true,
            }],
        );
        push_msg(
            ChannelId(1001),
            UserId(3),
            35,
            "GPU-accelerated rendering via wgpu with cosmic-text shaping keeps idle RAM and CPU tiny.",
            vec![],
        );
        push_msg(
            ChannelId(1001),
            UserId(4),
            18,
            "Settings and local state persist in SQLite via rusqlite. Try switching themes in Settings or sending a message below!",
            vec![Reaction {
                emoji: "🦀".into(),
                count: 3,
                me: false,
            }],
        );
        push_msg(
            ChannelId(1002),
            UserId(2),
            120,
            "Phase A Foundation: Cargo workspace, native shell, navigation, timeline, composer, and SQLite settings persistence.",
            vec![],
        );
        push_msg(
            ChannelId(1003),
            UserId(3),
            60,
            "zero-core models Guilds, Channels, Users, and bounded Message queues independently of Discord's network layer.",
            vec![],
        );
        push_msg(
            ChannelId(2001),
            UserId(2),
            15,
            "Zero-copy where possible, bounded LRU caches everywhere else.",
            vec![],
        );
        push_msg(
            ChannelId(9001),
            UserId(2),
            5,
            "Hey! Ready to benchmark memory usage once Phase A compiles?",
            vec![],
        );

        Self {
            current_user,
            users,
            guilds: vec![g1, g2],
            dm_channels,
            selected_guild: Some(GuildId(100)),
            selected_channel: ChannelId(1001),
            messages_by_channel,
            settings: AppSettings::default(),
            composer_drafts: HashMap::new(),
            next_message_id: msg_id,
        }
    }

    pub fn select_guild(&mut self, guild_id: Option<GuildId>) {
        self.selected_guild = guild_id;
        let next_channel = match guild_id {
            Some(gid) => self
                .guilds
                .iter()
                .find(|g| g.id == gid)
                .and_then(|g| g.channels.iter().find(|c| c.kind != ChannelKind::Voice))
                .map(|c| c.id),
            None => self.dm_channels.first().map(|c| c.id),
        };
        if let Some(cid) = next_channel {
            self.select_channel(cid);
        }
    }

    pub fn select_channel(&mut self, channel_id: ChannelId) {
        self.selected_channel = channel_id;
        for guild in &mut self.guilds {
            for ch in &mut guild.channels {
                if ch.id == channel_id {
                    ch.unread_count = 0;
                }
            }
        }
        for ch in &mut self.dm_channels {
            if ch.id == channel_id {
                ch.unread_count = 0;
            }
        }
    }

    pub fn current_guild(&self) -> Option<&Guild> {
        let gid = self.selected_guild?;
        self.guilds.iter().find(|g| g.id == gid)
    }

    pub fn current_channel(&self) -> Option<&Channel> {
        if let Some(guild) = self.current_guild() {
            if let Some(ch) = guild
                .channels
                .iter()
                .find(|c| c.id == self.selected_channel)
            {
                return Some(ch);
            }
        }
        self.dm_channels
            .iter()
            .find(|c| c.id == self.selected_channel)
    }

    pub fn current_messages(&self) -> Option<&VecDeque<Message>> {
        self.messages_by_channel.get(&self.selected_channel)
    }

    pub fn current_members(&self) -> Vec<&User> {
        match self.current_guild() {
            Some(guild) => guild
                .member_ids
                .iter()
                .filter_map(|id| self.users.get(id))
                .collect(),
            None => self.users.values().collect(),
        }
    }

    pub fn send_message(&mut self, raw_content: &str) -> Option<Message> {
        self.send_reply(raw_content, None)
    }

    pub fn send_reply(
        &mut self,
        raw_content: &str,
        reply_to: Option<MessageId>,
    ) -> Option<Message> {
        let content = raw_content.trim();
        if content.is_empty() {
            return None;
        }
        let msg = Message {
            id: MessageId(self.next_message_id),
            channel_id: self.selected_channel,
            author_id: self.current_user.id,
            content: content.to_string(),
            timestamp: Utc::now(),
            edited: false,
            pinned: false,
            reactions: Vec::new(),
            reply_to,
        };
        self.next_message_id += 1;

        let queue = self
            .messages_by_channel
            .entry(self.selected_channel)
            .or_default();
        if queue.len() >= MAX_CACHED_MESSAGES_PER_CHANNEL {
            queue.pop_front();
        }
        queue.push_back(msg.clone());
        self.composer_drafts.remove(&self.selected_channel);
        Some(msg)
    }

    pub fn edit_message(&mut self, message_id: MessageId, new_content: &str) -> bool {
        let trimmed = new_content.trim();
        if trimmed.is_empty() {
            return false;
        }
        let Some(queue) = self.messages_by_channel.get_mut(&self.selected_channel) else {
            return false;
        };
        let Some(msg) = queue.iter_mut().find(|m| m.id == message_id) else {
            return false;
        };
        msg.content = trimmed.to_string();
        msg.edited = true;
        true
    }

    pub fn delete_message(&mut self, message_id: MessageId) -> bool {
        let Some(queue) = self.messages_by_channel.get_mut(&self.selected_channel) else {
            return false;
        };
        let Some(pos) = queue.iter().position(|m| m.id == message_id) else {
            return false;
        };
        queue.remove(pos);
        true
    }

    pub fn toggle_pin(&mut self, message_id: MessageId) -> bool {
        let Some(queue) = self.messages_by_channel.get_mut(&self.selected_channel) else {
            return false;
        };
        let Some(msg) = queue.iter_mut().find(|m| m.id == message_id) else {
            return false;
        };
        msg.pinned = !msg.pinned;
        true
    }

    pub fn pinned_messages(&self) -> Vec<&Message> {
        self.current_messages()
            .map(|q| q.iter().filter(|m| m.pinned).collect())
            .unwrap_or_default()
    }

    pub fn create_local_channel(&mut self, raw_name: &str) -> Option<ChannelId> {
        let clean = raw_name.trim().to_lowercase().replace(' ', "-");
        if clean.is_empty() {
            return None;
        }
        let gid = self.selected_guild?;
        let new_id = ChannelId(
            self.guilds
                .iter()
                .flat_map(|g| g.channels.iter().map(|c| c.id.0))
                .max()
                .unwrap_or(1000)
                + 1,
        );
        if let Some(guild) = self.guilds.iter_mut().find(|g| g.id == gid) {
            guild.channels.push(Channel {
                id: new_id,
                guild_id: Some(gid),
                category: Some("GENERAL".into()),
                name: clean,
                topic: Some("Created locally in Litecord".into()),
                kind: ChannelKind::Text,
                unread_count: 0,
            });
            self.select_channel(new_id);
            return Some(new_id);
        }
        None
    }

    pub fn toggle_reaction(&mut self, message_id: MessageId, emoji: &str) {
        let Some(queue) = self.messages_by_channel.get_mut(&self.selected_channel) else {
            return;
        };
        let Some(msg) = queue.iter_mut().find(|m| m.id == message_id) else {
            return;
        };
        if let Some(idx) = msg.reactions.iter().position(|r| r.emoji == emoji) {
            if msg.reactions[idx].me {
                msg.reactions[idx].count = msg.reactions[idx].count.saturating_sub(1);
                msg.reactions[idx].me = false;
                if msg.reactions[idx].count == 0 {
                    msg.reactions.remove(idx);
                }
            } else {
                msg.reactions[idx].count += 1;
                msg.reactions[idx].me = true;
            }
        } else {
            msg.reactions.push(Reaction {
                emoji: emoji.to_string(),
                count: 1,
                me: true,
            });
        }
    }

    pub fn search_current_channel(&self, query: &str) -> Vec<&Message> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        self.current_messages()
            .map(|msgs| {
                msgs.iter()
                    .filter(|m| m.content.to_lowercase().contains(&q))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Populates `count` synthetic messages in `channel_id` for virtualization & memory benchmarking.
    pub fn populate_synthetic_messages(&mut self, channel_id: ChannelId, count: usize) {
        let now = Utc::now();
        let author_ids = [UserId(1), UserId(2), UserId(3), UserId(4)];
        let mut queue = VecDeque::with_capacity(count.min(MAX_CACHED_MESSAGES_PER_CHANNEL));
        for i in 0..count.min(MAX_CACHED_MESSAGES_PER_CHANNEL) {
            let author_id = author_ids[i % author_ids.len()];
            let content = match i % 4 {
                0 => format!("Synthetic message #{i}: short status check."),
                1 => format!(
                    "Synthetic message #{i}: testing variable-height wrapping in the virtualized timeline with a slightly longer paragraph of text."
                ),
                2 => format!("Synthetic message #{i}: Rust + iced native rendering benchmark."),
                _ => format!(
                    "Synthetic message #{i}: line 1\nline 2 with additional detail for height estimation."
                ),
            };
            queue.push_back(Message {
                id: MessageId(self.next_message_id),
                channel_id,
                author_id,
                content,
                timestamp: now - Duration::seconds((count - i) as i64),
                edited: false,
                pinned: false,
                reactions: Vec::new(),
                reply_to: None,
            });
            self.next_message_id += 1;
        }
        self.messages_by_channel.insert(channel_id, queue);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_state_initializes_valid_selection() {
        let state = ZeroState::new_with_demo_data();
        assert!(state.current_guild().is_some());
        assert!(state.current_channel().is_some());
        assert!(!state.current_messages().unwrap().is_empty());
    }

    #[test]
    fn send_message_appends_and_ignores_blank() {
        let mut state = ZeroState::new_with_demo_data();
        let initial_len = state.current_messages().unwrap().len();
        assert!(state.send_message("   \n\t ").is_none());
        assert_eq!(state.current_messages().unwrap().len(), initial_len);

        let sent = state.send_message("Hello Discord Zero!").unwrap();
        assert_eq!(sent.content, "Hello Discord Zero!");
        assert_eq!(state.current_messages().unwrap().len(), initial_len + 1);
    }

    #[test]
    fn message_queue_is_bounded() {
        let mut state = ZeroState::new_with_demo_data();
        for i in 0..(MAX_CACHED_MESSAGES_PER_CHANNEL + 25) {
            state.send_message(&format!("msg {i}"));
        }
        assert_eq!(
            state.current_messages().unwrap().len(),
            MAX_CACHED_MESSAGES_PER_CHANNEL
        );
    }

    #[test]
    fn selecting_channel_clears_unread_count() {
        let mut state = ZeroState::new_with_demo_data();
        state.select_channel(ChannelId(1002));
        let ch = state.current_channel().unwrap();
        assert_eq!(ch.id, ChannelId(1002));
        assert_eq!(ch.unread_count, 0);
    }

    #[test]
    fn toggle_reaction_adds_and_removes() {
        let mut state = ZeroState::new_with_demo_data();
        let first_id = state.current_messages().unwrap().front().unwrap().id;
        state.toggle_reaction(first_id, "🔥");
        let msg = state.current_messages().unwrap().front().unwrap();
        assert!(msg
            .reactions
            .iter()
            .any(|r| r.emoji == "🔥" && r.me && r.count == 1));

        state.toggle_reaction(first_id, "🔥");
        let msg = state.current_messages().unwrap().front().unwrap();
        assert!(!msg.reactions.iter().any(|r| r.emoji == "🔥"));
    }

    #[test]
    fn search_filters_case_insensitively() {
        let state = ZeroState::new_with_demo_data();
        let hits = state.search_current_channel("WGPU");
        assert_eq!(hits.len(), 1);
    }
}
