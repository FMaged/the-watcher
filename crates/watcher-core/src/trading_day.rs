//! A calendar date that identifies one trading day.
//!
//! This type knows nothing about whether a given date was a session. That question is
//! answered by the `trading_calendar` table, which is seeded by migration (D-15) rather than
//! computed, because a holiday rule that changes under a test is worse than a few hundred
//! rows. `TradingDay` is the identifier; the calendar is the authority.
//!
//! Dates are also never taken from the agent. The server derives `trading_day` at write time
//! from its own clock (D-11), which is what makes backdating impossible rather than merely
//! discouraged.

use std::fmt;
use std::str::FromStr;
use time::Date;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TradingDay(Date);

impl TradingDay {
    pub fn from_ymd(year: i32, month: u8, day: u8) -> Option<Self> {
        let month = time::Month::try_from(month).ok()?;
        Date::from_calendar_date(year, month, day).ok().map(Self)
    }

    pub const fn from_date(date: Date) -> Self {
        Self(date)
    }

    pub const fn date(self) -> Date {
        self.0
    }

    pub fn ymd(self) -> (i32, u8, u8) {
        let (y, m, d) = self.0.to_calendar_date();
        (y, m as u8, d)
    }

    /// Midnight UTC on this day, as Unix seconds.
    ///
    /// Providers take and return Unix timestamps. The conversion lives here rather than in
    /// `watcher-feeds` so that no provider implementation needs a date library of its own —
    /// which is also what keeps the dependency graph one-way (N-54).
    pub fn to_unix_midnight(self) -> i64 {
        i64::from(self.0.to_julian_day() - UNIX_EPOCH_JULIAN) * SECONDS_PER_DAY
    }

    /// The UTC calendar day a Unix timestamp falls on.
    pub fn from_unix_seconds(seconds: i64) -> Option<Self> {
        let days = seconds.div_euclid(SECONDS_PER_DAY);
        let julian = i32::try_from(days).ok()?.checked_add(UNIX_EPOCH_JULIAN)?;
        Date::from_julian_day(julian).ok().map(Self)
    }
}

const UNIX_EPOCH_JULIAN: i32 = 2_440_588;
const SECONDS_PER_DAY: i64 = 86_400;

impl fmt::Display for TradingDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (y, m, d) = self.ymd();
        write!(f, "{y:04}-{m:02}-{d:02}")
    }
}

impl fmt::Debug for TradingDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TradingDay({self})")
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct InvalidTradingDay;

impl fmt::Display for InvalidTradingDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a date in YYYY-MM-DD form")
    }
}

impl std::error::Error for InvalidTradingDay {}

impl FromStr for TradingDay {
    type Err = InvalidTradingDay;

    /// Parsed by hand rather than with `time`'s parser, which keeps the crate's feature set
    /// to the minimum. ISO 8601 calendar dates are the only form accepted.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let b = s.as_bytes();
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return Err(InvalidTradingDay);
        }
        // Every non-separator byte must be a digit, checked before parsing. `parse::<i32>()`
        // accepts a leading sign, which let "2026-+1-09" through as 2026-01-09 and
        // "-026-10-09" through as year -26. The earlier test for nonsense passed anyway,
        // which is the part worth remembering: the hole was in what the test did not try.
        let digits_only = |range: std::ops::Range<usize>| -> Result<i32, InvalidTradingDay> {
            let part = s.get(range).ok_or(InvalidTradingDay)?;
            if !part.bytes().all(|b| b.is_ascii_digit()) {
                return Err(InvalidTradingDay);
            }
            part.parse::<i32>().map_err(|_| InvalidTradingDay)
        };
        let num = digits_only;
        let year = num(0..4)?;
        let month = u8::try_from(num(5..7)?).map_err(|_| InvalidTradingDay)?;
        let day = u8::try_from(num(8..10)?).map_err(|_| InvalidTradingDay)?;
        Self::from_ymd(year, month, day).ok_or(InvalidTradingDay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_valid_date_round_trips_through_text() {
        let d: TradingDay = "2026-10-09".parse().unwrap();
        assert_eq!(d.to_string(), "2026-10-09");
        assert_eq!(d.ymd(), (2026, 10, 9));
    }

    #[test]
    fn single_digit_months_and_days_keep_their_padding() {
        let d = TradingDay::from_ymd(2026, 1, 5).unwrap();
        assert_eq!(d.to_string(), "2026-01-05");
        assert_eq!(d.to_string().parse::<TradingDay>().unwrap(), d);
    }

    #[test]
    fn leap_years_are_the_calendar_crate_s_problem_and_it_gets_them_right() {
        assert!(TradingDay::from_ymd(2024, 2, 29).is_some());
        assert!(TradingDay::from_ymd(2026, 2, 29).is_none());
    }

    #[test]
    fn nonsense_is_refused_rather_than_coerced() {
        for s in [
            "",
            "2026-10-9",  // unpadded
            "2026/10/09", // wrong separator
            "2026-13-01", // month 13
            "2026-00-01", // month 0
            "2026-10-32", // day 32
            "2026-10-09T00:00:00Z",
            "not a date",
            "26-10-09",
            // Signed components. `parse::<i32>()` accepts these, so they have to be
            // excluded before it ever runs.
            "+026-10-09",
            "-026-10-09",
            "2026-+1-09",
            "2026-10-+9",
            "2026--1-09",
        ] {
            assert_eq!(
                s.parse::<TradingDay>(),
                Err(InvalidTradingDay),
                "{s:?} should not parse"
            );
        }
    }

    #[test]
    fn unix_timestamps_round_trip_through_days() {
        let d = TradingDay::from_ymd(2026, 10, 9).unwrap();
        assert_eq!(TradingDay::from_unix_seconds(d.to_unix_midnight()), Some(d));
        // The epoch itself, and a timestamp partway through a day.
        assert_eq!(
            TradingDay::from_unix_seconds(0).unwrap().to_string(),
            "1970-01-01"
        );
        assert_eq!(
            TradingDay::from_unix_seconds(d.to_unix_midnight() + 13 * 3600)
                .unwrap()
                .to_string(),
            "2026-10-09"
        );
    }

    #[test]
    fn timestamps_before_the_epoch_still_land_on_the_right_day() {
        // div_euclid rather than integer division: -1 second is the day *before* the epoch,
        // not the epoch itself, and truncating division would get that wrong.
        assert_eq!(
            TradingDay::from_unix_seconds(-1).unwrap().to_string(),
            "1969-12-31"
        );
    }

    #[test]
    fn days_order_chronologically() {
        let mut v = [
            TradingDay::from_ymd(2026, 10, 9).unwrap(),
            TradingDay::from_ymd(2025, 1, 1).unwrap(),
            TradingDay::from_ymd(2026, 2, 1).unwrap(),
        ];
        v.sort();
        assert_eq!(v[0].to_string(), "2025-01-01");
        assert_eq!(v[2].to_string(), "2026-10-09");
    }
}
