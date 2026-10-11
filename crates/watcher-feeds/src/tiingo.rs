//! The paid price source, written now and left unconfigured.
//!
//! `PLAN.md` D15: §11's acceptance test needs fourteen *consecutive* trading days, so a free
//! endpoint breaking on day nine means starting the count again. Having the replacement
//! already written turns that from a rebuild into a restart — one environment variable.
//!
//! UNVERIFIED AGAINST THE LIVE API. This is written from Tiingo's documented response shape;
//! there is no API key here, which is the point of leaving it unconfigured. The parser is
//! built the same defensive way as the free one — every field optional, a changed shape
//! becoming a loud non-retryable error rather than an empty result — so the cost of the shape
//! being wrong is a clear failure on the first real call, not silently missing bars. Confirm
//! it against a real key before depending on it.

use std::time::Duration;

use serde::Deserialize;
use watcher_core::{
    minor_from_decimal_str, DailyBar, FetchError, Fetched, Money, PriceSource, Ticker, TradingDay,
};

const DEFAULT_BASE: &str = "https://api.tiingo.com";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_ATTEMPTS: u32 = 3;

pub struct TiingoPriceSource {
    client: reqwest::Client,
    base: String,
    token: String,
}

impl TiingoPriceSource {
    pub fn new(token: impl Into<String>) -> Result<Self, FetchError> {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent("the-watcher/0.1")
            .build()
            .map_err(|_| FetchError::Transport)?;
        Ok(Self {
            client,
            base: DEFAULT_BASE.to_string(),
            token: token.into(),
        })
    }

    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        self.base = base.into();
        self
    }

    async fn attempt(&self, url: &str) -> Result<String, FetchError> {
        let response = self
            .client
            .get(url)
            // The token goes in a header, not the query string. A token in a URL ends up in
            // access logs, in error messages that quote the URL, and in anything that
            // reports a failing request — which is the slow way to leak a credential (N-40).
            .header("Authorization", format!("Token {}", self.token))
            .send()
            .await
            .map_err(|_| FetchError::Transport)?;

        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(FetchError::UnknownSymbol);
        }
        if !status.is_success() {
            return Err(FetchError::Status {
                status: status.as_u16(),
            });
        }
        response.text().await.map_err(|_| FetchError::Transport)
    }
}

impl PriceSource for TiingoPriceSource {
    async fn daily_bars(
        &self,
        ticker: &Ticker,
        from: TradingDay,
        to: TradingDay,
    ) -> Result<Fetched, FetchError> {
        let url = format!(
            "{}/tiingo/daily/{}/prices?startDate={}&endDate={}",
            self.base,
            ticker.as_str().to_ascii_lowercase(),
            from,
            to
        );

        let mut last = FetchError::Transport;
        for attempt in 1..=MAX_ATTEMPTS {
            match self.attempt(&url).await {
                Ok(body) => return parse_prices(&body),
                Err(e) => {
                    if !e.is_retryable() || attempt == MAX_ATTEMPTS {
                        return Err(e);
                    }
                    tokio::time::sleep(Duration::from_millis(500 * u64::from(attempt))).await;
                    last = e;
                }
            }
        }
        Err(last)
    }
}

/// One row of the documented daily-prices response.
///
/// The adjusted fields (`adjOpen`, `adjClose` and friends) are deliberately not read. I-11:
/// adjusted prices are recomputed whenever a split or dividend happens, so storing one
/// silently rewrites what the chart showed last month. Splits are applied as explicit ledger
/// entries instead (F-70).
#[derive(Deserialize)]
struct PriceRow {
    date: Option<String>,
    open: Option<serde_json::Number>,
    high: Option<serde_json::Number>,
    low: Option<serde_json::Number>,
    close: Option<serde_json::Number>,
    volume: Option<i64>,
}

