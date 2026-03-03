//! Matrix Bot - A personal E2EE-enabled Matrix bot example.
//!
//! This bot demonstrates:
//! - Session persistence with SQLite store
//! - Cross-signing bootstrap for E2EE
//! - Interactive verification (SAS/emoji)
//! - Encrypted room creation
//! - Auto-joining rooms on owner invite
//! - Message echo functionality

mod client;
mod config;
mod encryption;
mod handlers;

use anyhow::{Context, Result};
use matrix_sdk::ruma::UserId;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    tracing::info!("Starting Matrix bot...");

    // Load configuration
    let config = config::Config::from_env().context("Failed to load configuration")?;

    tracing::info!("Configuration loaded successfully");

    // Initialize client (restore existing session or create new one)
    let client_init = client::init_client(&config)
        .await
        .context("Failed to initialize client")?;

    let client = client_init.client;

    // Parse owner's user ID
    let owner_id = UserId::parse(&config.matrix_bot_owner_handle)
        .with_context(|| format!("Invalid owner handle: {}", config.matrix_bot_owner_handle))?;

    // Bootstrap cross-signing if needed
    encryption::bootstrap_cross_signing(&client, &config.matrix_bot_password)
        .await
        .context("Failed to bootstrap cross-signing")?;

    // Register handlers that should catch events from the initial sync:
    // - Autojoin: accepts pending invites from the owner
    // - Verification: handles pending verification requests
    handlers::setup_autojoin_handler(&client, owner_id.clone());
    encryption::setup_verification_handlers(&client);

    // Perform initial sync to get current state (skips old messages)
    let sync_token = client::initial_sync(&client)
        .await
        .context("Failed to perform initial sync")?;

    // Register message handler AFTER initial sync so we don't respond to old messages
    handlers::setup_message_handler(&client);

    // Handle first-time setup or existing session
    if client_init.is_new_session {
        tracing::info!("First time setup - requesting verification with owner...");

        // Request verification with owner.
        // The SDK will automatically create an encrypted DM room with the owner
        // if one doesn't already exist.
        tracing::info!(
            "Requesting verification with owner. Please accept the verification request \
             and compare the emojis displayed on both devices."
        );

        // Spawn verification as a background task. The verification flow needs
        // the sync loop to be running to receive the owner's responses, so we
        // can't await it before starting sync.
        let verification_client = client.clone();
        let verification_owner_id = owner_id.clone();
        tokio::spawn(async move {
            match encryption::request_verification(&verification_client, &verification_owner_id)
                .await
            {
                Ok(()) => {
                    tracing::info!("Verification completed successfully!");
                }
                Err(e) => {
                    tracing::warn!(
                        "Verification could not be completed: {}. \
                         The owner can initiate verification later from their Matrix client.",
                        e
                    );
                }
            }
        });
    } else {
        tracing::info!("Existing session restored - finding existing room...");

        // Try to find existing room with owner
        if handlers::find_room_with_owner(&client, &owner_id)
            .await
            .is_none()
        {
            tracing::info!("No existing room found with owner, creating one...");

            let room = handlers::create_encrypted_room(&client, &owner_id)
                .await
                .context("Failed to create encrypted room")?;

            tracing::info!("Encrypted room created: {}", room.room_id());
        }

        // Check if we're already verified with the owner
        let needs_verification = match client.encryption().get_user_identity(&owner_id).await? {
            Some(identity) if identity.is_verified() => {
                tracing::info!("Already verified with owner.");
                false
            }
            Some(_) => {
                tracing::info!("Not verified with owner, will request verification...");
                true
            }
            None => {
                tracing::info!(
                    "Owner identity not found locally. They may need to enable cross-signing \
                     or share a room with the bot."
                );
                false
            }
        };

        if needs_verification {
            let verification_client = client.clone();
            let verification_owner_id = owner_id.clone();
            tokio::spawn(async move {
                match encryption::request_verification(&verification_client, &verification_owner_id)
                    .await
                {
                    Ok(()) => {
                        tracing::info!("Verification completed successfully!");
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Verification could not be completed: {}. \
                             The owner can initiate verification later from their Matrix client.",
                            e
                        );
                    }
                }
            });
        }
    }

    tracing::info!("Bot is ready! Listening for messages...");

    // Start sync loop, passing the token from initial_sync to avoid re-processing.
    // This must run for verification and message handling to work.
    client::sync_loop(client, sync_token)
        .await
        .context("Sync loop terminated with error")?;

    Ok(())
}
