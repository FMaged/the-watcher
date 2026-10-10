-- 0002_core.sql — the tables everything else hangs from.
--
-- Two names differ from the entity names in the requirements, because `user` and `order` are
-- SQL reserved words: `app_user` and, later, `paper_order` (doc/entities.md).
--
-- No UPDATE or DELETE is granted here. Migration 0001 set default privileges to
-- SELECT, INSERT only, so every table below starts append-only and the few that genuinely
-- need to change rows are granted explicitly in 0009. The tables that will need UPDATE there:
-- `portfolio` (cash), `watchlist_item` (removed_on), `api_token` (revoked_at),
-- `instrument` (listed). And one revoke: `trading_calendar`, which is seed-only.

CREATE TABLE app_user (
  id         text PRIMARY KEY,
  label      text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE app_user IS
  'One row in v1. There is no login yet (PLAN.md D14), but every record is attributed to a '
  'user from the start (D-1) so that adding login later is not a data migration. There is '
  'deliberately no column for income, savings, risk tolerance or investment goals: D-8 and '
  'C-2 mean the schema must make storing them impossible, not merely discouraged.';

CREATE TABLE api_token (
  id         text PRIMARY KEY,
  user_id    text NOT NULL REFERENCES app_user (id),
  -- N-41. The token itself is shown once, at creation, and never stored.
  token_hash text NOT NULL UNIQUE,
  label      text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  revoked_at timestamptz
);

CREATE INDEX api_token_user ON api_token (user_id) WHERE revoked_at IS NULL;

CREATE TABLE instrument (
  id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  ticker     text NOT NULL UNIQUE,
  name       text NOT NULL,
  -- A delisted instrument stays here; F-3 keeps history for anything the agent said about it.
  listed     boolean NOT NULL DEFAULT true,
  created_at timestamptz NOT NULL DEFAULT now(),
  -- The same shape `Ticker` in watcher-core enforces, but the two are deliberately not
  -- identical. The Rust type is lenient at the edge and normalises: ' aapl ' becomes 'AAPL',
  -- with a readable rejection for anything it cannot normalise (I-3). This constraint is
  -- strict and refuses lowercase outright, because by the time a value reaches the database
  -- it has already been through that normalising — so lowercase here means a code path
  -- bypassed the type, which is exactly what should fail.
  CONSTRAINT instrument_ticker_shape CHECK (ticker ~ '^[A-Z]{1,5}(\.[A-Z]{1,2})?$')
);

CREATE TABLE watchlist_item (
  user_id       text   NOT NULL REFERENCES app_user (id),
  instrument_id bigint NOT NULL REFERENCES instrument (id),
  added_on      date   NOT NULL DEFAULT current_date,
  -- F-3: removing a ticker hides it from the watchlist and keeps every analysis, annotation
  -- and trade. So removal sets a date; it never deletes a row.
  removed_on    date,
  PRIMARY KEY (user_id, instrument_id),
  CONSTRAINT watchlist_item_removed_after_added CHECK (removed_on IS NULL OR removed_on >= added_on)
);

CREATE INDEX watchlist_item_active ON watchlist_item (user_id) WHERE removed_on IS NULL;

COMMENT ON TABLE watchlist_item IS
  'F-2 caps a watchlist at 50 active tickers. That limit is enforced by the application '
  'rather than a trigger, because the requirement asks for a refusal that explains why and a '
  'constraint violation explains nothing.';

CREATE TABLE portfolio (
  id         text   PRIMARY KEY,
  -- One portfolio per user (F-60), so the foreign key is unique.
  user_id    text   NOT NULL UNIQUE REFERENCES app_user (id),
  cash_minor bigint NOT NULL,
  created_on date   NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  -- No shorting and no margin in v1 (F-68), so cash cannot go below zero.
  CONSTRAINT portfolio_cash_not_negative CHECK (cash_minor >= 0)
);

COMMENT ON COLUMN portfolio.cash_minor IS
  'Cents. D-3: money is an integer count of minor units, never a float, because a float cent '
  'error in an append-only ledger cannot be corrected, only reversed.';

-- The single v1 user and their portfolio. This is seeded rather than created at runtime
-- because v1 is deliberately single-user (PLAN.md D14) and the alternative is a first-run
-- code path that exists only to insert two rows.
--
-- F-60: a new portfolio always starts at exactly 10,000 USD. That is 1,000,000 cents.
INSERT INTO app_user (id, label) VALUES ('us_0000000000000000000000001', 'owner');

INSERT INTO portfolio (id, user_id, cash_minor, created_on)
VALUES ('pf_0000000000000000000000001', 'us_0000000000000000000000001', 1000000, current_date);
