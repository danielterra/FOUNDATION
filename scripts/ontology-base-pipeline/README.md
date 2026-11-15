# Ontology base pipeline

This Rust CLI is the sole generator and verifier for the curated `foundation-core` ontology seed and semantic widget catalog.

Run from the Foundation repository root:

```sh
cargo run --manifest-path scripts/ontology-base-pipeline/Cargo.toml -- generate-overlay
cargo run --manifest-path scripts/ontology-base-pipeline/Cargo.toml -- export-sql
cargo run --manifest-path scripts/ontology-base-pipeline/Cargo.toml -- verify
node --test src/lib/json-render/semantic/spec.test.ts
```

`generate-overlay` reads the checked-in semantic catalog manifest, preserves immutable historical contract versions, and writes the Turtle history plus the packaged core overlay. `export-sql` compiles only the tracked curated Turtle sources into the seed SQL; it never reads the user's database. `verify` checks contract hashes and versions, enforces the exact subject, predicate, and vocabulary-IRI sets in `../foundation-core/ontology/base/seed-allowlist.txt`, scans for personal or secret material, compares the seed to a deterministic re-export, and imports it into a clean in-memory database. The TypeScript test proves the build manifest matches the runtime component schemas, actions, and examples.

## Explicit seed allowlist

`seed-allowlist.txt` is a reviewed, checked-in inventory. Verification never derives or extends the allowlist from Turtle. The subject, predicate, and non-subject vocabulary-IRI sets must match exactly; adding or removing a canonical subject, predicate, or vocabulary term therefore fails until the inventory is manually reviewed and updated alongside the source change. Blank-node subjects are listed too, so a new structural node also requires an explicit inventory change. Do not generate the file from local database contents. Timestamp/name and secret scans remain secondary checks and never authorize an entry. A new timestamp-shaped core IRI also needs an explicit entry in the timestamp exception list in `verify_seed_safety`.

## Canonical semantic contract hash

The contract hash is SHA-256 over UTF-8 bytes of compact canonical JSON with fields `componentId`, `intent`, `propsSchema`, `actionsSchema`, and `examples`. All object keys are sorted recursively in ascending lexical order; array order is preserved. For a component with slots, add `propsSchema["x-foundation-slots"]` with the declared slot array; omit that key when there are no slots. `manifestVersion` and `contractVersion` are not part of the payload. The shared byte-exact Action payload and golden hashes for all current components are in `contract-hash-golden.json`; Rust, the runtime, and both verifiers must match these vectors.

## Curated timestamped IRIs

Timestamp-shaped IRIs are scanned in every RDF position (subject, predicate, and IRI object). Only the exact core vocabulary individuals listed in `verify_seed_safety` are allowed: the shipped Company, FileType, MCPTool, Persona, SoftwareRelease, Status, and WidgetType individuals. Local user entities, QA fixtures, automation instances, and ad-hoc properties must not be added to `core.ttl`. The allowlist is intentionally exact-ID based; adding an ID requires confirming it is part of the product's shipped core vocabulary and updating this documentation and the regression test.
