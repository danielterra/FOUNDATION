# Migration Plan: rusqlite → turso

**Date:** 2026-03-15
**Scope:** Replace `rusqlite` with the `turso` crate (v0.5.1) across the entire Rust backend
**Risk level:** High — turso is beta; touches the core database layer used by every subsystem

---

## Motivation

| Reason | Detail |
|---|---|
| Rust-native rewrite | turso is a complete SQLite rewrite in Rust (not a C fork) |
| Native async | No `spawn_blocking` workarounds needed |
| MVCC concurrency | Concurrent transactions planned |
| Vector search | Native built-in (future use for semantic search) |
| File format | Same `.db` format — **no data migration needed** |

---

## What Does NOT Change

- The SQLite file at `~/Documents/Foundation/FOUNDATION.db`
- All SQL queries, views, indexes, and schema
- The RDF triple store semantics
- Business logic above the database layer

---

## Dependency Change

```toml
# BEFORE
rusqlite = { version = "0.32", features = ["bundled"] }

# AFTER (already applied)
turso = "0.5.1"
```

---

## turso API Reference

| Concept | rusqlite | turso |
|---|---|---|
| Open local DB | `Connection::open(path)?` | `Builder::new_local(path_str).build().await?.connect()?` |
| Open in-memory | `Connection::open_in_memory()?` | `Builder::new_local(":memory:").build().await?.connect()?` |
| Execute SQL | `conn.execute(sql, params)?` | `conn.execute(sql, params).await?` |
| Execute batch | `conn.execute_batch(sql)?` | no equivalent — split on `;` and execute each |
| Prepare statement | `conn.prepare(sql)?` | `conn.prepare(sql).await?` |
| Query row (conn) | `conn.query_row(sql, params, \|row\| ...)` | `conn.prepare(sql).await?.query_row(params).await?` |
| Query rows | `stmt.query_map(params, \|row\| ...)?` | `stmt.query(params).await?` then `rows.next().await?` |
| Typed row access | `row.get::<_, T>(idx)?` | `row.get_value(idx)?` → match `Value` enum |
| `Value` variants | n/a | `Null`, `Integer(i64)`, `Real(f64)`, `Text(String)`, `Blob(Vec<u8>)` |
| Value accessors | n/a | `.as_integer()`, `.as_real()`, `.as_text()`, `.as_blob()` |
| Named params | `named_params! { ":k": v }` | `named_params! { ":k": v }` *(same)* |
| Positional params | `params![v1, v2]` | `params![v1, v2]` *(same macro name)* |
| Dynamic params | `params_from_iter(iter)` | `params_from_iter(iter)?` *(returns Result now)* |
| last_insert_rowid | `conn.last_insert_rowid()` | `SELECT last_insert_rowid()` query |
| Transactions | `conn.transaction()?` | raw SQL `BEGIN` / `COMMIT` / `ROLLBACK` |
| Savepoints | `conn.savepoint()?` | raw SQL `SAVEPOINT sp` / `RELEASE sp` / `ROLLBACK TO sp` |
| Error type | `rusqlite::Error` | `turso::Error` |

### Row Value Extraction Patterns

```rust
// String (non-null)
row.get_value(0)?.as_text().cloned().unwrap_or_default()

// Option<String>
match row.get_value(0)? {
    turso::Value::Null => None,
    v => v.as_text().cloned(),
}

// i64
row.get_value(0)?.as_integer().copied().unwrap_or(0)

// Option<i64>
row.get_value(0)?.as_integer().copied()

// Option<f64>
row.get_value(0)?.as_real().copied()

// bool from 0/1 integer
row.get_value(0)?.as_integer().copied().map(|i| i != 0).unwrap_or(false)
```

---

## Architecture Change: `DbExecutor`

### Before (rusqlite — sync with thread workarounds)
```
write() → mpsc channel → writer thread → rusqlite::Connection (sync)
read()  → spawn_blocking → Connection::open() per read (sync)
```

### After (turso — fully async)
```
write() → Mutex lock → db.connect() per write → Connection.await
read()  → db.connect() per read → Connection.await
```

`DbExecutor` holds `Arc<Database>` + `Arc<Mutex<()>>` (serialize writes). No dedicated thread needed.

---

## Batch Transaction Mechanism Change

The `thread_local! { IN_BATCH_TX }` pattern is unsafe in async code (coroutines move between threads at `.await` points).

**Replacement:** add `in_batch: bool` parameter to `assert_triples`, `append_triples`, `retract_triples`, `rename_iri`.

---

## execute_batch Replacement

