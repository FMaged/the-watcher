---
name: migration-review
description: Use when writing or reviewing a database migration, a schema change, a grant, an index, or a trigger (e.g. "review this migration", "add a column to analysis", "is this safe to run on prod", "why did the migration fail"). Reports what cannot be undone, what takes a lock long enough to matter, what grant was forgotten, and what constraint Postgres will refuse to accept.
---

# Migration review

Review `migrations/*.sql`. Report problems. Do not rewrite unless asked.

Migrations are the one place where a mistake is permanent — forward-only means there is no
edit, only another migration on top. Schema detail for this project is in
`doc/database.md`; project rules are in the `watcher-invariants` skill.

## 1. Forward only

- A migration edited after it has run anywhere, including on a laptop. `sqlx` records a
  checksum; changing the file makes every environment that already ran it refuse to start.
  The fix is always a new migration.
- A migration that is not idempotent where it might be re-run — `CREATE TABLE` without
  `IF NOT EXISTS` in a seed or repair migration.
- Numbering that collides with another branch. Check what is already in `migrations/` before
  picking a number.
- A `DROP` of anything holding data, with no migration before it that copied the data out.
  `DROP COLUMN` is instant and unrecoverable.
- A destructive step and a safe step in the same file, so the safe part cannot be deployed
  without the risky one.

## 2. Locks and the nightly job

This project runs one job a night that must not be blocked (**F-10**, **N-22**). A migration
that takes an exclusive lock for a minute at the wrong moment is an outage.

- `ALTER TABLE ... ADD COLUMN` with a volatile default, which rewrites the whole table. A
  constant default has been cheap since Postgres 11; `now()` is not a constant.
- `ALTER COLUMN ... TYPE`, which rewrites and holds `ACCESS EXCLUSIVE` throughout.
- `CREATE INDEX` without `CONCURRENTLY` on a table with real volume — `price_bar` is the one
  that grows. `CONCURRENTLY` cannot run inside a transaction, so it needs its own migration.
- `SET NOT NULL` on an existing column without first adding a validated `CHECK`, which lets
  Postgres skip the full scan.
- Adding a foreign key without `NOT VALID` plus a later `VALIDATE CONSTRAINT`, so the
  validation scan happens while holding the lock.
- No `lock_timeout` set, so a migration queues behind a long read and then blocks everything
  behind itself.

## 3. Grants, and the ones that must be taken away

**D-12** is the mechanism behind **N-1**: the application role cannot rewrite the record.
§11 tests it directly, so a forgotten grant is a failed release.

- A new table created without the grant migration being re-run. Grants live last, in their
  own migration, precisely so that adding a table cannot silently leave it ungranted.
- A new append-only table granted `UPDATE` or `DELETE` to `watcher_app`. The revoke list
  covers `analysis`, `annotation`, `ledger_entry`, `news_take`, `raw_model_output` and
  `call_verdict`; a new one belongs on that list in the same migration that creates it.
- `GRANT ALL`, or a grant to `PUBLIC`.
- An object created by the app role rather than the owner, so ownership and therefore future
  DDL rights end up in the wrong place.
- A trigger function that runs as the invoker where it needs to be `SECURITY DEFINER`, or the
  reverse — a `SECURITY DEFINER` function with a mutable `search_path`.

## 4. Constraints Postgres will refuse

- A `CHECK` containing a `STABLE` function. The common case here is
  `(created_at AT TIME ZONE 'America/New_York')::date` — timezone conversion with a named
  zone is `STABLE`, not `IMMUTABLE`, and the migration is rejected outright. Use a
  `BEFORE INSERT` trigger that overwrites the value, which is stronger anyway: a check
  refuses a bad write, a trigger makes one impossible.
- A `CHECK` referencing another row or another table. It cannot.
- A unique constraint intended to be partial. `UNIQUE` has no `WHERE`; use a partial unique
  index.
- `NOT NULL` on a column with no default added to a table that already has rows.
- A generated column depending on something non-immutable.

## 5. Keys and idempotency

- A natural key that exists in the design but not in the schema. **F-101** and **D-13** mean
  the nightly job is idempotent *because* of
  `UNIQUE (portfolio_id, trading_day, kind, subject_id)` — without it, the job has to be
  careful instead, and it will not be at 3am on day nine.
- A surrogate id added where a natural key already identifies the row — `price_bar`,
  `snapshot`, `position`, `benchmark_basket`, `watchlist_item`. One more column and one more
  way to insert a duplicate.
- A unique index whose column order does not match how it is queried, so it cannot also serve
  as the lookup index.

## 6. Indexes

- An index with no query behind it. Every index costs write time and space; name the query.
- A missing index on a foreign key that gets deleted through, so the delete scans the child
  table.
- A composite index whose leading column is not the one filtered on.
- A redundant index — one already covered by the leading columns of another.
- No partial index where the query is always filtered the same way (`WHERE state = 'pending'`
  is the obvious one here).

## 7. Seed data

- Reference data loaded as a test fixture rather than a migration. The trading calendar is
  reference data: **N-3** and the derived trading day both need a calendar that tests can rely
  on and that cannot shift under them at run time (**D-15**).
- A seed that assumes an empty table, so re-running it duplicates rows.
- Environment-specific data in a migration — a real user, a real token, a hostname.

## 8. Types

- `float`, `real` or `double precision` for money. `bigint` minor units (**D-3**).
- `numeric` without precision where an integer would do.
- `timestamp` without time zone. Everything is `timestamptz` in UTC (**D-4**).
- `varchar(n)` with an arbitrary `n`. Use `text` with a `CHECK` if there is a real limit.
- An enum as a Postgres `enum` type, which is awkward to extend, where `text` with a `CHECK`
  is easy to change in a later migration.

## Commands

```
sqlx migrate info                          # what has run where
sqlx migrate run --database-url "$MIGRATE_DATABASE_URL"

# the D-12 revokes actually took effect
psql -c "\dp analysis"
psql -c "SET ROLE watcher_app; UPDATE analysis SET summary='x' WHERE false;"   # must fail

# what a statement will actually lock, before running it for real
psql -c "BEGIN; <the statement>; SELECT locktype, mode, relation::regclass FROM pg_locks WHERE pid = pg_backend_pid(); ROLLBACK;"

psql -c "\d+ ledger_entry"                 # constraints, indexes, triggers as built
```

The `SET ROLE` line is worth running after every migration that adds a table, not only after
one that changes grants. It is the §11 acceptance box, and it is one command.

## How to report

Most serious first, as a list:

```
<file>:<line> — <blocker | should fix | nit>: <what is wrong and what it costs to recover from>
Fix: <the change, one line>
```

Say plainly which findings are irreversible once the migration has run somewhere — those are
a different category from "this index is unnecessary", and the reader needs to know which
they are looking at before they merge.

If the migration is fine, say so in one sentence and name what you checked hardest. Do not
invent findings to fill a list.
