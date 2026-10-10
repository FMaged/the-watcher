# The Watcher — requirements

Version 1.3 · 2026-10-10 · Status: draft for review

---

## 1. Purpose and scope

### 1.1 Purpose

This document states what the stock analyser must do. It does not say how to build it. Design and task breakdown live in `PLAN.md` and `TASKS.md`.

### 1.2 Product in one sentence

A workspace where a user keeps a watchlist, an AI agent analyses it daily, marks the price chart with where it would buy and sell, explains the news, answers questions, and trades on paper so every call can be scored later.

### 1.3 Scope of version 1

In scope: watchlist, daily analysis, annotated chart, news with commentary, assistant chat, paper trading, track record, two agent connection modes, US-listed stocks.

Out of scope: see section 10.

### 1.4 How to read a requirement

Each requirement has an id, a priority and an acceptance test.

| Priority | Meaning |
| --- | --- |
| **M** | Must be in version 1. Version 1 is not done without it. |
| **S** | Should be in version 1 if time allows. Does not block release. |
| **L** | Later. Recorded so the design does not block it. |

Ids are stable. Never renumber. If a requirement dies, mark it withdrawn and keep the id.

---

## 2. Definitions

| Term | Meaning |
| --- | --- |
| **Agent** | The AI model plus the loop that calls our tools. Either the user's own (MCP mode) or ours (hosted mode). |
| **Run** | One pass of the agent over one or more tickers. Produces analyses, annotations and possibly orders. |
| **Analysis** | The agent's written output for one ticker on one day. |
| **Annotation** | A structured mark the agent puts on a chart, such as a support line or an entry zone. |
| **Paper order** | An order recorded by the system that is never sent to a real market. |
| **Fill** | The moment a paper order gets a price, taken from the next market open. |
| **Snapshot** | The daily record of a portfolio's cash, positions and total value. |
| **Benchmark** | Equal-weight buy-and-hold of the watchlist as it stood when the portfolio was created. |
| **MCP** | Model Context Protocol. The interface a user's own AI assistant uses to call our tools. |
| **Ticker** | A US-listed stock symbol, for example `AAPL`. |

---

## 3. Actors

| Actor | Description |
| --- | --- |
| **User** | Owns a watchlist and a paper portfolio. Reads the pages, asks questions. |
| **User's agent** | An AI assistant the user already pays for, connected over MCP. Acts for the user. |
| **Hosted agent** | Our own runner, calling a model we pay for. Acts for the user on a schedule. |
| **Scheduler** | Internal. Triggers the nightly job and hosted runs. |
| **Operator** | Us. Deploys, monitors, restores backups. |

---

## 4. Assumptions and dependencies

**A-1.** Free market data for US stocks is available and good enough for daily bars. If it stops being free or available, the system must keep working with a paid source behind the same interface.

**A-2.** Users can connect an MCP server to their own AI assistant without our help beyond a short guide.

**A-3.** Daily granularity is enough. No requirement in this document needs intraday or real-time prices.

**A-4.** Model output is unreliable. Every value from the agent is treated as untrusted input.

**A-5.** The user has their own broker. We never hold money, securities or payment details for trading.

---

## 5. Functional requirements

### 5.1 Watchlist

**F-1 — Manage the watchlist** · M
The user can add and remove tickers. A ticker is validated against the instrument list when added.
*Accept:* adding an unknown symbol is rejected with a clear message; a removed ticker stops being analysed from the next run.

**F-2 — Watchlist size limit** · M
A watchlist holds at most 50 tickers.
*Accept:* adding the 51st is refused and explains why.

**F-3 — Keep history after removal** · M
Removing a ticker hides it from the watchlist but keeps its past analyses, annotations and trades.
*Accept:* after removal, the track record still shows what the agent said about that ticker.

### 5.2 Analysis runs

**F-10 — Scheduled run** · M
The system runs the agent once per trading day after the US close for every active watchlist.
*Accept:* with the scheduler on and no user action, fourteen trading days produce fourteen sets of analyses.

**F-11 — Manual run** · M
The user can trigger a run for one ticker or the whole watchlist.
*Accept:* a run started from the page produces a fresh analysis within two minutes.

**F-12 — Run record** · M
Every run is recorded with its trigger, start time, end time, outcome and which model was used.
*Accept:* a failed run is visible with its reason, not silently missing.

