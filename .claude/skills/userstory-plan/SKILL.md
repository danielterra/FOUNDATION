---
name: userstory-plan
description: Use when the user asks to plan a Foundation User Story (e.g. foundation:UserStory) — analyzes Project + Feature + Story context, produces an implementation plan, stores it in foundation:implementationPlan and moves the story to status "Planejado". Skill name follows entity-action convention.
disable-model-invocation: false
---

# Plan User Story

Skill que conduz uma `foundation:UserStory` da fase **Pendente** para **Planejado** dentro do ciclo de vida oficial:

```
Pendente → Planejado → Pronto para Dev → Em Progresso → Testando → Concluído
                                                     (↘ Bloqueado / Cancelado / Rejeitado)
```

Esta skill cobre apenas a transição **Pendente → Planejado**: ela exige que a story tenha `capability`, `benefit` e `acceptanceCriteria` definidos, e produz um plano técnico textual gravado em `foundation:implementationPlan`.

---

## Pré-requisitos

A skill recebe a IRI da User Story como argumento (ex. `foundation:UserStory_1778074850125`).
Se o usuário não informar, peça antes de prosseguir.

---

## Instruções

### Passo 1 — Carregar contexto completo

Em paralelo, busque todas as entidades da cadeia:

1. `describe_individual([<UserStoryIRI>])` — pega `capability`, `benefit`, `acceptanceCriteria`, `partOfFeature`, `hasStatus`, `userRole` (se houver).
2. Resolva `partOfFeature` → `describe_individual([<FeatureIRI>])` — pega `partOfProject`, `solvesProblem`, `successCriteria`.
3. Resolva `partOfProject` → `describe_individual([<ProjectIRI>])` — pega `hasObjective`, `usesMethodology`, `delivers`.

Valide invariantes antes de prosseguir:

- A User Story DEVE ter `capability`, `benefit` e `acceptanceCriteria` preenchidos. Se faltar qualquer um, **pare** e peça ao usuário para completar — sem isso o plano não tem âncora.
- O `hasStatus` atual deve permitir transição. Casos válidos:
  - `foundation:Pending` (Pendente) — fluxo normal.
  - `foundation:Status_1773581282341` (Mudança Pendente) — replanejamento após mudança de escopo.
  - `foundation:Status_1772596341042` (Planejado) — replanejamento explícito; confirme com o usuário antes de sobrescrever.
- Se já estiver em `InProgress`, `Completed`, `Cancelado` ou `Rejected`, **pare** e avise — não é uma transição válida desta skill.

### Passo 2 — Investigar o código

Antes de escrever qualquer plano, investigue o codebase para descobrir o que já existe:

- Use `Grep`/`Glob` para localizar módulos, comandos Tauri, MCP tools, classes da ontologia ou componentes Svelte mencionados na `capability` ou `acceptanceCriteria`.
- Se a story menciona uma class/property da ontologia, use `describe_class` / `describe_property` para confirmar nomes exatos.
- Anote arquivos relevantes com `path:linha` para referenciar no plano.

Não invente nomes de funções, classes ou IRIs. Se algo não existir ainda, diga "criar" no plano.

### Passo 3 — Redigir o plano

Grave em `foundation:implementationPlan` um texto Markdown com esta estrutura mínima:

```markdown
## Contexto
- Projeto: <label do Project>
- Funcionalidade: <label da Feature>
- História: <capability>

## Análise
<2-4 frases ligando a história ao código existente, citando arquivos com path:linha>

## Abordagem
<resumo da estratégia técnica em 3-6 bullets — qual camada toca, quais entidades envolve, fluxo end-to-end>

## Mudanças
- **Ontologia**: <novas classes/propriedades, ou "nenhuma">
- **Backend (Rust)**: <comandos Tauri / MCP tools / módulos a criar ou alterar, com path:linha>
- **Frontend (Svelte)**: <componentes a criar ou alterar, ou "nenhuma">
- **Migrações**: <se houver, descrever sob `src-tauri/src/bin/`; senão "nenhuma">

## Validação
<como cada Critério de Aceitação será verificado — um item por AC>

## Riscos
<2-3 itens; ou "nenhum identificado">
```

Regras:

- O plano descreve **o que** fazer e **onde** — não cole código pronto, isso é trabalho do `userstory-implement` (futuro) ou de quem for executar.
- Refira arquivos como `src-tauri/src/owl/individual/core.rs:42` para o usuário poder navegar.
- Mantenha terso. Plano de 30-80 linhas é o esperado — qualquer coisa maior provavelmente deveria virar mais de uma User Story.
- Não crie individuals `foundation:Plan` nem `foundation:Task` — o campo de texto `implementationPlan` é a saída desta skill. (Se o usuário quiser quebrar em Plan/Task formais, é um pedido separado.)

### Passo 4 — Persistir

Use **uma única chamada** `replace_property_values` com duas operações em paralelo:

```
replace_property_values(operations: [
  {
    iri: "<UserStoryIRI>",
    property_iri: "foundation:implementationPlan",
    values: ["<plano markdown gerado no Passo 3>"]
  },
  {
    iri: "<UserStoryIRI>",
    property_iri: "foundation:hasStatus",
    values: ["foundation:Status_1772596341042"]
  }
])
```

`foundation:Status_1772596341042` é o IRI fixo de **Planejado** — nunca substituir pelo label.

### Passo 5 — Reportar

Mostre ao usuário, em até 5 linhas:

- IRI e label da story.
- Status anterior → `Planejado`.
- Resumo de 1 frase do plano.
- Lista de arquivos principais que o plano vai tocar.

---

## Regras

- **NEVER** invente IRIs de status — use `foundation:Status_1772596341042` para "Planejado".
- **NEVER** altere `capability`, `benefit` ou `acceptanceCriteria` da story dentro desta skill — se estiverem errados, pare e peça correção primeiro.
- **NEVER** crie a User Story se ela não existir — esta skill é só de planejamento.
- **ALWAYS** investigue o código antes de redigir o plano; um plano sem `path:linha` é um plano inventado.
- **ALWAYS** persista plano e status na **mesma** chamada `replace_property_values` (atomicidade lógica).
- **ALWAYS** responda ao usuário em português (CLAUDE.md).
