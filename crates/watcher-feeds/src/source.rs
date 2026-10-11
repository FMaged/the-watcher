//! Choosing a price source from configuration.
//!
//! This is the whole of D15's promise: the rest of the system holds a `ConfiguredPriceSource`
//! and never learns which provider is behind it, so swapping one for the other is an
//! environment variable and a restart (I-31) rather than a code change (N-54).
//!
//! An enum rather than `Box<dyn PriceSource>`: there are exactly two implementations and the
//! set is closed, so dynamic dispatch would buy nothing and would force the trait to give up
//! `impl Future`.

use watcher_core::{FetchError, Fetched, PriceSource, Ticker, TradingDay};

use crate::{TiingoPriceSource, YahooPriceSource};

pub enum ConfiguredPriceSource {
    Free(YahooPriceSource),
    Paid(TiingoPriceSource),
}

/// Shows which provider is live and nothing else.
///
/// Written by hand rather than derived on purpose: `TiingoPriceSource` holds an API token, and
/// a derived `Debug` would put it in any log line, panic message or test failure that printed
/// the source (N-40).
impl std::fmt::Debug for ConfiguredPriceSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ConfiguredPriceSource({})", self.name())
    }
}

/// Why a source could not be built from configuration.
///
/// Separate from `FetchError` because these are startup problems, not fetch problems. The
/// binary should refuse to start rather than discover at midnight that the configured
/// provider was never usable.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigureError {
    #[error("unknown price source {name:?}: expected one of free, tiingo")]
    UnknownSource { name: String },

    #[error("price source {name:?} needs an API key, and PRICE_API_KEY is not set")]
    MissingKey { name: &'static str },

    #[error("could not build an HTTP client")]
    Client,
}

impl ConfiguredPriceSource {
    /// Build the source named by `PRICE_SOURCE`.
    ///
    /// `free` is configured today; `tiingo` is written and deliberately not configured
    /// (`PLAN.md` D15), so this function is the only place that needs to change when the free
    /// endpoint breaks — and it does not need to change, only the variable does.
    pub fn from_config(name: &str, api_key: Option<&str>) -> Result<Self, ConfigureError> {
        match name.trim().to_ascii_lowercase().as_str() {
            "free" | "yahoo" => Ok(Self::Free(
                YahooPriceSource::new().map_err(|_| ConfigureError::Client)?,
            )),
            "tiingo" | "paid" => {
                let key = api_key
                    .map(str::trim)
                    .filter(|k| !k.is_empty())
                    .ok_or(ConfigureError::MissingKey { name: "tiingo" })?;
                Ok(Self::Paid(
                    TiingoPriceSource::new(key).map_err(|_| ConfigureError::Client)?,
                ))
            }
            other => Err(ConfigureError::UnknownSource {
                name: other.to_string(),
            }),
        }
    }

    /// Which provider is live, for the startup line and the run record (F-12).
    ///
    /// Safe to log: a name, never the key.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Free(_) => "free",
            Self::Paid(_) => "tiingo",
        }
    }
}

impl PriceSource for ConfiguredPriceSource {
    async fn daily_bars(
        &self,
        ticker: &Ticker,
        from: TradingDay,
        to: TradingDay,
    ) -> Result<Fetched, FetchError> {
        match self {
            Self::Free(s) => s.daily_bars(ticker, from, to).await,
            Self::Paid(s) => s.daily_bars(ticker, from, to).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_free_source_needs_no_key() {
        let s = ConfiguredPriceSource::from_config("free", None).unwrap();
        assert_eq!(s.name(), "free");
    }

    #[test]
    fn the_paid_source_refuses_to_start_without_a_key() {
        // Failing here is the point: the alternative is discovering it on the first nightly
        // run after someone flips PRICE_SOURCE, which is the night it matters.
        for key in [None, Some(""), Some("   ")] {
            assert_eq!(
                ConfiguredPriceSource::from_config("tiingo", key).unwrap_err(),
                ConfigureError::MissingKey { name: "tiingo" }
            );
        }
    }

    #[test]
    fn the_paid_source_builds_when_configured() {
        let s = ConfiguredPriceSource::from_config("tiingo", Some("a-key")).unwrap();
        assert_eq!(s.name(), "tiingo");
    }

    #[test]
    fn names_are_forgiving_about_case_and_padding() {
        for name in ["FREE", " free ", "Yahoo"] {
            assert_eq!(
                ConfiguredPriceSource::from_config(name, None)
                    .unwrap()
                    .name(),
                "free"
            );
        }
        for name in ["TIINGO", " paid "] {
            assert_eq!(
                ConfiguredPriceSource::from_config(name, Some("k"))
                    .unwrap()
                    .name(),
                "tiingo"
            );
        }
    }

    #[test]
    fn an_unknown_name_says_what_is_allowed() {
        let e = ConfiguredPriceSource::from_config("bloomberg", None).unwrap_err();
        let shown = e.to_string();
        assert!(shown.contains("bloomberg"), "{shown}");
        assert!(shown.contains("free"), "{shown}");
        assert!(shown.contains("tiingo"), "{shown}");
    }

    #[test]
    fn an_error_never_echoes_the_key() {
        // The name is loggable, the key is not. A startup error that quoted the key would put
        // it in the logs of every failed deploy (N-40).
        let e = ConfiguredPriceSource::from_config("tiingo", Some("")).unwrap_err();
        assert!(e.to_string().contains("PRICE_API_KEY"));
    }

    #[test]
    fn debug_shows_the_provider_but_not_its_key() {
        let s = ConfiguredPriceSource::from_config("tiingo", Some("sk-a-real-token")).unwrap();
        let shown = format!("{s:?}");
        assert!(shown.contains("tiingo"));
        assert!(
            !shown.contains("sk-a-real-token"),
            "the token leaked: {shown}"
        );
    }
}
