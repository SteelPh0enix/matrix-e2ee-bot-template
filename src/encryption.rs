//! Encryption, cross-signing, and verification module.

use anyhow::{Context, Result};
use futures_util::stream::StreamExt;
use matrix_sdk::{
    Client,
    encryption::{
        CrossSigningResetAuthType,
        verification::{
            SasState, SasVerification, Verification, VerificationRequest, VerificationRequestState,
        },
    },
    ruma::{
        UserId,
        api::client::uiaa,
        events::{
            key::verification::request::ToDeviceKeyVerificationRequestEvent,
            room::message::{MessageType, OriginalSyncRoomMessageEvent},
        },
    },
};

/// Ensure cross-signing is set up and the bot's own device is signed.
///
/// This uses a safe, layered approach:
/// 1. If the device is already cross-signed, do nothing.
/// 2. If we have the private self-signing key locally, just sign the device.
/// 3. Otherwise, reset cross-signing to create new keys and sign the device.
pub async fn bootstrap_cross_signing(client: &Client, password: &str) -> Result<()> {
    let encryption = client.encryption();

    // Step 1: Check if our device is already cross-signed - if so, we're done.
    if let Some(device) = encryption.get_own_device().await? {
        if device.is_cross_signed_by_owner() {
            tracing::info!("Own device is already cross-signed, no action needed");
            return Ok(());
        }
        tracing::info!("Own device is NOT cross-signed by owner");
    }

    // Step 2: Check if we have the private cross-signing keys locally.
    // If we have the self-signing key, we can just sign our device directly.
    if let Some(status) = encryption.cross_signing_status().await {
        if status.has_self_signing {
            tracing::info!("Private self-signing key available locally, signing own device...");
            return sign_own_device(client).await;
        }
        tracing::info!(
            "Cross-signing status: master={}, self_signing={}, user_signing={}",
            status.has_master,
            status.has_self_signing,
            status.has_user_signing
        );
    }

    // Step 3: We don't have the self-signing key. Reset cross-signing to create
    // new keys and sign the device as part of the process.
    tracing::info!("Resetting cross-signing to create new keys and sign device...");

    let Some(handle) = encryption
        .reset_cross_signing()
        .await
        .context("Failed to reset cross-signing")?
    else {
        // None means no reset was needed (already set up, device already signed)
        tracing::info!("Cross-signing reset returned None - already set up");
        return Ok(());
    };

    // Handle authentication for the reset
    match handle.auth_type() {
        CrossSigningResetAuthType::Uiaa(uiaa_info) => {
            tracing::info!("UIAA required for cross-signing reset, providing password...");

            let user_id = client
                .user_id()
                .context("User ID not available after login")?;

            let mut password_auth = uiaa::Password::new(
                uiaa::UserIdentifier::UserIdOrLocalpart(user_id.localpart().to_owned()),
                password.to_owned(),
            );
            password_auth.session = uiaa_info.session.clone();

            handle
                .auth(Some(uiaa::AuthData::Password(password_auth)))
                .await
                .context("Failed to authenticate cross-signing reset")?;
        }
        CrossSigningResetAuthType::OAuth(oauth_info) => {
            tracing::warn!(
                "OAuth approval required for cross-signing reset. \
                 Please approve at: {}",
                oauth_info.approval_url
            );
            // This blocks until the user completes the OAuth approval at the URL
            handle
                .auth(None)
                .await
                .context("Failed to complete OAuth cross-signing reset")?;
        }
    }

    tracing::info!("Cross-signing reset completed, device should now be signed");

    // Verify the device is now signed (safety check)
    sign_own_device(client).await
}

/// Ensure the bot's own device is signed by its self-signing key.
///
/// This is a no-op if the device is already cross-signed. Otherwise, it calls
/// `device.verify()` which uses the locally-stored private self-signing key
/// to sign the device and upload the signature.
async fn sign_own_device(client: &Client) -> Result<()> {
    let Some(device) = client.encryption().get_own_device().await? else {
        return Err(anyhow::anyhow!("Could not get own device"));
    };

    if device.is_cross_signed_by_owner() {
        tracing::info!("Own device is already cross-signed");
        return Ok(());
    }

    tracing::info!("Signing own device with self-signing key...");
    device
        .verify()
        .await
        .context("Failed to sign own device with self-signing key")?;
    tracing::info!("Own device is now cross-signed");
    Ok(())
}

/// Request verification with the owner and wait for the flow to complete.
///
/// The bot is the **initiator** here: it sends the request, waits for the owner
/// to accept, then starts the SAS flow itself.
pub async fn request_verification(client: &Client, owner_id: &UserId) -> Result<()> {
    tracing::info!("Requesting verification with {}...", owner_id);

    let encryption = client.encryption();

    // Get the owner's identity
    let owner_identity = encryption
        .request_user_identity(owner_id)
        .await
        .context("Failed to request owner identity")?
        .context("Owner identity not found - owner may not have cross-signing set up")?;

    // Request verification - this sends the m.key.verification.request event
    let request = owner_identity
        .request_verification()
        .await
        .context("Failed to request verification")?;

    tracing::info!("Verification request sent, waiting for owner to accept...");

    // As the initiator, we do NOT call accept(). We wait for the owner to accept,
    // then we start the SAS flow.
    handle_outgoing_verification(request).await
}

