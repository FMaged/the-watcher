//! Choosing a price source from configuration.
//!
//! One source today. This indirection still earns its place: the rest of the system holds a
//! `ConfiguredPriceSource` and never learns which provider is behind it, so A-1's "keep
//! working with a paid source behind the same interface" stays true without a second
//! implementation existing yet. Adding one means a variant here and nothing else (N-54).
//!
//! An enum rather than `Box<dyn PriceSource>`: the set of providers is closed and chosen by a
//! configuration value read once at startup (I-31), so dynamic dispatch would buy nothing and
//! would force the trait to give up `impl Future`.

use watcher_core::{FetchError, Fetched, PriceSource, Ticker, TradingDay};

use crate::YahooPriceSource;

pub enum ConfiguredPriceSource {
    Free(YahooPriceSource),
}

/// Shows which provider is live and nothing else.
///
/// Written by hand rather than derived, and it stays that way: the moment a provider holds an
/// API key, a derived `Debug` would put it in any log line, panic message or test failure
/// that printed the source (N-40).
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
    #[error("unknown price source {name:?}: expected free")]
    UnknownSource { name: String },

    #[error("could not build an HTTP client")]
    Client,
}

impl ConfiguredPriceSource {
    /// Build the source named by `PRICE_SOURCE`.
    pub fn from_config(name: &str) -> Result<Self, ConfigureError> {
        match name.trim().to_ascii_lowercase().as_str() {
            "free" | "yahoo" => Ok(Self::Free(
                YahooPriceSource::new().map_err(|_| ConfigureError::Client)?,
            )),
            other => Err(ConfigureError::UnknownSource {
                name: other.to_string(),
            }),
        }
    }

    /// Which provider is live, for the startup line and the run record (F-12).
    pub fn name(&self) -> &'static str {
        match self {
            Self::Free(_) => "free",
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_free_source_is_what_free_selects() {
        assert_eq!(
            ConfiguredPriceSource::from_config("free").unwrap().name(),
            "free"
        );
    }

    #[test]
    fn names_are_forgiving_about_case_and_padding() {
        for name in ["FREE", " free ", "Yahoo", "yahoo"] {
            assert_eq!(
                ConfiguredPriceSource::from_config(name).unwrap().name(),
                "free",
                "{name:?}"
            );
        }
    }

    #[test]
    fn an_unknown_name_is_refused_and_says_what_is_allowed() {
        // A typo in PRICE_SOURCE must stop the binary, not silently fall back to a default.
        // Falling back would mean the run record (F-12) named a provider that never ran.
        let e = ConfiguredPriceSource::from_config("bloomberg").unwrap_err();
        let shown = e.to_string();
        assert!(shown.contains("bloomberg"), "{shown}");
        assert!(shown.contains("free"), "{shown}");
    }

    #[test]
    fn debug_names_the_provider() {
        let s = ConfiguredPriceSource::from_config("free").unwrap();
        assert_eq!(format!("{s:?}"), "ConfiguredPriceSource(free)");
    }
}
