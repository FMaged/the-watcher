# The Watcher: Tasks

Built from [`PLAN.md`](PLAN.md). Stages follow its milestones M0–M5. Schema detail, flows
and diagrams to build against are in [`doc/`](doc/README.md).

**The ordering has one hard constraint.** §11 of the requirements needs 14 *consecutive*
trading days of unattended running. That clock cannot be compressed later, so Stage 1 and
Stage 2 exist to start it as early as possible. Everything from Stage 3 on is built while
it runs.

---

## Stage 1: The spine that accrues (M0)

Ends with: a deployed instance ingesting daily price bars and writing snapshots, unattended.
**No AI in this stage at all.**

### [x] T1.1: Amend the requirements to v1.4
**What:** Fix the conflicts `PLAN.md` D11 created — C-12 withdrawn, C-14 rewritten for a
JSON-only backend, C-15 clarified, two new constraints for the Compose stack and basic auth,
I-40 rewritten, I-41 moved to frontend scope, N-10 split into an API budget and a frontend
budget.
**Why:** `PLAN.md` and `doc/` currently contradict two Musts. The requirements doc has been
the authority all along; leaving it wrong makes every later reference ambiguous.
**How:** Edit `stock-analyser-requirements.md`, bump to v1.4, add the change-log row. Leave
N-10's backend number as a hole — it gets measured in T3.3.
**Notes:** Done. Two things went beyond the task as written. **N-13 added**: splitting N-10
needed a second id, because the frontend's render budget had nowhere to live and one number
across two deployables lets each side blame the other. **O-11 opened** for N-10's missing
figure — `PLAN.md` had been tracking it as its own `P-1`, which was a second numbering scheme
for the same hole, so `P-1` is gone and the requirements doc owns it. Also updated the
downstream docs that had been announcing the conflict this task resolved: the notice at the
top of `PLAN.md`, D11's cost paragraph, its Open table, and the warning in
`doc/README.md`. C-12 is marked **Withdrawn** with its id kept, per §1.4.

### [x] T1.2: Repository and workspace skeleton
**What:** `git init`, Cargo workspace, seven crate stubs, `src/main.rs`.
**Why:** C-15. The dependency direction is what makes N-50 and N-54 checkable by reading
`Cargo.toml` instead of reviewing code.
**How:** `cargo new --lib` per crate under `crates/`, workspace `Cargo.toml`, `.gitignore`.
Add each crate's dependencies in the one-way order from `doc/architecture.md` and no others.
**Notes:** Done. `cargo build`, `clippy -D warnings`, `fmt --check` and `test` all clean, and
the internal dependency graph was asserted against `doc/architecture.md` package by package —
it matches, and nothing depends on `watcher-api` but the binary.

Three judgement calls. **No external dependencies yet**: "and no others" was read strictly, so
no sqlx, axum, reqwest or tokio is present. Each crate that needs one picks it up in the task
that needs it, and every `lib.rs` carries a doc comment naming what that crate may *not*
depend on, so the rule travels with the code rather than living only in `doc/`. **Edition
2021**, per `PLAN.md` — I had written 2024 first, which the toolchain supports and which is
the usual choice for a new project; reverted to match the plan rather than change it quietly.
Worth a decision either way (see the question below). **No boundary-check script**: the graph
was verified by hand this once and `rust-review` documents the two `cargo tree -i` commands.
A script asserting it in CI would stop the boundary rotting silently, which is the failure
this task exists to prevent — not added, because it is beyond what the task asked for.

### [ ] T1.3: Compose stack and configuration
**What:** `deploy/compose.yaml` with `watcher`, `postgres`, `caddy`. A `Config` struct read
from the environment, failing fast on anything missing.
**Why:** `PLAN.md` D18. N-40 — secrets come from the environment and never appear in the
repository, the browser or the logs.
**How:** Compose file, a plain Caddyfile proxying `/api` (basic auth comes in T3.7), config
module covering the variables in `PLAN.md` § Configuration.
**Notes:** _(added after implementation)_

