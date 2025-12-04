## Problem 3: Data Structure for Semantic Architecture

### What

We need a data structure that supports FOUNDATION's core principles while remaining simple and powerful.

### Why This Matters

**For Interoperability:**
- Different FOUNDATION instances must understand each other's data natively
- Users extend base concepts without breaking compatibility
- AI can reason about custom structures through semantic relationships

**For Immutability:**
- Complete audit trail — nothing is lost, ever
- Safe experimentation — fork your data, test changes, merge back
- Time travel — see any point in your data's history

**For Automation:**
- The database itself is the log — every change triggers actions
- Reliable, testable, debuggable — replay history to understand behavior
- Idempotent — same changes always produce same result

**For Transparency:**
- Every fact tracks its origin — user, AI, import, or automation
- Multiple sources can coexist — you choose which to trust
- Full accountability — who changed what, when, and why

**For Navigation:**
- Traverse relationships naturally — follow connections like spreadsheet formulas
- Query across entities without brittle references
- Complex questions answered simply

---

### Solutions

<details open>
<summary><strong>Solution 1: RDF-Native Triple Store with Transaction & Origin Tracking</strong> [✅ CURRENT]</summary>

**Core Architecture:**

Store all data as RDF triples with FOUNDATION extensions for immutability and provenance:

**RDF Triple (Core):**
- **Subject** - IRI or blank node (e.g., `ex:transaction_tx001`)
- **Predicate** - IRI for property (e.g., `ex:amount`, `ex:hasCategory`)
- **Object** - IRI, Literal with datatype, or Blank node
  - IRI: `ex:category_food`
  - Literal: `"45.50"^^xsd:decimal`, `"Grocery Store"^^xsd:string`

**FOUNDATION Extensions:**
- **Transaction (T)** - Monotonically increasing transaction ID (logical timestamp)
- **Origin (O)** - Who/what asserted this triple (e.g., `user:alice`, `import:bank`, `core` for base ontology)
- **Retracted** - Boolean flag for immutable timeline (never delete, only retract)

**Four Indices for Efficient Queries:**

We create four covering indices to optimize different RDF triple query patterns:

1. **SPO-TO** (Subject-Predicate-Object-Transaction-Origin) - Primary index, find all triples about a subject
2. **POS-TO** - Find all subjects with a specific predicate
3. **OSP-TO** - Find subjects by predicate-object pair (reverse lookup)
4. **OPS-TO** - Find all triples referencing an object (backlinks)

**Special Origin: `core`**

The origin `core` is reserved for immutable base facts:
- RDF/RDFS/OWL meta-vocabulary (essential primitives for defining classes and properties)
- FOUNDATION Base Ontology (see [base-ontology-selection.md](base-ontology-selection.md))
- System schema definitions
- Built-in types and properties
- These facts are never retracted and come pre-loaded in versioned database

### SQLite Schema Design

**RDF-Native Triple Store with Performance Optimization**

We store RDF triples natively while maintaining performance for range queries:

**Storage Strategy:**
- **RDF Triple columns** - `subject`, `predicate`, `object` (IRI or blank node)
- **Literal columns** - `object_value` (lexical form), `object_datatype` (IRI), `object_language` (tag)
- **Object type discriminator** - `object_type` ('iri', 'literal', 'blank')
- **Typed columns** - Optional performance optimization for numeric/temporal range queries

**RDF Compatibility:**
- ✅ Direct mapping to RDF triple structure (subject-predicate-object)
- ✅ Explicit datatype IRIs (xsd:decimal, xsd:string, xsd:dateTime)
- ✅ Support for language-tagged literals (@en, @pt)
- ✅ Blank nodes for anonymous resources
- ✅ Export to Turtle/JSON-LD without transformation
</details>

---

### RDF Meta-Vocabulary Import

**Why import RDF/RDFS/OWL into the database?**

Without these definitions in the database:
- ❌ User creates `owl:ObjectProperty` → system doesn't know what that is
- ❌ UI can't show available property types
- ❌ No validation that `rdfs:domain` requires a class as value
- ❌ Inference engine can't understand property characteristics

With these definitions:
- ✅ User creates property → system validates it's a known type
- ✅ UI shows dropdown: ObjectProperty, DatatypeProperty, etc.
- ✅ System knows `owl:TransitiveProperty` inherits from `owl:ObjectProperty`
- ✅ Inference: if `rdfs:domain` points to `Transaction`, validate instances