**F-13 — Partial failure** · M
If one ticker fails during a run, the others still complete.
*Accept:* a run over twenty tickers where one symbol has no data stores nineteen analyses and one recorded failure.

**F-14 — Analyses are immutable** · M
Every successful run creates a new analysis record. A record is never updated or replaced. Each one stores its trigger (`scheduled` or `manual`), the server time it was created, its data cutoff (the date of the latest price bar and the timestamp of the newest news item it saw) and a version number that increments per ticker per day.
*Accept:* running twice in one day leaves two records, both retrievable, neither altered.

**F-15 — Which analysis is shown** · M
The ticker page shows the newest successful analysis for that ticker. A failed run never replaces the last successful one. The page states when the shown analysis was made and what its data cutoff was.
*Accept:* after a failed manual run, the page still shows the last good analysis, labelled with its own date.

**F-16 — Reaching earlier analyses** · M
Earlier versions from the same day and from past days are reachable from the ticker page.
*Accept:* after a manual rerun in the afternoon, the morning's scheduled analysis is still one click away.

**F-17 — Scoring uses the scheduled analysis** · M
Where the track record compares calls over time, it uses the scheduled daily analysis, not manual reruns.
*Accept:* ten manual reruns in one day do not change the track record for that day.

**F-18 — One context pack** · M
The material handed to the model — prices, news, portfolio, past analyses — is assembled by one component, used by both the daily run and chat.
*Accept:* there is one place that builds model context. Chat and the daily analysis cannot contradict each other because they saw different data.

**F-19 — Context budget** · M
The budget belongs to the whole request, not to the pack alone. N-33 caps the request at 100,000 tokens; the assembler measures the stable prefix — system prompt and tool definitions — and gives the pack what is left, never more than 90,000 tokens. It drops the oldest material to stay inside that figure and records what it dropped. A pack that is itself under budget while the request around it is over is the failure this wording exists to prevent.
*Accept:* a ticker with two years of history and heavy news produces a request under 100,000 tokens, and the run log shows what was dropped to get there.

### 5.3 Analysis content

**F-20 — Daily summary** · M
Each analysis contains a plain-language summary of three to five sentences.
*Accept:* the summary renders above the chart and mentions this ticker, not general market talk.

**F-21 — Structured annotations** · M
Each analysis may contain annotations in a fixed schema. Allowed kinds: `support`, `resistance`, `entry_zone`, `stop`, `breakout`, `event`.
*Accept:* an annotation with an unknown kind is rejected and the run records a validation error.

**F-22 — Reason per annotation** · M
Every annotation carries a short text reason.
*Accept:* an annotation without a reason is rejected.

**F-23 — Annotation validation** · M
Validation rules are per mark kind. Structurally invalid values are rejected: a non-positive or non-numeric price, a zone whose low is above its high, a missing date on `breakout` or `event`, a date in the future. A price that is unusually far from the last close is **not** rejected. It is stored with a warning flag, drawn with that flag visible, and counted so the threshold can be tuned from real data.
*Accept:* a zone with low above high is rejected; a support line at a quarter of the last close is stored, drawn and flagged.

**F-24 — Confidence** · Withdrawn
A model's self-reported confidence is not calibrated, and a number that looks like a probability next to financial content invites the wrong reading. Not in version 1. It returns only with a defined scale, a stated interpretation and measured calibration behind it.

### 5.4 Chart

**F-30 — Price chart** · M
A ticker page shows a candlestick or line chart with at least one year of daily bars.
*Accept:* the chart loads in under two seconds on a normal connection.

**F-31 — Annotations drawn** · M
Current annotations are drawn on the chart: lines for levels, bands for zones, markers for events.
*Accept:* each annotation kind renders in its own visual form.

**F-32 — Reason on demand** · M
Hovering or clicking an annotation shows its reason and the date it was made.
*Accept:* the reason shown matches the stored text for that annotation.

**F-33 — Trade markers** · M
Paper buys and sells appear on the chart at their fill date and price.
*Accept:* a filled order appears on the chart the day after it was placed.

**F-34 — Historical annotations** · S
The user can switch on past annotations to see what the agent said weeks ago against what the price did.
*Accept:* with the toggle on, annotations older than today are drawn in a muted style.