/// Handle an outgoing verification request (we are the initiator).
///
/// Waits for the other side to accept, then starts SAS verification.
async fn handle_outgoing_verification(request: VerificationRequest) -> Result<()> {
    let mut stream = request.changes();

    while let Some(state) = stream.next().await {
        match state {
            VerificationRequestState::Created { .. } => {
                // We just created the request, waiting for the other side
            }
            VerificationRequestState::Ready { .. } => {
                // The owner accepted our request - now start SAS verification
                tracing::info!("Owner accepted verification request, starting SAS...");

                let sas = request
                    .start_sas()
                    .await
                    .context("Failed to start SAS verification")?
                    .context("SAS verification not supported by the other side")?;

                handle_sas_verification(sas).await?;
                return Ok(());
            }
            VerificationRequestState::Transitioned { verification } => match verification {
                Verification::SasV1(sas) => {
                    // The other side started SAS before us (race condition) - handle it
                    tracing::info!("SAS verification started by the other side...");
                    handle_sas_verification(sas).await?;
                    return Ok(());
                }
                _ => {
                    tracing::warn!("Unsupported verification method");
                    return Err(anyhow::anyhow!("Unsupported verification method"));
                }
            },
            VerificationRequestState::Done => {
                tracing::info!("Verification completed");
                return Ok(());
            }
            VerificationRequestState::Cancelled(cancel_info) => {
                tracing::warn!("Verification cancelled: {}", cancel_info.reason());
                return Err(anyhow::anyhow!(
                    "Verification cancelled: {}",
                    cancel_info.reason()
                ));
            }
            _ => {}
        }
    }

    Ok(())
}

/// Handle an incoming verification request (we are the responder).
///
/// Accepts the request and waits for the SAS flow to start.
async fn handle_incoming_verification(request: VerificationRequest) -> Result<()> {
    tracing::info!(
        "Received verification request from {}",
        request.other_user_id()
    );

    // As the responder, we accept the request
    request
        .accept()
        .await
        .context("Failed to accept verification request")?;
    tracing::info!("Accepted verification request");

    let mut stream = request.changes();

    while let Some(state) = stream.next().await {
        match state {
            VerificationRequestState::Transitioned { verification } => match verification {
                Verification::SasV1(sas) => {
                    tracing::info!("Starting SAS verification...");
                    handle_sas_verification(sas).await?;
                    return Ok(());
                }
                _ => {
                    tracing::warn!("Unsupported verification method");
                    return Err(anyhow::anyhow!("Unsupported verification method"));
                }
            },
            VerificationRequestState::Done => {
                tracing::info!("Verification completed");
                return Ok(());
            }
            VerificationRequestState::Cancelled(cancel_info) => {
                tracing::warn!("Verification cancelled: {}", cancel_info.reason());
                return Err(anyhow::anyhow!(
                    "Verification cancelled: {}",
                    cancel_info.reason()
                ));
            }
            _ => {}
        }
    }

    Ok(())
}

/// Handle SAS (emoji) verification.
///
/// Works for both initiator and responder. Only calls `accept()` if the
/// other side started the SAS flow (i.e. we are the responder).
async fn handle_sas_verification(sas: SasVerification) -> Result<()> {
    tracing::info!(
        "SAS verification with {} {}",
        sas.other_device().user_id(),
        sas.other_device().device_id()
    );

    // Only accept if we didn't start the SAS flow
    if !sas.we_started() {
        sas.accept()
            .await
            .context("Failed to accept SAS verification")?;
    }

    let mut stream = sas.changes();

    while let Some(state) = stream.next().await {
        match state {
            SasState::Created { .. } | SasState::Started { .. } | SasState::Accepted { .. } => {
                // Waiting for key exchange
            }
            SasState::KeysExchanged {
                emojis,
                decimals: _,
            } => {
                if let Some(emoji_data) = emojis {
                    tracing::info!("=== VERIFICATION EMOJIS ===");
                    tracing::info!("Compare these emojis with the owner's client:");
                    for emoji in &emoji_data.emojis {
                        tracing::info!("{} {}", emoji.symbol, emoji.description);
                    }
                    tracing::info!("===========================");

                    // Auto-confirm for a bot - the owner must confirm on their side
                    tracing::info!("Auto-confirming verification (bot mode)...");
                    sas.confirm()
                        .await
                        .context("Failed to confirm SAS verification")?;
                }
            }
            SasState::Confirmed => {
                tracing::info!("Waiting for other party to confirm...");
            }
            SasState::Done { .. } => {
                let device = sas.other_device();
                tracing::info!(
                    "Successfully verified device {} {}",
                    device.user_id(),
                    device.device_id()
                );
                return Ok(());
            }
            SasState::Cancelled(cancel_info) => {
                tracing::warn!("SAS verification cancelled: {}", cancel_info.reason());
                return Err(anyhow::anyhow!(
                    "SAS verification cancelled: {}",
                    cancel_info.reason()
                ));
            }
        }
    }

    Ok(())
}

/// Setup verification event handlers for incoming requests from other users.
pub fn setup_verification_handlers(client: &Client) {
    // Handle to-device verification requests
    client.add_event_handler({
        let client = client.clone();
        move |ev: ToDeviceKeyVerificationRequestEvent| {
            let client = client.clone();
            async move {
                if let Some(request) = client
                    .encryption()
                    .get_verification_request(&ev.sender, &ev.content.transaction_id)
                    .await
                {
                    tokio::spawn(handle_incoming_verification(request));
                }
            }
        }
    });

    // Handle verification requests in room messages
    client.add_event_handler({
        let client = client.clone();
        move |ev: OriginalSyncRoomMessageEvent| {
            let client = client.clone();
            async move {
                if let MessageType::VerificationRequest(_) = &ev.content.msgtype
                    && let Some(request) = client
                        .encryption()
                        .get_verification_request(&ev.sender, &ev.event_id)
                        .await
                {
                    tokio::spawn(handle_incoming_verification(request));
                }
            }
        }
    });
}
