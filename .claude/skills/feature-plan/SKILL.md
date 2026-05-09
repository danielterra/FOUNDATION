---
name: feature-plan
description: Use when the user asks to plan a Foundation SoftwareFeature — orchestrates planning of all child UserStories via /userstory-plan, then moves the Feature from "Pendente" to "Planejado". Skill name follows entity-action convention.
disable-model-invocation: false
---

# Plan Software Feature

Skill que conduz uma `foundation:SoftwareFeature` da fase **Pendente** para **Planejado**, orquestrando o planejamento de todas as `foundation:UserStory` filhas.

```
Pendente → Planejado → Pronto para Dev → Em Progresso → Testando → Concluído
```

Esta skill cobre apenas a transição **Pendente → Planejado** da Feature, delegando o planejamento individual de cada US filha à skill `/userstory-plan`.

---

## Pré-requisitos

A skill recebe a IRI da SoftwareFeature como argumento (ex. `foundation:SoftwareFeature_1777574442863`).
Se o usuário não informar, peça antes de prosseguir.

---

## Instruções

### Passo 1 — Carregar Feature + US filhas

Em paralelo:

1. `describe_individual([<FeatureIRI>])` — pega `partOfProject`, `partOfProduct`, `solvesProblem`, `successCriteria`, `hasStatus` e os backlinks.
2. `search(class_iri: "foundation:UserStory", filters: [{detail: "foundation:partOfFeature", value: "<FeatureIRI>"}], limit: 100)` — lista todas as US filhas.
3. Resolva `partOfProject` → `describe_individual([<ProjectIRI>])` para o contexto do plano.

Valide:

- A Feature precisa ter um **contrato observável**. A skill resolve o contrato nesta ordem:
  1. Se `successCriteria` está preenchido na Feature → use-o como contrato.
  2. Se vazio, mas a Feature tem ≥1 US filha com `acceptanceCriteria` preenchido → o **conjunto agregado dos AC das US filhas** é o `successCriteria` efetivo (a convenção do FOUNDATION é manter o contrato nas US, não na Feature).
  3. Se nenhuma US filha tem AC e a Feature não tem `successCriteria` → **pare** e peça ao usuário definir o contrato (ou criar pelo menos uma US com AC).
- `solvesProblem` é **opcional** — se preenchido, registre no relatório; se ausente, prossiga sem bloquear (apenas 4 de 50 Features do projeto usam).
- O `hasStatus` atual deve permitir transição. Casos válidos:
  - `foundation:Pending` (Pendente) — fluxo normal.
  - `foundation:Status_1773581282341` (Mudança Pendente) — replanejamento após mudança de escopo.
  - `foundation:Status_1772596341042` (Planejado) — replanejamento explícito; confirme com o usuário antes.
- Se já estiver em `InProgress`, `Testando`, `Completed`, `Cancelado` ou `Rejected`, **pare** e avise.

### Passo 2 — Mapear gap de cobertura

**Só aplicável quando a Feature tem `successCriteria` próprio** (cenário 1 do Passo 1).

Compare cada item de `successCriteria` da Feature com a `capability` das US filhas:

- Liste cada item de `successCriteria` (uma linha por critério).
- Para cada critério, identifique qual US filha o cobre (por correspondência semântica entre `successCriteria` e `capability`/`acceptanceCriteria`).
- **Critérios sem US** → liste como gaps. **Pare** e mostre ao usuário o gap. Pergunte se ele quer:
  - (a) criar User Stories faltantes manualmente antes de planejar, OU
  - (b) avançar com cobertura parcial (registre essa decisão no changelog da Feature).

Não invente User Stories nesta skill — criar US é responsabilidade do usuário ou de `/feature-code-sync`. Aqui apenas planejamos as que existem.

**Quando o contrato vem dos AC das US filhas** (cenário 2 do Passo 1), pule este passo — não há gap a calcular, o contrato é o próprio conjunto de US existentes.

### Passo 3 — Planejar cada US filha

Para cada US filha em status `foundation:Pending`, `foundation:Status_1773581282341` (Mudança Pendente) ou `foundation:Status_1772596341042` (Planejado, se for replanejamento explícito):

- Invoque `/userstory-plan <UserStoryIRI>` — uma US por vez, sequencialmente, para evitar conflitos de escrita.
- Se a skill `/userstory-plan` parar (por falta de `capability`/`benefit`/`acceptanceCriteria`), **pare** o ciclo e reporte ao usuário qual US precisa ser corrigida antes.

US filhas que já estão em `Pronto para Dev`, `InProgress`, `Testando` ou `Completed` são **ignoradas** — não replanejar trabalho em andamento.

### Passo 4 — Persistir status da Feature

Quando todas as US filhas elegíveis estiverem em pelo menos `Planejado`:

```
replace_property_values(operations: [
  {
    iri: "<FeatureIRI>",
    property_iri: "foundation:hasStatus",
    values: ["foundation:Status_1772596341042"]
  }
])
```

`foundation:Status_1772596341042` é o IRI fixo de **Planejado** — nunca substituir pelo label.

Se houver gaps aceitos pelo usuário no Passo 2, registre via `add_property_values` em `foundation:changelog`:

```
<YYYY-MM-DD> — Feature planejada com cobertura parcial. Critérios sem US: <lista>.
```

### Passo 5 — Reportar

Mostre ao usuário, em até 8 linhas:

- IRI e label da Feature.
- Status anterior → `Planejado`.
- Quantas US filhas foram planejadas, quantas ignoradas e por quê.
- Lista de gaps identificados (se houver).
- Próximo passo sugerido: `/feature-implement <FeatureIRI>` quando o usuário quiser começar a entrega.

---

## Regras

- **NEVER** invente IRIs de status — use `foundation:Status_1772596341042` para "Planejado".
- **NEVER** crie User Stories dentro desta skill — apenas orquestre o planejamento das existentes.
- **NEVER** planeje a Feature sem antes ter passado por **todas** as US filhas elegíveis em `/userstory-plan`.
- **NEVER** mude US filhas que já estão em `Pronto para Dev` ou estágios posteriores.
- **ALWAYS** resolva o contrato da Feature antes de prosseguir — `successCriteria` próprio OU agregado dos AC das US filhas. Sem nenhum dos dois, pare.
- **ALWAYS** rode `/userstory-plan` sequencialmente, uma US por vez.
- **ALWAYS** responda ao usuário em português (CLAUDE.md).

## When NOT to use this skill

- Planejar uma única US → `/userstory-plan`
- Implementar a Feature após planejar → `/feature-implement`
- Criar US a partir do código existente → `/feature-code-sync`
- Remover Feature → `/feature-remove`
