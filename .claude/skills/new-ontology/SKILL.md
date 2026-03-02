---
name: new-ontology
description: Creates a new ontology TTL file with no duplication and semantic consistency
disable-model-invocation: true
argument-hint: <name and description of the concept>
---

# New Ontology: $ARGUMENTS

## Existing Ontologies
!`ls core-ontology/*.ttl | sed 's|core-ontology/||;s|\.ttl||' | sort`

## Existing Classes
!`grep -rh "^foundation:[A-Z][a-zA-Z]* a owl:Class" core-ontology/*.ttl | sed 's/ a owl:Class.*//' | sort`

## Existing Properties
!`grep -rh "^foundation:[a-z][a-zA-Z]* a owl:" core-ontology/*.ttl | sed 's/ a owl:.*//' | sort`

---

## Step 1 — Duplication Check

Before writing anything, verify no existing class/property covers this concept. Use it instead of creating a duplicate.

Common base classes:

| Class | When to use |
|---|---|
| `foundation:AbstractThing` | ideas, concepts, information |
| `foundation:ConcreteThing` | physical objects |
| `foundation:DigitalThing` | files, software, data |
| `foundation:InformationObject` | structured information |
| `foundation:Process` | activities, workflows |
| `foundation:AgentCapacity` | persons, organizations, AI |
| `foundation:IdentificationDocument` | official documents |
| `foundation:Contract` | legal agreements |
| `foundation:FinancialTransaction` | money movements |
| `foundation:GeographicLocation` | places |

## Step 2 — Property Types

Prefer object properties over primitives. Use existing classes as `rdfs:range`:

| Value | Use |
|---|---|
| City/Municipality | `foundation:City` |
| State/Country | `foundation:State` / `foundation:Country` |
| Address | `foundation:Address` |
| Person | `foundation:Person` |
| Company/Org | `foundation:Company` / `foundation:Organization` |
| Email/Phone | `foundation:EmailAddress` / `foundation:PhoneNumber` |
| File | `foundation:File` |
| Financial institution | `foundation:FinancialInstitution` |
| Status | `foundation:Status` |

Use primitives only for: codes/identifiers, free-text, and scalar numeric values.

**Numeric measurements** must use `qudt:hasUnit`:
```turtle
foundation:height a owl:DatatypeProperty ; rdfs:range xsd:decimal ; qudt:hasUnit unit:M .
foundation:price  a owl:DatatypeProperty ; rdfs:range xsd:decimal ; qudt:hasUnit currency:BRL .
```

**Cardinality** must be explicit in the class `rdfs:subClassOf` restrictions:
- `owl:cardinality "1"` — required, exactly one
- `owl:maxCardinality "1"` — optional, at most one
- No restriction — 0 or more; document in `rdfs:seeAlso`

## Step 3 — File Structure

```turtle
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix foundation: <http://foundation.local/ontology/> .

# =============================================================================
# ClassName — one-line description
# Version: 0.1.0 | License: GNU GPL
# =============================================================================

<http://foundation.local/ontology/ClassName>
    a owl:Ontology ;
    owl:imports <http://foundation.local/ontology/ImportedClass> .

foundation:ClassName a owl:Class ;
    rdfs:subClassOf foundation:ParentClass ,
        [ a owl:Restriction ; owl:onProperty foundation:requiredProp ;
          owl:cardinality "1"^^xsd:nonNegativeInteger ] ,
        [ a owl:Restriction ; owl:onProperty foundation:optionalProp ;
          owl:maxCardinality "1"^^xsd:nonNegativeInteger ] ;
    rdfs:label "Class Name" ;
    rdfs:comment "Concise definition" ;
    foundation:icon "material_icon_name" ;
    rdfs:seeAlso """
Examples: ...
Cardinality: requiredProp exactly 1, optionalProp max 1, multiProp 0+
""" .

# -----------------------------------------------------------------------------
# Properties
# -----------------------------------------------------------------------------

foundation:propName a owl:ObjectProperty ;
    rdfs:label "prop name" ;
    rdfs:comment "What it represents" ;
    rdfs:domain foundation:ClassName ;
    rdfs:range foundation:OtherClass ;
    rdfs:seeAlso "Example: :inst foundation:propName :val ." .
```

## Step 4 — Consistency Rules

- **One class per file**; filename = class name (`MyClass.ttl` → `foundation:MyClass`)
- **`rdfs:domain`** on every property; omit only for shared properties (document why)
- **`owl:imports`** for every ontology used as `rdfs:range`
- **No orphan classes** — every class needs `rdfs:subClassOf`
- **Inverse properties** — add when a directional relationship warrants it
- **`rdfs:subPropertyOf`** when a property specializes a more general one

## Output

Write to `core-ontology/ClassName.ttl` then summarize: parent class, properties (type/range/cardinality), imports, and anything reused or ambiguous.
