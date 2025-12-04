### OKRs — Q1 2026

This quarter focuses on validating the core value proposition of FOUNDATION through a concrete use case: **personal finance control**. The goal is to enable non-technical users to create and use a functional financial tracking system with custom categories and simple automation.

**Objective: Enable non-technical users to create and use a functional personal finance tracker**

* **KR1:** 5 out of 5 beta users successfully record 20+ financial transactions with custom categories in < 30 minutes (with no training beyond initial tutorial)

* **KR2:** 4 out of 5 beta users create at least 1 useful automation (e.g., "alert when balance < $100" or "automatically sum monthly expenses")

* **KR3:** 4 out of 5 beta users synchronize their data between 2 devices (e.g., computer + phone) and confirm it "works as expected"

---

### Technical Requirements to Support User Goals

To achieve these user-focused outcomes, the following technical capabilities must be delivered:

1. **Entity and property creation interface** - Users can define custom classes (Transaction, Category) and their properties
2. **Simple data entry forms** - Intuitive forms for creating and editing instances
3. **Basic visualizations** - Lists, totals, and filters for viewing financial data
4. **Simple automation system** - If-then rules for alerts and calculations
5. **P2P synchronization** - Reliable data sync between devices without central server
6. **Immutable database** - Complete history tracking for auditability and undo
7. **Onboarding tutorial** - Guided first-time experience for financial tracking setup