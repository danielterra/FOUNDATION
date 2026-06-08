---
name: deps-check
description: Use when the user asks to check for available dependency updates in FOUNDATION — phrases like "verifica atualizações de dependências", "o que tem pra atualizar", "tem versão nova das libs", "deps desatualizadas". Runs the Rust deps-check binary (npm + Cargo) and presents the report. Same check that runs automatically on the first session of each day.
disable-model-invocation: false
---

# Deps Check

Relatório de atualizações disponíveis nas dependências do FOUNDATION (frontend `package.json` via npm + backend `src-tauri/Cargo.toml` via Cargo).

## Como rodar
1. Execute o binário Rust com `--force` (ignora o cache diário e sempre imprime):

   !`cargo run --quiet --release --manifest-path scripts/deps-check/Cargo.toml -- --force`

2. Apresente o relatório retornado ao usuário **em português, de forma concisa**, mantendo a tabela do npm e as linhas do Cargo.

## Regras
- NEVER atualize dependências automaticamente — este relatório é apenas informativo.
- ALWAYS deixe a decisão de atualizar com o usuário; se ele pedir, lembre que bumps **major** podem quebrar a build e devem passar pelo fluxo normal (architect → dev → qa).
- O mesmo binário roda no hook `SessionStart` (modo `--hook`), automaticamente na primeira sessão de cada dia. Esta skill é o gatilho manual.
- Bumps **major** de crates Rust não aparecem (exigiriam `cargo-outdated`, não instalado); `cargo update --dry-run` só mostra o que é semver-compatível com o `Cargo.toml`.
