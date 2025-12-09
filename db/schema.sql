-- ============================================================================
-- FOUNDATION Database Schema
-- ============================================================================
-- RDF-Native Triple Store with Transaction & Origin Tracking
--
-- Architecture: Stores RDF triples (subject-predicate-object) with FOUNDATION extensions
-- - Subject: IRI or blank node
-- - Predicate: IRI for property
-- - Object: IRI, Literal with datatype, or Blank node
-- - Transaction (T): Monotonically increasing transaction ID (logical timestamp)
-- - Origin (O): Who/what asserted this triple
-- - Retracted: Boolean flag for immutable timeline (never delete, only retract)
--
-- Storage: RDF-native with typed columns for performance
-- - RDF Triple columns: subject, predicate, object (IRI), object_value (literal)
-- - Typed columns: object_number, object_integer, object_datetime for range queries
-- - Full RDF compatibility: Export to Turtle/JSON-LD without transformation
-- ============================================================================

PRAGMA journal_mode = DELETE;
PRAGMA foreign_keys = ON;
PRAGMA synchronous = NORMAL;

-- ============================================================================
-- Transaction Log
-- ============================================================================
-- Metadata about each logical transaction
-- A transaction groups multiple triples asserted atomically

CREATE TABLE IF NOT EXISTS transactions (
  tx INTEGER PRIMARY KEY AUTOINCREMENT,  -- Transaction ID (logical timestamp)
  origin TEXT NOT NULL,                   -- Who initiated this transaction
  created_at INTEGER NOT NULL             -- Physical timestamp (Unix epoch milliseconds)
);

CREATE INDEX IF NOT EXISTS idx_tx_created ON transactions(created_at);
CREATE INDEX IF NOT EXISTS idx_tx_origin ON transactions(origin);

-- ============================================================================
-- Triples Table (Immutable, Append-Only, RDF-Native)
-- ============================================================================
-- Core data structure: every piece of information is an RDF triple
-- Triples are NEVER updated or deleted, only retracted and replaced

CREATE TABLE IF NOT EXISTS triples (
  -- RDF Triple (core)
  subject TEXT NOT NULL,           -- IRI or blank node (e.g., "ex:transaction_tx001", "_:b1")
  predicate TEXT NOT NULL,         -- IRI for property (e.g., "ex:amount", "rdf:type")

  -- Object (one of three forms based on object_type)
  object TEXT,                     -- IRI or blank node (if object_type = 'iri' or 'blank')
  object_value TEXT,               -- Literal lexical form (if object_type = 'literal')
  object_datatype TEXT,            -- Datatype IRI (e.g., "xsd:decimal", "xsd:string")
  object_language TEXT,            -- Language tag (e.g., "en", "pt", NULL if not language-tagged)

  object_type TEXT NOT NULL CHECK(object_type IN ('iri', 'literal', 'blank')),

  -- Performance optimization: typed columns (NULL if not applicable)
  object_number REAL,              -- Populated for xsd:decimal, xsd:double, xsd:float
  object_integer INTEGER,          -- Populated for xsd:integer, xsd:int, xsd:long
  object_datetime INTEGER,         -- Populated for xsd:dateTime (Unix epoch ms)
  object_boolean INTEGER,          -- Populated for xsd:boolean (0 = false, 1 = true)

  -- FOUNDATION extensions: transaction metadata
  tx INTEGER NOT NULL,             -- Transaction ID (references transactions.tx)
  origin_id INTEGER NOT NULL,      -- Origin ID (references origins.id)
  retracted INTEGER NOT NULL DEFAULT 0,  -- 0 = active, 1 = retracted
  created_at INTEGER NOT NULL,     -- Physical timestamp (Unix epoch milliseconds)

  FOREIGN KEY (origin_id) REFERENCES origins(id),

  -- Consistency constraints
  CHECK (
    -- IRI: object must be populated, object_value must be NULL
    (object_type = 'iri' AND object IS NOT NULL AND object_value IS NULL) OR

    -- Literal: object_value and object_datatype must be populated, object must be NULL
    (object_type = 'literal' AND object_value IS NOT NULL AND object_datatype IS NOT NULL AND object IS NULL) OR

    -- Blank node: object must be populated, object_value must be NULL
    (object_type = 'blank' AND object IS NOT NULL AND object_value IS NULL)
  ),

  -- Typed columns consistency
  CHECK (
    -- If datatype is numeric, object_number must be populated
    (object_datatype IN ('xsd:decimal', 'xsd:double', 'xsd:float') AND object_number IS NOT NULL) OR
    (object_datatype IN ('xsd:integer', 'xsd:int', 'xsd:long') AND object_integer IS NOT NULL) OR
    (object_datatype = 'xsd:dateTime' AND object_datetime IS NOT NULL) OR
    (object_datatype = 'xsd:boolean' AND object_boolean IS NOT NULL) OR
    -- Otherwise, typed columns are NULL
    (object_datatype NOT IN ('xsd:decimal', 'xsd:double', 'xsd:float', 'xsd:integer', 'xsd:int', 'xsd:long', 'xsd:dateTime', 'xsd:boolean'))
  )
);

