//! Configuration module for loading environment variables.

use std::path::PathBuf;

use anyhow::{Context, Result};
use dotenvy::from_path;

/// Project name used for cache directory paths.
const PROJECT_NAME: &str = "matrix-bot-test";

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
        let home_dir =
            std::env::home_dir().unwrap_or_else(|| PathBuf::from(".").canonicalize().unwrap());
        home_dir.join(".cache").join(PROJECT_NAME).join("data")
    }

    /// Get the session file path.
    pub fn session_file() -> PathBuf {
        Self::data_dir().join("session.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_dir_is_absolute() {
        let data_dir = Config::data_dir();
        assert!(data_dir.is_absolute());

        // Check that path contains the expected structure
        let path_str = data_dir.to_string_lossy();
        assert!(path_str.contains(".cache/matrix-bot-test/data"));
    }

    #[test]
    fn test_session_file_is_absolute() {
        let session_file = Config::session_file();
        assert!(session_file.is_absolute());

        // Check that path contains the expected structure
        let path_str = session_file.to_string_lossy();
        assert!(path_str.contains(".cache/matrix-bot-test/data/session.json"));
    }

    #[test]
    fn test_session_file_is_in_data_dir() {
        let data_dir = Config::data_dir();
        let session_file = Config::session_file();

        // session_file should be inside data_dir
        assert!(session_file.starts_with(&data_dir));
        assert_eq!(session_file.file_name().unwrap(), "session.json");
    }
}
