//! Money, as an integer count of minor units.
//!
//! There is no conversion to or from a floating-point number anywhere in this module, in
//! either direction, and that is the point. The ledger is append-only (D-2): a cent that
//! arrives wrong cannot be edited later, only reversed by a second entry. A type that cannot
//! be built from an `f64` cannot acquire a rounding error from one.

use std::fmt;

/// Minor units per major unit. US dollars only in v1 (C-11), so cents.
///
/// The chart contract sends this to the frontend as `price_scale`, so the browser divides and
/// the wire format stays integral.
pub const SCALE: i64 = 100;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Money(i64);

impl Money {
    pub const ZERO: Self = Self(0);

    pub const fn from_minor(minor: i64) -> Self {
        Self(minor)
    }

    pub const fn minor(self) -> i64 {
        self.0
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

    /// Price times a quantity. Returns `None` rather than wrapping, because a wrapped total
    /// would be written to the ledger and then be uncorrectable.
    pub fn checked_mul(self, factor: i64) -> Option<Self> {
        self.0.checked_mul(factor).map(Self)
    }

    /// A fraction of this amount in basis points, rounded **away from zero**.
    ///
    /// Used for the half-spread on a fill (C-19). The rounding direction is deliberate: the
    /// spread always moves the fill price against the portfolio, so rounding it up means a
    /// cost is never understated by a cent. Rounding the other way would let the paper record
    /// flatter itself very slightly on every single trade, which over months is exactly the
    /// kind of quiet bias this product exists not to have.
    pub fn bps_away_from_zero(self, bps: u32) -> Option<Self> {
        // i128 so that the intermediate product cannot overflow before the division.
        let numerator = (self.0 as i128) * (bps as i128);
        let quotient = numerator / 10_000;
        let remainder = numerator % 10_000;
        let rounded = if remainder == 0 {
            quotient
        } else if numerator > 0 {
            quotient + 1
        } else {
            quotient - 1
        };
        i64::try_from(rounded).ok().map(Self)
    }
}

/// Parse a decimal string such as `"24.31"` into minor units.
///
/// This exists because market data arrives as JSON numbers, and the obvious conversion —
/// `(value * 100.0).round()` — routes every price in the system through an `f64`. For the
/// magnitudes involved that happens to be accurate, but "happens to be accurate" is not the
/// standard a ledger that cannot be edited should be held to (D-3). Parsing the decimal text
/// directly means no float is ever constructed.
///
/// More than two decimal places are rounded half away from zero, which matches how the
/// half-spread rounds: a fraction of a cent is never rounded out of existence.
pub fn minor_from_decimal_str(s: &str) -> Option<i64> {
    let s = s.trim();
    let (negative, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    if digits.is_empty() {
        return None;
    }

    let (whole, frac) = match digits.split_once('.') {
        Some((w, f)) => (w, f),
        None => (digits, ""),
    };
    // "24." and ".31" are both accepted; "" is not, and neither is "1.2.3".
    if whole.is_empty() && frac.is_empty() {
        return None;
    }
    if !whole.bytes().all(|b| b.is_ascii_digit()) || !frac.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    let mut minor: i64 = if whole.is_empty() {
        0
    } else {
        whole.parse::<i64>().ok()?.checked_mul(SCALE)?
    };

    let mut frac_digits = frac.bytes().map(|b| i64::from(b - b'0'));
    let tenths = frac_digits.next().unwrap_or(0);
    let hundredths = frac_digits.next().unwrap_or(0);
    minor = minor.checked_add(tenths * 10 + hundredths)?;

    // Round on the third decimal place, away from zero.
    if frac_digits.next().is_some_and(|d| d >= 5) {
        minor = minor.checked_add(1)?;
    }

    if negative {
        minor = minor.checked_neg()?;
    }
    Some(minor)
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Via i128 because (i64::MIN).abs() overflows.
        let abs = (self.0 as i128).abs();
        let sign = if self.0 < 0 { "-" } else { "" };
        write!(
            f,
            "{sign}{}.{:02}",
            abs / SCALE as i128,
            abs % SCALE as i128
        )
    }
}

impl fmt::Debug for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Money({self})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minor_units_round_trip() {
        assert_eq!(Money::from_minor(1234).minor(), 1234);
        assert_eq!(Money::ZERO.minor(), 0);
    }

    #[test]
    fn display_puts_the_point_in_the_right_place() {
        assert_eq!(Money::from_minor(1234).to_string(), "12.34");
        assert_eq!(Money::from_minor(5).to_string(), "0.05");
        assert_eq!(Money::from_minor(100).to_string(), "1.00");
        assert_eq!(Money::from_minor(0).to_string(), "0.00");
        assert_eq!(Money::from_minor(-1234).to_string(), "-12.34");
        assert_eq!(Money::from_minor(-5).to_string(), "-0.05");
    }

    #[test]
    fn display_survives_the_extremes() {
        // (i64::MIN).abs() panics, so this would be a crash if Display used i64 arithmetic.
        assert_eq!(
            Money::from_minor(i64::MIN).to_string(),
            "-92233720368547758.08"
        );
        assert_eq!(
            Money::from_minor(i64::MAX).to_string(),
            "92233720368547758.07"
        );
    }

