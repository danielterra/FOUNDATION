#!/bin/bash

# Script para executar code coverage do projeto FOUNDATION

echo "📊 Gerando relatório de code coverage..."
echo ""

# Code coverage Rust (usando cargo-llvm-cov)
echo "📦 Coverage Rust (src-tauri):"
if ! command -v cargo-llvm-cov &> /dev/null; then
    echo "⚠️  cargo-llvm-cov não encontrado. Instalando..."
    cargo install cargo-llvm-cov
fi

cd src-tauri && cargo llvm-cov --html && cd ..
echo "✅ Relatório HTML gerado em: src-tauri/target/llvm-cov/html/index.html"
open src-tauri/target/llvm-cov/html/index.html

# Code coverage JavaScript/Svelte (frontend)
echo ""
echo "🎨 Coverage Frontend:"
npm run test:coverage
