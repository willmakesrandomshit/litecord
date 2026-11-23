use std::collections::HashMap;
use std::sync::Mutex;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("OS keyring error: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("Credential lock poisoned")]
    LockPoisoned,
}

pub trait CredentialStore: Send + Sync {
    fn set_secret(&self, key: &str, value: &str) -> Result<(), CredentialError>;
    fn get_secret(&self, key: &str) -> Result<Option<String>, CredentialError>;
    fn delete_secret(&self, key: &str) -> Result<(), CredentialError>;
}

/// Uses the platform's native credential manager (Windows Credential Manager,
/// macOS Keychain, or Linux Secret Service).
pub struct OsCredentialStore {
    service: String,
}

impl OsCredentialStore {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }
}

impl Default for OsCredentialStore {
    fn default() -> Self {
        Self::new("discord-zero")
    }
}

impl CredentialStore for OsCredentialStore {
    fn set_secret(&self, key: &str, value: &str) -> Result<(), CredentialError> {
        let entry = keyring::Entry::new(&self.service, key)?;
        entry.set_password(value)?;
        Ok(())
    }

    fn get_secret(&self, key: &str) -> Result<Option<String>, CredentialError> {
        let entry = keyring::Entry::new(&self.service, key)?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CredentialError::Keyring(e)),
        }
    }

    fn delete_secret(&self, key: &str) -> Result<(), CredentialError> {
        let entry = keyring::Entry::new(&self.service, key)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(CredentialError::Keyring(e)),
        }
    }
}

/// In-memory credential store for deterministic unit/integration tests.
#[derive(Default)]
pub struct MemoryCredentialStore {
    secrets: Mutex<HashMap<String, String>>,
}

impl CredentialStore for MemoryCredentialStore {
    fn set_secret(&self, key: &str, value: &str) -> Result<(), CredentialError> {
        let mut map = self
            .secrets
            .lock()
            .map_err(|_| CredentialError::LockPoisoned)?;
        map.insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn get_secret(&self, key: &str) -> Result<Option<String>, CredentialError> {
        let map = self
            .secrets
            .lock()
            .map_err(|_| CredentialError::LockPoisoned)?;
        Ok(map.get(key).cloned())
    }

    fn delete_secret(&self, key: &str) -> Result<(), CredentialError> {
        let mut map = self
            .secrets
            .lock()
            .map_err(|_| CredentialError::LockPoisoned)?;
        map.remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_credential_store_roundtrip() {
        let store = MemoryCredentialStore::default();
        assert_eq!(store.get_secret("bot_token").unwrap(), None);
        store.set_secret("bot_token", "secret-123").unwrap();
        assert_eq!(
            store.get_secret("bot_token").unwrap().as_deref(),
            Some("secret-123")
        );
        store.delete_secret("bot_token").unwrap();
        assert_eq!(store.get_secret("bot_token").unwrap(), None);
    }
}