### [ ] T1.4: Database roles, migration 0001
**What:** `watcher_owner` and `watcher_app`, two connection strings.
**Why:** `PLAN.md` D7. This is the mechanism behind N-1 — a database that *cannot* rewrite
history beats a promise that it will not.
**How:** `0001_roles.sql` per `doc/database.md`. `MIGRATE_DATABASE_URL` for migrations,
`DATABASE_URL` for the app.
**Notes:** _(added after implementation)_

### [ ] T1.5: Core types
**What:** `Money` (i64 minor units), `TradingDay`, `Ticker`, `Side` in `watcher-core`.
**Why:** D-3 and `PLAN.md` D20. A float cent error in an append-only ledger cannot be
corrected, only reversed.
**How:** No I/O in this crate. `Money` has no `f64` conversion at all, in either direction.
**Notes:** _(added after implementation)_

### [ ] T1.6: Migrations 0002 and 0003 — core tables and the trading calendar
**What:** `app_user`, `api_token`, `instrument`, `watchlist_item`, `portfolio`; then
`trading_calendar` seeded with US sessions and half-days for 2026–2030.
**Why:** `PLAN.md` D16, D-15. N-3 and the derived trading day both need a calendar that tests
can rely on and that cannot shift under them at runtime.
**How:** Note the reserved-word renames in `doc/entities.md` (`user` → `app_user`). Portfolio
starts at exactly 10,000 USD in minor units (F-60). The seed is data, not fixtures.
**Notes:** _(added after implementation)_

### [ ] T1.7: Migration 0004 — market data tables
**What:** `price_bar`, `corporate_action`, `news_item`.
**Why:** I-11 — raw prices are stored, adjusted prices are never the source of truth because
they change retroactively.
**How:** Natural primary key `(instrument_id, bar_date)`. No adjusted-price column exists.
**Notes:** _(added after implementation)_

### [ ] T1.8: PriceSource trait and the free implementation
**What:** The trait in `watcher-core`, a free-source implementation in `watcher-feeds`.
**Why:** A-1, `PLAN.md` D15.
**How:** Retry, then record a failed fetch (I-12). It must never write a partial or guessed
bar. `watcher-feeds` touches nothing to do with orders, analyses or pages (N-54).
**Notes:** _(added after implementation)_

### [ ] T1.9: Paid PriceSource implementation, left unconfigured
**What:** A second implementation behind the same trait, selected by `PRICE_SOURCE`.
**Why:** `PLAN.md` D15. §11 needs 14 *consecutive* days; a free endpoint breaking on day
nine means starting the count again. Written now, the swap is one environment variable.
**How:** Same trait, no other code aware of which is live. Do not configure it.
**Notes:** _(added after implementation)_

### [ ] T1.10: Watchlist endpoints
**What:** `GET`/`POST`/`DELETE /api/watchlist`.
**Why:** F-1, F-2, F-3.
**How:** Validate against the instrument list, refuse the 51st ticker with a reason, and make
removal set `removed_on` rather than delete — F-3 keeps the history of a removed ticker.
**Notes:** _(added after implementation)_

### [ ] T1.11: Nightly job — calendar check, price ingest, snapshot
**What:** Scheduler in `watcher-api`, job logic in `watcher-agent`. Holiday check, bar
ingest, one snapshot per portfolio per trading day.
**Why:** F-10, F-80, F-101, N-22, N-3.
**How:** Every write `ON CONFLICT DO NOTHING` against the natural key. Job logic takes the
date as an argument so it is testable without the scheduler. A holiday records a `skipped`
run, not a failure.
**Notes:** _(added after implementation)_

### [ ] T1.12: Operator trigger and health endpoint
**What:** `POST /api/admin/nightly?date=`, `GET /health`.
**Why:** F-100, F-102.
**How:** Health reports database reachability, last successful nightly job, last successful
price fetch. Re-running a processed date must change nothing.
**Notes:** _(added after implementation)_

### [ ] T1.13: Test harness and the D-12 role test
**What:** `#[sqlx::test]` transaction-per-test for `watcher-store`, plus a separate test that
connects as `watcher_app` and asserts `UPDATE` on `analysis` is refused.
**Why:** `PLAN.md` D19, and the §11 box "the application database role is proven unable to
update or delete an analysis".
**How:** The role test cannot live inside a rolled-back transaction, so it is its own case.
**Notes:** _(added after implementation)_

