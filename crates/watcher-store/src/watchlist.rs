//! The watchlist: F-1, F-2 and F-3.
//!
//! Every query here takes a `user_id`. There is one user in v1 (`PLAN.md` D14), so the
//! parameter buys nothing today and is the point anyway: N-43 is tested by inserting a second
//! user and asserting nothing leaks, and a query that cannot see another user's rows is one
//! that never had the chance to.

use sqlx::postgres::PgPool;
use watcher_core::{Ticker, TradingDay};

/// F-2: a watchlist holds at most 50 tickers.
///
/// Enforced here rather than by a constraint or a trigger, because the requirement asks for a
/// refusal that *explains why* and a constraint violation explains nothing. Migration 0002
/// says the same thing in a comment on the table.
pub const MAX_ACTIVE: i64 = 50;

/// One ticker currently on the watchlist.
pub struct WatchedItem {
    pub ticker: Ticker,
    pub added_on: TradingDay,
}

/// What adding a ticker did.
///
/// Three outcomes rather than a `bool` or an error: F-1 and F-2 both ask for a refusal the
/// user can act on, and the difference between "already there" and "you are at the limit" is
/// the whole of the message.
#[derive(Debug, PartialEq, Eq)]
pub enum Added {
    Watched { added_on: TradingDay },
    AlreadyWatched,
    Full,
}

/// The active watchlist, alphabetically.
pub async fn active(pool: &PgPool, user_id: &str) -> Result<Vec<WatchedItem>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"
        SELECT i.ticker, w.added_on
          FROM watchlist_item w
          JOIN instrument i ON i.id = w.instrument_id
         WHERE w.user_id = $1
           AND w.removed_on IS NULL
         ORDER BY i.ticker
        "#,
        user_id
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            Ok(WatchedItem {
                ticker: parse_stored_ticker(&r.ticker)?,
                added_on: TradingDay::from_date(r.added_on),
            })
        })
        .collect()
}

/// The instrument id for a ticker, if the symbol is already known to trade.
///
/// `None` is not "invalid": it means nothing has confirmed this symbol yet. F-1 validates
/// against the instrument list, and the list is grown by [`confirmed`] when a provider
/// confirms a symbol it did not already hold.
pub async fn instrument_id(pool: &PgPool, ticker: &Ticker) -> Result<Option<i64>, sqlx::Error> {
    let row = sqlx::query!(
        "SELECT id FROM instrument WHERE ticker = $1",
        ticker.as_str()
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.id))
}

/// Record that a symbol trades, and return its instrument id.
///
/// `name` is the ticker. Nothing in the system knows a company's name yet — the price trait
/// returns bars and nothing else (I-10) — and the column is `NOT NULL`, so the choice is
/// between the symbol and an invented name. It is the symbol, in one documented place, until
/// something real can fill it.
///
/// `ON CONFLICT DO NOTHING` then a read, rather than `DO UPDATE`: an instrument row already
/// there is the normal case under a re-add, and nothing here has anything better to write
/// into it than what is already stored.
pub async fn confirmed(pool: &PgPool, ticker: &Ticker) -> Result<i64, sqlx::Error> {
    let inserted = sqlx::query!(
        r#"
        INSERT INTO instrument (ticker, name)
        VALUES ($1, $1)
        ON CONFLICT (ticker) DO NOTHING
        RETURNING id
        "#,
        ticker.as_str()
    )
    .fetch_optional(pool)
    .await?;

    match inserted {
        Some(row) => Ok(row.id),
        None => {
            let row = sqlx::query!(
                "SELECT id FROM instrument WHERE ticker = $1",
                ticker.as_str()
            )
            .fetch_one(pool)
            .await?;
            Ok(row.id)
        }
    }
}

/// How many tickers are active on the watchlist.
///
/// Public, rather than only used by [`add`], because the order of the checks matters to the
/// user: a full watchlist is a refusal that needs no provider call, and asking one about a
/// symbol that cannot be added either way spends someone else's free endpoint on nothing.
pub async fn active_count(pool: &PgPool, user_id: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "n!" FROM watchlist_item WHERE user_id = $1 AND removed_on IS NULL"#,
        user_id
    )
    .fetch_one(pool)
    .await
}

