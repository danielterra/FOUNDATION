---
name: userstory-implement
description: Use when the user asks to implement / develop a Foundation User Story that is already in status "Planejado" or "Pronto para Dev" — executes the implementationPlan, validates the build, appends "Como testar" steps to the plan, logs delivery to changelog, and moves the story to "Testando" so the user can validate. Skill name follows entity-action convention.
disable-model-invocation: false
---

# Implement User Story

Skill que executa uma `foundation:UserStory` já planejada e a entrega para validação do usuário, transitando dentro do ciclo de vida oficial:

```
Planejado / Pronto para Dev → Em Progresso → Testando → (usuário valida) → Concluído
                                                     (↘ Mudança Pendente / Bloqueado / Rejeitado)
```

Esta skill cobre **Planejado → Em Progresso → Testando**. A transição final para `Concluído` é responsabilidade do usuário após validar.

---

## Pré-requisitos

A skill recebe a IRI da User Story como argumento (ex. `foundation:UserStory_1778074850125`).
Se o usuário não informar, peça antes de prosseguir.

---

## Instruções

### Passo 1 — Validar estado e carregar plano

`describe_individual([<UserStoryIRI>])` e valide:

- `hasStatus` ∈ { `foundation:Status_1772596341042` (Planejado), `foundation:Status_1773079329634` (Pronto para Dev) }. Em qualquer outro status, **pare** e avise — replanejar via `/userstory-plan` se for "Mudança Pendente"; se for "Pendente", o usuário precisa planejar primeiro.
- `implementationPlan` está preenchido. Se vazio, **pare** — execute `/userstory-plan` antes.
- `acceptanceCriteria` está preenchido (já validado pelo planejamento, mas reconfirme — eles guiam o "Como testar").

Leia `capability`, `benefit`, `acceptanceCriteria`, `implementationPlan` inteiros para a memória de trabalho. Esses são o contrato.

### Passo 2 — Marcar Em Progresso

Antes de qualquer edição de código, mova o status para `foundation:InProgress` via `replace_property_values` para `foundation:hasStatus`. Isso sinaliza no kanban que o trabalho começou e impede que outro fluxo concorrente tente replanejar.

### Passo 3 — Executar o plano

Siga as seções `## Mudanças` e `## Abordagem` do `implementationPlan` literalmente:

- Se o plano referencia outra skill (ex. `mcp-create`, `widget-create`, `ontology-change`), **invoque-a** para a parte correspondente — não duplique o trabalho.
- Implemente apenas o que o plano descreve. Se durante a execução perceber que o plano está errado ou incompleto, **pare**, retraga a transição (volte para `Mudança Pendente` = `foundation:Status_1773581282341`) e peça ao usuário para replanejar. Não desvie do plano em silêncio.
- Mantenha o estilo do CLAUDE.md: comentários só de WHY, sem TODO/FIXME, sem suprimir warnings, scripts em Rust.

### Passo 4 — Validar build

Conforme `CLAUDE.md`:

- Se o plano tocou em `Cargo.toml`, profile, features ou dependências, rode `cargo build --manifest-path src-tauri/Cargo.toml` (e **avise o usuário antes** se a mudança invalida 100% do cache).
- Caso contrário, rode `cargo check --manifest-path src-tauri/Cargo.toml`.
- Se o plano tocou em `src/` (Svelte/TS), o usuário roda `tauri dev` — você só sinaliza.
- Se houver migração nova em `src-tauri/src/bin/`, **não execute** — apenas documente o comando no "Como testar".

Se a validação falhar, conserte e revalide. Não conclua a skill com erros pendentes.

### Passo 5 — Anexar "Como testar" ao plano

Releia o `implementationPlan` atual e **acrescente ao final** uma nova seção:

