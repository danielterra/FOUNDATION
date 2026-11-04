---
name: userstory-implement
description: "Use when the user (PO) asks to implement / develop a Foundation User Story that is already in status \"Planejado\" or \"Pronto para Dev\" — orchestrates execution by (1) calling the `architect` agent in Modo Execução **Briefing** to validate state, move the US to Em Progresso and produce per-dev briefings; (2) dispatching `developer-backend` and `developer-frontend` in parallel (or sequential when the plan says so) with the briefings; (3) calling the `architect` again in Modo Execução **Costura** with the dev returns to validate builds, consolidate `## Como testar` (merging UX's \"Validação pelo usuário\" checklist when present), log changelog and move the story to \"Em Validação (QA)\". This skill runs in the primary Codex agent and explicitly orchestrates the specialist subagents. The PO never talks to devs directly; the skill mediates. Skill name follows entity-action convention."
---

# Implement User Story

Skill que executa uma `foundation:UserStory` já planejada e a entrega para validação do QA, transitando dentro do ciclo de vida oficial:

```
Planejado / Pronto para Dev → Em Progresso → Em Validação (QA) → (QA valida) → Concluído
                                                              (↘ Mudança Pendente / Bloqueado / Rejeitado)
```

Esta skill cobre **Planejado → Em Progresso → Em Validação (QA)**. A transição final para `Concluído` é responsabilidade do QA depois da validação do usuário.

> **Por que a skill orquestra os devs (em vez de o architect chamar):** por desenho deste fluxo a orquestração entre papéis permanece no agente principal. O `architect` é um sub-agente. A skill roda no agente Codex principal e dispara os subagentes especializados de forma explícita. Por isso o protocolo de Execução do `architect` foi dividido em **Briefing** (devolve prompts) e **Costura** (recebe retornos) — esta skill faz a ponte entre os dois.

---

## Pré-requisitos

A skill recebe a IRI da User Story como argumento (ex. `foundation:UserStory_1778074850125`). Se o usuário não informar, peça antes de prosseguir.

---

## Como executar

### Passo 1 — Acionar o Arquiteto em Modo 2a (Briefing)

Invoque o agente `architect` como subagente customizado (agente: `architect`) com prompt auto-contido. Ele valida estado, move a US para Em Progresso, lê o `implementationPlan` e devolve **briefings prontos** por dev.

Modelo de prompt para o `architect`:

```
Modo: Execução — 2a Briefing

US: <IRI da User Story>

Tarefa: executar o Modo 2a (Briefing) do Modo Execução conforme a sua persona:
- Validar pré-condições (status em Planejado / Pronto para Dev; `implementationPlan` preenchido com seção "Fatia de execução").
- Ler a lista atual de ACs e status via MCP; confirmar que todos têm status, pertencem ao escopo da US e serão cobertos pelo plano. Se um AC foi criado/alterado após QA ou conclusão, encaminhar a US para Mudança Pendente e replanejar antes de iniciar.
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

Reinvoque o agente `architect` como subagente customizado (agente: `architect`) com os retornos dos devs.

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
- Comparar a lista planejada com todos os ACs atuais da ontologia; ausência, duplicidade ou critério sem cobertura bloqueia a costura até replanejamento.
- Confirmar que o retorno do `developer-frontend` registra um smoke visual via Computer Use no navegador local; a validação independente dos ACs de UI fica com o `qa`, que também usa Computer Use visível.
- Consolidar `## Como testar` mesclando os itens da lista "Validação pelo usuário" da seção `## UX/UI` do plano, quando existir.
- Atualizar `foundation:changelog` via `add_property_values` com a entrega de hoje (data atual: <YYYY-MM-DD do contexto>).
- Persistir plano (com `## Como testar` consolidado) + status `foundation:Status_1772600993751` (Em Validação (QA)) numa única `replace_property_values`.
- Não alterar status de AC nem concluir a US; QA recebe a lista completa para validar e registrar evidência AC a AC.

Reporte de volta a esta skill conforme o "Relatório final — Modo 2b" da sua definição.
```

### Passo 4 — Acionar QA imediatamente

Assim que a costura mover a US para Em Validação (QA), invoque o agente `qa` nesta mesma execução. Não deixe a validação como uma ação que o usuário precisa lembrar de pedir.

Prompt mínimo para QA:

```
 Valide a User Story <IRI> com base nos ACs atuais e na seção `## Como testar` do implementationPlan. Rode os checks requeridos e valide cada AC por evidência concreta. Persista o status e a evidência de cada AC assim que o resultado individual estiver confirmado; não espere o fim da suíte para atualizar ACs já decididos. Leia de volta via MCP cada status persistido. Ao encerrar a rodada, persista também o status do parent: Completed se todos os ACs passaram e os checks obrigatórios estão verdes; Mudança Pendente se um AC falhou e precisa de correção; Blocked se não for possível continuar sem ação externa ou se a evidência/changelog não persistirem. Em Blocked, registre a ação exata que destrava a validação. Use Em Validação (QA) somente enquanto a rodada de validação estiver ativa. Registre a evidência no changelog e confirme a entrada por readback; se a leitura direta estiver truncada, use busca pelo valor. Releia o parent e todos os ACs antes de reportar.
```

QA continua sendo o único responsável por validar e concluir; o implementador e o arquiteto não alteram status de AC.

### Passo 5 — Lidar com blockers vindos do architect ou QA

Se o `architect` em Modo 2b sinalizar blocker (build vermelho persistente, cobertura de AC incompleta, plano divergente), repasse ao usuário com o briefing pronto para a correção (que vem do architect) e aguarde decisão antes de continuar. Se QA devolver a US como Blocked ou Mudança Pendente, informe a causa e o próximo passo; não restaure o status sem remover o blocker e executar nova validação.

### Passo 6 — Reportar ao usuário

Repasse o relatório final do QA ao usuário, incluindo o status lido de volta da US e de cada AC. Se concluído, confirme que a US e todos os ACs estão Concluídos. Se bloqueado, informe a condição externa concreta e a ação necessária para retomar.

Não faça commit nem push — `$code-commit` é skill separada e o usuário decide quando consolidar.

---

## Regras

- **NEVER** invoque o `architect` na esperança de que ele dispare os devs — o `architect` é sub-agente e NÃO pode invocar outros sub-agentes. Quem dispara é esta skill.
- **NEVER** invoque o `ux` aqui — UX só entra no Planejamento. A spec dele já está no plano como contrato.
- **NEVER** mova status, edite o `implementationPlan` ou rode `cargo`/`npm` aqui — tudo isso é responsabilidade do `architect` (status / plano) e dos devs (build).
- **NEVER** mova direto para `Concluído` — quem fecha é o QA depois de validar os critérios; só peça confirmação manual quando um AC realmente depender de uma observação do usuário.
- **ALWAYS** respeite a ordem indicada pelo architect (paralelo vs sequencial).
- **ALWAYS** passe a IRI da US aos agentes exatamente como o usuário forneceu — não troque por outra US "parecida".
- **ALWAYS** passe ao Modo 2b os retornos dos devs **na íntegra** — o architect precisa do material bruto para validar builds, cobertura de AC e consolidar o `## Como testar`.
- **ALWAYS** lembre-se: a orquestração entre papéis permanece nesta skill, executada pelo agente principal.
- **ALWAYS** responda ao usuário em português (AGENTS.md).