**F-35 — Range control** · S
The user can change the visible range: 1M, 6M, 1Y, all.
*Accept:* changing range does not re-fetch the whole history.

### 5.5 News

**F-40 — News list** · M
A ticker page shows recent news items with headline, source, timestamp and link.
*Accept:* items are ordered newest first and link out to the original.

**F-41 — Take per item** · M
Each news item in the last run carries a one-line comment from the agent.
*Accept:* every item visible on the page has either a take or a visible "not yet reviewed" state.

**F-42 — News window** · M
Only items from the last 14 days are shown.
*Accept:* older items do not appear, and are not sent to the model.

**F-43 — News is optional** · M
If the news source fails, the rest of the analysis still runs.
*Accept:* with news disabled, a run completes and the page shows an empty news panel with an explanation.

### 5.6 Assistant chat

**F-50 — Ask about a ticker** · M
The user can ask free-text questions on a ticker page.
*Accept:* a question returns an answer that refers to the analysis on the page.

**F-51 — Grounded answers** · M
Chat answers are produced from stored analysis, prices and positions, not from a fresh full run.
*Accept:* asking the same question twice in a day does not trigger a new analysis.

**F-52 — Chat history per ticker** · L
Chat is kept per ticker and reloads when the page is opened again.
*Accept:* a question asked yesterday is still visible today.

**F-53 — Chat cap** · M
Chat messages per user per day are capped.
*Accept:* past the cap the user sees a clear message and the cap resets the next day.

**F-54 — No personal advice** · M
The assistant must refuse to answer questions that ask what the user personally should do with their money, and must not ask for personal financial circumstances.
*Accept:* asking "should I put my savings into this?" produces a refusal plus general information, and nothing about the user's finances is stored.

### 5.7 Paper trading

**F-60 — Portfolio creation** · M
Each user gets one paper portfolio with 10,000 USD starting cash, fixed.
*Accept:* a new portfolio always starts at exactly 10,000 USD.

**F-61 — Place a paper order** · M
The agent can place a market buy or sell with ticker, side and whole-share quantity. It cannot set a price.
*Accept:* an order carrying a price field is rejected.

**F-62 — Reason required** · M
An order must reference the run it belongs to. The server resolves that reference to the analysis for that run and ticker, which is unique because one run produces one analysis per ticker. The order carries a run id, not an analysis id: a single-shot agent (F-97) emits its analysis and its order in the same turn and cannot cite an id the server has not assigned yet, and an agent that explores over several calls must not need a different tool surface to do the same thing (F-96).
*Accept:* an order whose run holds no analysis for that ticker is rejected; an order emitted in the same turn as its analysis is accepted and reaches that analysis from the log.

**F-63 — Validation at placement** · M
The server checks: ticker is in the instrument list, quantity is a positive whole number, enough cash for a buy, enough shares for a sell.
*Accept:* each failure is rejected with a distinct reason the agent can read.

**F-64 — Fill at next open** · M
Pending orders fill at the next session's open price, minus half the configured spread, minus a flat fee.
*Accept:* an order placed Friday evening fills at Monday's open.

**F-65 — Re-validate at fill** · M
Cash and position are checked again at fill time. An order that no longer fits is rejected, not partially filled.
*Accept:* three buys that each fit alone but not together result in fills until cash runs out, then rejections.

**F-66 — Order expiry** · M
An unfilled order expires after two trading days and the agent is told.
*Accept:* an order for a halted ticker expires and shows as expired.

**F-67 — No fractional shares** · M
Whole shares only, rounded down. If one share costs more than available cash, the order is rejected.
*Accept:* a buy of 1.5 shares is rejected.

**F-68 — No shorting in v1** · M
Sell orders are limited to shares held.
*Accept:* selling more than held is rejected.

**F-69 — Portfolio view** · M
The user sees cash, open positions with entry and current price and unrealised result, and pending orders.
*Accept:* the sum of cash and position values equals the equity in the latest snapshot.

### 5.8 Corporate actions

**F-70 — Splits** · M
Share splits adjust positions as an explicit ledger entry.
*Accept:* after a 4-for-1 split, 10 shares at 400 become 40 shares at 100 and total value is unchanged.

**F-71 — Dividends** · M
Cash dividends are credited on the pay date. No reinvestment.
*Accept:* a dividend appears as its own ledger entry and increases cash.

