# FOUNDATION

## A SOLID FOUNDATION FOR FREEDOM

FOUNDATION reimagines how anyone — not just developers — can manage, automate, and derive knowledge from their data. Your computer. Your data. Your rules. No Big Tech gatekeepers.

### ONTOLOGY

What if you could receive data from other systems natively, without integrations or mappings? FOUNDATION structures your data through ontology — formal relationships between entities that stay intact as your system evolves. A shared base ontology acts like a common dictionary, enabling different FOUNDATION instances to understand each other's data seamlessly.

### AUTOMATION

What if your data could act on its own? FOUNDATION reacts to every change with real automation. Connect to APIs, orchestrate multi-step processes, and trigger complex workflows without manual intervention. Your data becomes a living, responsive system.

### DISTRIBUTED POWER

What if you could scale without begging cloud providers? Just open FOUNDATION on another computer and synchronize it. FOUNDATION orchestrates data across your devices — your laptop, your friend's server, your spare machine, or yes, even a cloud server if you want one. Each node contributes storage and processing power. Together they act as one powerful system. Distributed by design. Resilient by nature.

### COLLABORATION

What if working together didn't mean conflicts and overwrites? FOUNDATION uses an immutable database, conflict-free by nature. Nothing is altered — new facts are stored, nothing is lost, history is immutable. When information from different sources diverges, it's your choice which source to trust. Every change is traceable, every contributor accountable through full audit trails.

### OWNERSHIP

What if you could own and control your data? FOUNDATION runs locally on your own computer — no big Tech servers required. You own your data, you control your tools, and everything stays under your control.

### LOCAL AI

What if you had intelligent assistance without subscriptions or external services? FOUNDATION includes an efficient local AI that helps you build, analyze, and automate — solving most problems without ever leaving your machine or hiring external services. Private, fast, and always available.

### OPEN SOURCE AND FREE

What if software worked for humanity instead of shareholders? FOUNDATION is open source (GNU GPL) — no corporation owns it, no vendor locks you in, no subscription extracts rent from your work. Built by the community, for everyone. These ideas only benefit society when they're free, shared, and owned collectively by all of us (humans and our robot friends 🤖).

**This is not just software. This is a statement:** Your data is not a commodity. Your computing power is not something to be rented back to you. Your freedom should not require a subscription.

---

## Why we need FOUNDATION?

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

### You Don't Own Your Data (Yet)

Most applications store your data on their servers. You access it through their interface, under their terms. They change pricing. They shut down. They restrict features. Your data — your knowledge, your work, your life — becomes a hostage to business models designed to extract maximum value from you.

**This is not an accident. It's a business model.**

### Wasted Computing Power

You carry a supercomputer in your bag. Multi-core processors, gigabytes of RAM, terabytes of storage. Yet Big Tech wants you to use it as a dumb terminal — sending your data to their servers, processing it in their clouds, paying them for the privilege of using computing power you already own.

**Your machine is powerful. It's time to use it.**

---

## Work Methodology

FOUNDATION follows a problem-driven, outcome-focused development approach:

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

Problems are documented in: `requirements/YYYY/Q[n]/O[n]/K[n]/problem-name.md`

Examples:
- `requirements/2026/Q1/O1/K1/database-selection.md`
- `requirements/2026/Q1/O1/K1/base-ontology-selection.md`
- `requirements/2026/Q1/O1/K1/semantic-data-structure.md`

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

### How to Contribute

FOUNDATION is built by people who believe technology should serve humanity, not corporations. If you share this vision, you're already part of the team.

**Working on Problems:**

1. **Create a branch** - Name it `username/problem-name` (e.g., `alice/local-ai-integration`)
2. **Develop** - Work on your solution, experiment, and iterate
3. **Document results** - Update the problem document with your findings:
   - Mark solution as `[✅ CURRENT]`, `[❌ REJECTED]`, or `[⏸️ PAUSED]`
   - Add what worked, what didn't, and why
   - Include performance metrics, trade-offs discovered
4. **Submit PR** - Share your work, even if incomplete

**Important:**
- **Failed experiments are valuable.** Document what you tried and learned — this helps everyone avoid the same pitfalls and builds collective knowledge.
- **Keep all communication in English** (code, comments, issues, PRs, documentation) so anyone in the world can contribute.
- **Be kind.** We're building something bigger than ourselves.

---

## Contributors

**Your name could be here!** 👋

We're just getting started. This is your chance to be part of something from the ground up — something that matters. Whether you write code, design interfaces, test features, write documentation, or simply believe in the mission — you belong here.

Every line of code, every bug report, every idea brings us closer to a world where people own their data and control their tools.

**Join us.**

---

<div align="center">

**Conceived by [Daniel Terra](https://github.com/danielterra) in 🇧🇷**

*Built with ❤️ by people who believe your data should belong to you*

*For a future where technology serves humanity, not the other way around*

</div>