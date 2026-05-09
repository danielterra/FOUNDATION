---
name: feature-remove
description: Use when permanently removing a Foundation SoftwareFeature — lists all dependent UserStories and other backlinks, retracts each child UserStory, then retracts the Feature individual. Skill name follows entity-action convention.
disable-model-invocation: false
---

# Remove Software Feature

Skill que remove permanentemente uma `foundation:SoftwareFeature` e suas `foundation:UserStory` filhas do grafo, juntamente com referências dependentes.

> **AVISO**: esta skill é **destrutiva**. Use somente quando a Feature inteira foi cancelada e nenhum trabalho dela deve permanecer no kanban. Para cancelar mantendo o histórico, mude o status para `foundation:Status_1772570972069` (Cancelado) sem retract.

---

## Pré-requisitos

A skill recebe a IRI da SoftwareFeature como argumento (ex. `foundation:SoftwareFeature_1777574442863`).
Se o usuário não informar, peça antes de prosseguir.

---

## Instruções

### Passo 1 — Levantar o impacto

Em paralelo:

1. `describe_individual([<FeatureIRI>])` — confirme que existe e capture `label`, `hasStatus`.
2. `search(class_iri: "foundation:UserStory", filters: [{detail: "foundation:partOfFeature", value: "<FeatureIRI>"}], limit: 200)` — lista de US filhas.
3. Outros backlinks da Feature: olhe `incomingProperties` no `describe_class("foundation:SoftwareFeature")` para mapear quem mais aponta. Tipicamente:
   - `foundation:Persona`, `foundation:Screen`, `foundation:Status`, `foundation:Automation`, `foundation:MetaProperty` via `partOfFeature` (mesma propriedade que UserStory).
   - `foundation:TauriCommand` / `foundation:MCPTool` via `foundation:implementedBy`.
   - `foundation:MetaAbstractTask` via `foundation:implementsFeature`.
4. Para cada um desses, rode um `search` similar para listar quem aponta. **Não retract entidades de outros tipos automaticamente** — apenas reporte.

### Passo 2 — Pedir confirmação

Antes de remover qualquer coisa, mostre ao usuário **a lista completa do impacto**:

```
Feature: <label> (<IRI>)
Status atual: <label do status>

Será retract:
- <N> UserStory(s) filha(s):
  - <IRI> — <label> [<status>]
  - ...

Outros backlinks (NÃO serão removidos automaticamente — você decide depois):
- <N> Persona(s): <lista>
- <N> Screen(s): <lista>
- <N> TauriCommand(s) implementando esta feature: <lista>
- ...
```

**Pare e pergunte explicitamente**: "Confirmar remoção de <N> User Stories e da Feature? (sim/não)"

Se o usuário negar ou pedir cancelamento em vez de retract, **pare** — sugira mudar `hasStatus` para `foundation:Status_1772570972069` (Cancelado) via `replace_property_values`.

### Passo 3 — Retract das User Stories filhas

Para cada US filha, **uma a uma**:

```
retract_individual({ iri: "<UserStoryIRI>" })
```

Faça sequencialmente para coletar erros individualmente. Se alguma falhar (por exemplo, retract bloqueado por dependência), **pare** e reporte ao usuário antes de prosseguir.

> Não existe `/userstory-remove` formal — `retract_individual` é a operação canônica.

### Passo 4 — Retract da Feature

Após todas as US filhas terem sido retraídas:

```
retract_individual({ iri: "<FeatureIRI>" })
```

Se o retract falhar porque ainda há backlinks de outras entidades (Persona, Screen, TauriCommand, etc.), **pare** e reporte. O usuário precisa decidir se quer:

- Retract manualmente cada backlink restante, OU
- Manter a Feature mas marcá-la como Cancelado (`foundation:Status_1772570972069`).

### Passo 5 — Reportar

Mostre ao usuário, em até 8 linhas:

- IRI e label da Feature removida.
- Quantas US filhas foram retraídas (lista de IRIs).
- Backlinks remanescentes que **não** foram tocados (Persona, Screen, etc.) e a recomendação para cada.
- Se a Feature foi retraída com sucesso ou se está pendente por backlinks.

Não faça commit nem push automático — `/code-commit` é skill separada.

---

## Regras

- **NEVER** retract a Feature antes de retract de **todas** as US filhas — viola integridade referencial.
- **NEVER** retract Persona/Screen/TauriCommand/MCPTool/MetaProperty automaticamente — eles podem servir outras Features. Apenas reporte.
- **NEVER** prossiga sem confirmação explícita do usuário no Passo 2 — operação destrutiva.
- **NEVER** use `replace_property_values` com lista vazia para "apagar" — use `retract_individual`.
- **ALWAYS** levante o impacto completo antes de pedir confirmação.
- **ALWAYS** processe US filhas sequencialmente para coletar falhas individuais.
- **ALWAYS** sugira `Cancelado` (status) como alternativa não-destrutiva ao retract.
- **ALWAYS** responda ao usuário em português (CLAUDE.md).

## When NOT to use this skill

- Apenas mudar status para `Cancelado` mantendo histórico → use `replace_property_values` com `foundation:hasStatus`.
- Remover apenas uma US específica, mantendo a Feature → `retract_individual` direto na US.
- Sincronizar US com código (remover obsoletas, criar faltantes) → `/feature-code-sync`.
