## Problem 3: Don't Know How to Structure Data for Semantic Architecture

### What
We don't know how to structure data to support FOUNDATION's architecture principles:

**Interoperability (Principle 3)**
- All instances share a common base ontology (FOUNDATION Base)
- Users extend base classes for specific needs (e.g., `RecurringTransaction extends Transaction`)
- AI must understand custom classes through their relationship to base classes
- Why: AI can interpret any instance's worldview by reasoning over the class hierarchy, enabling seamless integration and data exchange

**Class Extension & Inference (Principle 3)**
- Users extend base classes (Employee extends Person)
- System must infer inherited properties automatically
- Why: "SoftwareEngineer extends Person" should inherit all Person properties without configuration

**Immutable Timeline (Principle 5)**
- Never update records - only insert new facts that replace old ones
- Why: Complete history preserved for undo, audit, compliance

**Automation Reliability (Principle 4)**
- System must reliably detect and replay every data change
- Replaying the same sequence of changes always produces the same final state (idempotent)
- Why: "Database itself is the log" - automations can be safely retried, tested with production data, and debugged by replaying history

**Origin Tracking (Principle 6)**
- Every fact must track who/what asserted it (user, AI, import, automation)
- Why: Transparency and accountability for all data changes

**Multi-Source Truth**
- Different sources can assert conflicting facts simultaneously
- Why: User edits while AI suggests updates and imports run

**Relationship Navigation**
- Efficiently traverse entity relationships (Person → worksAt → Organization → locatedIn → City)
- Why: Query across connected entities like spreadsheet formulas but without breaking

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
- FOUNDATION Base Ontology (comprehensive foundation covering most nouns and verbs from English dictionary)
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

### Ontology Import Strategy

**Solutions Attempted:**

<details>
<summary><strong>Solution 1: Import Common Core Ontologies (CCO)</strong> [❌ REJECTED]</summary>

**Implementation:**
- Imported 258+ classes from CCO
- Attempted to use military/government ontology for personal finance

**Problems Found:**
- Military/government focus doesn't match personal finance use case
- Cognitive overhead for users trying to understand the model
- Most classes unused (e.g., Weapon, MilitaryOrganization, GeospatialRegion)
- Graph visualization cluttered with irrelevant concepts

**Decision:** Rejected - removed CCO entirely from FOUNDATION base ontology (2024-11-25)

</details>

<details>
<summary><strong>Solution 2: Import Basic Formal Ontology (BFO)</strong> [❌ REJECTED]</summary>

**Implementation:**
- Imported BFO as upper ontology foundation
- Used abstract concepts like Continuant, Occurrent, Entity

**Problems Found:**
- Upper ontology designed for philosophers and ontologists
- Concepts like "Continuant" and "Occurrent" confusing for regular users
- Adds complexity without practical value for FOUNDATION use cases
- Users don't need to understand formal ontology theory

**Decision:** Rejected - removed BFO from FOUNDATION base ontology (2024-11-25)

</details>

<details>
<summary><strong>Solution 3: Import Schema.org vocabulary</strong> [❌ REJECTED]</summary>

**Implementation:**
- Imported Schema.org classes and properties
- Attempted to use web-focused vocabulary for app data

**Problems Found:**
- Web-focused vocabulary (SEO, markup)
- Many classes irrelevant (Recipe, Movie, MedicalCondition)
- Requires extensive pruning and adaptation
- Doesn't align with FOUNDATION's personal data use cases

**Decision:** Rejected - removed Schema.org from FOUNDATION base ontology (2024-11-25)

</details>

<details open>
<summary><strong>Solution 4: Build comprehensive ontology from WordNet semantic graph</strong> [✅ IN PROGRESS]</summary>

**Implementation:**
- Keep RDF/RDFS/OWL meta-vocabulary (essential primitives)
- Convert WordNet synsets to OWL classes (120,630 concepts)
- Map WordNet semantic relations to RDF properties:
  - `wn:hypernym/hyponym` → `rdfs:subClassOf` (class hierarchy)
  - `wn:mero_part/holo_part` → object properties (part-of relationships)
  - `wn:causes/entails` → object properties (causation/implication)
  - `wn:definition` → `rdfs:comment` (documentation)
  - `wn:example` → `skos:example` (usage examples)
- Let users extend with domain-specific classes through UI

**Data Source: Open English WordNet 2024**
- **License**: CC-BY 4.0 (permissive, commercial use OK)
- **Coverage**: 161,705 words, 120,630 synsets, 419,168 semantic relations
- **Structure**: Lexical concepts organized in semantic hierarchy
- **Download**: `npm run update:wordnet` ([scripts/download-wordnet.sh](../../scripts/download-wordnet.sh))
- **Format**: RDF/Turtle with ontolex vocabulary

**Rationale:**
- RDF/RDFS/OWL cannot be removed - they define how to define classes and properties
- WordNet provides comprehensive English vocabulary coverage (most nouns/verbs)
- Semantic hierarchy (hypernym/hyponym) maps naturally to `rdfs:subClassOf`
- Human-readable definitions enable AI to understand domain-specific extensions
- Natural language foundation applicable to any use case
- Community-maintained, updated annually

**Conversion Strategy:**

