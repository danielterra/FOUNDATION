---
name: product-integrity-audit
description: "Run a read-only integrity audit of Foundation Features, User Stories, Acceptance Criteria, statuses, scope, and QA evidence. Use after feature code sync and before release preparation."
---

# Product Integrity Audit

Run a read-only audit of the product graph. This procedure reports inconsistencies; it never changes status, criteria, scope, or changelog values.

## Data sources

- Use Foundation MCP only for ontology reads. Never use raw SQL or direct database access.
- Start from the requested product IRI. If omitted for Foundation, use `foundation:FoundationProduct`.
- Traverse Product → Features → User Stories → Acceptance Criteria using current object-property values.
- Read each entity's current `foundation:hasStatus`, parent link, relevant `successCriteria` / `capability`, and available QA evidence in its changelog or implementation plan.
- Read code or test artifacts only to investigate a reported graph-to-code mismatch. Do not treat code presence or a changelog summary alone as proof that an AC passed.
- If a collection response is truncated or the complete child set cannot be established, report the audit as incomplete and do not certify the product.

## Checks

1. Every Feature, User Story, and Acceptance Criterion reached in the product traversal has exactly one current canonical status allowed for its class.
2. Every traversed User Story points back to the Feature being inspected, and every Acceptance Criterion points to its User Story; report missing or conflicting links.
3. A Completed User Story has every current in-scope AC Completed, no Pending or Rejected AC, and QA evidence for each AC.
4. A Completed Feature has an explicitly identified delivered scope, and every User Story in that scope is Completed through QA. A linked story alone does not prove that it belongs to the delivered scope.
5. A QA User Story remains in QA until every AC has evidence and the QA gate has finished. Completed ACs under a QA parent are valid while other checks remain.
6. An AC added or changed after its User Story reached QA or Completed requires the User Story to return to Mudança Pendente before re-planning. Reopen its Feature only when the change proves a gap in delivered `successCriteria`.
7. Flag statuses that cannot be reconciled with the available code, test, or QA evidence. Do not infer a replacement status.

## Severity

- **Blocker:** missing/invalid status or parent; Completed User Story with an unfinished in-scope AC; Completed Feature with an unfinished in-scope User Story; Completed status without required QA evidence; audit data incomplete.
- **Review needed:** Feature/story scope is unclear; code and AC appear different; evidence is insufficient to decide whether a status is current.
- **Informational:** completed ACs under a QA parent or explicitly out-of-scope expansion stories.

## Report

Return the product IRI, audit timestamp, whether the traversal was complete, totals by entity type and status, and each finding with entity IRI, current status, parent, evidence inspected, severity, and the next workflow (`$feature-code-sync`, `$userstory-plan`, or QA). Do not mutate the graph. Release preparation stops while any Blocker remains; resolve Review needed findings or record the explicit scope decision before release.
