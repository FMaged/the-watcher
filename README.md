# The Watcher

A workspace for people who follow a handful of stocks. An AI agent reads the watchlist every
day, marks the chart with where it would buy and sell, explains the news, and answers
questions about what it sees. It trades on paper, so every call it makes is scored against the
market over time. You act in your own broker app.

The product is the visible reasoning and the honest scoreboard. The agent beating the market
is explicitly **not** a goal — it probably will not, most of the time, and the record says so.

## Not investment advice

This is information, not advice. It places no real orders, holds no money or securities, and
manages nobody's assets. It never asks for your income, savings, risk tolerance or investment
goals, and there is no field anywhere in the schema to store them — collecting those and then
naming an instrument is what makes a product a licensed one.

## Status

Early. The planning documents are complete and the first three tasks are done.

| Done | |
|---|---|
| T1.1 | Requirements at v1.4 |
| T1.2 | Cargo workspace, seven crates with one-way dependencies |
| T1.3 | Compose stack, configuration with fail-fast validation |
| T1.4 | Two database roles, migration 0001, the D-12 privilege check |
| T1.5 | Core types: money, trading days, tickers, sides, shares |
| T1.6 | Core tables and the seeded 2026-2030 trading calendar |
| T1.7 | Market data tables: prices, corporate actions, news |

**What the binary does today:** reads its configuration, validates it, prints one line and
exits. There is no HTTP server yet (T1.9), no database schema (T1.4) and no agent (Stage 2).
See [`TASKS.md`](TASKS.md) for what is next.

The stack has been built and run: the image is 127MB and runs as uid 10001, `initdb` creates
the restricted role, all three migrations apply, and Caddy routes `/api`, `/health` and `/mcp`
to the binary (502 until T1.9) while serving the frontend volume at `/`. What has *not* been
proven is the VPS of C-18 — this was a laptop — and the restore drill of N-21.

## Running the containers

You need Docker with a running daemon, and Compose v2.

```bash
cp deploy/.env.example deploy/.env
$EDITOR deploy/.env          # set POSTGRES_PASSWORD and WATCHER_APP_PASSWORD
```

The env file lives in `deploy/`, beside the compose file, and not at the repository root.
Compose reads `.env` from the compose file's own directory for variable interpolation, so a
root env file would quietly reach the containers while leaving `${POSTGRES_PASSWORD}`
unsubstituted.

Check the configuration is accepted:

```bash
cd deploy
docker compose run --rm watcher
```

That prints a startup line and exits 0, or names the first variable that is wrong and exits 1.

**Do not use `docker compose up` for the watcher service yet.** The binary is not a server
until T1.9, so it exits as soon as it has printed, and `restart: unless-stopped` will start it
again immediately — a restart loop that looks like a fault and is not. The database and proxy
are fine to bring up on their own, and Caddy deliberately does not depend on the API so that
this really does leave the watcher service alone:

```bash
docker compose up -d postgres caddy
docker compose ps
docker compose logs -f
docker compose down            # add -v to also drop the database volume
```

### What the stack is

| Service | Role |
|---|---|
| `postgres` | PostgreSQL 16. Not published — nothing outside the network reaches it |
| `watcher` | The Rust binary. Not published either |
| `caddy` | The only way in. Terminates TLS, serves the frontend build, proxies `/api` |

Only Caddy publishes ports. That is deliberate: it is what stops the basic auth added in T3.7
from being bypassed by calling the API directly. Set `WATCHER_DOMAIN` to a real hostname and Caddy issues a
certificate for it automatically.

Left as `localhost` it still redirects HTTP to HTTPS and serves a certificate from Caddy's own
local CA, so `curl` needs `-k` and a browser will warn once. There is no plain-HTTP mode.

The `/mcp` route is excluded from that basic auth on purpose — a connected assistant
authenticates with its own token, and a proxy-level challenge would refuse it before that
token is ever read.

## Running it without containers

Against a local PostgreSQL cluster, provision the roles once — passwords passed in rather
than written into a file the repository tracks:

```bash
sudo -u postgres psql \
  -v owner_pw="$(openssl rand -base64 24)" \
  -v app_pw="$(openssl rand -base64 24)" \
  -f deploy/bootstrap-local.sql
```

Put those two passwords into `deploy/.env` as `MIGRATE_DATABASE_URL` and `DATABASE_URL`, with
`@localhost` in place of `@postgres`, then:

```bash
set -a; . deploy/.env; set +a
cargo run
```

Nothing loads `.env` automatically — the configuration comes from the environment and only
from the environment, so that no secret can arrive from a file someone forgot was there.

## Commands

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check

# the crate boundaries, which are an architectural requirement rather than a preference
cargo tree -i tokio -p watcher-ledger
cargo tree -i sqlx  -p watcher-core
```

Read those two carefully, because right now a pass looks like a failure. Neither `tokio` nor
`sqlx` is a dependency of anything yet, so both commands exit 101 with
`package ID specification ... did not match any packages` — and that is the passing state.
Once those crates are real dependencies elsewhere in the workspace, a pass prints nothing and
a failure prints the chain that broke the boundary. Until then the check only tells you the
dependency is absent, which is a weaker thing than it looks.

## Layout

```
crates/
  watcher-core     types, money, trading days, annotation schema, provider traits. No I/O
  watcher-store    Postgres: migrations, repositories
  watcher-feeds    price and news providers
  watcher-llm      model clients
  watcher-ledger   the paper trading rules, as a pure state machine
  watcher-agent    context pack, prompts, tool handlers, verdicts, run orchestration
  watcher-api      axum router, JSON handlers, MCP transport, scheduler
src/main.rs        configuration and wiring, nothing else
deploy/            compose.yaml, Caddyfile, .env.example
doc/               diagrams, schema reference, flows
```

Dependencies run strictly one way, `core` to `api`, and nothing depends on `watcher-api`.
Each crate's `lib.rs` says what it may *not* depend on, so the rule travels with the code.

## Documents

Read in this order. Where they disagree, the one higher in this list wins.

| Document | What it is |
|---|---|
| [`stock-analyser-product-brief.md`](stock-analyser-product-brief.md) | Why this exists, who for, what would prove it works |
| [`stock-analyser-requirements.md`](stock-analyser-requirements.md) | What it must do. Every requirement has an id and an acceptance test |
| [`PLAN.md`](PLAN.md) | How it is built, and why each decision was made over the alternatives |
| [`doc/`](doc/README.md) | Diagrams, the schema in detail, how a day passes |
| [`TASKS.md`](TASKS.md) | The build order, and notes on what actually happened |

## Licence

None yet.
