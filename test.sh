#!/bin/bash

# Script para executar testes do projeto FOUNDATION

echo "🧪 Executando testes..."
echo ""

# Testes Rust (backend Tauri)
echo "📦 Testes Rust (src-tauri):"
cd src-tauri && cargo test && cd ..

# Testes JavaScript/Svelte (frontend)
echo ""
echo "🎨 Testes Frontend:"
npm test
