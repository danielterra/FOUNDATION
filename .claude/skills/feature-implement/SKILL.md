---
name: feature-implement
description: Use when the user asks to implement / develop a Foundation SoftwareFeature whose UserStories are already planned — orchestrates execution of each child story via /userstory-implement, then moves the Feature to "Testando" for user validation. Skill name follows entity-action convention.
disable-model-invocation: false
---

# Implement Software Feature

Skill que executa uma `foundation:SoftwareFeature` já planejada e a entrega para validação do usuário, orquestrando a implementação de todas as `foundation:UserStory` filhas.

```
Planejado / Pronto para Dev → Em Progresso → Testando → (usuário valida) → Concluído
                                                     (↘ Mudança Pendente / Bloqueado / Rejeitado)
```

Esta skill cobre **Planejado → Em Progresso → Testando** da Feature, delegando a implementação de cada US filha à skill `/userstory-implement`. A transição final para `Concluído` é responsabilidade do usuário após validar.

---

## Pré-requisitos

A skill recebe a IRI da SoftwareFeature como argumento (ex. `foundation:SoftwareFeature_1777574442863`).
Se o usuário não informar, peça antes de prosseguir.

---

## Instruções

### Passo 1 — Validar estado e carregar US filhas

Em paralelo:

1. `describe_individual([<FeatureIRI>])` — confirme `hasStatus`, `solvesProblem`, `successCriteria`.
2. `search(class_iri: "foundation:UserStory", filters: [{detail: "foundation:partOfFeature", value: "<FeatureIRI>"}], limit: 100)` — lista US filhas com `id` e `status`.

Valide:

- O `hasStatus` da Feature deve ser `foundation:Status_1772596341042` (Planejado) ou `foundation:Status_1773079329634` (Pronto para Dev). Em qualquer outro status, **pare** e avise — replanejar via `/feature-plan` se for "Mudança Pendente".
- Devem existir US filhas em status `Planejado` ou `Pronto para Dev` (US implementáveis). Se nenhuma existir, **pare** — provavelmente a Feature precisa de `/feature-plan` antes.
- Cada US a ser implementada DEVE ter `implementationPlan` preenchido. Se alguma não tiver, **pare** e peça `/userstory-plan` para ela.

### Passo 2 — Mover Feature para Em Progresso

Antes de tocar código, mova o status:

```
replace_property_values(operations: [
  {
    iri: "<FeatureIRI>",
    property_iri: "foundation:hasStatus",
    values: ["foundation:InProgress"]
  }
])
```

Isso sinaliza no kanban que a Feature inteira entrou em execução e impede que outro fluxo concorrente tente replanejar.

### Passo 3 — Implementar cada US filha

Para cada US filha em `foundation:Status_1772596341042` (Planejado) ou `foundation:Status_1773079329634` (Pronto para Dev):

- Invoque `/userstory-implement <UserStoryIRI>` — uma US por vez, sequencialmente, na ordem retornada pelo `search` (ou em ordem de dependência se o plano explicitar).
- A skill `/userstory-implement` executa o plano, valida build (`cargo check`/`cargo build`), anexa "Como testar" e move a US para `Testando`.
- Se `/userstory-implement` parar (build vermelho, plano divergente, ou retornar a US para `Mudança Pendente`), **interrompa** o ciclo:
  - Não continue implementando outras US — o usuário precisa decidir.
  - Reverta a Feature para o status anterior (`Planejado` ou `Pronto para Dev`) via `replace_property_values`.
  - Reporte qual US falhou e por quê.

US que já estão em `Testando`, `InProgress` ou `Completed` são **ignoradas** — não reimplementar.

### Passo 4 — Mover Feature para Testando

Após **todas** as US filhas elegíveis estarem em `foundation:Status_1772600993751` (Testando) ou estágio posterior:

```
replace_property_values(operations: [
  {
    iri: "<FeatureIRI>",
    property_iri: "foundation:hasStatus",
    values: ["foundation:Status_1772600993751"]
  }
])
```

`foundation:Status_1772600993751` é o IRI fixo de **Testando** — nunca substituir pelo label.

### Passo 5 — Atualizar changelog da Feature

Use `add_property_values` (não substitua) em `foundation:changelog`:

```
<YYYY-MM-DD> — Feature entregue para teste. <N> User Stories implementadas: <lista de IRIs>. Commit: <hash se já houver, senão "pendente de commit">.
```

A data é a de hoje (resolva via `Today's date` do contexto).

### Passo 6 — Reportar

Mostre ao usuário, em até 10 linhas:

- IRI e label da Feature.
- Status anterior → `Em Progresso` → `Testando`.
- Quantas US foram implementadas, quantas ignoradas e por quê.
- Lista das US entregues (IRI + label) com link para validação.
- **Convite explícito** para validar: "Para cada US, execute a seção 'Como testar' do `implementationPlan` e mova para `Concluído` se ok, ou `Mudança Pendente` se houver divergência. Quando todas estiverem `Concluído`, mova a Feature para `Concluído`."

Não faça commit nem push automático — `/code-commit` é skill separada.

---

## Regras

- **NEVER** invente IRIs de status — use `foundation:InProgress` (Em Progresso) e `foundation:Status_1772600993751` (Testando).
- **NEVER** continue implementando US se uma US falhou — pare e reporte para o usuário decidir.
- **NEVER** mova a Feature direto para `Concluído` — quem valida é o usuário.
- **NEVER** rode `npm run tauri dev` / `npm run build` (CLAUDE.md). A skill `/userstory-implement` cuida do `cargo check`/`cargo build`.
- **ALWAYS** entre em `Em Progresso` antes de tocar código e saia para `Testando` apenas após **todas** as US elegíveis estarem em `Testando`.
- **ALWAYS** rode `/userstory-implement` sequencialmente, uma US por vez.
- **ALWAYS** use `add_property_values` para o `changelog` — é histórico cumulativo.
- **ALWAYS** responda ao usuário em português (CLAUDE.md).

## When NOT to use this skill

- Implementar uma única US → `/userstory-implement`
- Planejar a Feature antes de implementar → `/feature-plan`
- Sincronizar US com o código existente → `/feature-code-sync`
- Remover Feature → `/feature-remove`
