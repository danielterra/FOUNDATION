# Claude AI Assistant Guidelines for FOUNDATION Project

This document contains specific instructions for AI assistants working on the FOUNDATION project.

## 📋 Meta Rule - Document Maintenance

**⚠️ REGRA CRÍTICA: Sempre que o usuário corrigir você ou indicar uma preferência, ATUALIZE IMEDIATAMENTE este documento CLAUDE.md para registrar a correção/preferência.**

- Quando receber feedback negativo sobre uma ação tomada → adicione uma regra explícita
- Quando o usuário indicar "sempre faça X" ou "nunca faça Y" → documente aqui
- Quando houver uma correção de comportamento → crie uma seção apropriada se necessário
- Este documento deve evoluir continuamente com as preferências do usuário
- Sempre confirme ao usuário quando atualizar este documento

## Logs & Debugging

- **SEMPRE consulte os logs centralizados ao investigar problemas**
- Todos os erros de frontend e backend são logados de forma centralizada
- Use `npm run logs` para ver os últimos logs (mostra últimas 100 linhas)
- Ao investigar problemas, SEMPRE verifique os logs antes de fazer suposições

## Database & Storage

- **Banco de dados do usuário**: `~/Documents/Foundation/FOUNDATION.db`
- **Logs da aplicação**: `~/Library/Application Support/org.w3id.foundation/application.log` (macOS)
- Para queries SQL diretas: `sqlite3 ~/Documents/Foundation/FOUNDATION.db "SELECT ..."`
- **⚠️ REGRA INVIOLÁVEL: NUNCA, EM NENHUMA HIPÓTESE, DELETE O BANCO DE DADOS (`rm ~/Documents/Foundation/FOUNDATION.db`)**
- **⚠️ NUNCA execute comandos que alteram o banco de dados (UPDATE, DELETE, DROP, TRUNCATE, INSERT) sem confirmação explícita do usuário**
- Apenas consultas SELECT são permitidas sem confirmação prévia
- SEMPRE pergunte ao usuário antes de modificar qualquer dado no banco

## Project Structure

- **Frontend**: Svelte + TypeScript (src/)
- **Backend**: Rust + Tauri (src-tauri/)
- **Ontology**: TTL files (core-ontology/)
- **Database**: SQLite with RDF triples

## Development Commands

- `npm run tauri dev` - Start development server
- `npm run logs` - View application logs (last 100 lines)
- `npm run logs N` - View last N lines of logs
- `cargo check --manifest-path src-tauri/Cargo.toml` - Check Rust code
- `cargo build --manifest-path src-tauri/Cargo.toml` - Build Rust code

**⚠️ REGRAS DE EXECUÇÃO:**
- **NUNCA execute `npm run tauri dev` ou `npm run build`** - o usuário sempre roda isso no terminal dele
- **NUNCA mate processos do Tauri** (pkill, killall, etc.) - o usuário gerencia isso
- Apenas execute `cargo check` para validar código Rust
- O usuário é responsável por iniciar e parar o servidor de desenvolvimento

## Version Management & Releases

- **⚠️ Ao gerar uma nova release/versão do projeto:**
  1. Atualizar a versão em `src-tauri/Cargo.toml`
  2. Atualizar a versão em `package.json`
  3. **SEMPRE adicionar a nova versão em `core-ontology/SoftwareRelease.ttl`**
     - Criar uma nova entrada `foundation:FoundationRelease_X_Y_Z`
     - Incluir: label, comment, releaseOf, versionNumber, licenseType, releaseDate, changelog
  4. Verificar se há outros arquivos de ontologia que precisam ser atualizados

## Best Practices

- NUNCA suprimir warnings ou erros
- Ao terminar uma tarefa, sempre revise o que foi feito para identificar e resolver redundâncias e ambiguidades
- Sempre verifique os logs antes de fazer suposições sobre problemas
- Use consultas SELECT para investigar o banco antes de propor mudanças
- **SEMPRE use RUST para criar scripts** - Não use Node.js, Python ou outras linguagens para scripts de automação
