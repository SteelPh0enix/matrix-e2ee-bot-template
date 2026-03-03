# Example Matrix bot

This project is an example implementation of a personal Matrix bot.

This bot supports full E2E encryption and verification of the user and bot session.

## Configuration

The bot is configured via environmental variables loaded from .env file.
The .env file should provide following variables:

```env
MATRIX_BOT_USERNAME=example_bot_username
MATRIX_BOT_PASSWORD=example_bot_password
MATRIX_BOT_OWNER_HANDLE=@example_bot_owner:homeserver.org
MATRIX_BOT_HOMESERVER=https://homeserver.org/
# User should generate a secure passphrase and put it here
MATRIX_BOT_STORE_PASSWORD=example_bot_store_password
```

## Workflow

After starting the bot for the first time, the bot performs the following steps:

### 1. Login and Session Persistence

The bot logs in with the provided credentials and stores the session data persistently. On subsequent starts, it restores the session using the stored device ID (important: the same device ID must be reused to maintain encryption key validity).

### 2. Cross-Signing Bootstrap

The bot bootstraps cross-signing to create its cryptographic identity. This involves:

- Creating a master cross-signing key
- Creating self-signing and user-signing keys
- Signing its own device with the cross-signing identity

This requires User-Interactive Authentication (UIAA), typically using the bot's password.

**Result:** The bot's device becomes "self-signed" - it's trusted by its own cross-signing identity.

### 3. Encrypted Room Creation

The bot creates an encrypted room with its owner. The room is configured with E2EE enabled (using the `m.room.encryption` state event), ensuring all messages are encrypted end-to-end.

### 4. Interactive Verification

The bot requests verification with the owner. The verification flow uses **SAS (Short Authentication String) verification** with emojis:

1. Bot sends a verification request to the owner
2. Owner accepts the request in their Matrix client
3. Both sides exchange keys and display a set of 7 emojis
4. **Owner must compare the emojis** and confirm they match
5. If confirmed, the verification succeeds

**Important:** Verification is not automatic - the owner must actively compare and confirm the emoji match. This is a security feature to prevent man-in-the-middle attacks.

### 5. Trust Chain Establishment

After successful verification:

- The owner's client signs the bot's cross-signing identity
- The bot becomes "verified" from the owner's perspective
- All encrypted communication is now considered trusted

The trust chain works as follows:

```text
Owner's Identity
    └── Signs Bot's Cross-Signing Identity (after verification)
            └── Bot's Cross-Signing Key
                    └── Signs Bot's Device
```

### 6. Normal Operation

After verification, the owner can chat with the bot in the created room. The bot will echo back any messages sent to it.

## Data Storage

The persistent data for restoring the session on next login is stored in `data/` directory:

- Session tokens and device ID
- Encryption keys (stored encrypted with `MATRIX_BOT_STORE_PASSWORD`)
- Room state and sync tokens

## Technical Notes

### Cross-Signing

Cross-signing simplifies device verification. Instead of verifying each device individually, users only need to verify each other's cross-signing identity once. All devices signed by that identity are automatically trusted.

### Verification Methods

The Matrix SDK supports two verification methods:

1. **SAS (Short Authentication String)** - Uses emoji or decimal comparison
2. **QR Code** - Scanning a QR code displayed by the other party

This bot uses SAS verification with emojis, which is widely supported across Matrix clients.

### Session Restoration

When restoring a session, it's critical to reuse the same device ID. Using a different device ID will create a new device on the server, which won't have access to the existing encryption keys and won't be able to decrypt historical messages.
