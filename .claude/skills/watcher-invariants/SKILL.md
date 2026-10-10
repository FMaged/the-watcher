---
name: watcher-invariants
description: Use when reviewing, writing, or changing any code, migration, or query in The Watcher (e.g. "review this handler", "check this migration", "is this safe to merge", "add a column for X"). Checks the project's non-negotiables — the record cannot be rewritten, money is never a float, no personal financial data is stored, no real order is ever placed. Use it even for a change that looks trivial, because these are the rules that break silently.
---

# Watcher invariants

Check a change against the rules that make this product what it is. These are not style
preferences — each one, if broken, either changes what the product legally is or destroys the
thing it exists to sell: a record nobody can quietly improve after the fact.

Report problems. Do not rewrite unless asked.

Requirements are cited by id. The source of truth is `stock-analyser-requirements.md`, with
design in `PLAN.md` and schema detail in `doc/database.md`.

## 1. The record cannot be rewritten

This is **N-1**, and it is the whole product. A tool that is honest about being wrong is
useful; a tool that can tidy its history afterwards is worthless, and nobody can tell from
the outside which one they are using.

- An `UPDATE` or `DELETE` touching `analysis`, `annotation`, `ledger_entry`, `news_take`,
  `raw_model_output` or `call_verdict`. The app role has no such grant (**D-12**), so this
  fails at run time rather than at review — which means it fails in production, at night.
- A correction applied by editing a row rather than writing a reversing entry (**D-2**).
- `trading_day` appearing in an `INSERT` column list or bound as a parameter. It is set by
  trigger from the server clock (**D-11**); passing it is how backdating starts.
- Any code path that creates an analysis for a past date — a "backfill", a "catch-up run
  over history", a seed script that writes analyses. **N-1**'s acceptance test is that no
  such path exists.
- A verdict written as an update to the annotation it judges rather than a new `call_verdict`
  row (**F-84**). Verdicts append; that is also how a provisional one firms up.
- The one legitimate exception: a corporate-action ledger entry takes its day from the
  corporate action, because a dividend is credited on its pay date (**F-71**). Anything else
  reaching for that exception is wrong.

## 2. Money and quantities

- `f32` or `f64` anywhere near cash, prices or share counts. Money is `i64` minor units and
  shares are whole integers (**D-3**, **F-67**). A float cent error in an append-only ledger
  cannot be corrected, only reversed.
- A `Money` value converted to or from a float "just for display" or "just for the chart".
  The chart contract carries integer minor units with an explicit `price_scale` for exactly
  this reason.
- Rounding that is not stated. Fractional shares round down, and a buy of less than one share
  is rejected rather than rounded to zero (**F-67**).
- A division that silently truncates where the remainder is money.

## 3. No personal financial data

**D-8** and **C-2** are not caution, they are the line between this product and a licensed
one. A personal recommendation on a named instrument, based on personal circumstances, is
Anlageberatung under § 1 Abs. 1a S. 2 Nr. 1a KWG, and BaFin has said that automating it
changes nothing.

- A column, struct field, form input, or prompt asking for income, savings, net worth, risk
  tolerance, investment goals, age or employment. There is to be **no field for them anywhere
  in the schema**, so that no code path can start storing them by accident.
- A chat prompt or system prompt that invites the user to describe their circumstances.
- An answer that tells the user what they personally should do with their money. The
  assistant refuses and offers general information instead (**F-54**).
- A "suitability", "profile", "onboarding questionnaire" or "risk score" concept appearing
  anywhere.

## 4. No real orders

- Any HTTP call to a broker, exchange, or order endpoint. **C-1**: managing another person's
  assets with discretion needs a licence and 75,000 euro of capital; **C-6**: operating
  without one is a criminal offence under § 54 KWG.
- A `place_paper_order` that stops being clearly paper — a config flag that would route it
  somewhere real, a "live mode", a TODO about execution.
- Copy trading, signal following, or publishing one user's calls to others (**C-3**, **C-4**).
- A page or response missing the notice that this is information, not investment advice
  (**C-5**).

