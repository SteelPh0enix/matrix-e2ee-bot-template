//! Event handlers for the Matrix bot.

use anyhow::{Context, Result};
use matrix_sdk::{
    Client, RoomMemberships, RoomState,
    room::Room,
    ruma::{
        OwnedUserId, UserId,
        events::room::{
            member::StrippedRoomMemberEvent,
            message::{MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent},
        },
    },
};
use tokio::time::{Duration, sleep};

/// Set up autojoin handler that accepts room invites from the owner.
///
/// This should be registered BEFORE `sync_once()` to catch pending invites.
pub fn setup_autojoin_handler(client: &Client, owner_id: OwnedUserId) {
    client.add_event_handler(
        move |event: StrippedRoomMemberEvent, client: Client, room: Room| {
            let owner_id = owner_id.clone();
            async move {
                handle_owner_invite(event, client, room, &owner_id).await;
            }
        },
    );
}

/// Handle invite events - auto-join rooms when invited by the owner.
async fn handle_owner_invite(
    event: StrippedRoomMemberEvent,
    client: Client,
    room: Room,
    owner_id: &UserId,
) {
    // Only handle invites addressed to us
    let Some(my_user_id) = client.user_id() else {
        return;
    };
    if event.state_key != my_user_id {
        return;
    }

    // Only accept invites from the owner
    if event.sender != owner_id {
        tracing::warn!(
            "Received invite from {} but owner is {}, ignoring",
            event.sender,
            owner_id
        );
        return;
    }

    tracing::info!(
        "Received invite from owner {} to room {}",
        event.sender,
        room.room_id()
    );

    // Spawn a task because room-state-changing methods wait for the next sync,
    // and event handlers run before the next sync begins.
    // Use exponential backoff retry for the Synapse race condition workaround:
    // https://github.com/matrix-org/synapse/issues/4345
    tokio::spawn(async move {
        let mut delay = 2u64;
        while let Err(err) = room.join().await {
            tracing::error!(
                "Failed to join room {} ({err:?}), retrying in {delay}s",
                room.room_id()
            );
            sleep(Duration::from_secs(delay)).await;
            delay *= 2;

            if delay > 3600 {
                tracing::error!("Giving up joining room {} ({err:?})", room.room_id());
                return;
            }
        }
        tracing::info!("Successfully joined room {}", room.room_id());
    });
}

/// Set up a message handler for the bot.
///
/// This should be registered AFTER `sync_once()` to avoid responding to old messages.
pub fn setup_message_handler(client: &Client) {
    client.add_event_handler(
        |event: OriginalSyncRoomMessageEvent, room: Room, client: Client| async move {
            // Only handle messages in joined rooms
            if room.state() != RoomState::Joined {
                return;
            }

            // Only handle text messages
            let MessageType::Text(text) = &event.content.msgtype else {
                return;
            };

            // Don't respond to our own messages
            if Some(event.sender.as_ref()) == client.user_id() {
                return;
            }

            tracing::debug!(
                "Received message in room {} from {}: {}",
                room.room_id(),
                event.sender,
                text.body
            );

            // Echo the message back
            let content = RoomMessageEventContent::text_plain(format!("Echo: {}", text.body));
            if let Err(e) = room.send(content).await {
                tracing::error!("Failed to send echo message: {e}");
            }
        },
    );
}

/// Create an encrypted DM room with the owner.
pub async fn create_encrypted_room(client: &Client, owner_id: &UserId) -> Result<Room> {
    use matrix_sdk::ruma::{
        api::client::room::create_room::v3::{Request as CreateRoomRequest, RoomPreset},
        events::room::encryption::RoomEncryptionEventContent,
        serde::Raw,
    };

    tracing::info!("Creating encrypted room with owner {}...", owner_id);

    // RoomPreset::PrivateChat only sets join_rules=invite and history_visibility=shared.
    // Encryption must be explicitly enabled via an m.room.encryption initial state event.
    let encryption_content = RoomEncryptionEventContent::with_recommended_defaults();
    let encryption_event = serde_json::json!({
        "type": "m.room.encryption",
        "state_key": "",
        "content": encryption_content,
    });

    let mut request = CreateRoomRequest::new();
    request.invite = vec![owner_id.to_owned()];
    request.is_direct = true;
    request.preset = Some(RoomPreset::PrivateChat);
    request.initial_state = vec![Raw::from_json(
        serde_json::value::to_raw_value(&encryption_event)
            .context("Failed to serialize encryption event")?,
    )];

    let room = client
        .create_room(request)
        .await
        .context("Failed to create encrypted room")?;

    tracing::info!("Created encrypted room: {}", room.room_id());
    Ok(room)
}

/// Find an existing room shared with the owner (DM or any joined room).
pub async fn find_room_with_owner(client: &Client, owner_id: &UserId) -> Option<Room> {
    // First check DM rooms
    if let Some(room) = client.get_dm_room(owner_id) {
        return Some(room);
    }

    // Check all joined rooms for one with the owner
    for room in client.joined_rooms() {
        let members = room.members_no_sync(RoomMemberships::JOIN).await.ok()?;
        if members.iter().any(|m| m.user_id() == owner_id) {
            return Some(room);
        }
    }

    None
}
