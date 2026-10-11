//! `GET`, `POST` and `DELETE` on the watchlist — F-1, F-2 and F-3.
//!
//! The one decision worth stating here is how a ticker is *validated*. F-1 says a ticker is
//! checked against the instrument list, and the instrument list starts empty: nothing seeds
//! it, because nothing can seed it honestly — a committed list of US symbols is stale the
//! month after it is written. So the list is grown by the only component that actually knows
//! whether a symbol trades, which is the price source. An unknown symbol is the provider's
//! `UnknownSymbol`, already a first-class, tested outcome of `watcher-feeds`, and a symbol
//! that is confirmed is written into `instrument` and never asked about again.
//!
//! That also closes a quieter failure: a ticker accepted on shape alone that no provider knows
//! would make the nightly job record a failed fetch every night for a typo, which looks like a
//! degrading provider rather than a mistake someone made once.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use watcher_core::{FetchError, InvalidTicker, PriceSource, Ticker, TradingDay};
use watcher_store::watchlist::{self, Added, MAX_ACTIVE};

use crate::error::ApiError;
use crate::AppState;

/// How far back a symbol is asked about when confirming it exists.
///
/// Long enough that a holiday week, a thin symbol or a provider that lags by a day cannot make
/// a real symbol look unknown; short enough that the answer is about the symbol as it trades
/// now. A symbol with no bar in thirty days is one the nightly job could not analyse anyway.
const CONFIRM_WINDOW_DAYS: i64 = 30;

#[derive(Serialize)]
pub struct WatchlistBody {
    items: Vec<ItemBody>,
    /// Sent so the page can show "12 of 50" without hard-coding F-2's number.
    count: usize,
    limit: i64,
}

/// Dates and tickers go over the wire as strings.
///
/// The core types have no serde yet — that arrives with the chart contract in T3.1 — and
/// `Display` already writes the two forms the API promises: a normalised symbol and an ISO
/// date. No money crosses this endpoint, so the `price_scale` question does not arise here.
#[derive(Serialize)]
pub struct ItemBody {
    ticker: String,
    added_on: String,
}

#[derive(Serialize)]
pub struct RemovedBody {
    ticker: String,
    removed_on: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddBody {
    ticker: String,
}

/// `GET /api/watchlist` — the active watchlist (F-1).
///
/// `PLAN.md` promises the last analysis date beside each ticker. There is no `analysis` table
/// until M1, so the field is left out rather than sent as a permanent `null`; adding it later
/// is an additive change to the shape, and a frontend that never saw it cannot come to depend
/// on its being null.
pub async fn list<P>(State(state): State<AppState<P>>) -> Result<Json<WatchlistBody>, ApiError>
where
    P: PriceSource + Send + Sync + 'static,
{
    let items = watchlist::active(&state.db, state.user_id)
        .await
        .map_err(|e| ApiError::database(&e))?;

    Ok(Json(WatchlistBody {
        count: items.len(),
        limit: MAX_ACTIVE,
        items: items
            .into_iter()
            .map(|i| ItemBody {
                ticker: i.ticker.to_string(),
                added_on: i.added_on.to_string(),
            })
            .collect(),
    }))
}

/// `POST /api/watchlist` — add a ticker (F-1, F-2).
pub async fn add<P>(
    State(state): State<AppState<P>>,
    body: Result<Json<AddBody>, axum::extract::rejection::JsonRejection>,
) -> Result<(StatusCode, Json<ItemBody>), ApiError>
where
    P: PriceSource + Send + Sync + 'static,
{
    let Json(body) = body?;
    let ticker = parse_ticker(&body.ticker)?;

    // F-2 before anything that costs a request. Being full is a refusal that needs no
    // provider, and asking about a symbol that cannot be added either way spends someone
    // else's free endpoint on nothing.
    if watchlist::active_count(&state.db, state.user_id)
        .await
        .map_err(|e| ApiError::database(&e))?
        >= MAX_ACTIVE
    {
        return Err(full());
    }

    let instrument_id = match watchlist::instrument_id(&state.db, &ticker)
        .await
        .map_err(|e| ApiError::database(&e))?
    {
        Some(id) => id,
        None => {
            confirm(state.prices.as_ref(), &ticker).await?;
            watchlist::confirmed(&state.db, &ticker)
                .await
                .map_err(|e| ApiError::database(&e))?
        }
    };

    match watchlist::add(&state.db, state.user_id, instrument_id)
        .await
        .map_err(|e| ApiError::database(&e))?
    {
        Added::Watched { added_on } => Ok((
            StatusCode::CREATED,
            Json(ItemBody {
                ticker: ticker.to_string(),
                added_on: added_on.to_string(),
            }),
        )),
        Added::AlreadyWatched => Err(ApiError::conflict(
            "already_watched",
            format!("{ticker} is already on the watchlist."),
        )),
        Added::Full => Err(full()),
    }
}

/// `DELETE /api/watchlist/{ticker}` — remove a ticker, keeping its history (F-3).
pub async fn remove<P>(
    State(state): State<AppState<P>>,
    Path(ticker): Path<String>,
) -> Result<Json<RemovedBody>, ApiError>
where
    P: PriceSource + Send + Sync + 'static,
{
    let ticker = parse_ticker(&ticker)?;

    match watchlist::remove(&state.db, state.user_id, &ticker)
        .await
        .map_err(|e| ApiError::database(&e))?
    {
        Some(removed_on) => Ok(Json(RemovedBody {
            ticker: ticker.to_string(),
            removed_on: removed_on.to_string(),
        })),
        // Also the answer for a symbol nothing has ever confirmed. The two cases are the same
        // sentence from the user's side, and distinguishing them would tell a stranger which
        // symbols this instance has heard of.
        None => Err(ApiError::not_found(
            "not_watched",
            format!("{ticker} is not on the watchlist."),
        )),
    }
}

/// F-2's refusal, written once so the two paths that can hit the cap cannot word it
/// differently.
fn full() -> ApiError {
    ApiError::conflict(
        "watchlist_full",
        format!(
            "A watchlist holds at most {MAX_ACTIVE} tickers. Remove one before adding another."
        ),
    )
}

/// Parse a symbol, refusing anything the `Ticker` type cannot normalise.
///
/// The rejected text is never quoted back. `InvalidTicker` carries no payload for exactly that
/// reason (A-4): this input may have come from the agent, and an error that echoes it is how
/// an injection reaches a log or a page.
fn parse_ticker(text: &str) -> Result<Ticker, ApiError> {
    text.parse::<Ticker>()
        .map_err(|e: InvalidTicker| ApiError::bad_request("invalid_ticker", e.to_string()))
}

/// Ask the configured price source whether this symbol trades (F-1).
///
/// The window is measured in UTC days. New York would be more correct and makes no difference
/// over thirty days, and reaching for the calendar here would mean a database round trip to
/// answer a question that is "does this symbol exist".
async fn confirm<P: PriceSource>(prices: &P, ticker: &Ticker) -> Result<(), ApiError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default();

