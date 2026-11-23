#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod theme;

use std::collections::{HashSet, VecDeque};

use iced::widget::{
    button, column, container, horizontal_rule, row, scrollable, text, text_input, toggler, Space,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Size, Theme};
use tracing::{info, warn};
use zero_cache::ImageCache;
use zero_core::{
    estimate_message_height, parse_message_segments, Channel, ChannelId, ChannelKind, Guild,
    GuildId, Message, MessageId, MessageSegment, ThemeMode, User, UserStatus, VirtualTimeline,
    ZeroState,
};
use zero_discord::DiscordClient;
use zero_media::{
    capture_primary_screen, enumerate_audio_devices, play_test_chime, AudioDeviceList,
    AudioSettings, MicMonitor, ScreenCaptureFrame,
};
use zero_network::{CredentialStore, OsCredentialStore};
use zero_plugins::{PluginManager, PluginState, SettingValue};
use zero_storage::Storage;
use zero_themes::{parse_css_theme, ThemeManager};
use zero_vencord::{install_custom_vencord_plugin, register_builtin_vencord_plugins};

use theme::Palette;

const TIMELINE_OVERSCAN: usize = 6;
const IMAGE_CACHE_MAX_ITEMS: usize = 256;
const IMAGE_CACHE_MAX_BYTES: usize = 16 * 1024 * 1024;

pub fn main() -> iced::Result {
    if std::env::var_os("ICED_BACKEND").is_none() {
        std::env::set_var("ICED_BACKEND", "tiny-skia");
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Starting Litecord");

    iced::application("Litecord", DiscordZeroApp::update, DiscordZeroApp::view)
        .theme(DiscordZeroApp::theme)
        .window_size(Size::new(1240.0, 800.0))
        .run_with(DiscordZeroApp::new)
}

fn parse_synthetic_arg() -> Option<usize> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--synthetic" {
            if let Some(val) = args.next() {
                if let Ok(n) = val.parse::<usize>() {
                    return Some(n);
                }
            }
        }
    }
    std::env::var("DISCORD_ZERO_SYNTHETIC")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
}

#[derive(Debug, Clone)]
struct BotBootstrap {
    current_user: User,
    guilds: Vec<Guild>,
}

#[derive(Debug, Clone)]
enum UiMessage {
    SelectGuild(Option<GuildId>),
    SelectChannel(ChannelId),
    ComposerChanged(String),
    SendMessage,
    ToggleReaction(MessageId, String),
    SearchChanged(String),
    TimelineScrolled(f32, f32),
    ToggleMemberList,
    ToggleSettings,
    SetTheme(ThemeMode),
    ToggleCompactMode(bool),
    ToggleNotifications(bool),
    SetFontSize(f32),
    LoadSyntheticMessages(usize),
    TestDiscordGateway,
    GatewayTestFinished(Result<String, String>),
    BotTokenChanged(String),
    ConnectBot,
    BotConnected(Result<Box<BotBootstrap>, String>),
    DisconnectBot,
    ChannelMessagesFetched(ChannelId, Result<Vec<(User, Message)>, String>),
    LiveMessageSent(ChannelId, Result<(User, Message), String>),
    SelectThemeId(String),
    CssThemeInputChanged(String),
    ImportCssTheme,
    TogglePlugin(String),
    TogglePluginSetting(String, String, bool),
    RefreshAudioDevices,
    ToggleMicMute,
    ToggleDeafen,
    TogglePushToTalk(bool),
    TestOutputChime,
    OutputChimeFinished(Result<(), String>),
    ToggleMicMonitor,
    JoinVoiceChannel(ChannelId, String),
    LeaveVoiceChannel,
    ToggleScreenShare,
    ScreenShareCaptured(Result<ScreenCaptureFrame, String>),
    StartReply(MessageId),
    CancelReply,
    StartEdit(MessageId, String),
    EditInputChanged(String),
    SaveEdit,
    CancelEdit,
    DeleteMessage(MessageId),
    TogglePin(MessageId),
    TogglePinnedView,
    RevealSpoiler(MessageId),
    NewChannelInputChanged(String),
    CreateLocalChannel,
    CustomPluginInputChanged(String),
    ImportCustomPlugin,
}

struct DiscordZeroApp {
    state: ZeroState,
    storage: Option<Storage>,
    discord: Option<DiscordClient>,
    cred_store: OsCredentialStore,
    bot_token_input: String,
    live_mode: bool,
    gateway_status: Option<String>,
    plugin_manager: PluginManager,
    theme_manager: ThemeManager,
    css_theme_input: String,
    theme_import_status: Option<String>,
    custom_plugin_input: String,
    custom_plugin_status: Option<String>,
    new_channel_input: String,
    audio_devices: AudioDeviceList,
    audio_settings: AudioSettings,
    mic_monitor: Option<MicMonitor>,
    audio_status: Option<String>,
    active_voice_channel: Option<(ChannelId, String)>,
    screen_sharing: bool,
    last_screen_frame: Option<ScreenCaptureFrame>,
    replying_to: Option<MessageId>,
    editing_message: Option<(MessageId, String)>,
    show_pinned_only: bool,
    revealed_spoilers: HashSet<MessageId>,
    image_cache: ImageCache,
    timeline: VirtualTimeline,
    timeline_scroll_id: scrollable::Id,
    scroll_y: f32,
    viewport_height: f32,
    composer_input: String,
    search_query: String,
    show_settings: bool,
}

impl DiscordZeroApp {
    fn new() -> (Self, iced::Task<UiMessage>) {
        let mut state = ZeroState::new_with_demo_data();
        if let Some(count) = parse_synthetic_arg() {
            let cid = state.selected_channel;
            state.populate_synthetic_messages(cid, count);
            info!("Populated {count} synthetic messages in active channel");
        }

        let storage = match Storage::open_default() {
            Ok(s) => {
                match s.load_settings() {
                    Ok(settings) => state.settings = settings,
                    Err(e) => warn!("Failed to load settings from SQLite: {e}"),
                }
                Some(s)
            }
            Err(e) => {
                warn!("Failed to open SQLite storage, running in memory-only mode: {e}");
                None
            }
        };

        let cred_store = OsCredentialStore::default();
        let saved_token = cred_store.get_secret("bot_token").ok().flatten();
        let bot_token_input = saved_token.clone().unwrap_or_default();
        let discord = match DiscordClient::new(saved_token) {
            Ok(c) => Some(c),
            Err(e) => {
                warn!("Failed to initialize DiscordClient: {e}");
                None
            }
        };

        let mut plugin_manager = PluginManager::new();
        register_builtin_vencord_plugins(&mut plugin_manager);
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            if arg == "--enable-plugin" {
                if let Some(pid) = args.next() {
                    if let Err(e) = plugin_manager.enable_plugin(&pid) {
                        warn!("Failed to enable CLI plugin {pid}: {e}");
                    }
                }
            }
        }

        let mut theme_manager = ThemeManager::new();
        let initial_theme_id = match state.settings.theme {
            ThemeMode::Dark => "zero-dark",
            ThemeMode::Light => "zero-light",
        };
        let _ = theme_manager.set_active(initial_theme_id);

        let mut app = Self {
            state,
            storage,
            discord,
            cred_store,
            bot_token_input,
            live_mode: false,
            gateway_status: None,
            plugin_manager,
            theme_manager,
            css_theme_input: String::from(
                ":root {\n  --background-primary: #141619;\n  --accent: #3ba55d;\n}",
            ),
            theme_import_status: None,
            custom_plugin_input: String::from(
                "definePlugin({ name: \"CustomEcho\", onBeforeSend: function(s) { return s; } });",
            ),
            custom_plugin_status: None,
            new_channel_input: String::new(),
            audio_devices: AudioDeviceList::default(),
            audio_settings: AudioSettings::default(),
            mic_monitor: None,
            audio_status: None,
            active_voice_channel: None,
            screen_sharing: false,
            last_screen_frame: None,
            replying_to: None,
            editing_message: None,
            show_pinned_only: false,
            revealed_spoilers: HashSet::new(),
            image_cache: ImageCache::new(IMAGE_CACHE_MAX_ITEMS, IMAGE_CACHE_MAX_BYTES),
            timeline: VirtualTimeline::new(14.0),
            timeline_scroll_id: scrollable::Id::unique(),
            scroll_y: 0.0,
            viewport_height: 680.0,
            composer_input: String::new(),
            search_query: String::new(),
            show_settings: false,
        };
        app.rebuild_timeline();
        let scroll_task = app.scroll_to_bottom_task();