    #[test]
    fn arithmetic_refuses_to_wrap() {
        let max = Money::from_minor(i64::MAX);
        assert_eq!(max.checked_add(Money::from_minor(1)), None);
        assert_eq!(
            Money::from_minor(i64::MIN).checked_sub(Money::from_minor(1)),
            None
        );
        assert_eq!(max.checked_mul(2), None);
        assert_eq!(
            Money::from_minor(250).checked_mul(4),
            Some(Money::from_minor(1000))
        );
    }

    #[test]
    fn a_half_spread_always_rounds_against_the_portfolio() {
        // 5 bps of 100.00 is exactly 0.05, so nothing to round.
        assert_eq!(
            Money::from_minor(10_000).bps_away_from_zero(5),
            Some(Money::from_minor(5))
        );
        // 5 bps of 24.31 is 0.012155 — a fraction of a cent, which must still cost a cent
        // rather than silently costing nothing.
        assert_eq!(
            Money::from_minor(2431).bps_away_from_zero(5),
            Some(Money::from_minor(2))
        );
        // The smallest non-zero case: anything above zero costs at least one minor unit.
        assert_eq!(
            Money::from_minor(1).bps_away_from_zero(1),
            Some(Money::from_minor(1))
        );
        // Zero stays zero; there is nothing to round.
        assert_eq!(Money::ZERO.bps_away_from_zero(5), Some(Money::ZERO));
        // Negative amounts round away from zero too, so the magnitude still grows.
        assert_eq!(
            Money::from_minor(-2431).bps_away_from_zero(5),
            Some(Money::from_minor(-2))
        );
    }

    #[test]
    fn a_spread_never_rounds_a_cost_down_to_nothing() {
        // The property the previous test spot-checks: for any positive amount and any
        // non-zero rate, the cost is at least one minor unit.
        for minor in [1i64, 7, 99, 100, 2431, 999_999] {
            for bps in [1u32, 5, 25] {
                let cost = Money::from_minor(minor).bps_away_from_zero(bps).unwrap();
                assert!(
                    cost.minor() >= 1,
                    "{minor} at {bps}bps rounded down to {cost}"
                );
            }
        }
    }

    #[test]
    fn decimal_text_becomes_minor_units_without_a_float() {
        for (text, want) in [
            ("24.31", 2431),
            ("24.3", 2430),
            ("24", 2400),
            ("0.05", 5),
            ("0", 0),
            ("0.00", 0),
            ("24.", 2400),
            (".31", 31),
            ("-24.31", -2431),
            ("+24.31", 2431),
            (" 24.31 ", 2431),
            ("1234567.89", 123456789),
        ] {
            assert_eq!(minor_from_decimal_str(text), Some(want), "{text:?}");
        }
    }

    #[test]
    fn a_third_decimal_place_rounds_away_from_zero() {
        // Sub-cent prices are real. Rounding them down would quietly lose value on every
        // bar; rounding away from zero is the same direction the half-spread takes.
        assert_eq!(minor_from_decimal_str("24.315"), Some(2432));
        assert_eq!(minor_from_decimal_str("24.314"), Some(2431));
        assert_eq!(minor_from_decimal_str("24.319"), Some(2432));
        assert_eq!(minor_from_decimal_str("-24.315"), Some(-2432));
        assert_eq!(minor_from_decimal_str("0.005"), Some(1));
        assert_eq!(minor_from_decimal_str("0.004"), Some(0));
    }

    #[test]
    fn the_float_route_is_wrong_on_real_prices() {
        // Not every price breaks: 24.31 * 100.0 is exactly 2431.0. The ones that break are
        // the reason this parser exists, and they are ordinary prices, not contrived ones.

        // 1.15 * 100.0 is 114.99999999999999. Truncating loses a cent.
        assert_eq!((1.15_f64 * 100.0) as i64, 114);
        assert_eq!(minor_from_decimal_str("1.15"), Some(115));

        // 8.165 * 100.0 is 816.4999999999999, so even rounding gives 816 — while decimal
        // rounding half away from zero gives 817. Adding .round() does not save the float
        // route, it just moves which prices are wrong.
        assert_eq!((8.165_f64 * 100.0).round() as i64, 816);
        assert_eq!(minor_from_decimal_str("8.165"), Some(817));
    }

    #[test]
    fn nonsense_decimals_are_refused_rather_than_guessed() {
        for text in [
            "", " ", "-", "+", ".", "abc", "1.2.3", "1,5", "24.3a", "1e5", "NaN",
        ] {
            assert_eq!(minor_from_decimal_str(text), None, "{text:?}");
        }
    }

    #[test]
    fn ordering_works_on_the_underlying_integer() {
        let mut v = [
            Money::from_minor(10),
            Money::from_minor(-5),
            Money::from_minor(0),
        ];
        v.sort();
        assert_eq!(v[0], Money::from_minor(-5));
        assert_eq!(v[2], Money::from_minor(10));
    }
}