/// Add a confirmed instrument to the watchlist.
///
/// The count and the insert are two statements and so not atomic. That is deliberate rather
/// than overlooked: there is one user and one writer, and the case a transaction would have to
/// guard — the same ticker watched twice at once — is refused by the partial unique index no
/// matter who wins the race. Exceeding 50 by one under concurrency is a miscount, not a
/// corrupted record, and no amount of locking is worth paying for it here.
pub async fn add(pool: &PgPool, user_id: &str, instrument_id: i64) -> Result<Added, sqlx::Error> {
    // Checked again here even though the handler checks first: this is the function next to
    // the insert, so it is the one a later caller — an MCP tool, an import — cannot forget.
    if active_count(pool, user_id).await? >= MAX_ACTIVE {
        return Ok(Added::Full);
    }

    // Two conflicts are possible and they mean different things.
    //
    // The primary key `(user_id, instrument_id, added_on)` collides when the same ticker was
    // added and removed earlier *today*. That is someone undoing a mistake within the day, so
    // the removal is cleared — and the `WHERE` makes that conditional: a row that is already
    // active returns no row at all, which is `AlreadyWatched` rather than a silent success.
    // Nothing else can be written here, because 0005 grants UPDATE on `removed_on` alone.
    //
    // The partial unique index collides when an *earlier* period is still active. `ON
    // CONFLICT` can name only one target, so that one arrives as a unique violation and is
    // read from the index name below.
    let row = sqlx::query!(
        r#"
        INSERT INTO watchlist_item (user_id, instrument_id)
        VALUES ($1, $2)
        ON CONFLICT (user_id, instrument_id, added_on)
        DO UPDATE SET removed_on = NULL
              WHERE watchlist_item.removed_on IS NOT NULL
        RETURNING added_on
        "#,
        user_id,
        instrument_id
    )
    .fetch_optional(pool)
    .await;

    match row {
        Ok(Some(row)) => Ok(Added::Watched {
            added_on: TradingDay::from_date(row.added_on),
        }),
        Ok(None) => Ok(Added::AlreadyWatched),
        Err(e) if violates(&e, "watchlist_item_one_active") => Ok(Added::AlreadyWatched),
        Err(e) => Err(e),
    }
}

/// Remove a ticker, keeping its history (F-3).
///
/// An UPDATE that sets one date, never a DELETE. The removal date comes from the database in
/// New York time, the same expression `added_on` defaults to in 0002: `current_date` in a UTC
/// container is a day ahead of New York between midnight and 04:00 UTC, and a watchlist period
/// that starts after it ended is the kind of wrong that only shows up months later in a chart.
///
/// Returns `None` when the ticker is not on the watchlist — including when the symbol is
/// unknown, since the subquery then finds no instrument. The caller cannot tell those apart,
/// and does not need to: both answers are "that is not on your watchlist".
pub async fn remove(
    pool: &PgPool,
    user_id: &str,
    ticker: &Ticker,
) -> Result<Option<TradingDay>, sqlx::Error> {
    let row = sqlx::query!(
        r#"
        UPDATE watchlist_item
           SET removed_on = (now() AT TIME ZONE 'America/New_York')::date
         WHERE user_id = $1
           AND removed_on IS NULL
           AND instrument_id = (SELECT id FROM instrument WHERE ticker = $2)
        RETURNING removed_on AS "removed_on!"
        "#,
        user_id,
        ticker.as_str()
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| TradingDay::from_date(r.removed_on)))
}

/// Whether a database error is a unique violation of one named index or constraint.
fn violates(error: &sqlx::Error, name: &str) -> bool {
    error
        .as_database_error()
        .and_then(|e| e.constraint())
        .is_some_and(|c| c == name)
}

/// Turn a stored ticker back into a `Ticker`.
///
/// The database holds the normalised form and a CHECK constraint refuses anything else, so
/// this cannot fail for a row this code wrote. If it ever does, the column holds something no
/// code path should have been able to store, and the honest answer is a decode error — not a
/// panic, and certainly not a silently skipped row in a watchlist the user is looking at.
fn parse_stored_ticker(stored: &str) -> Result<Ticker, sqlx::Error> {
    stored
        .parse()
        .map_err(|e: watcher_core::InvalidTicker| sqlx::Error::Decode(Box::new(e)))
}
