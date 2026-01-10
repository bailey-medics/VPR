#!/bin/bash
# Run security audit with cargo-audit

set -e

echo "🔒 Running security audit..."

# Check if cargo-audit is installed
if ! command -v cargo-audit &> /dev/null; then
    echo "cargo-audit not found. Installing..."
    cargo install cargo-audit --locked
fi

# Run the audit
cargo audit

echo "✅ Security audit passed!"
