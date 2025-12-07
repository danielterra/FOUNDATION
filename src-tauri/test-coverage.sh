#!/bin/bash
# Test coverage script with 80% minimum requirement

set -e

echo "🧪 Running tests with coverage analysis..."

# Run tests with coverage (HTML report)
cargo llvm-cov --html --output-dir target/coverage

# Run tests with coverage (fail if below 80%)
cargo llvm-cov --fail-under-lines 80

echo "✅ Coverage meets 80% minimum requirement!"
echo "📊 Open target/coverage/html/index.html to see detailed report"