**F-72 — Delisting** · L
A delisted ticker closes the position at the last known price and records why.
*Accept:* the portfolio does not keep a position in an instrument that no longer trades.

### 5.9 Track record

**F-80 — Daily snapshot** · M
One snapshot per portfolio per trading day, including days with no activity.
*Accept:* fourteen trading days produce fourteen snapshot rows, whatever the agent did.

**F-81 — Equity curve** · M
A chart of portfolio value over time.
*Accept:* the curve starts at 10,000 on the day the portfolio was created.

**F-82 — Benchmark** · M
The equity curve is drawn against equal-weight buy-and-hold of the watchlist as it stood at portfolio creation, bought at the next open, never rebalanced. Tickers added later do not join it.
*Accept:* adding a ticker in week three does not change the benchmark line in week one.

**F-83 — Decision log** · M
A list of every analysis and order with date, ticker, action and reason, filterable by ticker.
*Accept:* every order in the ledger appears in the log with its reason.

**F-84 — Verdict per call** · M
Every past annotation and order carries a verdict computed by the nightly job, not a raw price change. The rule is falsifiable and defined per mark kind, for example: did the close break below a `support` level within 30 days; was an `entry_zone` touched, and where was the price 30 days later; was a `stop` hit before the entry paid.

A verdict is a new record about the call it judges, never an edit to it. The annotation stays immutable and the application cannot alter it in any case (D-6, D-12). A window that has not matured gives a provisional verdict, and each recomputation appends, so the page can show a verdict firming up instead of appearing finished on day 30.
*Accept:* the decision log shows a verdict beside each call, not just a date and a price; a 30-day verdict read on day 7 is marked provisional, and that day-7 reading is still retrievable on day 30.

> F-84 is **M**, not S, because the product's central test is whether people open an old analysis. A list of old analyses without verdicts is a log, and nobody opens a log. F-52 and F-72 were moved to L to pay for it.

**F-85 — Empty state** · M
Before there is history, the pages explain that the record builds forward from today.
*Accept:* a new user sees a useful explanation, not an empty chart.

### 5.10 Agent connection

**F-90 — MCP server** · M
The system exposes an MCP server so a user's own assistant can act for them.
*Accept:* Claude Desktop connects with a token and can complete a full run.

**F-91 — Token auth** · M
Each MCP connection carries a token that identifies one user. All writes are attributed to that user.
*Accept:* a request with no or a wrong token is refused.

**F-92 — Token management** · M
The user can create and revoke tokens.
*Accept:* a revoked token stops working immediately.

**F-93 — Hosted runner** · M
The system can run the agent itself against the same tools, with a model chosen by configuration.
*Accept:* switching the model provider needs a configuration change and a restart, no code change.

**F-94 — Model portability** · M
The hosted runner works with any OpenAI-compatible endpoint, and with Anthropic's API.
*Accept:* the same run completes against a local Ollama model and against a hosted one.

**F-95 — Per-user model cost** · S
Model spend per user per day is recorded.
*Accept:* the operator can see what one user cost last month.

**F-96 — Capability parity** · M
The hosted agent has exactly the tools a user's own agent has. Not one more. The tool handlers are implemented once and both paths go through the same validation. Parity is a claim about what an agent may write and what is checked before it is stored. It is not a claim about reach: a hosted run is single-shot, so it never calls the four read tools and the context pack supplies what they would have returned (F-97).
*Accept:* a capability added for the hosted runner is reachable over MCP in the same release, or it is not added; the write tools and their validation have one implementation, with no branch in it that asks which mode is calling.

**F-97 — Stated difference in reach** · M
Hosted runs are single-shot: the agent gets one context pack and answers once. A user's own agent can explore across several calls and is therefore more capable, not less. Wherever the two modes are described to users, this difference is stated plainly.
*Accept:* no page claims the hosted agent does the same thing as a connected one.

### 5.11 Operations

**F-100 — Manual job trigger** · M
The operator can run the nightly job by hand for a given date.
*Accept:* re-running it for a date already processed does not double-fill orders.

**F-101 — Idempotent nightly job** · M
Running the job twice for the same day has the same result as running it once.
*Accept:* two runs produce one snapshot per portfolio and one fill per order.