    let (from, to) = match (
        TradingDay::from_unix_seconds(now - CONFIRM_WINDOW_DAYS * 86_400),
        TradingDay::from_unix_seconds(now),
    ) {
        (Some(from), Some(to)) => (from, to),
        // Only reachable with a clock decades out, and then the honest answer is that the
        // symbol was not checked rather than that it does not exist.
        _ => {
            return Err(ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "clock_unusable",
                "The server clock is not usable, so the symbol could not be checked.",
            ))
        }
    };

    match prices.daily_bars(ticker, from, to).await {
        // A provider that knows the symbol but has no recent bar for it: not unknown, and not
        // usable either. Saying which it is beats one message for both.
        Ok(fetched) if fetched.bars.is_empty() => Err(ApiError::bad_request(
            "no_price_data",
            format!(
                "{ticker} returned no price data for the last {CONFIRM_WINDOW_DAYS} days, \
                 so there is nothing to analyse."
            ),
        )),
        Ok(_) => Ok(()),
        Err(FetchError::UnknownSymbol) => Err(ApiError::bad_request(
            "unknown_symbol",
            format!("{ticker} is not a symbol the price source knows."),
        )),
        Err(e) => Err(ApiError::provider(&e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use watcher_core::{DailyBar, Fetched, Money};

    /// A price source that answers from a script rather than from the network.
    ///
    /// F-1's acceptance test — "adding an unknown symbol is rejected with a clear message" —
    /// is a property of this mapping, not of the database, so it is tested here where it needs
    /// neither Postgres nor an HTTP call.
    struct Scripted(Result<Fetched, FetchError>);

    impl PriceSource for Scripted {
        async fn daily_bars(
            &self,
            _ticker: &Ticker,
            _from: TradingDay,
            _to: TradingDay,
        ) -> Result<Fetched, FetchError> {
            match &self.0 {
                Ok(f) => Ok(f.clone()),
                // FetchError is not Clone, so the script is re-made rather than copied.
                Err(FetchError::UnknownSymbol) => Err(FetchError::UnknownSymbol),
                Err(FetchError::Transport) => Err(FetchError::Transport),
                Err(FetchError::Status { status }) => Err(FetchError::Status { status: *status }),
                Err(FetchError::Unreadable { reason }) => Err(FetchError::Unreadable { reason }),
            }
        }
    }

    fn one_bar() -> Fetched {
        Fetched {
            bars: vec![DailyBar {
                day: TradingDay::from_ymd(2026, 10, 9).expect("a real date"),
                open: Money::from_minor(24_150),
                high: Money::from_minor(24_390),
                low: Money::from_minor(24_005),
                close: Money::from_minor(24_310),
                volume: 48_201_100,
            }],
            skipped: 0,
        }
    }

    fn ticker(s: &str) -> Ticker {
        s.parse().expect("a well-formed symbol")
    }

    #[tokio::test]
    async fn a_symbol_with_bars_is_confirmed() {
        let source = Scripted(Ok(one_bar()));
        assert!(confirm(&source, &ticker("AAPL")).await.is_ok());
    }

    #[tokio::test]
    async fn an_unknown_symbol_is_refused_and_says_so() {
        let source = Scripted(Err(FetchError::UnknownSymbol));
        let e = confirm(&source, &ticker("ZZZZZ"))
            .await
            .expect_err("an unknown symbol must not be confirmed");

        assert_eq!(e.status(), StatusCode::BAD_REQUEST);
        assert_eq!(e.code(), "unknown_symbol");
        // F-1 asks for a clear message, which means the symbol the user typed is in it. Safe
        // here and nowhere else: this ticker parsed, so it is at most five letters and a class
        // suffix. The unparsed text is never echoed.
        assert!(e.message().contains("ZZZZZ"), "{}", e.message());
    }

    #[tokio::test]
    async fn a_known_symbol_with_no_recent_bars_is_refused_separately() {
        // The provider knows it and has nothing to show. Not "unknown", and not addable:
        // one message for both would send someone looking for a typo that is not there.
        let source = Scripted(Ok(Fetched::default()));
        let e = confirm(&source, &ticker("DLST"))
            .await
            .expect_err("a symbol with no bars cannot be analysed");

        assert_eq!(e.status(), StatusCode::BAD_REQUEST);
        assert_eq!(e.code(), "no_price_data");
    }

    #[tokio::test]
    async fn a_provider_failure_is_not_the_user_s_fault() {
        // The distinction that matters operationally: a 400 says "fix your request", a 502
        // says "the endpoint we read is down". Recording the second as the first is how a
        // broken provider looks like a user typing badly all day.
        for failure in [
            FetchError::Transport,
            FetchError::Status { status: 503 },
            FetchError::Unreadable {
                reason: "shape changed",
            },
        ] {
            let source = Scripted(Err(failure));
            let e = confirm(&source, &ticker("AAPL"))
                .await
                .expect_err("a provider failure must not confirm a symbol");

            assert_eq!(e.status(), StatusCode::BAD_GATEWAY);
            assert_eq!(e.code(), "price_source_unavailable");
        }
    }

    #[test]
    fn a_malformed_symbol_is_refused_without_quoting_it() {
        // A-4: this text may have come from the agent. The message explains the rule; it does
        // not repeat the input into a log or a page.
        let injected = "AAPL<script>alert(1)</script>";
        let e = parse_ticker(injected).expect_err("that is not a symbol");

        assert_eq!(e.status(), StatusCode::BAD_REQUEST);
        assert_eq!(e.code(), "invalid_ticker");
        assert!(!e.message().contains("script"), "{}", e.message());
    }

    #[test]
    fn an_empty_symbol_is_refused() {
        for text in ["", "   ", "aa pl"] {
            assert_eq!(
                parse_ticker(text).expect_err("not a symbol").code(),
                "invalid_ticker"
            );
        }
    }

    #[test]
    fn the_full_message_names_the_limit_and_what_to_do() {
        // F-2: "adding the 51st is refused and explains why".
        let e = full();
        assert_eq!(e.status(), StatusCode::CONFLICT);
        assert_eq!(e.code(), "watchlist_full");
        assert!(e.message().contains("50"), "{}", e.message());
        assert!(e.message().contains("Remove one"), "{}", e.message());
    }

    #[test]
    fn a_symbol_is_normalised_before_it_is_used() {
        // The database constraint refuses lowercase outright, so the endpoint must be the
        // place that normalises — otherwise ' aapl ' is a 500 rather than an add.
        assert_eq!(parse_ticker(" aapl ").expect("normalised").as_str(), "AAPL");
    }

    #[test]
    fn a_body_with_extra_fields_is_refused() {
        // deny_unknown_fields: an agent sending {"ticker":"AAPL","quantity":100} is confused
        // about which endpoint it is calling, and silently ignoring the rest hides that.
        let ok: Result<AddBody, _> = serde_json::from_str(r#"{"ticker":"AAPL"}"#);
        assert!(ok.is_ok());

        let extra: Result<AddBody, _> = serde_json::from_str(r#"{"ticker":"AAPL","quantity":100}"#);
        assert!(extra.is_err());

        let missing: Result<AddBody, _> = serde_json::from_str("{}");
        assert!(missing.is_err());
    }
}