**Example triples from meta-ontology import:**

```sql
-- Define owl:ObjectProperty class (RDF triples)
INSERT INTO triples (subject, predicate, object, object_value, object_datatype, object_language, object_type, object_number, object_integer, object_datetime, object_boolean, tx, origin, retracted, created_at) VALUES
  ('owl:ObjectProperty', 'rdf:type', 'rdfs:Class', NULL, NULL, NULL, 'iri', NULL, NULL, NULL, NULL, 1, 'core', 0, 1732547890000),
  ('owl:ObjectProperty', 'rdfs:subClassOf', 'rdf:Property', NULL, NULL, NULL, 'iri', NULL, NULL, NULL, NULL, 1, 'core', 0, 1732547890000),
  ('owl:ObjectProperty', 'rdfs:label', NULL, 'ObjectProperty', 'xsd:string', NULL, 'literal', NULL, NULL, NULL, NULL, 1, 'core', 0, 1732547890000),
  ('owl:ObjectProperty', 'rdfs:comment', NULL, 'The class of object properties.', 'xsd:string', NULL, 'literal', NULL, NULL, NULL, NULL, 1, 'core', 0, 1732547890000);

-- Define rdfs:domain property (RDF triples)
INSERT INTO triples (subject, predicate, object, object_value, object_datatype, object_language, object_type, object_number, object_integer, object_datetime, object_boolean, tx, origin, retracted, created_at) VALUES
  ('rdfs:domain', 'rdf:type', 'rdf:Property', NULL, NULL, NULL, 'iri', NULL, NULL, NULL, NULL, 2, 'core', 0, 1732547890000),
  ('rdfs:domain', 'rdfs:label', NULL, 'domain', 'xsd:string', NULL, 'literal', NULL, NULL, NULL, NULL, 2, 'core', 0, 1732547890000),
  ('rdfs:domain', 'rdfs:domain', 'rdf:Property', NULL, NULL, NULL, 'iri', NULL, NULL, NULL, NULL, 2, 'core', 0, 1732547890000),
  ('rdfs:domain', 'rdfs:range', 'rdfs:Class', NULL, NULL, NULL, 'iri', NULL, NULL, NULL, NULL, 2, 'core', 0, 1732547890000);
```

**User creating a new property (uses imported vocabulary):**

```sql
-- User defines "ex:hasCategory" property using owl:ObjectProperty
INSERT INTO transactions (origin, created_at) VALUES ('user:alice', 1732547890000);
-- Returns tx: 10000

INSERT INTO triples (subject, predicate, object, object_value, object_datatype, object_language, object_type, object_number, object_integer, object_datetime, object_boolean, tx, origin, retracted, created_at) VALUES
  ('ex:hasCategory', 'rdf:type', 'owl:ObjectProperty', NULL, NULL, NULL, 'iri', NULL, NULL, NULL, NULL, 10000, 'user:alice', 0, 1732547890000),
  ('ex:hasCategory', 'rdfs:label', NULL, 'has category', 'xsd:string', NULL, 'literal', NULL, NULL, NULL, NULL, 10000, 'user:alice', 0, 1732547890000),
  ('ex:hasCategory', 'rdfs:domain', 'ex:Transaction', NULL, NULL, NULL, 'iri', NULL, NULL, NULL, NULL, 10000, 'user:alice', 0, 1732547890000),
  ('ex:hasCategory', 'rdfs:range', 'ex:Category', NULL, NULL, NULL, 'iri', NULL, NULL, NULL, NULL, 10000, 'user:alice', 0, 1732547890000);

-- System can now validate:
-- ✅ owl:ObjectProperty exists (from tx: 1)
-- ✅ rdfs:domain and rdfs:range are valid properties
-- ✅ ex:Transaction and ex:Category must be classes
```

---

### Success Criteria

**How we'll know the data structure problem is solved:**

1. ✅ **Immutability works** - Can track complete history of all changes
2. ✅ **Origin tracking works** - Every triple knows its source
3. ✅ **Efficient queries** - Four indices enable fast lookups for all query patterns
4. ✅ **RDF compatible** - Can export/import standard RDF formats
5. ✅ **Schema validation** - System validates user data against RDF/RDFS/OWL vocabulary
6. [ ] **Tested with real data** - Q1 KR1 financial tracking scenario works end-to-end

**Note:** For base ontology design (Entity, Agent, Event concepts), see [base-ontology-selection.md](base-ontology-selection.md)

