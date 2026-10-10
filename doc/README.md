# The Watcher — documentation

Diagrams and reference for the design in [`../PLAN.md`](../PLAN.md), which is itself the
design for [`../stock-analyser-requirements.md`](../stock-analyser-requirements.md).

Every diagram is Mermaid in plain Markdown — it renders on GitHub and needs no build step,
which is the same reason the rest of this project avoids tooling it does not need.

| File | What it covers |
|---|---|
| [`architecture.md`](architecture.md) | Crate dependency graph, the deployed stack, how a request reaches the binary |
| [`entities.md`](entities.md) | Entity-relationship diagrams and a per-entity reference |
| [`database.md`](database.md) | Tables, columns, constraints, indexes, the two database roles, migration order |
| [`flows.md`](flows.md) | The nightly job, a hosted run, a connected-agent run, chat, annotation validation |
| [`ledger.md`](ledger.md) | Order lifecycle, fills, corporate actions, snapshots, verdict rules |

## Reading order

Start with `architecture.md` for the shape of the system, then `entities.md` for what is
stored, then `flows.md` for how a day actually passes. `database.md` and `ledger.md` are
reference — read them when writing the migrations and the ledger crate respectively.

## Where the authority lies

When these documents disagree, the order is:

1. `stock-analyser-requirements.md` — what the system must do
2. `PLAN.md` — how it is built and why
3. `doc/` — this folder, which only elaborates the two above

> **Known disagreement:** `PLAN.md` D11 (JSON-only backend) contradicts requirements
> **C-12** and **C-14**, both Musts. These documents follow `PLAN.md`, because that is the
> design being built, but the requirements have not yet been amended to v1.4. Anything in
> `doc/` describing the frontend as a separate deployable inherits that disagreement.
