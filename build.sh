#!/usr/bin/env bash
set -e
cd "$(dirname "$0")"

echo "=== Building Frontend ==="
cd frontend
npm install
npm run build
cd ..

echo "=== Building Backend ==="
cargo build

echo "=== Build Complete! ==="
echo "Run: ./start.sh"
echo "Then open: http://localhost:8080"
