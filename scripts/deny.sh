#!/bin/bash
# Run cargo-deny checks

set -e

echo "🔒 Running cargo-deny security and licensing checks..."

# Check if cargo-deny is installed
if ! command -v cargo-deny &> /dev/null; then
    echo "cargo-deny not found. Installing v0.19.0..."
    cargo install cargo-deny --version 0.19.0 --locked
fi

# Run all checks
echo ""
echo "Checking advisories (security vulnerabilities)..."
cargo deny check advisories

echo ""
echo "Checking licenses..."
cargo deny check licenses

echo ""
echo "Checking bans (disallowed dependencies)..."
cargo deny check bans

echo ""
echo "Checking sources (dependency origins)..."
cargo deny check sources

echo ""
echo "✅ All cargo-deny checks passed!"
