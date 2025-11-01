#!/bin/bash
set -e

echo "Building Hyperlight guest binary..."

# Build the guest library
cd "$(dirname "$0")"
cargo build --release

echo "Guest binary built successfully!"
echo "Location: $(pwd)/target/release/libagent_guest.so"
echo ""
echo "To use this guest with the redis-agent-worker, set:"
echo "  export GUEST_BINARY_PATH=$(pwd)/target/release/libagent_guest.so"
