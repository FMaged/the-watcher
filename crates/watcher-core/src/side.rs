//! Which way an order goes.
//!
//! No short selling in v1 (F-68), so a sell is always a sale of shares already held. The
//! ledger enforces that; this type only names the direction.

use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    /// The stored form. Matches the `CHECK (side IN ('buy','sell'))` in the schema, so the
    /// database and this enum cannot disagree about spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Buy => "buy",
            Self::Sell => "sell",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct InvalidSide;

impl fmt::Display for InvalidSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a side is either buy or sell")
    }
}

impl std::error::Error for InvalidSide {}

impl FromStr for Side {
    type Err = InvalidSide;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "buy" => Ok(Self::Buy),
            "sell" => Ok(Self::Sell),
            _ => Err(InvalidSide),
        }
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_sides_round_trip_through_their_stored_form() {
        for side in [Side::Buy, Side::Sell] {
            assert_eq!(side.as_str().parse::<Side>().unwrap(), side);
        }
    }

    #[test]
    fn case_and_padding_are_tolerated() {
        assert_eq!("BUY".parse::<Side>().unwrap(), Side::Buy);
        assert_eq!(" sell ".parse::<Side>().unwrap(), Side::Sell);
    }

    #[test]
    fn anything_else_is_refused() {
        for s in ["", "long", "short", "b", "sel", "buy sell"] {
            assert_eq!(s.parse::<Side>(), Err(InvalidSide), "{s:?}");
        }
    }
}