### [ ] T1.14: Deploy and start accruing
**What:** Push to the VPS, `compose up`, verify two nights of bars and snapshots.
**Why:** Price history costs nothing to accumulate and you will want it. Running the stack
for real now surfaces the deployment problems before they can block the 14-day clock.
**How:** Confirm health is green, two consecutive nights produced bars, and a weekend passed
without a failed run.
**Notes:** _(added after implementation)_

---

## Stage 2: The agent writes, we store (M1)

Ends with: **the 14-day clock started.** An ugly endpoint that accrues a real record beats a
beautiful page with nothing behind it.

### [ ] T2.1: Annotation schema, one definition
**What:** The six-kind enum in `watcher-core`, with the model's JSON Schema and the
validator's schema both derived from it.
**Why:** N-55 and `PLAN.md` D6. Neither is written by hand, so they cannot drift.
**How:** Changing an allowed kind in one place must change both. An unknown kind becomes a
deserialization error before any validation runs (F-21).
**Notes:** _(added after implementation)_

### [ ] T2.2: Annotation validation — accept, flag, reject
**What:** Three outcomes, per `doc/flows.md`.
**Why:** F-22, F-23, C-20.
**How:** Reject a non-positive price, a zone with low above high, a missing date on
`breakout`/`event`, a future date, a missing reason. A price far from the last close is
**stored and flagged**, never rejected, and the flag is counted so the threshold can be tuned.
**Notes:** _(added after implementation)_

### [ ] T2.3: Migration 0005 — runs, analyses, annotations, raw output, and the trigger
**What:** `run`, `analysis`, `annotation`, `raw_model_output`, `news_take`, plus the
`set_trading_day` BEFORE INSERT trigger.
**Why:** F-14, D-11, D-14.
**How:** A `CHECK` cannot do this — `AT TIME ZONE` with a named zone is not `IMMUTABLE` and
Postgres rejects it. Use the trigger from `doc/database.md`, which overwrites whatever was
passed. `UNIQUE (instrument_id, trading_day, version)`.
**Notes:** _(added after implementation)_

### [ ] T2.4: ModelProvider trait and the Anthropic implementation
**What:** The trait in `watcher-core`, a raw-HTTP implementation in `watcher-llm`.
**Why:** I-30, I-32, `PLAN.md` D17. There is no official Anthropic SDK for Rust.
**How:** Timeout and retry limit — a hanging provider must not hang a run. Tool choice is
always automatic with the instrument named in the instruction, never forced, with strict
schema validation on where available (I-35).
**Notes:** _(added after implementation)_

### [ ] T2.5: OpenAI-compatible ModelProvider implementation
**What:** A second implementation behind the same trait.
**Why:** F-94, I-30. The same run must complete against a local model and a hosted one.
**How:** Provider, model name, base URL and key from configuration only (I-31). Switching
needs a config change and a restart, no code change (F-93).
**Notes:** _(added after implementation)_

### [ ] T2.6: Context pack assembler and token measurement
**What:** One component building the material for both the daily run and chat. Measures the
stable prefix, caps the pack at `min(90_000, 100_000 − prefix)`, records what it dropped.
**Why:** F-18, F-19, N-33. Past 100,000 input tokens the price per token multiplies on the
standard tier, so the budget belongs to the request, not the pack alone.
**How:** Use the provider's token-counting endpoint; cache the prefix figure per model and
schema version. **This closes O-10** — record the measured tokens per ticker-day the first
time it runs on a real pack, and check whether the upper-tier model has its own long-context
pricing before applying N-33's reasoning to both tiers.
**Notes:** _(added after implementation)_

### [ ] T2.7: The six tool handlers
**What:** `list_watchlist`, `get_prices`, `get_news`, `get_portfolio`, `record_analysis`,
`place_paper_order` — one implementation each, in `watcher-agent`.
**Why:** I-1, F-96. One implementation makes parity structural; there is no second code path
to drift.
**How:** Write tools return the stored object with its server id and timestamp (I-2).
Rejections return a reason the model can act on, not a stack trace (I-3). `place_paper_order`
is stubbed here and completed in T4.4.
**Notes:** _(added after implementation)_