-- ============================================================================
-- SPO Indices (Four Covering Indices for RDF Triple Queries)
-- ============================================================================
-- These indices cover all common RDF access patterns without table lookups

-- Index 1: SPO (Subject-Predicate-Object) - Find all triples about a subject (most common query)
CREATE INDEX IF NOT EXISTS idx_spo ON triples(subject, predicate, object, object_value, tx, origin_id);

-- Index 2: POS (Predicate-Object-Subject) - Find all subjects with a specific predicate-object
CREATE INDEX IF NOT EXISTS idx_pos ON triples(predicate, object, object_value, subject, tx, origin_id);

-- Index 3: OSP (Object-Subject-Predicate) - Find subjects by object (reverse lookup for IRIs)
CREATE INDEX IF NOT EXISTS idx_osp ON triples(object, subject, predicate, tx, origin_id) WHERE object_type = 'iri';

-- Index 4: OPS (Object-Predicate-Subject) - Find all triples referencing an object (backlinks)
CREATE INDEX IF NOT EXISTS idx_ops ON triples(object, predicate, subject, tx, origin_id) WHERE object_type = 'iri';

-- ============================================================================
-- Performance Indices for Typed Columns
-- ============================================================================
-- Additional indices for range queries on numeric/temporal data

-- Numeric range queries (e.g., amount > 100)
CREATE INDEX IF NOT EXISTS idx_predicate_number ON triples(predicate, object_number, tx)
  WHERE object_type = 'literal' AND object_datatype IN ('xsd:decimal', 'xsd:double', 'xsd:float') AND retracted = 0;

-- Integer range queries (e.g., age >= 18)
CREATE INDEX IF NOT EXISTS idx_predicate_integer ON triples(predicate, object_integer, tx)
  WHERE object_type = 'literal' AND object_datatype IN ('xsd:integer', 'xsd:int', 'xsd:long') AND retracted = 0;

-- Temporal range queries (e.g., created_at >= X AND created_at < Y)
CREATE INDEX IF NOT EXISTS idx_predicate_datetime ON triples(predicate, object_datetime, tx)
  WHERE object_type = 'literal' AND object_datatype = 'xsd:dateTime' AND retracted = 0;

-- Retraction queries (find active triples for a subject)
CREATE INDEX IF NOT EXISTS idx_subject_retracted ON triples(subject, retracted, tx);

-- Transaction queries (find all triples in a transaction)
CREATE INDEX IF NOT EXISTS idx_tx ON triples(tx);

-- ============================================================================
-- Namespaces Table
-- ============================================================================
-- Store RDF namespace prefixes for compact IRI representation
-- Instead of storing full IRIs like "http://www.w3.org/2000/01/rdf-schema#label"
-- we store "rdfs:label" and expand at query time

CREATE TABLE IF NOT EXISTS namespaces (
  prefix TEXT PRIMARY KEY,
  iri TEXT NOT NULL UNIQUE
);

-- Initialize common RDF namespaces
INSERT OR IGNORE INTO namespaces (prefix, iri) VALUES
  ('rdf', 'http://www.w3.org/1999/02/22-rdf-syntax-ns#'),
  ('rdfs', 'http://www.w3.org/2000/01/rdf-schema#'),
  ('owl', 'http://www.w3.org/2002/07/owl#'),
  ('xsd', 'http://www.w3.org/2001/XMLSchema#'),
  ('skos', 'http://www.w3.org/2004/02/skos/core#'),
  ('dtype', 'http://www.linkedmodel.org/schema/dtype#'),
  ('foundation', 'http://foundation.local/ontology/');

-- ============================================================================
-- Origins Table
-- ============================================================================
-- Store origin identifiers for triples
-- Instead of repeating origin strings thousands of times,
-- we store an integer ID and join when needed

