//! A whole number of shares.
//!
//! Whole shares only (F-67): a buy of 1.5 shares is rejected rather than rounded, and a buy
//! of less than one share's worth of cash is rejected rather than filled for zero. There is
//! no fractional constructor, so that rule holds by construction.
//!
//! Signed, because a ledger entry records a movement and a sale moves shares out
//! (`share_delta integer NOT NULL`, signed). A *position* is separately required to be
//! non-negative — no shorting in v1 (F-68) — and that is the ledger's rule to enforce, not
//! this type's, because it is a fact about the portfolio rather than about the number.

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Shares(i32);

impl Shares {
    pub const ZERO: Self = Self(0);

    pub const fn new(count: i32) -> Self {
        Self(count)
    }

    pub const fn count(self) -> i32 {
        self.0
    }

    pub const fn is_positive(self) -> bool {
        self.0 > 0
    }

    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        self.0.checked_add(other.0).map(Self)
    }

    pub fn checked_sub(self, other: Self) -> Option<Self> {
        self.0.checked_sub(other.0).map(Self)
    }

    pub const fn negated(self) -> Option<Self> {
        match self.0.checked_neg() {
            Some(v) => Some(Self(v)),
            None => None,
        }
    }
}

impl fmt::Display for Shares {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for Shares {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Shares({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_round_trip() {
        assert_eq!(Shares::new(12).count(), 12);
        assert_eq!(Shares::ZERO.count(), 0);
        assert_eq!(Shares::new(-3).count(), -3);
    }

    #[test]
    fn arithmetic_refuses_to_wrap() {
        assert_eq!(Shares::new(i32::MAX).checked_add(Shares::new(1)), None);
        assert_eq!(Shares::new(i32::MIN).checked_sub(Shares::new(1)), None);
        assert_eq!(Shares::new(i32::MIN).negated(), None);
        assert_eq!(
            Shares::new(10).checked_add(Shares::new(5)),
            Some(Shares::new(15))
        );
    }

    #[test]
    fn sign_predicates_agree_with_the_count() {
        assert!(Shares::new(1).is_positive());
        assert!(!Shares::ZERO.is_positive());
        assert!(Shares::new(-1).is_negative());
        assert!(Shares::ZERO.is_zero());
    }
}
