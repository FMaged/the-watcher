# Architecture

## Crate dependency graph

Dependencies run strictly one way. This is what makes **N-50** ("a change to one area must
not force changes in the others") and **N-54** ("swapping the price source touches no file
that deals with orders, analyses or pages") checkable by reading `Cargo.toml` rather than
by reviewing code.

```mermaid
flowchart TD
    MAIN["src/main.rs<br/>config + wiring, nothing else"] --> API
    MAIN --> STORE
    MAIN --> FEEDS
    API["watcher-api<br/>axum router, JSON handlers,<br/>MCP transport, scheduler"] --> AGENT
    API --> STORE
    API --> LEDGER
    AGENT["watcher-agent<br/>context pack, prompts, tool handlers,<br/>validation, verdicts, run orchestration"] --> STORE
    AGENT --> FEEDS
    AGENT --> LLM
    AGENT --> LEDGER
    STORE["watcher-store<br/>sqlx, migrations, repositories"] --> CORE
    FEEDS["watcher-feeds<br/>PriceSource + NewsSource impls"] --> CORE
    LLM["watcher-llm<br/>ModelProvider impls, batch, token counting"] --> CORE
    LEDGER["watcher-ledger<br/>pure state machine<br/>no sqlx, no tokio, no clock"] --> CORE
    CORE["watcher-core<br/>types, money, trading days,<br/>annotation schema, provider traits"]
```

Nothing depends on `watcher-api`. `watcher-ledger` depends on `watcher-core` alone, which is
what **N-56** means by "the ledger rules are testable without a database".

`src/main.rs` reaches past `watcher-api` to `watcher-store` and `watcher-feeds`, and only
there: it is the composition root, so it is the one file that opens the pool, applies the
migrations and decides which price source is live. The alternative was `watcher-api` naming a
provider itself, which is exactly the edge **N-54** exists to prevent — a handler that knows
which endpoint the prices came from. Everything below `main` is handed what it needs.

### What each crate may not do

| Crate | Forbidden |
|---|---|
| `watcher-core` | any I/O — no `sqlx`, no `reqwest`, no `tokio` |
| `watcher-ledger` | any I/O, and reading the clock. Time arrives as an argument |
| `watcher-store` | HTTP calls to providers. It talks to Postgres only |
| `watcher-api` | holding sqlx. The pool arrives as `watcher_store::Db` and every query is a function in that crate |
| `watcher-feeds` | touching analyses, orders or pages (N-54) |
| `watcher-llm` | deciding anything. It transports requests and returns responses |
| `watcher-agent` | serving HTTP. It knows nothing about axum |

## The deployed stack

One VPS, one Compose file, three services (`PLAN.md` D18).

```mermaid
flowchart LR
    B["Browser"] -->|"HTTPS + basic auth"| C["caddy<br/>auto TLS"]
    CD["Claude Desktop"] -->|"HTTPS + bearer token"| C
    C -->|"GET /"| S["frontend/dist<br/>static Svelte build"]
    C -->|"/api/*"| W["watcher<br/>axum, one Rust binary"]
    C -.->|"/mcp — basic auth EXCLUDED"| W
    W --> P[("postgres 16")]
    W -->|"HTTPS out"| EX["price source<br/>news source<br/>model provider"]
    T["host systemd timer<br/>daily"] -->|"docker exec pg_dump"| P
    T --> BK["/var/backups<br/>30 days retained"]
```

### Why `/mcp` is excluded from basic auth

Claude Desktop authenticates with the **F-91** bearer token, not with basic auth
credentials. If Caddy guarded `/*`, the MCP endpoint would return 401 before the token was
ever read and MCP would simply never work. The exclusion is deliberate; the route is not
unprotected, it is protected by the credential **F-92** can revoke.

```mermaid
flowchart TD
    R["incoming request"] --> M{"path starts with /mcp ?"}
    M -->|"yes"| T{"valid bearer token?"}
    M -->|"no"| BA{"basic auth ok?"}
    T -->|"yes"| H["tool handlers, scoped to that token's user"]
    T -->|"no"| X1["401 — F-91"]
    BA -->|"yes"| AP["static files or /api/*"]
    BA -->|"no"| X2["401 — Caddy"]
```

## Backup and restore

`pg_dump` runs from the **host**, not inside a container, on a systemd timer (**N-20**).
The consequence is that the **N-21** restore drill is rehearsed against the Compose stack
rather than a bare database — more work, and it is the one requirement that cannot be
satisfied by writing code, only by actually carrying it out and writing down what happened.

```mermaid
sequenceDiagram
    autonumber
    participant T as "host systemd timer"
    participant P as "postgres container"
    participant F as "/var/backups"
    T->>P: docker exec pg_dump
    P-->>F: dump written, dated
    T->>F: delete dumps older than 30 days
    Note over T,F: N-20 — nightly, 30 days retained
```
