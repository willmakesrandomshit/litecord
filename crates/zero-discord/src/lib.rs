#![forbid(unsafe_code)]

pub mod models;

use thiserror::Error;
use zero_network::{DiagnosticsSnapshot, HttpClient, NetworkError};

pub use models::{
    CreateMessagePayload, DiscordChannel, DiscordGuild, DiscordMessage, DiscordUser, GatewayInfo,
};

pub const DISCORD_API_BASE: &str = "https://discord.com/api/v10";
pub const USER_AGENT: &str = "DiscordZero (https://github.com/discord-zero/discord-zero, 0.1.0)";

#[derive(Debug, Error)]
pub enum DiscordError {
    #[error("Network error: {0}")]
    Network(#[from] NetworkError),
    #[error("Missing authorization token")]
    MissingToken,
}

#[derive(Clone)]
pub struct DiscordClient {
    http: HttpClient,
    bot_token: Option<String>,
}

impl DiscordClient {
    pub fn new(bot_token: Option<String>) -> Result<Self, DiscordError> {
        let http = HttpClient::new(USER_AGENT)?;
        Ok(Self { http, bot_token })
    }

    pub fn diagnostics(&self) -> DiagnosticsSnapshot {
        self.http.diagnostics()
    }

    pub fn set_bot_token(&mut self, token: Option<String>) {
        self.bot_token = token
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());
    }

    fn auth_header(&self) -> Result<String, DiscordError> {
        let token = self
            .bot_token
            .as_deref()
            .ok_or(DiscordError::MissingToken)?;
        if token.starts_with("Bot ") || token.starts_with("Bearer ") {
            Ok(token.to_string())
        } else {
            Ok(format!("Bot {token}"))
        }
    }

    /// Public unauthenticated Discord API v10 endpoint — verifies live TLS, DNS, and API health.
    pub async fn get_gateway(&self) -> Result<GatewayInfo, DiscordError> {
        let url = format!("{DISCORD_API_BASE}/gateway");
        Ok(self.http.get_json("GET /gateway", &url, None).await?)
    }

    pub async fn get_current_user(&self) -> Result<DiscordUser, DiscordError> {
        let auth = self.auth_header()?;
        let url = format!("{DISCORD_API_BASE}/users/@me");
        Ok(self
            .http
            .get_json("GET /users/@me", &url, Some(&auth))
            .await?)
    }

    pub async fn get_current_user_guilds(&self) -> Result<Vec<DiscordGuild>, DiscordError> {
        let auth = self.auth_header()?;
        let url = format!("{DISCORD_API_BASE}/users/@me/guilds");
        Ok(self
            .http
            .get_json("GET /users/@me/guilds", &url, Some(&auth))
            .await?)
    }

    pub async fn get_guild_channels(
        &self,
        guild_id: u64,
    ) -> Result<Vec<DiscordChannel>, DiscordError> {
        let auth = self.auth_header()?;
        let route = format!("GET /guilds/{guild_id}/channels");
        let url = format!("{DISCORD_API_BASE}/guilds/{guild_id}/channels");
        Ok(self.http.get_json(&route, &url, Some(&auth)).await?)
    }

    pub async fn get_channel_messages(
        &self,
        channel_id: u64,
        limit: u8,
    ) -> Result<Vec<DiscordMessage>, DiscordError> {
        let auth = self.auth_header()?;
        let route = format!("GET /channels/{channel_id}/messages");
        let url = format!(
            "{DISCORD_API_BASE}/channels/{channel_id}/messages?limit={}",
            limit.clamp(1, 100)
        );
        Ok(self.http.get_json(&route, &url, Some(&auth)).await?)
    }

    pub async fn create_message(
        &self,
        channel_id: u64,
        content: &str,
    ) -> Result<DiscordMessage, DiscordError> {
        let auth = self.auth_header()?;
        let route = format!("POST /channels/{channel_id}/messages");
        let url = format!("{DISCORD_API_BASE}/channels/{channel_id}/messages");
        let payload = CreateMessagePayload { content };
        Ok(self
            .http
            .post_json(&route, &url, Some(&auth), &payload)
            .await?)
    }

    pub async fn execute_webhook(
        &self,
        webhook_url: &str,
        content: &str,
    ) -> Result<DiscordMessage, DiscordError> {
        let sep = if webhook_url.contains('?') { '&' } else { '?' };
        let url = format!("{webhook_url}{sep}wait=true");
        let payload = CreateMessagePayload { content };
        Ok(self
            .http
            .post_json("POST /webhooks", &url, None, &payload)
            .await?)
    }
}
