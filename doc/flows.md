# Flows

How a day actually passes. Order lifecycle and verdict rules live in
[`ledger.md`](ledger.md).

## The nightly job

Fires after the US close. Idempotent by construction (**F-101**), and it recovers on its
own: a day missed yesterday is written today, because the only thing stopping a write is a
row already being there (**N-22**).

```mermaid
sequenceDiagram
    autonumber
    participant S as scheduler
    participant J as nightly job
    participant F as feeds
    participant L as ledger
    participant D as postgres
    S->>J: fire after US close
    J->>D: is today in trading_calendar?
    alt market holiday or weekend
        D-->>J: no session
        J->>D: record run, outcome "skipped"
        Note over J,D: N-3 — completes, no fills, not a failure
    else trading day
        J->>F: fetch bars for every active instrument
        alt provider fails
            F-->>J: error
            J->>D: record a failed fetch
            Note over J,D: I-12 — retried, then recorded.<br/>Never a guessed bar.
        else bars returned
            F-->>J: OHLCV
            J->>D: INSERT price_bar ON CONFLICT DO NOTHING
        end
        J->>L: fill pending orders at today's open
        L-->>J: events
        J->>D: INSERT ledger_entry, natural key
        J->>L: apply splits and credit dividends
        J->>D: INSERT ledger_entry, day from the action
        loop per ticker
            J->>J: analysis run (next diagram)
        end
        J->>D: compute verdicts, append call_verdict
        J->>D: INSERT snapshot, PK (portfolio, trading_day)
        Note over J,D: F-80 — one snapshot per trading day,<br/>including days with no activity
    end
    J->>D: finalise run outcome
```

**F-13** (partial failure) is the `loop per ticker` step: one ticker failing records a
failure and the other nineteen still complete. A run over twenty tickers where one symbol
has no data stores nineteen analyses and one recorded failure.

## A hosted run — single shot

One context pack in, one answer out, through the provider's asynchronous batch route
(**F-97**, **I-33**). Nobody is waiting on a job that starts after the close, and the batch
discount is large.

```mermaid
sequenceDiagram
    autonumber
    participant J as nightly job
    participant A as "context pack"
    participant M as "model (batch)"
    participant V as validator
    participant D as postgres
    J->>D: spend so far today vs DAILY_COST_CAP_CENTS
    alt at or over cap
        D-->>J: capped
        J->>D: record run, outcome "capped", user told
        Note over J,D: N-34 — checked BEFORE the run.<br/>A capped user spends nothing at all.
    else under cap
        J->>A: build pack for this ticker
        A->>D: bars, news, portfolio, past analyses
        A->>A: measure prefix, cap pack at min(90k, 100k − prefix)
        A-->>J: pack + what was dropped, logged
        J->>M: submit batch request
        Note over J,M: tool choice AUTO + strict schema.<br/>Never forced — I-35.
        M-->>J: record_analysis, optionally place_paper_order
        J->>D: INSERT raw_model_output — always, accepted or not
        J->>V: validate annotations
        V-->>J: accepted / flagged / rejected
        J->>D: INSERT analysis + annotation
        J->>D: resolve order's run id → that run's analysis for this ticker
        J->>D: INSERT model_call, tokens and cost
    end
```

The order carries a **run id**, not an analysis id (`PLAN.md` D9). A single-shot agent
emits its analysis and its order in the same turn and cannot cite an id the server has not
assigned yet. The server resolves run + ticker to the one analysis that run produced.

## A connected agent — over MCP

The user's own assistant, paying with its own tokens. It can explore across several calls,
which makes it **more** capable than the hosted runner, not less — and **F-97** obliges us
to say so plainly rather than imply parity.

```mermaid
sequenceDiagram
    autonumber
    participant U as "user's assistant"
    participant MC as "MCP transport"
    participant H as "tool handlers"
    participant D as postgres
    U->>MC: connect with bearer token
    MC->>MC: resolve token → user, or 401
    U->>MC: list_watchlist
    MC->>H: the same handlers the hosted runner calls
    H->>D: read, scoped to that user
    H-->>U: tickers
    loop explores as it chooses
        U->>MC: get_prices / get_news / get_portfolio
        H-->>U: data
    end
    U->>MC: record_analysis
    H->>H: validate against the generated schema
    H->>D: INSERT, trading_day set by trigger
    H-->>U: the stored object, with server id and timestamp
    Note over H,U: I-2 — write tools return what was stored
    U->>MC: place_paper_order with run id
    alt fails validation
        H-->>U: a reason the model can act on
        Note over H,U: I-3 — not a stack trace
    else accepted
        H->>D: INSERT paper_order, state pending
        H-->>U: the stored order
    end
```