### [ ] T2.8: Run orchestration and manual runs
**What:** `POST /api/runs`, run records, per-ticker partial failure, immutable versioned
analyses.
**Why:** F-11, F-12, F-13, F-14, F-15, F-16.
**How:** A run over twenty tickers with one bad symbol stores nineteen analyses and one
recorded failure. A failed run never replaces the last good analysis. Earlier versions stay
reachable.
**Notes:** _(added after implementation)_

### [ ] T2.9: Migration 0008, cost recording and the cap
**What:** `model_call`, spend recorded per call, and the daily cap checked **before** a run
starts.
**Why:** N-30, N-34, N-61, F-95.
**How:** A user at their cap produces no model spend at all that day. Count input tokens per
request — without it the §11 token-budget box cannot be ticked honestly.
**Deviation from `PLAN.md`:** its migration table has 0008 creating both `model_call` and
`chat_message`. `model_call` is needed now and chat is not, so 0008 covers `model_call` only
and `chat_message` moves to 0010 in T5.3. Update the table in `doc/database.md`.
**Notes:** _(added after implementation)_

### [ ] T2.10: Hosted runner — single shot through the batch route
**What:** One pack in, one answer out, submitted asynchronously.
**Why:** F-93, F-97, I-33, I-34.
**How:** Batch submit and poll. System prompt and tool definitions sit in the cached prefix.
One model handles a whole nightly run — caches are per model, so no cascade.
**Notes:** _(added after implementation)_

### [ ] T2.11: MCP server and token auth
**What:** The MCP transport over the T2.7 handlers, plus token create and revoke endpoints.
**Why:** F-90, F-91, F-92, N-41.
**How:** Tokens stored hashed. A revoked token stops working immediately. A tool list from a
connected client shows exactly the six tools and no others. Verify Claude Desktop connects
and completes a full run.
**Notes:** _(added after implementation)_

### [ ] T2.12: Nightly job calls the agent
**What:** Scheduled runs for every active watchlist, after the price ingest.
**Why:** F-10, F-13, F-17.
**How:** `trigger = 'scheduled'`. Manual reruns must not affect scoring — ten reruns in one
day change nothing for that day.
**Notes:** _(added after implementation)_

### [ ] T2.13: Decision log endpoint
**What:** `GET /api/decision-log?ticker=` — every analysis and order with date, ticker,
action and reason.
**Why:** F-83. Deliberately plain; it exists so the record is inspectable while it accrues.
**How:** JSON only. No page yet.
**Notes:** _(added after implementation)_

### [ ] T2.14: Deploy and start the 14-day clock
**What:** Ten tickers, scheduled runs, unattended.
**Why:** §11. "The 14 days is the real test. Everything else can be faked in an afternoon."
**How:** Confirm day one produced analyses with annotations and reasons, and that the cost
for the day matches what T2.9 recorded. From here, every later stage is built while this
runs.
**Notes:** _(added after implementation)_

---

## Stage 3: The contract and the chart (M2)

Ends with: a ticker page showing the chart with the agent's marks on it.

### [ ] T3.1: Chart JSON endpoint, contract frozen
**What:** `GET /api/ticker/{t}/chart?range=1y` returning bars, annotations and trade markers.
**Why:** I-40, F-30, F-31, F-33.
**How:** Exactly the shape in `PLAN.md` § The chart contract — integer minor units with an
explicit `price_scale`, so no float crosses the wire. Freeze it: the frontend is built
against it, so changes are versioned from here on.
**Notes:** _(added after implementation)_

### [ ] T3.2: Ticker endpoint
**What:** `GET /api/ticker/{t}` and `/analyses` — newest successful analysis, its
annotations, and the earlier versions.
**Why:** F-15, F-16.
**How:** State when the shown analysis was made and what its data cutoff was. A failed run
never replaces the last good one.
**Notes:** _(added after implementation)_

