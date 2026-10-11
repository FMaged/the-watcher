-- 0004_market_data.sql — prices, corporate actions and news.
--
-- Everything here is insert-only in practice. 0001's default privileges already give the
-- application SELECT and INSERT and nothing more, which is exactly right: a bar is fetched
-- once and never revised, and a provider that changes its mind produces a new row for a new
-- date rather than an edit to an old one. Nothing in this file belongs on 0009's grant list.
--
-- These tables deliberately have no foreign key to `trading_calendar`, even though a bar
-- should only exist on a session. The calendar is seeded from 2026 (D-15) while the first
-- chart needs at least a year of history behind it (F-30), so bars will predate the seed on
-- the day the system is first filled. A constraint that made historical loading impossible
-- would be a constraint we removed under pressure.

CREATE TABLE price_bar (
  instrument_id bigint NOT NULL REFERENCES instrument (id),
  bar_date      date   NOT NULL,
  open_minor    bigint NOT NULL,
  high_minor    bigint NOT NULL,
  low_minor     bigint NOT NULL,
  close_minor   bigint NOT NULL,
  volume        bigint NOT NULL,
  fetched_at    timestamptz NOT NULL DEFAULT now(),

  -- The natural key is the whole identity of a bar, so there is no surrogate id: one fewer
  -- column and one fewer way to insert the same day twice. It also makes the nightly
  -- ingest's ON CONFLICT DO NOTHING (F-101) fall out of the schema.
  PRIMARY KEY (instrument_id, bar_date),

  CONSTRAINT price_bar_prices_positive CHECK (
    open_minor > 0 AND high_minor > 0 AND low_minor > 0 AND close_minor > 0
  ),
  CONSTRAINT price_bar_volume_not_negative CHECK (volume >= 0),
  -- A provider glitch that returns a high below the low, or a close outside the day's range,
  -- is a bar we must refuse rather than store. I-12 says a failure is retried and then
  -- recorded; it never writes partial or guessed data, and a nonsense bar is guessed data.
  CONSTRAINT price_bar_range_coherent CHECK (
    high_minor >= low_minor
    AND high_minor >= open_minor AND high_minor >= close_minor
    AND low_minor  <= open_minor AND low_minor  <= close_minor
  )
);

-- A bar cannot exist for a day that has not closed yet. Nothing in the constraints above
-- catches it: they check prices against each other, not against the calendar.
--
-- This matters more than it looks. A future-dated bar becomes the "last close" that F-23's
-- annotation warning threshold is measured against, and it appears on the chart as fact. The
-- date also cannot be checked with a CHECK constraint — `current_date` is STABLE, not
-- IMMUTABLE, and Postgres refuses it there (see doc/database.md) — so this is a trigger,
-- which is stronger anyway: a check refuses a bad write, a trigger makes one impossible.
CREATE FUNCTION reject_future_bar() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
  -- The New York date, because that is the calendar the bars belong to (D-4).
  IF NEW.bar_date > (now() AT TIME ZONE 'America/New_York')::date THEN
    RAISE EXCEPTION 'price bar for % is in the future', NEW.bar_date;
  END IF;
  RETURN NEW;
END $$;

CREATE TRIGGER price_bar_not_in_the_future BEFORE INSERT ON price_bar
  FOR EACH ROW EXECUTE FUNCTION reject_future_bar();

COMMENT ON TABLE price_bar IS
  'Raw OHLCV, exactly as the provider gave it. I-11: adjusted prices are never the source of '
  'truth, because they are recomputed whenever a split or dividend happens — a stored '
  'adjusted price silently rewrites what the chart showed last month, which is the one thing '
  'this product exists not to do. Splits are applied as explicit ledger entries instead '
  '(F-70). There is deliberately no adjusted_close column anywhere.';

COMMENT ON COLUMN price_bar.fetched_at IS
  'When we saw this bar, not when it traded. Two bars for the same day can never coexist, so '
  'this is the audit trail for when the record was filled rather than a version number.';