### Why there is one set of handlers

**F-96** requires that the hosted agent has exactly the tools a user's own agent has, with
the same validation. One implementation makes that structural — there is no second code
path that could drift.

```mermaid
flowchart LR
    MCP["MCP transport<br/>(user's assistant)"] --> H
    HOST["hosted runner<br/>(in-process)"] --> H
    H["the six tool handlers<br/>watcher-agent"] --> V["validation"]
    V --> S["watcher-store"]
    H -.->|"never"| H2["a second implementation"]
    style H2 stroke-dasharray: 5 5
```

Parity is a claim about **what may be written and what is checked**. It is not a claim
about reach: a hosted run never calls the four read tools, because the context pack already
supplies what they would have returned.

## Annotation validation

Three outcomes, not two. A price far from the last close is **stored and flagged**, never
rejected — the threshold starts at 25% and is a value chosen to be revised (**C-20**), which
is only possible because every raw payload is kept (**D-14**).

```mermaid
flowchart TD
    P["model returns an annotation"] --> R["store raw_model_output<br/>always, first"]
    R --> K{"kind is one of six?"}
    K -->|"no"| X["REJECT<br/>run records a validation error"]
    K -->|"yes"| S{"structurally valid?"}
    S -->|"no"| X
    S -->|"yes"| N{"has a reason?"}
    N -->|"no"| X
    N -->|"yes"| F{"price within<br/>ANNOTATION_WARN_PCT<br/>of last close?"}
    F -->|"no"| FL["STORE, flagged = true<br/>drawn with the flag visible<br/>counted so the threshold can be tuned"]
    F -->|"yes"| OK["STORE"]
```

"Structurally valid" means: a positive numeric price, a zone whose low is not above its
high, a date present on `breakout` and `event`, and no date in the future (**F-23**).

The schema the model is given and the schema the validator checks are **generated from one
definition** in `watcher-core` (**N-55**), so they cannot disagree. An unknown kind is a
deserialization error before any of this runs.

## Chat

Grounded in what is already stored. Reading a page never costs money (**N-11**); asking a
question does, which is why it is capped.

```mermaid
sequenceDiagram
    autonumber
    participant B as browser
    participant API as watcher-api
    participant D as postgres
    participant M as model
    B->>API: POST /api/ticker/AAPL/chat
    API->>D: messages sent today vs CHAT_CAP_PER_DAY
    alt over cap
        API-->>B: clear message, resets tomorrow
        Note over API,B: F-53
    else under cap
        API->>D: stored analysis, prices, positions
        Note over API,D: F-51 — never a fresh run
        API->>M: the SAME context pack builder, synchronous
        Note over API,M: F-18 — one assembler, so chat and the<br/>page cannot contradict each other
        M-->>API: answer
        API->>D: INSERT chat_message, INSERT model_call
        API-->>B: answer
    end
```

### What chat must refuse

**F-54** and **C-2**: the assistant refuses questions asking what the user personally should
do with their money, and never asks for personal financial circumstances. "Should I put my
savings into this?" gets a refusal plus general information.

This is not a politeness setting. A personal recommendation on a named instrument, based on
personal circumstances, is Anlageberatung under § 1 Abs. 1a S. 2 Nr. 1a KWG, and BaFin
states that automating it changes nothing. There is also no column to store the answer in
(see `entities.md`).

## Manual run

**F-11** lets the user trigger a run for one ticker or the whole watchlist. It uses the same
pack and the same handlers, synchronously rather than through batch, and is recorded with
`trigger = 'manual'`.

```mermaid
flowchart LR
    U["POST /api/runs"] --> C{"cost cap?"}
    C -->|"over"| X["refused, told why"]
    C -->|"under"| R["same pack, same handlers,<br/>synchronous not batch"]
    R --> A["INSERT analysis,<br/>trigger = manual, version + 1"]
    A --> P["page shows it — newest wins, F-15"]
    A -.->|"excluded"| SC["scoring, F-17"]
```

Ten manual reruns in one day do not change the track record for that day (**F-17**), and
tomorrow's scheduled run replaces the view naturally — there is no decay rule to write
(closed **O-8**).
