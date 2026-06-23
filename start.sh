#!/usr/bin/env bash
cd "$(dirname "$0")"
if [ ! -f "target/debug/trading-backend" ]; then
    echo "Binary not found. Run build.sh first."
    exit 1
fi
if [ ! -f "frontend/dist/index.html" ]; then
    echo "Frontend not built. Run build.sh first."
    exit 1
fi
echo "Starting Trading App on http://localhost:8080"
RUST_LOG=info ./target/debug/trading-backend
