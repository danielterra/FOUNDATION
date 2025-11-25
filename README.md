# SuperNOVA

## Simple as a spreadsheet. Powerful as enterprise software.

**What if creating powerful systems was as simple as using a spreadsheet?**

SuperNOVA reimagines how anyone — not just developers — can create tools for managing, automating, and analyzing their data. Just as spreadsheets democratized data management by making it accessible to everyone, SuperNOVA gives you one flexible environment where you can manage, automate, and analyze anything — your projects, customers, inventory, finances, or whatever matters to your life.

Where spreadsheets give you columns, SuperNOVA gives you **ontologies** — structured relationships between your data that stay intact as your system evolves. A shared base ontology acts like a common dictionary, enabling different SuperNOVA instances to understand each other's data without custom mappings or integrations.

Where spreadsheets give you formulas, SuperNOVA gives you **automation**. Formulas recalculate values, but they can't send emails, update external systems, or trigger complex workflows. SuperNOVA reacts to every change with real automation — connecting to APIs and orchestrating multi-step processes without manual intervention.

Where spreadsheets break at scale, SuperNOVA grows without limits. Excel caps at 1 million rows, Google Sheets at 10 million cells — and performance degrades long before that. SuperNOVA uses database architecture to handle billions of records efficiently. Need more? Add nodes to your cluster — another laptop, a cloud server, whatever fits your needs.

Just as you can share a spreadsheet and collaborate with others, SuperNOVA enables seamless collaboration through its shared ontology. But unlike spreadsheets where multiple editors risk conflicts and overwrites, SuperNOVA provides proper conflict resolution, granular permissions, and full audit trails.

---

## Why This Project Exists

### The Spreadsheet Dilemma

Spreadsheets are the most popular software in the world. An estimated 1+ billion people use Excel alone[^1], with over 100 million professionals listing it as a core skill[^2]. Businesses of all sizes — from small startups to Fortune 500 companies — depend on spreadsheets for critical operations: 72% of enterprises use them for financial modeling and business intelligence[^3], and over 90% of administrative and managerial jobs require spreadsheet proficiency[^4].

Yet spreadsheets have fundamental limits that make them fragile and difficult to scale:

- **Weak data relationships**: No proper foreign keys, no referential integrity — relationships are just cell references that break when rows move or sheets are reorganized
- **No reactive automation**: Changes don't trigger workflows; there's no event system to automate responses when data changes
- **No validation or constraints**: Anyone can type anything anywhere; there's no way to enforce data types, required fields, or business rules
- **Manual and error-prone**: Copy-paste operations, formula mistakes, and accidental deletions happen constantly with no safeguards
- **Limited scalability**: Performance degrades severely with size; hitting row/column limits breaks critical systems
- **No proper querying**: You can't ask complex questions across multiple sheets or perform relational queries without building elaborate, brittle formulas
- **Flat structure**: Everything lives in rows and columns; you can't model hierarchies, graphs, or complex entity relationships naturally

**And when they fail, the consequences are serious:** JP Morgan Chase lost $6.2 billion due to a copy-paste error in a risk model[^5]. TransAlta Corp lost $24 million when misaligned rows caused bids to match wrong contracts[^6]. Fidelity Investments made a $2.6 billion accounting error from a missing minus sign[^7]. During the COVID-19 pandemic, Public Health England lost track of 15,841 positive cases because Excel hit its row limit[^8]. Studies show that 88% of all spreadsheets contain serious errors[^9] — yet businesses have no better alternative for the flexibility they need.