```markdown

## Como testar

**Pré-requisitos**
<o que o usuário precisa preparar antes — ex.: "rodar `npm run tauri dev`", "ter um arquivo PDF local em ~/Downloads", "ter um indivíduo X criado">

**Passos**
1. <ação concreta — ex.: "Abrir Claude Desktop e chamar a tool `attach_file_to_individual` com `file_path=...` e `target_iri=...`">
2. <próxima ação>
3. ...

**Resultado esperado** (um item por Critério de Aceitação)
- AC1: <como observar que foi atendido — ex.: "describe_individual retorna o file_iri sob foundation:hasFile">
- AC2: ...
- ACn: ...

**Como reportar falha**
Se algum passo desviar do esperado, mover a story para `Mudança Pendente` (`foundation:Status_1773581282341`) e descrever a divergência no `foundation:changelog`.
```

Regras para "Como testar":

- **Cobertura 1:1 com `acceptanceCriteria`** — cada AC vira um item em "Resultado esperado". Sem isso, o usuário não consegue validar contra o contrato.
- Use comandos e IRIs reais; nada genérico tipo "verifique se funciona".
- Inclua dados de teste concretos quando aplicável (ex. caminho de um PDF de exemplo, IRI de um indivíduo existente).
- Se o plano envolve UI: liste o caminho de cliques e o que deve aparecer.
- Se o plano envolve MCP/Tauri commands: dê a chamada exata com argumentos.
- Mantenha curto — 8 a 20 linhas para a seção inteira.

### Passo 6 — Atualizar changelog

Adicione (não substitua — usar `add_property_values`) uma entrada em `foundation:changelog`:

```
<YYYY-MM-DD> — implementação entregue para teste. Arquivos tocados: <lista path:linha curta>. Commit: <hash se já houver, senão "pendente de commit">.
```

A data é a de hoje (resolva via `Today's date` do contexto). Mantenha uma linha só.

### Passo 7 — Persistir plano + status

Use **uma única chamada** `replace_property_values` com duas operações:

```
replace_property_values(operations: [
  {
    iri: "<UserStoryIRI>",
    property_iri: "foundation:implementationPlan",
    values: ["<plano original + nova seção '## Como testar'>"]
  },
  {
    iri: "<UserStoryIRI>",
    property_iri: "foundation:hasStatus",
    values: ["foundation:Status_1772600993751"]
  }
])
```

`foundation:Status_1772600993751` é o IRI fixo de **Testando** — nunca substituir pelo label.

### Passo 8 — Reportar

Mostre ao usuário, em até 8 linhas:

- IRI e label da story.
- Status anterior → `Em Progresso` → `Testando`.
- Resumo do que foi entregue (1-2 frases).
- Lista de arquivos modificados com `path:linha` quando útil.
- **Convite explícito** para validar: "Execute os passos da seção 'Como testar' no `implementationPlan`. Se ok, mude o status para Concluído; senão, mova para Mudança Pendente e descreva a divergência."

Não faça commit nem push automático — `code-commit` é skill separada e o usuário decide quando consolidar.

---

## Regras

- **NEVER** invente IRIs de status — use `foundation:InProgress` (Em Progresso) e `foundation:Status_1772600993751` (Testando).
- **NEVER** desvie do `implementationPlan` em silêncio. Se o plano está errado, pare e peça replanejamento.
- **NEVER** mova direto para `Concluído` — quem valida é o usuário, não a skill.
- **NEVER** rode `npm run tauri dev` / `npm run build` (CLAUDE.md). Use `cargo check` / `cargo build`.
- **NEVER** suprima warnings ou erros para "passar" a build (CLAUDE.md).
- **ALWAYS** entre em `Em Progresso` antes de tocar código e saia para `Testando` apenas após build verde.
- **ALWAYS** ancore "Como testar" nos `acceptanceCriteria` — um item de validação por AC, sem exceção.
- **ALWAYS** persista plano-com-testes e status na **mesma** chamada `replace_property_values` (atomicidade lógica).
- **ALWAYS** use `add_property_values` (não `replace_property_values`) para o `changelog` — é histórico cumulativo.
- **ALWAYS** responda ao usuário em português (CLAUDE.md).
