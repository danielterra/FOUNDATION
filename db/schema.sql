-- ============================================================================
-- SuperNOVA Database Schema
-- ============================================================================
-- EAVTO (Entity-Attribute-Value-Time-Origin) immutable fact store
--
-- Architecture: Inspired by event sourcing and append-only databases
-- - E: Entity ID (subject)
-- - A: Attribute ID (predicate)
-- - V: Value as text (object, primary source of truth)
-- - T: Transaction ID (logical timestamp, monotonically increasing)
-- - O: Origin (who asserted this fact)
--
-- Storage: Hybrid approach with typed columns for performance
-- - v: Always populated (text representation)
-- - v_number, v_integer, v_datetime: Optional typed columns for range queries
-- ============================================================================

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
PRAGMA synchronous = NORMAL;

-- ============================================================================
-- Transaction Log
-- ============================================================================
-- Metadata about each logical transaction
-- A transaction groups multiple facts asserted atomically

CREATE TABLE IF NOT EXISTS transactions (
  tx INTEGER PRIMARY KEY AUTOINCREMENT,  -- Transaction ID (logical timestamp)
  origin TEXT NOT NULL,                   -- Who initiated this transaction
  created_at INTEGER NOT NULL             -- Physical timestamp (Unix epoch milliseconds)
);

CREATE INDEX IF NOT EXISTS idx_tx_created ON transactions(created_at);
CREATE INDEX IF NOT EXISTS idx_tx_origin ON transactions(origin);

-- ============================================================================
-- Facts Table (Immutable, Append-Only)
-- ============================================================================
-- Core data structure: every piece of information is a fact (datom)
-- Facts are NEVER updated or deleted, only retracted and replaced

CREATE TABLE IF NOT EXISTS facts (
  -- EAV Core (always populated)
  e TEXT NOT NULL,          -- Entity ID (e.g., "transaction:tx001", "CCO:Person")
  a TEXT NOT NULL,          -- Attribute ID (e.g., "amount", "rdfs:subClassOf")
  v TEXT NOT NULL,          -- Value as text (primary source of truth)

  -- Typed columns for performance (NULL if not applicable)
  v_number REAL,            -- Populated when v_type = 'number' (for range queries)
  v_integer INTEGER,        -- Populated when v_type = 'integer' (for range queries)
  v_datetime INTEGER,       -- Populated when v_type = 'datetime' (Unix epoch ms)

  -- Transaction metadata
  tx INTEGER NOT NULL,      -- Transaction ID (references transactions.tx)
  origin TEXT NOT NULL,     -- Who asserted this fact (can differ from transaction origin)
  retracted INTEGER NOT NULL DEFAULT 0,  -- 0 = active, 1 = retracted

  -- Value type and physical timestamp
  v_type TEXT NOT NULL CHECK(v_type IN ('string', 'number', 'integer', 'boolean', 'ref', 'datetime')),
  created_at INTEGER NOT NULL,  -- Physical timestamp (Unix epoch milliseconds)

  -- Consistency constraints
  CHECK (
    (v_type = 'string') OR
    (v_type = 'number' AND v_number IS NOT NULL) OR
    (v_type = 'integer' AND v_integer IS NOT NULL) OR
    (v_type = 'boolean' AND v IN ('true', 'false', '0', '1')) OR
    (v_type = 'ref') OR
    (v_type = 'datetime' AND v_datetime IS NOT NULL)
  )
);

-- ============================================================================
-- EAVTO Indices (Four Covering Indices)
-- ============================================================================
-- These indices cover all common access patterns without table lookups

-- Index 1: EAVTO - Find all facts about an entity (most common query)
CREATE INDEX IF NOT EXISTS idx_eavto ON facts(e, a, v, tx, origin);

-- Index 2: AEVTO - Find all entities with a specific attribute
CREATE INDEX IF NOT EXISTS idx_aevto ON facts(a, e, v, tx, origin);

-- Index 3: AVETO - Find entities by attribute-value pair (reverse lookup)
CREATE INDEX IF NOT EXISTS idx_aveto ON facts(a, v, e, tx, origin);

