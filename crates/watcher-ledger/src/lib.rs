//! The paper trading rules as a pure state machine: decide() and apply().
//!
//! Must not depend on: sqlx, tokio, or a clock. Time arrives as an argument, which is what lets these tests run with no database (N-56).
