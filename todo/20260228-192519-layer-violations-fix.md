# Instruções para Correção de Violações de Camada

## Contexto

O projeto FOUNDATION usa uma arquitetura em camadas estrita conforme descrito no [README.md](../README.md):

```
Commands Layer (Tauri commands)
    ↓ (usa apenas)
OWL Layer (Abstração semântica RDF/OWL)
    ↓ (usa apenas)
EAVTO Layer (Operações SQL diretas)
    ↓
SQLite Database
```

**REGRA FUNDAMENTAL:** Cada camada só pode usar a camada imediatamente abaixo.

## Violações Encontradas

### Tipo 1: Commands → EAVTO (bypass do OWL)

**Arquivos afetados:**
- `src-tauri/src/commands/ai.rs` (linhas 42, 1448, 1605)
- `src-tauri/src/commands/entity.rs` (linhas 367, 417, 485, 704)
- `src-tauri/src/commands/chat.rs` (linhas 1448, 1605)

**Violações:**
- Chamadas diretas a `crate::eavto::store::retract_triples()`
- Chamadas diretas a `crate::eavto::query::get_by_entity_predicate()`
- Chamadas diretas a `crate::eavto::store::assert_triples()`

### Tipo 2: Commands → SQL direto (bypass de OWL e EAVTO)

**Arquivos afetados:**
- `src-tauri/src/commands/entity.rs` (linha 650)
- `src-tauri/src/commands/widget.rs` (múltiplas linhas)

**Violações:**
- Chamadas diretas a `conn.prepare()`
- Chamadas diretas a `conn.execute()`

## Plano de Correção

### Fase 1: Expandir OWL Layer

Crie novas funções no OWL layer para cobrir as operações que Commands está fazendo diretamente:

#### 1.1. Adicionar métodos em `src-tauri/src/owl/thing.rs`

```rust
impl Thing {
    /// Get a specific property value for any entity
    pub fn get_property_value(
        conn: &Connection,
        entity_iri: &str,
        property_iri: &str
    ) -> Result<Option<Object>> {
        let result = query::get_by_entity_predicate(conn, entity_iri, property_iri)?;
        Ok(result.triples.first().map(|t| t.object.clone()))
    }

    /// Get multiple property values for any entity
    pub fn get_property_values(
        conn: &Connection,
        entity_iri: &str,
        property_iri: &str
    ) -> Result<Vec<Object>> {
        let result = query::get_by_entity_predicate(conn, entity_iri, property_iri)?;
        Ok(result.triples.into_iter().map(|t| t.object).collect())
    }
}
```

#### 1.2. Adicionar método de retração em `src-tauri/src/owl/individual.rs`

```rust
impl Individual {
    /// Retract (delete) this individual and all its properties
    pub fn retract(&self, conn: &mut Connection, origin: &str) -> Result<()> {
        // Get all triples for this individual
        let all_triples = query::get_by_entity(conn, &self.iri)?;

        if !all_triples.triples.is_empty() {
            store::retract_triples(conn, &all_triples.triples, origin)?;
        }

        Ok(())
    }

    /// Retract specific triples
    pub fn retract_triples(
        conn: &mut Connection,
        triples: &[Triple],
        origin: &str
    ) -> Result<()> {
        store::retract_triples(conn, triples, origin)
            .map_err(|e| OwlError::DatabaseError(e.to_string()))
    }
}
```

#### 1.3. Adicionar método para assert de triples genéricas

```rust
// Em src-tauri/src/owl/mod.rs ou thing.rs
pub fn assert_triples(
    conn: &mut Connection,
    triples: &[Triple],
    origin: &str
) -> Result<()> {
    store::assert_triples(conn, triples, origin)
        .map_err(|e| OwlError::DatabaseError(e.to_string()))
}
```

### Fase 2: Refatorar Commands Layer

#### 2.1. Refatorar `src-tauri/src/commands/ai.rs`

**Linha 42:**
```rust
// ANTES:
crate::eavto::store::retract_triples(conn, &all_triples.triples, "ai")

// DEPOIS:
Individual::retract_triples(conn, &all_triples.triples, "ai")
```

#### 2.2. Refatorar `src-tauri/src/commands/entity.rs`

**Linhas 367, 485 (buscar qudt:symbol):**
```rust
// ANTES:
let symbol_result = crate::eavto::query::get_by_entity_predicate(conn, unit_iri, "qudt:symbol");
let unit_display = if let Ok(result) = symbol_result {
    result.triples.first()
        .and_then(|t| t.object.as_literal())
        .map(|s| s.to_string())
} else {
    None
};

// DEPOIS:
use crate::owl::Thing;
let unit_display = Thing::get_property_value(conn, unit_iri, "qudt:symbol")
    .ok()
    .flatten()
    .and_then(|obj| obj.as_literal())
    .map(|s| s.to_string());
```

