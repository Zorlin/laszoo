#!/bin/bash
# Laszoo Test Runner

set -e

echo "Building Laszoo in release mode..."
cargo build --release

echo "Running tests..."
echo "Note: Some tests require proper MooseFS setup or may need to be run as root"

# Set test environment
export RUST_BACKTRACE=1
export RUST_LOG=laszoo=debug

# Run tests sequentially to avoid conflicts
cargo test --release -- --test-threads=1

echo "Test run complete!"