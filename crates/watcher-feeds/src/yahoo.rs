//! The free price source.
//!
//! This reads an unofficial, undocumented endpoint. The brief lists that as a risk and A-1
//! requires the system to keep working behind the same trait when it breaks — so the shape of
//! this file assumes it will break: parsing is separated from fetching, every field is
//! optional until proven otherwise, and an unreadable response is a distinct, non-retryable
//! error rather than an empty result that looks like a quiet market.

use std::time::Duration;

use serde::Deserialize;
use watcher_core::{
    minor_from_decimal_str, DailyBar, FetchError, Fetched, Money, PriceSource, Ticker, TradingDay,
};

const DEFAULT_BASE: &str = "https://query1.finance.yahoo.com";

/// I-32 in spirit: a hanging provider must not hang a run.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_ATTEMPTS: u32 = 3;

pub struct YahooPriceSource {
    client: reqwest::Client,
    base: String,
}

impl YahooPriceSource {
    pub fn new() -> Result<Self, FetchError> {
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            // The endpoint refuses requests with no user agent. Saying what we are is also
            // the polite thing when reading someone else's undocumented API.
            .user_agent("the-watcher/0.1 (daily bars; one request per ticker per day)")
            .build()
            .map_err(|_| FetchError::Transport)?;
        Ok(Self {
            client,
            base: DEFAULT_BASE.to_string(),
        })
    }

