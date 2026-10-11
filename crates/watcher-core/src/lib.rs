//! Domain types, money in minor units, trading days, the annotation schema, and the provider
//! traits.
//!
//! Must not depend on: anything that does I/O — sqlx, reqwest, tokio. It holds types and
//! traits only.
//!
//! Everything here is a newtype over a primitive rather than the primitive itself. That is
//! not ceremony: `place_order(price, quantity)` taking two integers is a function whose
//! arguments can be swapped and still compile, and the mistake would be written to an
//! append-only ledger that cannot be edited afterwards.

pub mod money;
pub mod price_source;
pub mod shares;
pub mod side;
pub mod ticker;
pub mod trading_day;

pub use money::{minor_from_decimal_str, Money, SCALE};
pub use price_source::{DailyBar, FetchError, Fetched, PriceSource};
pub use shares::Shares;
pub use side::{InvalidSide, Side};
pub use ticker::{InvalidTicker, Ticker};
pub use trading_day::{InvalidTradingDay, TradingDay};
