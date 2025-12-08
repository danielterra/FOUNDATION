## Problem 4: Base Ontology Selection

### What

FOUNDATION needs a foundational ontology that provides essential concepts without coupling to application-specific data structures.

### Why This Matters

**For Semantic Foundation:**
- Provides common vocabulary for reasoning and inference
- Enables concept discovery ("what is an agent?", "what types of processes exist?")
- Establishes clear class hierarchy for user extensions

**For Interoperability:**
- Shared understanding across FOUNDATION instances
- AI can reason about relationships between concepts
- Users extend base concepts maintaining compatibility

**For Maintainability:**
- Clear separation between foundation and application concerns
- Can evolve base ontology without breaking user data
- Documented examples show proper usage patterns

**For Simplicity:**
- Only includes concepts actually needed
- Grows incrementally based on real requirements
- Avoids over-engineering with massive vocabularies

---

### Solutions

<details>
<summary><strong>Solution 1: WordNet Integration</strong> [❌ REJECTED]</summary>

**What WordNet Provides:**
- **Concepts as Classes**: Every synset becomes an `owl:Class`
  - Example: `FOUNDATION:Concept_oewn_02084442_n` = "dog, domestic dog, Canis familiaris"
- **Semantic Relations**:
  - Hierarchical: hypernym/hyponym (is-a)
  - Part-whole: meronym/holonym (part-of, member-of, made-of)
  - Similarity: similar, also-see, antonym
  - Causality: entails, causes
  - Domain: domain-topic, pertainym
- **Metadata**: Labels (via `rdfs:label`), definitions (via `rdfs:comment`), examples (via `skos:example`)

**What WordNet Does NOT Provide:**
- ❌ Datatype properties (`hasName`, `hasAge`, `hasBirthDate`)
- ❌ Instances (no "Lassie" or "Snoopy", only the concept "dog")
- ❌ Domain-specific schemas (no "Customer", "Invoice", "Order")
- ❌ Business rules or constraints

**Pros:**
- Rich semantic network (102K+ concepts)
- Well-established linguistic resource
- Covers nouns, verbs, adjectives, adverbs
- Provides semantic relations automatically

**Cons:**
- Large import size (~520K triples, 246MB database)
- Linguistic focus (good for NLP, overkill for business data)
- No instances, only concepts
- English-only (WordNet 2024)
- 30+ second import time

**Why Rejected:**
Too comprehensive for practical use. FOUNDATION needs essential concepts, not exhaustive linguistic coverage. The overhead of 520K triples outweighs the benefit of having every possible concept pre-defined.

**Implementation Attempt:**
- Full import of WordNet 2024 (102K synsets)
- O(n) parser in [scripts/build-database.cjs](../../../../scripts/build-database.cjs)
- Result: Working but impractical (246MB database, 30s build time)

</details>

<details>
<summary><strong>Solution 2: Schema.org Vocabulary</strong> [⏸️ EVALUATED]</summary>

**Characteristics:**
- Web-focused structured data vocabulary
- ~800 types, ~1,400 properties
- Has instances and rich property definitions
- Multilingual support
- Smaller than WordNet (~50K triples)

**Pros:**
- Industry standard for web data
- Practical, real-world concepts
- Good coverage of common domains (Person, Organization, Event, Product)
- Well-documented with examples

**Cons:**
- Still external dependency
- Web-centric bias (may not fit all FOUNDATION use cases)
- Some concepts too specific, others too abstract

**Status:** Considered but not implemented. Prefer building custom ontology incrementally.

</details>

<details open>
<summary><strong>Solution 3: Custom Minimal Ontology</strong> [✅ CURRENT]</summary>

**Approach:**
Build minimal base ontology manually, grow incrementally based on real needs.

**Core Ontology Structure:**

1. **External Vocabularies**:
   - **DTYPE** (Datatype Schema): 288 triples for typed data (unit-aware properties)
   - **QUDT** (Quantities, Units, Dimensions, Types): 48K+ triples for scientific units and quantities
   - **RDF/RDFS/OWL Core**: Standard W3C vocabulary

**Key Design Decisions:**

1. **Naming Convention**:
   - Classes ending in "Capacity" = behavior/capability (e.g., `AgentCapacity`)
   - Without suffix = nature/essence (e.g., `Person`, `Computer`)

2. **One File Per Class**:
   - Each class in separate .ttl file in [core-ontology/](../../../../core-ontology/)
   - OOP-style: class definition + its specific properties in same file
   - Example: `Person.ttl` defines both `foundation:Person` class and person-specific properties

3. **Documentation in Ontology**:
   - Every class has `rdfs:label`, `rdfs:comment`, `foundation:icon`
   - `rdfs:seeAlso` contains practical usage examples
   - Examples use real data from FOUNDATION instances

4. **Runtime Import**:
   - Ontology loaded at application startup (not build-time)
   - Progress events emitted during import for UI feedback
   - Graceful handling of missing/malformed ontology files

5. **Dependency Resolution**:
   - Foundation classes imported via directory scan
</details>

---

### Success Criteria

A successful base ontology should:

1. ✅ **Provide semantic foundation** without dictating application structure
2. ✅ **Be queryable** for concept discovery ("what is an agent?", "what types of things exist?")
3. ✅ **Have reasonable size** (tradeoff: completeness vs performance)
4. ✅ **Be maintainable** (can update/extend without breaking user data)

## Related Problems

- [Don't Know How to Structure Data](semantic-data-structure.md) - Application schema definition (separate concern)
- [Database Selection](database-selection.md) - Storage architecture
- [Technology Stack](technology-stack.md) - RDF as the solution

## References

- WordNet: https://wordnet.princeton.edu/
- English WordNet 2024: https://en-word.net/
- Schema.org: https://schema.org/
- ConceptNet: https://conceptnet.io/
- DBpedia: https://www.dbpedia.org/