```turtle
# WordNet Synset (before conversion)
wnid:oewn-00007846-n
    a ontolex:LexicalConcept ;
    wn:definition "a human being; person, singular..."@en ;
    wn:hypernym wnid:oewn-00004475-n ;  # organism
    wn:hyponym wnid:oewn-09628155-n ;    # adult
    wn:mero_part wnid:oewn-04624919-n ;  # body

# FOUNDATION Class (after conversion)
sn:Person
    a owl:Class ;
    rdfs:subClassOf sn:Organism ;
    rdfs:comment "a human being; person, singular..."@en ;
    skos:example "there was too much for one person to do"@en ;
    sn:derivedFrom wnid:oewn-00007846-n .

sn:Adult
    a owl:Class ;
    rdfs:subClassOf sn:Person .

sn:hasPart
    a owl:ObjectProperty ;
    rdfs:domain sn:Person ;
    rdfs:range sn:Body .
```

**Benefits:**
- ✅ Comprehensive coverage - 120k+ concepts from natural language
- ✅ Semantic hierarchy - Ready-made class taxonomy via hypernym/hyponym
- ✅ AI-friendly - Definitions enable AI to reason about extensions
- ✅ Plain English - No academic jargon (uses everyday words)
- ✅ Maintained - Annual updates from Open English WordNet
- ✅ Permissive license - CC-BY 4.0 allows commercial use
- ✅ Interoperable - RDF format enables standard export/import

**Challenges:**
- Need to convert ontolex:LexicalConcept → owl:Class
- Filter/curate most relevant concepts (may not need all 120k)
- Map WordNet relations to appropriate OWL/RDFS properties
- Handle polysemy (one word, multiple meanings/synsets)

**Build Process:**

```bash
# Development: Build ontology from WordNet
npm run build:ontology

# This script:
# 1. Downloads WordNet 2024 if not present (.gitignore'd)
# 2. Converts synsets → owl:Class → RDF triples
# 3. Imports triples into SQLite (tx: 1-500, origin: "core")
# 4. Creates FOUNDATION.db ready for version control
```

**Repository Structure:**

```
FOUNDATION.db                           # ✅ Single database (versioned)
                                       #    - Base ontology (tx: 1-500, origin: "core")
                                       #    - User data (tx: 501+, origin: "user:*")

core-ontology/
├── rdf-rdfs-owl-core.ttl          # ✅ Versioned (meta-vocabulary source)
└── english-wordnet-2024.ttl       # ❌ Not versioned (.gitignore'd)

scripts/
├── download-wordnet.sh            # Downloads WordNet .ttl
└── build-ontology-simple.js       # Converts WordNet → SQLite
```

**Important:** `FOUNDATION.db` at project root is the **only database** used by FOUNDATION. No other databases should be created or used.

**Why SQLite Instead of .ttl in Git:**
- ✅ **Small** - Compressed binary vs 202MB text
- ✅ **Ready to use** - No parsing/conversion on startup
- ✅ **Final format** - Triples already indexed and optimized
- ✅ **Immutable** - Base ontology (origin: "core") never changes
- ✅ **Reproducible** - Script rebuilds from WordNet when needed

**Status:** ✅ In Progress
- ✅ RDF/RDFS/OWL core imported and working
- ✅ WordNet 2024 download script ready
- 🔜 Create conversion script (synset → owl:Class → SQLite)
- 🔜 Test with person/transaction concepts
- 🔜 Commit FOUNDATION.db to git

</details>

**New Strategy: Build from RDF/RDFS/OWL Foundation**

```
Layer 1: RDF/RDFS/OWL Meta-Ontology (KEEP)
  ├─ core-ontology/rdf-rdfs-owl-core.ttl
  │  Defines: rdf:type, rdfs:Class, owl:ObjectProperty, xsd:string, etc.
  │  Purpose: Meta-vocabulary for defining classes and properties
  │  Status: ✅ Essential foundation - cannot be removed
  │
Layer 2: FOUNDATION Base Ontology (NEW - TO BE CREATED)
  ├─ core-ontology/FOUNDATION-base.ttl
  │  Defines: Comprehensive vocabulary covering most nouns and verbs from English dictionary
  │  Examples: Person, Organization, Transaction, Event, Location, Document, etc.
  │  Purpose: Natural language foundation enabling AI to understand any domain
  │  Principles:
  │    - Comprehensive coverage (thousands of base classes and properties)
  │    - Clear, everyday language (no academic jargon)
  │    - Domain-agnostic but practical
  │    - Extensible by users
  │
Layer 3: FOUNDATION Domain Ontologies (USER-DEFINED)
  ├─ Defined by users through UI
  │  Examples: Transaction, Account, Category, Contact
  │  Purpose: Application-specific concepts
  │  Extends: FOUNDATION Base classes
```

**Import Order:**

1. **RDF/RDFS/OWL** (tx: 1-200) - Meta-vocabulary primitives
2. **FOUNDATION Base** (tx: 201-500) - Lightweight foundation ontology
3. **User Domains** (tx: 501+) - Application and user-specific classes

**Why import RDF/RDFS/OWL?**

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

### FOUNDATION Base Ontology Design Principles

**Goal**: Design a comprehensive, natural language foundation ontology covering most English nouns and verbs

**Principles**:
1. **Plain language** - No academic jargon (avoid: Continuant, Occurrent, Endurant)
2. **Comprehensive coverage** - Thousands of classes and properties from everyday English
3. **AI-friendly** - Enable AI to understand domain-specific extensions through natural language hierarchy
4. **Domain agnostic** - Applicable to personal finance, health tracking, project management, etc.
5. **Extensible** - Easy for users to subclass and adapt

**Current state** (PR #7):
- ✅ RDF/RDFS/OWL core imported and working
- ✅ Graph visualization working with force-directed layout
- ⏸️ Schema.org/FOAF/BFO still imported (temporary, will be replaced)
- 🔜 FOUNDATION Base to be created through analysis and iteration