```rust
async fn execute_batch(conn: &Connection, sql: &str) -> turso::Result<()> {
    for stmt in sql.split(';') {
        let stmt = stmt.trim();
        if !stmt.is_empty() {
            conn.execute(stmt, ()).await?;
        }
    }
    Ok(())
}
```

Assumes no semicolons in SQL string literals (safe for our controlled schema/ontology SQL).

---

## initialize_with_progress Return Type Change

Before: `Result<(Connection, PathBuf), DbError>`
After: `Result<(Database, PathBuf), DbError>`

`DbExecutor` needs the `Database` (not just one Connection) to open new connections per operation.

---

## Files to Change

### Phase 1 — Core Database Layer

| File | Status | Notes |
|---|---|---|
| `src-tauri/Cargo.toml` | ✅ Done | turso added, rusqlite removed |
| `src/eavto/connection.rs` | ⬜ | Rewrite async, replace execute_batch, fix row access |
| `src/eavto/executor.rs` | ⬜ | Remove writer thread, hold Arc<Database>, async closures |
| `src/eavto/store.rs` | ⬜ | Async, raw SQL transactions, IN_BATCH_TX → in_batch param |

### Phase 2 — Query Layer

| File | Status |
|---|---|
| `src/eavto/query/mod.rs` | ⬜ |
| `src/eavto/query/find.rs` | ⬜ |
| `src/eavto/query/search.rs` | ⬜ |
| `src/eavto/stats.rs` | ⬜ |

### Phase 3 — Business Logic (add `.await` throughout)

| File | Status |
|---|---|
| `src/ai/functions/mod.rs` | ⬜ |
| `src/ai/functions/concept.rs` | ⬜ |
| `src/ai/functions/class.rs` | ⬜ |
| `src/ai/functions/concept_graph.rs` | ⬜ |
| `src/ai/functions/meta_process.rs` | ⬜ |
| `src/ai/functions/batch.rs` | ⬜ |
| `src/owl/formula.rs` | ⬜ |
| `src/owl/formula_worker.rs` | ⬜ |
| `src/owl/individual/write.rs` | ⬜ |
| `src/process_automation/executor.rs` | ⬜ |
| `src/process_automation/scheduler.rs` | ⬜ |
| `src/process_automation/trigger.rs` | ⬜ |
| `src/commands/entity.rs` | ⬜ |
| `src/commands/setup.rs` | ⬜ |
| `src/commands/connector_package.rs` | ⬜ |
| `src/commands/chat/message_utils.rs` | ⬜ |
| `src/commands/chat/settings.rs` | ⬜ |
| `src/commands/logging.rs` | ⬜ |
| `src/commands/shortcuts.rs` | ⬜ |
| `src/commands/setup_system_info.rs` | ⬜ |

### Phase 4 — Tests

| File | Status |
|---|---|
| `src/eavto/test_helpers.rs` | ⬜ |
| `src/eavto/store_tests.rs` | ⬜ |
| `src/ai/functions/thing_tests.rs` | ⬜ |
| `src/process_automation/trigger_tests.rs` | ⬜ |
| `src/process_automation/executor_tests.rs` | ⬜ |

---

## Execution Order

```
Step 1  ✅ Cargo.toml — swap dependency
Step 2  connection.rs — rewrite async (Database return type, execute_batch helper)
Step 3  executor.rs — rewrite (Arc<Database>, async closures, Mutex for writes)
Step 4  store.rs — async, raw SQL transactions, remove IN_BATCH_TX thread-local
Step 5  cargo check — fix remaining errors in query/ and stats.rs
Step 6  query/mod.rs, query/find.rs, query/search.rs, stats.rs
Step 7  cargo check — fix remaining errors in ai/, owl/, process_automation/, commands/
Step 8  All Phase 3 files
Step 9  test_helpers.rs and all test files
Step 10 cargo check — full clean build
Step 11 cargo test
Step 12 Manual smoke test
```

---

## Risks

| Risk | Mitigation |
|---|---|
| turso is beta — API may change | Pin to v0.5.1 |
| `Transaction` not implemented in turso | Use raw SQL BEGIN/COMMIT/ROLLBACK |
| execute_batch not available | Split on `;` helper (safe for controlled SQL) |
| thread_local IN_BATCH_TX unsafe in async | Replace with `in_batch: bool` param |
| last_insert_rowid not on Connection | Use `SELECT last_insert_rowid()` after INSERT |
| In-memory test isolation | Same behavior as before (writes only) |

---

## Definition of Done

- [ ] `cargo check` passes with zero errors
- [ ] `cargo test` passes with zero failures
- [ ] App starts and loads existing `FOUNDATION.db` without errors
- [ ] Can create a new thing via MCP tool and read it back
- [ ] Can run a process end-to-end
- [ ] `rusqlite` no longer appears anywhere in the codebase
