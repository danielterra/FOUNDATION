---
name: userstory-plan
description: Use when the user (PO) asks to plan a Foundation User Story (e.g. foundation:UserStory) — orchestrates planning by FIRST checking the UX heuristic (does the story touch interface?), calling the `ux` agent ahead of time when it does, then delegating to the `architect` agent in Modo Planejamento with the UX section already in hand. The architect analyzes Project + Feature + Story context, produces the architecture-level plan, stores it in foundation:implementationPlan and moves the story to status "Planejado". Sub-agents cannot call other sub-agents in Claude — this skill (which runs in the main loop) IS the orchestrator. The PO never talks to devs/UX directly; the skill mediates. Skill name follows entity-action convention.
disable-model-invocation: false
---

# Plan User Story

Skill que conduz uma `foundation:UserStory` da fase **Pendente** para **Planejado** dentro do ciclo de vida oficial:

```
Pendente → Planejado → Pronto para Dev → Em Progresso → Em Validação (QA) → Concluído
                                                                          (↘ Bloqueado / Cancelado / Rejeitado)
```

Esta skill cobre apenas a transição **Pendente → Planejado**: ela exige que a story tenha `capability`, `benefit` e `acceptanceCriteria` definidos, e produz um plano de arquitetura textual gravado em `foundation:implementationPlan`.

> **Por que a skill orquestra `ux` + `architect` aqui (em vez de o architect chamar `ux` sozinho):** o harness do Claude não permite que um sub-agente invoque outro sub-agente. As skills, ao contrário, rodam no main loop do Claude principal e PODEM disparar sub-agentes. Por isso a orquestração mora aqui.

---

## Pré-requisitos

A skill recebe a IRI da User Story como argumento (ex. `foundation:UserStory_1778074850125`). Se o usuário não informar, peça antes de prosseguir.

---

## Como executar

### Passo 1 — Decidir se UX é necessário (heurística)

Antes de chamar o architect, faça uma leitura rápida da US:

1. `describe_individual([<US IRI>])` — capture `capability`, `acceptanceCriteria`, `userRole`.

A **heurística de UX dispara** quando QUALQUER destes for verdadeiro:

- `capability` ou qualquer item de `acceptanceCriteria` menciona: interface, widget, tela, formulário, visualização, página, view, botão, modal, sidebar, dashboard, inspetor, chat, lousa, blackboard, ícone, hover, click, drag, input, picker, select, checkbox, accordion, popup, toast, NotificationBell, header.
- `userRole` aponta para `foundation:Persona_1772476248172` (João — não-técnico) ou `foundation:Persona_1773783644387` (Daniel — power user) **com qualquer menção a interação visível** nos AC.
- A US, pela `capability` + AC, claramente exige tocar em `src/**` (componentes Svelte, rotas, stores de UI).

Se NENHUM critério dispara, marque "UX: não aplicável (US backend pura)" e vá direto para o Passo 3.

### Passo 2 — Quando UX é necessário, invocar o agente `ux` PRIMEIRO

Invoque o agente `ux` via `Agent` (subagent_type: `ux`) com prompt **auto-contido** (o sub-agente não verá esta conversa). Para o briefing, monte rapidamente o contexto:

- `describe_individual` da Feature (via `partOfFeature`) → `solvesProblem`, `successCriteria`.
- `describe_individual` do Project (via `partOfProject` da Feature) → `hasObjective`, `usesMethodology`.

Prompt para o `ux`:

```
Modo: Spec UX

US: <IRI da User Story>

Contexto:
- Produto: <label do Project> (<IRI>)
- Funcionalidade: <label da Feature> (<IRI>)
- História: "Como <userRole label> quero <capability> para <benefit>"
- Critérios de Aceitação:
  1. <AC #1>
  2. <AC #2>
  ...

Decisão de arquitetura preliminar (alto nível, sem código): nenhuma ainda — esta skill ainda não chamou o `architect`. Foque na forma de uso (componentes a reutilizar, wireframe, estados, acessibilidade). O `architect` vai correlacionar a sua spec à arquitetura no próximo passo.

Tarefa: produza a seção `## UX/UI` no formato definido na sua persona — componentes a reutilizar (paths reais), wireframe textual inequívoco, fluxo de interação, estados loading/vazio/erro/sucesso, acessibilidade, heurísticas Nielsen aplicadas, mapeamento AC↔UX, e a lista "Validação pelo usuário" com 3-6 itens binários observáveis.

