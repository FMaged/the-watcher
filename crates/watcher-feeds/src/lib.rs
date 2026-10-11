//! Price and news provider implementations behind the traits in watcher-core.
//!
//! Must not depend on: analyses, orders or pages. Swapping a provider must touch nothing else
//! (N-54), and the dependency graph is what makes that checkable rather than promised.

pub mod source;
pub mod tiingo;
pub mod yahoo;

pub use source::{ConfigureError, ConfiguredPriceSource};
pub use tiingo::TiingoPriceSource;
pub use yahoo::YahooPriceSource;
