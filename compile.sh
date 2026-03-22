#!/bin/bash
set -e

cd "$(dirname "$0")"

echo "=== Running tests ==="
cargo test

echo ""
echo "=== Building release binary ==="
cargo build --release

BINARY="./target/release/catdns"
SIZE=$(ls -lh "$BINARY" | awk '{print $5}')
echo ""
echo "Build complete: $BINARY ($SIZE)"
echo ""
echo "Usage:"
echo "  $BINARY --gen-config > config.json   # generate example config"
echo "  $BINARY -c config.json               # start server"
