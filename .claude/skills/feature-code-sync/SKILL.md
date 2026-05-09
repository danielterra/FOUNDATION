---
name: feature-code-sync
description: Use when the user asks to reconcile a Foundation SoftwareFeature with the current codebase — investigates what is implemented, creates UserStories for capabilities that exist in code but not in the graph, marks UserStories as "Mudança Pendente" when the code drifted from the original AC, and flags stories that have no code at all. Skill name follows entity-action convention.
disable-model-invocation: false
---

# Sync Software Feature with Code

Skill que **reconcilia** uma `foundation:SoftwareFeature` com o estado atual da base de código. O grafo (ontologia) e o código tendem a divergir ao longo do tempo — esta skill identifica o delta e atualiza o grafo para refletir a realidade do código.

> **Esta skill NÃO implementa nada e NÃO planeja nada.** Ela apenas atualiza o grafo. Use `/feature-plan` ou `/userstory-plan` depois, em cima das US criadas/marcadas por aqui.

---

## Pré-requisitos

A skill recebe a IRI da SoftwareFeature como argumento (ex. `foundation:SoftwareFeature_1777574442863`).
Se o usuário não informar, peça antes de prosseguir.

---

## Instruções

### Passo 1 — Carregar Feature + US existentes

Em paralelo:

1. `describe_individual([<FeatureIRI>])` — capture `label`, `solvesProblem`, `successCriteria`, `partOfProject`.
2. `search(class_iri: "foundation:UserStory", filters: [{detail: "foundation:partOfFeature", value: "<FeatureIRI>"}], limit: 200)` — todas as US filhas.
3. Para cada US, `describe_individual` em batch (uma única chamada com array de IRIs) para obter `capability`, `acceptanceCriteria`, `implementationPlan`, `hasStatus`, `changelog`.

Anote internamente `(IRI, capability, AC, status, plan)` por US.

### Passo 2 — Mapear o código relacionado

Investigue o codebase:

- Use `Grep`/`Glob` por palavras-chave do `label` da Feature, dos `solvesProblem`, dos `successCriteria` e da `capability` de cada US.
- Identifique:
  - Componentes Svelte em `src/lib/components/**` e `src/routes/**` que pareçam pertencer ao tema da Feature.
  - Comandos Tauri em `src-tauri/src/commands/**` (procure `#[tauri::command]` próximos do tema).
  - MCP tools em `src-tauri/src/mcp/**` ou backlinks `foundation:implementedBy` apontando para a Feature.
  - Classes/properties da ontologia ligadas (use `describe_class` quando suspeitar).
- Para cada arquivo achado, anote `path` + um resumo de 1 linha do que ele faz.

Não invente arquivos. Se a busca não trouxer nada do tema, registre "sem código encontrado" — é informação válida.

### Passo 3 — Cruzar US ↔ código

Para cada US existente:

a) **Tem código correspondente e AC bate** → não fazer nada.
b) **Tem código, mas o código foi além/diferente do AC original** → marcar como **Mudança Pendente**:
   - Status → `foundation:Status_1773581282341`.
   - Atualizar `acceptanceCriteria` para refletir o comportamento atual do código (use AC concretos, observáveis).
   - Adicionar entrada no `changelog` via `add_property_values`:
     `<YYYY-MM-DD> — code-sync: AC ajustado ao código atual em <path:linha>. Replanejar via /userstory-plan.`
c) **Não tem código nenhum** → marcar como **Mudança Pendente** (não cancelar — só o usuário decide cancelar):
   - Status → `foundation:Status_1773581282341`.
   - `changelog`: `<YYYY-MM-DD> — code-sync: nenhum código encontrado para a US; candidata a cancelamento.`
   - **Não** apague nem altere `capability`/`benefit` — preservar a intenção original para o usuário decidir.

Para **funcionalidades no código sem US**:

- Crie nova `foundation:UserStory` com `assert_individual`:
  - `label`: descrição curta da capability (3-6 palavras).
  - `foundation:partOfFeature`: `<FeatureIRI>`.
  - `foundation:capability`: o que o usuário pode fazer com aquele código (uma frase).
  - `foundation:benefit`: o ganho (inferir do contexto; se não for óbvio, usar `"a confirmar"`).
  - `foundation:acceptanceCriteria`: AC concretos baseados no comportamento observável do código (3-6 itens, um por linha).
  - `foundation:hasStatus`: `foundation:Pending` (a US é nova, ainda não foi planejada).
  - `foundation:changelog`: `<YYYY-MM-DD> — criada por code-sync a partir de <path:linha>.`

Não invoque `/userstory-plan` daqui — sync entrega a US em `Pendente` para o usuário decidir se planeja, ajusta ou cancela.

### Passo 4 — Atualizar a Feature

Adicione (via `add_property_values`) ao `foundation:changelog` da Feature uma linha-resumo:

```
<YYYY-MM-DD> — code-sync executado. Criadas: <N>. Atualizadas: <N>. Sem código: <N>.
```

**Não mude o `hasStatus` da Feature** — sync não promove nem rebaixa o status da Feature em si. Se as US ficaram em Mudança Pendente e o usuário quiser refletir isso na Feature, ele move manualmente ou roda `/feature-plan` depois.

### Passo 5 — Reportar

Mostre ao usuário, em formato tabular conciso:

```
Feature: <label> (<IRI>)

US criadas (status: Pendente):
- <IRI> — <label> [origem: <path>]
- ...

US atualizadas (status: Mudança Pendente):
- <IRI> — <label> [motivo: AC drift / sem código]
- ...

US ok (não alteradas):
- <N> US sem mudanças.

Próximos passos sugeridos:
1. Revise as US criadas — capability/benefit foram inferidos e podem precisar de ajuste manual.
2. Para cada US em Mudança Pendente, decida: replanejar (`/userstory-plan`) ou cancelar (status → Cancelado).
3. Quando o backlog estiver limpo, rode `/feature-plan` para alinhar a Feature.
```

Mantenha o relatório navegável — IRIs como `foundation:UserStory_xxx`, paths como `src/lib/.../File.svelte:42`.

---

## Regras

- **NEVER** retract User Stories aqui — apenas mude status. Retract definitivo é responsabilidade de `/feature-remove` ou ação explícita do usuário.
- **NEVER** mude a Feature para `Cancelado`/`Concluído` automaticamente.
- **NEVER** invente arquivos ou linhas — toda referência a código deve vir de busca real (Grep/Glob/Read).
- **NEVER** sobrescreva `capability`/`benefit` de US existente — só atualize `acceptanceCriteria` quando o código justifica.
- **NEVER** invoque `/userstory-plan` ou `/userstory-implement` daqui — sync entrega o backlog atualizado e termina.
- **ALWAYS** registre cada mudança no `changelog` da US (ou da Feature) com a data de hoje e a referência a `path:linha`.
- **ALWAYS** trate "sem código" como sinal para o usuário decidir — não como motivo para retract.
- **ALWAYS** crie US faltantes com `hasStatus = foundation:Pending`, nunca direto em Planejado.
- **ALWAYS** responda ao usuário em português (CLAUDE.md).

## When NOT to use this skill

- Planejar User Stories existentes → `/feature-plan` ou `/userstory-plan`.
- Implementar User Stories planejadas → `/feature-implement` ou `/userstory-implement`.
- Remover Feature inteira → `/feature-remove`.
- Adicionar comportamento novo no código sem passar pelo backlog → não use sync; crie a US manualmente ou via `/feature-plan`.
