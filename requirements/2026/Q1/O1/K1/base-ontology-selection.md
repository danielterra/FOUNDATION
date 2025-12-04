# Problem 4: Base Ontology Selection ~~and WordNet Integration~~

## Problem Statement

FOUNDATION needs a foundational ontology that provides essential concepts without coupling to application-specific data structures. ~~We've been exploring WordNet as the vocabulary source, but need to clarify its role and limitations.~~ **Update (2024-12-04):** WordNet approach rejected. Moving to manual, curated base ontology.

## Context

There are two distinct concerns that were getting mixed:

1. **Base Ontology** (universal vocabulary)
   - Provides concepts and semantic relationships
   - Should be application-agnostic
   - Examples: "dog is-a mammal", "telephone is-a device"

2. **Application Data Structure** (FOUNDATION-specific schema)
   - Properties for instances: `hasName`, `hasAge`, `hasPhoneNumber`
   - Business rules and constraints
   - Should be defined separately (see [semantic-data-structure.md](semantic-data-structure.md))

## Current State

### What We Have
- ✅ RDF/RDFS/OWL core vocabulary ([rdf-rdfs-owl-core.ttl](../../../../core-ontology/rdf-rdfs-owl-core.ttl))
- ✅ WordNet 2024 imported (102,154 synsets: 84,956 nouns, 13,830 verbs, 7,502 adjectives, 3,622 adverbs)
- ✅ Semantic relations working: hypernym, meronym, holonym, antonym, etc.
- ✅ Parser optimized for O(n) single-pass processing
- ✅ Labels extracted from lemmas and definitions

### What WordNet Provides
- **Concepts as Classes**: Every synset becomes an `owl:Class`
  - Example: `FOUNDATION:Concept_oewn_02084442_n` = "dog, domestic dog, Canis familiaris"
- **Semantic Relations**:
  - Hierarchical: hypernym/hyponym (is-a)
  - Part-whole: meronym/holonym (part-of, member-of, made-of)
  - Similarity: similar, also-see, antonym
  - Causality: entails, causes
  - Domain: domain-topic, pertainym
- **Metadata**: Labels (via `rdfs:label`), definitions (via `rdfs:comment`), examples (via `skos:example`)

### What WordNet Does NOT Provide
- ❌ Datatype properties (`hasName`, `hasAge`, `hasBirthDate`)
- ❌ Instances (no "Lassie" or "Snoopy", only the concept "dog")
- ❌ Domain-specific schemas (no "Customer", "Invoice", "Order")
- ❌ Business rules or constraints

## Questions to Answer

### 1. Is WordNet the Right Choice?

**Pros:**
- Rich semantic network (102K+ concepts)
- Well-established linguistic resource
- Covers nouns, verbs, adjectives, adverbs
- Provides semantic relations automatically

**Cons:**
- Large import size (~520K triples, 246MB database)
- Linguistic focus (good for NLP, maybe overkill for business data?)
- No instances, only concepts
- English-only (WordNet 2024)

**Alternatives to Consider:**
- **Schema.org**: Web-focused, has instances and properties, smaller, multilingual
- **DBpedia**: Wikipedia-based, has instances (Barack Obama, Eiffel Tower)
- **UMBEL**: Upper-level ontology, lighter than WordNet
- **ConceptNet**: Multilingual, simpler structure
- **Custom minimal ontology**: Only concepts we actually need

### 2. Should We Keep All WordNet Data?

Current import: **All synsets** (102K concepts)

Options:
- **A) Keep all**: Rich semantic network, but large
- **B) Filter by frequency**: Only common concepts (top 10K? 20K?)
- **C) Filter by domain**: Only concrete nouns (remove abstract/rare terms)
- **D) Lazy loading**: Import concepts on-demand as users reference them
- **E) Replace with lighter alternative**: Schema.org, custom ontology

### 3. How Should WordNet Integrate with User Data?

When a user creates a class like `FOUNDATION:Customer`:

**Option A - Subclass WordNet concepts:**
```turtle
FOUNDATION:Customer rdfs:subClassOf FOUNDATION:Concept_oewn_person_n .
```

**Option B - SKOS mapping (loose coupling):**
```turtle
FOUNDATION:Customer skos:related FOUNDATION:Concept_oewn_person_n .
```

**Option C - No direct link:**
```turtle
FOUNDATION:Customer a owl:Class .
# User manually adds relationships if needed
```

### 4. What About Multilingual Support?

WordNet 2024 is English-only. Options:
- Keep English WordNet, add translation layer later
- Switch to multilingual alternative (ConceptNet, BabelNet)
- Use language-tagged literals for labels

## Implementation History

### Attempts Made

1. **Initial import of complex ontologies** (FOAF, Dublin Core, Schema.org)
   - **Result**: Too complex, too many dependencies
   - **Decision**: Removed, kept only RDF/RDFS/OWL core

2. **WordNet full import** (current)
   - **Result**: Works, but large (246MB)
   - **Performance**: O(n) parser, ~30s import time
   - **Status**: ✅ Working, but questioning if it's the right approach

3. **Attempted to add `hasName` to core ontology**
   - **Result**: Correctly identified as mixing concerns
   - **Decision**: Reverted, keep base ontology separate from app schema

### Current Files

- [core-ontology/rdf-rdfs-owl-core.ttl](../../../../core-ontology/rdf-rdfs-owl-core.ttl) - RDF/RDFS/OWL vocabulary (189 triples)
- [core-ontology/english-wordnet-2024.ttl](../../../../core-ontology/english-wordnet-2024.ttl) - WordNet synsets (5.7GB source)
- [scripts/build-database.cjs](../../../../scripts/build-database.cjs) - Import script with WordNet parser
- Database: `FOUNDATION.db` (246MB, 520K triples)

## Success Criteria

A successful base ontology should:

1. **Provide semantic foundation** without dictating application structure
2. **Be queryable** for concept discovery ("what is a dog?", "what are types of vehicles?")
3. **Have reasonable size** (tradeoff: completeness vs performance)
4. **Support multilingual** labels (future requirement)
5. **Not conflict** with user-defined schemas
6. **Be maintainable** (can update/extend without breaking user data)

## ~~Next Steps~~ Decision Made (2024-12-04)

~~Need to decide:~~

1. [x] ~~Keep WordNet or explore alternatives?~~ **REJECTED WordNet** - too comprehensive, not practical
2. [x] ~~If keeping WordNet: Full import or filtered subset?~~ **N/A** - not using WordNet
3. [ ] How should user classes relate to base ontology concepts? - **Still relevant**
4. [ ] Should we support multilingual from the start? - **Defer to later**
5. [x] Document the separation between base ontology and app schema clearly - **Documented in semantic-data-structure.md**

## New Approach: Manual, Curated Base Ontology

**Decision:** Build minimal base ontology manually, grow incrementally based on real needs.

**Starting point (Q1 focus on personal finance):**
- Transaction, Category, Account (financial concepts)
- Core properties: amount, date, hasCategory
- Extend as needed for other use cases

**See:** [Solution 5 in semantic-data-structure.md](semantic-data-structure.md) for implementation details.

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