-- Index 4: VAETO - Find all references to an entity (backlinks)
CREATE INDEX IF NOT EXISTS idx_vaeto ON facts(v, a, e, tx, origin) WHERE v_type = 'ref';

-- ============================================================================
-- Performance Indices for Typed Columns
-- ============================================================================
-- Additional indices for range queries on numeric/temporal data

-- Numeric range queries (e.g., amount > 100)
CREATE INDEX IF NOT EXISTS idx_a_number ON facts(a, v_number, tx)
  WHERE v_type = 'number' AND retracted = 0;

-- Integer range queries (e.g., age >= 18)
CREATE INDEX IF NOT EXISTS idx_a_integer ON facts(a, v_integer, tx)
  WHERE v_type = 'integer' AND retracted = 0;

-- Temporal range queries (e.g., created_at >= X AND created_at < Y)
CREATE INDEX IF NOT EXISTS idx_a_datetime ON facts(a, v_datetime, tx)
  WHERE v_type = 'datetime' AND retracted = 0;

-- Retraction queries (find active facts for an entity)
CREATE INDEX IF NOT EXISTS idx_e_retracted ON facts(e, retracted, tx);

-- Transaction queries (find all facts in a transaction)
CREATE INDEX IF NOT EXISTS idx_tx ON facts(tx);

-- ============================================================================
-- Metadata Table
-- ============================================================================
-- Store database metadata (version, import status, etc.)

CREATE TABLE IF NOT EXISTS metadata (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);

-- Initialize metadata
INSERT OR IGNORE INTO metadata (key, value, updated_at) VALUES
  ('schema_version', '1', strftime('%s', 'now') * 1000),
  ('created_at', strftime('%s', 'now') * 1000, strftime('%s', 'now') * 1000),
  ('ontology_imported', 'false', strftime('%s', 'now') * 1000);

-- ============================================================================
-- Views for Common Queries
-- ============================================================================

-- Current state view: Only non-retracted facts, latest tx per (e, a, origin)
CREATE VIEW IF NOT EXISTS facts_current AS
SELECT DISTINCT
  e, a, v, v_number, v_integer, v_datetime,
  FIRST_VALUE(tx) OVER (PARTITION BY e, a, origin ORDER BY tx DESC) as tx,
  origin, v_type, created_at
FROM facts
WHERE retracted = 0;

-- Entity view: All current facts grouped by entity
CREATE VIEW IF NOT EXISTS entities AS
SELECT DISTINCT e
FROM facts
WHERE retracted = 0;

-- Ontology classes view: All OWL/RDFS classes defined
CREATE VIEW IF NOT EXISTS ontology_classes AS
SELECT DISTINCT e as class_id,
  (SELECT v FROM facts WHERE e = class_id AND a = 'rdfs:label' AND retracted = 0 LIMIT 1) as label,
  (SELECT v FROM facts WHERE e = class_id AND a = 'rdfs:comment' AND retracted = 0 LIMIT 1) as comment,
  (SELECT v FROM facts WHERE e = class_id AND a = 'rdfs:subClassOf' AND retracted = 0 LIMIT 1) as parent_class
FROM facts
WHERE a = 'rdf:type'
  AND v IN ('owl:Class', 'rdfs:Class')
  AND retracted = 0;

-- Ontology properties view: All OWL/RDFS properties defined
CREATE VIEW IF NOT EXISTS ontology_properties AS
SELECT DISTINCT e as property_id,
  (SELECT v FROM facts WHERE e = property_id AND a = 'rdf:type' AND retracted = 0 LIMIT 1) as property_type,
  (SELECT v FROM facts WHERE e = property_id AND a = 'rdfs:label' AND retracted = 0 LIMIT 1) as label,
  (SELECT v FROM facts WHERE e = property_id AND a = 'rdfs:domain' AND retracted = 0 LIMIT 1) as domain,
  (SELECT v FROM facts WHERE e = property_id AND a = 'rdfs:range' AND retracted = 0 LIMIT 1) as range
FROM facts
WHERE a = 'rdf:type'
  AND v IN ('owl:ObjectProperty', 'owl:DatatypeProperty', 'owl:AnnotationProperty', 'rdf:Property')
  AND retracted = 0;