CREATE TABLE IF NOT EXISTS origins (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE,
  description TEXT
);

-- Initialize common origins
INSERT OR IGNORE INTO origins (id, name, description) VALUES
  (1, 'rdf:core', 'RDF/RDFS/OWL core ontology'),
  (2, 'foundation:CurrentUser', 'Data provided by the current user'),
  (3, 'foundation:FOUNDATION', 'Data collected automatically by FOUNDATION application');

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
  ('schema_version', '3', strftime('%s', 'now') * 1000),
  ('created_at', strftime('%s', 'now') * 1000, strftime('%s', 'now') * 1000),
  ('ontology_imported', 'false', strftime('%s', 'now') * 1000);

-- ============================================================================
-- Ontology Files Table
-- ============================================================================
-- Track imported ontology files for incremental reimport
-- This is separate from the triple store to avoid circular dependencies
-- (we need this table to know which files to import, including DigitalThing.ttl)

CREATE TABLE IF NOT EXISTS ontology_files (
  file_path TEXT PRIMARY KEY,          -- Full path to the file
  file_name TEXT NOT NULL,             -- Just the filename (e.g., "Thing.ttl")
  last_modified INTEGER NOT NULL,      -- Unix timestamp (seconds) when file was last modified on disk
  last_imported INTEGER NOT NULL,      -- Unix timestamp (seconds) when file was last imported into DB
  checksum TEXT NOT NULL,              -- SHA-256 hash of file contents for integrity verification
  triple_count INTEGER NOT NULL        -- Number of triples imported from this file
);

CREATE INDEX IF NOT EXISTS idx_ontology_files_name ON ontology_files(file_name);
CREATE INDEX IF NOT EXISTS idx_ontology_files_modified ON ontology_files(last_modified);

-- ============================================================================
-- Views for Common Queries
-- ============================================================================

-- Current state view: Only non-retracted triples, latest tx per (subject, predicate, origin_id)
CREATE VIEW IF NOT EXISTS triples_current AS
SELECT DISTINCT
  subject, predicate, object, object_value, object_datatype, object_language,
  object_number, object_integer, object_datetime, object_boolean,
  FIRST_VALUE(tx) OVER (PARTITION BY subject, predicate, origin_id ORDER BY tx DESC) as tx,
  origin_id, object_type, created_at
FROM triples
WHERE retracted = 0;

-- Entity view: All subjects with at least one non-retracted triple
CREATE VIEW IF NOT EXISTS entities AS
SELECT DISTINCT subject
FROM triples
WHERE retracted = 0;

-- Ontology classes view: All OWL/RDFS classes defined
CREATE VIEW IF NOT EXISTS ontology_classes AS
SELECT DISTINCT subject as class_id,
  (SELECT object_value FROM triples WHERE subject = class_id AND predicate = 'rdfs:label' AND retracted = 0 LIMIT 1) as label,
  (SELECT object_value FROM triples WHERE subject = class_id AND predicate = 'rdfs:comment' AND retracted = 0 LIMIT 1) as comment,
  (SELECT object FROM triples WHERE subject = class_id AND predicate = 'rdfs:subClassOf' AND retracted = 0 LIMIT 1) as parent_class
FROM triples
WHERE predicate = 'rdf:type'
  AND object IN ('owl:Class', 'rdfs:Class')
  AND retracted = 0;

-- Ontology properties view: All OWL/RDFS properties defined
CREATE VIEW IF NOT EXISTS ontology_properties AS
SELECT DISTINCT subject as property_id,
  (SELECT object FROM triples WHERE subject = property_id AND predicate = 'rdf:type' AND retracted = 0 LIMIT 1) as property_type,
  (SELECT object_value FROM triples WHERE subject = property_id AND predicate = 'rdfs:label' AND retracted = 0 LIMIT 1) as label,
  (SELECT object FROM triples WHERE subject = property_id AND predicate = 'rdfs:domain' AND retracted = 0 LIMIT 1) as domain,
  (SELECT object FROM triples WHERE subject = property_id AND predicate = 'rdfs:range' AND retracted = 0 LIMIT 1) as range
FROM triples
WHERE predicate = 'rdf:type'
  AND object IN ('owl:ObjectProperty', 'owl:DatatypeProperty', 'owl:AnnotationProperty', 'rdf:Property')
  AND retracted = 0;