pub fn parse_prices(body: &str) -> Result<Fetched, FetchError> {
    // An error from this API comes back as an object, not an array.
    let rows: Vec<PriceRow> = serde_json::from_str(body).map_err(|_| {
        if body.contains("\"detail\"") || body.trim_start().starts_with('{') {
            FetchError::Unreadable {
                reason: "provider returned an object, not a price array",
            }
        } else {
            FetchError::Unreadable { reason: "not JSON" }
        }
    })?;

    let mut fetched = Fetched::default();
    for row in rows {
        match build_bar(row) {
            Some(bar) if bar.is_coherent() => fetched.bars.push(bar),
            // Dropped, never interpolated: an invented bar is the guessed data I-12 forbids.
            _ => fetched.skipped += 1,
        }
    }
    Ok(fetched)
}

fn build_bar(row: PriceRow) -> Option<DailyBar> {
    // Timestamps arrive as "2026-10-09T00:00:00.000Z"; only the calendar date matters for a
    // daily bar, so the time is cut rather than parsed.
    let date = row.date?;
    let day: TradingDay = date.get(..10)?.parse().ok()?;

    Some(DailyBar {
        day,
        open: money(row.open.as_ref())?,
        high: money(row.high.as_ref())?,
        low: money(row.low.as_ref())?,
        close: money(row.close.as_ref())?,
        volume: u64::try_from(row.volume?).ok()?,
    })
}

/// Through the decimal text, never an `f64`. See `minor_from_decimal_str`.
fn money(n: Option<&serde_json::Number>) -> Option<Money> {
    minor_from_decimal_str(&n?.to_string()).map(Money::from_minor)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"[
      {"date":"2026-10-08T00:00:00.000Z","open":241.50,"high":243.90,"low":240.05,
       "close":243.10,"volume":48201100,"adjClose":121.55},
      {"date":"2026-10-09T00:00:00.000Z","open":243.10,"high":244.00,"low":242.00,
       "close":null,"volume":31000000,"adjClose":null}
    ]"#;

    #[test]
    fn a_documented_response_becomes_bars() {
        let f = parse_prices(BODY).unwrap();
        assert_eq!(f.bars.len(), 1);
        assert_eq!(
            f.skipped, 1,
            "the null close must be dropped, not filled in"
        );

        let bar = f.bars[0];
        assert_eq!(bar.day.to_string(), "2026-10-08");
        assert_eq!(bar.open, Money::from_minor(24150));
        assert_eq!(bar.close, Money::from_minor(24310));
        assert_eq!(bar.volume, 48_201_100);
    }

    #[test]
    fn the_adjusted_close_is_never_read() {
        // adjClose is half the raw close in the fixture. If it leaked into any field the
        // numbers below would be wrong — which is the I-11 failure, and it would be silent.
        let f = parse_prices(BODY).unwrap();
        let bar = f.bars[0];
        for value in [bar.open, bar.high, bar.low, bar.close] {
            assert_ne!(value, Money::from_minor(12155), "an adjusted price got in");
        }
    }

    #[test]
    fn an_error_object_is_unreadable_rather_than_empty() {
        // The failure that matters: an error must not parse as zero bars, or the nightly job
        // records success with nothing fetched.
        let body = r#"{"detail":"Error: Ticker not found."}"#;
        match parse_prices(body) {
            Err(FetchError::Unreadable { reason }) => {
                assert!(reason.contains("object"), "got {reason}")
            }
            other => panic!("wanted Unreadable, got {other:?}"),
        }
    }

    #[test]
    fn junk_is_refused() {
        assert!(matches!(
            parse_prices("not json"),
            Err(FetchError::Unreadable { .. })
        ));
    }

    #[test]
    fn an_empty_array_is_not_an_error() {
        let f = parse_prices("[]").unwrap();
        assert!(f.bars.is_empty());
        assert_eq!(f.skipped, 0);
    }

    #[test]
    fn an_incoherent_bar_is_skipped() {
        let body = BODY.replace("\"high\":243.90", "\"high\":1.00");
        let f = parse_prices(&body).unwrap();
        assert_eq!(f.bars.len(), 0);
        assert_eq!(f.skipped, 2);
    }

    #[test]
    fn a_malformed_date_is_skipped_not_guessed() {
        let body = BODY.replace("2026-10-08T00:00:00.000Z", "yesterday");
        let f = parse_prices(&body).unwrap();
        assert_eq!(f.bars.len(), 0);
        assert_eq!(f.skipped, 2);
    }
}