        (app, scroll_task)
    }

    fn theme(&self) -> Theme {
        Palette::iced_theme(self.state.settings.theme)
    }

    fn row_spacing(&self) -> f32 {
        if self.state.settings.compact_mode {
            6.0
        } else {
            14.0
        }
    }

    fn rebuild_timeline(&mut self) {
        let spacing = self.row_spacing();
        let font_size = self.state.settings.font_size;
        let compact = self.state.settings.compact_mode;
        if let Some(msgs) = self.state.current_messages() {
            self.timeline
                .rebuild_with_layout(msgs.iter(), 720.0, font_size, compact, spacing);
        } else {
            self.timeline.rebuild(std::iter::empty(), spacing);
        }
    }

    fn scroll_to_bottom_task(&mut self) -> iced::Task<UiMessage> {
        let target_y = (self.timeline.total_height() - self.viewport_height).max(0.0);
        self.scroll_y = target_y;
        scrollable::scroll_to(
            self.timeline_scroll_id.clone(),
            scrollable::AbsoluteOffset {
                x: 0.0,
                y: target_y,
            },
        )
    }

    fn persist_settings(&self) {
        if let Some(storage) = &self.storage {
            if let Err(e) = storage.save_settings(&self.state.settings) {
                warn!("Failed to persist settings: {e}");
            }
        }
    }

    fn update(&mut self, message: UiMessage) -> iced::Task<UiMessage> {
        match message {
            UiMessage::SelectGuild(gid) => {
                self.state.select_guild(gid);
                self.show_settings = false;
                self.composer_input = self
                    .state
                    .composer_drafts
                    .get(&self.state.selected_channel)
                    .cloned()
                    .unwrap_or_default();
                self.rebuild_timeline();
                return self.scroll_to_bottom_task();
            }
            UiMessage::SelectChannel(cid) => {
                self.state
                    .composer_drafts
                    .insert(self.state.selected_channel, self.composer_input.clone());
                self.state.select_channel(cid);
                self.show_settings = false;
                self.composer_input = self
                    .state
                    .composer_drafts
                    .get(&cid)
                    .cloned()
                    .unwrap_or_default();
                self.rebuild_timeline();
                let scroll_task = self.scroll_to_bottom_task();
                if self.live_mode {
                    if let Some(client) = self.discord.clone() {
                        let fetch_task = iced::Task::perform(
                            async move {
                                let msgs = client
                                    .get_channel_messages(cid.0, 50)
                                    .await
                                    .map_err(|e| e.to_string())?;
                                Ok(msgs
                                    .into_iter()
                                    .rev()
                                    .map(|m| (m.author.to_zero_user(), m.to_zero_message()))
                                    .collect())
                            },
                            move |res| UiMessage::ChannelMessagesFetched(cid, res),
                        );
                        return iced::Task::batch([scroll_task, fetch_task]);
                    }
                }
                return scroll_task;
            }
            UiMessage::ComposerChanged(val) => {
                self.composer_input = val;
            }
            UiMessage::SendMessage => {
                let raw_trimmed = self.composer_input.trim();
                let after_cmd = if let Some(rest) = raw_trimmed.strip_prefix('/') {
                    let mut parts = rest.splitn(2, ' ');
                    let cmd = parts.next().unwrap_or("");
                    let arg = parts.next().unwrap_or("");
                    self.plugin_manager
                        .dispatch_command(cmd, arg)
                        .unwrap_or_else(|| self.composer_input.clone())
                } else {
                    self.composer_input.clone()
                };
                let transformed = self.plugin_manager.dispatch_before_send(&after_cmd);
                if self.live_mode {
                    let content = transformed.trim().to_string();
                    if content.is_empty() {
                        return iced::Task::none();
                    }
                    let cid = self.state.selected_channel;
                    if let Some(client) = self.discord.clone() {
                        self.composer_input.clear();
                        return iced::Task::perform(
                            async move {
                                let msg = client
                                    .create_message(cid.0, &content)
                                    .await
                                    .map_err(|e| e.to_string())?;
                                Ok((msg.author.to_zero_user(), msg.to_zero_message()))
                            },
                            move |res| UiMessage::LiveMessageSent(cid, res),
                        );
                    }
                }
                let anchor = self
                    .timeline
                    .capture_anchor(self.scroll_y, self.viewport_height);
                let reply_to = self.replying_to.take();
                if let Some(msg) = self.state.send_reply(&transformed, reply_to) {
                    self.plugin_manager
                        .emit_event("MESSAGE_CREATE", "{\"local\":true}");
                    if let Some(storage) = &self.storage {
                        if let Err(e) = storage.save_message(&msg) {
                            warn!("Failed to persist message: {e}");
                        }
                    }
                    let h = estimate_message_height(
                        &msg,
                        720.0,
                        self.state.settings.font_size,
                        self.state.settings.compact_mode,
                    );
                    self.timeline.push_height(h);
                    self.composer_input.clear();

                    let next_y = self.timeline.resolve_anchor(anchor, self.viewport_height);
                    self.scroll_y = next_y;
                    return scrollable::scroll_to(
                        self.timeline_scroll_id.clone(),
                        scrollable::AbsoluteOffset { x: 0.0, y: next_y },
                    );
                }
            }
            UiMessage::ToggleReaction(msg_id, emoji) => {
                self.state.toggle_reaction(msg_id, &emoji);
                if let Some(msgs) = self.state.current_messages() {
                    if let Some((idx, msg)) = msgs.iter().enumerate().find(|(_, m)| m.id == msg_id)
                    {
                        let h = estimate_message_height(
                            msg,
                            720.0,
                            self.state.settings.font_size,
                            self.state.settings.compact_mode,
                        );
                        self.timeline.update_height(idx, h);
                    }
                }
            }
            UiMessage::SearchChanged(q) => {
                self.search_query = q;
            }
            UiMessage::TimelineScrolled(y, height) => {
                self.scroll_y = y;
                if height > 1.0 {
                    self.viewport_height = height;
                }
            }
            UiMessage::ToggleMemberList => {
                self.state.settings.show_member_list = !self.state.settings.show_member_list;
                self.persist_settings();
            }
            UiMessage::ToggleSettings => {
                self.show_settings = !self.show_settings;
                if self.show_settings
                    && self.audio_devices.inputs.is_empty()
                    && self.audio_devices.outputs.is_empty()
                {
                    self.audio_devices = enumerate_audio_devices();
                }
            }
            UiMessage::SetTheme(mode) => {
                self.state.settings.theme = mode;
                let tid = match mode {
                    ThemeMode::Dark => "zero-dark",
                    ThemeMode::Light => "zero-light",
                };
                let _ = self.theme_manager.set_active(tid);
                self.persist_settings();
            }
            UiMessage::ToggleCompactMode(v) => {
                self.state.settings.compact_mode = v;
                self.persist_settings();
                self.rebuild_timeline();
            }
            UiMessage::ToggleNotifications(v) => {
                self.state.settings.notifications_enabled = v;
                self.persist_settings();
            }
            UiMessage::SetFontSize(sz) => {
                self.state.settings.font_size = sz;
                self.persist_settings();
                self.rebuild_timeline();
            }
            UiMessage::LoadSyntheticMessages(count) => {
                let cid = self.state.selected_channel;
                self.state.populate_synthetic_messages(cid, count);
                self.rebuild_timeline();
                self.show_settings = false;
                return self.scroll_to_bottom_task();
            }
            UiMessage::TestDiscordGateway => {
                if let Some(client) = self.discord.clone() {
                    self.gateway_status = Some("Testing GET /api/v10/gateway...".into());
                    return iced::Task::perform(
                        async move {
                            let info = client.get_gateway().await.map_err(|e| e.to_string())?;
                            let lat = client
                                .diagnostics()
                                .last_latency_ms
                                .map(|ms| format!("{ms} ms"))
                                .unwrap_or_else(|| "ok".into());
                            Ok(format!("{} ({lat})", info.url))
                        },
                        UiMessage::GatewayTestFinished,
                    );
                }
            }
            UiMessage::GatewayTestFinished(res) => {
                self.gateway_status = Some(match res {
                    Ok(ok) => format!("Live Gateway OK: {ok}"),
                    Err(e) => format!("Gateway Error: {e}"),
                });
            }
            UiMessage::BotTokenChanged(tok) => {
                self.bot_token_input = tok;
            }
            UiMessage::ConnectBot => {
                let token = self.bot_token_input.trim().to_string();
                if token.is_empty() {
                    self.gateway_status = Some("Enter a Discord Bot token first.".into());
                    return iced::Task::none();
                }
                if let Err(e) = self.cred_store.set_secret("bot_token", &token) {
                    warn!("Could not save token to OS keyring: {e}");
                }
                if let Some(client) = &mut self.discord {
                    client.set_bot_token(Some(token));
                }
                if let Some(client) = self.discord.clone() {
                    self.gateway_status = Some("Connecting authorized bot...".into());
                    return iced::Task::perform(
                        async move {
                            let me = client.get_current_user().await.map_err(|e| e.to_string())?;
                            let raw_guilds = client
                                .get_current_user_guilds()
                                .await
                                .map_err(|e| e.to_string())?;
                            let mut guilds = Vec::new();
                            for g in raw_guilds.into_iter().take(15) {
                                let gid = g.id.parse::<u64>().unwrap_or(0);
                                let chans = client
                                    .get_guild_channels(gid)
                                    .await
                                    .unwrap_or_default()
                                    .into_iter()
                                    .filter_map(|c| c.to_zero_channel())
                                    .collect();
                                guilds.push(g.to_zero_guild(chans));
                            }
                            Ok(Box::new(BotBootstrap {
                                current_user: me.to_zero_user(),
                                guilds,
                            }))
                        },
                        UiMessage::BotConnected,
                    );
                }
            }
            UiMessage::BotConnected(res) => match res {
                Ok(boot) => {
                    self.state.current_user = boot.current_user.clone();
                    self.state
                        .users
                        .insert(boot.current_user.id, boot.current_user);
                    self.state.guilds = boot.guilds;
                    self.state.dm_channels.clear();
                    self.state.messages_by_channel.clear();
                    self.live_mode = true;
                    let first_gid = self.state.guilds.first().map(|g| g.id);
                    self.state.select_guild(first_gid);
                    self.gateway_status = Some(format!(
                        "Connected as {} ({} servers)",
                        self.state.current_user.username,
                        self.state.guilds.len()
                    ));
                    self.rebuild_timeline();
                }
                Err(e) => {
                    self.gateway_status = Some(format!("Bot connection failed: {e}"));
                }
            },
            UiMessage::DisconnectBot => {
                let _ = self.cred_store.delete_secret("bot_token");
                self.bot_token_input.clear();
                if let Some(client) = &mut self.discord {
                    client.set_bot_token(None);
                }
                let saved_settings = self.state.settings.clone();
                self.state = ZeroState::new_with_demo_data();
                self.state.settings = saved_settings;
                self.live_mode = false;
                self.gateway_status = Some("Disconnected; returned to local demo mode.".into());
                self.rebuild_timeline();
                return self.scroll_to_bottom_task();
            }
            UiMessage::ChannelMessagesFetched(cid, res) => {
                if let Ok(items) = res {
                    let mut queue = VecDeque::with_capacity(items.len());
                    for (user, msg) in items {
                        self.state.users.insert(user.id, user);
                        queue.push_back(msg);
                    }
                    self.state.messages_by_channel.insert(cid, queue);
                    if self.state.selected_channel == cid {
                        self.rebuild_timeline();
                        return self.scroll_to_bottom_task();
                    }
                }
            }
            UiMessage::LiveMessageSent(cid, res) => match res {
                Ok((user, msg)) => {
                    self.state.users.insert(user.id, user);
                    self.state
                        .messages_by_channel
                        .entry(cid)
                        .or_default()
                        .push_back(msg);
                    if self.state.selected_channel == cid {
                        self.rebuild_timeline();
                        return self.scroll_to_bottom_task();
                    }
                }
                Err(e) => {
                    self.gateway_status = Some(format!("Send failed: {e}"));
                }
            },
            UiMessage::SelectThemeId(id) => {
                if self.theme_manager.set_active(&id).is_ok() {
                    let t = self.theme_manager.active_theme();
                    self.state.settings.theme = t.base_mode;
                    self.state.settings.font_size = t.tokens.font_size;
                    self.persist_settings();
                    self.rebuild_timeline();
                }
            }
            UiMessage::CssThemeInputChanged(css) => {
                self.css_theme_input = css;
            }
            UiMessage::ImportCssTheme => {
                let report = parse_css_theme(
                    "custom-css",
                    "Custom CSS Theme",
                    "User",
                    &self.css_theme_input,
                );
                let msg = if report.warnings.is_empty() {
                    format!(
                        "Imported {} CSS variables cleanly.",
                        report.mapped_variables
                    )
                } else {
                    format!(
                        "Imported {} vars ({} unsupported warnings: {})",
                        report.mapped_variables,
                        report.warnings.len(),
                        report.warnings[0]
                    )
                };
                self.theme_manager.install_theme(report.theme);
                let _ = self.theme_manager.set_active("custom-css");
                self.theme_import_status = Some(msg);
            }
            UiMessage::TogglePlugin(id) => {
                let is_active = self
                    .plugin_manager
                    .plugins()
                    .iter()
                    .find(|p| p.manifest.id == id)
                    .is_some_and(|p| p.state == PluginState::Active);
                if is_active {
                    let _ = self.plugin_manager.disable_plugin(&id);
                } else {
                    let _ = self.plugin_manager.enable_plugin(&id);
                }
            }
            UiMessage::TogglePluginSetting(id, key, val) => {
                let _ =
                    self.plugin_manager
                        .set_plugin_setting(&id, &key, SettingValue::Boolean(val));
            }
            UiMessage::RefreshAudioDevices => {
                self.audio_devices = enumerate_audio_devices();
                self.audio_status = Some(format!(
                    "Found {} input(s), {} output(s)",
                    self.audio_devices.inputs.len(),
                    self.audio_devices.outputs.len()
                ));
            }
            UiMessage::ToggleMicMute => {
                self.audio_settings.mic_muted = !self.audio_settings.mic_muted;
            }
            UiMessage::ToggleDeafen => {
                self.audio_settings.deafened = !self.audio_settings.deafened;
            }
            UiMessage::TogglePushToTalk(v) => {
                self.audio_settings.push_to_talk = v;
            }
            UiMessage::TestOutputChime => {
                let dev = self.audio_settings.output_device.clone();
                let vol = self.audio_settings.output_volume;
                self.audio_status = Some("Playing 440 Hz test chime...".into());
                return iced::Task::perform(
                    async move {
                        std::thread::spawn(move || play_test_chime(dev, vol))
                            .join()
                            .map_err(|_| "Audio thread panicked".to_string())?
                            .map_err(|e| e.to_string())
                    },
                    UiMessage::OutputChimeFinished,
                );
            }
            UiMessage::OutputChimeFinished(res) => {
                self.audio_status = Some(match res {
                    Ok(()) => "Output test chime completed.".into(),
                    Err(e) => format!("Audio output error: {e}"),
                });
            }
            UiMessage::ToggleMicMonitor => {
                if self.mic_monitor.is_some() {
                    self.mic_monitor = None;
                    self.audio_status = Some("Microphone monitor stopped.".into());
                } else {
                    match MicMonitor::start(self.audio_settings.input_device.as_deref()) {
                        Ok(mon) => {
                            self.mic_monitor = Some(mon);
                            self.audio_status = Some("Microphone monitor active.".into());
                        }
                        Err(e) => {
                            self.audio_status = Some(format!("Mic monitor error: {e}"));
                        }
                    }
                }
            }
            UiMessage::JoinVoiceChannel(cid, name) => {
                self.active_voice_channel = Some((cid, name.clone()));
                self.audio_status = Some(format!("Connected to local voice channel: {name}"));
            }
            UiMessage::LeaveVoiceChannel => {
                self.active_voice_channel = None;
                self.screen_sharing = false;
                self.last_screen_frame = None;
                self.audio_status = Some("Disconnected from voice channel.".into());
            }
            UiMessage::ToggleScreenShare => {
                if self.screen_sharing {
                    self.screen_sharing = false;
                    self.last_screen_frame = None;
                } else {
                    self.screen_sharing = true;
                    return iced::Task::perform(
                        async move {
                            std::thread::spawn(capture_primary_screen)
                                .join()
                                .map_err(|_| "Capture thread panicked".to_string())?
                                .map_err(|e| e.to_string())
                        },
                        UiMessage::ScreenShareCaptured,
                    );
                }
            }
            UiMessage::ScreenShareCaptured(res) => match res {
                Ok(frame) => {
                    self.last_screen_frame = Some(frame);
                }
                Err(e) => {
                    self.screen_sharing = false;
                    self.audio_status = Some(format!("Screen share error: {e}"));
                }
            },
            UiMessage::StartReply(mid) => {
                self.replying_to = Some(mid);
            }
            UiMessage::CancelReply => {
                self.replying_to = None;
            }
            UiMessage::StartEdit(mid, current_text) => {
                self.editing_message = Some((mid, current_text));
            }
            UiMessage::EditInputChanged(val) => {
                if let Some((_, ref mut text_buf)) = self.editing_message {
                    *text_buf = val;
                }
            }
            UiMessage::SaveEdit => {
                if let Some((mid, new_text)) = self.editing_message.take() {
                    if self.state.edit_message(mid, &new_text) {
                        self.rebuild_timeline();
                    }
                }
            }
            UiMessage::CancelEdit => {
                self.editing_message = None;
            }
            UiMessage::DeleteMessage(mid) => {
                if self.state.delete_message(mid) {
                    self.rebuild_timeline();
                }
            }
            UiMessage::TogglePin(mid) => {
                if self.state.toggle_pin(mid) {
                    self.rebuild_timeline();
                }
            }
            UiMessage::TogglePinnedView => {
                self.show_pinned_only = !self.show_pinned_only;
            }
            UiMessage::RevealSpoiler(mid) => {
                self.revealed_spoilers.insert(mid);
            }
            UiMessage::NewChannelInputChanged(val) => {
                self.new_channel_input = val;
            }
            UiMessage::CreateLocalChannel => {
                if self
                    .state
                    .create_local_channel(&self.new_channel_input)
                    .is_some()
                {
                    self.new_channel_input.clear();
                    self.rebuild_timeline();
                    return self.scroll_to_bottom_task();
                }
            }
            UiMessage::CustomPluginInputChanged(val) => {
                self.custom_plugin_input = val;
            }
            UiMessage::ImportCustomPlugin => {
                let id = format!("custom.plugin.{}", self.plugin_manager.plugins().len() + 1);
                let name = format!("Custom Plugin #{}", self.plugin_manager.plugins().len() + 1);
                let analysis = install_custom_vencord_plugin(
                    &mut self.plugin_manager,
                    id,
                    name,
                    &self.custom_plugin_input,
                );
                self.custom_plugin_status =
                    Some(format!("{} — {}", analysis.status.label(), analysis.reason));
            }
        }
        iced::Task::none()
    }

    fn view(&self) -> Element<'_, UiMessage> {
        let active_theme = self.theme_manager.active_theme();
        let p = Palette::from_tokens(&active_theme.tokens, active_theme.base_mode);

        if self.show_settings {
            return self.view_settings(p);
        }

        let server_rail = self.view_server_rail(p);
        let channel_sidebar = self.view_channel_sidebar(p);
        let main_area = self.view_main_chat(p);

        let mut layout = row![server_rail, channel_sidebar, main_area]
            .width(Length::Fill)
            .height(Length::Fill);

        if self.state.settings.show_member_list {
            layout = layout.push(self.view_member_sidebar(p));
        }

        container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_server_rail(&self, p: Palette) -> Element<'_, UiMessage> {
        let is_home = self.state.selected_guild.is_none();
        let home_btn = button(
            container(text("DM").size(14))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .width(46)
        .height(46)
        .style(move |_, status| {
            let active = is_home || matches!(status, button::Status::Hovered);
            button::Style {
                background: Some(Background::Color(if active { p.accent } else { p.bg_chat })),
                text_color: if active { Color::WHITE } else { p.text_primary },
                border: Border {
                    radius: if is_home { 14.0 } else { 23.0 }.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .on_press(UiMessage::SelectGuild(None));

        let mut rail = column![home_btn, horizontal_rule(1)]
            .spacing(10)
            .align_x(Alignment::Center);

        for guild in &self.state.guilds {
            let selected = self.state.selected_guild == Some(guild.id);
            let accent = Color::from_rgb8(
                guild.accent_color[0],
                guild.accent_color[1],
                guild.accent_color[2],
            );
            let total_unread: u32 = guild.channels.iter().map(|c| c.unread_count).sum();

            let label = if total_unread > 0 && !selected {
                format!("{}*", guild.short_name)
            } else {
                guild.short_name.clone()
            };

            let btn = button(
                container(text(label).size(14))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            )
            .width(46)
            .height(46)
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if selected || hovered {
                        accent
                    } else {
                        p.bg_chat
                    })),
                    text_color: if selected || hovered {
                        Color::WHITE
                    } else {
                        p.text_primary
                    },
                    border: Border {
                        radius: if selected { 14.0 } else { 23.0 }.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(UiMessage::SelectGuild(Some(guild.id)));

            rail = rail.push(btn);
        }

        let settings_btn = button(
            container(text("⚙").size(16))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .width(42)
        .height(42)
        .style(move |_, status| button::Style {
            background: Some(Background::Color(
                if matches!(status, button::Status::Hovered) {
                    p.bg_hover
                } else {
                    p.bg_chat
                },
            )),
            text_color: p.text_secondary,
            border: Border {
                radius: 21.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .on_press(UiMessage::ToggleSettings);

        rail = rail
            .push(Space::with_height(Length::Fill))
            .push(settings_btn);

        container(rail)
            .width(68)
            .height(Length::Fill)
            .padding(Padding::from([12, 10]))
            .style(move |_| container::Style {
                background: Some(Background::Color(p.bg_server_rail)),
                ..Default::default()
            })
            .into()
    }

    fn view_channel_sidebar(&self, p: Palette) -> Element<'_, UiMessage> {
        let header_title = self
            .state
            .current_guild()
            .map(|g| g.name.as_str())
            .unwrap_or("Direct Messages");

        let header = container(text(header_title).size(15).color(p.text_primary))
            .width(Length::Fill)
            .padding(Padding::from([12, 14]))
            .style(move |_| container::Style {
                border: Border {
                    width: 1.0,
                    color: p.border,
                    ..Default::default()
                },
                ..Default::default()
            });

        let mut list_col = column![].spacing(3).padding(Padding::from([8, 8]));

        if let Some(guild) = self.state.current_guild() {
            let mut last_cat: Option<&str> = None;
            for ch in &guild.channels {
                if ch.category.as_deref() != last_cat {
                    last_cat = ch.category.as_deref();
                    if let Some(cat) = last_cat {
                        list_col = list_col.push(
                            container(text(cat).size(11).color(p.text_muted))
                                .padding(Padding::from([8, 6])),
                        );
                    }
                }
                list_col = list_col.push(self.view_channel_row(p, ch));
            }
            list_col = list_col.push(
                container(
                    row![
                        text_input("+ new-channel", &self.new_channel_input)
                            .on_input(UiMessage::NewChannelInputChanged)
                            .on_submit(UiMessage::CreateLocalChannel)
                            .padding(Padding::from([4, 8]))
                            .size(12)
                            .width(Length::Fill),
                        button(text("+").size(12))
                            .padding(Padding::from([4, 8]))
                            .on_press(UiMessage::CreateLocalChannel),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                )
                .padding(Padding::from([8, 2])),
            );
        } else {
            list_col = list_col.push(
                container(text("DIRECT MESSAGES").size(11).color(p.text_muted))
                    .padding(Padding::from([8, 6])),
            );
            for ch in &self.state.dm_channels {
                list_col = list_col.push(self.view_channel_row(p, ch));
            }
        }

        let user = &self.state.current_user;
        let status_color = match user.status {
            UserStatus::Online => p.online,
            UserStatus::Idle => p.idle,
            UserStatus::DoNotDisturb => p.dnd,
            UserStatus::Offline => p.offline,
        };

        let user_panel = container(
            row![
                container(text(user.initials()).size(12).color(Color::WHITE))
                    .width(32)
                    .height(32)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center)
                    .style(move |_| container::Style {
                        background: Some(Background::Color(Color::from_rgb8(
                            user.avatar_color[0],
                            user.avatar_color[1],
                            user.avatar_color[2],
                        ))),
                        border: Border {
                            radius: 16.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                column![
                    text(&user.display_name).size(13).color(p.text_primary),
                    row![
                        container(Space::new(8, 8)).style(move |_| container::Style {
                            background: Some(Background::Color(status_color)),
                            border: Border {
                                radius: 4.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                        text(user.status.label()).size(11).color(p.text_muted),
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center)
                ]
                .spacing(2)
                .width(Length::Fill),
                button(
                    text(if self.audio_settings.mic_muted {
                        "🔇"
                    } else {
                        "🎙"
                    })
                    .size(13)
                )
                .padding(Padding::from([4, 6]))
                .style(move |_, status| button::Style {
                    background: Some(Background::Color(
                        if matches!(status, button::Status::Hovered) {
                            p.bg_hover
                        } else {
                            Color::TRANSPARENT
                        },
                    )),
                    text_color: if self.audio_settings.mic_muted {
                        p.unread_badge
                    } else {
                        p.text_secondary
                    },
                    border: Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .on_press(UiMessage::ToggleMicMute),
                button(
                    text(if self.audio_settings.deafened {
                        "🔕"
                    } else {
                        "🎧"
                    })
                    .size(13)
                )
                .padding(Padding::from([4, 6]))
                .style(move |_, status| button::Style {
                    background: Some(Background::Color(
                        if matches!(status, button::Status::Hovered) {
                            p.bg_hover
                        } else {
                            Color::TRANSPARENT
                        },
                    )),
                    text_color: if self.audio_settings.deafened {
                        p.unread_badge
                    } else {
                        p.text_secondary
                    },
                    border: Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .on_press(UiMessage::ToggleDeafen),
                button(text("⚙").size(14))
                    .padding(Padding::from([4, 8]))
                    .style(move |_, status| button::Style {
                        background: Some(Background::Color(
                            if matches!(status, button::Status::Hovered) {
                                p.bg_hover
                            } else {
                                Color::TRANSPARENT
                            },
                        )),
                        text_color: p.text_secondary,
                        border: Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .on_press(UiMessage::ToggleSettings)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(Padding::from([10, 12]))
        .style(move |_| container::Style {
            background: Some(Background::Color(p.bg_user_panel)),
            ..Default::default()
        });

        let mut sidebar_col = column![header, scrollable(list_col).height(Length::Fill),]
            .width(Length::Fill)
            .height(Length::Fill);

        if let Some((_, vc_name)) = &self.active_voice_channel {
            let share_status = if let Some(frame) = &self.last_screen_frame {
                format!(
                    "Sharing {}x{} ({}ms)",
                    frame.width, frame.height, frame.capture_ms
                )
            } else if self.screen_sharing {
                "Capturing display...".into()
            } else {
                "Local Voice Ready".into()
            };

            let voice_panel = container(
                column![
                    row![
                        column![
                            text("Voice Connected").size(12).color(p.online),
                            text(vc_name).size(11).color(p.text_muted),
                        ]
                        .spacing(1)
                        .width(Length::Fill),
                        button(
                            text(if self.screen_sharing {
                                "Stop Share"
                            } else {
                                "Share Screen"
                            })
                            .size(11)
                        )
                        .padding(Padding::from([4, 8]))
                        .on_press(UiMessage::ToggleScreenShare),
                        button(text("Leave").size(11))
                            .padding(Padding::from([4, 8]))
                            .on_press(UiMessage::LeaveVoiceChannel),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                    text(share_status).size(10).color(p.text_secondary),
                ]
                .spacing(4),
            )
            .width(Length::Fill)
            .padding(Padding::from([8, 10]))
            .style(move |_| container::Style {
                background: Some(Background::Color(p.bg_user_panel)),
                border: Border {
                    width: 1.0,
                    color: p.border,
                    ..Default::default()
                },
                ..Default::default()
            });

            sidebar_col = sidebar_col.push(voice_panel);
        }

        sidebar_col = sidebar_col.push(user_panel);

        container(sidebar_col)
            .width(236)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(p.bg_channel_rail)),
                ..Default::default()
            })
            .into()
    }

    fn view_channel_row<'a>(&self, p: Palette, ch: &'a Channel) -> Element<'a, UiMessage> {
        let selected = self.state.selected_channel == ch.id;
        let prefix = match ch.kind {
            ChannelKind::Text => "#",
            ChannelKind::Voice => "🔊",
            ChannelKind::Announcement => "📢",
            ChannelKind::DirectMessage => "@",
        };

        let mut row_content = row![
            text(prefix).size(14).color(p.text_muted),
            text(&ch.name)
                .size(14)
                .color(if selected || ch.unread_count > 0 {
                    p.text_primary
                } else {
                    p.text_secondary
                })
                .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        if ch.unread_count > 0 && !selected {
            row_content = row_content.push(
                container(
                    text(ch.unread_count.to_string())
                        .size(11)
                        .color(Color::WHITE),
                )
                .padding(Padding::from([1, 6]))
                .style(move |_| container::Style {
                    background: Some(Background::Color(p.unread_badge)),
                    border: Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            );
        }

        let on_click = if ch.kind == ChannelKind::Voice {
            UiMessage::JoinVoiceChannel(ch.id, ch.name.clone())
        } else {
            UiMessage::SelectChannel(ch.id)
        };

        button(row_content)
            .width(Length::Fill)
            .padding(Padding::from([6, 8]))
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered);
                button::Style {
                    background: Some(Background::Color(if selected {
                        p.bg_selected
                    } else if hovered {
                        p.bg_hover
                    } else {
                        Color::TRANSPARENT
                    })),
                    text_color: p.text_primary,
                    border: Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })
            .on_press(on_click)
            .into()
    }

    fn view_message_row<'a>(&'a self, p: Palette, msg: &'a Message) -> Element<'a, UiMessage> {
        let font_sz = self.state.settings.font_size;
        let author = self.state.users.get(&msg.author_id);
        let author_name = author
            .map(|u| u.display_name.as_str())
            .unwrap_or("Unknown User");
        let initials = author.map(|u| u.initials()).unwrap_or_else(|| "??".into());
        let avatar_rgb = author.map(|u| u.avatar_color).unwrap_or([120, 120, 120]);
        let ts = msg
            .timestamp
            .with_timezone(&chrono::Local)
            .format("%H:%M")
            .to_string();

        let mut reactions_row = row![].spacing(6).align_y(Alignment::Center);
        for r in &msg.reactions {
            let emoji = r.emoji.clone();
            let active = r.me;
            reactions_row = reactions_row.push(
                button(
                    text(format!("{} {}", r.emoji, r.count))
                        .size(12)
                        .color(if active {
                            Color::WHITE
                        } else {
                            p.text_secondary
                        }),
                )
                .padding(Padding::from([2, 7]))
                .style(move |_, _| button::Style {
                    background: Some(Background::Color(if active {
                        p.accent
                    } else {
                        p.bg_composer
                    })),
                    text_color: p.text_primary,
                    border: Border {
                        radius: 10.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .on_press(UiMessage::ToggleReaction(msg.id, emoji)),
            );
        }
        reactions_row = reactions_row.push(
            button(text("+👍").size(11).color(p.text_muted))
                .padding(Padding::from([2, 6]))
                .style(move |_, status| button::Style {
                    background: Some(Background::Color(
                        if matches!(status, button::Status::Hovered) {
                            p.bg_hover
                        } else {
                            Color::TRANSPARENT
                        },
                    )),
                    text_color: p.text_muted,
                    border: Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .on_press(UiMessage::ToggleReaction(msg.id, "👍".into())),
        );

        let avatar = container(text(initials).size(12).color(Color::WHITE))
            .width(36)
            .height(36)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .style(move |_| container::Style {
                background: Some(Background::Color(Color::from_rgb8(
                    avatar_rgb[0],
                    avatar_rgb[1],
                    avatar_rgb[2],
                ))),
                border: Border {
                    radius: 18.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            });

        let is_editing = self
            .editing_message
            .as_ref()
            .is_some_and(|(id, _)| *id == msg.id);

        let content_view: Element<'a, UiMessage> = if let Some((_edit_id, edit_buf)) = self
            .editing_message
            .as_ref()
            .filter(|(id, _)| *id == msg.id)
        {
            row![
                text_input("Edit message...", edit_buf)
                    .on_input(UiMessage::EditInputChanged)
                    .on_submit(UiMessage::SaveEdit)
                    .size(font_sz)
                    .padding(Padding::from([4, 8]))
                    .width(Length::Fill),
                button(text("Save").size(11).color(Color::WHITE))
                    .padding(Padding::from([4, 8]))
                    .on_press(UiMessage::SaveEdit),
                button(text("Cancel").size(11))
                    .padding(Padding::from([4, 8]))
                    .on_press(UiMessage::CancelEdit),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into()
        } else {
            let _ = is_editing;
            let segments = parse_message_segments(&msg.content);
            if segments.len() == 1 {
                if let MessageSegment::Text(t) = &segments[0] {
                    text(t.clone()).size(font_sz).color(p.text_secondary).into()
                } else {
                    self.render_segments(p, msg.id, segments, font_sz)
                }
            } else {
                self.render_segments(p, msg.id, segments, font_sz)
            }
        };

        let mut header_row = row![
            text(author_name).size(14).color(p.text_primary),
            text(ts).size(11).color(p.text_muted),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        if msg.edited {
            header_row = header_row.push(text("(edited)").size(10).color(p.text_muted));
        }
        if msg.pinned {
            header_row = header_row.push(text("📌").size(11).color(p.accent));
        }

        header_row = header_row
            .push(Space::with_width(Length::Fill))
            .push(
                button(text("Reply").size(10).color(p.text_muted))
                    .padding(Padding::from([1, 5]))
                    .style(move |_, _| button::Style {
                        background: None,
                        text_color: p.text_muted,
                        ..Default::default()
                    })
                    .on_press(UiMessage::StartReply(msg.id)),
            )
            .push(
                button(
                    text(if msg.pinned { "Unpin" } else { "Pin" })
                        .size(10)
                        .color(p.text_muted),
                )
                .padding(Padding::from([1, 5]))
                .style(move |_, _| button::Style {
                    background: None,
                    text_color: p.text_muted,
                    ..Default::default()
                })
                .on_press(UiMessage::TogglePin(msg.id)),
            );

        if msg.author_id == self.state.current_user.id {
            header_row = header_row
                .push(
                    button(text("Edit").size(10).color(p.text_muted))
                        .padding(Padding::from([1, 5]))
                        .style(move |_, _| button::Style {
                            background: None,
                            text_color: p.text_muted,
                            ..Default::default()
                        })
                        .on_press(UiMessage::StartEdit(msg.id, msg.content.clone())),
                )
                .push(
                    button(text("Delete").size(10).color(p.unread_badge))
                        .padding(Padding::from([1, 5]))
                        .style(move |_, _| button::Style {
                            background: None,
                            text_color: p.unread_badge,
                            ..Default::default()
                        })
                        .on_press(UiMessage::DeleteMessage(msg.id)),
                );
        }

        let mut body_col = column![].spacing(3).width(Length::Fill);
        if let Some(reply_id) = msg.reply_to {
            let reply_preview = self
                .state
                .current_messages()
                .and_then(|q| q.iter().find(|m| m.id == reply_id))
                .map(|rm| {
                    let rname = self
                        .state
                        .users
                        .get(&rm.author_id)
                        .map(|u| u.display_name.as_str())
                        .unwrap_or("User");
                    format!(
                        "↳ Replying to {rname}: {}",
                        rm.content.chars().take(48).collect::<String>()
                    )
                })
                .unwrap_or_else(|| "↳ Replying to message".into());
            body_col = body_col.push(text(reply_preview).size(11).color(p.text_muted));
        }

        let body = body_col
            .push(header_row)
            .push(content_view)
            .push(reactions_row);

        row![avatar, body]
            .spacing(12)
            .align_y(Alignment::Start)
            .width(Length::Fill)
            .into()
    }

    fn render_segments<'a>(
        &'a self,
        p: Palette,
        msg_id: MessageId,
        segments: Vec<MessageSegment>,
        font_sz: f32,
    ) -> Element<'a, UiMessage> {
        let revealed = self.revealed_spoilers.contains(&msg_id);
        let mut col = column![].spacing(4);
        let mut inline_row = row![].spacing(4).align_y(Alignment::Center);
        let mut has_inline = false;

        for seg in segments {
            match seg {
                MessageSegment::CodeBlock(code) => {
                    if has_inline {
                        col = col.push(inline_row);
                        inline_row = row![].spacing(4).align_y(Alignment::Center);
                        has_inline = false;
                    }
                    col = col.push(
                        container(
                            text(code)
                                .size((font_sz - 1.0).max(11.0))
                                .color(p.text_primary),
                        )
                        .padding(8)
                        .width(Length::Fill)
                        .style(move |_| container::Style {
                            background: Some(Background::Color(p.bg_server_rail)),
                            border: Border {
                                radius: 6.0.into(),
                                width: 1.0,
                                color: p.border,
                            },
                            ..Default::default()
                        }),
                    );
                }
                MessageSegment::Text(t) => {
                    inline_row = inline_row.push(text(t).size(font_sz).color(p.text_secondary));
                    has_inline = true;
                }
                MessageSegment::InlineCode(c) => {
                    inline_row = inline_row.push(
                        container(
                            text(c)
                                .size((font_sz - 1.0).max(11.0))
                                .color(p.text_primary),
                        )
                        .padding(Padding::from([1, 5]))
                        .style(move |_| container::Style {
                            background: Some(Background::Color(p.bg_server_rail)),
                            border: Border {
                                radius: 4.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                    );
                    has_inline = true;
                }
                MessageSegment::Mention(m) => {
                    inline_row = inline_row.push(
                        container(text(m).size(font_sz).color(Color::WHITE))
                            .padding(Padding::from([1, 4]))
                            .style(move |_| container::Style {
                                background: Some(Background::Color(p.accent)),
                                border: Border {
                                    radius: 4.0.into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }),
                    );
                    has_inline = true;
                }
                MessageSegment::Spoiler(s) => {
                    let label = if revealed {
                        s
                    } else {
                        "Spoiler (click)".to_string()
                    };
                    inline_row = inline_row.push(
                        button(text(label).size(font_sz - 1.0).color(if revealed {
                            p.text_primary
                        } else {
                            p.text_muted
                        }))
                        .padding(Padding::from([1, 6]))
                        .style(move |_, _| button::Style {
                            background: Some(Background::Color(p.bg_server_rail)),
                            text_color: p.text_primary,
                            border: Border {
                                radius: 4.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .on_press(UiMessage::RevealSpoiler(msg_id)),
                    );
                    has_inline = true;
                }
            }
        }
        if has_inline {
            col = col.push(inline_row);
        }
        col.into()
    }

    fn view_main_chat(&self, p: Palette) -> Element<'_, UiMessage> {
        let ch_name = self
            .state
            .current_channel()
            .map(|c| c.name.as_str())
            .unwrap_or("unknown");
        let ch_topic = self
            .state
            .current_channel()
            .and_then(|c| c.topic.as_deref())
            .unwrap_or("");

        let searching = !self.search_query.trim().is_empty();
        let total_msgs = self.state.current_messages().map(|q| q.len()).unwrap_or(0);

        let (rendered_count, timeline_widget): (usize, Element<'_, UiMessage>) = if self
            .show_pinned_only
        {
            let pins = self.state.pinned_messages();
            let count = pins.len();
            let mut col = column![]
                .spacing(self.row_spacing())
                .padding(Padding::from([14, 16]));
            if pins.is_empty() {
                col = col.push(
                    text("No pinned messages in this channel yet.")
                        .size(13)
                        .color(p.text_muted),
                );
            } else {
                for msg in pins {
                    col = col.push(self.view_message_row(p, msg));
                }
            }
            (
                count,
                scrollable(col)
                    .id(self.timeline_scroll_id.clone())
                    .height(Length::Fill)
                    .into(),
            )
        } else if searching {
            let hits = self.state.search_current_channel(&self.search_query);
            let count = hits.len();
            let mut col = column![]
                .spacing(self.row_spacing())
                .padding(Padding::from([14, 16]));
            if hits.is_empty() {
                col = col.push(
                    text("No matching messages in this channel.")
                        .size(13)
                        .color(p.text_muted),
                );
            } else {
                for msg in hits.into_iter().take(200) {
                    col = col.push(self.view_message_row(p, msg));
                }
            }
            (
                count,
                scrollable(col)
                    .id(self.timeline_scroll_id.clone())
                    .height(Length::Fill)
                    .into(),
            )
        } else {
            let range =
                self.timeline
                    .visible_range(self.scroll_y, self.viewport_height, TIMELINE_OVERSCAN);
            let mut col = column![]
                .spacing(self.row_spacing())
                .padding(Padding::from([14, 16]));

            if let Some(msgs) = self.state.current_messages() {
                if msgs.is_empty() {
                    col = col.push(
                        text("No messages in this channel yet. Send a message below!")
                            .size(13)
                            .color(p.text_muted),
                    );
                } else {
                    if range.top_spacer > 0.0 {
                        col = col.push(Space::with_height(range.top_spacer));
                    }
                    for idx in range.start..range.end {
                        if let Some(msg) = msgs.get(idx) {
                            col = col.push(self.view_message_row(p, msg));
                        }
                    }
                    if range.bottom_spacer > 0.0 {
                        col = col.push(Space::with_height(range.bottom_spacer));
                    }
                }
            }

            (
                range.end.saturating_sub(range.start),
                scrollable(col)
                    .id(self.timeline_scroll_id.clone())
                    .on_scroll(|vp| {
                        UiMessage::TimelineScrolled(vp.absolute_offset().y, vp.bounds().height)
                    })
                    .height(Length::Fill)
                    .into(),
            )
        };

        let top_bar = container(
            row![
                text(format!("# {ch_name}")).size(15).color(p.text_primary),
                text(ch_topic)
                    .size(12)
                    .color(p.text_muted)
                    .width(Length::Fill),
                text(format!("{rendered_count}/{total_msgs} msgs"))
                    .size(11)
                    .color(p.text_muted),
                text_input("Search channel...", &self.search_query)
                    .on_input(UiMessage::SearchChanged)
                    .width(170)
                    .padding(Padding::from([5, 10]))
                    .size(13),
                button(
                    text(if self.show_pinned_only {
                        "📌 Pinned: On"
                    } else {
                        "📌 Pinned"
                    })
                    .size(12)
                )
                .padding(Padding::from([5, 10]))
                .style(move |_, status| button::Style {
                    background: Some(Background::Color(if self.show_pinned_only {
                        p.accent
                    } else if matches!(status, button::Status::Hovered) {
                        p.bg_hover
                    } else {
                        p.bg_composer
                    })),
                    text_color: if self.show_pinned_only {
                        Color::WHITE
                    } else {
                        p.text_secondary
                    },
                    border: Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .on_press(UiMessage::TogglePinnedView),
                button(
                    text(if self.state.settings.show_member_list {
                        "Members: On"
                    } else {
                        "Members: Off"
                    })
                    .size(12)
                )
                .padding(Padding::from([5, 10]))
                .style(move |_, status| button::Style {
                    background: Some(Background::Color(
                        if matches!(status, button::Status::Hovered) {
                            p.bg_hover
                        } else {
                            p.bg_composer
                        },
                    )),
                    text_color: p.text_secondary,
                    border: Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .on_press(UiMessage::ToggleMemberList),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(Padding::from([10, 16]))
        .style(move |_| container::Style {
            border: Border {
                width: 1.0,
                color: p.border,
                ..Default::default()
            },
            ..Default::default()
        });

        let font_sz = self.state.settings.font_size;
        let composer = container(
            row![
                text_input(
                    &format!("Message #{ch_name} (local demo)"),
                    &self.composer_input
                )
                .on_input(UiMessage::ComposerChanged)
                .on_submit(UiMessage::SendMessage)
                .padding(Padding::from([10, 14]))
                .size(font_sz)
                .width(Length::Fill),
                button(text("Send").size(13).color(Color::WHITE))
                    .padding(Padding::from([10, 16]))
                    .style(move |_, _| button::Style {
                        background: Some(Background::Color(p.accent)),
                        text_color: Color::WHITE,
                        border: Border {
                            radius: 8.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .on_press(UiMessage::SendMessage),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(Padding::from([12, 16]));

        let mut composer_col = column![].spacing(6).width(Length::Fill);
        if let Some(reply_id) = self.replying_to {
            let rname = self
                .state
                .current_messages()
                .and_then(|q| q.iter().find(|m| m.id == reply_id))
                .and_then(|rm| self.state.users.get(&rm.author_id))
                .map(|u| u.display_name.as_str())
                .unwrap_or("message");
            composer_col = composer_col.push(
                container(
                    row![
                        text(format!("Replying to {rname}"))
                            .size(12)
                            .color(p.text_secondary)
                            .width(Length::Fill),
                        button(text("Cancel").size(11))
                            .padding(Padding::from([2, 8]))
                            .on_press(UiMessage::CancelReply),
                    ]
                    .align_y(Alignment::Center),
                )
                .padding(Padding::from([6, 16])),
            );
        }
        composer_col = composer_col.push(composer);

        container(
            column![top_bar, timeline_widget, composer_col]
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.bg_chat)),
            ..Default::default()
        })
        .into()
    }

    fn view_member_sidebar(&self, p: Palette) -> Element<'_, UiMessage> {
        let members = self.state.current_members();
        let mut col = column![text(format!("MEMBERS — {}", members.len()))
            .size(11)
            .color(p.text_muted)]
        .spacing(8)
        .padding(Padding::from([14, 12]));

        for member in members {
            let status_color = match member.status {
                UserStatus::Online => p.online,
                UserStatus::Idle => p.idle,
                UserStatus::DoNotDisturb => p.dnd,
                UserStatus::Offline => p.offline,
            };

            let item = row![
                container(text(member.initials()).size(11).color(Color::WHITE))
                    .width(28)
                    .height(28)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center)
                    .style(move |_| container::Style {
                        background: Some(Background::Color(Color::from_rgb8(
                            member.avatar_color[0],
                            member.avatar_color[1],
                            member.avatar_color[2],
                        ))),
                        border: Border {
                            radius: 14.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                column![
                    row![
                        text(&member.display_name).size(13).color(p.text_primary),
                        container(Space::new(7, 7)).style(move |_| container::Style {
                            background: Some(Background::Color(status_color)),
                            border: Border {
                                radius: 4.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                    if let Some(cs) = &member.custom_status {
                        text(cs).size(11).color(p.text_muted)
                    } else {
                        text(member.status.label()).size(11).color(p.text_muted)
                    }
                ]
                .spacing(1)
            ]
            .spacing(8)
            .align_y(Alignment::Center);

            col = col.push(item);
        }

        container(scrollable(col))
            .width(220)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(Background::Color(p.bg_channel_rail)),
                border: Border {
                    width: 1.0,
                    color: p.border,
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    }

    fn view_settings(&self, p: Palette) -> Element<'_, UiMessage> {
        let s = &self.state.settings;
        let cache_m = self.image_cache.metrics();
        let diag = self
            .discord
            .as_ref()
            .map(|c| c.diagnostics())
            .unwrap_or_default();

        let mut theme_buttons = row![].spacing(8).align_y(Alignment::Center);
        for t in self.theme_manager.themes() {
            let tid = t.id.clone();
            let active = self.theme_manager.active_theme_id() == t.id;
            theme_buttons = theme_buttons.push(
                button(text(&t.name).size(12))
                    .padding(Padding::from([5, 10]))
                    .style(move |_, _| button::Style {
                        background: Some(Background::Color(if active {
                            p.accent
                        } else {
                            p.bg_composer
                        })),
                        text_color: if active { Color::WHITE } else { p.text_primary },
                        border: Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .on_press(UiMessage::SelectThemeId(tid)),
            );
        }

        let mut plugin_col = column![].spacing(10);
        for plugin in self.plugin_manager.plugins() {
            let pid = plugin.manifest.id.clone();
            let is_active = plugin.state == PluginState::Active;
            let status_text = match &plugin.state {
                PluginState::Disabled => "Disabled".to_string(),
                PluginState::Active => "Active".to_string(),
                PluginState::Failed(err) => format!("Error/Blocked: {err}"),
            };
            let perms_text = if plugin.manifest.permissions.is_empty() {
                "None".to_string()
            } else {
                plugin
                    .manifest
                    .permissions
                    .iter()
                    .map(|perm| perm.label())
                    .collect::<Vec<_>>()
                    .join(", ")
            };

            let mut card = column![
                row![
                    column![
                        text(&plugin.manifest.name).size(14).color(p.text_primary),
                        text(&plugin.manifest.description)
                            .size(12)
                            .color(p.text_muted),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    button(text(if is_active { "Disable" } else { "Enable" }).size(12))
                        .padding(Padding::from([5, 12]))
                        .on_press(UiMessage::TogglePlugin(pid.clone())),
                ]
                .align_y(Alignment::Center)
                .spacing(8),
                text(format!(
                    "Compatibility: {} | State: {} | Permissions: {}",
                    plugin.manifest.compatibility.label(),
                    status_text,
                    perms_text
                ))
                .size(11)
                .color(p.text_secondary),
            ]
            .spacing(6);

            for def in &plugin.manifest.settings_schema {
                if let Some(SettingValue::Boolean(curr)) = plugin.settings.get(&def.key) {
                    let key = def.key.clone();
                    let pid_clone = pid.clone();
                    card = card.push(toggler(*curr).label(&def.description).size(16).on_toggle(
                        move |v| UiMessage::TogglePluginSetting(pid_clone.clone(), key.clone(), v),
                    ));
                }
            }

            plugin_col = plugin_col.push(container(card).padding(10).width(Length::Fill).style(
                move |_| container::Style {
                    background: Some(Background::Color(p.bg_composer)),
                    border: Border {
                        radius: 6.0.into(),
                        width: 1.0,
                        color: p.border,
                    },
                    ..Default::default()
                },
            ));
        }

        let content = column![
            row![
                text("Litecord — Settings")
                    .size(22)
                    .color(p.text_primary),
                Space::with_width(Length::Fill),
                button(text("Close").size(13).color(Color::WHITE))
                    .padding(Padding::from([8, 14]))
                    .style(move |_, _| button::Style {
                        background: Some(Background::Color(p.accent)),
                        text_color: Color::WHITE,
                        border: Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .on_press(UiMessage::ToggleSettings),
            ]
            .align_y(Alignment::Center),
            horizontal_rule(1),
            text("Discord Integration & Network Diagnostics")
                .size(16)
                .color(p.text_primary),
            text(format!(
                "Mode: {} | Network: {} | Requests: {} ok / {} err | Rate limits: {} | Latency: {}",
                if self.live_mode {
                    "Live Bot API"
                } else {
                    "Local Demo"
                },
                diag.state.label(),
                diag.successful_requests,
                diag.failed_requests,
                diag.rate_limit_hits,
                diag.last_latency_ms
                    .map(|ms| format!("{ms} ms"))
                    .unwrap_or_else(|| "—".into())
            ))
            .size(12)
            .color(p.text_secondary),
            row![
                button(text("Test Live Discord API (GET /gateway)").size(13))
                    .padding(Padding::from([6, 12]))
                    .on_press(UiMessage::TestDiscordGateway),
                if let Some(status) = &self.gateway_status {
                    text(status).size(12).color(p.text_muted)
                } else {
                    text("Unauthenticated public API v10 check").size(12).color(p.text_muted)
                },
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            row![
                text_input("Authorized Bot Token (stored in OS Keychain)", &self.bot_token_input)
                    .secure(true)
                    .on_input(UiMessage::BotTokenChanged)
                    .padding(Padding::from([6, 10]))
                    .size(13)
                    .width(Length::Fill),
                button(text("Connect Bot").size(13))
                    .padding(Padding::from([6, 12]))
                    .on_press(UiMessage::ConnectBot),
                button(text("Disconnect").size(13))
                    .padding(Padding::from([6, 12]))
                    .on_press(UiMessage::DisconnectBot),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            text("Capability boundary: Uses official Discord API v10 (Bot/App). Normal user-account token scraping and client-integrity spoofing are disabled.")
                .size(11)
                .color(p.text_muted),
            horizontal_rule(1),
            text("Appearance").size(16).color(p.text_primary),
            row![
                text("Theme:").size(14).color(p.text_secondary),
                button(text("Dark").size(13))
                    .padding(Padding::from([6, 14]))
                    .on_press(UiMessage::SetTheme(ThemeMode::Dark)),
                button(text("Light").size(13))
                    .padding(Padding::from([6, 14]))
                    .on_press(UiMessage::SetTheme(ThemeMode::Light)),
                text(format!("Active: {:?}", s.theme))
                    .size(12)
                    .color(p.text_muted),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            row![
                text("Message Font Size:").size(14).color(p.text_secondary),
                button(text("12px").size(13))
                    .padding(Padding::from([6, 12]))
                    .on_press(UiMessage::SetFontSize(12.0)),
                button(text("14px").size(13))
                    .padding(Padding::from([6, 12]))
                    .on_press(UiMessage::SetFontSize(14.0)),
                button(text("16px").size(13))
                    .padding(Padding::from([6, 12]))
                    .on_press(UiMessage::SetFontSize(16.0)),
                text(format!("{}px", s.font_size))
                    .size(12)
                    .color(p.text_muted),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            toggler(s.compact_mode)
                .label("Compact message spacing")
                .on_toggle(UiMessage::ToggleCompactMode),
            toggler(s.show_member_list)
                .label("Show member sidebar")
                .on_toggle(|_| UiMessage::ToggleMemberList),
            toggler(s.notifications_enabled)
                .label("Enable desktop notifications")
                .on_toggle(UiMessage::ToggleNotifications),
            horizontal_rule(1),
            text("Theme Manager & CSS Variable Import")
                .size(16)
                .color(p.text_primary),
            theme_buttons,
            row![
                text_input("Paste Discord/Vencord CSS variables (:root { --background-primary: #...; })", &self.css_theme_input)
                    .on_input(UiMessage::CssThemeInputChanged)
                    .padding(Padding::from([6, 10]))
                    .size(12)
                    .width(Length::Fill),
                button(text("Import CSS").size(12))
                    .padding(Padding::from([6, 12]))
                    .on_press(UiMessage::ImportCssTheme),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            if let Some(msg) = &self.theme_import_status {
                text(msg).size(11).color(p.text_muted)
            } else {
                text("CSS variables (--background-primary, --accent, etc.) map to native tokens; unsupported selectors are reported.")
                    .size(11)
                    .color(p.text_muted)
            },
            horizontal_rule(1),
            text(format!(
                "Plugin Manager & Vencord Compatibility (Sandbox: {})",
                if self.plugin_manager.is_sandbox_loaded() {
                    "Loaded"
                } else {
                    "Unloaded / 0 MB overhead"
                }
            ))
            .size(16)
            .color(p.text_primary),
            plugin_col,
            row![
                text_input("Paste Vencord/JS plugin source...", &self.custom_plugin_input)
                    .on_input(UiMessage::CustomPluginInputChanged)
                    .padding(Padding::from([6, 10]))
                    .size(12)
                    .width(Length::Fill),
                button(text("Import Plugin").size(12))
                    .padding(Padding::from([6, 12]))
                    .on_press(UiMessage::ImportCustomPlugin),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            if let Some(st) = &self.custom_plugin_status {
                text(st).size(11).color(p.text_muted)
            } else {
                text("Imported plugins are analyzed for Webpack dependencies and run inside the Boa JS sandbox.")
                    .size(11)
                    .color(p.text_muted)
            },
            horizontal_rule(1),
            text("Voice & Audio Devices (Local Native I/O)")
                .size(16)
                .color(p.text_primary),
            text(format!(
                "Input devices: {} | Output devices: {} | Mic: {} | Output: {}",
                self.audio_devices.inputs.len(),
                self.audio_devices.outputs.len(),
                if self.audio_settings.mic_muted {
                    "Muted"
                } else {
                    "Active"
                },
                if self.audio_settings.deafened {
                    "Deafened"
                } else {
                    "Active"
                }
            ))
            .size(12)
            .color(p.text_secondary),
            row![
                button(text("Refresh Devices").size(12))
                    .padding(Padding::from([6, 12]))
                    .on_press(UiMessage::RefreshAudioDevices),
                button(text("Test Output Chime (440 Hz)").size(12))
                    .padding(Padding::from([6, 12]))
                    .on_press(UiMessage::TestOutputChime),
                button(
                    text(if self.mic_monitor.is_some() {
                        "Stop Mic Monitor"
                    } else {
                        "Start Mic Monitor"
                    })
                    .size(12)
                )
                .padding(Padding::from([6, 12]))
                .on_press(UiMessage::ToggleMicMonitor),
            ]
            .spacing(8),
            toggler(self.audio_settings.push_to_talk)
                .label("Push-to-Talk mode")
                .on_toggle(UiMessage::TogglePushToTalk),
            text(format!(
                "Audio status: {} | Mic RMS: {:.3}",
                self.audio_status.as_deref().unwrap_or("Ready"),
                self.mic_monitor
                    .as_ref()
                    .map(|m| m.current_level())
                    .unwrap_or(0.0)
            ))
            .size(12)
            .color(p.text_muted),
            horizontal_rule(1),
            text("Performance & Virtualization Stress Test")
                .size(16)
                .color(p.text_primary),
            row![
                button(text("Load 1,000 msgs").size(13))
                    .padding(Padding::from([6, 12]))
                    .on_press(UiMessage::LoadSyntheticMessages(1_000)),
                button(text("Load 10,000 msgs").size(13))
                    .padding(Padding::from([6, 12]))
                    .on_press(UiMessage::LoadSyntheticMessages(10_000)),
                button(text("Load 100,000 msgs").size(13))
                    .padding(Padding::from([6, 12]))
                    .on_press(UiMessage::LoadSyntheticMessages(100_000)),
            ]
            .spacing(10),
            text(format!(
                "Image Cache: {}/{} bytes ({} items, {} hits / {} misses / {} evictions)",
                cache_m.current_bytes,
                cache_m.max_bytes,
                cache_m.current_items,
                cache_m.hits,
                cache_m.misses,
                cache_m.evictions
            ))
            .size(12)
            .color(p.text_muted),
            text(if self.storage.is_some() {
                "Persistence: SQLite active (settings and local messages saved automatically)."
            } else {
                "Persistence: In-memory fallback (SQLite unavailable)."
            })
            .size(12)
            .color(p.text_muted),
        ]
        .spacing(14)
        .max_width(720);

        container(
            scrollable(
                container(content)
                    .padding(32)
                    .width(Length::Fill)
                    .align_x(Alignment::Center),
            )
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(p.bg_chat)),
            ..Default::default()
        })
        .into()
    }
}
