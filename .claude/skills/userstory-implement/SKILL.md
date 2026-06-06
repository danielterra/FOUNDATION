---
name: userstory-implement
description: Use when the user (PO) asks to implement / develop a Foundation User Story that is already in status "Planejado" or "Pronto para Dev" — orchestrates execution by (1) calling the `architect` agent in Modo Execução **Briefing** to validate state, move the US to Em Progresso and produce per-dev briefings; (2) dispatching `developer-backend` and `developer-frontend` in parallel (or sequential when the plan says so) with the briefings; (3) calling the `architect` again in Modo Execução **Costura** with the dev returns to validate builds, consolidate `## Como testar` (merging UX's "Validação pelo usuário" checklist when present), log changelog and move the story to "Em Validação (QA)". Sub-agents cannot call other sub-agents in Claude — this skill (which runs in the main loop) IS the orchestrator. The PO never talks to devs directly; the skill mediates. Skill name follows entity-action convention.
disable-model-invocation: false
---

# Implement User Story

Skill que executa uma `foundation:UserStory` já planejada e a entrega para validação do QA, transitando dentro do ciclo de vida oficial:

```
Planejado / Pronto para Dev → Em Progresso → Em Validação (QA) → (QA valida) → Concluído
                                                              (↘ Mudança Pendente / Bloqueado / Rejeitado)
```

Esta skill cobre **Planejado → Em Progresso → Em Validação (QA)**. A transição final para `Concluído` é responsabilidade do QA depois da validação do usuário.

> **Por que a skill orquestra os devs (em vez de o architect chamar):** o harness do Claude não permite que um sub-agente invoque outro sub-agente. O `architect` é um sub-agente. As skills, ao contrário, rodam no main loop e PODEM disparar sub-agentes. Por isso o protocolo de Execução do `architect` foi dividido em **Briefing** (devolve prompts) e **Costura** (recebe retornos) — esta skill faz a ponte entre os dois.

---

## Pré-requisitos

A skill recebe a IRI da User Story como argumento (ex. `foundation:UserStory_1778074850125`). Se o usuário não informar, peça antes de prosseguir.

---

## Como executar

### Passo 1 — Acionar o Arquiteto em Modo 2a (Briefing)

Invoque o agente `architect` via `Agent` (subagent_type: `architect`) com prompt auto-contido. Ele valida estado, move a US para Em Progresso, lê o `implementationPlan` e devolve **briefings prontos** por dev.

Modelo de prompt para o `architect`:

```
Modo: Execução — 2a Briefing

US: <IRI da User Story>

Tarefa: executar o Modo 2a (Briefing) do Modo Execução conforme a sua persona:
- Validar pré-condições (status em Planejado / Pronto para Dev; `implementationPlan` preenchido com seção "Fatia de execução").
- Mover a US para `foundation:InProgress` via `replace_property_values`.
- Ler `capability` / `benefit` / `acceptanceCriteria` / `implementationPlan` inteiros.
- Classificar a fatia (Backend / Frontend / ambos) a partir da seção "Fatia de execução" do plano.
- Produzir um briefing PRONTO PARA EU DISPARAR a cada dev envolvido — incluindo, no briefing do `developer-frontend`, a seção `## UX/UI` INTEIRA do plano (contrato visual).
- Indicar a ordem de disparo (paralelo / sequencial e por quê).

NÃO invoque os devs — eu (esta skill) faço isso. Devolva os briefings junto com a ordem; eu disparo e reinvoco você em Modo 2b com os retornos consolidados.

Se o plano estiver errado/incompleto, NÃO desvie — retraia para `foundation:Status_1773581282341` (Mudança Pendente) e reporte para eu devolver ao PO.

Reporte de volta a esta skill conforme o "Relatório final — Modo 2a" da sua definição.
```

### Passo 2 — Disparar os devs conforme a ordem indicada

O `architect` devolve os briefings + a ordem. Aplique:

- **Ordem: paralelo** → invoque `developer-backend` e `developer-frontend` no MESMO bloco de tool calls (paralelo real), cada um com o seu briefing.
- **Ordem: sequencial (BE primeiro)** → invoque `developer-backend` sozinho; quando voltar, monte o briefing do `developer-frontend` substituindo o trecho "Contrato do backend (assinaturas…) <do plano>" pelo trecho equivalente do retorno BE; só então invoque `developer-frontend`.

Cada dev retorna o pacote no formato da sua persona (resumo, arquivos `path:linha`, build verde, seção `## Como testar`).

**Se algum dev retornar com build vermelho ou pedindo replan:**

- Build vermelho → reenvie ao mesmo dev pedindo correção (uma rodada de retry). Se ainda vermelho na segunda volta, leve ao Passo 4 com flag de blocker para o architect decidir.
- Pedido de replan (plano errado/incompleto) → vá direto ao Passo 4 com a observação; o architect retrai para Mudança Pendente.

### Passo 3 — Acionar o Arquiteto em Modo 2b (Costura)

Reinvoque o agente `architect` via `Agent` (subagent_type: `architect`) com os retornos dos devs.

Modelo de prompt:

```
Modo: Execução — 2b Costura

US: <IRI da User Story>

Retornos dos devs (consolidados, no formato exato em que cada um devolveu):

### Backend (`developer-backend`)
<colar o retorno completo do dev BE — incluindo Resumo, Arquivos tocados, Build, ## Como testar (fatia backend)>

### Frontend (`developer-frontend`)
<colar o retorno completo do dev FE — incluindo Resumo, Arquivos tocados, Componentes reusados/criados, Aderência à spec UX, Build, ## Como testar (fatia frontend)>

(Omitir a seção que não se aplica se uma das fatias era "nenhuma".)

Tarefa: executar o Modo 2b (Costura) conforme a sua persona:
- Validar que os builds estão verdes (vermelho de qualquer lado é blocker — me reporte).
- Validar cobertura de AC (a soma das fatias cobre todos os AC).
- Consolidar `## Como testar` mesclando os itens da lista "Validação pelo usuário" da seção `## UX/UI` do plano, quando existir.
- Atualizar `foundation:changelog` via `add_property_values` com a entrega de hoje (data atual: <YYYY-MM-DD do contexto>).
- Persistir plano (com `## Como testar` consolidado) + status `foundation:Status_1772600993751` (Em Validação (QA)) numa única `replace_property_values`.

Reporte de volta a esta skill conforme o "Relatório final — Modo 2b" da sua definição.
```

### Passo 4 — Lidar com blockers vindos do architect

Se o `architect` em Modo 2b sinalizar blocker (build vermelho persistente, cobertura de AC incompleta, plano divergente), repasse ao usuário com o briefing pronto para a correção (que vem do architect) e aguarde decisão antes de continuar.

### Passo 5 — Reportar ao usuário

Repasse o relatório final do `architect` (Modo 2b) ao usuário tal qual, e acrescente um convite explícito:

> "A US está em **Em Validação (QA)**. Acione o agente `qa` para validar pelos passos da seção `## Como testar` no `implementationPlan`."

Não faça commit nem push — `/code-commit` é skill separada e o usuário decide quando consolidar.

---

## Regras

- **NEVER** invoque o `architect` na esperança de que ele dispare os devs — o `architect` é sub-agente e NÃO pode invocar outros sub-agentes. Quem dispara é esta skill.
- **NEVER** invoque o `ux` aqui — UX só entra no Planejamento. A spec dele já está no plano como contrato.
- **NEVER** mova status, edite o `implementationPlan` ou rode `cargo`/`npm` aqui — tudo isso é responsabilidade do `architect` (status / plano) e dos devs (build).
- **NEVER** mova direto para `Concluído` — quem fecha é o QA, depois da validação do usuário.
- **ALWAYS** respeite a ordem indicada pelo architect (paralelo vs sequencial).
- **ALWAYS** passe a IRI da US aos agentes exatamente como o usuário forneceu — não troque por outra US "parecida".
- **ALWAYS** passe ao Modo 2b os retornos dos devs **na íntegra** — o architect precisa do material bruto para validar builds, cobertura de AC e consolidar o `## Como testar`.
- **ALWAYS** lembre-se: agentes (sub-agentes) **NÃO podem invocar outros sub-agentes**. Quem orquestra é esta skill, no main loop.
- **ALWAYS** responda ao usuário em português (CLAUDE.md).