    /// Point the source at a different host. Used by tests to avoid the network.
    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        self.base = base.into();
        self
    }

    async fn attempt(&self, url: &str) -> Result<String, FetchError> {
        // Timeout, connect failure and malformed request all collapse to Transport, which is
        // the retryable case. Distinguishing them would add branches the caller cannot act on
        // differently — what it needs to know is whether asking again could help.
        let response = self
            .client
            .get(url)
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

impl PriceSource for YahooPriceSource {
    async fn daily_bars(
        &self,
        ticker: &Ticker,
        from: TradingDay,
        to: TradingDay,
    ) -> Result<Fetched, FetchError> {
        let url = format!(
            "{}/v8/finance/chart/{}?period1={}&period2={}&interval=1d&events=div%2Csplit",
            self.base,
            ticker.as_str(),
            from.to_unix_midnight(),
            // Inclusive of the last day: the endpoint treats period2 as exclusive.
            to.to_unix_midnight() + 86_400
        );

        // Every branch below returns, so there is no fallback after the loop. An earlier
        // version kept a `last` error and ended with `Err(last)`, which was unreachable and
        // read as though it were the real failure path.
        for attempt in 1..=MAX_ATTEMPTS {
            match self.attempt(&url).await {
                Ok(body) => return parse_chart(&body).map(|f| f.within(from, to)),
                Err(e) => {
                    // I-12: retried, then recorded as a failed fetch. A parse error or an
                    // unknown symbol will not change on a second ask, so it stops here
                    // instead of spending the budget.
                    if !e.is_retryable() || attempt == MAX_ATTEMPTS {
                        return Err(e);
                    }
                    // Linear backoff. One request per ticker per day does not justify
                    // anything cleverer, and a tight loop against someone else's free
                    // endpoint is how access gets withdrawn.
                    tokio::time::sleep(Duration::from_millis(500 * u64::from(attempt))).await;
                }
            }
        }
        unreachable!("the loop returns on every path")
    }
}

// The response shape, as much of it as we read. Everything is optional because this is an
// undocumented endpoint: a missing field must produce a readable error, not a panic.
#[derive(Deserialize)]
struct ChartEnvelope {
    chart: Option<Chart>,
}

#[derive(Deserialize)]
struct Chart {
    result: Option<Vec<ChartResult>>,
    error: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ChartResult {
    timestamp: Option<Vec<i64>>,
    indicators: Option<Indicators>,
}

#[derive(Deserialize)]
struct Indicators {
    quote: Option<Vec<Quote>>,
}

#[derive(Deserialize)]
struct Quote {
    open: Option<Vec<Option<serde_json::Number>>>,
    high: Option<Vec<Option<serde_json::Number>>>,
    low: Option<Vec<Option<serde_json::Number>>>,
    close: Option<Vec<Option<serde_json::Number>>>,
    volume: Option<Vec<Option<i64>>>,
}

/// Turn a response body into bars.
///
/// Separated from the HTTP call so that every shape this endpoint can return is testable
/// without a network — which matters more than usual here, because the shape is not promised
/// to anyone.
pub fn parse_chart(body: &str) -> Result<Fetched, FetchError> {
    let envelope: ChartEnvelope =
        serde_json::from_str(body).map_err(|_| FetchError::Unreadable { reason: "not JSON" })?;

    let chart = envelope.chart.ok_or(FetchError::Unreadable {
        reason: "no chart object",
    })?;

    if chart.error.is_some_and(|e| !e.is_null()) {
        return Err(FetchError::UnknownSymbol);
    }

    let result = chart
        .result
        .and_then(|r| r.into_iter().next())
        .ok_or(FetchError::Unreadable {
            reason: "no result entry",
        })?;

    let timestamps = result.timestamp.unwrap_or_default();
    let quote = result
        .indicators
        .and_then(|i| i.quote)
        .and_then(|q| q.into_iter().next())
        .ok_or(FetchError::Unreadable {
            reason: "no quote series",
        })?;

    let (opens, highs, lows, closes, volumes) = (
        quote.open.unwrap_or_default(),
        quote.high.unwrap_or_default(),
        quote.low.unwrap_or_default(),
        quote.close.unwrap_or_default(),
        quote.volume.unwrap_or_default(),
    );

    // A response whose series disagree in length is not something to read halfway.
    let n = timestamps.len();
    if [
        opens.len(),
        highs.len(),
        lows.len(),
        closes.len(),
        volumes.len(),
    ]
    .iter()
    .any(|&len| len != n)
    {
        return Err(FetchError::Unreadable {
            reason: "series lengths disagree",
        });
    }

    let mut fetched = Fetched::default();
    for i in 0..n {
        match build_bar(
            timestamps[i],
            opens[i].as_ref(),
            highs[i].as_ref(),
            lows[i].as_ref(),
            closes[i].as_ref(),
            volumes[i],
        ) {
            Some(bar) if bar.is_coherent() => fetched.bars.push(bar),
            // Dropped, never interpolated. A day the provider could not fill is a day we do
            // not have, and inventing one is the guessed data I-12 forbids.
            _ => fetched.skipped += 1,
        }
    }

    Ok(fetched)
}

fn build_bar(
    timestamp: i64,
    open: Option<&serde_json::Number>,
    high: Option<&serde_json::Number>,
    low: Option<&serde_json::Number>,
    close: Option<&serde_json::Number>,
    volume: Option<i64>,
) -> Option<DailyBar> {
    let day = TradingDay::from_unix_seconds(timestamp)?;
    Some(DailyBar {
        day,
        open: money(open)?,
        high: money(high)?,
        low: money(low)?,
        close: money(close)?,
        volume: u64::try_from(volume?).ok()?,
    })
}

/// A JSON number becomes money through its decimal text, never through an `f64`.
///
/// `serde_json::Number`'s `Display` is the shortest text that round-trips the value, so a
/// response carrying `24.31` yields exactly `"24.31"` here and the integer conversion is
/// exact. See `minor_from_decimal_str` for why the float route is not good enough.
fn money(n: Option<&serde_json::Number>) -> Option<Money> {
    minor_from_decimal_str(&n?.to_string()).map(Money::from_minor)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two sessions, the second with a null close — the shape this endpoint really returns
    /// for a day it could not fill.
    const BODY: &str = r#"{
      "chart": {
        "error": null,
        "result": [{
          "timestamp": [1791331200, 1791417600],
          "indicators": { "quote": [{
            "open":   [241.50, 243.10],
            "high":   [243.90, 244.00],
            "low":    [240.05, 242.00],
            "close":  [243.10, null],
            "volume": [48201100, 31000000]
          }]}
        }]
      }
    }"#;

    #[test]
    fn a_normal_response_becomes_bars() {
        let f = parse_chart(BODY).unwrap();
        assert_eq!(f.bars.len(), 1);
        assert_eq!(
            f.skipped, 1,
            "the null close must be dropped, not filled in"
        );

        let bar = f.bars[0];
        // Pinned, because a later test reasons about which day survives clamping and it is
        // easy to assume the wrong one: the second entry (2026-10-08) is the null close.
        assert_eq!(bar.day.to_string(), "2026-10-07");
        assert_eq!(bar.open, Money::from_minor(24150));
        assert_eq!(bar.high, Money::from_minor(24390));
        assert_eq!(bar.low, Money::from_minor(24005));
        assert_eq!(bar.close, Money::from_minor(24310));
        assert_eq!(bar.volume, 48_201_100);
        assert!(bar.is_coherent());
    }

    #[test]
    fn prices_arrive_as_exact_minor_units() {
        // The whole point of parsing the decimal text: 1.15 through an f64 would be 114.
        let body = BODY.replace("241.50", "1.15").replace("240.05", "1.15");
        let f = parse_chart(&body).unwrap();
        assert_eq!(f.bars[0].open, Money::from_minor(115));
    }

    #[test]
    fn an_unknown_symbol_is_its_own_error() {
        let body = r#"{"chart":{"error":{"code":"Not Found"},"result":null}}"#;
        assert!(matches!(parse_chart(body), Err(FetchError::UnknownSymbol)));
    }

    #[test]
    fn a_changed_shape_is_unreadable_rather_than_empty() {
        // This is the failure that matters for an undocumented endpoint: it must not look
        // like a quiet market, because the nightly job would record success and no bars.
        for (body, expect) in [
            ("not json at all", "not JSON"),
            (r#"{}"#, "no chart object"),
            (r#"{"chart":{"result":[]}}"#, "no result entry"),
            (
                r#"{"chart":{"result":[{"timestamp":[1],"indicators":{}}]}}"#,
                "no quote series",
            ),
            (
                r#"{"chart":{"result":[{"timestamp":[1,2],"indicators":{"quote":[{"open":[1.0],"high":[1.0],"low":[1.0],"close":[1.0],"volume":[1]}]}}]}}"#,
                "series lengths disagree",
            ),
        ] {
            match parse_chart(body) {
                Err(FetchError::Unreadable { reason }) => assert_eq!(reason, expect),
                other => panic!("{body:?} gave {other:?}, wanted Unreadable({expect})"),
            }
        }
    }

    #[test]
    fn an_incoherent_bar_is_skipped_not_stored() {
        // High below low. The database would refuse it; this refuses it earlier, so the run
        // records a skip rather than a failed insert.
        let body = BODY.replace(
            r#""high":   [243.90, 244.00]"#,
            r#""high":   [1.00, 244.00]"#,
        );
        let f = parse_chart(&body).unwrap();
        assert_eq!(f.bars.len(), 0);
        assert_eq!(f.skipped, 2);
    }

    #[test]
    fn an_empty_but_valid_response_is_not_an_error() {
        // A range with no sessions in it — a week of public holidays, or a symbol that had
        // not listed yet. Zero bars and zero skips is the honest answer.
        let body = r#"{"chart":{"error":null,"result":[{"timestamp":[],"indicators":{"quote":[{"open":[],"high":[],"low":[],"close":[],"volume":[]}]}}]}}"#;
        let f = parse_chart(body).unwrap();
        assert!(f.bars.is_empty());
        assert_eq!(f.skipped, 0);
    }

    #[test]
    fn bars_outside_the_requested_range_are_dropped() {
        // The request sends `to + 1 day` because the endpoint's end is exclusive. If that
        // ever returns a day past `to`, it must not be stored as though it were asked for.
        let f = parse_chart(BODY).unwrap();
        assert_eq!(f.bars.len(), 1);

        // The surviving bar is 2026-10-07, so the range has to end before it. My first
        // version of this test ended at 10-07 and expected the bar gone, having assumed the
        // good bar was the second entry — it is the first.
        let from = TradingDay::from_ymd(2026, 10, 1).unwrap();
        let to = TradingDay::from_ymd(2026, 10, 6).unwrap();
        let clamped = f.within(from, to);
        assert_eq!(
            clamped.bars.len(),
            0,
            "2026-10-07 is past the requested end"
        );
        assert_eq!(clamped.skipped, 2, "the dropped bar is counted, not lost");
    }

    #[test]
    fn a_bar_inside_the_range_survives_clamping() {
        let f = parse_chart(BODY).unwrap();
        let from = TradingDay::from_ymd(2026, 10, 1).unwrap();
        let to = TradingDay::from_ymd(2026, 10, 31).unwrap();
        let clamped = f.within(from, to);
        assert_eq!(clamped.bars.len(), 1);
        assert_eq!(clamped.skipped, 1);
    }

    #[test]
    fn retryable_errors_are_the_ones_worth_asking_again() {
        assert!(FetchError::Transport.is_retryable());
        assert!(FetchError::Status { status: 429 }.is_retryable());
        assert!(FetchError::Status { status: 503 }.is_retryable());
        assert!(!FetchError::Status { status: 400 }.is_retryable());
        assert!(!FetchError::UnknownSymbol.is_retryable());
        assert!(!FetchError::Unreadable { reason: "x" }.is_retryable());
    }
}
