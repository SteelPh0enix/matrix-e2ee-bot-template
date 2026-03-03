//! Configuration module for loading environment variables.

use std::path::PathBuf;

use anyhow::{Context, Result};
use dotenvy::from_path;

/// Configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    /// Bot's Matrix username (e.g., "@bot:homeserver.org")
    pub matrix_bot_username: String,
    /// Bot's Matrix password
    pub matrix_bot_password: String,
    /// Owner's Matrix handle (e.g., "@owner:homeserver.org")
    pub matrix_bot_owner_handle: String,
    /// Homeserver URL (e.g., "https://homeserver.org/")
    pub matrix_bot_homeserver: String,
    /// Password for encrypting the local store
    pub matrix_bot_store_password: String,
}

impl Config {
    /// Load configuration from .env file.
    pub fn from_env() -> Result<Self> {
        // Try to load from .env file in the current directory
        let env_path = PathBuf::from(".env");
        if env_path.exists() {
            from_path(&env_path).context("Failed to load .env file")?;
        }

        Ok(Config {
            matrix_bot_username: std::env::var("MATRIX_BOT_USERNAME")
                .context("MATRIX_BOT_USERNAME not set")?,
            matrix_bot_password: std::env::var("MATRIX_BOT_PASSWORD")
                .context("MATRIX_BOT_PASSWORD not set")?,
            matrix_bot_owner_handle: std::env::var("MATRIX_BOT_OWNER_HANDLE")
                .context("MATRIX_BOT_OWNER_HANDLE not set")?,
            matrix_bot_homeserver: std::env::var("MATRIX_BOT_HOMESERVER")
                .context("MATRIX_BOT_HOMESERVER not set")?,
            matrix_bot_store_password: std::env::var("MATRIX_BOT_STORE_PASSWORD")
                .context("MATRIX_BOT_STORE_PASSWORD not set")?,
        })
    }

    /// Get the data directory path for persistent storage.
    pub fn data_dir() -> PathBuf {
        PathBuf::from("data")
    }

    /// Get the session file path.
    pub fn session_file() -> PathBuf {
        Self::data_dir().join("session.json")
    }
}
