//! A US-listed stock symbol.
//!
//! This type checks shape only. Whether a symbol actually trades is answered by the
//! `instrument` table (F-1), because that is a fact about the world and not about the string.

use std::fmt;
use std::str::FromStr;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ticker(String);

impl Ticker {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum InvalidTicker {
    Empty,
    TooLong,
    /// Carries no payload: the rejected text may come from the agent, and an error that
    /// echoes untrusted input back into a log is how an injection travels (A-4).
    BadCharacter,
}

impl fmt::Display for InvalidTicker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("a ticker cannot be empty"),
            Self::TooLong => {
                f.write_str("a ticker is at most 5 letters, with an optional .X class suffix")
            }
            Self::BadCharacter => {
                f.write_str("a ticker is letters, optionally followed by a dot and a class letter")
            }
        }
    }
}

impl std::error::Error for InvalidTicker {}

impl FromStr for Ticker {
    type Err = InvalidTicker;

    /// Trims and upper-cases, so `" aapl "` is accepted as `AAPL`. Being strict about case
    /// would only move the normalising somewhere less visible.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim().to_ascii_uppercase();
        if s.is_empty() {
            return Err(InvalidTicker::Empty);
        }

        let (root, class) = match s.split_once('.') {
            Some((r, c)) => (r, Some(c)),
            None => (s.as_str(), None),
        };

        if root.is_empty() || root.len() > 5 {
            return Err(if root.is_empty() {
                InvalidTicker::BadCharacter
            } else {
                InvalidTicker::TooLong
            });
        }
        if !root.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(InvalidTicker::BadCharacter);
        }

        // Class shares such as BRK.B and RDS.A.
        if let Some(class) = class {
            if class.is_empty() || class.len() > 2 {
                return Err(InvalidTicker::BadCharacter);
            }
            if !class.bytes().all(|b| b.is_ascii_uppercase()) {
                return Err(InvalidTicker::BadCharacter);
            }
        }

        Ok(Self(s))
    }
}

impl fmt::Display for Ticker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Ticker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ticker({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Result<Ticker, InvalidTicker> {
        s.parse()
    }

    #[test]
    fn ordinary_symbols_are_accepted() {
        for s in ["A", "F", "AAPL", "MSFT", "GOOGL"] {
            assert_eq!(t(s).unwrap().as_str(), s);
        }
    }

    #[test]
    fn class_shares_are_accepted() {
        assert_eq!(t("BRK.B").unwrap().as_str(), "BRK.B");
        assert_eq!(t("RDS.A").unwrap().as_str(), "RDS.A");
    }

    #[test]
    fn input_is_normalised_rather_than_refused() {
        assert_eq!(t("aapl").unwrap().as_str(), "AAPL");
        assert_eq!(t("  msft  ").unwrap().as_str(), "MSFT");
        assert_eq!(t("brk.b").unwrap().as_str(), "BRK.B");
    }

    #[test]
    fn nonsense_is_refused() {
        assert_eq!(t(""), Err(InvalidTicker::Empty));
        assert_eq!(t("   "), Err(InvalidTicker::Empty));
        assert_eq!(t("TOOLONG"), Err(InvalidTicker::TooLong));
        assert_eq!(t("AA PL"), Err(InvalidTicker::BadCharacter));
        assert_eq!(t("AA1"), Err(InvalidTicker::BadCharacter));
        assert_eq!(t("AAPL."), Err(InvalidTicker::BadCharacter));
        assert_eq!(t(".B"), Err(InvalidTicker::BadCharacter));
        assert_eq!(t("BRK.BBB"), Err(InvalidTicker::BadCharacter));
        assert_eq!(t("BRK.1"), Err(InvalidTicker::BadCharacter));
        // Length is checked before characters, so this is TooLong rather than BadCharacter.
        assert_eq!(t("DROP TABLE"), Err(InvalidTicker::TooLong));
        // A short symbol with a bad character still reports the character.
        assert_eq!(t("A B"), Err(InvalidTicker::BadCharacter));
    }

    #[test]
    fn an_error_never_echoes_the_rejected_text() {
        // A-4: model output is untrusted, so a rejection must not carry it into a log line.
        let e = t("'; DROP TABLE analysis; --").unwrap_err();
        assert!(!e.to_string().contains("DROP"));
    }
}