CREATE TABLE corporate_action (
  id            text   PRIMARY KEY,
  instrument_id bigint NOT NULL REFERENCES instrument (id),
  kind          text   NOT NULL,

  -- A 4-for-1 split is from 1 to 4. Null for a dividend.
  split_from    integer,
  split_to      integer,

  -- Cash per share in minor units (D-3). Null for a split.
  amount_minor  bigint,

  ex_date       date   NOT NULL,
  -- Dividends only. A split has no pay date, which matters for the ledger — see below.
  pay_date      date,
  fetched_at    timestamptz NOT NULL DEFAULT now(),

  CONSTRAINT corporate_action_kind CHECK (kind IN ('split', 'dividend')),
  -- Each kind carries its own fields and none of the other's, so a half-filled row cannot be
  -- stored and then be ambiguous to read.
  --
  -- The IS NOT NULL tests are not redundant, and leaving them out is a bug this file had for
  -- one test run. A CHECK only rejects a row when it evaluates to FALSE: with split_from
  -- NULL, `split_from > 0` is NULL, the whole AND-chain is NULL, and the row is accepted. A
  -- split with no ratio got in. Any CHECK over a nullable column needs the null case stated
  -- explicitly.
  CONSTRAINT corporate_action_split_fields CHECK (
    kind <> 'split' OR (
      split_from IS NOT NULL AND split_to IS NOT NULL
      AND split_from > 0 AND split_to > 0
      AND amount_minor IS NULL AND pay_date IS NULL
    )
  ),
  CONSTRAINT corporate_action_dividend_fields CHECK (
    kind <> 'dividend' OR (
      amount_minor IS NOT NULL AND pay_date IS NOT NULL
      AND amount_minor > 0
      AND split_from IS NULL AND split_to IS NULL
    )
  ),
  CONSTRAINT corporate_action_pay_after_ex CHECK (pay_date IS NULL OR pay_date >= ex_date),
  -- One action of a kind per instrument per ex-date, so re-fetching is idempotent.
  CONSTRAINT corporate_action_natural_key UNIQUE (instrument_id, kind, ex_date)
);

COMMENT ON COLUMN corporate_action.pay_date IS
  'NOTE FOR MIGRATION 0006. D-11 gives corporate-action ledger entries one exception to the '
  'derived trading day: they take their date from the action, because a dividend is credited '
  'on its pay date (F-71) and may be processed later. But a split has no pay date, so that '
  'trigger must read coalesce(pay_date, ex_date) — reading pay_date alone would try to write '
  'NULL into a NOT NULL column and every split would fail to apply.';

CREATE TABLE news_item (
  id            text   PRIMARY KEY,
  instrument_id bigint NOT NULL REFERENCES instrument (id),
  headline      text   NOT NULL,
  source        text   NOT NULL,
  url           text   NOT NULL,
  published_at  timestamptz NOT NULL,
  fetched_at    timestamptz NOT NULL DEFAULT now(),

  -- The same story fetched twice is one row, so the nightly fetch is idempotent without the
  -- job having to remember what it already has.
  CONSTRAINT news_item_natural_key UNIQUE (instrument_id, url),
  CONSTRAINT news_item_headline_not_blank CHECK (length(btrim(headline)) > 0)
);

COMMENT ON TABLE news_item IS
  'I-21: headline, source, timestamp and URL only. Full article text is never stored — there '
  'is no column for it, because what the provider terms allow is not something to re-check '
  'later against a table that already holds the text. F-42 shows a 14-day window, which is a '
  'query concern rather than a retention one.';

-- F-40 reads newest first; F-42 bounds it to the last 14 days. One index serves both.
CREATE INDEX news_item_recent ON news_item (instrument_id, published_at DESC);

-- The chart asks for one instrument over a date range (F-30), which the primary key already
-- serves in the right order. No second index earns its write cost here.
