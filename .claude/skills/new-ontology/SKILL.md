---
name: new-ontology
description: Creates a new ontology concept in Foundation with no duplication and semantic consistency
argument-hint: <name and description of the concept>
---

# New Ontology: $ARGUMENTS

## Step 1 — Duplication Check

Call `remember_concepts` with keywords from the concept name to check for existing classes. Search multiple times with different keywords. If an existing class already covers the concept, use it instead of creating a new one.

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

Call `remember_concepts` to check if similar properties already exist before defining new ones. Prefer object properties over primitives:

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

## Step 3 — Consistency Rules

- **No orphan classes** — every class needs a `super_class`
- **`domain`** on every property; omit only for shared properties (domain: `owl:Thing`)
- **Property placement** — define each property on its **domain** class, never on the range class
- **Inverse properties** — add when a directional relationship warrants it

## Output

1. Call `learn_concept` to create the new class:
   - `iri`: `foundation:ClassName`
   - `label`: English name
   - `icon`: Material icon name
   - `comment`: Concise definition
   - `super_class`: parent class IRI

2. For each property, call `learn_connection_type`:
   - `iri`: `foundation:propertyName`
   - `label`: English name
   - `property_type`: `object` for links to other classes, `datatype` for scalars
   - `domain`: class this property belongs to (omit for universal `owl:Thing`)
   - `range`: target class IRI (for `object`) or xsd type (for `datatype`)
   - `comment`: what it represents

After creating, summarize: parent class, properties created (type/range), and anything reused or ambiguous.