**F-102 — Health endpoint** · M
An endpoint reports database reachability, last successful nightly job and last successful price fetch.
*Accept:* with the nightly job failing, the endpoint reports unhealthy.

---

## 6. External interfaces

### 6.1 MCP tool surface

**I-1** · M — The MCP server exposes these tools, and no others in v1:

| Tool | Purpose | Writes? |
| --- | --- | --- |
| `list_watchlist` | the user's tickers | no |
| `get_prices` | daily bars for a ticker | no |
| `get_news` | recent items for a ticker | no |
| `get_portfolio` | cash, positions, pending orders | no |
| `record_analysis` | summary plus annotations | yes |
| `place_paper_order` | ticker, side, quantity, run id | yes |

*Accept:* a tool list from a connected client shows exactly these six.

**I-2** · M — Write tools return the stored object with its server-assigned id and timestamp.

**I-3** · M — Rejections return a reason the model can act on, not a stack trace.

### 6.2 Market data

**I-10** · M — Daily OHLCV bars and corporate actions for US-listed stocks, behind one internal interface so the provider can be swapped.

**I-11** · M — Raw prices are stored. Adjusted prices are never stored as the source of truth, because they change retroactively.

**I-12** · M — A provider failure is retried, then recorded as a failed fetch. It never writes partial or guessed data.

### 6.3 News

**I-20** · M — Headline, source, timestamp and URL per item, behind one internal interface.

**I-21** · M — Full article text is not stored. Only what the provider's terms allow.

### 6.4 Model provider

**I-30** · M — One internal interface for chat completion. One implementation for OpenAI-compatible endpoints, one for Anthropic.

**I-31** · M — Provider, model name, base URL and key come from configuration.

**I-32** · M — Model calls have a timeout and a retry limit. A hanging provider must not hang a run.

**I-33** · M — Scheduled runs go through the provider's asynchronous batch route where one exists. Nobody is waiting on a job that starts after the US close, and the discount is large. Chat and manual runs stay synchronous.

**I-34** · M — Where the provider supports caching of a stable prompt prefix, the system prompt and tool definitions sit in that prefix. Caches are per model, so a single model handles a whole nightly run rather than a cascade of models.

**I-35** · M — Tool calls are never forced. The run names the tool it wants in the instruction and leaves the choice to the model, and where the provider can validate tool arguments against the declared schema before returning them, that is switched on. Some current models reject a forced tool choice outright, and which ones do is not something a configuration change (I-31) should be able to break.
*Accept:* the same run completes against two configured models with no code change, and arguments that do not match the N-55 schema never reach the validator.

### 6.5 Web interface

**I-40** · M — Chart data is available as its own JSON endpoint, separate from the page that displays it.

**I-41** · M — Works on a phone screen for reading. Editing the watchlist on a phone is **S**.

---

## 7. Data requirements

### 7.1 Entities

`user`, `api_token`, `instrument`, `watchlist_item`, `price_bar`, `corporate_action`, `news_item`, `news_take`, `run`, `analysis`, `annotation`, `portfolio`, `position`, `order`, `ledger_entry`, `snapshot`, `chat_message`, `model_call`, `raw_model_output`, `call_verdict`.

### 7.2 Rules

**D-1** · M — Every record that belongs to a user is attributed to that user from the start, even while there is no login.

**D-2** · M — The ledger is append-only. No row is updated or deleted after it is written. A correction is a new reversing entry.

**D-3** · M — All money is stored in minor units as integers. No floating point for cash or quantities.

**D-4** · M — All timestamps are stored in UTC. Trading days follow the US market calendar.

**D-5** · M — Timestamps on agent output are set by the server, never taken from the agent.

**D-6** · M — Analyses and annotations are kept indefinitely and are never updated after they are written. They are the product.

**D-7** · M — Price bars are kept for at least five years.

**D-8** · M — No personal financial data is stored: no income, savings, risk tolerance or investment goals. There is no field for them anywhere in the schema.

**D-9** · M — A user can export their own data as JSON.

**D-10** · S — A user can delete their account, which removes their rows within 30 days.

**D-11** · M — `trading_day` is never accepted as input on an analysis or a ledger entry. It is derived at write time from the server clock and the trading calendar. There is then no code path that can backdate a record, which is what N-1 asks for.

