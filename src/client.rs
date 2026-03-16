//! Client setup and session management module.

use std::path::Path;

use anyhow::{Context, Result};
use matrix_sdk::{
    Client,
    authentication::matrix::MatrixSession,
    config::SyncSettings,
    encryption::{BackupDownloadStrategy, EncryptionSettings},
    ruma::api::client::filter::FilterDefinition,
};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::config::{Config, PROJECT_NAME};

/// The data needed to re-build a client.
#[derive(Debug, Serialize, Deserialize)]
struct ClientSession {
    /// The URL of the homeserver of the user.
    homeserver: String,
    /// The path of the database.
    db_path: String,
    /// The passphrase of the database.
    passphrase: String,
}

/// The full session to persist.
#[derive(Debug, Serialize, Deserialize)]
struct FullSession {
    /// The data to re-build the client.
    client_session: ClientSession,
    /// The Matrix user session.
    user_session: MatrixSession,
}

/// Result of client initialization.
pub struct ClientInit {
    pub client: Client,
    pub is_new_session: bool,
}

/// Initialize the client, either restoring an existing session or creating a new one.
pub async fn init_client(config: &Config) -> Result<ClientInit> {
    let session_file = Config::session_file();
    let data_dir = Config::data_dir();

    // Ensure data directory exists
    if !data_dir.exists() {
        fs::create_dir_all(&data_dir)
            .await
            .context("Failed to create data directory")?;
    }

    if session_file.exists() {
        tracing::info!("Found existing session, attempting to restore...");
        let client = restore_session(&session_file).await?;
        Ok(ClientInit {
            client,
            is_new_session: false,
        })
    } else {
        tracing::info!("No existing session found, creating new one...");
        let client = login_new_session(config).await?;
        Ok(ClientInit {
            client,
            is_new_session: true,
        })
    }
}

/// Restore a previous session from the session file.
async fn restore_session(session_file: &Path) -> Result<Client> {
    let serialized_session = fs::read_to_string(session_file)
        .await
        .context("Failed to read session file")?;

    let FullSession {
        client_session,
        user_session,
    } = serde_json::from_str(&serialized_session).context("Failed to parse session file")?;

    tracing::info!(
        "Restoring session for user {}...",
        user_session.meta.user_id
    );

    let client = Client::builder()
        .homeserver_url(&client_session.homeserver)
        .sqlite_store(&client_session.db_path, Some(&client_session.passphrase))
        .with_encryption_settings(EncryptionSettings {
            auto_enable_cross_signing: true,
            auto_enable_backups: true,
            ..Default::default()
        })
        .build()
        .await
        .context("Failed to build client")?;

    client
        .restore_session(user_session)
        .await
        .context("Failed to restore session")?;

    // Wait for E2EE initialization to complete (device key upload, cross-signing, etc.)
    client
        .encryption()
        .wait_for_e2ee_initialization_tasks()
        .await;

    tracing::info!("Session restored successfully");
    Ok(client)
}

/// Login with a new device and persist the session.
async fn login_new_session(config: &Config) -> Result<Client> {
    let data_dir = Config::data_dir();
    let db_path = data_dir.join("sqlite_store");
    let session_file = Config::session_file();

    let client = Client::builder()
        .homeserver_url(config.matrix_bot_homeserver.as_str())
        .sqlite_store(&db_path, Some(config.matrix_bot_store_password.as_str()))
        .with_encryption_settings(EncryptionSettings {
            auto_enable_cross_signing: true,
            auto_enable_backups: true,
            backup_download_strategy: BackupDownloadStrategy::AfterDecryptionFailure,
        })
        .build()
        .await
        .context("Failed to build client")?;

    tracing::info!("Logging in as {}...", config.matrix_bot_username);

    let response = client
        .matrix_auth()
        .login_username(&config.matrix_bot_username, &config.matrix_bot_password)
        .initial_device_display_name(PROJECT_NAME)
        .await
        .context("Failed to login")?;

    tracing::info!("Logged in successfully as {}", response.user_id);

    // Wait for E2EE initialization to complete (device key upload, cross-signing, etc.)
    client
        .encryption()
        .wait_for_e2ee_initialization_tasks()
        .await;

    // Save the session
    let user_session = client
        .matrix_auth()
        .session()
        .expect("A logged-in client should have a session");

    let full_session = FullSession {
        client_session: ClientSession {
            homeserver: config.matrix_bot_homeserver.clone(),
            db_path: db_path.to_string_lossy().to_string(),
            passphrase: config.matrix_bot_store_password.clone(),
        },
        user_session,
    };

    let serialized_session =
        serde_json::to_string(&full_session).context("Failed to serialize session")?;

    fs::write(&session_file, serialized_session)
        .await
        .context("Failed to write session file")?;

    tracing::info!("Session saved to {}", session_file.display());

    Ok(client)
}

/// Build sync settings with lazy-loading filter.
fn sync_settings() -> SyncSettings {
    let filter = FilterDefinition::with_lazy_loading();
    SyncSettings::default().filter(filter.into())
}

/// Perform an initial sync to get the client ready.
/// Returns the sync token to pass to the continuous sync loop.
pub async fn initial_sync(client: &Client) -> Result<String> {
    tracing::info!("Performing initial sync...");

    let response = client
        .sync_once(sync_settings())
        .await
        .context("Initial sync failed")?;

    tracing::info!("Initial sync completed");
    Ok(response.next_batch)
}

/// Start the sync loop. The `sync_token` should come from `initial_sync`.
pub async fn sync_loop(client: Client, sync_token: String) -> Result<()> {
    tracing::info!("Starting sync loop...");

    let settings = sync_settings().token(sync_token);

    client.sync(settings).await.context("Sync loop failed")?;

    Ok(())
}
