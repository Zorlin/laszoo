#!/bin/bash
# Build a portable Linux binary for laszoo using musl

set -e

echo "Building portable laszoo binary..."

# Build with musl target, without gamepad support for maximum compatibility
cargo build --release --target x86_64-unknown-linux-musl --no-default-features

# Strip the binary to reduce size
strip target/x86_64-unknown-linux-musl/release/laszoo

# Copy to a convenient location
cp target/x86_64-unknown-linux-musl/release/laszoo ./laszoo-portable

echo "Portable binary created: ./laszoo-portable"
echo "This binary will work on any modern Linux system without dependency issues."
ls -lh ./laszoo-portable