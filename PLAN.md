# The Watcher — technical plan

Version 1.0 · 2026-10-10 · Design for `stock-analyser-requirements.md` v1.3

This document records **how** the system is built and **why** each choice was made.
What it must do lives in the requirements. Task breakdown lives in `TASKS.md`.

> **Open conflict:** the JSON-only decision (D11) contradicts requirements **C-12** and
> **C-14**, both Musts. The requirements doc needs a v1.4 amendment to match this plan.
> Until that lands, this plan and the requirements disagree, and the requirements win.

---

## Goal

A workspace for one person following ten to thirty US-listed stocks. An agent analyses
the watchlist daily, marks the chart with where it would buy and sell, explains the news,
answers questions, and trades on paper so every call it makes can be scored against the
market later. The user acts in their own broker app; this system never touches money.

The product is the visible reasoning and the honest scoreboard. The agent beating the
market is explicitly **not** a goal.

## MVP Features

- Watchlist of up to 50 tickers
- One agent run per trading day per watchlist, plus manual runs
- Immutable analyses: summary, structured annotations, news takes
- Chart data with annotations and paper-trade markers
- Chat grounded in stored analysis, never a fresh run
- Paper portfolio: 10,000 USD, fills at next open, splits, dividends
- Track record: equity curve against a frozen benchmark, decision log, verdict per call
- Two agent paths: the user's own assistant over MCP, or our hosted runner
- Daily snapshots, health endpoint, nightly backup

## Later / Out of Scope

**Requirements already marked Later:** chat history per ticker (F-52), delisting (F-72).

**Out of scope for v1 (requirements §10):** real order execution, suitability profiling,
public leaderboards, copy trading, crypto and derivatives, non-US markets, intraday data,
mobile apps, broker import, tax reporting, multi-currency, separate services, payments.

**Deferred technical work:** password login and multi-user (needed before the brief's ten
invited users), rate limiting per token (N-45), account deletion (D-10), alerting (N-62).

---

## Stack

| Part | Choice |
|------|--------|
| Language | Rust (stable), 2021 edition |
| HTTP | axum + tokio |
| Database | PostgreSQL 16 |
| DB access | sqlx, compile-time checked queries, versioned migrations |
| Frontend | Svelte 5 + Vite + TypeScript |
| Charts | **Frontend's decision.** Backend owns the JSON contract only |
| Reverse proxy | Caddy — automatic TLS, basic auth, serves the frontend build |
| Deployment | Docker Compose on one small VPS: `watcher`, `postgres`, `caddy` |
| Price data | One trait, free source configured, paid source written and idle |
| Model access | One trait, raw HTTP (no official Rust SDK), two implementations |
| Backups | Host systemd timer running `pg_dump` against the container |

---

## Structure

### Workspace layout

```
the-watcher/
├── Cargo.toml                 workspace
├── src/main.rs                config + wiring, nothing else
├── crates/
│   ├── watcher-core/          types, money, trading days, annotation schema,
│   │                          provider traits. No I/O. No sqlx. No reqwest.
│   ├── watcher-store/         sqlx, migrations, repositories
│   ├── watcher-feeds/         PriceSource + NewsSource implementations
│   ├── watcher-llm/           ModelProvider implementations, batch, token counting
│   ├── watcher-ledger/        paper trading rules. Pure. Depends on core alone.
│   ├── watcher-agent/         context pack, prompts, tool handlers, validation,
│   │                          verdict rules, run orchestration, nightly job logic
│   └── watcher-api/           axum router, JSON handlers, MCP transport, scheduler
├── migrations/                NNNN_name.sql, forward only
├── frontend/                  Svelte + Vite, builds to frontend/dist
├── deploy/
│   ├── compose.yaml
│   ├── Caddyfile
│   └── backup.{service,timer}
└── PLAN.md · TASKS.md · stock-analyser-requirements.md
```

Dependency direction is strictly one-way. `watcher-core` depends on nothing of ours;
`watcher-ledger` depends only on `watcher-core`; `watcher-api` depends on everything.
Nothing depends on `watcher-api`. This is what makes N-50 and N-54 checkable by reading
`Cargo.toml` rather than by reviewing code.

### Entities

