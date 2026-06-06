---
name: bug-fix
description: Use when the user (PO) asks to fix a Foundation bug by IRI — e.g. "fix bug foundation:Bug_123", "corrija o bug foundation:Bug_456", "resolva o bug <IRI>". Orchestrates the bug pipeline: `support` (investigation & technical dossier) → `architect` Modo Triagem **Briefing** (decide BE/FE/both and produce per-dev briefings) → dispatch `developer-backend` / `developer-frontend` with the briefings → `architect` Modo Triagem **Costura** (validate builds, consolidate `## Como testar`, move to Em Validação (QA)) → `qa` (validation gate before Concluído). Sub-agents cannot call other sub-agents in Claude — this skill (which runs in the main loop) IS the orchestrator. The PO never invokes devs directly; the bug is NEVER closed without QA validation. Always invoke this skill when the user mentions a bug IRI.
---

# Bug Fix

Skill que conduz um `foundation:Bug` por todo o pipeline do FOUNDATION, dentro do ciclo:

```
Pendente → [Support] Pronto para Dev → [Architect Triagem Brief] Em Progresso → [Devs] → [Architect Costura] Em Validação (QA) → [QA] Concluído
                                                                                                                                                (↘ Mudança Pendente)
```

O PO nunca fala com dev direto; o bug **nunca** é fechado sem passar pelo `qa`.

> **Por que a skill orquestra todos os passos:** o harness do Claude não permite que um sub-agente invoque outro sub-agente. O `architect` (e qualquer outro agente) é sub-agente. As skills, ao contrário, rodam no main loop e PODEM disparar sub-agentes. Por isso o Modo Triagem do `architect` foi dividido em **Briefing** (devolve prompts) e **Costura** (recebe retornos) — esta skill faz a ponte entre os dois e dispara os devs.

---

## Pré-requisitos

A skill recebe a IRI do bug como argumento (ex. `foundation:Bug_1780369248862`). Se o usuário não informar, peça antes de prosseguir.

---

## Como executar

Esta skill é o orquestrador completo do pipeline de bug. O PO não investiga, não distribui, não corrige, não fecha — só aciona.

### Passo 1 — `support` produz o dossiê técnico

Invoque o agente `support` via `Agent` (subagent_type: `support`) com prompt auto-contido:

```
Bug: <IRI>

Tarefa: investigue conforme o protocolo do agente Support — reproduza o sintoma (ou descreva como reproduzir), siga a ordem CLAUDE.md (logs → mensagens → DB → código), formule causa provável, mapeie camadas afetadas e arquivos suspeitos (path:linha), persista o dossiê no Bug e mova para `foundation:Status_1773079329634` (Pronto para Desenvolvimento).

Não corrija código. Não invoque outros agentes. Reporte de volta a esta skill o dossiê + indicação do próximo passo.
```

Se o `support` parar (app não rodando, IRI inválido, bug sem evidência coletável), reporte ao usuário e aguarde.

### Passo 2 — `architect` em Modo 3a (Triagem — Briefing)

Quando o `support` retorna com o bug em **Pronto para Dev** e dossiê preenchido, invoque o agente `architect` via `Agent` (subagent_type: `architect`):

```
Modo: Triagem de Bug — 3a Briefing

Bug: <IRI>

Tarefa: executar o Modo 3a (Briefing) do Modo Triagem conforme a sua persona:
- Ler o dossiê do `support` (causeAnalysis + stepsToReproduce + expectedBehavior + camadas afetadas).
- Decidir a fatia (developer-backend / developer-frontend / ambos).
- Mover o bug para `foundation:InProgress` via `replace_property_values`.
- Produzir um briefing PRONTO PARA EU DISPARAR a cada dev envolvido.

NÃO invoque os devs — eu (esta skill) faço isso. Devolva os briefings; eu disparo e reinvoco você em Modo 3b com os retornos.

Se o dossiê estiver incompleto, NÃO investigue — reporte para eu acionar o `support` de novo.

Reporte de volta a esta skill conforme o "Relatório final — Modo 3a" da sua definição.
```

Se o `architect` reportar que o dossiê está incompleto, retorne ao Passo 1.

### Passo 3 — Disparar os devs com os briefings

O `architect` devolve os briefings + a fatia (BE / FE / ambos). Aplique:

- **Apenas BE** → invoque `developer-backend` com o briefing.
- **Apenas FE** → invoque `developer-frontend` com o briefing.
- **Ambos** → invoque `developer-backend` e `developer-frontend` no MESMO bloco de tool calls (paralelo), cada um com o seu briefing.