[^5]: [JPMorgan "London Whale" Excel error - Dear Analyst](https://www.thekeycuts.com/dear-analyst-38-breaking-down-an-excel-error-that-led-to-six-billion-loss-at-jpmorgan-chase/)
[^6]: [TransAlta $24M loss - Excel Disasters](https://sheetcast.com/articles/ten-memorable-excel-disasters)
[^7]: [Fidelity $2.6B error - Biggest Excel Mistakes](https://blog.hurree.co/8-of-the-biggest-excel-mistakes-of-all-time)
[^8]: [COVID-19 data loss - Spreadsheet Disasters](https://gridfox.com/blog/5-spreadsheet-disasters-that-prove-their-risk/)
[^9]: [88% error rate - Wall of Shame for Excel Errors](https://www.solving-finance.com/post/the-wall-of-shame-for-the-worst-excel-errors)

[^1]: [Senacea - How many people use Excel?](https://www.senacea.co.uk/post/excel-users-how-many)
[^2]: [LinkedIn profiles analysis - Excel as listed skill](https://www.senacea.co.uk/post/excel-users-how-many)
[^3]: [Global office software market research - Grand View Research](https://www.grandviewresearch.com/industry-analysis/office-software-market-report)
[^4]: [U.S. Bureau of Labor Statistics - Spreadsheet proficiency requirements](https://www.excel4business.com/resources/research-into-excel-use.php)

### The Fragmentation Problem

Today, our data lives scattered across hundreds of applications and services. Your contacts are in one place, your projects in another, your finances elsewhere. Each silo has its own interface, its own rules, its own way of doing things. Moving data between them is painful or impossible. You can't create your own connections, your own automations, your own view of how everything relates.

### You Don't Own Your Data

Most applications store your data on their servers. You access it through their interface, under their terms. If they change their pricing, shut down, or decide to restrict features, you're stuck. Your data — your knowledge, your work, your life — is held hostage by business models you can't control.

### Wasted Computing Power

Modern personal computers are extraordinarily powerful — multi-core processors, gigabytes of RAM, terabytes of storage. Yet we use them mostly as dumb terminals to access cloud services. All that computing power sits idle while we wait for distant servers to process our requests, subject to their limitations and costs.

---

## Principles and Strategies

1. **Simplicity**
   SuperNOVA must be as intuitive as a spreadsheet. AI acts as an assistant, helping users build without writing code.

2. **Decentralization and Autonomy**
   Everything runs on the users’ own devices. Each machine is an independent, collaborative node capable of sharing load and data. There are no central servers — computing power and control belong to the users.

3. **Open and Extensible Ontology**
   Knowledge is structured by an ontology composed of **classes** and **relationships**. This foundation can follow standards like RDF and build upon public ontologies to ensure interoperability. Users only create what is specific to their context, extending existing classes. Through inference, the system automatically understands extended classes and their relationships — if you create a "SoftwareEngineer" that extends "Person", SuperNOVA knows it inherits all Person properties and behaviors without explicit configuration.

4. **Total Reactivity**
   Everything in SuperNOVA is a reaction to persisted database changes. The system doesn’t need logs or external events — the database itself *is* the log. Each state modification triggers actions, automatic or manual, turning data into a living process.

5. **Immutable Data**
   No record is directly updated. Each change is the insertion of a new fact that logically replaces the previous one. The history is continuous and traceable, preserving integrity and the complete system timeline.

6. **Origin and Traceability**
   Every piece of information has an origin — whether a user, an external system, or an automation. This traceability ensures transparency and accountability for every event.

7. **Open Source and Collaboration**
   SuperNOVA is an open project built with and by the community. The codebase, base ontology, and documentation will be public, fostering innovation, trust, and collective evolution.

8. **Decentralized Authentication**
   Identity in SuperNOVA is guaranteed through **ECDSA (Elliptic Curve Digital Signature Algorithm)**. Each user shares their public key as a digital calling card, allowing others to verify their identity through data signatures. The private key never leaves the user’s computer — it’s personal, non-transferable, and secure. This approach eliminates dependencies on authentication servers and reinforces individual data sovereignty.

9. **Secure and Distributed Communication**
   Communication between computers in SuperNOVA can occur over local networks or the internet. All interactions between nodes must use **HTTPS connections**, ensuring privacy, mutual authentication, and data integrity. This secure layer is vital for maintaining decentralization without sacrificing trust.

10. **Fork Your Data**
    Just like duplicating a spreadsheet to test new ideas, SuperNOVA allows users to **fork their data**. They can experiment with changes, simulate scenarios, and validate hypotheses without affecting the main environment. Once satisfied, they can apply the modifications to the original system. This freedom fosters safety and continuous innovation.

---

## Work Methodology

SuperNOVA follows a problem-driven, outcome-focused development approach:

### Quarterly Planning with OKRs

Every quarter, we define **OKRs (Objectives and Key Results)** that are always aligned with the project's core objective and guiding principles. OKRs are documented in `requirements/YYYY/Q[n]/OKRS.md`.

- **Objectives** are user-centered outcomes that deliver clear value
- **Key Results** are measurable indicators of success from the user's perspective
- Technical implementation is a means to achieve user outcomes, not the goal itself

### Problem-Driven Development

For each Key Result, we identify the **problems** that need to be solved to achieve that result. A problem can be:
- A technical challenge that blocks the outcome
- A knowledge gap that requires research or experimentation
- A design question that needs user validation

Problems are documented in: `requirements/YYYY/Q[n]/O[n]/K[n]/P[n].md`

Each problem document should clearly state:
- **What** we don't know or can't do yet
- **Why** solving it is necessary for the Key Result
- **How** we might approach solving it (hypotheses)
- **Success criteria** - how we'll know the problem is solved

### From Problems to Solutions

Once problems are identified and understood, we create solutions through iterative development:
1. **Research** - Gather information, prototype, experiment
2. **Implement** - Build the minimal solution that addresses the problem
3. **Validate** - Test with real users or realistic scenarios
4. **Iterate** - Refine based on feedback until success criteria are met

This approach keeps us focused on delivering real value rather than building features for their own sake.

---

## Technology Stack

SuperNOVA is built with a **minimalist, pragmatic** philosophy: native technologies, rapid validation, zero complexity.

**MVP Stack:**
- **Framework**: Tauri (cross-platform desktop apps with web frontend)
- **Backend**: Rust (systems language, memory-safe, high performance)
- **Frontend**: Svelte (reactive, minimal framework, fast)
- **Database**: SQLite (embedded, zero-config, ACID-compliant, universally supported)
- **Data Model**: RDF-inspired graph model (subject-predicate-object)
- **Ontology**: BFO + Common Core Ontologies as base vocabulary
- **Build System**: Cargo + npm

**Why This Stack for MVP:**

1. **Tauri** - Cross-platform from day one (macOS, Windows, Linux), small binaries, native OS integration
2. **Rust** - Memory safety, excellent performance, strong ecosystem for backend logic
3. **Svelte** - Minimal JavaScript framework, reactive by default, easy to learn
4. **SQLite** - Battle-tested embedded database, zero-config, ACID guarantees, perfect for local-first architecture, easily replicable file-based storage
5. **BFO + CCO** - Industry-standard upper ontology (BFO) with domain extensions (Common Core Ontologies) for interoperability and semantic richness

**Data Architecture:**

SuperNOVA uses an RDF-inspired architecture that stores data as **triples** (subject-predicate-object):

```
Person (Class) → owns property → "name" (DataProperty)
Person (Class) → owns property → "worksAt" (ObjectProperty → Organization)

john:123 (Resource) → rdf:type → Person (Class)
john:123 (Resource) → name → "John Doe" (Literal)
john:123 (Resource) → worksAt → acme:456 (Resource)
```

This enables:
- ✅ **Strong relationships** - Real graph queries, not fragile cell references
- ✅ **Interoperability** - Export/import RDF (Turtle, JSON-LD) between instances
- ✅ **Extensibility** - Extend Schema.org classes without breaking compatibility
- ✅ **Semantic queries** - Navigate relationships naturally
- ✅ **AI-friendly** - Structured data that both humans and AI understand

**Philosophy: Cross-Platform, Local-First, Distributed-Ready**

This stack delivers:
- ✅ **Cross-platform from start** - Single codebase runs on macOS, Windows, Linux
- ✅ **Local-first architecture** - SQLite embedded, zero-config, single file database
- ✅ **Distributed-ready** - SQLite file easily synced, replicated via git, rsync, or custom protocols
- ✅ **Small footprint** - Tauri apps are 3-5 MB, significantly smaller than Electron
- ✅ **Native performance** - Rust backend with native OS integration via Tauri
- ✅ **Versioned ontologies** - OWL/TTL ontologies imported into SQLite, versionable in git
- ✅ **Universal support** - SQLite runs everywhere, no dependencies, rock-solid stability

**Future Enhancements:**

As the project matures, we'll add:
- ⏳ Advanced RDF features (SPARQL queries, OWL reasoning)
- ⏳ CRDT-based conflict resolution for distributed sync
- ⏳ Plugin system for extending ontology and automations
- ⏳ Mobile apps (iOS, Android) using Tauri's mobile support

**Binary Size**: ~3-5 MB (cross-platform desktop app)

**Decision References:**
- Database selection: [P1 - Database Technology Selection](requirements/2026/Q1/O1/K1/P1.md)
- Stack selection: [P2 - Technology Stack Selection](requirements/2026/Q1/O1/K1/P2.md)
- RDF Architecture: [P3 - RDF Triple Store Architecture](requirements/2026/Q1/O1/K1/P3.md)