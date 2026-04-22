# Diretrizes do Assistente de IA para o Projeto FOUNDATION

## Regra Meta

**CRÍTICO: Sempre que o usuário corrigir você ou indicar uma preferência, ATUALIZE IMEDIATAMENTE este CLAUDE.md.**

Adicione regras explícitas para feedback negativo; documente diretivas "sempre faça X / nunca faça Y"; confirme ao usuário após atualizar.

## Depuração

**Fluxo de trabalho:** logs → histórico de mensagens → banco de dados → código (nesta ordem). Nunca faça perguntas que você mesmo pode responder.

### Logs da Aplicação

```bash
npm run logs          # últimas 100 linhas
npm run logs 500      # últimas 500 linhas
npm run logs 1000     # últimas 1000 linhas
# ou diretamente:
tail -f ~/Library/Application\ Support/org.w3id.foundation/application.log
```

### Histórico de Mensagens

As mensagens de chat usam a classe `foundation:AIConversationMessage` na tabela `triples`:

- `foundation:role` / `foundation:content` — literais em `object_value`
- `foundation:sentAt` — timestamp Unix ms em `object_datetime` (integer), NÃO em `object_value`
- `foundation:partOfConversation` / `foundation:sender` / `foundation:receiver` — IRIs em `object`

### Inspeção do Banco de Dados

```sql
sqlite3 ~/Documents/Foundation/FOUNDATION.db ".tables"
sqlite3 ~/Documents/Foundation/FOUNDATION.db ".schema table_name"
sqlite3 ~/Documents/Foundation/FOUNDATION.db "SELECT subject, predicate, COALESCE(object, object_value) FROM triples ORDER BY rowid DESC LIMIT 20;"
```

## Banco de Dados & Armazenamento

- **DB**: `~/Documents/Foundation/FOUNDATION.db`
- **Logs**: `~/Library/Application Support/org.w3id.foundation/application.log`
- **NUNCA deletar o banco de dados** em nenhuma circunstância
- **NUNCA executar INSERT/UPDATE/DELETE/DROP/TRUNCATE** sem confirmação explícita do usuário — apenas SELECT

### Estrutura da Tabela `triples`

- **`object`**: IRIs/blank nodes (`object_type = 'iri'` ou `'blank'`)
- **`object_value`**: valor lexical literal (`object_type = 'literal'`)
- **`object_datatype`**: ex. `xsd:string`, `xsd:integer`, `xsd:dateTime`
- Use `COALESCE(object, object_value)` quando o tipo for desconhecido

### Modelo de Imutabilidade (Datomic-style)

**A camada OWL usa a maior transação (`tx`) como fonte da verdade — NÃO usa o campo `retracted`.**

#### Atualização de valores — TX é a verdade

- Para "atualizar" um valor: basta inserir um novo triple com `tx` maior. O valor mais recente vence.
- **NUNCA** presuma que é necessário fazer `retracted = 1` no triple antigo antes de inserir o novo.
- O campo `retracted` existe mas **não é o mecanismo de versionamento** — `tx DESC LIMIT 1` é.

#### `retracted = 1` — somente para apagar fatos

- `retracted` serve exclusivamente para **dizer que um fato deixou de ser verdade** (exclusão permanente).
- **Nunca use `retracted` para "atualizar"** — atualizar significa inserir um novo TX com o valor correto.

#### Propriedades multi-valoradas — o TX inteiro é a verdade

- Para propriedades com múltiplos valores (ex: lista de obrigações vinculadas), **todo o conjunto do TX mais recente é a fonte da verdade**.
- Exemplo: `TX1 = (A, B, C)` → `TX2 = (A, B)` significa que C foi removido — **sem precisar retrair C**.
- Queries devem filtrar `AND tx = (SELECT MAX(tx) FROM triples WHERE subject = ? AND predicate = ?)` para obter apenas os itens do TX atual, não todos os históricos com `retracted = 0`.
- **Erro comum:** usar só `WHERE retracted = 0` retorna itens de todos os TXs históricos, incluindo removidos.

## Ferramentas MCP

**SEMPRE use ferramentas MCP** para todas as operações de dados do Foundation. Nunca use SQL INSERT/UPDATE/DELETE.  
Se o app não estiver rodando, reporte os achados e aguarde o usuário iniciá-lo.

**NUNCA adivinhe ou deduza IRIs** — sempre consulte:
- Buscar por label: `search(concept_iri: "foundation:Status", filters: [{detail: "rdfs:label", value: "<label>"}])`
- Nunca hardcode um IRI sem confirmar que ele existe via MCP

### Operações de Ontologia

**SEMPRE use ferramentas MCP** para criar ou modificar ontologia. Sempre invoque a skill `/new-ontology`.

- Classes → `define_class`
- Propriedades → `define_property` (use `formula` para campos calculados)
- Indivíduos → `assert_individual`

**Regra de domínio de propriedade:** defina `domain` para a classe que *possui* a propriedade, nunca para a classe range.
- ✅ `foundation:hasStatus` → `domain: owl:Thing` (não `foundation:Status`)
- ✅ `foundation:userRole` → `domain: foundation:UserStory` (não `foundation:Persona`)

**Use referências de ontologia em vez de primitivos:** prefira `owl:ObjectProperty` apontando para uma classe existente em vez de `xsd:string` etc.