### [ ] T3.3: Measure the chart endpoint and close O-11
**What:** Measure p95 latency with a year of bars and 50 annotations, then write that number
into N-10.
**Why:** N-10's figure is deliberately absent and recorded as O-11 rather than guessed. A
number invented now would be the thing later measurements got compared against.
**How:** Measure against real accrued data, not fixtures. Record the backend figure in the
requirements and leave the frontend's render budget to the frontend.
**Notes:** _(added after implementation)_

### [ ] T3.4: Svelte app scaffold
**What:** `frontend/` with Vite, TypeScript, routing and a fetch wrapper.
**Why:** `PLAN.md` D11, D12.
**How:** **Never pass model output to `{@html}`.** That construct is the specific way N-44
gets violated in this stack, so keep it greppable — ideally zero occurrences in the
repository.
**Notes:** _(added after implementation)_

### [ ] T3.5: Watchlist page
**What:** List tickers with their last analysis date; add and remove.
**Why:** F-1. I-41's phone-reading requirement is the frontend's from v1.4 on.
**How:** Against the T1.10 endpoints.
**Notes:** _(added after implementation)_

### [ ] T3.6: Ticker page with the chart
**What:** Summary, chart with annotations drawn, reason and date on hover or click.
**Why:** F-20, F-30, F-31, F-32.
**How:** Each annotation kind renders in its own visual form — lines for levels, bands for
zones, markers for events. The flag from F-23 must be visible on a flagged mark. **The chart
library is the frontend's choice** (`PLAN.md` D13); only the T3.1 contract is fixed.
**Notes:** _(added after implementation)_

### [ ] T3.7: Caddy basic auth, static serving, and the MCP exclusion
**What:** Basic auth over the whole site, Caddy serving `frontend/dist`, `/api` proxied, and
`/mcp` **excluded** from basic auth.
**Why:** `PLAN.md` D14. One origin means no CORS and no token in JavaScript. Basic auth
guards the metered chat endpoint before strangers can reach it.
**How:** If Caddy guards `/*`, Claude Desktop gets a 401 before its bearer token is read and
MCP silently stops working. Test both paths after the change: a browser request and an MCP
connection.
**Notes:** _(added after implementation)_

### [ ] T3.8: Deploy the frontend
**What:** Frontend build stage in Compose, output served by Caddy.
**Why:** The 14-day run is already going; this must not interrupt it.
**How:** Verify the nightly job still ran the night of the deploy.
**Notes:** _(added after implementation)_

---

## Stage 4: The ledger (M3)

Ends with: paper trades filling at the next open and an equity curve beside the benchmark.

### [ ] T4.1: The ledger state machine
**What:** `decide(state, cmd, day) -> Result<Vec<Event>, Rejection>` and `apply(state, event)`
in `watcher-ledger`.
**Why:** `PLAN.md` D2, N-56.
**How:** No sqlx, no tokio, no clock — time arrives as an argument. `decide` never writes,
`apply` never fails. Check `Cargo.toml` has nothing it should not.
**Notes:** _(added after implementation)_

### [ ] T4.2: The six ledger tests
**What:** Fill at next open, insufficient cash, split, dividend, expiry, double-run
idempotency.
**Why:** N-53, and N-56 says they run with no Postgres present.
**How:** The insufficient-cash case is the subtle one: three buys that each fit alone but not
together must fill until cash runs out, then reject — `apply` advances the state between
each, so the later decisions see the earlier fills.
**Notes:** _(added after implementation)_

### [ ] T4.3: Migration 0006 — orders, positions, ledger, snapshots, basket
**What:** `paper_order`, `position`, `ledger_entry`, `snapshot`, `benchmark_basket`, the
natural-key unique index, and the conditional `trading_day` trigger.
**Why:** D-2, D-13, and D-11's corporate-action exception.
**How:** `UNIQUE (portfolio_id, trading_day, kind, subject_id)` makes F-101 a property of
the schema. The trigger takes the pay date for corporate-action entries and the clock for
everything else.
**Notes:** _(added after implementation)_