Cada dev retorna o pacote no formato da sua persona (resumo do fix, arquivos `path:linha`, build verde, seção `## Como testar` da fatia).

**Build vermelho persistente após uma rodada de correção** → leve ao Passo 4 com flag de blocker para o architect decidir (em geral retraindo para Mudança Pendente).

### Passo 4 — `architect` em Modo 3b (Triagem — Costura)

Reinvoque o agente `architect` via `Agent` (subagent_type: `architect`) com os retornos dos devs:

```
Modo: Triagem de Bug — 3b Costura

Bug: <IRI>

Retornos dos devs (consolidados, no formato exato em que cada um devolveu):

### <Backend e/ou Frontend — conforme aplicável>
<colar os retornos completos>

Tarefa: executar o Modo 3b (Costura) conforme a sua persona:
- Validar que os builds estão verdes (vermelho de qualquer lado é blocker — me reporte).
- Concatenar `## Como testar` produzido pelo(s) dev(s) num único bloco anexado ao Bug (em `foundation:causeAnalysis` ou propriedade dedicada se existir).
- `add_property_values` em `foundation:changelog`: `<YYYY-MM-DD do contexto> — fix entregue. <BE/FE/ambos>: <arquivos>. Encaminhado ao QA.`
- `replace_property_values` em `foundation:hasStatus` ← `foundation:Status_1772600993751` (Em Validação (QA)).

Reporte de volta a esta skill conforme o "Relatório final — Modo 3b" da sua definição.
```

### Passo 5 — `qa` valida e fecha (ou abre regressão)

Quando o `architect` retorna com o bug em **Em Validação (QA)**, invoque o agente `qa` via `Agent` (subagent_type: `qa`):

```
Tarefa: valide o Bug <IRI> conforme o protocolo de validação de Bug do QA — rode a suíte (cargo test + npm run check), reproduza os stepsToReproduce e confirme que o sintoma não acontece mais e que o comportamento bate com `expectedBehavior`. Analise logs durante a reprodução.

Decisão:
- ✅ Sintoma não reproduz + comportamento esperado + suíte verde → fechar o bug em `foundation:Completed` com veredito QA em `causeAnalysis`.
- ❌ Ainda reproduz ou regressão nova → registrar Bug novo (regressão / fix incompleto) e devolver o original para `foundation:Status_1773581282341` (Mudança Pendente).
- ⚠️ Manual → manter Em Validação (QA) e pedir confirmação do usuário.

Se o bug estava ligado a uma US (`bugOfUserStory`), sinalize se a US precisa voltar para Em Validação também.

Não invoque outros agentes. Reporte de volta a esta skill o veredito.
```

### Passo 6 — Reportar ao usuário

Repasse o veredito do QA ao usuário com convite explícito ao próximo passo:
- Veredito ✅ → bug fechado, nada a fazer.
- Veredito ❌ → bug novo registrado; rode `/bug-fix <IRI do novo bug>` para reabrir o ciclo.
- Veredito ⚠️ → guia de teste manual; aguardar confirmação do usuário antes de fechar.

---

## Regras

- **NEVER** invoque o `architect` na esperança de que ele dispare os devs — o `architect` é sub-agente e NÃO pode invocar outros sub-agentes. Quem dispara é esta skill.
- **NEVER** invoque `developer-backend`, `developer-frontend` ou `support` fora da ordem prescrita — `support` antes, `architect` (Briefing) depois, devs, `architect` (Costura), `qa` por último.
- **NEVER** feche o bug aqui — Concluído é responsabilidade exclusiva do `qa` após validação.
- **NEVER** pule o `support` mesmo que o bug pareça "óbvio" — o dossiê é o input do Arquiteto, não uma formalidade.
- **NEVER** pule o `qa` mesmo que o fix pareça trivial — todo bug (e toda US) passa pelo QA antes de fechar.
- **ALWAYS** passe a IRI do bug exatamente como o usuário forneceu.
- **ALWAYS** passe ao Modo 3b os retornos dos devs **na íntegra** — o architect precisa do material bruto para validar builds e consolidar o `## Como testar`.
- **ALWAYS** lembre-se: agentes (sub-agentes) **NÃO podem invocar outros sub-agentes**. Quem orquestra é esta skill, no main loop.
- **ALWAYS** responda ao usuário em português (CLAUDE.md).
