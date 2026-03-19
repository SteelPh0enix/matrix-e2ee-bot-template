#!/usr/bin/env fish
# Run the Matrix bot with logging filtered to disable matrix_sdk_base::client

set -gx RUST_LOG "info,matrix_sdk_base::client=off,matrix_sdk::room::futures=off"

cargo run
