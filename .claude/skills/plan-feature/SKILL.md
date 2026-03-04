---
name: plan-feature
description: Plans a new feature by analyzing the codebase and storing it directly in Foundation
disable-model-invocation: true
argument-hint: "<description of the feature to plan>"
---

# Plan Feature: $ARGUMENTS

## Current State
!`git status --short`

## Recent Commits
!`git log --oneline -10`

---

## Steps

1. **Understand the feature** from `$ARGUMENTS`
2. **Explore the codebase** — read the project structure, identify languages, frameworks, layers, and conventions used
3. **Load architecture context** — call `remember_things` with `concept_iri=foundation:FrontendArchitecturePlan`
   and `concept_iri=foundation:BackendArchitecturePlan`, filtering by `foundation:contributesTo = foundation:FoundationProduct`;
   for each plan found, call `remember_thing` to read its full content; use this context to ensure new plans align with the existing architecture
4. **Decompose into User Stories** — each story: "As a [role], I want [capability], so that [benefit]"
4. **Identify plan perspectives per story** — only include types that apply:
   - `BackendArchitecturePlan`: server-side services, APIs, data access, business logic
   - `FrontendArchitecturePlan`: UI components, state management, routing, API integration
   - `UIUXDesignPlan`: new screens, user flows, or significant interaction design changes
   - `DataArchitecturePlan`: schema changes, migrations, new storage structures
   - `OntologyPlan`: new or modified ontology concepts
5. **Design tasks bottom-up** — data layer first, then business logic, then API, then UI
6. **Identify plan dependencies** — e.g., FrontendArchitecturePlan dependsOn BackendArchitecturePlan
7. **Identify risks** — migrations, breaking changes, new dependencies, test coverage gaps

---

## Output — Store in Foundation via MCP

### 1. Feature

Call `learn_thing` with `concept_iri=foundation:SoftwareFeature`, then set details with `learn_thing_detail`:
- `rdfs:comment` → overview of what this feature does and why (value_type: literal)

### 2. Personas

Before creating, call `remember_things` with `concept_iri=foundation:Persona` — reuse existing ones if they match.

Call `learn_thing` with `concept_iri=foundation:Persona`, then set:
- `foundation:personaGoals` → what this persona wants to achieve
- `foundation:personaContext` → device, frequency, expertise, environment

### 3. User Stories

Call `learn_thing` with `concept_iri=foundation:UserStory`, label: `"As a ..., I want ..."`, then set:
- `foundation:capability` → the capability
- `foundation:benefit` → the benefit
- `foundation:acceptanceCriteria` → bullet list of done conditions
- `foundation:userRole` → Persona IRI (value_type: iri)
- `foundation:partOfFeature` → Feature IRI (value_type: iri)
- `foundation:storyStatus` → `foundation:Pending` (value_type: iri)

### 4. Plans

Call `learn_thing` with the appropriate plan concept, then set:
- `foundation:overview` → what work is needed and why
- `foundation:risks` → risks and mitigations (omit if none)
- `foundation:plannedAt` → ISO datetime (datatype: xsd:dateTime)
- `foundation:contributesTo` → UserStory IRI (value_type: iri)
- `foundation:dependsOn` → other Plan IRI (value_type: iri) — if applicable

Plan icons: `BackendArchitecturePlan`=`dns`, `FrontendArchitecturePlan`=`web`, `UIUXDesignPlan`=`design_services`, `DataArchitecturePlan`=`schema`, `OntologyPlan`=`account_tree`

### 5. Tasks

Call `learn_thing` with `concept_iri=foundation:Task`, then set:
- `foundation:description` → what to implement (files, functions, endpoints, etc.)
- `foundation:notes` → path/to/file — if applicable
- `foundation:status` → `foundation:Pending` (value_type: iri)
- `foundation:dependsOn` → previous Task IRI (value_type: iri) — for ordering
- `foundation:contributesTo` → Plan IRI (value_type: iri)

---

After storing all entities, print a summary: feature name, plan types created, number of tasks per plan, and the IRIs of created entities.
