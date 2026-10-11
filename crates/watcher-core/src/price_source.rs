//! The interface every price provider sits behind.
//!
//! A-1 assumes free market data is available and good enough for daily bars, and requires the
//! system to keep working with a paid source behind the same interface when it is not. The
//! brief names that risk explicitly: the free endpoint is unofficial and can change without
//! notice, and §11's acceptance test needs fourteen *consecutive* trading days — so a provider
//! breaking on day nine means starting the count again.
//!
//! The trait is therefore the point. It is deliberately small, returns only what a daily bar
//! is, and knows nothing about how the data arrived.

use std::future::Future;

use crate::{Money, Ticker, TradingDay};

/// One daily bar, raw. I-11: adjusted prices are never the source of truth, so there is no
/// adjusted field to populate and no way for a provider to supply one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DailyBar {
    pub day: TradingDay,
    pub open: Money,
    pub high: Money,
    pub low: Money,
    pub close: Money,
    pub volume: u64,
}

impl DailyBar {
    /// Whether the bar is internally coherent.
    ///
    /// The same rule the `price_bar` constraints enforce, checked here as well so that a
    /// nonsense bar is refused before it reaches the database rather than by it. The
    /// duplication is deliberate: the database guarantee is what makes it impossible, and
    /// this one is what makes the failure legible (I-3).
    pub fn is_coherent(&self) -> bool {
        self.open.minor() > 0
            && self.high.minor() > 0
            && self.low.minor() > 0
            && self.close.minor() > 0
            && self.high >= self.low
            && self.high >= self.open
            && self.high >= self.close
            && self.low <= self.open
            && self.low <= self.close
    }
}

/// What came back from one fetch.
///
/// `skipped` is not noise. Providers return days with null fields — a holiday they did not
/// filter, a halted session, a gap in their own history. Those days are dropped rather than
/// interpolated, because an invented bar is exactly the "partial or guessed data" I-12
/// forbids, and counting them is how a silently degrading provider becomes visible (N-61)
/// instead of looking like a quiet market.
#[derive(Clone, Debug, Default)]
pub struct Fetched {
    pub bars: Vec<DailyBar>,
    pub skipped: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// The provider was reachable but said no. Retrying may help.
    #[error("provider returned status {status}")]
    Status { status: u16 },

    /// The network failed. Retrying may help.
    #[error("transport failed")]
    Transport,

    /// The provider answered with something this implementation cannot read. Retrying will
    /// not help, and this is the error that means the unofficial endpoint has changed shape.
    #[error("provider response could not be parsed: {reason}")]
    Unreadable { reason: &'static str },

    /// The symbol is not one the provider knows. Retrying will not help.
    #[error("provider does not know this symbol")]
    UnknownSymbol,
}

impl FetchError {
    /// Whether another attempt could plausibly succeed.
    ///
    /// I-12 says a failure is retried and then recorded as a failed fetch. Retrying a parse
    /// error would just burn the retry budget on a response that will not change.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Transport => true,
            Self::Status { status } => *status == 429 || *status >= 500,
            Self::Unreadable { .. } | Self::UnknownSymbol => false,
        }
    }
}

/// Daily bars for one instrument over a closed date range.
///
/// Static dispatch rather than `dyn`: the set of providers is closed and the choice is a
/// configuration value read once at startup (I-31), so an enum in `watcher-feeds` dispatches
/// between them. That keeps this crate free of `async-trait` and keeps the trait readable.
///
/// Only one provider exists today (A-1's free source). The trait is what makes that a
/// detail — it is the thing A-1 actually asks for, and it has to be in place *before* the
/// free endpoint breaks rather than after.
pub trait PriceSource {
    fn daily_bars(
        &self,
        ticker: &Ticker,
        from: TradingDay,
        to: TradingDay,
    ) -> impl Future<Output = Result<Fetched, FetchError>> + Send;
}