### [ ] T4.4: `place_paper_order` with run-id resolution
**What:** Complete the T2.7 stub.
**Why:** F-61, F-62, F-63, F-67, F-68, `PLAN.md` D9.
**How:** The order carries a **run id**, not an analysis id — a single-shot agent cannot cite
an id the server has not assigned yet. Resolve run + ticker to that run's analysis. Reject an
order carrying a price, a fractional quantity, a sell beyond shares held, or a run with no
analysis for that ticker — each with its own reason.
**Notes:** _(added after implementation)_

### [ ] T4.5: The fill pass
**What:** Pending orders filled at the next session's open, oldest first.
**Why:** F-64, F-65, F-66.
**How:** `open ± half_spread`, then the flat fee. Re-validate at fill: an order that no
longer fits is rejected, never partially filled. Unfilled after two trading days expires and
the agent is told.
**Notes:** _(added after implementation)_

### [ ] T4.6: Splits and dividends
**What:** Both as explicit ledger entries.
**Why:** F-70, F-71.
**How:** After a 4-for-1 split, 10 shares at 400 become 40 at 100 with total value
unchanged. A dividend is credited on its **pay date** even when processed later — otherwise
the ledger stops agreeing with the snapshot series.
**Notes:** _(added after implementation)_

### [ ] T4.7: The frozen benchmark basket
**What:** Store tickers and share counts at portfolio creation, bought at the next open.
**Why:** F-82, `PLAN.md` D21.
**How:** Never derive it from the current watchlist. Adding a ticker in week three must not
change the benchmark line in week one — write that test.
**Notes:** _(added after implementation)_

### [ ] T4.8: Portfolio and track-record endpoints
**What:** `GET /api/portfolio` and `GET /api/track-record`.
**Why:** F-69, F-81, F-82, F-85.
**How:** Cash plus position values must equal the equity in the latest snapshot. The curve
starts at 10,000 on the day the portfolio was created. Before there is history, say that the
record builds forward from today rather than showing an empty chart.
**Notes:** _(added after implementation)_

### [ ] T4.9: Portfolio and track-record pages
**What:** Positions with unrealised result, pending orders, equity curve against the
benchmark, trade markers on the ticker chart.
**Why:** F-33, F-69, F-81, F-82.
**How:** A filled order appears on the chart the day after it was placed.
**Notes:** _(added after implementation)_

### [ ] T4.10: Job-level idempotency test
**What:** Run the nightly job twice for one date and assert the second changes nothing.
**Why:** F-100, F-101, and the §11 box.
**How:** One snapshot per portfolio, one fill per order, no duplicate ledger entries.
**Notes:** _(added after implementation)_

---

## Stage 5: News, chat and verdicts (M4)

Ends with: the decision log carrying a verdict beside every call.

### [ ] T5.1: NewsSource trait and implementation
**What:** The trait and one implementation. **Requires O-5 to be answered first.**
**Why:** I-20, I-21, F-40, F-42, F-43.
**How:** Headline, source, timestamp and URL only — no article text stored. A 14-day window.
If the news source fails, the rest of the analysis still runs and the panel explains itself.
**Notes:** _(added after implementation)_

### [ ] T5.2: News takes in the run
**What:** A one-line comment per item, stored in `news_take`.
**Why:** F-41.
**How:** Every visible item has either a take or a visible "not yet reviewed" state. Items
outside the 14-day window are not sent to the model.
**Notes:** _(added after implementation)_

### [ ] T5.3: Migration 0010 — chat messages
**What:** `chat_message`.
**Why:** F-50. Split out of 0008 in T2.9.
**How:** No column anywhere for income, savings, risk tolerance or goals — D-8 and C-2 mean
the schema must make storing them impossible, not merely discouraged.
**Notes:** _(added after implementation)_

### [ ] T5.4: Chat endpoint
**What:** `POST /api/ticker/{t}/chat`, grounded, capped, with refusals.
**Why:** F-50, F-51, F-53, F-54, C-2, N-11.
**How:** Answer from stored analysis, prices and positions via the T2.6 assembler — never a
fresh run. Past the cap, a clear message that resets tomorrow. Refuse "what should I do with
my money" questions with a refusal plus general information, and never ask for personal
circumstances.
**Notes:** _(added after implementation)_