**Linhas 417, 704 (buscar rdf:type):**
```rust
// ANTES:
let source_types_result = crate::eavto::query::get_by_entity_predicate(conn, source_entity, "rdf:type");
let (source_class_iri, source_class_label) = if let Ok(types) = source_types_result {
    if let Some(first_type) = types.triples.first() {
        if let Some(class_iri) = first_type.object.as_iri() {
            let class_thing = crate::owl::Thing::get(conn, class_iri);
            (Some(class_iri.to_string()), Some(class_thing.label))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    }
} else {
    (None, None)
};

// DEPOIS:
let (source_class_iri, source_class_label) = Thing::get_property_value(conn, source_entity, "rdf:type")
    .ok()
    .flatten()
    .and_then(|obj| obj.as_iri().map(|s| s.to_string()))
    .map(|class_iri| {
        let class_thing = Thing::get(conn, &class_iri);
        (Some(class_iri), Some(class_thing.label))
    })
    .unwrap_or((None, None));
```

**Linha 650 (query SQL direta para backlinks):**
```rust
// ANTES:
let backlink_query = "SELECT subject, predicate
                      FROM triples
                      WHERE object = ? AND object_type = 'iri'
                      AND predicate != 'rdf:type'
                      AND retracted = 0";
let mut stmt = conn.prepare(backlink_query).map_err(|e| e.to_string())?;
// ... resto do código

// DEPOIS:
// Esta funcionalidade já existe em Individual::get()
// Use individual.backlinks ao invés de fazer query direta
// Se for necessário filtrar por predicado específico, adicione no OWL layer:

// Em src-tauri/src/owl/individual.rs:
pub fn get_backlinks_by_predicate(
    conn: &Connection,
    entity_iri: &str,
    predicate: Option<&str>
) -> Result<Vec<(String, String)>> {
    let result = query::get_by_object(conn, entity_iri)?;
    let backlinks = result.triples.iter()
        .filter(|t| {
            t.subject != entity_iri
            && t.predicate != "rdf:type"
            && predicate.map_or(true, |p| t.predicate == p)
        })
        .map(|t| (t.subject.clone(), t.predicate.clone()))
        .collect();
    Ok(backlinks)
}
```

#### 2.3. Refatorar `src-tauri/src/commands/chat.rs`

**Linhas 1448, 1605:**
```rust
// ANTES:
crate::eavto::store::assert_triples(conn, &[part_of_msg_triple], "ai")

// DEPOIS:
use crate::owl;
owl::assert_triples(conn, &[part_of_msg_triple], "ai")
```

### Fase 3: Widget Layer (Avaliar)

O arquivo `src-tauri/src/commands/widget.rs` usa SQL direto mas em uma tabela `widgets` separada (não triples).

**Opções:**

1. **Se widgets são dados técnicos da UI (não semânticos):** Deixar como está, mas documentar explicitamente como exceção
2. **Se widgets devem ser modelados semanticamente:** Refatorar para usar RDF/OWL

**Decisão recomendada:** Avaliar se widgets precisam de semântica RDF. Se não, documentar como exceção legítima no código.

### Fase 4: Validação

#### 4.1. Remover imports diretos de EAVTO

Após refatoração, certifique-se de que Commands layer não importa mais:
```rust
// REMOVER de todos os arquivos em src-tauri/src/commands/:
use crate::eavto::query;
use crate::eavto::store;
use crate::eavto::*;
```

#### 4.2. Verificar com grep

```bash
# Não deve retornar nenhum resultado:
grep -r "use crate::eavto::(query|store)" src-tauri/src/commands/
grep -r "crate::eavto::(query|store)::" src-tauri/src/commands/
grep -r "conn\.prepare\|conn\.execute" src-tauri/src/commands/ | grep -v widget.rs
```

#### 4.3. Executar testes

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

## Checklist de Execução

- [ ] Fase 1.1: Adicionar `Thing::get_property_value()` e `Thing::get_property_values()`
- [ ] Fase 1.2: Adicionar `Individual::retract()` e `Individual::retract_triples()`
- [ ] Fase 1.3: Adicionar `owl::assert_triples()`
- [ ] Fase 2.1: Refatorar `ai.rs`
- [ ] Fase 2.2: Refatorar `entity.rs`
- [ ] Fase 2.3: Refatorar `chat.rs`
- [ ] Fase 3: Avaliar e decidir sobre `widget.rs`
- [ ] Fase 4.1: Remover imports de EAVTO
- [ ] Fase 4.2: Validar com grep
- [ ] Fase 4.3: Executar testes
- [ ] Revisar código final para garantir que todas as violações foram corrigidas

## Notas Importantes

1. **NUNCA adicione `use crate::eavto` em arquivos da Commands layer**
2. **SEMPRE use a OWL layer como intermediária**
3. **Se uma operação não existe no OWL layer, adicione-a lá primeiro**
4. **Mantenha a semântica RDF/OWL consistente em todas as operações**
5. **Execute testes após cada mudança significativa**

## Referências

- [README.md - Arquitetura](../README.md#arquitetura)
- [CLAUDE.md - Best Practices](../CLAUDE.md#best-practices)
- Código existente em `src-tauri/src/owl/` para exemplos de implementação
