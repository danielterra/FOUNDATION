---
name: bug-fix
description: Use when the user asks to fix a Foundation bug by IRI — e.g. "fix bug foundation:Bug_123", "corrija o bug foundation:Bug_456", "resolva o bug <IRI>". Fetches the bug from the ontology, investigates the root cause following CLAUDE.md debugging order (logs → messages → DB → code), applies the fix, validates the build, and closes the bug with a cause analysis. Always invoke this skill when the user mentions a bug IRI.
---

# Bug Fix

Skill que investiga e corrige um `foundation:Bug`, seguindo a ordem de depuração do CLAUDE.md e fechando o registro com a análise da causa raiz.

---

## Pré-requisitos

A skill recebe a IRI do bug como argumento (ex. `foundation:Bug_1780369248862`).  
Se o usuário não informar, peça antes de prosseguir.

---

## Instruções

### Passo 1 — Carregar o bug

`describe_individual([<BugIRI>])` e leia:

- `rdfs:label` — título do bug
- `foundation:bugDescription` — descrição detalhada
- `foundation:expectedBehavior` — comportamento esperado
- `foundation:stepsToReproduce` — passos para reproduzir
- `foundation:causeAnalysis` — análise já existente (pode estar vazia)
- `foundation:relatedTo` — entidades relacionadas (tasks, outros bugs, processos)

Mantenha essas informações na memória de trabalho — elas guiam toda a investigação.

### Passo 2 — Investigar a causa raiz

Siga a **ordem de depuração do CLAUDE.md**: logs → histórico de mensagens → DB → código.

**Logs**  
```
npm run logs 20
```
Procure erros ou warnings relacionados às entidades/predicados mencionados no bug.

**Histórico de mensagens** (se o bug envolve AI / conversas)  
Consulte a classe `foundation:AIConversationMessage` se relevante. Campos: `foundation:role`, `foundation:content` em `object_value`; `foundation:sentAt` em `object_datetime`.

**DB** (se necessário)  
Use apenas `SELECT` — nunca escreva SQL de mutação diretamente. Verifique a tabela `triples` filtrando pelas IRIs das entidades em `foundation:relatedTo`. Lembre-se das regras de imutabilidade do CLAUDE.md: filtre pelo maior `tx` para obter o estado atual.

**Código**  
Localize os arquivos relevantes com Grep/Glob. Leia e entenda o fluxo completo antes de alterar qualquer coisa. Preste atenção na arquitetura em camadas (`EAVTO → OWL → Core-Ontology → Commands`) — a causa raiz geralmente é uma propriedade que nunca é lida, um desvio de camada, ou um fallback que esconde o erro real.

### Passo 3 — Confirmar a hipótese antes de editar

Antes de tocar no código, formule a causa raiz em uma frase objetiva. Se a hipótese não for óbvia, anuncie-a ao usuário e aguarde confirmação.

### Passo 4 — Aplicar a correção

Edite apenas o necessário para corrigir o bug. Respeite o CLAUDE.md:

- Comentários só de WHY — nunca WHAT.
- Sem TODO/FIXME.
- Sem suprimir warnings ou erros.
- Scripts em Rust (nunca Node, Python ou shell).
- Não adicione tratamento de erro, fallbacks ou validações para cenários que não podem ocorrer.

Se a correção tocar em camadas proibidas (ex. EAVTO referenciando IRIs de domínio), ajuste para respeitar a arquitetura antes de prosseguir.

### Passo 5 — Validar build

Conforme CLAUDE.md:

- Se tocou em `Cargo.toml`, features ou dependências → `cargo build --manifest-path src-tauri/Cargo.toml` (avise o usuário antes se invalida 100% do cache).
- Caso contrário → `cargo check --manifest-path src-tauri/Cargo.toml`.
- Se tocou em `src/` (Svelte/TS) → sinalize ao usuário para validar no `tauri dev`.

Se a validação falhar, corrija e revalide. Não encerre a skill com erros pendentes.

### Passo 6 — Fechar o bug

Use **uma única chamada** `replace_property_values` com duas operações:

```
replace_property_values(operations: [
  {
    iri: "<BugIRI>",
    property_iri: "foundation:causeAnalysis",
    values: ["<causa raiz confirmada>\n\nCorreção aplicada: <descrição objetiva da mudança — arquivo:linha, o que foi adicionado/removido e por quê>. Build validado com cargo check."]
  },
  {
    iri: "<BugIRI>",
    property_iri: "foundation:hasStatus",
    values: ["foundation:Completed"]
  }
])
```

`foundation:Completed` é o status de **Concluído** — use sempre este IRI exato.

### Passo 7 — Reportar

Em até 8 linhas:

- IRI e label do bug.
- Causa raiz em uma frase.
- Arquivos modificados com `path:linha`.
- Status atualizado para Concluído.

---

## Regras

- **ALWAYS** siga a ordem de depuração: logs → mensagens → DB → código. Não pule direto para o código.
- **ALWAYS** formule a causa raiz antes de editar qualquer arquivo.
- **ALWAYS** valide com `cargo check` / `cargo build` antes de fechar o bug.
- **ALWAYS** feche o bug com `foundation:causeAnalysis` + `foundation:hasStatus = foundation:Completed` na mesma chamada.
- **NEVER** use SQL de mutação (INSERT/UPDATE/DELETE) — apenas SELECT, ou MCP tools para writes.
- **NEVER** altere `rdfs:comment` das entidades relacionadas — é o campo de descrição original.
- **NEVER** rode `npm run tauri dev` / `npm run build` (CLAUDE.md).
- **NEVER** suprima warnings ou erros para "passar" a build.
- **ALWAYS** responda ao usuário em português (CLAUDE.md).