### [ ] T5.5: Migration 0007 and the verdict rules
**What:** `call_verdict`, and the per-kind rules from `doc/ledger.md`.
**Why:** F-84.
**How:** `UNIQUE (subject_kind, subject_id, as_of)`. `event` gets **no verdict** — a marker
is not a claim, and scoring one would invent a prediction the agent never made. Test each
rule with a fixed price series.
**Notes:** _(added after implementation)_

### [ ] T5.6: Verdict computation in the nightly job
**What:** Compute verdicts for past calls, appending a row per call per day.
**Why:** F-84, `PLAN.md` D10.
**How:** Append, never update — the annotation stays immutable and the app role could not
edit it anyway. A window that has not matured gives a `provisional` verdict, so the page can
show one firming up rather than appearing finished on day 30.
**Notes:** _(added after implementation)_

### [ ] T5.7: Decision log and track-record pages
**What:** Every call with its reason and verdict, filterable by ticker.
**Why:** F-83, F-84, F-85, and the §11 box "every call in the decision log carries a verdict,
not just a date".
**How:** Show provisional verdicts as provisional. This page is the product's central bet —
it is the thing the brief wants to know whether anyone opens.
**Notes:** _(added after implementation)_

### [ ] T5.8: News and chat on the ticker page
**What:** The news panel with takes, and the chat box.
**Why:** F-40, F-41, F-50.
**How:** Items newest first, linking out to the original. Chat answers must visibly refer to
the analysis on the page.
**Notes:** _(added after implementation)_

---

## Stage 6: Ops and proof (M5)

Ends with: the §11 checklist walked end to end on the deployed instance.

### [ ] T6.1: Backup timer
**What:** `pg_dump` from the host on a systemd timer, 30 days retained.
**Why:** N-20.
**How:** Runs on the host against the container, not inside it. `deploy/backup.service` and
`backup.timer`.
**Notes:** _(added after implementation)_

### [ ] T6.2: Restore drill
**What:** Restore a backup into an empty instance, then write down the procedure.
**Why:** N-21, and the §11 box. This is the one requirement that cannot be met by writing
code — only by carrying it out.
**How:** Against the Compose stack, not a bare database. Record what actually happened,
including anything that did not work first time.
**Notes:** _(added after implementation)_

### [ ] T6.3: Per-user cost accounting
**What:** Report what one user cost over a month from stored data.
**Why:** N-32, F-95, and the §11 box "the cost of one user for one month is a known number".
**How:** From `model_call`, not an estimate. Compare it against the tier pricing in the
brief and say plainly whether the margin works.
**Notes:** _(added after implementation)_

### [ ] T6.4: Structured logs and counters
**What:** A run id tying every step of one run together, plus the N-61 counters.
**Why:** N-60, N-61.
**How:** Counters for runs started and failed, orders placed, rejected and filled, model
calls and their cost, and input tokens per request — the last one is what makes the §11
token-budget box checkable.
**Notes:** _(added after implementation)_

### [ ] T6.5: Data export
**What:** `GET /api/export` — the user's own data as JSON.
**Why:** D-9.
**How:** Everything attributed to that user, nothing belonging to anyone else.
**Notes:** _(added after implementation)_

### [ ] T6.6: Walk the §11 checklist
**What:** Go through all eighteen acceptance boxes on the deployed instance.
**Why:** It is the release gate, and the requirements say "on a deployed instance, not a
laptop".
**How:** Tick each box with evidence, not judgement. The 14-day box should already be
satisfied by the run started in T2.14. Where a box fails, record why rather than quietly
adjusting it.
**Notes:** _(added after implementation)_

---

## Not in these tasks

Deliberately absent, per `PLAN.md` § Later / Out of Scope: password login and multi-user,
chat history per ticker (F-52, Later), delisting (F-72, Later), rate limits per token (N-45),
account deletion (D-10), alerting (N-62).

**Still unanswered and blocking:** **O-3** (hours per week — it decides whether six stages is
a plan or a wish) and **O-5**, which blocks T5.1.