| Conceito | Uso |
|---|---|
| Cidade/Município | `foundation:City` |
| Empresa/Organização | `foundation:Company` / `foundation:Organization` |
| Email | `foundation:EmailAddress` |
| Telefone | `foundation:PhoneNumber` |
| Endereço | `foundation:Address` |
| Pessoa | `foundation:Person` |
| Moeda | `currency:BRL`, `currency:USD` (QUDT) |

Use tipos primitivos para: números escalares/booleanos/datas, identificadores/códigos, texto livre, valores monetários.

### Tipos de Datatype para Propriedades

Ao usar `define_property` com `property_type: "datatype"`, escolha o `range` adequado:

| Range | Uso | Editor no Inspetor |
|---|---|---|
| `xsd:string` | Texto livre, identificadores, códigos | Textarea |
| `xsd:integer` | Inteiros — **requer `unit`** (ex: `unit:Count`) | Input numérico (step=1) |
| `xsd:decimal` | Decimais — **requer `unit`** (ex: `unit:Meter`) | Input numérico |
| `xsd:date` | Data (sem hora) | Date picker |
| `xsd:dateTime` | Data e hora | Datetime picker |
| `xsd:boolean` | Verdadeiro/Falso | Sem editor (não suportado) |
| `xsd:anyURI` | URLs e links externos | Texto + botão abrir |
| `foundation:rrule` | Regra de recorrência iCalendar (RFC 5545) | Editor de recorrência inline |

`foundation:rrule` armazena strings no formato `FREQ=WEEKLY;INTERVAL=1;BYDAY=MO`. O Inspector renderiza automaticamente um editor visual de recorrência para qualquer propriedade com este tipo.

### Ícones de Entidades

**Única propriedade:** `foundation:hasIcon` — usada para TODOS os ícones de qualquer entidade.

| Tipo | Formato no banco | Como obter |
|---|---|---|
| Material Symbol | IRI `foundation:icon-material-symbols-name-{name}` | nome do símbolo, ex: `"person"` |
| Arquivo local | Literal `file:///caminho/imagem.png` | caminho completo |
| URL | Literal `https://...` | URL direta |

**Criação** (`assert_individual`): campo `icon` aceita nome do símbolo ou path/URL — a função `icon_store_value` converte corretamente para IRI ou literal.

**Atualização via `replace_property_values`:** funciona para Material Symbols passando o IRI completo (`foundation:icon-material-symbols-name-{name}`). **Não funciona para `file://` ou URLs** — a ferramenta armazena como IRI (objeto), não como literal, e `icon_iri_to_display` não reconhece o padrão. Para alterar ícone de arquivo em entidade existente, use `assert_individual` recriando a entidade ou um comando Tauri interno.

## System Prompt do Agente

O prompt base do agente **não está hardcoded no código** — é carregado em runtime do indivíduo `foundation:DefaultSystemPromptSetting` (propriedade `foundation:settingValue`).

- **Para editar o prompt base:** use `replace_property_values` no indivíduo `foundation:DefaultSystemPromptSetting`, campo `foundation:settingValue`. A mudança tem efeito imediato (sem recompilar).
- **Ponto de carga no código:** `src-tauri/src/commands/chat/settings.rs` → função `load_base_system_prompt(conn)`, chamada por `load_agent_config` e pelos executores de `AgentTask`/`Task`.
- **Fallback:** se o setting não existir, retorna string vazia.
- **Prompt de persona do agente** (ex: personalidade da NOVA) vem separadamente do campo `foundation:basePrompt` no indivíduo do agente — é concatenado após o prompt base.

## Estrutura do Projeto

- **Frontend**: Svelte + TypeScript (`src/`)
- **Backend**: Rust + Tauri (`src-tauri/`)
- **Ontologia**: `core-ontology/ontology.sql` — gerado automaticamente, **nunca edite manualmente**

`ontology.sql` é gerado por `scripts/dump-ontology`, embutido em tempo de compilação via `include_str!()` em `src-tauri/src/eavto/connection.rs`. Para alterar o conteúdo da ontologia: use ferramentas MCP no DB live — o dump captura as mudanças no momento do release.

## Comandos de Desenvolvimento

```bash
npm run tauri dev                               # inicia o servidor de desenvolvimento (o usuário executa)
npm run logs [N]                                # exibe as últimas N linhas de log
cargo check --manifest-path src-tauri/Cargo.toml
cargo build --manifest-path src-tauri/Cargo.toml
```

**Nunca execute `npm run tauri dev`/`npm run build`** — o usuário gerencia isso.  
**Nunca mate processos Tauri** (pkill, killall, etc.).  
Use apenas `cargo check` para validar o código Rust.

## Releases de Versão

Use a skill `/release`.

## Documentação TODO

Nomenclatura de arquivos: `YYYYMMDD-HHMMSS-descricao.md` (ex. `20260228-192519-layer-violations-fix.md`)

## Código & Scripts

- **Scripts devem ser escritos em Rust** — sem Node.js, Python ou shell scripts
- **Comentários de código**: apenas comentários de *por quê* — remova comentários de *o quê*, código comentado e marcadores TODO/FIXME
- **Evite funções redundantes**: se uma função pode ser substituída por chamadas a funções existentes, remova-a
- **NUNCA suprimir avisos ou erros**

## Comunicação

Todas as respostas, documentação e comentários devem estar em **português**.