One exception, and it is deliberately narrow: a ledger entry for a corporate action takes its trading day from the corporate action record, because a dividend is credited on its pay date (F-71) and may be processed later than that. The exception is open to corporate-action entries and to nothing else — analyses, orders and fills take the clock. Without it, a dividend processed on Monday for Friday's pay date lands on Monday and the ledger stops agreeing with the snapshot series (F-80).
*Accept:* a dividend with Friday's pay date processed on Monday appears in the ledger on Friday; no path writes an analysis with any trading day but today's.

**D-12** · M — The application's database role has no UPDATE or DELETE on `analysis`, `annotation` or `ledger_entry`. Migrations run under a separate role. D-2 and D-6 become unforgeable rather than reviewed.

**D-13** · M — Every ledger entry carries a deterministic natural key — portfolio, trading day, kind, order — and the database enforces its uniqueness. Idempotency (F-101) is then a property of the schema, not something the job has to get right.

**D-14** · M — The model's raw output is stored for every run, keyed to the run and the ticker rather than to the annotation, and kept whether the annotations it carried were accepted, flagged or rejected. A rejected annotation has no annotation row to sit beside, and those are the payloads worth keeping most.
*Why:* without a distribution of real values there is no way to tune the threshold C-20 starts at, and a rejection with no payload cannot be debugged.

**D-15** · M — A US trading calendar covering at least 2026 to 2030 is part of the schema, seeded by migration, not computed at runtime. It is what makes N-3 and D-11 testable.

---

## 8. Non-functional requirements

### 8.1 Correctness of the record

**N-1** · M — The record must never be improved after the fact. No backfilling history by running the agent over past data.
*Accept:* there is no code path that creates an analysis with a past date, because `trading_day` is derived rather than supplied (D-11), and the application role cannot rewrite what is already written (D-12). This requirement is enforced by the schema, not by review.

**N-2** · M — Every agent-supplied value is validated before storage.

**N-3** · M — A missing price, a market holiday or a halted ticker never crashes a run.
*Accept:* a run on Thanksgiving completes and records no fills.

### 8.2 Performance

**N-10** · M — A ticker page renders in under 2 seconds with a year of bars and 50 annotations.

**N-11** · M — Reading any page triggers no model call.

**N-12** · S — A full run over 50 tickers completes in under 10 minutes.

### 8.3 Availability and recovery

**N-20** · M — Nightly database backup, retained 30 days.

**N-21** · M — A restore into an empty instance is tested before release and documented.
*Accept:* the restore procedure has been carried out at least once and written down.

**N-22** · M — The nightly job recovers on its own after a failure: the next run catches up the missed day.

**N-23** · S — Planned downtime does not lose a trading day of snapshots.

### 8.4 Cost

**N-30** · M — Model spend per user per day is capped by configuration. Past the cap, runs stop and the user is told.

**N-31** · M — Analysis is cached per ticker per day. Repeat page views cost nothing.

**N-32** · M — The cost of running one user for one month is measurable from stored data.

**N-33** · M — No single request exceeds 100,000 input tokens, counting cache reads and writes.
*Why:* on the model chosen for the standard tier, crossing that line multiplies the price of that request several times over. The budget is a cost control, not a technical limit.

**N-34** · M — The per-user daily cap (N-30) is enforced before a run starts, not after it finishes.
*Accept:* a user at their cap produces no model spend at all that day.

### 8.5 Security and privacy

**N-40** · M — Secrets and model keys come from the environment. They never appear in the repository, the browser or the logs.

**N-41** · M — API tokens are stored hashed.

**N-42** · M — All traffic over TLS.

**N-43** · M — One user can never read another user's watchlist, analyses or portfolio.
*Accept:* a request with a valid token for user A cannot reach user B's data by changing an id.

**N-44** · M — Input from the model is never rendered as raw HTML.

**N-45** · S — Rate limits per token on all write tools.

### 8.6 Maintainability

**N-50** · M — A change to one area of behaviour — analysis, orders, web — must not force changes in the others.

**N-51** · Moved to `PLAN.md` — where interfaces sit is a design decision, not a requirement.

**N-52** · Moved to `PLAN.md` — migration mechanics are a design decision, not a requirement.

**N-53** · M — The paper ledger rules have unit tests covering: fill at next open, insufficient cash, split, dividend, expiry, double-run idempotency.

