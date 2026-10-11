//! Postgres access: migrations, repositories, queries.
//!
//! Must not depend on: the price, news and model providers. It talks to Postgres only.
//!
//! Two connections, never one (D7). Migrations run as the owner role, which may create and
//! alter tables; everything else runs as `watcher_app`, which holds no UPDATE or DELETE on the
//! append-only tables. That is what makes N-1 a property of the database rather than a promise
//! about our code.

pub mod watchlist;

use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};

/// The connection pool, under a name that is not sqlx's.
///
/// `watcher-api` holds one of these and hands it to the functions in this crate. Re-exporting
/// the type means sqlx stays a dependency of this crate alone, which is the shape the
/// architecture claims: "it talks to Postgres only" is checkable in `Cargo.toml` rather than
/// by reading handlers.
pub type Db = PgPool;

/// The single v1 user, seeded by migration 0002.
///
/// There is no login yet (`PLAN.md` D14), but every record is attributed to a user from the
/// start (D-1) and every query scopes by one (N-43). Naming the id here rather than defaulting
/// it inside each query is what keeps that scoping visible: a query with no `user_id`
/// parameter is a bug you can see.
pub const OWNER_USER_ID: &str = "us_00000000000000000000000001";

/// Connect as the application role.
pub async fn connect(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        // One user, a nightly job and a handful of page loads. The pool exists to bound the
        // database's side of a mistake, not to serve traffic.
        .max_connections(8)
        // Fail a request rather than queue behind an unreachable database until the client
        // gives up — F-102's health endpoint is how "the database is down" should be found
        // out, not a page that hangs.
        .acquire_timeout(Duration::from_secs(5))
        .connect(url)
        .await
}

/// Apply every migration that has not run, as the owner role.
///
/// Called at startup, which is the only moment the owner credential is used. The alternative —
/// applying them by hand before a deploy — is a step that gets skipped exactly once, and the
/// symptom is a binary querying a table that does not exist.
pub async fn migrate(url: &str) -> Result<(), MigrateError> {
    // A single connection rather than a pool: this runs once, and a migration holding locks
    // on several connections is how a half-applied schema happens.
    let mut connection = <sqlx::PgConnection as sqlx::Connection>::connect(url)
        .await
        .map_err(MigrateError::Connect)?;

    sqlx::migrate!("../../migrations")
        .run(&mut connection)
        .await
        .map_err(MigrateError::Apply)?;

    Ok(())
}

/// Why migrating failed.
///
/// Separate from `sqlx::Error` so the startup line can say which half went wrong: an owner
/// credential that cannot connect and a migration that will not apply are different problems
/// with different fixes. Neither variant carries the URL, which holds a password (N-40).
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    #[error("could not connect as the owner role: {0}")]
    Connect(#[source] sqlx::Error),

    #[error("a migration failed to apply: {0}")]
    Apply(#[source] sqlx::migrate::MigrateError),
}