Devolva APENAS a seção `## UX/UI` em Markdown pronta para embutir no `implementationPlan`. NÃO persista nada na ontologia. Reporte de volta a esta skill o resultado dessa única seção.
```

Se o `ux` parar (US sem AC suficiente, contexto faltando), repasse ao usuário e aguarde — não avance para o Passo 3.

### Passo 3 — Invocar o agente `architect` em Modo Planejamento

Invoque o agente `architect` via `Agent` (subagent_type: `architect`) com prompt auto-contido. **Quando o Passo 2 rodou**, inclua a seção `## UX/UI` retornada pelo `ux` no prompt para o architect embutir direto no `implementationPlan`. **Quando o Passo 2 foi pulado** (heurística não disparou), avise o architect que UX não se aplica.

Modelo de prompt para o `architect`:

```
Modo: Planejamento

US: <IRI da User Story>

UX/UI: <um destes três modos>
  (A) "Já produzida — embuta esta seção no `implementationPlan`:" + <colar a seção `## UX/UI` retornada pelo agente `ux` no Passo 2>
  (B) "Não aplicável (US backend pura)."
  (C) "Pendente — caso a sua análise de arquitetura confirme que toca interface e a heurística desta skill falhou em detectar, devolva o briefing para `ux` que esta skill vai disparar."

Tarefa: planejar a arquitetura desta US conforme o protocolo do Modo 1 — carregar contexto Projeto → Feature → História, validar invariantes (capability/benefit/acceptanceCriteria preenchidos), mapear estado atual no codebase em modo leitura, enumerar pelo menos 2 alternativas, decidir, e produzir o desenho completo no formato definido na sua persona (Contexto / Estado atual / Decisão de arquitetura / Mapeamento por camada / UX/UI conforme acima / Fluxo end-to-end / Alternativas / Trade-offs / Riscos / ACs↔Arquitetura / Fatia de execução).

Persistir plano + status (foundation:Status_1772596341042 — Planejado) em uma única `replace_property_values` — SÓ quando a seção UX/UI estiver resolvida (vinda do Passo 2 ou marcada "não aplicável"). Se você concluir que UX é necessário mas eu (a skill) não passei a seção, NÃO persista — devolva o briefing pronto para `ux` e me reporte.

Se faltar capability, benefit ou acceptanceCriteria, NÃO planeje — pare e reporte.

Reporte de volta a esta skill em até 8 linhas conforme o "Relatório final — Modo 1" da sua definição.
```

### Passo 4 — Caso o architect peça UX a posteriori

Se o `architect` retornar dizendo "preciso da seção UX/UI" (heurística desta skill falhou em detectar; ele detectou no protocolo dele) e devolver o briefing para o `ux`:

1. Dispare o agente `ux` com o briefing exato que o architect devolveu.
2. Quando o `ux` retornar a seção, reinvoque o `architect` em Modo Planejamento passando a seção pronta (volta ao Passo 3 modo A).
3. Aí sim ele persiste e move o status.

### Passo 5 — Reportar ao usuário

Repasse o relatório final do `architect` ao usuário tal qual, e acrescente um convite explícito:

> "Plano gravado. Quando quiser executar, peça `/userstory-implement <IRI>`."

---

## Regras

- **NEVER** invoque `developer-backend` ou `developer-frontend` daqui — eles só entram em Execução (`/userstory-implement`).
- **NEVER** invoque o `architect` antes de resolver a heurística de UX (Passo 1) — economiza um round-trip.
- **NEVER** persista plano nem mude status aqui — isso é responsabilidade do `architect` na sua chamada final.
- **NEVER** invente IRIs de status — o architect usa `foundation:Status_1772596341042` (Planejado).
- **NEVER** crie a User Story se ela não existir — esta skill é só de planejamento.
- **ALWAYS** passe a IRI da US aos agentes exatamente como o usuário forneceu.
- **ALWAYS** lembre-se: agentes (sub-agentes) **NÃO podem invocar outros sub-agentes**. Quem orquestra é esta skill, no main loop.
- **ALWAYS** responda ao usuário em português (CLAUDE.md).
