#![forbid(unsafe_code)]

use std::fmt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use rusqlite::{params, Connection, OptionalExtension};
use zero_core::{AppSettings, ChannelId, Message, MessageId, UserId};

#[derive(Debug)]
pub enum StorageError {
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    Io(std::io::Error),
    NoDataDir,
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(e) => write!(f, "SQLite error: {e}"),
            Self::Json(e) => write!(f, "JSON error: {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::NoDataDir => write!(f, "Could not resolve OS local data directory"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<rusqlite::Error> for StorageError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

impl From<serde_json::Error> for StorageError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

pub struct Storage {
    conn: Connection,
}

impl Storage {
    pub fn default_db_path() -> Result<PathBuf, StorageError> {
        let dirs = ProjectDirs::from("org", "DiscordZero", "DiscordZero")
            .ok_or(StorageError::NoDataDir)?;
        let data_dir = dirs.data_local_dir();
        std::fs::create_dir_all(data_dir)?;
        Ok(data_dir.join("discord_zero.db"))
    }

    pub fn open_default() -> Result<Self, StorageError> {
        let path = Self::default_db_path()?;
        Self::open_path(&path)
    }

    pub fn open_path(path: &Path) -> Result<Self, StorageError> {
        let conn = Connection::open(path)?;
        let storage = Self { conn };
        storage.init_schema()?;
        Ok(storage)
    }

    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        let storage = Self { conn };
        storage.init_schema()?;
        Ok(storage)
    }

    fn init_schema(&self) -> Result<(), StorageError> {
        self.conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;

            CREATE TABLE IF NOT EXISTS app_settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS local_messages (
                id INTEGER PRIMARY KEY,
                channel_id INTEGER NOT NULL,
                author_id INTEGER NOT NULL,
                content TEXT NOT NULL,
                timestamp TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_local_messages_channel_id
                ON local_messages (channel_id, id DESC);
            ",
        )?;
        Ok(())
    }

    pub fn load_settings(&self) -> Result<AppSettings, StorageError> {
        let json: Option<String> = self
            .conn
            .query_row("SELECT json FROM app_settings WHERE id = 1", [], |row| {
                row.get(0)
            })
            .optional()?;

        match json {
            Some(raw) => Ok(serde_json::from_str(&raw)?),
            None => Ok(AppSettings::default()),
        }
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), StorageError> {
        let json = serde_json::to_string(settings)?;
        self.conn.execute(
            "INSERT INTO app_settings (id, json) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET json = excluded.json",
            params![json],
        )?;
        Ok(())
    }

    pub fn save_message(&self, msg: &Message) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO local_messages (id, channel_id, author_id, content, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                msg.id.0 as i64,
                msg.channel_id.0 as i64,
                msg.author_id.0 as i64,
                msg.content,
                msg.timestamp.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn load_messages(
        &self,
        channel_id: ChannelId,
        limit: usize,
    ) -> Result<Vec<Message>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, channel_id, author_id, content, timestamp
             FROM local_messages
             WHERE channel_id = ?1
             ORDER BY id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![channel_id.0 as i64, limit as i64], |row| {
            let id: i64 = row.get(0)?;
            let cid: i64 = row.get(1)?;
            let uid: i64 = row.get(2)?;
            let content: String = row.get(3)?;
            let ts_str: String = row.get(4)?;
            let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            Ok(Message {
                id: MessageId(id as u64),
                channel_id: ChannelId(cid as u64),
                author_id: UserId(uid as u64),
                content,
                timestamp,
                edited: false,
                pinned: false,
                reactions: Vec::new(),
                reply_to: None,
            })
        })?;

        let mut msgs = Vec::new();
        for r in rows {
            msgs.push(r?);
        }
        msgs.reverse();
        Ok(msgs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zero_core::ThemeMode;

    #[test]
    fn settings_roundtrip() {
        let storage = Storage::open_in_memory().unwrap();
        let default_loaded = storage.load_settings().unwrap();
        assert_eq!(default_loaded, AppSettings::default());

        let updated = AppSettings {
            theme: ThemeMode::Light,
            compact_mode: true,
            show_member_list: false,
            font_size: 15.0,
            notifications_enabled: false,
        };
        storage.save_settings(&updated).unwrap();
        let reloaded = storage.load_settings().unwrap();
        assert_eq!(reloaded, updated);
    }

    #[test]
    fn message_persistence_roundtrip() {
        let storage = Storage::open_in_memory().unwrap();
        let msg = Message {
            id: MessageId(42),
            channel_id: ChannelId(1001),
            author_id: UserId(1),
            content: "Persisted in SQLite".into(),
            timestamp: Utc::now(),
            edited: false,
            pinned: false,
            reactions: Vec::new(),
            reply_to: None,
        };
        storage.save_message(&msg).unwrap();
        let loaded = storage.load_messages(ChannelId(1001), 50).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].content, "Persisted in SQLite");
    }
}
