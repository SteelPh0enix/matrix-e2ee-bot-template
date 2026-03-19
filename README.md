# Example Matrix bot

This project is an example implementation of a personal Matrix bot written in Rust.

This bot supports full E2E encryption and verification of the user and bot session.

## Prerequisites

- [Rust](https://rustup.rs/) (latest stable version recommended)
- A Matrix homeserver account for the bot
- A Matrix client account for the owner (can be the same or different homeserver)

## Installation

Clone the repository and build the bot:

```bash
git clone <repo-url>
cd matrix-bot-test
cargo build --release
```

The compiled binary will be at `target/release/matrix-bot-test`.

## Configuration

The bot is configured via environmental variables loaded from `.env` file.
The `.env` file should provide following variables:

```env
MATRIX_BOT_USERNAME=example_bot_username
MATRIX_BOT_PASSWORD=example_bot_password
MATRIX_BOT_OWNER_HANDLE=@example_bot_owner:homeserver.org
MATRIX_BOT_HOMESERVER=https://homeserver.org/
# User should generate a secure passphrase and put it here
MATRIX_BOT_STORE_PASSWORD=example_bot_store_password
```

| Variable                    | Description                                                  |
|-----------------------------|--------------------------------------------------------------|
| `MATRIX_BOT_USERNAME`       | The bot's Matrix username (e.g., `@bot:homeserver.org`)      |
| `MATRIX_BOT_PASSWORD`       | The bot's Matrix password                                    |
| `MATRIX_BOT_OWNER_HANDLE`   | The owner's Matrix user ID (e.g., `@owner:homeserver.org`)   |
| `MATRIX_BOT_HOMESERVER`     | The homeserver URL for the bot (e.g., `https://matrix.org/`) |
| `MATRIX_BOT_STORE_PASSWORD` | A secure passphrase used to encrypt local session data       |

## Running the Bot

```bash
# Create .env file with your configuration (see above)
cargo run --release
```

The bot will run continuously, listening for messages and verification requests.

> **Note:** The bot echoes back plain text messages only. Markdown formatting in incoming messages is not preserved in the response.

## Project Structure

```text
src/
├── main.rs       # Bot entry point, initialization, and main event loop
├── client.rs     # Client setup, session persistence, and sync logic
├── config.rs     # Environment variable configuration loading
├── encryption.rs # Cross-signing, verification, and key backup
└── handlers.rs   # Event handlers for invites, messages, and room creation
```

## Workflow

After starting the bot, it performs the following steps:

### First-Time Setup

1. **Login and Session Persistence**
   - The bot logs in with the provided credentials
   - Creates a new device on the homeserver
   - Stores session data persistently to `~/.cache/matrix-bot-test/data/session.json`

2. **Cross-Signing Bootstrap**
   - The bot bootstraps cross-signing to create its cryptographic identity
   - Creates master, self-signing, and user-signing keys
   - Signs its own device with the cross-signing identity
   - This requires User-Interactive Authentication (UIAA), typically using the bot's password

3. **Key Backup**
   - Ensures a key backup exists on the homeserver
   - Deletes any stale backups and creates a fresh one
   - Prevents recurring warnings about missing backup keys

4. **Encrypted Room Creation**
   - The bot checks if a room already exists with the owner
   - If no room exists, creates a new encrypted DM room
   - The room is configured with E2EE enabled via `m.room.encryption` state event

5. **Interactive Verification**
   - The bot requests verification with the owner
   - The verification flow uses **SAS (Short Authentication String)** with emojis:
     1. Bot sends a verification request to the owner
     2. Owner accepts the request in their Matrix client
     3. Both sides exchange keys and display a set of 7 emojis
     4. **Owner must compare the emojis** and confirm they match
     5. If confirmed, the verification succeeds

6. **Trust Chain Establishment**
   - After successful verification, the owner's client signs the bot's cross-signing identity
   - The bot becomes "verified" from the owner's perspective
   - All encrypted communication is now considered trusted

### Subsequent Runs

When the bot starts with an existing session:

1. **Session Restoration**
   - The bot restores the session using stored device ID
   - **Important:** The same device ID is reused to maintain encryption key validity
   - If a different device ID were used, the bot couldn't decrypt historical messages

2. **Verification Check**
   - The bot checks if it's already verified with the owner
   - If not verified, requests verification again

3. **Room Discovery**
   - The bot looks for an existing room with the owner
   - If none found, creates a new encrypted room

## Features

### E2E Encryption

The bot uses Matrix's end-to-end encryption (E2EE) with:

- Megolm group encryption for room messages
- Olm for device-to-device messaging
- Key backup to the homeserver for recovery

### Cross-Signing

Cross-signing simplifies device verification. Instead of verifying each device individually, users only need to verify each other's cross-signing identity once. All devices signed by that identity are automatically trusted.

### Verification Methods

The Matrix SDK supports two verification methods:

1. **SAS (Short Authentication String)** - Uses emoji or decimal comparison
2. **QR Code** - Scanning a QR code displayed by the other party

This bot uses SAS verification with emojis, which is widely supported across Matrix clients.

### Auto-Join

The bot automatically joins rooms when invited by the owner. This uses exponential backoff retry to handle race conditions with certain homeserver implementations.

### Incoming Verification

The bot can handle verification requests initiated by the owner (not just the ones it starts). This allows the owner to verify the bot from their client if needed.

## Data Storage

The persistent data for restoring the session is stored in:

```text
~/.cache/matrix-bot-test/data/
├── session.json     # Session tokens, device ID, and homeserver info
└── sqlite_store/     # SQLite database with encrypted keys and room state
```

### Session File

The `session.json` file contains:

- Homeserver URL
- Database path and encryption passphrase
- Matrix user session (access token, device ID, etc.)

### SQLite Store

The SQLite database stores:

- Encryption keys (stored encrypted with `MATRIX_BOT_STORE_PASSWORD`)
- Room state and sync tokens
- Device lists

**Security Note:** The `session.json` file contains sensitive data encrypted with the store password. Keep it protected.

## Technical Notes

### Session Restoration

When restoring a session, it's critical to reuse the same device ID. Using a different device ID will create a new device on the server, which won't have access to the existing encryption keys and won't be able to decrypt historical messages.

### Initial Sync Pattern

The bot uses a two-phase sync approach:

1. `sync_once()` - Initial sync to get current state without processing old messages
2. Continuous `sync()` loop - For ongoing event handling

This ensures the bot doesn't respond to messages that existed before it started.

### Key Backup

Key backup ensures that if the bot needs to recover its encryption keys (e.g., after a database loss), it can do so from the homeserver. This is especially important for maintaining access to historical encrypted messages.