| Entity | Purpose | Mutable? |
|---|---|---|
| `user` | owner of a watchlist and portfolio. One row in v1 | yes |
| `api_token` | hashed MCP token (N-41) | yes |
| `instrument` | validated tickers (F-1) | yes |
| `watchlist_item` | ticker on a watchlist, with removal date (F-3) | yes |
| `trading_calendar` | US sessions and half-days, seeded 2026–2030 (D-15) | seed only |
| `price_bar` | raw daily OHLCV. Never adjusted (I-11) | insert only |
| `corporate_action` | splits and dividends with pay dates | insert only |
| `news_item` | headline, source, timestamp, URL. No article text (I-21) | insert only |
| `news_take` | the agent's one line per item (F-41) | **append only** |
| `run` | one pass: trigger, times, outcome, model (F-12) | yes (outcome) |
| `analysis` | summary + cutoff + version per ticker per day | **append only** |
| `annotation` | one structured mark, with reason and flag | **append only** |
| `raw_model_output` | the model's unparsed response per run and ticker (D-14) | **append only** |
| `call_verdict` | verdict about one annotation or order, as of a day (F-84) | **append only** |
| `portfolio` | cash in minor units, created date | yes (cash) |
| `position` | shares held per ticker | yes |
| `order` | paper order with run reference, state | yes (state) |
| `ledger_entry` | every cash and share movement, with natural key | **append only** |
| `benchmark_basket` | frozen share counts at portfolio creation (F-82) | insert only |
| `snapshot` | daily cash, positions, equity (F-80) | insert only |
| `chat_message` | question and answer per ticker | insert only |
| `model_call` | tokens in and out, cost, latency (F-95, N-61) | insert only |

### JSON API

