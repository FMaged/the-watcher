---
name: rust-review
description: Use when reviewing Rust code — an axum handler, an sqlx query, a trait implementation, an async function, a crate boundary, or a whole diff (e.g. "review this handler", "check my sqlx query", "why does this deadlock", "look at this crate"). Reports what panics in production, what blocks the runtime, what the type system was supposed to prevent, and where a crate reached past its boundary.
---

# Rust review

Review Rust. Report problems. Do not rewrite unless asked.

For project-specific rules — append-only tables, integer money, no real orders — use the
`watcher-invariants` skill as well. This one is about the language and the stack.

## 1. Panics in a request path

A panic in a handler takes the task down and returns a bare 500 with no reason. In a nightly
job it loses the night.

- `unwrap()` or `expect()` on anything that depends on input, the network, the database, or
  the clock. In `main.rs` at startup they are fine and arguably better — fail loudly before
  serving.
- Indexing a slice or map (`v[0]`, `map[&k]`) where the length or key came from outside.
- Integer arithmetic on money that can overflow. `checked_add` / `checked_mul` where a wrong
  answer would be written to an append-only ledger.
- `unreachable!()` guarding a case the type system does not actually prevent.
- A `catch_unwind`-shaped workaround instead of fixing the panic.

## 2. Errors that say nothing

- One error type for the whole application, so a caller cannot tell "not found" from
  "provider timed out" and the HTTP layer maps everything to 500.
- `anyhow` in a library crate's public signature. `anyhow` is for the binary and the top of
  the call stack; a library returns a type the caller can match on (`thiserror`).
- An error logged and then swallowed, so the caller continues with a default.
- `?` across a boundary where the `From` conversion loses the context that made the error
  diagnosable.
- An error message that leaks a connection string, a key, or a full SQL statement. Secrets
  never reach the logs.

## 3. sqlx

- `query!` / `query_as!` not used where it could be. The compile-time check is most of the
  reason to pick sqlx, and it needs `cargo sqlx prepare` committed so CI can build offline.
- A query built by string concatenation with any value that came from outside. Bind it.
- A loop issuing one query per iteration where a single `WHERE id = ANY($1)` would do. Fifty
  tickers times a year of bars is where this stops being theoretical.
- Multi-statement work with no transaction, so a failure halfway leaves half the change.
- A transaction held open across an HTTP call or a model call. The connection is pinned for
  the whole round trip and the pool runs out.
- `fetch_all` on something unbounded. A year of bars is fine; "all analyses ever" is not.
- A pool created per request instead of once and shared through state.
- No statement timeout, so one slow query holds a connection until something else gives up.
- A migration edited after it has run anywhere. See the `migration-review` skill.

## 4. axum

- Shared state rebuilt per request rather than held in `State` — especially an HTTP client,
  which keeps its own connection pool and is meant to be cloned, not recreated.
- A handler returning `Result<_, E>` where `E` has no `IntoResponse`, so the mapping happens
  by accident.
- No body size limit on a POST. The default is generous and the chat endpoint is reachable
  from a browser.
- Extractor order wrong — a body-consuming extractor not last.
- A long operation held in a handler with no timeout, so a hanging provider holds the
  connection. Model calls have a timeout and a retry limit by requirement (**I-32**).
- Routes that differ only by auth, with the auth applied per handler rather than by layer —
  the one you forget is the one that matters.
- `#[debug_handler]` left on in release code, or missing while you are actually debugging a
  trait-bound error (it is the fastest way to read those).

## 5. Async

- Blocking work on the async runtime: file I/O, `std::thread::sleep`, a CPU-bound loop, a
  synchronous HTTP or database client. Use `spawn_blocking`, or do not be async there.
- A `std::sync::Mutex` held across an `.await`. Either use the async mutex or restructure so
  the lock is not held over a yield point.
- A `tokio::spawn` whose `JoinHandle` is dropped, so a panic inside it vanishes silently.
  This is how a nightly job appears to succeed while having done nothing.
- Work that is not cancellation-safe sitting inside `select!`.
- `join_all` over an unbounded set, hitting a provider with fifty concurrent requests and
  earning a rate limit. Bound the concurrency.
- An outbound HTTP call with no timeout at all. The default in most clients is "forever".

## 6. Crate boundaries

The dependency graph in `doc/architecture.md` runs one way, and it is what makes N-50 and
N-54 checkable by reading `Cargo.toml` instead of trusting a review.

- `watcher-core` gaining `sqlx`, `reqwest` or `tokio`. It holds types and traits and does no
  I/O.
- `watcher-ledger` gaining any dependency beyond `watcher-core` — in particular a clock. Time
  arrives as an argument, which is what lets its tests run with no database (**N-56**).
- `watcher-feeds` or `watcher-llm` reaching into analyses, orders or pages (**N-54**).
- `watcher-agent` depending on axum. It knows nothing about HTTP.
- Anything depending on `watcher-api`.
- A type defined twice in two crates because moving it to `core` felt like a bigger change.

## 7. Types doing less work than they could

- A `String` or `i64` where a newtype would make a wrong call impossible — a ticker, a money
  amount, a trading day, an id. These are cheap and they catch argument-order mistakes that
  tests do not.
- `Option<T>` threaded through many layers where the invalid case could have been excluded at
  the boundary.
- A `bool` parameter at a call site, where the reader cannot tell what `true` means.
- `clone()` on a large owned value in a loop, where a borrow would do. Not a correctness
  problem; worth a nit when it is in a hot path.
- `as` casts between integer widths where the value could truncate. `try_into` and handle it.

## 8. Tests

- A rule with no test, where the rule is the product. The ledger cases are named in **N-53**.
- A test that needs a database to check something pure. If it does, the logic is in the wrong
  crate.
- A test asserting on a formatted string rather than the value, so it breaks on wording.
- No test for the rejection paths. Most of `place_paper_order` is rejections, each with its
  own reason (**I-3**).
- `#[ignore]` with no comment saying what would un-ignore it.

## Commands

```
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
cargo test --workspace
cargo sqlx prepare --check --workspace     # the offline query cache is current
cargo tree -i tokio -p watcher-ledger      # should find nothing
cargo tree -i sqlx  -p watcher-core        # should find nothing
cargo deny check                           # licences and advisories
grep -rn "unwrap()\|expect(" --include=*.rs crates/ | grep -v "^crates/.*/tests/"
```

The two `cargo tree -i` lines are the cheapest boundary check there is: if they return a
path, the dependency direction has been broken and no amount of review will keep it straight.

## How to report

Most serious first, as a list:

```
<file>:<line> — <blocker | should fix | nit>: <what is wrong and what it does at run time>
Fix: <the change, one line>
```

Separate "this panics on bad input" from "this clones more than it needs to". Both are worth
saying, not at the same volume.

If the code is fine, say so in one sentence and name the two or three things you checked
hardest. Do not invent findings to fill a list. If you add a comment as part of a fix, follow
the `code-comments` skill.