The brief names the warning sign precisely: the first line of code that calls an order
endpoint. Treat it as a blocker, not a discussion.

## 5. Model output is untrusted input

**A-4**: every value from the agent is treated as hostile until validated (**N-2**).

- An annotation stored without passing the generated validator (**N-55**). The schema the
  model is given and the schema we check against come from one definition; a hand-written
  check beside it will drift.
- A model-supplied timestamp or id used instead of the server's (**D-5**, **I-2**).
- A price far from the last close being *rejected*. It is stored, flagged and counted, so the
  threshold can be tuned from real data (**F-23**, **C-20**). Rejecting it throws away the
  only evidence that would set the threshold.
- Model text rendered as raw HTML (**N-44**). In the frontend this is Svelte's `{@html}`;
  there should be zero occurrences in the repository.
- A rejection that returns a stack trace rather than a reason the model can act on (**I-3**).
- A raw payload discarded because the annotation was rejected. Those are the payloads worth
  keeping most (**D-14**).

## 6. One implementation, not two

- A second set of tool handlers, or a branch inside one asking whether MCP or the hosted
  runner is calling. **F-96** means parity of writes and validation is structural; a mode
  check is where it starts to drift.
- A capability added for the hosted runner that is not reachable over MCP in the same release.
- Chat and the daily run assembling their own context. **F-18** exists so they cannot
  contradict each other by having seen different data.
- A forced tool choice (`tool_choice` of `any` or a named tool). Several current models reject
  it outright, and which ones is not something a config change should be able to break
  (**I-35**). Automatic choice, the tool named in the instruction, strict schema on.

## 7. Idempotency and the calendar

- A nightly-job write without `ON CONFLICT DO NOTHING` against its natural key (**D-13**).
  **F-101** is a property of the schema, not something the job remembers.
- A holiday, a halted ticker or a missing price treated as a failure. A run on Thanksgiving
  completes and records no fills (**N-3**).
- Trading days computed at run time instead of read from `trading_calendar` (**D-15**).
- A snapshot skipped on a day with no activity. Fourteen trading days produce fourteen rows
  whatever the agent did (**F-80**).

## 8. Cost

- A model call on a page read. **N-11**: reading any page triggers no model call.
- The daily cap checked after a run rather than before it. **N-34**: a capped user produces no
  model spend at all that day.
- A request that could exceed 100,000 input tokens (**N-33**). Past that line the price per
  token multiplies on the standard tier, so this is a cost control, not a technical limit.
- A model call that is not recorded in `model_call` with its tokens and cost. Without it the
  §11 box "the cost of one user for one month is a known number" cannot be ticked.

## Commands

```
# append-only violations
grep -rniE "(UPDATE|DELETE FROM) +(analysis|annotation|ledger_entry|news_take|raw_model_output|call_verdict)" --include=*.rs --include=*.sql .

# trading_day passed as input rather than derived
grep -rn "trading_day" --include=*.rs crates/ | grep -iE "bind|insert|param"

# float money
grep -rnE "\bf(32|64)\b" --include=*.rs crates/watcher-core crates/watcher-ledger crates/watcher-store

# forbidden personal data
grep -rniE "income|savings|net_worth|risk_tolerance|investment_goal|suitability|questionnaire" --include=*.rs --include=*.sql --include=*.svelte .

# anything that looks like real execution
grep -rniE "broker|live_?mode|execute_order|submit_order|order_endpoint" --include=*.rs .

# raw HTML of model output
grep -rn "@html" frontend/src

# forced tool choice
grep -rn "tool_choice" --include=*.rs crates/ | grep -v auto
```

A clean grep is not a pass. These catch the obvious shape; read the diff for the rest.

## How to report

Most serious first, as a list:

```
<file>:<line> — <blocker | should fix | nit>: <what is wrong, which invariant it breaks, and what it costs>
Fix: <the change, one line>
```

Sections 3 and 4 are blockers by default — they change what the product legally is, and that
is not a judgement call to make in a code review. Everything else takes its severity from
what it actually breaks.

If the change is clean, say so in one sentence and name the two or three invariants you
checked hardest. Do not invent findings to fill a list.