Everything under `/api` is JSON. No endpoint returns HTML.

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/watchlist` | tickers with last analysis date |
| POST | `/api/watchlist` | add a ticker (F-1, F-2) |
| DELETE | `/api/watchlist/{ticker}` | remove, keeping history (F-3) |
| GET | `/api/ticker/{t}` | newest analysis, annotations, news takes (F-15) |
| GET | `/api/ticker/{t}/chart?range=1y` | **frozen contract.** Bars, annotations, trades |
| GET | `/api/ticker/{t}/analyses` | earlier versions and past days (F-16) |
| GET | `/api/analysis/{id}` | one analysis, as it was written |
| POST | `/api/ticker/{t}/chat` | grounded question (F-50, F-51, F-53) |
| POST | `/api/runs` | manual run, one ticker or all (F-11) |
| GET | `/api/runs/{id}` | run record including failures (F-12, F-13) |
| GET | `/api/portfolio` | cash, positions, pending orders (F-69) |
| GET | `/api/track-record` | equity curve and benchmark (F-81, F-82) |
| GET | `/api/decision-log?ticker=` | every call with reason and verdict (F-83, F-84) |
| GET/POST/DELETE | `/api/tokens` | MCP token management (F-92) |
| POST | `/api/admin/nightly?date=` | operator rerun, idempotent (F-100, F-101) |
| GET | `/api/export` | the user's own data as JSON (D-9) |
| GET | `/health` | database, last nightly, last price fetch (F-102) |
| — | `/mcp` | MCP transport. **Excluded from basic auth** |

### The chart contract

Frozen at M2 and versioned thereafter, because the frontend is built against it.

```json
{
  "ticker": "AAPL",
  "range": "1y",
  "price_scale": 100,
  "bars": [{ "d": "2026-10-09", "o": 24150, "h": 24390, "l": 24005, "c": 24310, "v": 48201100 }],
  "annotations": [{
    "id": "an_01J...", "kind": "support", "price": 23800,
    "low": null, "high": null, "date": null,
    "reason": "Held three times since July.",
    "made_on": "2026-10-09", "flagged": false,
    "verdict": { "status": "provisional", "text": "Held so far, 7 of 30 days." }
  }],
  "trades": [{ "d": "2026-10-07", "side": "buy", "qty": 12, "price": 24055 }]
}
```

All money is integer minor units with an explicit `price_scale`, so no float crosses the
wire (D-3). `kind` is one of the six allowed values; `low`/`high` are set for zones and
`date` for `breakout` and `event`, null otherwise.

---

## Decisions

### D1: Seven crates, one binary
- **Choice:** `core`, `store`, `feeds`, `llm`, `ledger`, `agent`, `api`, wired by `src/main.rs`.
- **Why:** C-15 demands boundaries between analysis, orders and web. Giving feeds and the
  model client their own crates makes N-54 ("swapping the price source touches no file that
  deals with orders, analyses or pages") a fact about the dependency graph rather than a
  claim to re-check at review time.
- **Alternatives:** five crates with feeds and the model client folded into `store` — fewer
  files, but N-54 becomes a convention. One crate with modules — violates C-15.

### D2: The ledger is a pure state machine
- **Choice:** `decide(state, command) -> Result<Vec<Event>, Rejection>` and
  `apply(state, event)`. `watcher-ledger` has no sqlx, no tokio, no clock of its own.
- **Why:** N-56 requires the ledger to be testable with no Postgres present, and N-53's six
  cases become ordinary unit tests. Append-only storage (D-2) stops being a rule to remember
  and becomes the only shape the crate can express.
- **Alternatives:** repository methods that read, decide and write together. Faster to write,
  but then every rule test needs a database and D-2 is enforced by discipline.

### D3: One tool implementation, two transports
- **Choice:** the six I-1 tools are implemented once in `watcher-agent`. The MCP server is a
  transport adapter; the hosted runner calls the same handlers in-process.
- **Why:** F-96 requires parity of writes and validation. One implementation makes that
  structural — there is no second code path that could drift. It also means the whole agent
  surface is testable without a model, by calling the handlers directly.
- **Alternatives:** separate handlers per mode, which is how parity quietly breaks.

### D4: Hosted runs are single-shot and go through the batch route
- **Choice:** one context pack in, one response out, submitted to the provider's
  asynchronous batch endpoint. Chat and manual runs stay synchronous.
- **Why:** F-97 and I-33 are the same decision from two sides. Nobody waits on a job that
  starts after the US close, the batch discount is large, and batch routes suit single-shot
  work far better than multi-turn loops. F-97 also obliges us to say plainly that a connected
  agent, which can explore over several calls, is *more* capable than ours.
- **Alternatives:** a synchronous multi-turn loop for hosted runs — more capable, forfeits
  the discount, and makes the hosted path diverge from what we tell users it is.

### D5: One context pack assembler, budget measured against the request
- **Choice:** one component in `watcher-agent` builds the material for both the daily run and
  chat. It measures the stable prefix with the provider's token-counting endpoint, caches that
  figure per model and schema version, and caps the pack at
  `min(90_000, 100_000 - prefix)`. What it drops, it records.
- **Why:** F-18 exists so chat and the daily analysis cannot contradict each other by having
  seen different data. F-19 and N-33 are about cost, not capability: past 100,000 input tokens
  the price per token multiplies on the standard tier, so the budget belongs to the request,
  not to the pack in isolation.
- **Alternatives:** two builders, which drift, and the drift surfaces as chat disagreeing
  with the page the user is looking at.

### D6: One annotation schema definition
- **Choice:** the enum lives in `watcher-core`. The JSON Schema handed to the model is derived
  from it, and the validator checks against the same derivation.
- **Why:** N-55. The model's contract and our validator cannot disagree if neither is written
  by hand. F-21's "unknown kind is rejected" then comes free as a deserialization error.
- **Alternatives:** a hand-written schema string beside a hand-written validator — two places
  to change, and nothing that notices when only one of them changes.

### D7: Immutability lives in the schema, not in review
- **Choice:** `trading_day` is derived at write time and never accepted as input. The
  application connects as `watcher_app`, which holds no UPDATE or DELETE on `analysis`,
  `annotation` or `ledger_entry`. Migrations run as a separate owner role. Two connection
  strings in config.
- **Why:** N-1 is the product. A promise that the record is never improved after the fact is
  worth less than a database that cannot do it. D-11 and D-12 asked for exactly this, and §11
  now tests it: the app role is *proven* unable to update an analysis.
- **Alternatives:** one role plus code review. Cheaper, and the acceptance test becomes
  unprovable.
- **Note:** one narrow exception, per D-11 — a corporate-action ledger entry takes its day
  from the corporate action record, because a dividend is credited on its pay date and may be
  processed later. Analyses, orders and fills take the clock.

### D8: Idempotency by natural key
- **Choice:** every ledger entry carries `(portfolio_id, trading_day, kind, subject_id)` with
  a unique index. The nightly job writes with `ON CONFLICT DO NOTHING`.
- **Why:** F-101 and D-13. Running the job twice is then a no-op because the schema says so,
  not because the job remembered to check. F-100's operator rerun needs this to be safe.
- **Alternatives:** a "processed days" table the job consults — one more thing to get right,
  and it fails open.

### D9: Orders reference their run, not an analysis id
- **Choice:** `place_paper_order` takes a run id. The server resolves it to the analysis for
  that run and ticker, which is unique because one run produces one analysis per ticker.
- **Why:** a single-shot agent (D4) emits its analysis and its order in the same turn and
  cannot cite an id the server has not assigned yet. Anchoring to the run keeps F-62's intent
  — every order carries a reason — while letting both agent paths use one tool (F-96).
- **Alternatives:** an analysis id, which hosted mode cannot produce; or orders nested inside
  `record_analysis`, which gives the two modes different tools.

### D10: Verdicts append, and may be provisional
- **Choice:** `call_verdict` rows keyed `(subject, as_of_trading_day)`, latest shown. Computed
  by the nightly job. Rules per kind:

| Kind | Verdict rule |
|---|---|
| `support` | broke if any close falls below the level within 30 trading days |
| `resistance` | broke if any close rises above the level within 30 trading days |
| `entry_zone` | touched if any bar's range overlaps the zone; then the close 30 days after first touch |
| `stop` | hit if any low reaches the level within 30 trading days |
| `breakout` | confirmed if two consecutive closes sit beyond the level within 10 trading days |
| `event` | **no verdict.** A marker is not a claim, and pretending otherwise is dishonest |
| order | return since fill at 7, 30 and 90 trading days, beside the benchmark over the same window |

- **Why:** F-84 is the brief's central bet made testable. Appending rather than updating keeps
  D-12 intact — the annotation stays immutable and the app role could not edit it anyway — and
  gives something better than a verdict appearing finished on day 30: the user watches it firm
  up. `event` having no verdict is deliberate.
- **Alternatives:** a verdict column on `annotation`, which needs UPDATE and so contradicts
  D-12 and the §11 acceptance test.

### D11: JSON-only backend; the frontend owns every page
- **Choice:** the backend serves JSON and nothing else. The Svelte app owns pages, routing and
  its own build.
- **Why:** chosen by the project owner. It gives the frontend full freedom over rendering and
  over the chart library, and keeps the backend's obligation to one thing: a contract.
- **Alternatives:** server-rendered pages with a chart JSON endpoint, as C-14 currently
  requires — one deployable, no build pipeline, less frontend freedom.
- **Cost, stated plainly:** this amends **C-12** and **C-14**, both Musts, and adds a second
  deployable to a single host. Those amendments are not yet written.

### D12: Svelte 5 + Vite + TypeScript
- **Choice:** Svelte, five pages, `fetch` against `/api`.
- **Why:** the app is almost entirely "fetch JSON and draw it", which is where Svelte needs
  the least ceremony. Small bundle, no virtual DOM to reason about.
- **Alternatives:** React, with a larger ecosystem and more people able to pick it up; vanilla
  TypeScript, with the smallest dependency surface and hand-rolled routing.
- **Rule:** model output is never passed to Svelte's `{@html}`. That construct is the specific
  way N-44 gets violated in this stack, so it is greppable in review.

### D13: Chart rendering is the frontend's decision
- **Choice:** the backend defines and freezes the chart JSON contract. Which library draws it
  is not decided here.
- **Why:** the owner's call, and the right boundary: the contract is the thing both sides must
  agree on, and the drawing is not the backend's business. I-40 already wanted the data
  separate from its display.
- **Alternatives:** picking a library now, which would bind the frontend to a licence question
  it has not looked at yet.

### D14: No login in v1; Caddy guards everything except MCP
- **Choice:** one seeded `user` row. Caddy applies basic auth over the whole site, serves the
  frontend build and proxies `/api`. The MCP route is **excluded** and relies on the F-91
  bearer token.
- **Why:** §11's acceptance test is one person's own watchlist, and D-1 anticipated records
  being attributed before any login exists. Basic auth is one line of config, protects the
  metered chat endpoint from strangers, and is deleted the day real login arrives. Serving
  frontend and API from one origin means no CORS and no token in JavaScript.
- **Alternatives:** password login now — real work before the 14-day run can even start, for
  exactly one user. A bearer token shipped to the browser — readable by anyone who loads the
  page. Nothing at all — an open, metered LLM endpoint on the public internet.
- **Trap worth naming:** if Caddy guards `/*`, Claude Desktop gets a 401 before its token is
  read, and MCP never works. The exclusion is deliberate, not an oversight.
- **N-43 stays testable** without login: insert a second user row and assert every query
  scopes by owner.

### D15: One price trait, free source live, paid source written and idle
- **Choice:** `trait PriceSource` in `watcher-core`. The free implementation is configured; a
  paid implementation is written in M0 and left unconfigured. Swapping is one environment
  variable.
- **Why:** A-1 and the brief's own risk entry. §11 needs 14 *consecutive* trading days, so a
  provider breaking on day nine means starting the count again. Having the replacement already
  written turns that from a rebuild into a restart.
- **Alternatives:** paid from day one — no breakage risk, a monthly cost before any revenue.
  Free only — least code, and the acceptance test is hostage to an unofficial endpoint.
- **Per I-12:** a failure retries, then records a failed fetch. It never writes a guessed bar.

### D16: The trading calendar is data, seeded by migration
- **Choice:** a `trading_calendar` table covering 2026–2030, including half-days, inserted by
  a migration.
- **Why:** D-15. N-3 ("a run on Thanksgiving completes and records no fills") and D-11's
  derived trading day both need a calendar that tests can rely on and that does not change
  under them at runtime.
- **Alternatives:** computing holidays at runtime, which is more code and harder to test than
  a few hundred seeded rows.

### D17: One model trait, raw HTTP, tool choice never forced
- **Choice:** `trait ModelProvider` with two implementations over `reqwest` — one for the
  Anthropic API, one for OpenAI-compatible endpoints. Provider, model, base URL and key come
  from config (I-31). Tool choice is always automatic, with the instruction naming the tool,
  and provider-side strict schema validation is switched on where available.
- **Why:** there is no official Anthropic SDK for Rust, so raw HTTP is the honest option and
  it suits I-30. Forced tool choice is rejected outright by several current models, and which
  ones is not something a config change (I-31) should be able to break — so we never rely on
  it. Strict validation means arguments that do not match the D6 schema never reach our
  validator, which then only checks meaning, not shape.
- **Alternatives:** forcing the tool call where supported, which couples the run to the model
  the config happens to name.

### D18: Docker Compose on one VPS
- **Choice:** three services — `watcher` (the Rust binary), `postgres`, `caddy`. The frontend
  is built in a stage and its output served by Caddy as static files.
- **Why:** chosen by the project owner, and D11 justifies it better than my framing did: with
  a second deployable, compose stops being overhead and starts doing real work — one file
  describes what runs, and the frontend build is reproducible.
- **Alternatives:** a pushed binary under systemd with Caddy on the host, which is what C-10
  and C-13 ("one binary, one host") point at; building on the VPS, where a release build can
  exhaust a small box's memory.
- **Consequence:** `pg_dump` for N-20 runs from the *host*, on a systemd timer, against the
  container. The N-21 restore drill is therefore rehearsed against the compose stack, not a
  bare database — slightly more work, and it is the one requirement that can only be met by
  actually doing it.

### D19: Tests
- **Choice:**

| Layer | How |
|---|---|
| `watcher-ledger` | pure unit tests, no database. The six N-53 cases |
| `watcher-store` | `#[sqlx::test]`, each test in a transaction that rolls back |
| D-12 role rule | its own test: connect as `watcher_app`, assert UPDATE on `analysis` is denied |
| F-101 idempotency | run the nightly job twice for one date, assert the second changes nothing |
| Annotation validation | table-driven over the D6 schema, including the F-23 flag case |
| Tool handlers | called directly, with a fake `ModelProvider`. No network, no model |
| Verdict rules | fixed price series in, expected verdicts out |

- **Why:** the database-free parts are where the rules live, so they should be the fastest
  tests to run. Transaction-per-test needs no Docker and leaves nothing behind. The role rule
  cannot run inside a transaction that rolls back, so it is deliberately separate.
- **Alternatives:** testcontainers, identical everywhere but makes Docker a test dependency;
  a template database cloned per test, which tests DDL and roles for real but is slower.

### D20: Money and shares are integers
- **Choice:** `i64` minor units for cash and prices, `i32` whole shares. No float anywhere in
  storage, in the ledger, or on the wire — the chart contract carries `price_scale`.
- **Why:** D-3, and F-67 (whole shares only). A float cent error in an append-only ledger
  cannot be corrected, only reversed.
- **Alternatives:** `rust_decimal`, which is correct but heavier than this problem needs.

### D21: The benchmark basket is frozen at creation
- **Choice:** `benchmark_basket` stores the tickers and share counts bought at the next open
  after portfolio creation. The benchmark curve is a daily valuation of those fixed rows.
- **Why:** F-82 says tickers added later do not join, and its acceptance test is that adding a
  ticker in week three does not change the benchmark line in week one. Deriving the basket
  from the current watchlist at read time would silently rewrite history — the one thing this
  product exists not to do.
- **Alternatives:** computing it on each read from the watchlist as it stands, which fails
  that test in a way nobody notices until the chart is wrong.

---

## Milestones

Sized against **O-3**, which is still open and decides whether this is a plan or a wish.
The 14 consecutive trading days in §11 cannot be compressed, so the sequence exists to start
that clock as early as possible.

| # | Name | Contents |
|---|---|---|
| **M0** | The spine that accrues | Workspace, compose stack, migrations, two DB roles, instrument list, trading calendar, price ingest with both implementations, idempotent nightly job, snapshots, health endpoint. **No AI at all.** Point it at ten tickers and leave it running — price history costs nothing to accumulate and you will want it. |
| **M1** | The agent writes, we store | Model trait, context pack assembler and its token measurement, annotation schema and validator, the six tool handlers, MCP transport, hosted runner, run records and partial failures, raw payload storage, a plain decision-log endpoint. **The 14-day clock starts here.** |
| **M2** | The contract and the chart | Chart JSON endpoint, frozen contract, Svelte app with watchlist and ticker pages, annotations drawn with reasons, Caddy basic auth. |
| **M3** | The ledger | Orders, fills at next open with costs, re-validation at fill, expiry, splits, dividends, frozen benchmark basket, equity curve, portfolio page. |
| **M4** | News, chat and verdicts | News source and takes, grounded chat with its cap, verdict computation in the nightly job, track-record and decision-log pages. |
| **M5** | Ops and proof | Backup timer, the N-21 restore drill carried out and written down, per-user cost accounting, the §11 checklist walked end to end on the deployed instance. |

M1 before M2 is deliberate: an ugly endpoint that accrues a real record beats a beautiful
page with nothing behind it. Everything in M2 onward can be built while the clock runs.

---

## Configuration

All secrets from the environment, never in the repository, the browser or the logs (N-40).

| Variable | Purpose |
|---|---|
| `DATABASE_URL` | as `watcher_app` — no UPDATE or DELETE on the append-only tables |
| `MIGRATE_DATABASE_URL` | as the owner role, used only by migrations |
| `MODEL_PROVIDER`, `MODEL_NAME`, `MODEL_BASE_URL`, `MODEL_API_KEY` | I-31 |
| `PRICE_SOURCE`, `NEWS_SOURCE` | which implementation is live (D15) |
| `DAILY_COST_CAP_CENTS` | N-30, checked *before* a run starts (N-34) |
| `CHAT_CAP_PER_DAY` | F-53 |
| `HALF_SPREAD_BPS=5`, `FEE_CENTS=100` | C-19 |
| `ANNOTATION_WARN_PCT=25` | C-20, a starting value chosen to be revised |

---

## Open

| # | Question | Blocks |
|---|---|---|
| **P-1** | The N-10 split. The backend's chart-endpoint latency budget is **not set** — it will be measured on real data in M2 and recorded then, rather than guessed now. | N-10 amendment |
| **O-3** | Hours per week. The only one nobody else can answer. | milestone sizing |
| **O-5** | News provider. | I-20, M4 |
| **O-10** | Measured tokens per ticker-day, via the provider's token-counting endpoint over a real pack, on the day M1's assembler first works. | N-33, F-19, cost model |
| — | The v1.4 requirements amendment that makes C-12 and C-14 agree with D11. | consistency |