**N-54** · M — Swapping the price source, the news source or the model provider must not require changes outside that one area.
*Accept:* replacing the price provider touches no file that deals with orders, analyses or pages.

**N-55** · M — The annotation schema has one definition. The schema handed to the model and the schema the validator checks against are generated from it, so they cannot drift apart.
*Accept:* changing an allowed annotation kind in one place changes both the model's contract and the validator.

**N-56** · M — The paper ledger rules are testable without a database.
*Accept:* the N-53 tests run with no Postgres present.

### 8.7 Observability

**N-60** · M — Structured logs with a run id that ties together every step of one run.

**N-61** · M — Counters for: runs started and failed, orders placed, rejected and filled, model calls and their cost, and input tokens per request. The last is not spare bookkeeping. The token budget (N-33) cannot be shown to have held across the 14 days unless every request's size was recorded while it ran.

**N-62** · S — An alert when the nightly job has not succeeded for 36 hours.

---

## 9. Constraints

### 9.1 Regulatory

These are not preferences. Breaking one changes what the product legally is. Based on [BaFin's guidance on automated advisory and trading systems](https://www.bafin.de/DE/unternehmen-maerkte/erlaubnis-registrierung/fintech/beratungs-handelssysteme/beratungs-handelssysteme.html).

**C-1** · M — No real orders, ever. Managing another person's assets with discretion is Finanzportfolioverwaltung and needs a licence under § 32 Abs. 1 S. 1 KWG or § 15 Abs. 1 WpIG, plus 75,000 euro of initial capital.

**C-2** · M — No suitability questionnaire. Never collect income, savings, risk tolerance or investment goals. A personal recommendation on a named instrument based on personal circumstances is Anlageberatung under § 1 Abs. 1a S. 2 Nr. 1a KWG. BaFin states that automating it changes nothing.

**C-3** · M — Track records stay private to their owner in v1. Publishing calls on named instruments to an open audience can trigger notification duties under § 86 WpHG and the MAR rules.

**C-4** · M — No copy trading or signal following. BaFin treats operating such a platform as licensable in the normal case.

**C-5** · M — Every page carries a notice that this is information, not investment advice.

**C-6** · M — Operating without a required licence is a criminal offence under § 54 KWG and § 82 WpIG. Before anything in the product touches a real order, obtain written legal advice.

*This document is not legal advice.*

### 9.2 Technical

**C-10** · M — Rust with axum. PostgreSQL. One binary.
**C-11** · M — US-listed stocks only in v1.
**C-12** · M — No build pipeline for the frontend in v1.
**C-13** · M — Runs on a single host.
**C-14** · M — Pages are server-rendered. Charts are drawn client-side from JSON.
**C-15** · M — One Cargo workspace, one deployable binary, with crate-level boundaries between analysis, orders and web.
**C-16** · M — Schema changes are versioned migrations.
**C-17** · M — The repository is `the-watcher`, the binary `watcher`.
**C-18** · M — Deployed on a single small VPS, not on home hardware. TLS, uptime and the unattended 14-day run are all easier on a host with provider snapshots.
**C-19** · M — Half-spread 5 basis points, flat fee 1 USD per order. Both configurable, both revisited once there is real data.
**C-20** · M — The annotation warning threshold (F-23) starts at 25% from the last close. It is a starting value chosen to be revised, not a considered one.

These are choices already made, recorded so the requirements above do not have to carry them. The reasoning behind each belongs in `PLAN.md`.

---

## 10. Out of scope for version 1

Real order execution · suitability profiling · public leaderboards · copy trading · crypto, derivatives, forex · European and Asian markets · intraday and real-time data · mobile apps · portfolio import from a real broker · tax reporting · multi-currency · separately deployed services · OpenFGA and fine-grained sharing · payment and subscription handling.

Each has a reason, recorded in the product brief or the idea document.

---

## 11. Acceptance for version 1

Version 1 ships when all of these are true on a deployed instance, not a laptop:

- [ ] A watchlist of 10 tickers runs unattended for 14 trading days
- [ ] Every run leaves an analysis, its annotations and any order, each with a reason
- [ ] The ticker page shows the chart with annotations drawn on it
- [ ] Clicking an annotation shows why it is there
- [ ] News appears with a take on each item
- [ ] A chat question is answered from stored analysis
- [ ] Orders fill at the next open with costs subtracted
- [ ] The equity curve runs beside the benchmark from day one
- [ ] A market holiday passes with no failed run
- [ ] A split or dividend is applied correctly and visibly, including one processed later than its pay date
- [ ] The database restores from backup into an empty instance
- [ ] The cost of one user for one month is a known number
- [ ] The MCP server completes a full run from Claude Desktop
- [ ] The hosted runner completes the same run with a configured model
- [ ] Every call in the decision log carries a verdict, not just a date
- [ ] The application database role is proven unable to update or delete an analysis
- [ ] Running the nightly job twice for the same date changes nothing the second time
- [ ] No request in the 14 days exceeded the token budget

The 14 days is the real test. Everything else can be faked in an afternoon. A record that accumulates on its own cannot.

---

## 12. Open issues

### Still open

| # | Question | Blocks |
| --- | --- | --- |
| O-3 | **Hours per week available.** The only one nobody else can answer, and per the risk list it decides whether the milestones are a plan or a wish. | milestone sizing |
| O-5 | News provider. | I-20 |
| O-10 | **Measured tokens per ticker-day.** Not an estimate: the provider's token-counting endpoint, run over a real context pack, on the day F-18's assembler first works. N-33 is a Must and on the standard tier the price per token multiplies past the line, so a wrong figure is not a cost surprise but a fivefold one. Check separately whether the upper tier's model carries its own long-context pricing before N-33's reasoning is applied to both tiers. | N-33, F-19, cost model |

### Closed

| # | Decision |
| --- | --- |
| O-1 | Name: `the-watcher`. See C-17. |
| O-2 | A small VPS, not home hardware. See C-18. |
| O-4 | A cheap, fast model for the standard tier; a mid-tier model for the upper tier, priced to cover it. Both through the batch route (I-33). Costs verified against [current API pricing](https://platform.claude.com/docs/en/about-claude/pricing). |
| O-6 | 5 bps half-spread, 1 USD flat fee, configurable. See C-19. |
| O-7 | Starts at 25%, tuned from stored raw payloads (D-14) after the first 14 days. See C-20. |
| O-8 | No special rule. F-15 already says newest successful wins and the page is always labelled; tomorrow's scheduled run replaces it naturally, and F-17 keeps scoring clean. |
| O-9 | Closed as a decision rather than a lookup: the tool choice is never forced, so which models reject forcing it stops mattering. See I-35. |

---

## 13. Change log

| Version | Date | Change |
| --- | --- | --- |
| 1.3 | 2026-10-10 | Four conflicts between v1.2 requirements resolved. An order now references its run, not an analysis id, which a single-shot agent cannot know (F-62, I-1). Verdicts append rather than edit, and may be provisional (F-84). The context budget belongs to the request, not the pack (F-19). `trading_day` gets one narrow exception for corporate actions (D-11). Raw payloads keyed to the run so rejections survive (D-14). Parity scoped to writes and validation, not reach (F-96). Tool choice never forced, closing O-9 (I-35). Input tokens per request counted, without which the last acceptance box cannot be ticked (N-61). Entities `raw_model_output` and `call_verdict` added. |
| 1.2 | 2026-10-10 | Capability parity and the single-shot difference between hosted and connected agents stated (F-96, F-97). One context pack with a token budget (F-18, F-19). F-84 raised to M as a verdict per call, paid for by moving F-52 and F-72 to L. N-1 moved from promise to schema: derived trading day, revoked UPDATE and DELETE, natural keys, raw payloads, seeded trading calendar (D-11 to D-15). Batch and prompt caching added (I-33, I-34, N-33, N-34). One annotation schema definition (N-55). Ledger testable without a database (N-56). Six open issues closed into constraints C-17 to C-20; O-9 and O-10 opened. |
| 1.1 | 2026-10-10 | Analyses made immutable with trigger, data cutoff and version (F-14 to F-17). Annotation validation reworked per kind, distant prices flagged instead of rejected (F-23). Confidence withdrawn (F-24). Design decisions moved out of N-50 to N-52, I-40 and D-1 into constraints C-14 to C-16 and `PLAN.md`. |
| 1.0 | 2026-10-10 | First full version |
