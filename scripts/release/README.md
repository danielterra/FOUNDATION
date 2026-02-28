# Release Script

Script Rust automatizado para gerenciar releases do FOUNDATION seguindo [Semantic Versioning](https://semver.org/).

## O que o script faz

1. 📖 Lê a versão atual do `package.json`
2. 🔢 Incrementa a versão (major, minor ou patch)
3. 📝 Atualiza versão em:
   - `package.json`
   - `src-tauri/Cargo.toml`
   - `core-ontology/SoftwareRelease.ttl` (adiciona nova entrada)
4. 🤖 Atualiza modelos da Claude API (via script `update-models`)
5. 🔧 Operações Git:
   - Verifica se working tree está limpo (exceto arquivos de versão)
   - Stage das mudanças
   - Commit com mensagem `chore: Bump version to X.Y.Z`
   - Cria tag anotada `vX.Y.Z`

## Como usar

### Comandos disponíveis

```bash
# Patch release (0.3.0 → 0.3.1)
npm run release:patch

# Minor release (0.3.0 → 0.4.0)
npm run release:minor

# Major release (0.3.0 → 1.0.0)
npm run release:major
```

### Ou diretamente com Cargo

```bash
cd scripts/release
cargo run --release patch   # ou minor, major
```

## Semantic Versioning

### MAJOR (X.0.0)
**Breaking changes** - incompatibilidades com versões anteriores
- Mudanças na API pública
- Remoção de features
- Mudanças que quebram backward compatibility

**Exemplo:** `1.0.0 → 2.0.0`

### MINOR (0.X.0)
**Novas features** - adições compatíveis com versões anteriores
- Novas funcionalidades
- Novas APIs
- Depreciações (mas mantém compatibilidade)

**Exemplo:** `0.3.0 → 0.4.0`

### PATCH (0.0.X)
**Bug fixes** - correções compatíveis com versões anteriores
- Correção de bugs
- Melhorias de performance
- Atualizações de documentação

**Exemplo:** `0.3.0 → 0.3.1`

## Fluxo completo de release

```bash
# 1. Criar release (escolher patch, minor ou major)
npm run release:patch

# 2. Revisar mudanças
git show HEAD
git log --oneline -5

# 3. Push para o repositório
git push && git push --tags

# 4. Build da release
npm run build:release
```

## Exemplo de output

```
🚀 FOUNDATION Release Script
============================

📖 Reading current version...
   Current version: 0.3.0
   New version: 0.4.0 (Minor)

📝 Updating version in files...
   ✅ package.json
   ✅ src-tauri/Cargo.toml
   ✅ core-ontology/SoftwareRelease.ttl

🤖 Updating AI models from Claude API...
   ✅ Models updated

🔧 Git operations...
   ✅ Changes staged
   ✅ Committed: chore: Bump version to 0.4.0
   ✅ Tagged: v0.4.0

🎉 Release 0.4.0 created successfully!

Next steps:
  1. Review changes: git show HEAD
  2. Push changes: git push && git push --tags
  3. Build release: npm run build:release
```

## Segurança

O script verifica se há mudanças não commitadas **antes** de fazer qualquer alteração. Apenas arquivos de versão e ontologia são permitidos estar modificados.

Se houver mudanças inesperadas, o script falha com erro explicativo.

## Troubleshooting

### Erro: "Working tree has unexpected uncommitted changes"
Você tem arquivos modificados que não são de versão. Commit ou stash essas mudanças antes de fazer release.

```bash
git status
git add .
git commit -m "feat: my changes"
# Agora pode rodar npm run release:patch
```

### Erro: "Update models failed"
O script de atualização de modelos falhou. Verifique:
- Arquivo `.env` existe com `ANTHROPIC_API_KEY`
- Claude CLI está instalado
- Você tem créditos na API

### Tag já existe
Se você tentar criar uma release que já existe:

```bash
# Remover tag local e remota
git tag -d v0.4.0
git push origin :refs/tags/v0.4.0
```

## Estrutura de arquivos atualizados

### package.json
```json
{
  "version": "0.4.0"
}
```

### src-tauri/Cargo.toml
```toml
[package]
version = "0.4.0"
```

### core-ontology/SoftwareRelease.ttl
```turtle
# Version 0.4.0 - 2026-02-28
foundation:FoundationRelease_0_4_0 a foundation:SoftwareRelease ;
    rdfs:label "FOUNDATION v0.4.0" ;
    rdfs:comment "Release version 0.4.0" ;
    foundation:icon "new_releases" ;
    foundation:releaseOf foundation:FoundationProduct ;
    foundation:versionNumber "0.4.0" ;
    foundation:licenseType "MIT" ;
    foundation:releaseDate "2026-02-28"^^xsd:date .
```
